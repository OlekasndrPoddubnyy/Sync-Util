//! Scheda "Esclusioni": regole in stile .gitignore, salvate da sole poco dopo l'ultima modifica.

use sync_util::engine::IgnoreRules;

use super::{AppWindow, settings};

const SEPARATOR: &str = "  ·  ";

pub struct Exclusions {
    rules: IgnoreRules,
    dirty: bool,
    save_error: Option<String>,
}

impl Exclusions {
    pub fn new(text: &str) -> Self {
        Exclusions { rules: IgnoreRules::parse(text), dirty: false, save_error: None }
    }

    /// Le regole corrispondenti al testo attuale dell'editor.
    pub fn rules(&self) -> &IgnoreRules {
        &self.rules
    }

    pub fn edited(&mut self, ui: &AppWindow, text: &str) {
        self.rules = IgnoreRules::parse(text);
        self.dirty = true;
        self.update_status(ui);
    }

    /// Scrive le regole se ci sono modifiche non ancora salvate.
    pub fn save(&mut self, ui: &AppWindow) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.save_error = settings::save_rules(&ui.get_rules_text()).err().map(|e| e.to_string());
        self.update_status(ui);
    }

    pub fn update_status(&self, ui: &AppWindow) {
        ui.set_rules_count(self.rules.len() as i32);
        let invalid = self.rules.invalid_lines();
        let (text, kind) = if let Some(error) = &self.save_error {
            (format!("Salvataggio non riuscito: {error}"), 2)
        } else if let Some(first) = invalid.first() {
            let more = if invalid.len() > 1 { format!("  (e altre {})", invalid.len() - 1) } else { String::new() };
            (format!("Riga {} non valida, viene ignorata: {}{more}", first.number, first.text), 1)
        } else {
            let count = self.rules.len();
            let rules = if count == 1 { "1 regola attiva".to_string() } else { format!("{count} regole attive") };
            (format!("{rules}{SEPARATOR}salvate in {}", settings::rules_file().display()), 0)
        };
        ui.set_rules_status(text.into());
        ui.set_rules_status_kind(kind);
    }
}
