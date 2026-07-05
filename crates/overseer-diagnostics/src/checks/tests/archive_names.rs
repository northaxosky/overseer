//! Tests for the archive-name auto-load check

use super::*;
use crate::context::ArchiveScan;
use crate::finding::Severity;
use camino::Utf8Path;
use overseer_core::ini::Ini;

fn archive(relative: &str, mod_name: &str) -> ArchiveInfo {
    let rel = Utf8Path::new(relative);
    ArchiveInfo {
        name: rel.file_name().unwrap_or_default().to_owned(),
        mod_name: mod_name.to_owned(),
        relative: rel.to_owned(),
        // This check ignores the header scan entirely
        scan: ArchiveScan::Invalid,
    }
}

fn run(archives: Vec<ArchiveInfo>) -> Vec<Finding> {
    super::run(&GameContext {
        archives,
        ..GameContext::default()
    })
}

fn run_with_ini(archives: Vec<ArchiveInfo>, settings: &str) -> Vec<Finding> {
    super::run(&GameContext {
        archives,
        inis: Some(GameInis {
            settings: Ini::parse(settings),
            ..GameInis::default()
        }),
        ..GameContext::default()
    })
}

/// Assert a run produced only the clean-bill Info line
fn assert_clean(findings: Vec<Finding>) {
    assert_eq!(findings.len(), 1, "expected only the clean-bill Info");
    assert_eq!(findings[0].severity, Severity::Info);
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
    // An empty language after `voices_` is not a valid suffix
    assert!(!name_auto_loads("mymod - voices_.ba2"));
}

#[test]
fn the_last_separator_decides_the_suffix() {
    // Split on the final " - ": the trailing token is what the engine keys on
    assert!(name_auto_loads("my - cool - mod - main.ba2"));
    assert!(!name_auto_loads("my - main - mod.ba2"));
}

// --- run ---

#[test]
fn valid_names_report_a_clean_info() {
    assert_clean(run(vec![
        archive("Data/MyMod - Main.ba2", "CoolMod"),
        archive("Data/MyMod - Textures.ba2", "CoolMod"),
    ]));
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
fn base_game_whitelisted_names_report_a_clean_info() {
    assert_clean(run(vec![
        archive("Data/Fallout4 - Textures1.ba2", "M"),
        archive("Data/DLCUltraHighResolution - Textures16.ba2", "M"),
        archive("Data/CreationKit - Shaders.ba2", "M"),
    ]));
}

#[test]
fn a_name_just_past_the_whitelist_range_warns() {
    // `textures16` is whitelisted; `textures17` is not a real base archive
    let findings = run(vec![archive(
        "Data/DLCUltraHighResolution - Textures17.ba2",
        "M",
    )]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
}

#[test]
fn nested_and_root_archives_are_out_of_scope() {
    assert_clean(run(vec![
        // Nested under Data/ never auto-loads regardless of name
        archive("Data/textures/bad.ba2", "M"),
        // Not under Data/ at all
        archive("Root/bad.ba2", "M"),
    ]));
}

#[test]
fn matching_is_case_insensitive() {
    assert_clean(run(vec![archive("Data/MYMOD - MAIN.BA2", "M")]));
    assert_clean(run(vec![archive("Data/FALLOUT4 - TEXTURES1.BA2", "M")]));
}

#[test]
fn an_ini_registered_archive_is_exempt() {
    let settings = "[Archive]\nsResourceArchiveList2=CustomStuff.ba2, Other - Main.ba2\n";
    assert_clean(run_with_ini(
        vec![archive("Data/CustomStuff.ba2", "M")],
        settings,
    ));
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
