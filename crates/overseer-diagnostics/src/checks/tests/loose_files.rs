//! Tests for the loose-files check

use super::*;
use crate::finding::Severity;

fn df(path: &str) -> DataFile {
    DataFile {
        path: Utf8Path::new(path).to_owned(),
        mod_name: "TestMod".to_owned(),
    }
}

fn ctx(files: Vec<DataFile>) -> GameContext {
    GameContext {
        data_files: files,
        ..GameContext::default()
    }
}

fn run(files: Vec<DataFile>) -> Vec<Finding> {
    super::run(&ctx(files))
}

/// Run, asserting exactly one warning came out, and return it
fn only_warning(files: Vec<DataFile>) -> Finding {
    let mut warnings: Vec<Finding> = run(files)
        .into_iter()
        .filter(|f| f.severity == Severity::Warning)
        .collect();
    assert_eq!(warnings.len(), 1, "expected exactly one warning");
    warnings.pop().unwrap()
}

/// Assert the files produce no warnings (only the clean-bill Info line)
fn assert_no_warnings(files: Vec<DataFile>) {
    let findings = run(files);
    assert!(
        findings.iter().all(|f| f.severity == Severity::Info),
        "expected no warnings, got: {findings:?}"
    );
}

#[test]
fn recognized_assets_report_nothing() {
    let findings = run(vec![
        df("textures/armor.dds"),
        df("meshes/armor.nif"),
        df("materials/armor.bgsm"),
        df("MyMod.esp"),
    ]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
    assert!(findings[0].title.contains("No loose-file problems"));
}

#[test]
fn a_dotfile_is_a_warning() {
    let warning = only_warning(vec![df("textures/.ds_store")]);
    assert!(warning.title.contains(".ds_store"));
    assert!(warning.title.contains("hidden"));
}

#[test]
fn a_dll_outside_f4se_plugins_warns() {
    let warning = only_warning(vec![df("loose.dll")]);
    assert!(warning.title.contains("F4SE/Plugins"));
}

#[test]
fn a_dll_inside_f4se_plugins_is_an_asset() {
    assert_no_warnings(vec![df("F4SE/Plugins/buffout4.dll")]);
}

#[test]
fn a_wrong_texture_format_suggests_converting() {
    let warning = only_warning(vec![df("textures/armor.png")]);
    assert!(
        warning
            .detail
            .as_deref()
            .unwrap()
            .contains("Convert to `.dds`")
    );
}

#[test]
fn a_wrong_audio_format_suggests_converting() {
    let warning = only_warning(vec![df("sound/voice.mp3")]);
    assert!(
        warning
            .detail
            .as_deref()
            .unwrap()
            .contains("Convert to `.wav`")
    );
}

#[test]
fn a_source_format_is_flagged_regardless_of_folder() {
    // Folder-independent: a `.png` won't load anywhere, so flag it even outside textures/
    let warning = only_warning(vec![df("meshes/preview.png")]);
    assert!(warning.title.contains("won't load"));
}

#[test]
fn a_valid_format_in_the_wrong_folder_is_left_alone() {
    // We flag only confident mistakes; a real asset in an odd folder isn't one
    assert_no_warnings(vec![df("textures/model.nif")]);
}

#[test]
fn source_and_doc_files_are_left_alone() {
    // Files the game ignores but that do no harm are not reported
    assert_no_warnings(vec![df("scripts/quest.psc"), df("readme.txt")]);
}

#[test]
fn unmodeled_and_tool_folders_are_left_alone() {
    let findings = run(vec![
        df("mcm/config/MyMod/config.json"),
        df("distantlod/something.bin"),
    ]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("No loose-file problems"));
}

#[test]
fn ignored_and_hidden_subtrees_are_skipped() {
    let findings = run(vec![
        df("meshes/source/armor.psc"), // `source` is an ignore-folder
        df(".git/config"),             // hidden directory
        df("tools/bodyslide/armor.osp"),
    ]);
    assert_eq!(findings.len(), 1);
    assert!(findings[0].title.contains("No loose-file problems"));
}

#[test]
fn matching_is_case_insensitive() {
    assert_no_warnings(vec![df("TEXTURES/Armor.DDS"), df("F4SE/PLUGINS/Tool.DLL")]);
}

#[test]
fn the_warning_names_the_mod() {
    let findings = super::run(&ctx(vec![DataFile {
        path: Utf8Path::new("loose.dll").to_owned(),
        mod_name: "Cool Mod".to_owned(),
    }]));
    let warning = findings
        .iter()
        .find(|f| f.severity == Severity::Warning)
        .unwrap();
    assert!(warning.title.contains("Cool Mod"));
}
