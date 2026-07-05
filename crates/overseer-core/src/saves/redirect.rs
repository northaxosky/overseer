//! Per-profile save-game redirection via the game's `SLocalSavePath` INI key

use crate::error::IoError;
use crate::fs;
use crate::ini::{self, Ini};
use crate::restore::{MissingCurrent, Restore, restore_if_ours};
use camino::Utf8Path;

const SECTION: &str = "General";
const KEY: &str = "SLocalSavePath";

/// The `SLocalSavePath` value for a profile: `Saves\<profile>\`
pub fn save_redirect_value(profile: &str) -> String {
    format!("Saves\\{profile}\\")
}

/// Point the game's saves at this profile's subfolder
pub fn apply_save_redirect(
    custom_ini: &Utf8Path,
    saves_dir: &Utf8Path,
    profile: &str,
) -> Result<Option<String>, IoError> {
    let text = fs::read_to_string_opt(custom_ini)?.unwrap_or_default();

    let original = Ini::parse(&text).get(SECTION, KEY).map(str::to_owned);
    let updated = ini::set_key(&text, SECTION, KEY, &save_redirect_value(profile));

    fs::write_atomic(custom_ini, updated.as_bytes())?;
    fs::ensure_dir(saves_dir)?;
    Ok(original)
}

/// Undo the redirect, but only when the live value is the one we wrote
pub fn restore_save_redirect(
    custom_ini: &Utf8Path,
    profile: &str,
    original: Option<&str>,
) -> Result<Restore, IoError> {
    restore_if_ours(
        Some(Some(save_redirect_value(profile))),
        || {
            Ok(fs::read_to_string_opt(custom_ini)?.map(|text| {
                let current = Ini::parse(&text).get(SECTION, KEY).map(str::to_owned);
                (current, text)
            }))
        },
        |text| {
            let Some(text) = text else {
                return Ok(());
            };
            let updated = match original {
                Some(value) => ini::set_key(&text, SECTION, KEY, value),
                None => ini::unset_key(&text, SECTION, KEY),
            };
            fs::write_atomic(custom_ini, updated.as_bytes())
        },
        MissingCurrent::Restored,
    )
}

#[cfg(test)]
#[path = "tests/redirect.rs"]
mod tests;
