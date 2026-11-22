//! Scheda "File": un albero per ogni esito (copiati, eliminati, ...), una categoria visibile per volta.

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};
use sync_util::engine::{Outcome, SyncState};

use super::file_tree::FileTree;
use super::{AppWindow, TreeRow, format};

/// Etichetta e testo "vuoto" di ogni categoria, nell'ordine di `Outcome::ALL`.
const CATEGORIES: [(&str, &str); 5] = [
    ("Copiati", "Nessun file copiato"),
    ("Eliminati", "Nessun file eliminato dalla destinazione"),
    ("Invariati", "Nessun file invariato"),
    ("Accesso negato", "Nessun file con accesso negato"),
    ("Esclusi", "Nessun elemento escluso"),
];

pub struct FilesView {
    trees: Vec<FileTree>,
    rows: Rc<VecModel<TreeRow>>,
    paths: Vec<Vec<usize>>, // per ogni riga visibile, la posizione del nodo nell'albero
    selected: usize,
    started: bool,
}

impl FilesView {
    pub fn new() -> Self {
        FilesView {
            trees: new_trees(),
            rows: Rc::new(VecModel::default()),
            paths: Vec::new(),
            selected: 0,
            started: false,
        }
    }

    pub fn model(&self) -> ModelRc<TreeRow> {
        self.rows.clone().into()
    }

    /// Da chiamare all'avvio di una sincronizzazione.
    pub fn reset(&mut self, ui: &AppWindow) {
        self.started = true;
        self.trees = new_trees();
        self.refresh(ui);
    }

    pub fn show_category(&mut self, ui: &AppWindow, index: usize) {
        self.selected = index.min(CATEGORIES.len() - 1);
        self.trees[self.selected].expand_default();
        self.refresh(ui);
    }

    /// Aggiunge agli alberi i percorsi elaborati dall'ultima chiamata.
    pub fn take_processed(&mut self, ui: &AppWindow, state: &SyncState) {
        let mut changed = false;
        for outcome in Outcome::ALL {
            let paths = state.take_processed(outcome);
            let tree = &mut self.trees[outcome.index()];
            for path in &paths {
                tree.insert(path);
            }
            changed |= !paths.is_empty();
        }
        if changed {
            self.refresh(ui);
        }
    }

    /// A sincronizzazione finita: l'albero visibile si apre nella forma predefinita.
    pub fn finish(&mut self, ui: &AppWindow) {
        self.trees[self.selected].expand_default();
        self.refresh(ui);
    }

    pub fn toggle_row(&mut self, ui: &AppWindow, row: usize) {
        if let Some(path) = self.paths.get(row) {
            self.trees[self.selected].toggle(path);
            self.refresh(ui);
        }
    }

    pub fn set_all_expanded(&mut self, ui: &AppWindow, expanded: bool) {
        self.trees[self.selected].set_all_expanded(expanded);
        self.refresh(ui);
    }

    /// Ricostruisce le righe visibili, le etichette con i conteggi e il testo da mostrare se l'albero è vuoto.
    pub fn refresh(&mut self, ui: &AppWindow) {
        let (rows, paths) = self.trees[self.selected].rows();
        self.rows.set_vec(rows);
        self.paths = paths;

        let labels: Vec<SharedString> = CATEGORIES
            .iter()
            .zip(&self.trees)
            .map(|((label, _), tree)| {
                if self.started { format!("{label}  {}", format::count(tree.count())).into() } else { (*label).into() }
            })
            .collect();
        ui.set_category_labels(Rc::new(VecModel::from(labels)).into());
        ui.set_selected_category(self.selected as i32);
        ui.set_files_empty_text(
            if self.started {
                CATEGORIES[self.selected].1
            } else {
                "Avvia una sincronizzazione per vedere qui i file elaborati"
            }
            .into(),
        );
    }
}

fn new_trees() -> Vec<FileTree> {
    (0..CATEGORIES.len()).map(|_| FileTree::default()).collect()
}
