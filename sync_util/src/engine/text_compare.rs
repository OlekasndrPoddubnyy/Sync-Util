//! Confronto di file di testo riga per riga, senza distinguere i fine riga (CRLF, LF, CR).
//!
//! Il primo file è letto in UTF-8 e il secondo in windows-1252; se un file non è valido nella sua codifica
//! si prova l'altra, e se non lo è in nessuna delle due i file vengono considerati uguali (quindi non copiati),
//! come nella versione Java. I file sono letti a flusso: anche file molto grandi non vengono caricati in memoria.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use encoding_rs::WINDOWS_1252;

const BUFFER: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charset {
    Utf8,
    Windows1252,
}

/// `true` se i file hanno le stesse righe; `true` anche se uno dei due non è leggibile in nessuna codifica.
pub fn text_files_equal(source: &Path, target: &Path) -> io::Result<bool> {
    let Some(cs1) = readable_charset(source, Charset::Utf8, Charset::Windows1252)? else {
        return Ok(true);
    };
    let Some(cs2) = readable_charset(target, Charset::Windows1252, Charset::Utf8)? else {
        return Ok(true);
    };
    let mut lines1 = LineReader::new(File::open(source)?);
    let mut lines2 = LineReader::new(File::open(target)?);
    loop {
        let l1 = lines1.next_line()?;
        let l2 = lines2.next_line()?;
        match (l1, l2) {
            (None, None) => return Ok(true),
            (Some(a), Some(b)) => {
                if !lines_equal(&a, cs1, &b, cs2) {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
}

fn lines_equal(a: &[u8], cs1: Charset, b: &[u8], cs2: Charset) -> bool {
    if cs1 == cs2 || (a.is_ascii() && b.is_ascii()) {
        return a == b;
    }
    decode(a, cs1) == decode(b, cs2)
}

fn decode(bytes: &[u8], charset: Charset) -> std::borrow::Cow<'_, str> {
    match charset {
        // già verificato valido
        Charset::Utf8 => String::from_utf8_lossy(bytes),
        Charset::Windows1252 => WINDOWS_1252.decode_without_bom_handling(bytes).0,
    }
}

/// La prima codifica, tra le due indicate, in cui il file si legge senza errori; `None` se nessuna.
fn readable_charset(path: &Path, preferred: Charset, fallback: Charset) -> io::Result<Option<Charset>> {
    if can_decode(path, preferred)? {
        return Ok(Some(preferred));
    }
    if can_decode(path, fallback)? {
        return Ok(Some(fallback));
    }
    Ok(None)
}

fn can_decode(path: &Path, charset: Charset) -> io::Result<bool> {
    let mut file = File::open(path)?;
    match charset {
        Charset::Utf8 => is_valid_utf8(&mut file),
        Charset::Windows1252 => is_valid_windows_1252(&mut file),
    }
}

/// Validazione UTF-8 a blocchi: una sequenza spezzata tra due blocchi viene riportata in testa al successivo.
fn is_valid_utf8(reader: &mut impl Read) -> io::Result<bool> {
    let mut buf = vec![0u8; BUFFER];
    let mut carry = 0usize;
    loop {
        let n = reader.read(&mut buf[carry..])?;
        if n == 0 {
            return Ok(carry == 0);
        }
        let len = carry + n;
        match std::str::from_utf8(&buf[..len]) {
            Ok(_) => carry = 0,
            Err(e) if e.error_len().is_none() => {
                let valid = e.valid_up_to();
                buf.copy_within(valid..len, 0);
                carry = len - valid;
            }
            Err(_) => return Ok(false),
        }
    }
}

/// Il decoder windows-1252 di Java rifiuta i cinque byte senza corrispondenza: 0x81, 0x8D, 0x8F, 0x90, 0x9D.
fn is_valid_windows_1252(reader: &mut impl Read) -> io::Result<bool> {
    let mut buf = vec![0u8; BUFFER];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            return Ok(true);
        }
        if buf[..n].iter().any(|b| matches!(b, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D)) {
            return Ok(false);
        }
    }
}

/// Righe come `BufferedReader.readLine` di Java: terminate da \n, \r o \r\n, senza il terminatore.
struct LineReader<R: Read> {
    reader: BufReader<R>,
    pending: VecDeque<Vec<u8>>,
    chunk: Vec<u8>,
}

impl<R: Read> LineReader<R> {
    fn new(inner: R) -> Self {
        LineReader { reader: BufReader::with_capacity(BUFFER, inner), pending: VecDeque::new(), chunk: Vec::new() }
    }

    fn next_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        if let Some(line) = self.pending.pop_front() {
            return Ok(Some(line));
        }
        self.chunk.clear();
        if self.reader.read_until(b'\n', &mut self.chunk)? == 0 {
            return Ok(None);
        }
        let mut data: &[u8] = &self.chunk;
        if let Some(rest) = data.strip_suffix(b"\n") {
            data = rest;
        }
        if let Some(rest) = data.strip_suffix(b"\r") {
            data = rest;
        }
        // eventuali \r isolati all'interno separano altre righe
        let mut parts = data.split(|&b| b == b'\r');
        let first = parts.next().unwrap_or_default().to_vec();
        self.pending.extend(parts.map(<[u8]>::to_vec));
        Ok(Some(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn lines(data: &[u8]) -> Vec<Vec<u8>> {
        let mut reader = LineReader::new(Cursor::new(data.to_vec()));
        let mut out = Vec::new();
        while let Some(line) = reader.next_line().unwrap() {
            out.push(line);
        }
        out
    }

    #[test]
    fn line_endings_like_java_read_line() {
        assert_eq!(lines(b"a\r\nb\nc\rd"), vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()]);
        assert_eq!(lines(b"a\n"), vec![b"a".to_vec()]);
        assert_eq!(lines(b"a\r"), vec![b"a".to_vec()]);
        assert_eq!(lines(b"x\r\r\n"), vec![b"x".to_vec(), b"".to_vec()]);
        assert_eq!(lines(b"\n\n"), vec![b"".to_vec(), b"".to_vec()]);
        assert!(lines(b"").is_empty());
    }

    #[test]
    fn utf8_validation_across_chunk_boundaries() {
        let mut data = vec![b'a'; BUFFER - 1];
        data.extend_from_slice("è".as_bytes()); // spezzato a cavallo del blocco
        assert!(is_valid_utf8(&mut Cursor::new(data.clone())).unwrap());
        data.push(0xC3); // sequenza troncata alla fine
        assert!(!is_valid_utf8(&mut Cursor::new(data)).unwrap());
        assert!(!is_valid_utf8(&mut Cursor::new(vec![0xE8, b'a'])).unwrap());
    }

    #[test]
    fn windows_1252_rejects_unmapped_bytes() {
        assert!(is_valid_windows_1252(&mut Cursor::new(vec![0xE8, 0x80])).unwrap());
        assert!(!is_valid_windows_1252(&mut Cursor::new(vec![0x81])).unwrap());
    }

    #[test]
    fn crlf_and_encoding_differences_are_equal() {
        let dir = std::env::temp_dir().join(format!("sync_util_text_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("a.txt");
        let dst = dir.join("b.txt");
        std::fs::write(&src, "perché\nciao\n").unwrap(); // UTF-8, LF
        std::fs::write(&dst, b"perch\xE9\r\nciao\r\n").unwrap(); // windows-1252, CRLF
        assert!(text_files_equal(&src, &dst).unwrap());
        std::fs::write(&dst, b"perch\xE9\r\nciao!\r\n").unwrap();
        assert!(!text_files_equal(&src, &dst).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
