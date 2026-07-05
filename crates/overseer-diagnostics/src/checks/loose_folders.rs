//! Folders that shouldn't sit loose in `Data/`: previs/precombines, AnimTextData, FOMOD leftovers

use super::under;
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
pub fn run(ctx: &GameContext) -> Vec<Finding> {
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
            )
            .detail(folder.fix)
        }));
    }
    if findings.is_empty() {
        findings.push(Finding::info("No loose-folder problems found"));
    }
    findings
}

#[cfg(test)]
#[path = "tests/loose_folders.rs"]
mod tests;
