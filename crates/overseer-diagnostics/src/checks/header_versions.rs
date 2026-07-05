//! Plugin `TES4`/`HEDR` module versions: flag any that Fallout 4 doesn't accept

use crate::context::GameContext;
use crate::finding::Finding;
use overseer_core::plugins::PluginMeta;

/// True if `v` is exactly one of the two `HEDR` versions Fallout 4 accepts (0.95 or 1.00)
fn is_known_hedr(v: f32) -> bool {
    let bits = v.to_bits();
    bits == 0.95f32.to_bits() || bits == 1.0f32.to_bits()
}

/// Flags plugins whose header version isn't one Fallout 4 recognizes
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings: Vec<Finding> = ctx.loaded_plugins.iter().filter_map(warn_unknown).collect();
    if findings.is_empty() {
        findings.push(Finding::info("All plugin header versions are recognized"));
    }
    findings
}

/// Warn when a plugin's `HEDR` version is present but not one Fallout 4 accepts
fn warn_unknown(plugin: &PluginMeta) -> Option<Finding> {
    let v = plugin.header_version?;
    if is_known_hedr(v) {
        return None;
    }
    Some(
        Finding::warning(format!(
            "`{}` has header version {v} (Fallout 4 uses 0.95 or 1.00)",
            plugin.name
        ))
        .detail(
            "Resave it in the Creation Kit to update the header, then confirm the result in xEdit.",
        ),
    )
}

#[cfg(test)]
#[path = "tests/header_versions.rs"]
mod tests;
