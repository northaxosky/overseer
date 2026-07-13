//! Tests for the profile mod list and reconciliation

use super::*;

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

use crate::test_support::{temp_instance, write_plugin};

/// A profile with the given mods, all enabled and managed, in priority order
fn profile_of(names: &[&str]) -> Profile {
    Profile {
        name: "P".to_owned(),
        mods: names.iter().map(|n| entry(n, true)).collect(),
        local_saves: false,
    }
}

fn names_of(profile: &Profile) -> Vec<&str> {
    profile.mods.iter().map(|e| e.name.as_str()).collect()
}

// --- separators ---

#[test]
fn insert_separator_adds_an_inert_separator_at_the_given_index() {
    let mut profile = profile_of(&["Top", "Bottom"]);
    profile
        .insert_separator(1, "Gameplay")
        .expect("insert separator");
    // The display name is stored in MO2's `<name>_separator` form
    assert_eq!(names_of(&profile), ["Top", "Gameplay_separator", "Bottom"]);
    let sep = &profile.mods[1];
    assert_eq!(sep.kind, ModKind::Separator);
    assert!(!sep.enabled, "a separator is never enabled/deployed");
}

#[test]
fn insert_separator_rejects_invalid_display_names() {
    let mut profile = profile_of(&["A"]);
    // Empty/whitespace, path separators, control chars, `#`/`*` leads, and a redundant suffix
    for bad in [
        "",
        "   ",
        "load/order",
        "load\\order",
        "bell\u{7}here",
        "#comment",
        "*star",
        "Zone_separator",
    ] {
        let err = profile
            .insert_separator(0, bad)
            .expect_err("invalid separator name must be rejected");
        assert!(
            matches!(err, InstanceError::InvalidSeparatorName(_)),
            "{bad:?} should be rejected"
        );
    }
    // A rejected insert never mutates the list
    assert_eq!(names_of(&profile), ["A"]);
}

#[test]
fn insert_separator_rejects_a_duplicate_name() {
    let mut profile = profile_of(&["A"]);
    profile
        .insert_separator(0, "Gameplay")
        .expect("first insert");
    let err = profile
        .insert_separator(0, "Gameplay")
        .expect_err("duplicate separator must be rejected");
    assert!(matches!(err, InstanceError::ModAlreadyInList(n) if n == "Gameplay_separator"));
}

#[test]
fn rename_separator_updates_the_stored_name() {
    let mut profile = Profile {
        name: "P".to_owned(),
        mods: vec![separator_entry("Gameplay_separator"), entry("A", true)],
        local_saves: false,
    };
    profile.rename_separator(0, "Overhauls").expect("rename");
    assert_eq!(profile.mods[0].name, "Overhauls_separator");
    assert_eq!(profile.mods[0].kind, ModKind::Separator);
}

#[test]
fn rename_separator_rejects_a_non_separator_index_and_a_colliding_name() {
    let mut profile = Profile {
        name: "P".to_owned(),
        mods: vec![
            separator_entry("Gameplay_separator"),
            separator_entry("Visuals_separator"),
            entry("A", true),
        ],
        local_saves: false,
    };
    // Index 2 is a managed mod, not a separator
    assert!(matches!(
        profile
            .rename_separator(2, "Nope")
            .expect_err("not a separator"),
        InstanceError::InvalidSeparatorName(_)
    ));
    // Renaming Gameplay -> Visuals would collide with the sibling separator
    assert!(matches!(
        profile
            .rename_separator(0, "Visuals")
            .expect_err("colliding name"),
        InstanceError::ModAlreadyInList(n) if n == "Visuals_separator"
    ));
    // Both separators are left untouched
    assert_eq!(profile.mods[0].name, "Gameplay_separator");
    assert_eq!(profile.mods[1].name, "Visuals_separator");
}

#[test]
fn remove_separator_drops_the_divider_and_keeps_its_members() {
    let mut profile = Profile {
        name: "P".to_owned(),
        mods: vec![
            entry("A", true),
            separator_entry("Gameplay_separator"),
            entry("B", true),
        ],
        local_saves: false,
    };
    profile.remove_separator(1).expect("remove separator");
    assert_eq!(names_of(&profile), ["A", "B"]);
}

#[test]
fn remove_separator_rejects_a_non_separator_index() {
    let mut profile = Profile {
        name: "P".to_owned(),
        mods: vec![separator_entry("Gameplay_separator"), entry("A", true)],
        local_saves: false,
    };
    assert!(matches!(
        profile
            .remove_separator(1)
            .expect_err("index 1 is a managed mod, not a separator"),
        InstanceError::InvalidSeparatorName(_)
    ));
    assert_eq!(names_of(&profile), ["Gameplay_separator", "A"]);
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
    // A real MO2 separator line: preserved verbatim, never a deployable mod
    let mods = parse_modlist("-Gameplay_separator\n");
    assert_eq!(mods, vec![separator_entry("Gameplay_separator")]);
    assert!(!mods[0].enabled, "a separator is never enabled/deployed");
}

#[test]
fn skips_blank_comment_and_unmarked_lines() {
    // Blank lines, comments, and lines without a +/-/* marker are not entries
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
        local_saves: false,
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
        local_saves: false,
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
        local_saves: false,
    };
    let text = profile.to_modlist_string();
    assert_eq!(text, "+Alpha\n-Gameplay_separator\n-Beta\n");
    assert_eq!(parse_modlist(&text), profile.mods);
}

// --- deploy_sources bridge ---

#[test]
fn deploy_sources_reverses_to_lowest_priority_first() {
    let (_tmp, instance) = temp_instance();
    // Stored highest-priority-first; the engine wants lowest-priority-first
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
        local_saves: false,
    };
    let sources = profile.deploy_sources(&instance);
    let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    // Only the managed mods, lowest-priority first; the separator never deploys
    assert_eq!(names, ["Low", "High"]);
}

#[test]
fn deploy_sources_excludes_foreign_mods() {
    // Foreign (game-shipped DLC/CC) entries have no `mods/` dir; including them would crash the; deploy/diagnose plan with MissingStaging on any real MO2 profile that lists DLC
    let (_tmp, instance) = temp_instance();
    let profile = Profile {
        name: "P".to_owned(),
        mods: vec![
            entry("RealMod", true),
            foreign_entry("DLC: Wasteland Workshop"),
        ],
        local_saves: false,
    };
    let names: Vec<String> = profile
        .deploy_sources(&instance)
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert_eq!(names, ["RealMod"], "foreign DLC/CC entries never deploy");
}

#[test]
fn deploy_sources_excludes_disabled_mods() {
    let (_tmp, instance) = temp_instance();
    let profile = Profile {
        name: "P".to_owned(),
        mods: vec![entry("Yes", true), entry("No", false), entry("Also", true)],
        local_saves: false,
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
fn load_existing_missing_profile_dir_errors() {
    let (_tmp, instance) = temp_instance();
    let err = Profile::load_existing(&instance, "DoesNotExist").expect_err("missing profile");
    assert!(matches!(err, InstanceError::ProfileNotFound(name) if name == "DoesNotExist"));
}

#[test]
fn load_existing_missing_modlist_yields_empty_profile() {
    let (_tmp, instance) = temp_instance();
    std::fs::create_dir_all(instance.profile_dir("Empty")).expect("mkdir");
    let profile = Profile::load_existing(&instance, "Empty").expect("load");
    assert_eq!(profile.name, "Empty");
    assert!(profile.mods.is_empty());
}

#[test]
fn save_then_load_preserves_the_mod_list() {
    let (_tmp, instance) = temp_instance();
    let profile = Profile {
        name: "Default".to_owned(),
        mods: vec![entry("A", true), entry("B", false), foreign_entry("DLC")],
        local_saves: false,
    };
    profile.save(&instance).expect("save");
    let loaded = Profile::load(&instance, "Default").expect("load");
    assert_eq!(loaded.mods, profile.mods);
}

/// `save_modlist` updates mods without rewriting persisted profile settings
#[test]
fn save_modlist_updates_mods_without_touching_settings() {
    let (_tmp, instance) = temp_instance();
    let mut profile = Profile {
        name: "Default".to_owned(),
        mods: vec![entry("A", true)],
        local_saves: true,
    };
    profile.save(&instance).expect("seed profile");
    let settings_path = instance.profile_dir("Default").join("settings.ini");
    let settings_before = std::fs::read(&settings_path).expect("read seeded settings");

    profile.mods = vec![entry("B", false)];
    profile.local_saves = false;
    profile.save_modlist(&instance).expect("save mod list");

    let loaded = Profile::load(&instance, "Default").expect("reload profile");
    assert_eq!(loaded.mods, profile.mods);
    assert!(loaded.local_saves, "the persisted setting remains true");
    assert_eq!(
        std::fs::read(settings_path).expect("read settings"),
        settings_before,
        "settings.ini remains byte-for-byte unchanged"
    );
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

#[test]
fn save_then_load_round_trips_the_local_saves_flag() {
    let (_tmp, instance) = temp_instance();

    let mut on = profile_of(&["A"]);
    on.name = "On".to_owned();
    on.local_saves = true;
    on.save(&instance).expect("save on");
    assert!(
        Profile::load(&instance, "On").expect("load on").local_saves,
        "LocalSaves=true persists across save/load"
    );

    let mut off = profile_of(&["A"]);
    off.name = "Off".to_owned();
    off.save(&instance).expect("save off");
    assert!(
        !Profile::load(&instance, "Off")
            .expect("load off")
            .local_saves,
        "LocalSaves=false persists across save/load"
    );
}

#[test]
fn local_saves_defaults_to_false_without_a_settings_ini() {
    let (_tmp, instance) = temp_instance();
    // An MO2 profile (or one saved before this flag existed) has only modlist.txt
    let dir = instance.profile_dir("Legacy");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("modlist.txt"), "+A\n").expect("seed modlist");

    let loaded = Profile::load(&instance, "Legacy").expect("load");
    assert!(
        !loaded.local_saves,
        "a missing settings.ini reads as LocalSaves off"
    );
}

#[test]
fn sync_plugins_persists_changed_order_and_returns_discovered_state() {
    let (_tmp, instance) = temp_instance();
    write_plugin(&instance.mods_dir().join("ModA"), "Patch.esp", 0, &[]);
    let profile = profile_of(&["ModA"]);

    let (discovered, order) = profile.sync_plugins(&instance).expect("sync");

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].name, "Patch.esp");
    assert_eq!(
        order.plugins,
        vec![crate::plugins::PluginEntry {
            name: "Patch.esp".to_owned(),
            active: true,
        }]
    );
    let loaded = PluginLoadOrder::load(&instance, &profile.name).expect("load");
    assert_eq!(loaded.plugins, order.plugins);
}

#[test]
fn enabling_local_saves_preserves_other_settings_keys() {
    let (_tmp, instance) = temp_instance();
    let dir = instance.profile_dir("P");
    std::fs::create_dir_all(&dir).expect("mkdir");
    // MO2 writes sibling keys into the same [General] block; they must survive
    std::fs::write(
        dir.join("settings.ini"),
        "[General]\r\nLocalSettings=true\r\nAutomaticArchiveInvalidation=false\r\n",
    )
    .expect("seed settings.ini");

    write_local_saves(&dir, true).expect("write");

    let ini =
        crate::ini::Ini::parse(&std::fs::read_to_string(dir.join("settings.ini")).expect("read"));
    assert_eq!(ini.get("General", "LocalSaves"), Some("true"));
    assert_eq!(
        ini.get("General", "LocalSettings"),
        Some("true"),
        "sibling MO2 key kept"
    );
    assert_eq!(
        ini.get("General", "AutomaticArchiveInvalidation"),
        Some("false"),
        "sibling MO2 key kept"
    );
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
        local_saves: false,
    };
    // Foreign entries always serialize as `*`, so a flip would be a lie; reject it
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
        local_saves: false,
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
    // usize::MAX means "send to the bottom"
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

/// Create empty `mods/<name>/` folders so `installed_mods()` discovers them
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
    // New mod is appended at the back (lowest priority), existing order kept
    assert_eq!(names_of(&profile), ["Existing", "BrandNew"]);
    assert!(!profile.mods[1].enabled);
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
        // Deliberately not alphabetical, with B disabled
        mods: vec![entry("C", true), entry("B", false), entry("A", true)],
        local_saves: false,
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
        local_saves: false,
    };

    let changed = profile.reconcile(&instance).expect("reconcile");
    // DLCRobot has no mods/ folder but must not be dropped
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
        local_saves: false,
    };

    let changed = profile.reconcile(&instance).expect("reconcile");
    // A separator has no mods/ folder but must survive reconcile (and the save that follows), so importing an MO2 profile and running `mod list` can't silently destroy it
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
