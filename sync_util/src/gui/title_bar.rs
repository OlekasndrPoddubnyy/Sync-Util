//! Barra del titolo di Windows in tinta con l'applicazione: modalità chiara/scura e, su Windows 11,
//! lo stesso colore di sfondo della finestra (così barra e contenuto sembrano un'unica superficie).

use std::ffi::c_void;

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;
use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};

use super::AppWindow;

// gli stessi colori di Theme.background e Theme.foreground in ui/theme.slint
const LIGHT: (u32, u32) = (0xf5f6fa, 0x0f172a);
const DARK: (u32, u32) = (0x0f1117, 0xe7e9ee);

/// Da chiamare a ogni cambio di tema; restituisce false se la finestra di sistema non esiste ancora.
pub fn apply(ui: &AppWindow, dark: bool) -> bool {
    let hwnd = ui.window().with_winit_window(|window| match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get() as *mut c_void),
        _ => None,
    });
    let Some(Some(hwnd)) = hwnd else { return false };

    let (background, text) = if dark { DARK } else { LIGHT };
    // su Windows 10 gli attributi del colore non esistono: la chiamata fallisce senza conseguenze
    set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, i32::from(dark) as u32);
    set_attribute(hwnd, DWMWA_CAPTION_COLOR, colorref(background));
    set_attribute(hwnd, DWMWA_TEXT_COLOR, colorref(text));
    true
}

fn set_attribute(hwnd: *mut c_void, attribute: i32, value: u32) {
    // SAFETY: hwnd è la finestra di questa applicazione; il valore è un DWORD di 4 byte, come richiesto
    unsafe {
        DwmSetWindowAttribute(hwnd, attribute as u32, (&raw const value).cast(), size_of::<u32>() as u32);
    }
}

/// 0xRRGGBB → COLORREF di Windows (0x00BBGGRR).
fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

#[cfg(test)]
mod tests {
    #[test]
    fn colorref_swaps_red_and_blue() {
        assert_eq!(super::colorref(0x112233), 0x332211);
    }
}
