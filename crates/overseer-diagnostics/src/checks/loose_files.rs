//! Loose files in the deployed `Data` tree: junk, misplaced, wrong formats

use super::Check;
use crate::context::{DataFile, GameContext};
use crate::finding::{Finding, Severity};
use camino::Utf8Path;
use std::collections::BTreeSet;

/// Tool output folders that arent game data but we ignore
const IGNORE_FOLDERS: &[&str] = &["bodyslide", "fo4edit", "robco_patcher", "source"];

/// What Fallout 4 loads from a top-level `Data/` folder
enum Rule {
    /// Recognized but not judged (F4SE/MCM, or folders we dont model)
    Any,
    /// The engine loads exactly these extensions from this folder
    Loads(&'static [&'static str]),
}

/// What Fallout 4 loads from `folder` (`None` is the `Data/` root)
fn folder_rule(folder: Option<&str>) -> Rule {
    let Some(folder) = folder else {
        return Rule::Loads(&["esp", "esm", "esl", "ba2"]);
    };

    match folder {
        "textures" => Rule::Loads(&["dds"]),
        "meshes" => Rule::Loads(&["nif", "hkx", "tri", "bto", "btr"]),
        "materials" => Rule::Loads(&["bgsm", "bgem"]),
        "sound" => Rule::Loads(&["fuz", "wav", "xwm", "lip"]),
        "music" => Rule::Loads(&["wav", "xwm"]),
        "interface" => Rule::Loads(&["swf", "gfx", "dds", "txt"]),
        "strings" => Rule::Loads(&["strings", "dlstrings", "ilstrings"]),
        "scripts" => Rule::Loads(&["pex"]),
        "vis" => Rule::Loads(&["uvd"]),
        "lodsettings" => Rule::Loads(&["lod", "txt"]),
        "video" => Rule::Loads(&["bik", "bk2"]),
        _ => Rule::Any,
    }
}

/// Formats that are a known wrong form of an expected one
fn proper_formats(ext: &str) -> Option<&'static [&'static str]> {
    match ext {
        "bmp" | "jpeg" | "jpg" | "png" | "psd" | "tga" => Some(&["dds"]),
        "mp3" => Some(&["wav", "xwm"]),
        _ => None,
    }
}

/// What we make of one deployed file
enum Verdict {
    /// Fallout 4 loads it, nothing to report
    Asset,
    /// A high-confidence problem worth a warning
    Problem(Finding),
    /// The game wont load it (source/docs/leftovers)
    WontLoad,
}

/// Flags deployed files that shouldn't be there
pub struct LooseFiles;

impl Check for LooseFiles {
    fn id(&self) -> &'static str {
        "loose-files"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        // Every deployed data path, lowercased
        let present: BTreeSet<String> = ctx
            .data_files
            .iter()
            .map(|f| f.path.as_str().to_lowercase())
            .collect();

        let mut findings = Vec::new();
        let mut wont_load = 0usize;

        for file in &ctx.data_files {
            match self.inspect(file, &present) {
                Verdict::Asset => {}
                Verdict::Problem(finding) => findings.push(finding),
                Verdict::WontLoad => wont_load += 1,
            }
        }

        if wont_load > 0 {
            let s = if wont_load == 1 { "" } else { "s" };
            findings.push(Finding {
                check: self.id(),
                severity: Severity::Info,
                title: format!("{wont_load} file{s} Fallout 4 won't load"),
                detail: Some("Source, docs, or leftovers: harmless, but safe to remove".to_owned()),
            });
        }

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
    /// Classify one deployed `Data/` file
    fn inspect(&self, file: &DataFile, present: &BTreeSet<String>) -> Verdict {
        let path = file.path.as_path();

        // Skip tool output and hidden subtrees
        if in_skipped_folder(path) {
            return Verdict::Asset;
        }

        let name = path.file_name().unwrap_or_default();
        if name.starts_with('.') {
            return Verdict::Problem(self.warn(
                file,
                "is a hidden metadata file",
                "Delete it: the game and mod managers ignore dotfiles",
            ));
        }

        // The only DLL the game cares about is an F4SE plugin from `F4SE/Plugins/`
        if has_ext(path, "dll") {
            return if under(path, &["f4se", "plugins"]) {
                Verdict::Asset
            } else {
                Verdict::Problem(self.warn(
                    file,
                    "is a DLL outside `F4SE/Plugins/`",
                    "Script-extender plugins load only from `F4SE/Plugins/`",
                ))
            };
        }

        let Some(ext) = path.extension() else {
            return Verdict::WontLoad;
        };

        match folder_rule(top_folder(path).as_deref()) {
            Rule::Any => Verdict::Asset,
            Rule::Loads(formats) if formats.iter().any(|f| ext.eq_ignore_ascii_case(f)) => {
                Verdict::Asset
            }
            Rule::Loads(_) => match self.wrong_format(file, ext, present) {
                Some(finding) => Verdict::Problem(finding),
                None => Verdict::WontLoad,
            },
        }
    }

    /// A warning for a wrong format file
    fn wrong_format(
        &self,
        file: &DataFile,
        ext: &str,
        present: &BTreeSet<String>,
    ) -> Option<Finding> {
        let proper = proper_formats(&ext.to_lowercase())?;
        let folder = top_folder(file.path.as_path()).unwrap_or_default();
        let detail = if proper
            .iter()
            .any(|e| sibling_exists(&file.path, e, present))
        {
            format!(
                "A matching `.{}` already exists: delete this one",
                proper[0]
            )
        } else {
            format!("Convert to `.{}` or remove it", proper[0])
        };
        Some(Finding {
            check: self.id(),
            severity: Severity::Warning,
            title: format!(
                "`{}` doesn't belong in `{folder}` (from `{}`)",
                file.path, file.mod_name
            ),
            detail: Some(detail),
        })
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

/// The lowercased top level folder of a `Data/`-relative path, or `None` at the root
fn top_folder(path: &Utf8Path) -> Option<String> {
    let parent = path.parent()?;
    if parent.as_str().is_empty() {
        return None;
    }
    path.components().next().map(|c| c.as_str().to_lowercase())
}

/// True if `path`'s extension equals `ext`
fn has_ext(path: &Utf8Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// True if `path`'s leading components match `prefix`
fn under(path: &Utf8Path, prefix: &[&str]) -> bool {
    let mut components = path.components();
    prefix.iter().all(|d| {
        components
            .next()
            .is_some_and(|c| c.as_str().eq_ignore_ascii_case(d))
    })
}

/// true if a sibling file with the same stem but `ext` is among the deployed files
fn sibling_exists(path: &Utf8Path, ext: &str, present: &BTreeSet<String>) -> bool {
    present.contains(&path.with_extension(ext).as_str().to_lowercase())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        let findings = run(vec![df("F4SE/Plugins/buffout4.dll")]);
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
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
    fn a_wrong_format_next_to_the_right_one_suggests_deleting() {
        let warning = only_warning(vec![df("textures/armor.png"), df("textures/armor.dds")]);
        assert!(
            warning
                .detail
                .as_deref()
                .unwrap()
                .contains("already exists")
        );
    }

    #[test]
    fn a_valid_format_in_the_wrong_folder_is_not_flagged() {
        // `.nif` is real, but not loaded from textures/ — informational, not a warning.
        let findings = run(vec![df("textures/model.nif")]);
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
        assert!(findings.iter().any(|f| f.title.contains("won't load")));
    }

    #[test]
    fn source_and_doc_files_are_summarized_not_flagged() {
        let findings = run(vec![df("scripts/quest.psc"), df("meshes/armor.max")]);
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
        let summary = findings
            .iter()
            .find(|f| f.title.contains("won't load"))
            .unwrap();
        assert!(summary.title.contains("2 files"));
    }

    #[test]
    fn the_wont_load_summary_is_singular_for_one_file() {
        let findings = run(vec![df("readme.txt")]);
        let summary = findings
            .iter()
            .find(|f| f.title.contains("won't load"))
            .unwrap();
        assert!(
            summary.title.contains("1 file Fallout"),
            "got: {}",
            summary.title
        );
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
        let findings = run(vec![df("TEXTURES/Armor.DDS"), df("F4SE/PLUGINS/Tool.DLL")]);
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
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
