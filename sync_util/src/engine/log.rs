//! Registro delle attività: riceve righe da qualsiasi thread, la GUI le preleva a blocchi.

use std::sync::Mutex;

/// Tipologia di riga: ne determina il colore nella console.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Detail,
    Section,
    Success,
    Warning,
    Error,
}

impl LogKind {
    /// Indice usato dalla GUI per scegliere il colore.
    pub fn index(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub time: String,
    pub kind: LogKind,
    pub text: String,
}

#[derive(Default)]
pub struct Log {
    pending: Mutex<Vec<LogEntry>>,
}

impl Log {
    pub fn push(&self, kind: LogKind, text: impl Into<String>) {
        let entry = LogEntry { time: chrono::Local::now().format("%H:%M:%S").to_string(), kind, text: text.into() };
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).push(entry);
    }

    /// Toglie e restituisce le righe arrivate dall'ultima chiamata.
    pub fn take(&self) -> Vec<LogEntry> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn clear(&self) {
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}
