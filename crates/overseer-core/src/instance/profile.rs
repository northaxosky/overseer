use super::error::{InstanceError, io_err};
use super::model::Instance;
use crate::deploy::ModSource;

/// One line of a profile's mod list: a mod name plus whether it's enabled
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModListEntry {
    pub name: String,
    pub enabled: bool,
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
            out.push(if entry.enabled { '+' } else { '-' });
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
}

/// Parse `modlist.txt`: `+Name` enabled, `-Name` disabled, top line = highest priority, other lines skipped
fn parse_modlist(text: &str) -> Vec<ModListEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let enabled = match line.chars().next() {
                Some('+') => true,
                Some('-') => false,
                _ => return None,
            };
            let name = line[1..].trim();
            (!name.is_empty()).then(|| ModListEntry {
                name: name.to_owned(),
                enabled,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use tempfile::TempDir;

    fn entry(name: &str, enabled: bool) -> ModListEntry {
        ModListEntry {
            name: name.to_owned(),
            enabled,
        }
    }

    fn temp_instance() -> (TempDir, Instance) {
        let dir = TempDir::new().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 path");
        let instance = Instance::new(root.join("instance"), root.join("game"));
        (dir, instance)
    }

    #[test]
    fn parses_enabled_and_disabled_markers() {
        let mods = parse_modlist("+Enabled\n-Disabled\n");
        assert_eq!(mods, vec![entry("Enabled", true), entry("Disabled", false)]);
    }

    #[test]
    fn skips_blank_comment_and_separator_lines() {
        // MO2 files contain separators and blank lines we don't model; they must be ignored.
        let text = "+A\n\n# a comment\nSomeSeparator_separator\n-B\n";
        let mods = parse_modlist(text);
        assert_eq!(mods, vec![entry("A", true), entry("B", false)]);
    }

    #[test]
    fn skips_bare_markers_with_no_name() {
        assert!(parse_modlist("+\n-\n").is_empty());
    }

    #[test]
    fn modlist_string_round_trips_through_parse() {
        let profile = Profile {
            name: "Default".to_owned(),
            mods: vec![
                entry("Alpha", true),
                entry("Beta", false),
                entry("Gamma", true),
            ],
        };
        let text = profile.to_modlist_string();
        assert_eq!(parse_modlist(&text), profile.mods);
    }

    #[test]
    fn to_modlist_string_uses_plus_minus_prefixes() {
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("On", true), entry("Off", false)],
        };
        assert_eq!(profile.to_modlist_string(), "+On\n-Off\n");
    }

    #[test]
    fn deploy_sources_reverses_to_lowest_priority_first() {
        let (_tmp, instance) = temp_instance();
        // Stored highest-priority-first; the engine wants lowest-priority-first.
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("High", true), entry("Mid", true), entry("Low", true)],
        };
        let sources = profile.deploy_sources(&instance);
        let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Low", "Mid", "High"]);
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
        let profile = Profile {
            name: "P".to_owned(),
            mods: vec![entry("CoolMod", true)],
        };
        let sources = profile.deploy_sources(&instance);
        assert_eq!(sources[0].staging_dir, instance.mods_dir().join("CoolMod"));
    }

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
            mods: vec![entry("A", true), entry("B", false)],
        };
        profile.save(&instance).expect("save");
        let loaded = Profile::load(&instance, "Default").expect("load");
        assert_eq!(loaded.mods, profile.mods);
    }

    #[test]
    fn save_creates_the_profile_directory() {
        let (_tmp, instance) = temp_instance();
        let profile = Profile {
            name: "Fresh".to_owned(),
            mods: vec![entry("X", true)],
        };
        profile.save(&instance).expect("save");
        assert!(instance.profile_dir("Fresh").join("modlist.txt").exists());
    }
}
