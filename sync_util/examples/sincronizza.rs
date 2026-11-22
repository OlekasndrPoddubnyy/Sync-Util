//! Sincronizzazione da riga di comando, senza GUI, con le esclusioni predefinite:
//!
//!     cargo run --release --example sincronizza -- <sorgente> <destinazione>

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use sync_util::engine::{DEFAULT_TEXT, IgnoreRules, Log, Outcome, SyncState, folders_overlap, synchronize};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, target] = args.as_slice() else {
        eprintln!("uso: sincronizza <sorgente> <destinazione>");
        std::process::exit(2);
    };
    let (source, target) = (Path::new(source), Path::new(target));
    if folders_overlap(source, target) {
        eprintln!("sorgente e destinazione si sovrappongono");
        std::process::exit(2);
    }

    let log = Arc::new(Log::default());
    let state = SyncState::new(log.clone());
    let started = Instant::now();
    synchronize(source, target, &IgnoreRules::parse(DEFAULT_TEXT), &state);
    let elapsed = started.elapsed();

    for (outcome, label) in [
        (Outcome::Copied, "Copiati"),
        (Outcome::Deleted, "Eliminati"),
        (Outcome::Identical, "Invariati"),
        (Outcome::Denied, "Accesso negato"),
        (Outcome::Excluded, "Esclusi"),
    ] {
        let mut paths = state.take_processed(outcome);
        paths.sort();
        let shown: Vec<&str> = paths.iter().filter(|p| !p.starts_with("generati/")).map(String::as_str).collect();
        let generated = paths.len() - shown.len();
        println!("{label}: {}", paths.len());
        for path in shown {
            println!("    {path}");
        }
        if generated > 0 {
            println!("    … e {generated} file in generati/");
        }
    }
    for entry in log.take().iter().filter(|e| e.text.contains("vuota") || e.text.contains("Impossibile")) {
        println!("log: {}", entry.text);
    }
    println!("Tempo: {elapsed:?}");
}
