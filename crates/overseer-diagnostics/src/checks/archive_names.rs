//! Top-level `Data/*.ba2` names: flag archives the game won't auto load because of their name

use super::Check;
use crate::context::{ArchiveInfo, GameContext};
use crate::finding::{Finding, Severity};
use overseer_core::deploy::strip_data_prefix;
use overseer_core::ini::GameInis;
use std::collections::BTreeSet;

/// Base-game archives whose names break the auto-load rule but are legitimate, so never flagged
const NAME_WHITELIST: &[&str] = &[
    "creationkit - shaders.ba2",
    "creationkit - textures.ba2",
    "fallout4 - animations.ba2",
    "fallout4 - interface.ba2",
    "fallout4 - materials.ba2",
    "fallout4 - meshes.ba2",
    "fallout4 - meshesextra.ba2",
    "fallout4 - misc.ba2",
    "fallout4 - nvflex.ba2",
    "fallout4 - shaders.ba2",
    "fallout4 - sounds.ba2",
    "fallout4 - startup.ba2",
    "fallout4 - textures1.ba2",
    "fallout4 - textures2.ba2",
    "fallout4 - textures3.ba2",
    "fallout4 - textures4.ba2",
    "fallout4 - textures5.ba2",
    "fallout4 - textures6.ba2",
    "fallout4 - textures7.ba2",
    "fallout4 - textures8.ba2",
    "fallout4 - textures9.ba2",
    "fallout4 - texturespatch.ba2",
    "fallout4 - voices.ba2",
    "dlcultrahighresolution - textures01.ba2",
    "dlcultrahighresolution - textures02.ba2",
    "dlcultrahighresolution - textures03.ba2",
    "dlcultrahighresolution - textures04.ba2",
    "dlcultrahighresolution - textures05.ba2",
    "dlcultrahighresolution - textures06.ba2",
    "dlcultrahighresolution - textures07.ba2",
    "dlcultrahighresolution - textures08.ba2",
    "dlcultrahighresolution - textures09.ba2",
    "dlcultrahighresolution - textures10.ba2",
    "dlcultrahighresolution - textures11.ba2",
    "dlcultrahighresolution - textures12.ba2",
    "dlcultrahighresolution - textures13.ba2",
    "dlcultrahighresolution - textures14.ba2",
    "dlcultrahighresolution - textures15.ba2",
    "dlcultrahighresolution - textures16.ba2",
];

/// `[Archive]` keys whose values list archives the engine loads regardless of name
const INI_ARCHIVE_KEYS: &[&str] = &[
    "sResourceIndexFileList",
    "sResourceStartUpArchiveList",
    "sResourceArchiveList",
    "sResourceArchiveList2",
];

/// Flags top-level `Data/*.ba2` archives the engine won't auto-load because of their name
pub struct ArchiveNames;

impl Check for ArchiveNames {
    fn id(&self) -> &'static str {
        "archive-names"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let registered = ini_registered_archives(ctx.inis.as_ref());
        ctx.archives
            .iter()
            .filter(|a| is_top_level_data(a))
            .filter_map(|a| flag(a, &registered))
            .collect()
    }
}

/// True if the archive deploys directly under `Data/` (not nested)
fn is_top_level_data(archive: &ArchiveInfo) -> bool {
    strip_data_prefix(&archive.relative).is_some_and(|r| r.components().count() == 1)
}

/// Warn if a top-level archive is neither whitelisted, INI-registered, nor validly named
fn flag(archive: &ArchiveInfo, registered: &BTreeSet<String>) -> Option<Finding> {
    let lower = archive.name.to_lowercase();
    if NAME_WHITELIST.contains(&lower.as_str())
        || registered.contains(&lower)
        || name_auto_loads(&lower)
    {
        return None;
    }
    Some(Finding::new(
        Severity::Warning,
        format!("`{}` (from `{}`) won't be loaded by the game", archive.name, archive.mod_name),
        Some("Fallout 4 auto-loads an archive only when its named `<Plugin> - Main.ba2` or `<Plugin> \
         - Textures.ba2`. Rename it to match its plugin, or register it in an INI archive list.".to_owned()),
    ))
}

/// True if a lowercased `*.ba2` filename follows Fallout 4's auto-load naming convention
fn name_auto_loads(filename_lower: &str) -> bool {
    let stem = filename_lower
        .strip_suffix(".ba2")
        .unwrap_or(filename_lower);
    let Some((_, suffix)) = stem.rsplit_once(" - ") else {
        return false;
    };
    suffix == "main"
        || suffix == "textures"
        || suffix
            .strip_prefix("voices_")
            .is_some_and(|lang| !lang.is_empty())
}

/// The lowercased archive filenames registered across the `[Archive]` load lists
fn ini_registered_archives(inis: Option<&GameInis>) -> BTreeSet<String> {
    let mut registered = BTreeSet::new();
    let Some(inis) = inis else {
        return registered;
    };
    for key in INI_ARCHIVE_KEYS {
        if let Some(value) = inis.settings.get("Archive", key) {
            for name in value.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    registered.insert(name.to_lowercase());
                }
            }
        }
    }
    registered
}

// ---------------------------------------------------------------------------; Tests; ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ArchiveScan;
    use camino::Utf8Path;
    use overseer_core::ini::Ini;

    fn archive(relative: &str, mod_name: &str) -> ArchiveInfo {
        let rel = Utf8Path::new(relative);
        ArchiveInfo {
            name: rel.file_name().unwrap_or_default().to_owned(),
            mod_name: mod_name.to_owned(),
            relative: rel.to_owned(),
            // This check ignores the header scan entirely.
            scan: ArchiveScan::Invalid,
        }
    }

    fn run(archives: Vec<ArchiveInfo>) -> Vec<Finding> {
        ArchiveNames.run(&GameContext {
            archives,
            ..GameContext::default()
        })
    }

    fn run_with_ini(archives: Vec<ArchiveInfo>, settings: &str) -> Vec<Finding> {
        ArchiveNames.run(&GameContext {
            archives,
            inis: Some(GameInis {
                settings: Ini::parse(settings),
                ..GameInis::default()
            }),
            ..GameContext::default()
        })
    }

    // --- name_auto_loads (pure) ---

    #[test]
    fn recognized_suffixes_auto_load() {
        assert!(name_auto_loads("mymod - main.ba2"));
        assert!(name_auto_loads("mymod - textures.ba2"));
        assert!(name_auto_loads("mymod - voices_en.ba2"));
        assert!(name_auto_loads("mymod - voices_de.ba2"));
    }

    #[test]
    fn bad_or_missing_suffixes_do_not_auto_load() {
        assert!(!name_auto_loads("mymod - extra.ba2"));
        assert!(!name_auto_loads("randomthing.ba2"));
        // An empty language after `voices_` is not a valid suffix.
        assert!(!name_auto_loads("mymod - voices_.ba2"));
    }

    #[test]
    fn the_last_separator_decides_the_suffix() {
        // Split on the final " - ": the trailing token is what the engine keys on.
        assert!(name_auto_loads("my - cool - mod - main.ba2"));
        assert!(!name_auto_loads("my - main - mod.ba2"));
    }

    // --- run ---

    #[test]
    fn valid_names_are_silent() {
        assert!(
            run(vec![
                archive("Data/MyMod - Main.ba2", "CoolMod"),
                archive("Data/MyMod - Textures.ba2", "CoolMod"),
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_bad_suffix_warns_and_names_the_mod() {
        let findings = run(vec![archive("Data/MyMod - Extra.ba2", "Cool Mod")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("MyMod - Extra.ba2"));
        assert!(findings[0].title.contains("Cool Mod"));
    }

    #[test]
    fn a_name_with_no_separator_warns() {
        let findings = run(vec![archive("Data/RandomThing.ba2", "M")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn base_game_whitelisted_names_are_silent() {
        assert!(
            run(vec![
                archive("Data/Fallout4 - Textures1.ba2", "M"),
                archive("Data/DLCUltraHighResolution - Textures16.ba2", "M"),
                archive("Data/CreationKit - Shaders.ba2", "M"),
            ])
            .is_empty()
        );
    }

    #[test]
    fn a_name_just_past_the_whitelist_range_warns() {
        // `textures16` is whitelisted; `textures17` is not a real base archive.
        let findings = run(vec![archive(
            "Data/DLCUltraHighResolution - Textures17.ba2",
            "M",
        )]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn nested_and_root_archives_are_out_of_scope() {
        assert!(
            run(vec![
                // Nested under Data/ never auto-loads regardless of name.
                archive("Data/textures/bad.ba2", "M"),
                // Not under Data/ at all.
                archive("Root/bad.ba2", "M"),
            ])
            .is_empty()
        );
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(run(vec![archive("Data/MYMOD - MAIN.BA2", "M")]).is_empty());
        assert!(run(vec![archive("Data/FALLOUT4 - TEXTURES1.BA2", "M")]).is_empty());
    }

    #[test]
    fn an_ini_registered_archive_is_exempt() {
        let settings = "[Archive]\nsResourceArchiveList2=CustomStuff.ba2, Other - Main.ba2\n";
        assert!(run_with_ini(vec![archive("Data/CustomStuff.ba2", "M")], settings).is_empty());
    }

    #[test]
    fn an_unregistered_archive_still_warns_with_ini_present() {
        let settings = "[Archive]\nsResourceArchiveList2=SomethingElse.ba2\n";
        let findings = run_with_ini(vec![archive("Data/CustomStuff.ba2", "M")], settings);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn the_whitelist_has_exactly_thirty_nine_entries() {
        assert_eq!(NAME_WHITELIST.len(), 39);
    }
}
