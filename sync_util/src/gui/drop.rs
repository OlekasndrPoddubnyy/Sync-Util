//! Drag & drop delle cartelle da Esplora risorse sui riquadri sorgente e destinazione.
//!
//! winit segnala i file trascinati sulla finestra ma non la posizione del cursore: durante il trascinamento
//! la si legge dal sistema, così si evidenzia (e riceve la cartella) il riquadro che sta sotto il cursore.

use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Timer, TimerMode};
use sync_util::engine::LogKind;

use super::{AppWindow, Shared};

const SOURCE_ZONE: i32 = 1;
const TARGET_ZONE: i32 = 2;

pub fn install(ui: &AppWindow, app: &Shared) {
    let hover_timer = Rc::new(Timer::default());
    let ui_weak = ui.as_weak();
    let app = app.clone();
    ui.window().on_winit_window_event(move |_, event| {
        match event {
            WindowEvent::HoveredFile(_) => {
                if let Ok(mut a) = app.try_borrow_mut() {
                    a.drop_handled = false;
                }
                if !hover_timer.running() {
                    let (ui_weak, app) = (ui_weak.clone(), app.clone());
                    hover_timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
                        let Some(ui) = ui_weak.upgrade() else { return };
                        let locked = app.try_borrow().map(|a| a.running()).unwrap_or(true);
                        ui.set_drag_zone(if locked { 0 } else { zone_under_cursor(&ui) });
                    });
                }
            }
            WindowEvent::HoveredFileCancelled => {
                hover_timer.stop();
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_drag_zone(0);
                }
            }
            WindowEvent::DroppedFile(path) => {
                hover_timer.stop();
                // fuori dal gestore dell'evento: la finestra winit si può interrogare senza conflitti
                let (path, ui_weak, app) = (path.clone(), ui_weak.clone(), app.clone());
                Timer::single_shot(Duration::ZERO, move || {
                    let Some(ui) = ui_weak.upgrade() else { return };
                    let zone = zone_under_cursor(&ui);
                    ui.set_drag_zone(0);
                    let Ok(mut a) = app.try_borrow_mut() else { return };
                    // si usa solo il primo elemento trascinato, e solo se è una cartella
                    if a.drop_handled || a.running() {
                        return;
                    }
                    a.drop_handled = true;
                    if !path.is_dir() {
                        a.log.push(LogKind::Warning, format!("Trascina una cartella, non un file: {}", path.display()));
                        return;
                    }
                    match zone {
                        SOURCE_ZONE => a.set_source(path),
                        TARGET_ZONE => a.set_target(path),
                        _ => {}
                    }
                });
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

/// Riquadro sotto il cursore: 1 sorgente, 2 destinazione, 0 nessuno.
fn zone_under_cursor(ui: &AppWindow) -> i32 {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut cursor = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos scrive soltanto nella struttura passata
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return 0;
    }
    let Some(Ok(origin)) = ui.window().with_winit_window(|w| w.inner_position()) else {
        return 0;
    };
    let scale = ui.window().scale_factor();
    let x = (cursor.x - origin.x) as f32 / scale;
    let y = (cursor.y - origin.y) as f32 / scale;
    ui.invoke_zone_at(x, y)
}
