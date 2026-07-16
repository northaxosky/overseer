//! Plugins that could not be parsed during inspection

use crate::context::GameContext;
use crate::finding::Finding;

/// Warns about each plugin the inspector could not read
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let mut findings: Vec<Finding> = ctx
        .unreadable_plugins
        .iter()
        .map(|p| {
            Finding::warning(format!("`{}` could not be read", p.name)).detail(p.reason.clone())
        })
        .collect();

    if findings.is_empty() {
        findings.push(Finding::info("All plugins were read successfully"))
    }
    findings
}

#[cfg(test)]
#[path = "tests/plugins.rs"]
mod tests;
