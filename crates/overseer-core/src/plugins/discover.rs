//! Discovering the plugins a profile's enabled mods provide

use super::error::{PluginError, io_err};
use super::metadata::{PluginMeta, is_plugin_file, read_metadata};
use crate::instance::{Instance, Profile};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Discover the plugins a profile would deploy
pub fn discover_plugins(
    instance: &Instance,
    profile: &Profile,
) -> Result<Vec<PluginMeta>, PluginError> {
    let mut seen: Vec<String> = Vec::new();
    let mut plugins: Vec<PluginMeta> = Vec::new();
    let game_id = instance.config.game.plugin_id();

    for entry in &profile.mods {
        if !entry.enabled {
            continue;
        }
        let mod_dir = instance.mods_dir().join(&entry.name);
        for found in find_plugin_files(&mod_dir)? {
            let name = found
                .file_name()
                .expect("Walked plugin file always has a name")
                .to_owned();

            if seen.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                continue;
            }
            seen.push(name.clone());
            plugins.push(read_metadata(game_id, &name, &found)?);
        }
    }

    Ok(plugins)
}

/// Plugin files (`.esp`/`.esm`/`.esl`) directly under a directory; a missing directory yields an empty list
fn find_plugin_files(dir: &Utf8Path) -> Result<Vec<camino::Utf8PathBuf>, PluginError> {
    let mut found = Vec::new();
    for entry in WalkDir::new(dir).min_depth(1).max_depth(1) {
        let entry = match entry {
            Ok(e) => e,
            // A directory that doesn't exist yet (no plugins)
            Err(e)
                if e.io_error().map(std::io::Error::kind) == Some(std::io::ErrorKind::NotFound) =>
            {
                return Ok(found);
            }
            Err(e) => {
                return Err(io_err(dir, e.into()).into());
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = Utf8Path::from_path(entry.path()).ok_or_else(|| {
            io_err(
                dir,
                std::io::Error::new(std::io::ErrorKind::InvalidData, "non-UTF-8 path"),
            )
        })?;
        if let Some(name) = path.file_name()
            && is_plugin_file(name)
        {
            found.push(path.to_owned());
        }
    }
    // WalkDir yields filesystem order; sort so a mod's plugins deploy deterministically
    found.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_ascii_lowercase())
    });
    Ok(found)
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{ModKind, ModListEntry, Profile};
    use crate::test_support::{FLAG_MASTER, temp_instance, write_plugin};

    fn entry(name: &str, enabled: bool) -> ModListEntry {
        ModListEntry {
            name: name.to_owned(),
            enabled,
            kind: ModKind::Managed,
        }
    }

    fn profile(mods: Vec<ModListEntry>) -> Profile {
        Profile {
            name: "P".to_owned(),
            mods,
            local_saves: false,
        }
    }

    fn names(plugins: &[PluginMeta]) -> Vec<&str> {
        plugins.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn discovers_plugins_from_enabled_mods_in_priority_order() {
        let (_t, instance) = temp_instance();
        write_plugin(&instance.mods_dir().join("ModA"), "Alpha.esp", 0, &[]);
        write_plugin(&instance.mods_dir().join("ModB"), "Beta.esp", 0, &[]);
        let profile = profile(vec![entry("ModA", true), entry("ModB", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["Alpha.esp", "Beta.esp"]);
    }

    #[test]
    fn skips_disabled_mods() {
        let (_t, instance) = temp_instance();
        write_plugin(&instance.mods_dir().join("On"), "On.esp", 0, &[]);
        write_plugin(&instance.mods_dir().join("Off"), "Off.esp", 0, &[]);
        let profile = profile(vec![entry("On", true), entry("Off", false)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["On.esp"]);
    }

    #[test]
    fn higher_priority_mod_wins_a_plugin_name_conflict() {
        let (_t, instance) = temp_instance();
        // Both mods provide Shared.esp; the higher-priority one (ModA, listed first) is a master, the lower-priority one is not — we must read the winner's metadata
        write_plugin(
            &instance.mods_dir().join("ModA"),
            "Shared.esp",
            FLAG_MASTER,
            &[],
        );
        write_plugin(&instance.mods_dir().join("ModB"), "Shared.esp", 0, &[]);
        let profile = profile(vec![entry("ModA", true), entry("ModB", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(found.len(), 1, "the shared plugin collapses to one");
        assert!(
            found[0].is_master,
            "the winning (higher-priority) copy is the master"
        );
    }

    #[test]
    fn only_top_level_plugins_are_discovered() {
        let (_t, instance) = temp_instance();
        let mod_dir = instance.mods_dir().join("ModA");
        write_plugin(&mod_dir, "Top.esp", 0, &[]);
        // A plugin buried in a subdirectory is loose data, not a loadable plugin
        write_plugin(&mod_dir.join("Meshes"), "Nested.esp", 0, &[]);
        let profile = profile(vec![entry("ModA", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["Top.esp"]);
    }

    #[test]
    fn ignores_non_plugin_files() {
        let (_t, instance) = temp_instance();
        let mod_dir = instance.mods_dir().join("ModA");
        write_plugin(&mod_dir, "Real.esp", 0, &[]);
        std::fs::write(mod_dir.join("Textures.ba2"), b"archive").unwrap();
        std::fs::write(mod_dir.join("readme.txt"), b"hi").unwrap();
        let profile = profile(vec![entry("ModA", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["Real.esp"]);
    }

    #[test]
    fn missing_mod_folder_contributes_nothing() {
        let (_t, instance) = temp_instance();
        write_plugin(&instance.mods_dir().join("Present"), "Here.esp", 0, &[]);
        // "Absent" is in the list but was never installed (no folder)
        let profile = profile(vec![entry("Absent", true), entry("Present", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["Here.esp"]);
    }

    #[test]
    fn carries_metadata_through() {
        let (_t, instance) = temp_instance();
        write_plugin(
            &instance.mods_dir().join("ModA"),
            "Dep.esp",
            0,
            &["Fallout4.esm"],
        );
        let profile = profile(vec![entry("ModA", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(found[0].masters, ["Fallout4.esm"]);
    }

    #[test]
    fn plugins_within_a_mod_are_sorted_deterministically() {
        let (_t, instance) = temp_instance();
        // Written out of order; discovery must return them name-sorted, not in FS order
        write_plugin(&instance.mods_dir().join("ModA"), "Zeta.esp", 0, &[]);
        write_plugin(&instance.mods_dir().join("ModA"), "alpha.esp", 0, &[]);
        let profile = profile(vec![entry("ModA", true)]);

        let found = discover_plugins(&instance, &profile).expect("discover");
        assert_eq!(names(&found), ["alpha.esp", "Zeta.esp"]);
    }

    #[test]
    fn empty_profile_discovers_nothing() {
        let (_t, instance) = temp_instance();
        let found = discover_plugins(&instance, &profile(vec![])).expect("discover");
        assert!(found.is_empty());
    }
}
