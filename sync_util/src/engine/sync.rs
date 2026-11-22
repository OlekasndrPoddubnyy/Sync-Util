//! Allineamento della cartella di destinazione al contenuto della sorgente.
//!
//! Stesso comportamento della versione Java, con queste differenze di prestazioni:
//! - le cartelle e i file vengono elaborati in parallelo (rayon);
//! - il tipo e la dimensione dei file arrivano dall'elenco della cartella, senza una chiamata per file;
//! - due file di dimensione diversa sono diversi senza leggerli; il confronto byte per byte si ferma alla prima differenza;
//! - le cartelle rimaste vuote vengono eliminate durante la stessa visita, non con una seconda scansione.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;

use super::ignore_rules::IgnoreRules;
use super::log::{Log, LogKind};
use super::text_compare::text_files_equal;

const DM_FOLDER_NAME: &str = ".dm";
const COMPARE_BUFFER: usize = 256 * 1024;

/// Estensioni di file generalmente testuali: per questi file si ignorano fine riga e codifica.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "log", "md", "csv", "tsv", "json", "xml", "html", "htm", "java", "py", "js", "css", "yaml", "yml", "ini",
    "cfg", "properties", "rtf", "sh", "bat", "ps1", "sql", "tex", "pom", "jsp", "inc", "xsd", "tld", "xmi",
];

/// Esito di un elemento durante la sincronizzazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Copied,
    Deleted,
    Identical,
    Denied,
    Excluded,
}

impl Outcome {
    pub const ALL: [Outcome; 5] = [Outcome::Copied, Outcome::Deleted, Outcome::Identical, Outcome::Denied, Outcome::Excluded];

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Statistiche e percorsi elaborati, letti dalla GUI mentre la sincronizzazione è in corso.
pub struct SyncState {
    copied: AtomicUsize,
    deleted: AtomicUsize,
    identical: AtomicUsize,
    denied: Mutex<Vec<(PathBuf, String)>>,
    // percorsi relativi per esito, finché la GUI non li preleva; le cartelle intere terminano con "/"
    processed: [Mutex<Vec<String>>; 5],
    log: Arc<Log>,
}

impl SyncState {
    pub fn new(log: Arc<Log>) -> Self {
        SyncState {
            copied: AtomicUsize::new(0),
            deleted: AtomicUsize::new(0),
            identical: AtomicUsize::new(0),
            denied: Mutex::new(Vec::new()),
            processed: Default::default(),
            log,
        }
    }

    pub fn copied(&self) -> usize {
        self.copied.load(Ordering::Relaxed)
    }

    pub fn deleted(&self) -> usize {
        self.deleted.load(Ordering::Relaxed)
    }

    pub fn identical(&self) -> usize {
        self.identical.load(Ordering::Relaxed)
    }

    pub fn denied_count(&self) -> usize {
        lock(&self.denied).len()
    }

    /// File con accesso negato: (percorso completo nella sorgente, nome).
    pub fn denied_files(&self) -> Vec<(PathBuf, String)> {
        lock(&self.denied).clone()
    }

    /// Toglie e restituisce i percorsi registrati per l'esito indicato dall'ultima chiamata.
    pub fn take_processed(&self, outcome: Outcome) -> Vec<String> {
        std::mem::take(&mut *lock(&self.processed[outcome.index()]))
    }

    fn track(&self, outcome: Outcome, relative_path: &str, directory: bool) {
        let path = if directory { format!("{relative_path}/") } else { relative_path.to_string() };
        lock(&self.processed[outcome.index()]).push(path);
    }

    fn log(&self, kind: LogKind, text: impl Into<String>) {
        self.log.push(kind, text);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Verifica che sorgente e destinazione siano cartelle distinte e non contenute l'una nell'altra.
pub fn folders_overlap(source: &Path, target: &Path) -> bool {
    fn components(path: &Path) -> Vec<String> {
        let resolved = fs::canonicalize(path).or_else(|_| std::path::absolute(path)).unwrap_or_else(|_| path.to_path_buf());
        resolved
            .components()
            .filter(|c| !matches!(c, std::path::Component::CurDir))
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect()
    }
    let s = components(source);
    let t = components(target);
    s.starts_with(&t) || t.starts_with(&s)
}

/// Allinea `target` a `source`. I percorsi esclusi dalle regole non vengono copiati e,
/// se presenti solo nella destinazione, non vengono eliminati.
pub fn synchronize(source: &Path, target: &Path, rules: &IgnoreRules, state: &SyncState) {
    if !source.is_dir() {
        state.log(LogKind::Warning, "La cartella sorgente non esiste o non è una directory.");
        return;
    }
    sync_dir(source, target, "", rules, state);
}

struct Entry {
    name: String,
    path: PathBuf,
    directory: bool,
    len: u64,
}

fn list_dir(dir: &Path, state: &SyncState) -> Option<Vec<Entry>> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e) => {
            state.log(LogKind::Warning, format!("Impossibile leggere la cartella {}: {e}", dir.display()));
            return None;
        }
    };
    let mut entries = Vec::new();
    for item in read {
        let Ok(item) = item else { continue };
        let path = item.path();
        // su Windows tipo e dimensione arrivano già dall'elenco della cartella; i collegamenti vanno risolti
        let metadata = match item.file_type() {
            Ok(kind) if kind.is_symlink() => fs::metadata(&path),
            _ => item.metadata(),
        };
        let (directory, len) = metadata.map(|m| (m.is_dir(), m.len())).unwrap_or((false, 0));
        entries.push(Entry { name: item.file_name().to_string_lossy().into_owned(), path, directory, len });
    }
    Some(entries)
}

fn child_path(relative_dir: &str, name: &str) -> String {
    if relative_dir.is_empty() { name.to_string() } else { format!("{relative_dir}/{name}") }
}

fn sync_dir(source: &Path, target: &Path, relative_dir: &str, rules: &IgnoreRules, state: &SyncState) {
    if !target.exists()
        && let Err(e) = fs::create_dir_all(target) {
            state.log(LogKind::Error, format!("Impossibile creare la cartella {}: {e}", target.display()));
        }

    let sources = list_dir(source, state);
    let targets = if target.is_dir() { list_dir(target, state) } else { None };

    // elimina dalla destinazione quanto non esiste più nella sorgente
    if let (Some(sources), Some(targets)) = (&sources, &targets) {
        let names: HashSet<&str> = sources.iter().map(|e| e.name.as_str()).collect();
        targets
            .par_iter()
            .filter(|t| t.name != DM_FOLDER_NAME && !names.contains(t.name.as_str()))
            .for_each(|t| remove_extra(t, relative_dir, rules, state));
    }

    let Some(sources) = sources else { return };
    sources.par_iter().for_each(|entry| {
        let relative = child_path(relative_dir, &entry.name);
        if rules.is_ignored(&relative, entry.directory) {
            state.log(LogKind::Detail, format!("Escluso: {relative}"));
            state.track(Outcome::Excluded, &relative, entry.directory);
            return;
        }
        let target_path = target.join(&entry.name);
        if entry.directory {
            sync_dir(&entry.path, &target_path, &relative, rules, state);
            remove_if_empty(&target_path, &entry.name, &relative, state);
        } else {
            sync_file(entry, &target_path, &relative, state);
        }
    });
}

fn remove_extra(entry: &Entry, relative_dir: &str, rules: &IgnoreRules, state: &SyncState) {
    let relative = child_path(relative_dir, &entry.name);
    if rules.is_ignored(&relative, entry.directory) {
        state.log(LogKind::Detail, format!("Non eliminato (escluso): {relative}"));
        state.track(Outcome::Excluded, &relative, entry.directory);
        return;
    }
    let result = if entry.directory { fs::remove_dir_all(&entry.path) } else { fs::remove_file(&entry.path) };
    match result {
        Ok(()) => {
            state.deleted.fetch_add(1, Ordering::Relaxed);
            state.track(Outcome::Deleted, &relative, entry.directory);
            state.log(LogKind::Detail, format!("Eliminato (non presente nella sorgente): {relative}"));
        }
        Err(e) => state.log(LogKind::Warning, format!("Impossibile eliminare {relative}: {e}")),
    }
}

/// Elimina la cartella di destinazione se è vuota o contiene solo la cartella .dm.
fn remove_if_empty(dir: &Path, name: &str, relative: &str, state: &SyncState) {
    if name == DM_FOLDER_NAME {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut only_dm = false;
    for (count, item) in read.enumerate() {
        let Ok(item) = item else { return };
        let is_dm = item.file_name() == DM_FOLDER_NAME && item.file_type().is_ok_and(|t| t.is_dir());
        if count > 0 || !is_dm {
            return;
        }
        only_dm = true;
    }
    let result = if only_dm { fs::remove_dir_all(dir) } else { fs::remove_dir(dir) };
    match result {
        Ok(()) => state.log(LogKind::Detail, format!("Cartella vuota eliminata: {relative}")),
        Err(e) => state.log(LogKind::Warning, format!("Impossibile eliminare la cartella vuota {relative}: {e}")),
    }
}

fn sync_file(entry: &Entry, target: &Path, relative: &str, state: &SyncState) {
    let must_copy = || -> io::Result<bool> {
        let metadata = match fs::metadata(target) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(e) => return Err(e),
        };
        if metadata.is_dir() {
            return Err(io::Error::other("nella destinazione esiste una cartella con lo stesso nome"));
        }
        if files_identical(&entry.path, target, entry.len, metadata.len())? {
            return Ok(false);
        }
        if !is_text_extension(&entry.name) {
            return Ok(true);
        }
        match text_files_equal(&entry.path, target) {
            Ok(equal) => Ok(!equal),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(e),
            Err(e) => {
                state.log(LogKind::Error, format!("Errore durante la lettura dei file {relative}: {e}"));
                Ok(false)
            }
        }
    };

    let result = must_copy().and_then(|copy| {
        if copy {
            fs::copy(&entry.path, target)?;
        }
        Ok(copy)
    });
    match result {
        Ok(true) => {
            state.copied.fetch_add(1, Ordering::Relaxed);
            state.track(Outcome::Copied, relative, false);
            state.log(LogKind::Detail, format!("File copiato: {relative}"));
        }
        Ok(false) => {
            state.identical.fetch_add(1, Ordering::Relaxed);
            state.track(Outcome::Identical, relative, false);
            state.log(LogKind::Detail, format!("File non copiato (identico): {relative}"));
        }
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            lock(&state.denied).push((entry.path.clone(), entry.name.clone()));
            state.track(Outcome::Denied, relative, false);
            state.log(LogKind::Warning, format!("Accesso negato: {relative}"));
        }
        Err(e) => state.log(LogKind::Error, format!("Errore durante la copia del file {relative}: {e}")),
    }
}

fn is_text_extension(name: &str) -> bool {
    match name.rfind('.') {
        Some(i) if i > 0 && i < name.len() - 1 => {
            let extension = name[i + 1..].to_ascii_lowercase();
            TEXT_EXTENSIONS.contains(&extension.as_str())
        }
        _ => false,
    }
}

/// Confronto byte per byte; file di dimensione diversa sono diversi senza leggerli.
fn files_identical(a: &Path, b: &Path, len_a: u64, len_b: u64) -> io::Result<bool> {
    if len_a != len_b {
        return Ok(false);
    }
    let mut fa = File::open(a)?;
    let mut fb = File::open(b)?;
    let mut buf_a = vec![0u8; COMPARE_BUFFER];
    let mut buf_b = vec![0u8; COMPARE_BUFFER];
    loop {
        let n = read_full(&mut fa, &mut buf_a)?;
        let m = read_full(&mut fb, &mut buf_b)?;
        if n != m || buf_a[..n] != buf_b[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn read_full(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("sync_util_{name}_{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, content: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    #[test]
    fn mirrors_source_into_target() {
        let tmp = TempDir::new("mirror");
        let src = tmp.0.join("src");
        let dst = tmp.0.join("dst");
        write(&src.join("a.bin"), b"new");
        write(&src.join("same.bin"), b"same");
        write(&src.join("crlf.txt"), b"uno\ndue\n");
        write(&src.join("sub/deep/b.java"), b"class B {}");
        write(&src.join("empty/.keep_out"), b"x");
        write(&src.join("target/out.class"), b"x");

        write(&dst.join("a.bin"), b"old");
        write(&dst.join("same.bin"), b"same");
        write(&dst.join("crlf.txt"), b"uno\r\ndue\r\n");
        write(&dst.join("gone.txt"), b"bye");
        write(&dst.join("gone_dir/x.txt"), b"bye");
        write(&dst.join(".dm/meta"), b"keep");
        write(&dst.join("onlydm/.dm/meta"), b"x");
        write(&dst.join("node_modules/lib.js"), b"keep");

        let rules = IgnoreRules::parse("target\nnode_modules\n.keep_out");
        let state = SyncState::new(Arc::new(Log::default()));
        // "onlydm" non esiste nella sorgente: viene eliminata come elemento in più
        synchronize(&src, &dst, &rules, &state);

        assert_eq!(fs::read(dst.join("a.bin")).unwrap(), b"new");
        assert_eq!(fs::read(dst.join("crlf.txt")).unwrap(), b"uno\r\ndue\r\n", "solo i fine riga cambiano: non si copia");
        assert_eq!(fs::read(dst.join("sub/deep/b.java")).unwrap(), b"class B {}");
        assert!(!dst.join("gone.txt").exists());
        assert!(!dst.join("gone_dir").exists());
        assert!(!dst.join("onlydm").exists());
        assert!(dst.join(".dm/meta").exists(), ".dm nella radice non si tocca");
        assert!(dst.join("node_modules/lib.js").exists(), "gli esclusi non si eliminano");
        assert!(!dst.join("target").exists(), "gli esclusi non si copiano");
        assert!(!dst.join("empty").exists(), "le cartelle rimaste vuote vengono eliminate");

        assert_eq!(state.copied(), 2);
        assert_eq!(state.identical(), 2);
        assert_eq!(state.deleted(), 3);
        assert_eq!(sorted(state.take_processed(Outcome::Copied)), vec!["a.bin", "sub/deep/b.java"]);
        assert_eq!(sorted(state.take_processed(Outcome::Deleted)), vec!["gone.txt", "gone_dir/", "onlydm/"]);
        assert_eq!(sorted(state.take_processed(Outcome::Excluded)), vec!["empty/.keep_out", "node_modules/", "target/"]);
        assert!(state.take_processed(Outcome::Copied).is_empty(), "i percorsi prelevati non restano in memoria");
    }

    #[test]
    fn empty_folder_with_only_dm_is_removed() {
        let tmp = TempDir::new("onlydm");
        let src = tmp.0.join("src");
        let dst = tmp.0.join("dst");
        fs::create_dir_all(src.join("folder")).unwrap();
        write(&dst.join("folder/.dm/meta"), b"x");
        let state = SyncState::new(Arc::new(Log::default()));
        synchronize(&src, &dst, &IgnoreRules::default(), &state);
        assert!(!dst.join("folder").exists());
        assert!(dst.exists(), "la radice della destinazione resta");
    }

    #[test]
    fn overlap_detection() {
        let tmp = TempDir::new("overlap");
        let a = tmp.0.join("a");
        let b = tmp.0.join("a/b");
        let c = tmp.0.join("c");
        fs::create_dir_all(&b).unwrap();
        fs::create_dir_all(&c).unwrap();
        assert!(folders_overlap(&a, &b));
        assert!(folders_overlap(&b, &a));
        assert!(folders_overlap(&a, &tmp.0.join("A")));
        assert!(!folders_overlap(&a, &c));
    }

    #[test]
    fn text_extension_detection() {
        assert!(is_text_extension("Main.JAVA"));
        assert!(!is_text_extension("image.png"));
        assert!(!is_text_extension(".txt"));
        assert!(!is_text_extension("noext"));
    }

    /// Misura su un albero sintetico: cargo test --release -- --ignored --nocapture large_tree
    #[test]
    #[ignore]
    fn large_tree_timing() {
        let tmp = TempDir::new("large");
        let src = tmp.0.join("src");
        let dst = tmp.0.join("dst");
        for d in 0..200 {
            for f in 0..100 {
                let content = format!("riga {d} {f}\n").repeat(50);
                write(&src.join(format!("m{}/pkg{d}/F{f}.java", d % 10)), content.as_bytes());
            }
        }
        let run = |label: &str| {
            let state = SyncState::new(Arc::new(Log::default()));
            let started = std::time::Instant::now();
            synchronize(&src, &dst, &IgnoreRules::parse(super::super::ignore_rules::DEFAULT_TEXT), &state);
            println!("{label}: {:?}  (copiati {}, invariati {})", started.elapsed(), state.copied(), state.identical());
        };
        run("prima copia (20.000 file)");
        run("seconda passata, tutto identico");
        // fine riga diversi: confronto testuale completo
        for d in 0..20 {
            for f in 0..100 {
                let path = dst.join(format!("m{}/pkg{d}/F{f}.java", d % 10));
                let crlf = fs::read_to_string(&path).unwrap().replace('\n', "\r\n");
                fs::write(path, crlf).unwrap();
            }
        }
        run("2.000 file con CRLF da confrontare come testo");
    }
}
