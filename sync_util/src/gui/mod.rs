//! Interfaccia grafica (Slint): stato dell'applicazione e collegamento dei callback.
//!
//! - `console`: log con ricerca
//! - `files` / `file_tree`: alberatura dei file elaborati
//! - `exclusions`: editor delle regole di esclusione
//! - `drop`: drag & drop delle cartelle
//! - `settings`: regole e tema salvati nella cartella dell'utente
//! - `title_bar`: barra del titolo di Windows in tinta con il tema

mod console;
mod drop;
mod exclusions;
mod file_tree;
mod files;
mod format;
mod settings;
mod title_bar;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Timer, TimerMode};
use sync_util::engine::{self, DEFAULT_TEXT, Log, LogKind, SyncState};

use console::Console;
use exclusions::Exclusions;
use files::FilesView;

slint::include_modules!();

const SEPARATOR: &str = "  ·  ";

/// Stato mostrato accanto ai pulsanti: determina il colore del pallino.
#[derive(Clone, Copy)]
enum Status {
    Idle = 0,
    Running = 1,
    Success = 2,
    Warning = 3,
    Error = 4,
}

struct App {
    ui: slint::Weak<AppWindow>,
    log: Arc<Log>,
    console: Console,
    files: FilesView,
    exclusions: Exclusions,
    source: Option<PathBuf>,
    target: Option<PathBuf>,
    worker: Option<(JoinHandle<()>, Arc<SyncState>)>,
    started_at: Instant,
    dark: bool,
    drop_handled: bool,
}

type Shared = Rc<RefCell<App>>;

impl App {
    fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("finestra principale")
    }

    fn running(&self) -> bool {
        self.worker.is_some()
    }

    /* ======= Stato e cartelle ======= */

    fn set_status(&self, status: Status, text: &str) {
        let ui = self.ui();
        ui.set_status_kind(status as i32);
        ui.set_status_text(text.into());
    }

    fn update_ready_status(&self) {
        let ready = self.source.is_some() && self.target.is_some();
        self.ui().set_can_start(ready);
        if ready {
            self.set_status(Status::Idle, "Pronto per la sincronizzazione");
        } else {
            self.set_status(Status::Idle, "Seleziona o trascina le cartelle sorgente e destinazione per iniziare");
        }
    }

    fn set_source(&mut self, dir: PathBuf) {
        let text = dir.display().to_string();
        self.ui().set_source_path(text.as_str().into());
        self.log.push(LogKind::Info, format!("Cartella sorgente selezionata: {text}"));
        self.source = Some(dir);
        self.update_ready_status();
    }

    fn set_target(&mut self, dir: PathBuf) {
        let text = dir.display().to_string();
        self.ui().set_target_path(text.as_str().into());
        self.log.push(LogKind::Info, format!("Cartella destinazione selezionata: {text}"));
        self.target = Some(dir);
        self.update_ready_status();
    }

    /// Sposta nella console le righe arrivate da qualsiasi thread.
    fn flush_log(&mut self) {
        let ui = self.ui();
        self.console.append(&ui, self.log.take());
    }

    fn copy_log(&mut self) {
        self.flush_log();
        let text = self.console.text();
        if text.is_empty() {
            return;
        }
        if let Err(e) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
            self.log.push(LogKind::Warning, format!("Impossibile copiare negli appunti: {e}"));
        }
    }

    /* ======= Sincronizzazione ======= */

    fn refresh_stats(&self, state: &SyncState) {
        let ui = self.ui();
        ui.set_copied_value(format::count(state.copied()).into());
        ui.set_deleted_value(format::count(state.deleted()).into());
        ui.set_identical_value(format::count(state.identical()).into());
        ui.set_denied_value(format::count(state.denied_count()).into());
    }

    fn start_sync(&mut self) {
        let (Some(source), Some(target)) = (self.source.clone(), self.target.clone()) else {
            self.log.push(LogKind::Warning, "Seleziona sia la cartella sorgente che quella di destinazione.");
            return;
        };
        if self.running() {
            return;
        }
        // cartelle annidate: la sincronizzazione cancellerebbe la sorgente o ricorrerebbe all'infinito
        if engine::folders_overlap(&source, &target) {
            self.log.push(
                LogKind::Error,
                "Sorgente e destinazione devono essere cartelle distinte e non contenute l'una nell'altra.",
            );
            self.set_status(Status::Error, "Cartelle non valide: sorgente e destinazione si sovrappongono");
            return;
        }

        let ui = self.ui();
        self.files.reset(&ui);
        self.exclusions.save(&ui);
        let rules = self.exclusions.rules().clone();
        self.log.push(LogKind::Section, "› Avvio sincronizzazione");
        self.log.push(
            LogKind::Info,
            format::plural(rules.len(), "regola di esclusione attiva", "regole di esclusione attive"),
        );

        let state = Arc::new(SyncState::new(self.log.clone()));
        self.refresh_stats(&state);
        let (worker_state, log) = (state.clone(), self.log.clone());
        let handle = std::thread::spawn(move || {
            log.push(LogKind::Info, format!("Copia dei file da {} a {}", source.display(), target.display()));
            engine::synchronize(&source, &target, &rules, &worker_state);
        });
        self.worker = Some((handle, state));
        self.started_at = Instant::now();
        ui.set_running(true);
        self.tick();
    }

    /// Aggiornamento periodico: tempo trascorso, contatori e alberi in tempo reale; chiude la sincronizzazione finita.
    fn tick(&mut self) {
        let Some((handle, state)) = &self.worker else { return };
        let (state, finished) = (state.clone(), handle.is_finished());
        let ui = self.ui();
        self.refresh_stats(&state);
        self.files.take_processed(&ui, &state);
        if !finished {
            let text = format!("Sincronizzazione in corso…  {}", format::elapsed(self.started_at.elapsed()));
            self.set_status(Status::Running, &text);
            return;
        }

        let (handle, state) = self.worker.take().expect("sincronizzazione in corso");
        let outcome = handle.join();
        ui.set_running(false);
        self.refresh_stats(&state);
        self.files.take_processed(&ui, &state);
        self.files.finish(&ui);
        let elapsed = format::elapsed(self.started_at.elapsed());

        if outcome.is_err() {
            self.log.push(LogKind::Error, "La sincronizzazione si è interrotta per un errore imprevisto.");
            self.set_status(Status::Error, "Sincronizzazione interrotta da un errore imprevisto");
            return;
        }
        let copied = state.copied();
        let summary = format!(
            "Completata in {elapsed}{SEPARATOR}{}{SEPARATOR}{}",
            format::plural(copied, "file copiato", "file copiati"),
            format::plural(state.deleted(), "eliminato", "eliminati")
        );
        self.log.push(
            LogKind::Success,
            format!("Sincronizzazione completata in {elapsed}. Numero totale file copiati: {}", format::count(copied)),
        );
        let denied = state.denied_files();
        if denied.is_empty() {
            self.set_status(Status::Success, &summary);
        } else {
            self.log.push(
                LogKind::Warning,
                "È consigliato rivedere i diritti di accesso dei seguenti file oppure copiarli a mano:",
            );
            for (path, name) in &denied {
                self.log.push(LogKind::Warning, format!("  • {name}  →  {}", path.display()));
            }
            self.set_status(Status::Warning, &format!("{summary}{SEPARATOR}{} con accesso negato", denied.len()));
        }
    }
}

fn open_in_explorer(path: Option<&Path>, log: &Log) {
    match path {
        None => log.push(LogKind::Warning, "Cartella non selezionata."),
        Some(path) => {
            if let Err(e) = std::process::Command::new("explorer").arg(path).spawn() {
                log.push(LogKind::Warning, format!("Impossibile aprire la cartella: {e}"));
            }
        }
    }
}

fn choose_directory(ui: &AppWindow, title: &str, current: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title).set_parent(&ui.window().window_handle());
    if let Some(dir) = current {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_folder()
}

/// Crea la finestra, collega i callback e resta in attesa fino alla chiusura.
pub fn run() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new().backend_name("winit".into()).select()?;
    let ui = AppWindow::new()?;

    let rules_text = settings::load_rules();
    let dark = settings::load_dark_theme();
    let app: Shared = Rc::new(RefCell::new(App {
        ui: ui.as_weak(),
        log: Arc::new(Log::default()),
        console: Console::new(),
        files: FilesView::new(),
        exclusions: Exclusions::new(&rules_text),
        source: None,
        target: None,
        worker: None,
        started_at: Instant::now(),
        dark,
        drop_handled: false,
    }));

    {
        let mut a = app.borrow_mut();
        ui.set_log_rows(a.console.model());
        ui.set_tree_rows(a.files.model());
        ui.set_rules_text(rules_text.into());
        ui.invoke_set_dark(dark);
        a.exclusions.update_status(&ui);
        a.files.refresh(&ui);
        a.update_ready_status();
    }

    connect_folders(&ui, &app);
    connect_actions(&ui, &app);
    connect_console(&ui, &app);
    connect_files(&ui, &app);
    let _save_timer = connect_exclusions(&ui, &app);
    drop::install(&ui, &app);

    // console aggiornata a blocchi; stato e contatori aggiornati 4 volte al secondo durante la sincronizzazione.
    // try_borrow_mut: se lo stato è in uso (es. un dialogo di sistema aperto) il timer salta un giro
    let log_timer = Timer::default();
    log_timer.start(TimerMode::Repeated, Duration::from_millis(100), {
        let app = app.clone();
        move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                a.flush_log();
            }
        }
    });
    let progress_timer = Timer::default();
    progress_timer.start(TimerMode::Repeated, Duration::from_millis(250), {
        let app = app.clone();
        move || {
            if let Ok(mut a) = app.try_borrow_mut() {
                a.tick();
            }
        }
    });

    // la finestra di sistema viene creata dopo l'avvio del ciclo degli eventi: si riprova finché non c'è
    let title_timer = Rc::new(Timer::default());
    title_timer.start(TimerMode::Repeated, Duration::from_millis(30), {
        let (ui_weak, app, timer) = (ui.as_weak(), app.clone(), Rc::downgrade(&title_timer));
        move || {
            let Some(ui) = ui_weak.upgrade() else { return };
            let dark = app.try_borrow().map(|a| a.dark).unwrap_or(dark);
            if title_bar::apply(&ui, dark)
                && let Some(timer) = timer.upgrade()
            {
                timer.stop();
            }
        }
    });

    ui.run()?;
    // chiusura dalla finestra: le esclusioni non ancora salvate vengono scritte
    app.borrow_mut().exclusions.save(&ui);
    Ok(())
}

/* ======= Callback ======= */
// I dialoghi di sistema fanno girare i messaggi della finestra mentre sono aperti:
// lo stato non resta in prestito durante la loro apertura.

fn connect_folders(ui: &AppWindow, app: &Shared) {
    ui.on_browse_source({
        let (app, ui_weak) = (app.clone(), ui.as_weak());
        move || {
            let current = app.borrow().source.clone();
            if let Some(dir) = choose_directory(&ui_weak.unwrap(), "Seleziona la cartella sorgente", current) {
                app.borrow_mut().set_source(dir);
            }
        }
    });
    ui.on_browse_target({
        let (app, ui_weak) = (app.clone(), ui.as_weak());
        move || {
            let current = app.borrow().target.clone();
            if let Some(dir) = choose_directory(&ui_weak.unwrap(), "Seleziona la cartella di destinazione", current) {
                app.borrow_mut().set_target(dir);
            }
        }
    });
    ui.on_open_source({
        let app = app.clone();
        move || {
            let a = app.borrow();
            open_in_explorer(a.source.as_deref(), &a.log);
        }
    });
    ui.on_open_target({
        let app = app.clone();
        move || {
            let a = app.borrow();
            open_in_explorer(a.target.as_deref(), &a.log);
        }
    });
}

fn connect_actions(ui: &AppWindow, app: &Shared) {
    ui.on_start({
        let app = app.clone();
        move || app.borrow_mut().start_sync()
    });
    ui.on_close_requested({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.exclusions.save(&ui);
            let _ = ui.hide();
        }
    });
    ui.on_toggle_theme({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            a.dark = !a.dark;
            let ui = a.ui();
            ui.invoke_set_dark(a.dark);
            title_bar::apply(&ui, a.dark);
            if let Err(e) = settings::save_dark_theme(a.dark) {
                a.log.push(LogKind::Warning, format!("Impossibile salvare il tema: {e}"));
            }
        }
    });
}

fn connect_console(ui: &AppWindow, app: &Shared) {
    ui.on_copy_log({
        let app = app.clone();
        move || app.borrow_mut().copy_log()
    });
    ui.on_clear_log({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            a.log.clear();
            let ui = a.ui();
            a.console.clear(&ui);
        }
    });
    ui.on_search({
        let app = app.clone();
        move |query| {
            let mut a = app.borrow_mut();
            a.flush_log();
            let ui = a.ui();
            a.console.search(&ui, &query, true);
        }
    });
    ui.on_next_match({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.console.move_match(&ui, 1);
        }
    });
    ui.on_previous_match({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.console.move_match(&ui, -1);
        }
    });
}

fn connect_files(ui: &AppWindow, app: &Shared) {
    ui.on_select_category({
        let app = app.clone();
        move |index| {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.files.show_category(&ui, index.max(0) as usize);
        }
    });
    ui.on_toggle_row({
        let app = app.clone();
        move |row| {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.files.toggle_row(&ui, row.max(0) as usize);
        }
    });
    ui.on_expand_all({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.files.set_all_expanded(&ui, true);
        }
    });
    ui.on_collapse_all({
        let app = app.clone();
        move || {
            let mut a = app.borrow_mut();
            let ui = a.ui();
            a.files.set_all_expanded(&ui, false);
        }
    });
}

/// Restituisce il timer del salvataggio ritardato, che deve vivere quanto la finestra.
fn connect_exclusions(ui: &AppWindow, app: &Shared) -> Rc<Timer> {
    let save_timer = Rc::new(Timer::default());
    ui.on_rules_edited({
        let (app, save_timer) = (app.clone(), save_timer.clone());
        move |text| {
            {
                let mut a = app.borrow_mut();
                let ui = a.ui();
                a.exclusions.edited(&ui, &text);
            }
            let app = app.clone();
            save_timer.start(TimerMode::SingleShot, Duration::from_millis(600), move || {
                if let Ok(mut a) = app.try_borrow_mut() {
                    let ui = a.ui();
                    a.exclusions.save(&ui);
                }
            });
        }
    });
    ui.on_restore_defaults({
        let (app, ui_weak) = (app.clone(), ui.as_weak());
        move || {
            let ui = ui_weak.unwrap();
            let confirmed = rfd::MessageDialog::new()
                .set_title("Ripristina esclusioni predefinite")
                .set_description("Sostituire le regole attuali con quelle predefinite?")
                .set_buttons(rfd::MessageButtons::OkCancel)
                .set_parent(&ui.window().window_handle())
                .show();
            if confirmed == rfd::MessageDialogResult::Ok {
                ui.set_rules_text(DEFAULT_TEXT.into());
                let mut a = app.borrow_mut();
                a.exclusions.edited(&ui, DEFAULT_TEXT);
                a.exclusions.save(&ui);
            }
        }
    });
    save_timer
}
