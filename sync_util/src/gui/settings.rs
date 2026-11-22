//! Impostazioni salvate nella cartella dell'utente, in ~/.dimensions-sync:
//! - syncignore: le regole di esclusione (lo stesso file della versione Java, quindi condiviso);
//! - settings: il tema scelto.

use std::fs;
use std::io;
use std::path::PathBuf;

use sync_util::engine::DEFAULT_TEXT;

const THEME_KEY: &str = "theme.dark";

fn config_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).unwrap_or_default();
    PathBuf::from(home).join(".dimensions-sync")
}

pub fn rules_file() -> PathBuf {
    config_dir().join("syncignore")
}

fn settings_file() -> PathBuf {
    config_dir().join("settings")
}

/// Le regole salvate, oppure quelle predefinite al primo avvio.
pub fn load_rules() -> String {
    fs::read_to_string(rules_file()).unwrap_or_else(|_| DEFAULT_TEXT.to_string())
}

pub fn save_rules(text: &str) -> io::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::write(rules_file(), text)
}

pub fn load_dark_theme() -> bool {
    fs::read_to_string(settings_file())
        .map(|text| text.lines().any(|line| line.trim() == format!("{THEME_KEY}=true")))
        .unwrap_or(false)
}

pub fn save_dark_theme(dark: bool) -> io::Result<()> {
    fs::create_dir_all(config_dir())?;
    fs::write(settings_file(), format!("{THEME_KEY}={dark}\n"))
}
