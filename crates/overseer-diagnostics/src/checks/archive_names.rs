//! Top-level `Data/*.ba2` names: flag archives the game won't auto load because of their name

use crate::context::{ArchiveInfo, GameContext};
use crate::finding::Finding;
use overseer_core::deploy::strip_data_prefix;
use overseer_core::ini::GameInis;
use std::collections::BTreeSet;

/// Base-game archives whose names break the auto-load rule but are legitimate, so never flagged
const NAME_WHITELIST: &[&str] = &[
    "creationkit - shaders.ba2",
    "creationkit - textures.ba2",
    "fallout4 - animations.ba2",
    "fallout4 - interface.ba2",
    "fallout4 - materials.ba2",
    "fallout4 - meshes.ba2",
    "fallout4 - meshesextra.ba2",
    "fallout4 - misc.ba2",
    "fallout4 - nvflex.ba2",
    "fallout4 - shaders.ba2",
    "fallout4 - sounds.ba2",
    "fallout4 - startup.ba2",
    "fallout4 - textures1.ba2",
    "fallout4 - textures2.ba2",
    "fallout4 - textures3.ba2",
    "fallout4 - textures4.ba2",
    "fallout4 - textures5.ba2",
    "fallout4 - textures6.ba2",
    "fallout4 - textures7.ba2",
    "fallout4 - textures8.ba2",
    "fallout4 - textures9.ba2",
    "fallout4 - texturespatch.ba2",
    "fallout4 - voices.ba2",
    "dlcultrahighresolution - textures01.ba2",
    "dlcultrahighresolution - textures02.ba2",
    "dlcultrahighresolution - textures03.ba2",
    "dlcultrahighresolution - textures04.ba2",
    "dlcultrahighresolution - textures05.ba2",
    "dlcultrahighresolution - textures06.ba2",
    "dlcultrahighresolution - textures07.ba2",
    "dlcultrahighresolution - textures08.ba2",
    "dlcultrahighresolution - textures09.ba2",
    "dlcultrahighresolution - textures10.ba2",
    "dlcultrahighresolution - textures11.ba2",
    "dlcultrahighresolution - textures12.ba2",
    "dlcultrahighresolution - textures13.ba2",
    "dlcultrahighresolution - textures14.ba2",
    "dlcultrahighresolution - textures15.ba2",
    "dlcultrahighresolution - textures16.ba2",
];

/// `[Archive]` keys whose values list archives the engine loads regardless of name
const INI_ARCHIVE_KEYS: &[&str] = &[
    "sResourceIndexFileList",
    "sResourceStartUpArchiveList",
    "sResourceArchiveList",
    "sResourceArchiveList2",
];

/// Flags top-level `Data/*.ba2` archives the engine won't auto-load because of their name
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let registered = ini_registered_archives(ctx.inis.as_ref());
    let mut findings: Vec<Finding> = ctx
        .archives
        .iter()
        .filter(|a| is_top_level_data(a))
        .filter_map(|a| flag(a, &registered))
        .collect();
    if findings.is_empty() {
        findings.push(Finding::info("No archive-name problems found"));
    }
    findings
}

/// True if the archive deploys directly under `Data/` (not nested)
fn is_top_level_data(archive: &ArchiveInfo) -> bool {
    strip_data_prefix(&archive.relative).is_some_and(|r| r.components().count() == 1)
}

/// Warn if a top-level archive is neither whitelisted, INI-registered, nor validly named
fn flag(archive: &ArchiveInfo, registered: &BTreeSet<String>) -> Option<Finding> {
    let lower = archive.name.to_lowercase();
    if NAME_WHITELIST.contains(&lower.as_str())
        || registered.contains(&lower)
        || name_auto_loads(&lower)
    {
        return None;
    }
    Some(
        Finding::warning(format!(
            "`{}` (from `{}`) won't be loaded by the game",
            archive.name, archive.mod_name
        ))
        .detail(
            "Fallout 4 auto-loads an archive only when its named `<Plugin> - Main.ba2` or `<Plugin> \
         - Textures.ba2`. Rename it to match its plugin, or register it in an INI archive list.",
        ),
    )
}

/// True if a lowercased `*.ba2` filename follows Fallout 4's auto-load naming convention
fn name_auto_loads(filename_lower: &str) -> bool {
    let stem = filename_lower
        .strip_suffix(".ba2")
        .unwrap_or(filename_lower);
    let Some((_, suffix)) = stem.rsplit_once(" - ") else {
        return false;
    };
    suffix == "main"
        || suffix == "textures"
        || suffix
            .strip_prefix("voices_")
            .is_some_and(|lang| !lang.is_empty())
}

/// The lowercased archive filenames registered across the `[Archive]` load lists
fn ini_registered_archives(inis: Option<&GameInis>) -> BTreeSet<String> {
    let mut registered = BTreeSet::new();
    let Some(inis) = inis else {
        return registered;
    };
    for key in INI_ARCHIVE_KEYS {
        if let Some(value) = inis.settings.get("Archive", key) {
            for name in value.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    registered.insert(name.to_lowercase());
                }
            }
        }
    }
    registered
}

#[cfg(test)]
#[path = "tests/archive_names.rs"]
mod tests;
