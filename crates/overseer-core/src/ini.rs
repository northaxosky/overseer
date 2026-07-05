//! Reading the game's INI files (`Fallout4.ini`, `Fallout4Custom.ini`, `Fallout4Prefs.ini`)

use crate::instance::{Instance, InstanceError};
use std::collections::BTreeMap;
use thiserror::Error;

/// Something went wrong locating or reading the game INIs
#[derive(Debug, Error)]
pub enum IniError {
    #[error(transparent)]
    Instance(#[from] InstanceError),

    #[error(transparent)]
    Io(#[from] crate::error::IoError),
}

/// A parsed INI file: sections of key/value pairs
#[derive(Debug, Clone, Default)]
pub struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    /// Parse INI text: `[section]` headers and `key=value` lines; everything else ignored
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = name.trim().to_lowercase();
            } else if let Some((key, value)) = line.split_once('=') {
                sections
                    .entry(current.clone())
                    .or_default()
                    .insert(key.trim().to_lowercase(), value.trim().to_owned());
            }
        }
        Self { sections }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_lowercase())?
            .get(&key.to_lowercase())
            .map(String::as_str)
    }

    pub fn merge(&mut self, other: Ini) {
        for (section, keys) in other.sections {
            let target = self.sections.entry(section).or_default();
            target.extend(keys);
        }
    }
}

/// `Some(lowercased_name)` if `line` is a `[section]` header
fn section_name(line: &str) -> Option<String> {
    line.trim()
        .strip_prefix('[')
        .and_then(|l| l.strip_suffix(']'))
        .map(|n| n.trim().to_lowercase())
}

/// Whether `line` assigned `key` (case-insensitive)
fn assigns(line: &str, key_lower: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(k, _)| k.trim().to_lowercase() == key_lower)
}

/// The newline to write for `text`: preserve LF/CRLF (default=CRLF)
fn newline_of(text: &str) -> &'static str {
    if text.contains("\r\n") || !text.contains('\n') {
        "\r\n"
    } else {
        "\n"
    }
}

/// Remove `[section] key`, leaving every other line intact. No-op if absent
pub fn unset_key(text: &str, section: &str, key: &str) -> String {
    let want_section = section.trim().to_lowercase();
    let want_key = key.trim().to_lowercase();
    let mut in_section = false;
    text.lines()
        .filter(|line| {
            if let Some(name) = section_name(line) {
                in_section = name == want_section;
                true
            } else {
                !(in_section && assigns(line, &want_key))
            }
        })
        .collect::<Vec<_>>()
        .join(newline_of(text))
}

/// Set `[section] key=value`, leaving every other line intact
pub fn set_key(text: &str, section: &str, key: &str, value: &str) -> String {
    let cleaned = unset_key(text, section, key);
    let want_section = section.trim().to_lowercase();
    let mut lines: Vec<String> = cleaned.lines().map(str::to_owned).collect();

    let mut insert_at = None;
    let mut in_section = false;
    for (i, line) in lines.iter().enumerate() {
        if let Some(name) = section_name(line) {
            if in_section {
                insert_at = Some(i);
            }
            in_section = name == want_section;
        }
    }
    if in_section {
        insert_at = Some(lines.len());
    }

    match insert_at {
        Some(at) => lines.insert(at, format!("{key}={value}")),
        None => {
            lines.push(format!("[{}]", section.trim()));
            lines.push(format!("{key}={value}"));
        }
    }
    lines.join(newline_of(text))
}

/// The game INIs, parsed: `settings` is `<stem>.ini` merged with `<stem>Custom.ini`; prefs is `<stem>Prefs.ini`
#[derive(Debug, Clone, Default)]
pub struct GameInis {
    pub settings: Ini,
    pub prefs: Ini,
}

/// Read and parse the game's INIs from the resolved directory
pub fn read_game_inis(instance: &Instance) -> Result<GameInis, IniError> {
    let dir = instance.ini_dir()?;
    let stem = instance.config.game.ini_stem();

    let read = |name: String| -> Result<Ini, IniError> {
        let path = dir.join(name);
        Ok(crate::fs::read_to_string_opt(&path)?
            .map(|t| Ini::parse(&t))
            .unwrap_or_default())
    };

    let mut settings = read(format!("{stem}.ini"))?;
    settings.merge(read(format!("{stem}Custom.ini"))?);
    let prefs = read(format!("{stem}Prefs.ini"))?;

    Ok(GameInis { settings, prefs })
}

#[cfg(test)]
#[path = "tests/ini.rs"]
mod tests;
