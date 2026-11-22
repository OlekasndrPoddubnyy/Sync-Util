// in release niente finestra di console accanto a quella dell'applicazione
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod gui;

fn main() -> Result<(), slint::PlatformError> {
    gui::run()
}
