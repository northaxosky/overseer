//! Per-profile save-game redirection via the game's `SLocalSavePath` INI key

use crate::error::IoError;
use crate::fs;
use crate::ini::{self, Ini};
use crate::restore::{Restore, restore_if_ours};
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
    let original = read_save_redirect(custom_ini)?;
    write_save_redirect(custom_ini, saves_dir, profile)?;
    Ok(original)
}

/// Read the current save redirect without changing the INI
pub(crate) fn read_save_redirect(custom_ini: &Utf8Path) -> Result<Option<String>, IoError> {
    let text = fs::read_to_string_opt(custom_ini)?.unwrap_or_default();
    Ok(Ini::parse(&text).get(SECTION, KEY).map(str::to_owned))
}

/// Write the profile save redirect after its original value is journalled
pub(crate) fn write_save_redirect(
    custom_ini: &Utf8Path,
    saves_dir: &Utf8Path,
    profile: &str,
) -> Result<(), IoError> {
    let text = fs::read_to_string_opt(custom_ini)?.unwrap_or_default();
    let updated = ini::set_key(&text, SECTION, KEY, &save_redirect_value(profile));

    fs::write_atomic(custom_ini, updated.as_bytes())?;
    fs::ensure_dir(saves_dir)?;
    Ok(())
}

/// Undo the redirect, but only when the live value is the one we wrote
pub fn restore_save_redirect(
    custom_ini: &Utf8Path,
    profile: &str,
    original: Option<&str>,
) -> Result<Restore, IoError> {
    restore_if_ours(
        Some(Some(save_redirect_value(profile))),
        original.map(str::to_owned),
        || {
            Ok(match fs::read_to_string_opt(custom_ini)? {
                Some(text) => {
                    let current = Ini::parse(&text).get(SECTION, KEY).map(str::to_owned);
                    (current, Some(text))
                }
                None => (None, None),
            })
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
    )
}

/// Restore the original save redirect without comparing live content
pub(crate) fn restore_save_redirect_unconditionally(
    custom_ini: &Utf8Path,
    original: Option<&str>,
) -> Result<(), IoError> {
    let current = fs::read_to_string_opt(custom_ini)?;
    if current.is_none() && original.is_none() {
        return Ok(());
    }
    let text = current.unwrap_or_default();
    let updated = match original {
        Some(value) => ini::set_key(&text, SECTION, KEY, value),
        None => ini::unset_key(&text, SECTION, KEY),
    };
    fs::write_atomic(custom_ini, updated.as_bytes())
}

#[cfg(test)]
#[path = "tests/redirect.rs"]
mod tests;
