//! Installed DLC that isn't at the cross-storefront consistency revision

use crate::context::{DlcGroupState, GameContext};
use crate::finding::Finding;

/// Flags installed DLC whose files aren't at the consistency revision
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    if ctx.dlc_consistency.is_empty() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for group in &ctx.dlc_consistency {
        if !group.off_revision.is_empty() {
            findings.push(off_revision_warn(group));
        }
        if !group.missing.is_empty() {
            findings.push(missing_warn(group));
        }
    }
    if findings.is_empty() {
        findings.push(Finding::info(format!(
            "DLC is at the cross-storefront consistency revision ({} group(s))",
            ctx.dlc_consistency.len()
        )));
    }
    findings
}

/// A warning that a DLC group has files off the consistency revision
fn off_revision_warn(group: &DlcGroupState) -> Finding {
    Finding::warning(format!(
        "`{}` DLC isn't at the cross-storefront consistency revision ({} file(s) differ)",
        group.group,
        group.off_revision.len()
    ))
    .detail("Run `overseer patch dlc-consistency` to bring the DLC to the consistency revision")
}

/// A warning that an installed DLC group is missing required files
fn missing_warn(group: &DlcGroupState) -> Finding {
    Finding::warning(format!(
        "`{}` DLC is missing {} file(s) from a complete install",
        group.group,
        group.missing.len()
    ))
    .detail(
        "Verify the game files through your storefront (or reinstall); the DLC install is incomplete",
    )
}

#[cfg(test)]
#[path = "tests/dlc_consistency.rs"]
mod tests;
