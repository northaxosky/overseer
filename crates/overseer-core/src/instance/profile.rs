//! A profile's mod list: `modlist.txt` ordering, separators, and reconciliation

use super::error::InstanceError;
use super::model::Instance;
use crate::deploy::ModSource;
use crate::fs;
use crate::plugins::{PluginError, PluginLoadOrder, PluginMeta, discover_plugins};
use camino::{Utf8Path, Utf8PathBuf};

/// What kind of `modlist.txt` line an entry is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKind {
    /// A mod Overseer manages, deployed from `mods/<name>/`
    Managed,
    /// A game-shipped/foreign plugin (DLC, CC) managed elsewhere; always active
    Foreign,
    /// An MO2 separator: visual divider, never deployed
    Separator,
}

/// One line of a profile's mod list: a mod name plus whether it's enabled
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModListEntry {
    pub name: String,
    pub enabled: bool,
    pub kind: ModKind,
}

/// Profile: a named, ordered mod list
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub mods: Vec<ModListEntry>,
    pub local_saves: bool,
}

impl Profile {
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
    pub(crate) fn save_modlist(&self, instance: &Instance) -> Result<(), InstanceError> {
        let dir = instance.profile_dir(&self.name);
        fs::write_atomic(
            &dir.join("modlist.txt"),
            self.to_modlist_string().as_bytes(),
        )?;
        Ok(())
    }

    /// Serialize a mod list to `modlist.txt` text (`+`/`-` prefixes, one per line)
    pub(crate) fn to_modlist_string(&self) -> String {
        let mut out = String::new();
        for entry in &self.mods {
            out.push(match entry.kind {
                ModKind::Foreign => '*',
                _ if entry.enabled => '+',
                _ => '-',
            });
            out.push_str(&entry.name);
            out.push('\n');
        }
        out
    }

    /// Enabled *managed* mods as deploy sources, lowest priority first (foreign/separator entries have no `mods/` dir)
    pub fn deploy_sources(&self, instance: &Instance) -> Vec<ModSource> {
        self.mods
            .iter()
            .rev()
            .filter(|entry| entry.enabled && entry.kind == ModKind::Managed)
            .map(|entry| ModSource::new(entry.name.clone(), instance.mods_dir().join(&entry.name)))
            .collect()
    }

    /// Index of a mod by name (case-insensitive)
    pub fn position(&self, name: &str) -> Option<usize> {
        self.mods
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.position(name).is_some()
    }

    /// Add a mod at the highest priority
    pub fn add(&mut self, name: impl Into<String>, enabled: bool) -> Result<(), InstanceError> {
        let name = name.into();
        if self.contains(&name) {
            return Err(InstanceError::ModAlreadyInList(name));
        }
        self.mods.insert(
            0,
            ModListEntry {
                name,
                enabled,
                kind: ModKind::Managed,
            },
        );
        Ok(())
    }

    /// Remove a mod from the profile
    pub fn remove(&mut self, name: &str) -> Result<(), InstanceError> {
        let idx = self
            .position(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        self.mods.remove(idx);
        Ok(())
    }

    fn entry_mut(&mut self, name: &str) -> Result<&mut ModListEntry, InstanceError> {
        let idx = self
            .position(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        Ok(&mut self.mods[idx])
    }

    fn managed_entry_mut(&mut self, name: &str) -> Result<&mut ModListEntry, InstanceError> {
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
            .position(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        if idx > 0 {
            self.mods.swap(idx, idx - 1);
        }
        Ok(())
    }

    /// Lower a mod's priority by one (toward the back)
    pub fn move_down(&mut self, name: &str) -> Result<(), InstanceError> {
        let idx = self
            .position(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        if idx + 1 < self.mods.len() {
            self.mods.swap(idx, idx + 1);
        }
        Ok(())
    }

    /// Move a mod to an absolute index
    pub fn move_to(&mut self, name: &str, target: usize) -> Result<(), InstanceError> {
        let from = self
            .position(name)
            .ok_or_else(|| InstanceError::ModNotInList(name.to_owned()))?;
        let entry = self.mods.remove(from);
        let target = target.min(self.mods.len());
        self.mods.insert(target, entry);
        Ok(())
    }

    /// Insert a separator (`_separator` divider) at `index`
    pub fn insert_separator(
        &mut self,
        index: usize,
        display_name: &str,
    ) -> Result<(), InstanceError> {
        let name = Self::separator_name(display_name)?;
        if self.contains(&name) {
            return Err(InstanceError::ModAlreadyInList(name));
        }
        let index = index.min(self.mods.len());
        self.mods.insert(
            index,
            ModListEntry {
                name,
                enabled: false,
                kind: ModKind::Separator,
            },
        );
        Ok(())
    }

    /// Rename the separator at `index` from a user display name
    pub fn rename_separator(
        &mut self,
        index: usize,
        display_name: &str,
    ) -> Result<(), InstanceError> {
        self.ensure_separator(index)?;
        let name = Self::separator_name(display_name)?;
        if self
            .mods
            .iter()
            .enumerate()
            .any(|(i, m)| i != index && m.name.eq_ignore_ascii_case(&name))
        {
            return Err(InstanceError::ModAlreadyInList(name));
        }
        self.mods[index].name = name;
        Ok(())
    }

    /// Confirm `index` points at a separator entry
    fn ensure_separator(&self, index: usize) -> Result<(), InstanceError> {
        match self.mods.get(index) {
            Some(e) if e.kind == ModKind::Separator => Ok(()),
            _ => Err(InstanceError::InvalidSeparatorName(
                "no separator at that position".to_owned(),
            )),
        }
    }

    /// Validate a user separator display name and return its stored `<name>_separator` form
    fn separator_name(display_name: &str) -> Result<String, InstanceError> {
        let name = display_name.trim();
        let reject = |why: &str| Err(InstanceError::InvalidSeparatorName(why.to_owned()));
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
        if name.to_ascii_lowercase().ends_with("_separator") {
            return reject("name cannot end with `_separator`");
        }
        Ok(format!("{name}_separator"))
    }

    /// Reconcile this profile's mod list with what's actually installed under `mods/`
    pub fn reconcile(&mut self, instance: &Instance) -> Result<bool, InstanceError> {
        let installed = instance.installed_mods()?;
        let before = self.mods.len();

        // Drop entries with no folder
        self.mods.retain(|e| {
            e.kind != ModKind::Managed
                || installed
                    .iter()
                    .any(|m| m.name.eq_ignore_ascii_case(&e.name))
        });
        let removed = before - self.mods.len();

        // Append installed mods not already present
        let mut added = 0;
        for m in &installed {
            if !self.contains(&m.name) {
                self.mods.push(ModListEntry {
                    name: m.name.clone(),
                    enabled: true,
                    kind: ModKind::Managed,
                });
                added += 1;
            }
        }

        Ok(removed + added > 0)
    }

    /// Discover this profile's plugins, reconcile its load order, and persist changes
    pub fn sync_plugins(
        &self,
        instance: &Instance,
    ) -> Result<(Vec<PluginMeta>, PluginLoadOrder), PluginError> {
        let discovered = discover_plugins(instance, self)?;
        let mut order = PluginLoadOrder::load(instance, &self.name)?;
        if order.reconcile(&discovered) {
            order.save(instance)?;
        }
        Ok((discovered, order))
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

/// Parse `modlist.txt`: `+Name` enabled, `-Name` disabled, top line = highest priority, other lines skipped
fn parse_modlist(text: &str) -> Vec<ModListEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let enabled = match line.chars().next() {
                Some('+' | '*') => true,
                Some('-') => false,
                _ => return None,
            };
            let foreign = line.starts_with('*');
            let name = line[1..].trim();
            if name.is_empty() {
                return None;
            }
            let kind = if name.ends_with("_separator") {
                ModKind::Separator
            } else if foreign {
                ModKind::Foreign
            } else {
                ModKind::Managed
            };
            // separators never deploy
            let enabled = enabled && kind != ModKind::Separator;
            Some(ModListEntry {
                name: name.to_owned(),
                enabled,
                kind,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/profile.rs"]
mod tests;
