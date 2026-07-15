//! Tests for the shared setup context gathered once per run

use super::*;
use overseer_core::deploy::ModSource;
use overseer_core::game::GameKind;
use overseer_core::test_support::{
    FLAG_MASTER, ba2_bytes, install_plugin, save_profile, temp as temp_base, write_plugin,
};
use tempfile::TempDir;

fn active_set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|n| n.to_lowercase()).collect()
}

fn meta(name: &str) -> PluginMeta {
    overseer_core::test_support::plugin_meta(name, false, false, &[])
}

// --- scan_dlc_consistency (disk) ---

#[test]
fn dlc_survey_flags_off_revision_files_in_an_owned_group() {
    let (_tmp, root) = temp_base();
    let data = root.join("Data");
    std::fs::create_dir_all(&data).unwrap();
    // The sentinel makes DLCworkshop02 owned; neither file is the revision identity
    std::fs::write(data.join("DLCworkshop02.esm"), b"not the corrected master").unwrap();
    std::fs::write(data.join("DLCworkshop02 - Textures.ba2"), b"tiny").unwrap();

    let survey = scan_dlc_consistency(&root);
    let group = survey
        .iter()
        .find(|g| g.group == "DLCworkshop02")
        .expect("owned group surveyed");
    // The master fails the fingerprint check; the archive fails the size check
    assert!(group.off_revision.contains(&"Data/DLCworkshop02.esm"));
    assert!(
        group
            .off_revision
            .contains(&"Data/DLCworkshop02 - Textures.ba2")
    );
}

#[test]
fn dlc_survey_skips_a_group_whose_sentinel_is_absent() {
    let (_tmp, root) = temp_base();
    std::fs::create_dir_all(root.join("Data")).unwrap();
    // No DLCworkshop02.esm → the group isn't owned → not surveyed
    assert!(
        scan_dlc_consistency(&root)
            .iter()
            .all(|g| g.group != "DLCworkshop02")
    );
}

#[test]
fn dlc_survey_reports_a_missing_companion_file() {
    let (_tmp, root) = temp_base();
    let data = root.join("Data");
    std::fs::create_dir_all(&data).unwrap();
    // Sentinel present (group owned), but the textures archive is absent
    std::fs::write(data.join("DLCworkshop02.esm"), b"present but off-revision").unwrap();
    let survey = scan_dlc_consistency(&root);
    let group = survey
        .iter()
        .find(|g| g.group == "DLCworkshop02")
        .expect("owned group surveyed");
    assert!(group.missing.contains(&"Data/DLCworkshop02 - Textures.ba2"));
}

// --- active_plugins_name (pure) ---

#[test]
fn names_a_top_level_active_plugin() {
    let active = active_set(&["foo.esp"]);
    assert_eq!(
        active_plugins_name(Utf8Path::new("Data/Foo.esp"), &active),
        Some("Foo.esp")
    );
}

#[test]
fn rejects_inactive_nested_and_non_data_paths() {
    let active = active_set(&["foo.esp"]);
    // Not in the active set
    assert_eq!(
        active_plugins_name(Utf8Path::new("Data/Bar.esp"), &active),
        None
    );
    // Deeper than Data/<plugin>
    assert_eq!(
        active_plugins_name(Utf8Path::new("Data/meshes/Foo.esp"), &active),
        None
    );
    // Not under Data/
    assert_eq!(active_plugins_name(Utf8Path::new("Foo.esp"), &active), None);
}

#[test]
fn folder_and_name_match_case_insensitively() {
    let active = active_set(&["foo.esp"]);
    assert_eq!(
        active_plugins_name(Utf8Path::new("data/FOO.ESP"), &active),
        Some("FOO.ESP")
    );
}

// --- scan_sadd (real temp-dir plan) ---

#[test]
fn counts_markers_only_in_active_top_level_plugins() {
    let (_tmp, base) = temp_base();
    let mod_dir = base.join("mods/A");
    std::fs::create_dir_all(mod_dir.join("meshes")).unwrap();
    // Two markers in the active plugin; markers elsewhere must be ignored
    std::fs::write(mod_dir.join("Active.esp"), b"--\x00SADD--\x00SADD--").unwrap();
    std::fs::write(mod_dir.join("Inactive.esp"), b"\x00SADD").unwrap();
    std::fs::write(mod_dir.join("meshes/anim.nif"), b"\x00SADD").unwrap();

    let plan =
        DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)]).unwrap();

    let records = scan_sadd(&plan, &[meta("Active.esp")]);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].plugin, "Active.esp");
    assert_eq!(records[0].count, 2);
}

#[test]
fn a_plugin_without_markers_is_omitted() {
    let (_tmp, base) = temp_base();
    let mod_dir = base.join("mods/A");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(mod_dir.join("Clean.esp"), b"no markers here").unwrap();

    let plan =
        DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)]).unwrap();

    assert!(scan_sadd(&plan, &[meta("Clean.esp")]).is_empty());
}

// --- script overrides (provenance: the F4SE package vs. other mods) ---

#[test]
fn base_script_pex_name_accepts_only_top_level_base_scripts() {
    let ok = |p| base_script_pex_name(Utf8Path::new(p));
    assert_eq!(ok("Data/Scripts/Actor.pex"), Some("Actor.pex"));
    // Folder + extension match case-insensitively; the returned name keeps its casing
    assert_eq!(ok("Data/scripts/ACTOR.PEX"), Some("ACTOR.PEX"));
    // Not one of the base script names
    assert_eq!(ok("Data/Scripts/MyCustom.pex"), None);
    // Nested below Scripts/ — the engine path differs, out of scope
    assert_eq!(ok("Data/Scripts/source/Actor.pex"), None);
    // A base name but not under Scripts/, or not under Data/ at all
    assert_eq!(ok("Data/Actor.pex"), None);
    assert_eq!(ok("Root/Actor.pex"), None);
}

#[test]
fn the_base_script_list_has_twenty_nine_unique_entries() {
    let names: BTreeSet<&str> = BASE_SCRIPT_NAMES.iter().copied().collect();
    assert_eq!(names.len(), 29);
}

#[test]
fn dominant_provider_picks_the_biggest_supplier() {
    assert_eq!(dominant_provider(&[]), None);
    assert_eq!(
        dominant_provider(&[
            ("actor.pex", "F4SE"),
            ("game.pex", "F4SE"),
            ("form.pex", "Other"),
        ]),
        Some("F4SE")
    );
    // A tie has no clear F4SE package, so no mod is treated as the provider
    assert_eq!(
        dominant_provider(&[("actor.pex", "A"), ("game.pex", "B")]),
        None
    );
}

fn write_file(path: &Utf8Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn the_f4se_package_alone_reports_no_overrides() {
    // The mod that ships the base scripts is the F4SE package — its own scripts are not; overrides, whatever their bytes (this is the AE / newer-F4SE case that must stay silent)
    let (_tmp, base) = temp_base();
    let f4se = base.join("mods/F4SE");
    write_file(&f4se.join("Scripts/Actor.pex"), b"ae bytes");
    write_file(&f4se.join("Scripts/Game.pex"), b"ae bytes");
    write_file(&f4se.join("Scripts/Form.pex"), b"ae bytes");

    let plan =
        DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("F4SE", &f4se)]).unwrap();
    assert!(scan_script_overrides(&plan).is_empty());
}

/// Without a deployed F4SE/Plugins/*.dll, the Address Library requirement does not apply
#[test]
fn address_library_is_not_applicable_without_a_deployed_f4se_plugin() {
    let version = overseer_core::detect::ExeVersion {
        major: 1,
        minor: 10,
        patch: 163,
        build: 0,
    };
    let files = vec![DataFile {
        path: Utf8PathBuf::from("textures/armor.dds"),
        mod_name: "ArmorMod".to_owned(),
    }];
    assert!(matches!(
        address_library_status(&files, Some(version), "F4SE/Plugins"),
        AddressLibraryStatus::NotApplicable
    ));
}

/// A deployed plugin needs the Address Library, but an undetectable runtime can't name the expected bin
#[test]
fn address_library_is_not_applicable_when_the_game_version_is_unknown() {
    let files = vec![DataFile {
        path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
        mod_name: "Buffout".to_owned(),
    }];
    assert!(matches!(
        address_library_status(&files, None, "F4SE/Plugins"),
        AddressLibraryStatus::NotApplicable
    ));
}

/// read_ccc parses the manifest's plugin lines, trimming whitespace and dropping blank lines
#[test]
fn read_ccc_parses_entries_and_drops_blank_lines() {
    let (_tmp, base) = temp_base();
    let mut instance = Instance::new(base.join("instance"), base.join("game"));
    instance.config.game = GameKind::Fallout4;
    std::fs::create_dir_all(&instance.config.game_dir).unwrap();
    std::fs::write(
        instance.config.game_dir.join("Fallout4.ccc"),
        "ccOne.esl\n\n  ccTwo.esl  \n\n",
    )
    .unwrap();

    match read_ccc(&instance) {
        CccStatus::Present { file, entries } => {
            assert_eq!(file, "Fallout4.ccc");
            assert_eq!(
                entries,
                vec!["ccOne.esl".to_owned(), "ccTwo.esl".to_owned()]
            );
        }
        _ => panic!("a readable manifest reads as Present"),
    }
}

/// A game folder with no Fallout4.ccc reads as a missing manifest
#[test]
fn read_ccc_reports_a_missing_manifest() {
    let (_tmp, base) = temp_base();
    let mut instance = Instance::new(base.join("instance"), base.join("game"));
    instance.config.game = GameKind::Fallout4;
    std::fs::create_dir_all(&instance.config.game_dir).unwrap();
    assert!(matches!(
        read_ccc(&instance),
        CccStatus::Missing {
            file: "Fallout4.ccc"
        }
    ));
}

/// A corrupt .ba2 (right extension, wrong bytes) surfaces through scan_archives as Invalid
#[test]
fn scan_archives_flags_a_corrupt_ba2_as_invalid() {
    let (_tmp, base) = temp_base();
    let mod_dir = base.join("mods/A");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(
        mod_dir.join("Broken - Main.ba2"),
        b"this is not a valid BA2 header",
    )
    .unwrap();

    let plan =
        DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("A", &mod_dir)]).unwrap();

    let archives = scan_archives(&plan);
    assert_eq!(archives.len(), 1);
    assert_eq!(archives[0].name, "Broken - Main.ba2");
    assert_eq!(archives[0].mod_name, "A");
    assert!(matches!(archives[0].scan, ArchiveScan::Invalid));
}

/// An F4SE plugin DLL that can't be read is collected as unreadable, not silently dropped
#[test]
fn scan_f4se_plugins_collects_an_unreadable_dll() {
    let (_tmp, base) = temp_base();
    let f4se = base.join("mods/F4SE");
    let dll = f4se.join("F4SE/Plugins/Buffout4.dll");
    write_file(&dll, b"stub");
    let plan =
        DeployPlan::from_rooted_mods(base.join("game"), &[ModSource::new("F4SE", &f4se)]).unwrap();
    // Swap the planned DLL for a directory so reading its bytes fails
    std::fs::remove_file(&dll).unwrap();
    std::fs::create_dir(&dll).unwrap();
    let (plugins, unreadable) = scan_f4se_plugins(&plan, "F4SE/Plugins");
    assert!(
        plugins.is_empty(),
        "an unreadable DLL is not scanned as a plugin"
    );
    assert_eq!(
        unreadable.len(),
        1,
        "the unreadable DLL is reported, not dropped"
    );
    assert_eq!(unreadable[0].name, "Buffout4.dll");
}

#[test]
fn a_base_script_from_a_non_provider_mod_is_flagged() {
    let (_tmp, base) = temp_base();
    let f4se = base.join("mods/F4SE");
    write_file(&f4se.join("Scripts/Actor.pex"), b"f4se");
    write_file(&f4se.join("Scripts/Game.pex"), b"f4se");
    write_file(&f4se.join("Scripts/Form.pex"), b"f4se");
    // A different mod ships a base script — an override the F4SE package doesn't own
    let other = base.join("mods/Other");
    write_file(&other.join("Scripts/Weapon.pex"), b"override");

    let plan = DeployPlan::from_rooted_mods(
        base.join("game"),
        &[
            ModSource::new("F4SE", &f4se),
            ModSource::new("Other", &other),
        ],
    )
    .unwrap();
    let scans = scan_script_overrides(&plan);

    assert_eq!(
        scans.len(),
        1,
        "only the non-provider's base script is flagged"
    );
    assert_eq!(scans[0].name, "Weapon.pex");
    assert_eq!(scans[0].mod_name, "Other");
}

// --- gather: installed implicit (base/DLC/CC) plugins (real temp-dir install) ---

/// A fake Fallout 4 install with temp local/INI dirs away from real `%LOCALAPPDATA%`/Documents, plus empty `Data/`
fn fake_install() -> (TempDir, Instance) {
    let (tmp, base) = temp_base();
    let mut instance = Instance::new(base.join("instance"), base.join("game"));
    instance.config.game = GameKind::Fallout4;
    instance.config.local_dir = Some(base.join("local"));
    instance.config.ini_dir = Some(base.join("ini"));
    std::fs::create_dir_all(instance.config.game_dir.join("Data")).unwrap();
    std::fs::create_dir_all(instance.mods_dir()).unwrap();
    (tmp, instance)
}

fn install_game_plugin(instance: &Instance, name: &str, flags: u32) {
    write_plugin(&instance.config.game_dir.join("Data"), name, flags, &[]);
}

fn write_ba2(path: &Utf8Path, version: u32, tag: &[u8; 4]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, ba2_bytes(version, tag, b"")).unwrap();
}

#[test]
fn gather_loads_only_installed_implicit_plugins() {
    let (_tmp, instance) = fake_install();
    // The base master, one owned DLC, and a Creation Club plugin are installed
    install_game_plugin(&instance, "Fallout4.esm", FLAG_MASTER);
    install_game_plugin(&instance, "DLCCoast.esm", FLAG_MASTER);
    install_game_plugin(&instance, "ccBGSFO4001-PipBoy.esl", 0);
    std::fs::write(
        instance.config.game_dir.join("Fallout4.ccc"),
        "ccBGSFO4001-PipBoy.esl\n",
    )
    .unwrap();

    let ctx = GameContext::gather(&instance, "Default").expect("gather");
    let names: Vec<&str> = ctx.loaded_plugins.iter().map(|p| p.name.as_str()).collect();

    assert!(names.contains(&"Fallout4.esm"), "base master force-loads");
    assert!(names.contains(&"DLCCoast.esm"), "owned DLC force-loads");
    assert!(
        names.contains(&"ccBGSFO4001-PipBoy.esl"),
        "CC plugin from Fallout4.ccc force-loads"
    );
    // An implicit candidate that isn't installed must not be counted
    assert!(
        !names.contains(&"DLCNukaWorld.esm"),
        "an uninstalled DLC does not load"
    );

    // The budget the engine actually sees: 2 full ESMs + 1 light ESL
    let full = ctx.loaded_plugins.iter().filter(|p| !p.is_light).count();
    let light = ctx.loaded_plugins.iter().filter(|p| p.is_light).count();
    assert_eq!(full, 2, "Fallout4.esm + DLCCoast.esm");
    assert_eq!(light, 1, "the CC .esl");
}

#[test]
fn gather_counts_loaded_archives_and_excludes_inactive_plugin_archives() {
    let (_tmp, instance) = fake_install();
    install_game_plugin(&instance, "Fallout4.esm", FLAG_MASTER);
    write_ba2(
        &instance
            .config
            .game_dir
            .join("Data/Fallout4 - Textures1.ba2"),
        1,
        b"DX10",
    );

    install_plugin(&instance, "ActiveMod", "Active.esp");
    write_ba2(
        &instance.mods_dir().join("ActiveMod/Active - Main.ba2"),
        7,
        b"GNRL",
    );
    // A nested archive is not top-level in Data/, so the engine won't auto-load it — even though its basename matches the active plugin. Must not be counted
    write_ba2(
        &instance
            .mods_dir()
            .join("ActiveMod/textures/Active - Extra.ba2"),
        7,
        b"GNRL",
    );
    install_plugin(&instance, "InactiveMod", "Inactive.esp");
    write_ba2(
        &instance.mods_dir().join("InactiveMod/Inactive - Main.ba2"),
        8,
        b"GNRL",
    );
    save_profile(
        &instance,
        "Default",
        &[("ActiveMod", true), ("InactiveMod", true)],
    );
    std::fs::write(
        instance.profile_dir("Default").join("plugins.txt"),
        "*Active.esp\nInactive.esp\n",
    )
    .unwrap();

    let ctx = GameContext::gather(&instance, "Default").expect("gather");

    assert_eq!(
        ctx.loaded_archive_counts,
        LoadedArchiveCounts {
            gnrl: 1,
            dx10: 1,
            v1: 1,
            vng: 1,
        },
        "base + active-mod archives count; the inactive-plugin and nested archives do not"
    );
}

#[test]
fn archive_plugin_stem_strips_only_the_basename_prefix() {
    assert_eq!(
        archive_plugin_stem("Fallout4 - Textures1.ba2").as_deref(),
        Some("fallout4")
    );
    assert_eq!(
        archive_plugin_stem("MyMod - Main.ba2").as_deref(),
        Some("mymod")
    );
    assert_eq!(archive_plugin_stem("MyMod.ba2").as_deref(), Some("mymod"));
    assert_eq!(archive_plugin_stem("MyMod.txt"), None);
}

#[test]
fn address_library_outside_f4se_plugins_does_not_count_as_present() {
    // A stray version-*.bin loose in Data/ must not satisfy the check; it belongs under F4SE/Plugins/
    let version = overseer_core::detect::ExeVersion {
        major: 1,
        minor: 10,
        patch: 163,
        build: 0,
    };
    let files = vec![
        DataFile {
            path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
            mod_name: "Buffout".to_owned(),
        },
        DataFile {
            path: Utf8PathBuf::from("version-1-10-163-0.bin"),
            mod_name: "Stray".to_owned(),
        },
    ];
    match address_library_status(&files, Some(version), "F4SE/Plugins") {
        AddressLibraryStatus::Missing { expected } => {
            assert_eq!(expected, "version-1-10-163-0.bin")
        }
        _ => panic!("a loose version-*.bin outside F4SE/Plugins must read as Missing"),
    }
}

#[test]
fn address_library_under_f4se_plugins_is_present() {
    let version = overseer_core::detect::ExeVersion {
        major: 1,
        minor: 10,
        patch: 163,
        build: 0,
    };
    let files = vec![
        DataFile {
            path: Utf8PathBuf::from("F4SE/Plugins/Buffout4.dll"),
            mod_name: "Buffout".to_owned(),
        },
        DataFile {
            path: Utf8PathBuf::from("F4SE/Plugins/version-1-10-163-0.bin"),
            mod_name: "AddrLib".to_owned(),
        },
    ];
    assert!(matches!(
        address_library_status(&files, Some(version), "F4SE/Plugins"),
        AddressLibraryStatus::Present
    ));
}
