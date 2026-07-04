//! Folders that shouldn't sit loose in `Data/`: previs/precombines, AnimTextData, FOMOD leftovers

use super::{Check, under};
use crate::context::GameContext;
use crate::finding::{Finding, Severity};
use std::collections::BTreeSet;

/// A folder that's a problem when deployed loose into `Data/`
struct LooseFolder {
    /// The path prefix that identifies the folder
    prefix: &'static [&'static str],
    /// The severity of the loose folder
    severity: Severity,
    /// Completes the title: "`<folder>` {summary}"
    summary: &'static str,
    /// How to fix it
    fix: &'static str,
}

/// The folders the game does not want loose, most serious first
const LOOSE_FOLDERS: &[LooseFolder] = &[
    LooseFolder {
        prefix: &["meshes", "animtextdata"],
        severity: Severity::Error,
        summary: "is unpacked AnimTextData, a known crash cause",
        fix: "Pack it into a BA2 archive, or remove it",
    },
    LooseFolder {
        prefix: &["meshes", "precombined"],
        severity: Severity::Warning,
        summary: "holds loose precombined meshes (previs)",
        fix: "Pack them into a BA2 so they follow plugin load order",
    },
    LooseFolder {
        prefix: &["vis"],
        severity: Severity::Warning,
        summary: "holds loose previs (visibility) data",
        fix: "Pack it into a BA2 so it follows plugin load order",
    },
    LooseFolder {
        prefix: &["fomod"],
        severity: Severity::Warning,
        summary: "is leftover FOMOD installer data",
        fix: "Remove it from the mod; nothing reads it",
    },
];

/// Flags folders that shouldn't be deployed loose into `Data/`
pub struct LooseFolders;

impl Check for LooseFolders {
    fn id(&self) -> &'static str {
        "loose-folders"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings = Vec::new();
        for folder in LOOSE_FOLDERS {
            let mods: BTreeSet<&str> = ctx
                .data_files
                .iter()
                .filter(|file| under(&file.path, folder.prefix))
                .map(|file| file.mod_name.as_str())
                .collect();

            findings.extend(mods.into_iter().map(|name| {
                Finding::new(
                    folder.severity,
                    format!(
                        "`{}` {} (from `{}`)",
                        folder.prefix.join("/"),
                        folder.summary,
                        name
                    ),
                    Some(folder.fix.to_owned()),
                )
            }));
        }
        findings
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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
        LooseFolders.run(&ctx)
    }

    #[test]
    fn a_clean_tree_reports_nothing() {
        let findings = run(vec![df("meshes/armor.nif", "A"), df("textures/x.dds", "A")]);
        assert!(findings.is_empty());
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
        assert!(findings.is_empty());
    }
}
