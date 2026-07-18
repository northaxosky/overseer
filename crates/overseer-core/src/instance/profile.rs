//! A profile's mod list: `modlist.txt` ordering, separators, and reconciliation

use super::error::InstanceError;
use super::model::Instance;
use crate::deploy::ModSource;
use crate::fs;
use crate::plugins::{PluginError, PluginLoadOrder, PluginMeta, discover_plugins};
use crate::separator::validate_separator_name;
use camino::{Utf8Path, Utf8PathBuf};

/// What kind of `modlist.txt` line an entry is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKind {
    /// A mod Overseer manages, deployed from `mods/<name>/`
    Managed,
    /// A game-shipped/foreign plugin (DLC, CC) managed elsewhere; always active
    Foreign,
}

/// A real mod in a profile's mod list
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModEntry {
    /// Mod display and folder name
    pub name: String,
    /// Whether this mod contributes files and plugins
    pub enabled: bool,
    /// Whether Overseer manages this mod's files
    pub kind: ModKind,
}

/// One positional row in a profile's mod list
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModRow {
    /// A real managed or foreign mod
    Item(ModEntry),
    /// A visual divider with its raw display name
    Separator(String),
}

/// Profile: a named, ordered mod list
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    mods: Vec<ModRow>,
    pub local_saves: bool,
}

/// The persisted result of deriving a profile's plugin load order
#[derive(Debug, Clone)]
pub struct CommitOutcome {
    /// All plugins discovered from the enabled mods
    pub discovered: Vec<PluginMeta>,
    /// The reconciled and persisted load order
    pub order: PluginLoadOrder,
}

impl Profile {
    /// Build a profile from positional rows
    pub fn new(name: impl Into<String>, rows: Vec<ModRow>, local_saves: bool) -> Self {
        Self {
            name: name.into(),
            mods: rows,
            local_saves,
        }
    }

    /// All positional mod-list rows
    pub fn rows(&self) -> &[ModRow] {
        &self.mods
    }

    /// Replace all positional rows
    pub fn replace_rows(&mut self, rows: Vec<ModRow>) {
        self.mods = rows;
    }

    /// Append one positional row
    pub fn push_row(&mut self, row: ModRow) {
        self.mods.push(row);
    }

    /// Real mods only, in storage order
    pub fn items(&self) -> impl DoubleEndedIterator<Item = &ModEntry> + '_ {
        self.mods.iter().filter_map(|row| match row {
            ModRow::Item(item) => Some(item),
            ModRow::Separator(_) => None,
        })
    }

    /// Mutable real mods only, in storage order
    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut ModEntry> + '_ {
        self.mods.iter_mut().filter_map(|row| match row {
            ModRow::Item(item) => Some(item),
            ModRow::Separator(_) => None,
        })
    }

    /// Real mod at a positional row index
    pub fn item_at_row(&self, row: usize) -> Option<&ModEntry> {
        match self.mods.get(row) {
            Some(ModRow::Item(item)) => Some(item),
            _ => None,
        }
    }

    /// Separator display name at a positional row index
    pub fn separator_at_row(&self, row: usize) -> Option<&str> {
        match self.mods.get(row) {
            Some(ModRow::Separator(name)) => Some(name),
            _ => None,
        }
    }

    /// Swap two positional rows
    pub fn swap_rows(&mut self, a: usize, b: usize) -> bool {
        if a >= self.mods.len() || b >= self.mods.len() {
            return false;
        }
        self.mods.swap(a, b);
        true
    }

    /// Set the enabled state of the managed item at a row
    pub fn set_item_enabled_at_row(
        &mut self,
        row: usize,
        enabled: bool,
    ) -> Result<(), InstanceError> {
        let Some(ModRow::Item(item)) = self.mods.get_mut(row) else {
            return Err(InstanceError::ModNotInList(format!("row {row}")));
        };
        if item.kind != ModKind::Managed {
            return Err(InstanceError::NotManaged(item.name.clone()));
        }
        item.enabled = enabled;
        Ok(())
    }

    /// Row index of the item at a 0-based item ordinal, or the row count at the end
    pub fn row_for_item_ordinal(&self, ordinal: usize) -> usize {
        self.mods
            .iter()
            .enumerate()
            .filter_map(|(row, value)| matches!(value, ModRow::Item(_)).then_some(row))
            .nth(ordinal)
            .unwrap_or(self.mods.len())
    }

    /// Read a profile's `modlist.txt` + `settings.ini`. A missing modlist = empty profile
    pub fn load(instance: &Instance, name: &str) -> Result<Self, InstanceError> {
        let dir = instance.profile_dir(name);
        let text = fs::read_to_string_opt(&dir.join("modlist.txt"))?.unwrap_or_default();
        Ok(Self {
            name: name.to_owned(),
            mods: parse_modlist(&text),
            local_saves: read_local_saves(&dir)?,
        })
    }

    /// Read an existing profile directory; a missing `modlist.txt` still means an empty profile
    pub fn load_existing(instance: &Instance, name: &str) -> Result<Self, InstanceError> {
        if !instance.profile_dir(name).is_dir() {
            return Err(InstanceError::ProfileNotFound(name.to_owned()));
        }
        Self::load(instance, name)
    }

    /// Write the profile's `modlist.txt` + `settings.ini`, creating the dir if needed
    pub fn save(&self, instance: &Instance) -> Result<(), InstanceError> {
        self.save_modlist(instance)?;
        let dir = instance.profile_dir(&self.name);
        write_local_saves(&dir, self.local_saves)?;
        Ok(())
    }

    /// Write only `modlist.txt` (a single atomic write), leaving `settings.ini` untouched
    pub fn save_modlist(&self, instance: &Instance) -> Result<(), InstanceError> {
        instance.ensure_mod_state_available()?;
        let dir = instance.profile_dir(&self.name);
        fs::write_atomic(
            &dir.join("modlist.txt"),
            self.to_modlist_string().as_bytes(),
        )?;
        Ok(())
    }

    /// Serialize items and separator markers to `modlist.txt`
    pub(crate) fn to_modlist_string(&self) -> String {
        let mut out = String::new();
        for row in &self.mods {
            match row {
                ModRow::Separator(name) => {
                    out.push_str("|\tseparator\t");
                    out.push_str(name);
                    out.push('\n');
                }
                ModRow::Item(entry) => {
                    out.push(match entry.kind {
                        ModKind::Foreign => '*',
                        ModKind::Managed if entry.enabled => '+',
                        ModKind::Managed => '-',
                    });
                    out.push_str(&entry.name);
                    out.push('\n');
                }
            }
        }
        out
    }

    /// Enabled managed mods as deploy sources, lowest priority first
    pub fn deploy_sources(&self, instance: &Instance) -> Vec<ModSource> {
        self.items()
            .rev()
            .filter(|entry| entry.enabled && entry.kind == ModKind::Managed)
            .map(|entry| ModSource::new(entry.name.clone(), instance.mods_dir().join(&entry.name)))
            .collect()
    }

    /// Row index of an item by name (case-insensitive)
    pub fn item_row(&self, name: &str) -> Option<usize> {
        self.mods.iter().position(
            |row| matches!(row, ModRow::Item(item) if item.name.eq_ignore_ascii_case(name)),
        )
    }

    /// 0-based position among real mods only
    pub fn item_ordinal(&self, name: &str) -> Option<usize> {
        self.items()
            .position(|item| item.name.eq_ignore_ascii_case(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.item_row(name).is_some()
    }

    /// Add a mod at the highest priority
    pub fn add(&mut self, name: impl Into<String>, enabled: bool) -> Result<(), InstanceError> {
        let name = name.into();
        if self.contains(&name) {
            return Err(InstanceError::ModAlreadyInList(name));
        }
        self.mods.insert(
            0,
            ModRow::Item(ModEntry {
                name,
                enabled,
                kind: ModKind::Managed,
            }),
        );
        Ok(())
    }

    /// Remove a mod from the profile
    pub fn remove(&mut self, name: &str) -> Result<(), InstanceError> {
        let idx = self
            .item_row(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        self.mods.remove(idx);
        Ok(())
    }

    fn entry_mut(&mut self, name: &str) -> Result<&mut ModEntry, InstanceError> {
        let idx = self
            .item_row(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        match &mut self.mods[idx] {
            ModRow::Item(item) => Ok(item),
            ModRow::Separator(_) => unreachable!("item lookup returned a separator"),
        }
    }

    fn managed_entry_mut(&mut self, name: &str) -> Result<&mut ModEntry, InstanceError> {
        let entry = self.entry_mut(name)?;
        if entry.kind != ModKind::Managed {
            return Err(InstanceError::NotManaged(name.to_owned()));
        }
        Ok(entry)
    }

    /// Mark a mod enabled in this profile's mod list
    pub fn enable(&mut self, name: &str) -> Result<(), InstanceError> {
        self.managed_entry_mut(name)?.enabled = true;
        Ok(())
    }

    /// Mark a mod disabled in this profile's mod list
    pub fn disable(&mut self, name: &str) -> Result<(), InstanceError> {
        self.managed_entry_mut(name)?.enabled = false;
        Ok(())
    }

    /// Raise a mod's priority by one (toward the front)
    pub fn move_up(&mut self, name: &str) -> Result<(), InstanceError> {
        let idx = self
            .item_row(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        if idx > 0 {
            self.mods.swap(idx, idx - 1);
        }
        Ok(())
    }

    /// Lower a mod's priority by one (toward the back)
    pub fn move_down(&mut self, name: &str) -> Result<(), InstanceError> {
        let idx = self
            .item_row(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        if idx + 1 < self.mods.len() {
            self.mods.swap(idx, idx + 1);
        }
        Ok(())
    }

    /// Move a mod to an absolute index
    pub fn move_to(&mut self, name: &str, target: usize) -> Result<(), InstanceError> {
        let from = self
            .item_row(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        let entry = self.mods.remove(from);
        let target = target.min(self.mods.len());
        self.mods.insert(target, entry);
        Ok(())
    }

    /// Insert a separator at a positional row index
    pub fn insert_separator(
        &mut self,
        index: usize,
        display_name: &str,
    ) -> Result<(), InstanceError> {
        let name =
            validate_separator_name(display_name).map_err(InstanceError::InvalidSeparatorName)?;
        let index = index.min(self.mods.len());
        self.mods.insert(index, ModRow::Separator(name));
        Ok(())
    }

    /// Rename the separator at `index` from a user display name
    pub fn rename_separator(
        &mut self,
        index: usize,
        display_name: &str,
    ) -> Result<(), InstanceError> {
        let name =
            validate_separator_name(display_name).map_err(InstanceError::InvalidSeparatorName)?;
        let Some(ModRow::Separator(current)) = self.mods.get_mut(index) else {
            return Err(InstanceError::InvalidSeparatorName(
                "no separator at that position".to_owned(),
            ));
        };
        *current = name;
        Ok(())
    }

    /// Remove the separator at `index`, merge its members to the group above
    pub fn remove_separator(&mut self, index: usize) -> Result<(), InstanceError> {
        if !matches!(self.mods.get(index), Some(ModRow::Separator(_))) {
            return Err(InstanceError::InvalidSeparatorName(
                "no separator at that position".to_owned(),
            ));
        }
        self.mods.remove(index);
        Ok(())
    }

    /// Reconcile this profile's mod list with what's actually installed under `mods/`
    pub fn reconcile(&mut self, instance: &Instance) -> Result<bool, InstanceError> {
        let installed = instance.installed_mods()?;
        let before = self.mods.len();

        // Drop entries with no folder
        self.mods.retain(|row| match row {
            ModRow::Separator(_) => true,
            ModRow::Item(m) => {
                m.kind == ModKind::Foreign
                    || installed
                        .iter()
                        .any(|x| x.name.eq_ignore_ascii_case(&m.name))
            }
        });
        let removed = before - self.mods.len();

        // Append installed mods not already present
        let mut added = 0;
        for m in &installed {
            if !self.contains(&m.name) {
                self.mods.push(ModRow::Item(ModEntry {
                    name: m.name.clone(),
                    enabled: false,
                    kind: ModKind::Managed,
                }));
                added += 1;
            }
        }

        Ok(removed + added > 0)
    }

    /// Discover this profile's plugins and reconcile its load order in memory, without persisting
    pub fn resolve_plugins(
        &self,
        instance: &Instance,
    ) -> Result<(Vec<PluginMeta>, PluginLoadOrder), PluginError> {
        instance.ensure_mod_state_available()?;
        let discovered = discover_plugins(instance, self)?;
        let mut order = PluginLoadOrder::load(instance, &self.name)?;
        order.reconcile(&discovered);
        Ok((discovered, order))
    }

    /// Discover, reconcile, and persist this profile's load order
    pub fn commit_load_order(&self, instance: &Instance) -> Result<CommitOutcome, PluginError> {
        let (discovered, order) = self.resolve_plugins(instance)?;
        order.save(instance)?;
        Ok(CommitOutcome { discovered, order })
    }
}

/// `profiles/<p>/settings.ini` — the MO2-compatible per-profile settings file
fn settings_path(profile_dir: &Utf8Path) -> Utf8PathBuf {
    profile_dir.join("settings.ini")
}

/// Read `[General] LocalSaves` (MO2-compatible). Missing file or key means false
fn read_local_saves(profile_dir: &Utf8Path) -> Result<bool, InstanceError> {
    let Some(text) = fs::read_to_string_opt(&settings_path(profile_dir))? else {
        return Ok(false);
    };
    Ok(crate::ini::Ini::parse(&text)
        .get("General", "LocalSaves")
        .is_some_and(|v| v.eq_ignore_ascii_case("true")))
}

/// Set `[General] LocalSaves`, preserving any other MO2 keys already in the file
fn write_local_saves(profile_dir: &Utf8Path, local_saves: bool) -> Result<(), InstanceError> {
    let path = settings_path(profile_dir);
    let text = fs::read_to_string_opt(&path)?.unwrap_or_default();
    let value = if local_saves { "true" } else { "false" };
    let updated = crate::ini::set_key(&text, "General", "LocalSaves", value);
    fs::write_atomic(&path, updated.as_bytes())?;
    Ok(())
}

/// Parse item prefixes and inline separator markers from `modlist.txt`
fn parse_modlist(text: &str) -> Vec<ModRow> {
    text.lines()
        .filter_map(|line| {
            if let Some(name) = line.strip_prefix("|\tseparator\t") {
                return Some(ModRow::Separator(name.to_owned()));
            }
            let line = line.trim();
            let enabled = match line.chars().next() {
                Some('+' | '*') => true,
                Some('-') => false,
                _ => return None,
            };
            let foreign = line.starts_with('*');
            let name = line[1..].trim();
            if name.is_empty() || name.to_ascii_lowercase().ends_with("_separator") {
                return None;
            }
            let kind = if foreign {
                ModKind::Foreign
            } else {
                ModKind::Managed
            };
            Some(ModRow::Item(ModEntry {
                name: name.to_owned(),
                enabled,
                kind,
            }))
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/profile.rs"]
mod tests;
