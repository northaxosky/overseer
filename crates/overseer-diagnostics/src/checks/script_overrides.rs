//! Loose `Data/Scripts/*.pex` from a mod that overrides a base F4SE script

use crate::context::{GameContext, ScriptOverrideScan};
use crate::finding::Finding;

/// Flags mods that override a base F4SE Papyrus script
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings: Vec<Finding> = ctx.script_overrides.iter().map(warn).collect();
    if findings.is_empty() {
        findings.push(Finding::info("No base F4SE script overrides found"));
    }
    findings
}

/// A warning that a mod overrides a base F4SE script
fn warn(scan: &ScriptOverrideScan) -> Finding {
    Finding::warning(format!(
        "`{}` (from `{}`) overrides a base F4SE script",
        scan.name, scan.mod_name
    ))
    .detail(
        "This isn't the mod that provides F4SE's scripts, so it's replacing one of them — which \
         usually breaks F4SE unless the mod is built for your exact game version. If it isn't, \
         remove this file.",
    )
}

#[cfg(test)]
#[path = "tests/script_overrides.rs"]
mod tests;
