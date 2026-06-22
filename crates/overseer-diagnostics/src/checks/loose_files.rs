//! Loose files in the deployed `Data` tree: high-confidence mistakes worth flagging.

use super::{Check, under};
use crate::context::{DataFile, GameContext};
use crate::finding::{Finding, Severity};
use camino::Utf8Path;

/// Tool-output folders that aren't game data, so we leave their contents alone
const IGNORE_FOLDERS: &[&str] = &["bodyslide", "fo4edit", "robco_patcher", "source"];

/// A source/intermediate format Fallout 4 never loads, mapped to the form it should be in.
fn wrong_format(ext: &str) -> Option<&'static str> {
    match ext {
        "bmp" | "jpeg" | "jpg" | "png" | "psd" | "tga" => Some("dds"),
        "mp3" => Some("wav"),
        _ => None,
    }
}

/// Flags deployed files that are almost certainly a mistake
pub struct LooseFiles;

impl Check for LooseFiles {
    fn id(&self) -> &'static str {
        "loose-files"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings: Vec<Finding> = ctx
            .data_files
            .iter()
            .filter_map(|f| self.inspect(f))
            .collect();

        if findings.is_empty() {
            findings.push(Finding {
                check: self.id(),
                severity: Severity::Info,
                title: "No loose-file problems found".to_owned(),
                detail: None,
            });
        }
        findings
    }
}

impl LooseFiles {
    /// Flag one deployed `Data/` file if it's a confident mistake, otherwise `None`
    fn inspect(&self, file: &DataFile) -> Option<Finding> {
        let path = file.path.as_path();

        // Leave tool output and hidden subtrees alone
        if in_skipped_folder(path) {
            return None;
        }

        let name = path.file_name().unwrap_or_default();
        if name.starts_with('.') {
            return Some(self.warn(
                file,
                "is a hidden metadata file",
                "Delete it: the game and mod managers ignore dotfiles",
            ));
        }

        let ext = path.extension()?;

        // The only DLL the game loads is an F4SE plugin from `F4SE/Plugins/`
        if ext.eq_ignore_ascii_case("dll") && !under(path, &["f4se", "plugins"]) {
            return Some(self.warn(
                file,
                "is a DLL outside `F4SE/Plugins/`",
                "Script-extender plugins load only from `F4SE/Plugins/`",
            ));
        }

        // A source texture/audio format the game can't load, anywhere in the tree
        if let Some(proper) = wrong_format(&ext.to_lowercase()) {
            return Some(self.warn(
                file,
                &format!("is a `.{ext}` the game won't load"),
                &format!("Convert to `.{proper}` or remove it"),
            ));
        }

        None
    }

    /// A warning naming the offending file and its mod
    fn warn(&self, file: &DataFile, problem: &str, fix: &str) -> Finding {
        Finding {
            check: self.id(),
            severity: Severity::Warning,
            title: format!("`{}` {problem} (from `{}`)", file.path, file.mod_name),
            detail: Some(fix.to_owned()),
        }
    }
}

/// True if a directory in the path is a tool-output (`bodyslide`, …) or hidden (`.git`) folder
fn in_skipped_folder(path: &Utf8Path) -> bool {
    path.parent().is_some_and(|parent| {
        parent.components().any(|c| {
            let name = c.as_str();
            name.starts_with('.') || IGNORE_FOLDERS.iter().any(|s| name.eq_ignore_ascii_case(s))
        })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn df(path: &str) -> DataFile {
        DataFile {
            path: Utf8Path::new(path).to_owned(),
            mod_name: "TestMod".to_owned(),
        }
    }

    fn ctx(files: Vec<DataFile>) -> GameContext {
        GameContext {
            active_plugins: Vec::new(),
            present_plugins: BTreeSet::new(),
            data_files: files,
            ccc: crate::context::CccStatus::NotApplicable,
        }
    }

    fn run(files: Vec<DataFile>) -> Vec<Finding> {
        LooseFiles.run(&ctx(files))
    }

    /// Run, asserting exactly one warning came out, and return it.
    fn only_warning(files: Vec<DataFile>) -> Finding {
        let mut warnings: Vec<Finding> = run(files)
            .into_iter()
            .filter(|f| f.severity == Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1, "expected exactly one warning");
        warnings.pop().unwrap()
    }

    /// Assert the files produce no warnings (only the clean-bill Info line).
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
        // Folder-independent: a `.png` won't load anywhere, so flag it even outside textures/.
        let warning = only_warning(vec![df("meshes/preview.png")]);
        assert!(warning.title.contains("won't load"));
    }

    #[test]
    fn a_valid_format_in_the_wrong_folder_is_left_alone() {
        // We flag only confident mistakes; a real asset in an odd folder isn't one.
        assert_no_warnings(vec![df("textures/model.nif")]);
    }

    #[test]
    fn source_and_doc_files_are_left_alone() {
        // Files the game ignores but that do no harm are not reported.
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
        let findings = LooseFiles.run(&ctx(vec![DataFile {
            path: Utf8Path::new("loose.dll").to_owned(),
            mod_name: "Cool Mod".to_owned(),
        }]));
        let warning = findings
            .iter()
            .find(|f| f.severity == Severity::Warning)
            .unwrap();
        assert!(warning.title.contains("Cool Mod"));
    }
}
