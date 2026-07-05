//! BA2 archives: report the type breakdown and flag unreadable or unsupported archives

use crate::context::{ArchiveScan, GameContext};
use crate::finding::Finding;

use super::{LimitTier, limit_tier};

/// BA2 header versions Fallout 4 can read: 1 = OG, 7/8 = NG/AE. Starfield's v2/v3 are not FO4-readable
const SUPPORTED_VERSIONS: &[u32] = &[1, 7, 8];
const MAX_ARCHIVES_GNRL: usize = 256;
const MAX_ARCHIVES_DX10: usize = 255;

/// Reports the BA2 archive types/counts and flags ones the engine can't use
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings = Vec::new();

    for archive in &ctx.archives {
        match &archive.scan {
            ArchiveScan::Header(header) => {
                if !SUPPORTED_VERSIONS.contains(&header.version) {
                    findings.push(
                        Finding::warning(format!(
                            "`{}` (from `{}`) is an unsupported BA2 version ({})",
                            archive.name, archive.mod_name, header.version
                        ))
                        .detail("Fallout 4 reads BA2 versions 1, 7 and 8"),
                    );
                }
            }
            ArchiveScan::Invalid => findings.push(
                Finding::warning(format!(
                    "`{}` (from `{}`) is not a valid BA2 (bad header)",
                    archive.name, archive.mod_name
                ))
                .detail("The game may fail to load it; re-pack or remove it"),
            ),
            ArchiveScan::Unreadable(why) => findings.push(
                Finding::warning(format!(
                    "`{}` (from `{}`) could not be read",
                    archive.name, archive.mod_name
                ))
                .detail(why.clone()),
            ),
        }
    }

    let counts = &ctx.loaded_archive_counts;
    if let Some(finding) = limit_finding(counts.gnrl, MAX_ARCHIVES_GNRL, "General") {
        findings.push(finding);
    }
    if let Some(finding) = limit_finding(counts.dx10, MAX_ARCHIVES_DX10, "Texture") {
        findings.push(finding);
    }

    if counts.gnrl + counts.dx10 > 0 {
        findings.push(Finding::info(format!(
            "{}/{} general + {}/{} texture BA2 loaded · versions: {} v1, {} v7/8",
            counts.gnrl, MAX_ARCHIVES_GNRL, counts.dx10, MAX_ARCHIVES_DX10, counts.v1, counts.vng
        )));
    }
    findings
}

fn limit_finding(count: usize, limit: usize, label: &str) -> Option<Finding> {
    match limit_tier(count, limit) {
        LimitTier::Over => Some(
            Finding::error(format!(
                "{label} BA2 archives: {count} / {limit} — over the limit"
            ))
            .detail(
                "Unpack or merge archives to reduce the total (don't mix texture and non-texture when merging).",
            ),
        ),
        LimitTier::Near => Some(Finding::warning(format!(
            "{label} BA2 archives: {count} / {limit} — approaching the limit"
        ))),
        LimitTier::Under => None,
    }
}

#[cfg(test)]
#[path = "tests/archives.rs"]
mod tests;
