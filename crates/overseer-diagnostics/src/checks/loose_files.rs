//! Loose files in the deployed `Data` tree: high-confidence mistakes worth flagging.

use super::under;
use crate::context::{DataFile, GameContext};
use crate::finding::Finding;
use camino::Utf8Path;

/// Tool-output folders that aren't game data, so we leave their contents alone
const IGNORE_FOLDERS: &[&str] = &["bodyslide", "fo4edit", "robco_patcher", "source"];

/// A source/intermediate format Fallout 4 never loads, mapped to the form it should be in
fn wrong_format(ext: &str) -> Option<&'static str> {
    match ext {
        "bmp" | "jpeg" | "jpg" | "png" | "psd" | "tga" => Some("dds"),
        "mp3" => Some("wav"),
        _ => None,
    }
}

/// Flags deployed files that are almost certainly a mistake
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings: Vec<Finding> = ctx.data_files.iter().filter_map(inspect).collect();

    if findings.is_empty() {
        findings.push(Finding::info("No loose-file problems found"));
    }
    findings
}

/// Flag one deployed `Data/` file if it's a confident mistake, otherwise `None`
fn inspect(file: &DataFile) -> Option<Finding> {
    let path = file.path.as_path();

    // Leave tool output and hidden subtrees alone
    if in_skipped_folder(path) {
        return None;
    }

    let name = path.file_name().unwrap_or_default();
    if name.starts_with('.') {
        return Some(warn(
            file,
            "is a hidden metadata file",
            "Delete it: the game and mod managers ignore dotfiles",
        ));
    }

    let ext = path.extension()?;

    // The only DLL the game loads is an F4SE plugin from `F4SE/Plugins/`
    if ext.eq_ignore_ascii_case("dll") && !under(path, &["f4se", "plugins"]) {
        return Some(warn(
            file,
            "is a DLL outside `F4SE/Plugins/`",
            "Script-extender plugins load only from `F4SE/Plugins/`",
        ));
    }

    // A source texture/audio format the game can't load, anywhere in the tree
    if let Some(proper) = wrong_format(&ext.to_lowercase()) {
        return Some(warn(
            file,
            &format!("is a `.{ext}` the game won't load"),
            &format!("Convert to `.{proper}` or remove it"),
        ));
    }

    None
}

/// A warning naming the offending file and its mod
fn warn(file: &DataFile, problem: &str, fix: &str) -> Finding {
    Finding::warning(format!(
        "`{}` {problem} (from `{}`)",
        file.path, file.mod_name
    ))
    .detail(fix)
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

#[cfg(test)]
#[path = "tests/loose_files.rs"]
mod tests;
