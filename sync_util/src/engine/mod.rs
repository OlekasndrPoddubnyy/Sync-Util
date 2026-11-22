//! Motore di sincronizzazione: regole di esclusione, confronto dei file e allineamento delle cartelle.

pub mod ignore_rules;
pub mod log;
pub mod sync;
pub mod text_compare;

pub use ignore_rules::{DEFAULT_TEXT, IgnoreRules, InvalidLine};
pub use log::{Log, LogEntry, LogKind};
pub use sync::{Outcome, SyncState, folders_overlap, synchronize};
