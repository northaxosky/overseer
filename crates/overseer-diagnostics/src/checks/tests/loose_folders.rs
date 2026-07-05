//! Tests for the loose-folders check

use super::*;
use crate::context::DataFile;
use camino::Utf8Path;

fn df(path: &str, mod_name: &str) -> DataFile {
    DataFile {
        path: Utf8Path::new(path).to_owned(),
        mod_name: mod_name.to_owned(),
    }
}

fn run(files: Vec<DataFile>) -> Vec<Finding> {
    let ctx = GameContext {
        data_files: files,
        ..GameContext::default()
    };
    super::run(&ctx)
}

#[test]
fn a_clean_tree_reports_a_clean_info() {
    let findings = run(vec![df("meshes/armor.nif", "A"), df("textures/x.dds", "A")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}

#[test]
fn unpacked_animtextdata_is_an_error() {
    let findings = run(vec![df("meshes/animtextdata/male.txt", "A")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].title.contains("meshes/animtextdata"));
}

#[test]
fn loose_precombined_warns() {
    let findings = run(vec![df("meshes/precombined/abc.nif", "A")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("meshes/precombined"));
}

#[test]
fn loose_vis_warns() {
    let findings = run(vec![df("vis/abc.uvd", "A")]);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("`vis`"));
}

#[test]
fn a_fomod_folder_warns() {
    let findings = run(vec![df("fomod/ModuleConfig.xml", "A")]);
    assert_eq!(findings[0].severity, Severity::Warning);
    assert!(findings[0].title.contains("`fomod`"));
}

#[test]
fn many_files_from_one_mod_collapse_to_one_finding() {
    let findings = run(vec![
        df("vis/a.uvd", "A"),
        df("vis/b.uvd", "A"),
        df("vis/sub/c.uvd", "A"),
    ]);
    assert_eq!(findings.len(), 1);
}

#[test]
fn each_mod_gets_its_own_finding() {
    let findings = run(vec![
        df("meshes/precombined/a.nif", "ModA"),
        df("meshes/precombined/b.nif", "ModB"),
    ]);
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|f| f.title.contains("ModA")));
    assert!(findings.iter().any(|f| f.title.contains("ModB")));
}

#[test]
fn matching_is_case_insensitive() {
    let findings = run(vec![df("MESHES/PreCombined/a.nif", "A")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Warning);
}

#[test]
fn a_normal_meshes_subfolder_is_not_flagged() {
    let findings = run(vec![df("meshes/architecture/a.nif", "A")]);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}
