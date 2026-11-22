//! Console di log: righe virtualizzate nella GUI e ricerca con evidenziazione delle occorrenze.

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};
use sync_util::engine::LogEntry;

use super::{AppWindow, LogRow, format};

/// Oltre questo numero di righe la console scarta le più vecchie.
const MAX_ROWS: usize = 200_000;
/// Oltre questo numero di occorrenze la ricerca si ferma.
const MAX_MATCHES: usize = 2_000;

const MARK_NONE: i32 = 0;
const MARK_MATCH: i32 = 1;
const MARK_CURRENT: i32 = 2;

#[derive(Default)]
struct Search {
    query: String, // in minuscolo
    matches: Vec<usize>,
    current: Option<usize>,
}

pub struct Console {
    rows: Rc<VecModel<LogRow>>,
    lower: Vec<String>, // testo delle righe in minuscolo, per la ricerca
    search: Search,
}

impl Console {
    pub fn new() -> Self {
        Console { rows: Rc::new(VecModel::default()), lower: Vec::new(), search: Search::default() }
    }

    pub fn model(&self) -> ModelRc<LogRow> {
        self.rows.clone().into()
    }

    /// Aggiunge le righe in coda; se la console era in fondo, resta in fondo.
    pub fn append(&mut self, ui: &AppWindow, entries: Vec<LogEntry>) {
        if entries.is_empty() {
            return;
        }
        let at_bottom = ui.invoke_log_at_bottom();
        let start = self.rows.row_count();
        let mut rows = Vec::with_capacity(entries.len());
        for (i, entry) in entries.into_iter().enumerate() {
            let lower = entry.text.to_lowercase();
            let mut mark = MARK_NONE;
            if !self.search.query.is_empty()
                && self.search.matches.len() < MAX_MATCHES
                && lower.contains(&self.search.query)
            {
                self.search.matches.push(start + i);
                mark = MARK_MATCH;
            }
            rows.push(LogRow { time: entry.time.into(), kind: entry.kind.index(), text: entry.text.into(), mark });
            self.lower.push(lower);
        }
        self.rows.extend(rows);

        if self.rows.row_count() > MAX_ROWS + MAX_ROWS / 10 {
            self.trim(ui);
        }
        self.update_info(ui);
        if at_bottom {
            ui.invoke_log_scroll_to_end();
        }
    }

    /// Scarta le righe più vecchie e ricalcola le occorrenze.
    fn trim(&mut self, ui: &AppWindow) {
        let drop = self.rows.row_count() - MAX_ROWS;
        let kept: Vec<LogRow> = self.rows.iter().skip(drop).collect();
        self.rows.set_vec(kept);
        self.lower.drain(..drop);
        let query = std::mem::take(&mut self.search.query);
        self.search = Search::default();
        self.search(ui, &query, false);
    }

    /// Evidenzia tutte le righe che contengono `query`, senza distinguere maiuscole/minuscole; "" annulla la ricerca.
    pub fn search(&mut self, ui: &AppWindow, query: &str, scroll: bool) {
        let anchor = self.search.current.map(|i| self.search.matches[i]).unwrap_or(0);
        for &row in &self.search.matches {
            self.set_mark(row, MARK_NONE);
        }
        self.search = Search { query: query.to_lowercase(), ..Default::default() };
        if !self.search.query.is_empty() {
            let query = &self.search.query;
            self.search.matches =
                self.lower.iter().enumerate().filter(|(_, t)| t.contains(query)).map(|(i, _)| i).take(MAX_MATCHES).collect();
        }
        for &row in &self.search.matches {
            self.set_mark(row, MARK_MATCH);
        }
        if !self.search.matches.is_empty() {
            let index = self.search.matches.iter().position(|&row| row >= anchor).unwrap_or(0);
            self.set_current(ui, index, scroll);
        }
        self.update_info(ui);
    }

    /// Occorrenza successiva (+1) o precedente (-1), ricominciando dall'altro capo.
    pub fn move_match(&mut self, ui: &AppWindow, step: isize) {
        let count = self.search.matches.len();
        if count == 0 {
            return;
        }
        let index = match self.search.current {
            Some(current) => (current as isize + step).rem_euclid(count as isize) as usize,
            None if step < 0 => count - 1,
            None => 0,
        };
        self.set_current(ui, index, true);
        self.update_info(ui);
    }

    pub fn clear(&mut self, ui: &AppWindow) {
        self.rows.set_vec(Vec::new());
        self.lower.clear();
        let query = std::mem::take(&mut self.search.query);
        self.search = Search { query, ..Default::default() };
        self.update_info(ui);
    }

    /// Tutto il log come testo, una riga per voce.
    pub fn text(&self) -> String {
        self.rows.iter().map(|row| format!("{}  {}", row.time, row.text)).collect::<Vec<_>>().join("\n")
    }

    fn set_current(&mut self, ui: &AppWindow, index: usize, scroll: bool) {
        if let Some(previous) = self.search.current {
            self.set_mark(self.search.matches[previous], MARK_MATCH);
        }
        self.search.current = Some(index);
        let row = self.search.matches[index];
        self.set_mark(row, MARK_CURRENT);
        if scroll {
            ui.invoke_log_scroll_to(row as i32);
        }
    }

    fn set_mark(&self, row: usize, mark: i32) {
        if let Some(mut data) = self.rows.row_data(row)
            && data.mark != mark
        {
            data.mark = mark;
            self.rows.set_row_data(row, data);
        }
    }

    fn update_info(&self, ui: &AppWindow) {
        let info = if self.search.query.is_empty() {
            String::new()
        } else if self.search.matches.is_empty() {
            "Nessun risultato".to_string()
        } else {
            let more = if self.search.matches.len() >= MAX_MATCHES { "+" } else { "" };
            let current = self.search.current.map_or(0, |i| i + 1);
            format!("{current} di {}{more}", format::count(self.search.matches.len()))
        };
        ui.set_match_info(info.into());
    }
}
