//! Plugin-list separators: a per-profile sidecar (`separators.txt`) kept out of `plugins.txt`

use super::load_order::PluginEntry;
use crate::fs;
use camino::Utf8Path;
use thiserror::Error;

/// Errors from reading or managing plugin separators
#[derive(Debug, Error)]
pub enum SeparatorError {
    #[error(transparent)]
    Io(#[from] crate::error::IoError),

    #[error("invalid separator name: {0}")]
    InvalidName(String),

    #[error("no separator at index: {0}")]
    NoSeparatorAt(usize),
}

/// A user divider in the plugins list, sitting above an anchor plugin
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Separator {
    /// Display name, stored raw (no `_separator` suffix)
    pub name: String,
    /// Filename of the plugin directly below it
    pub anchor: Option<String>,
}

/// A profile's plugin separators, persisted as `separators.txt` beside `plugins.txt`
#[derive(Debug, Clone, Default)]
pub struct PluginSeparators {
    /// Separators in top to bottom display order
    pub items: Vec<Separator>,
}

impl PluginSeparators {
    /// Load a profile's `separators.txt`; missing a file yields an empty set
    pub fn load(profile_dir: &Utf8Path) -> Result<Self, SeparatorError> {
        let path = profile_dir.join("separators.txt");
        let text = fs::read_to_string_opt(&path)?.unwrap_or_default();
        Ok(Self {
            items: parse_separators(&text),
        })
    }

    /// Write the profile's `separators.txt`, creating the profile dir if necessary
    pub fn save(&self, profile_dir: &Utf8Path) -> Result<(), SeparatorError> {
        let path = profile_dir.join("separators.txt");
        fs::write_atomic(&path, self.to_text().as_bytes())?;
        Ok(())
    }

    /// Serialize to `separators.txt` text: one `<anchor>\t<name>` line per separator
    fn to_text(&self) -> String {
        let mut out = String::new();
        for sep in &self.items {
            out.push_str(sep.anchor.as_deref().unwrap_or(""));
            out.push('\t');
            out.push_str(&sep.name);
            out.push('\n');
        }
        out
    }

    /// Insert a separator anchored above `anchor` at display position `at`, clamped to the end
    pub fn insert(
        &mut self,
        at: usize,
        anchor: Option<String>,
        name: &str,
    ) -> Result<(), SeparatorError> {
        let name = validate_separator_name(name)?;
        let at = at.min(self.items.len());
        self.items.insert(at, Separator { name, anchor });
        Ok(())
    }

    /// Rename the separator at `index`
    pub fn rename(&mut self, index: usize, name: &str) -> Result<(), SeparatorError> {
        let name = validate_separator_name(name)?;
        let sep = self
            .items
            .get_mut(index)
            .ok_or(SeparatorError::NoSeparatorAt(index))?;
        sep.name = name;
        Ok(())
    }

    /// Remove the separator at `index`
    pub fn remove(&mut self, index: usize) -> Result<(), SeparatorError> {
        if index >= self.items.len() {
            return Err(SeparatorError::NoSeparatorAt(index));
        }
        self.items.remove(index);
        Ok(())
    }
}

/// A row in the merged plugins view: an index into the plugins slice or the separators slice
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRow {
    Plugin(usize),
    Separator(usize),
}

/// Display rows: separators interleaved above their anchor plugin, in the plugins' load order
pub fn merge_rows(plugins: &[PluginEntry], separators: &[Separator]) -> Vec<PluginRow> {
    let mut rows = Vec::with_capacity(plugins.len() + separators.len());
    let mut emitted = vec![false; separators.len()];
    for (i, plugin) in plugins.iter().enumerate() {
        for (s, sep) in separators.iter().enumerate() {
            if !emitted[s]
                && sep
                    .anchor
                    .as_deref()
                    .is_some_and(|a| a.eq_ignore_ascii_case(&plugin.name))
            {
                emitted[s] = true;
                rows.push(PluginRow::Separator(s));
            }
        }
        rows.push(PluginRow::Plugin(i));
    }
    for (s, done) in emitted.iter().enumerate() {
        if !done {
            rows.push(PluginRow::Separator(s));
        }
    }
    rows
}

/// Validate a plugin separator display name and return it trimmed; store raw, no suffix
fn validate_separator_name(name: &str) -> Result<String, SeparatorError> {
    let name = name.trim();
    let reject = |why: &str| Err(SeparatorError::InvalidName(why.to_owned()));
    if name.is_empty() {
        return reject("name cannot be empty");
    }
    if name.chars().any(char::is_control) {
        return reject("name cannot contain control characters");
    }
    if name.contains(['/', '\\']) {
        return reject("name cannot contain path separators");
    }
    if name.starts_with('#') || name.starts_with('*') {
        return reject("name cannot start with `#` or `*`");
    }
    Ok(name.to_owned())
}

/// Parse `separators.txt`: each line `<anchor>\t<name>`, an empty anchor field means None
fn parse_separators(text: &str) -> Vec<Separator> {
    text.lines()
        .filter_map(|line| {
            if line.trim().is_empty() || line.trim_start().starts_with('#') {
                return None;
            }
            let (anchor, name) = line.split_once('\t').unwrap_or(("", line));
            let name = name.trim();
            (!name.is_empty()).then(|| Separator {
                name: name.to_owned(),
                anchor: (!anchor.is_empty()).then(|| anchor.to_owned()),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/separator.rs"]
mod tests;
