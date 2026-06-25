use super::error::{InstanceError, io_err};
use super::model::Instance;
use crate::deploy::ModSource;

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

/// Profile: a named, ordered mod list.
#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub mods: Vec<ModListEntry>,
}

impl Profile {
    /// Load a profile's `modlist.txt`. A missing file is treated as an empty mod list
    pub fn load(instance: &Instance, name: &str) -> Result<Self, InstanceError> {
        let path = instance.profile_dir(name).join("modlist.txt");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io_err(&path, e)),
        };
        Ok(Self {
            name: name.to_owned(),
            mods: parse_modlist(&text),
        })
    }

    /// Write the profile's `modlist.txt` & create the profile dir if necessary
    pub fn save(&self, instance: &Instance) -> Result<(), InstanceError> {
        let dir = instance.profile_dir(&self.name);
        std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let path = dir.join("modlist.txt");
        std::fs::write(&path, self.to_modlist_string()).map_err(|e| io_err(&path, e))?;
        Ok(())
    }

    /// Serialize a mod list to `modlist.txt` text (`+`/`-` prefixes, one per line)
    pub fn to_modlist_string(&self) -> String {
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

    /// Enabled mods as deploy sources, lowest priority first
    pub fn deploy_sources(&self, instance: &Instance) -> Vec<ModSource> {
        self.mods
            .iter()
            .rev()
            .filter(|entry| entry.enabled)
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

    /// Mark a mod enabled in this profile's mod list.
    pub fn enable(&mut self, name: &str) -> Result<(), InstanceError> {
        self.managed_entry_mut(name)?.enabled = true;
        Ok(())
    }

    /// Mark a mod disabled in this profile's mod list.
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

    /// Reconcile this profile's mod list with whats actually installed under `mods/`
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
}

/// Parse `modlist.txt`: `+Name` enabled, `-Name` disabled, top line = highest priority, other lines skipped
fn parse_modlist(text: &str) -> Vec<ModListEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let enabled = match line.chars().next() {
                Some('+') | Some('*') => true,
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn entry(name: &str, enabled: bool) -> ModListEntry {
        ModListEntry {
            name: name.to_owned(),
            enabled,
            kind: ModKind::Managed,
        }
    }

    fn foreign_entry(name: &str) -> ModListEntry {
        ModListEntry {
            name: name.to_owned(),
            enabled: true,
            kind: ModKind::Foreign,
        }
    }

    fn separator_entry(name: &str) -> ModListEntry {
        ModListEntry {
            name: name.to_owned(),
            enabled: false,
            kind: ModKind::Separator,
        }
    }

    fn temp_instance() -> (TempDir, Instance) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        let instance = Instance::new(root.join("instance"), root.join("game"));
        (dir, instance)
    }

    /// A profile with the given mods, all enabled and managed, in priority order.
    fn profile_of(names: &[&str]) -> Profile {
        Profile {
            name: "P".to_owned(),
            mods: names.iter().map(|n| entry(n, true)).collect(),
        }
    }

    fn names_of(profile: &Profile) -> Vec<&str> {
        profile.mods.iter().map(|e| e.name.as_str()).collect()
    }

    // --- parsing ---

    #[test]
    fn parses_enabled_and_disabled_markers() {
        let mods = parse_modlist("+Enabled\n-Disabled\n");
        assert_eq!(mods, vec![entry("Enabled", true), entry("Disabled", false)]);
    }

    #[test]
    fn parses_asterisk_as_enabled_foreign() {
        let mods = parse_modlist("*DLCRobot\n");
        assert_eq!(mods, vec![foreign_entry("DLCRobot")]);
    }

    #[test]
    fn parses_a_separator_as_an_inert_entry() {
        // A real MO2 separator line: preserved verbatim, never a deployable mod.
        let mods = parse_modlist("-Gameplay_separator\n");
        assert_eq!(mods, vec![separator_entry("Gameplay_separator")]);
        assert!(!mods[0].enabled, "a separator is never enabled/deployed");
    }

    #[test]
    fn skips_blank_comment_and_unmarked_lines() {
        // Blank lines, comments, and lines without a +/-/* marker are not entries.
        let text = "+A\n\n# a comment\nno marker here\n-B\n";
        let mods = parse_modlist(text);
        assert_eq!(mods, vec![entry("A", true), entry("B", false)]);
    }

    #[test]
    fn skips_bare_markers_with_no_name() {
        assert!(parse_modlist("+\n-\n").is_empty());
    }

    // --- serialization ---

    #[test]
    fn to_modlist_string_uses_correct_prefixes() {
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("On", true), entry("Off", false), foreign_entry("DLC")],
        };
        assert_eq!(profile.to_modlist_string(), "+On\n-Off\n*DLC\n");
    }

    #[test]
    fn modlist_string_round_trips_through_parse() {
        let profile = Profile {
            name: "Default".to_owned(),
            mods: vec![
                entry("Alpha", true),
                entry("Beta", false),
                foreign_entry("DLCworkshop01"),
                entry("Gamma", true),
            ],
        };
        let text = profile.to_modlist_string();
        assert_eq!(parse_modlist(&text), profile.mods);
    }

    #[test]
    fn a_separator_round_trips_through_serialize_and_parse() {
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![
                entry("Alpha", true),
                separator_entry("Gameplay_separator"),
                entry("Beta", false),
            ],
        };
        let text = profile.to_modlist_string();
        assert_eq!(text, "+Alpha\n-Gameplay_separator\n-Beta\n");
        assert_eq!(parse_modlist(&text), profile.mods);
    }

    // --- deploy_sources bridge ---

    #[test]
    fn deploy_sources_reverses_to_lowest_priority_first() {
        let (_tmp, instance) = temp_instance();
        // Stored highest-priority-first; the engine wants lowest-priority-first.
        let profile = profile_of(&["High", "Mid", "Low"]);
        let sources = profile.deploy_sources(&instance);
        let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Low", "Mid", "High"]);
    }

    #[test]
    fn deploy_sources_excludes_separators() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![
                entry("High", true),
                separator_entry("Mid_separator"),
                entry("Low", true),
            ],
        };
        let sources = profile.deploy_sources(&instance);
        let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
        // Only the managed mods, lowest-priority first; the separator never deploys.
        assert_eq!(names, ["Low", "High"]);
    }

    #[test]
    fn deploy_sources_excludes_disabled_mods() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("Yes", true), entry("No", false), entry("Also", true)],
        };
        let names: Vec<String> = profile
            .deploy_sources(&instance)
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert_eq!(names, ["Also", "Yes"]);
    }

    #[test]
    fn deploy_sources_point_into_the_mods_dir() {
        let (_tmp, instance) = temp_instance();
        let profile = profile_of(&["CoolMod"]);
        let sources = profile.deploy_sources(&instance);
        assert_eq!(sources[0].staging_dir, instance.mods_dir().join("CoolMod"));
    }

    // --- load / save ---

    #[test]
    fn load_missing_modlist_yields_empty_profile() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile::load(&instance, "DoesNotExist").expect("load");
        assert_eq!(profile.name, "DoesNotExist");
        assert!(profile.mods.is_empty());
    }

    #[test]
    fn save_then_load_preserves_the_mod_list() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile {
            name: "Default".to_owned(),
            mods: vec![entry("A", true), entry("B", false), foreign_entry("DLC")],
        };
        profile.save(&instance).expect("save");
        let loaded = Profile::load(&instance, "Default").expect("load");
        assert_eq!(loaded.mods, profile.mods);
    }

    #[test]
    fn save_creates_the_profile_directory() {
        let (_tmp, instance) = temp_instance();
        let profile = profile_of(&["X"]);
        let profile = Profile {
            name: "Fresh".to_owned(),
            ..profile
        };
        profile.save(&instance).expect("save");
        assert!(instance.profile_dir("Fresh").join("modlist.txt").exists());
    }

    // --- lookup ---

    #[test]
    fn position_and_contains_are_case_insensitive() {
        let profile = profile_of(&["MyMod", "Other"]);
        assert_eq!(profile.position("mymod"), Some(0));
        assert_eq!(profile.position("OTHER"), Some(1));
        assert_eq!(profile.position("missing"), None);
        assert!(profile.contains("mYmOd"));
        assert!(!profile.contains("nope"));
    }

    // --- add / remove ---

    #[test]
    fn add_inserts_at_highest_priority() {
        let mut profile = profile_of(&["Existing"]);
        profile.add("Newcomer", true).expect("add");
        assert_eq!(names_of(&profile), ["Newcomer", "Existing"]);
        assert_eq!(profile.mods[0].kind, ModKind::Managed);
    }

    #[test]
    fn add_rejects_duplicate() {
        let mut profile = profile_of(&["Dup"]);
        let err = profile.add("dup", true).expect_err("should reject");
        assert!(matches!(err, InstanceError::ModAlreadyInList(n) if n == "dup"));
    }

    #[test]
    fn remove_deletes_the_mod() {
        let mut profile = profile_of(&["A", "B", "C"]);
        profile.remove("b").expect("remove");
        assert_eq!(names_of(&profile), ["A", "C"]);
    }

    #[test]
    fn remove_missing_is_an_error() {
        let mut profile = profile_of(&["A"]);
        let err = profile.remove("ghost").expect_err("should error");
        assert!(matches!(err, InstanceError::ModNotInList(n) if n == "ghost"));
    }

    // --- enable / disable ---

    #[test]
    fn enable_and_disable_toggle_state() {
        let mut profile = profile_of(&["M"]);
        profile.disable("m").expect("disable");
        assert!(!profile.mods[0].enabled);
        profile.enable("M").expect("enable");
        assert!(profile.mods[0].enabled);
    }

    #[test]
    fn enable_missing_is_an_error() {
        let mut profile = profile_of(&["M"]);
        assert!(matches!(
            profile.enable("x").expect_err("err"),
            InstanceError::ModNotInList(_)
        ));
    }

    #[test]
    fn disabling_a_foreign_entry_is_rejected_not_a_silent_noop() {
        let mut profile = Profile {
            name: "P".to_owned(),
            mods: vec![foreign_entry("DLCRobot")],
        };
        // Foreign entries always serialize as `*`, so a flip would be a lie; reject it.
        assert!(matches!(
            profile.disable("DLCRobot").expect_err("err"),
            InstanceError::NotManaged(_)
        ));
        assert!(profile.mods[0].enabled, "the entry is left untouched");
    }

    #[test]
    fn toggling_a_separator_is_rejected() {
        let mut profile = Profile {
            name: "P".to_owned(),
            mods: vec![separator_entry("Gameplay_separator")],
        };
        assert!(matches!(
            profile.enable("Gameplay_separator").expect_err("err"),
            InstanceError::NotManaged(_)
        ));
        assert!(!profile.mods[0].enabled, "the separator stays inert");
    }

    // --- reorder ---

    #[test]
    fn move_up_raises_priority() {
        let mut profile = profile_of(&["A", "B", "C"]);
        profile.move_up("C").expect("move_up");
        assert_eq!(names_of(&profile), ["A", "C", "B"]);
    }

    #[test]
    fn move_up_at_top_is_a_noop() {
        let mut profile = profile_of(&["A", "B"]);
        profile.move_up("A").expect("move_up");
        assert_eq!(names_of(&profile), ["A", "B"]);
    }

    #[test]
    fn move_down_lowers_priority() {
        let mut profile = profile_of(&["A", "B", "C"]);
        profile.move_down("A").expect("move_down");
        assert_eq!(names_of(&profile), ["B", "A", "C"]);
    }

    #[test]
    fn move_down_at_bottom_is_a_noop() {
        let mut profile = profile_of(&["A", "B"]);
        profile.move_down("B").expect("move_down");
        assert_eq!(names_of(&profile), ["A", "B"]);
    }

    #[test]
    fn move_to_relocates_to_absolute_index() {
        let mut profile = profile_of(&["A", "B", "C", "D"]);
        profile.move_to("D", 1).expect("move_to");
        assert_eq!(names_of(&profile), ["A", "D", "B", "C"]);
    }

    #[test]
    fn move_to_clamps_target_to_the_end() {
        let mut profile = profile_of(&["A", "B", "C"]);
        // usize::MAX means "send to the bottom".
        profile.move_to("A", usize::MAX).expect("move_to");
        assert_eq!(names_of(&profile), ["B", "C", "A"]);
    }

    #[test]
    fn move_to_top_raises_to_highest() {
        let mut profile = profile_of(&["A", "B", "C"]);
        profile.move_to("C", 0).expect("move_to");
        assert_eq!(names_of(&profile), ["C", "A", "B"]);
    }

    #[test]
    fn move_to_missing_is_an_error() {
        let mut profile = profile_of(&["A"]);
        assert!(matches!(
            profile.move_to("ghost", 0).expect_err("err"),
            InstanceError::ModNotInList(_)
        ));
    }

    // --- reconcile ---

    /// Create empty `mods/<name>/` folders so `installed_mods()` discovers them.
    fn install_dirs(instance: &Instance, names: &[&str]) {
        for name in names {
            std::fs::create_dir_all(instance.mods_dir().join(name)).expect("mkdir");
        }
    }

    #[test]
    fn reconcile_appends_newly_installed_at_lowest_priority() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["Existing", "BrandNew"]);
        let mut profile = profile_of(&["Existing"]);

        let changed = profile.reconcile(&instance).expect("reconcile");
        assert!(changed);
        // New mod is appended at the back (lowest priority), existing order kept.
        assert_eq!(names_of(&profile), ["Existing", "BrandNew"]);
        assert!(profile.mods[1].enabled);
    }

    #[test]
    fn reconcile_drops_uninstalled_mods() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["Kept"]);
        let mut profile = profile_of(&["Kept", "Gone"]);

        let changed = profile.reconcile(&instance).expect("reconcile");
        assert!(changed);
        assert_eq!(names_of(&profile), ["Kept"]);
    }

    #[test]
    fn reconcile_preserves_existing_order_and_enabled_state() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["A", "B", "C"]);
        let mut profile = Profile {
            name: "P".to_owned(),
            // Deliberately not alphabetical, with B disabled.
            mods: vec![entry("C", true), entry("B", false), entry("A", true)],
        };

        let changed = profile.reconcile(&instance).expect("reconcile");
        assert!(!changed, "everything already present, nothing to do");
        assert_eq!(names_of(&profile), ["C", "B", "A"]);
        assert!(!profile.mods[1].enabled, "B stays disabled");
    }

    #[test]
    fn reconcile_keeps_foreign_mods_without_a_folder() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["Managed"]);
        let mut profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("Managed", true), foreign_entry("DLCRobot")],
        };

        let changed = profile.reconcile(&instance).expect("reconcile");
        // DLCRobot has no mods/ folder but must not be dropped.
        assert!(!changed);
        assert!(profile.contains("DLCRobot"));
    }

    #[test]
    fn reconcile_keeps_a_separator_without_a_folder() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["Managed"]);
        let mut profile = Profile {
            name: "P".to_owned(),
            mods: vec![
                separator_entry("Gameplay_separator"),
                entry("Managed", true),
            ],
        };

        let changed = profile.reconcile(&instance).expect("reconcile");
        // A separator has no mods/ folder but must survive reconcile (and the save that follows),
        // so importing an MO2 profile and running `mod list` can't silently destroy it.
        assert!(!changed, "a separator is not a change to reconcile away");
        assert!(
            profile.mods.iter().any(|e| e.kind == ModKind::Separator),
            "the separator is preserved"
        );
    }

    #[test]
    fn reconcile_reports_no_change_when_in_sync() {
        let (_tmp, instance) = temp_instance();
        install_dirs(&instance, &["A", "B"]);
        let mut profile = profile_of(&["A", "B"]);
        assert!(!profile.reconcile(&instance).expect("reconcile"));
    }
}
