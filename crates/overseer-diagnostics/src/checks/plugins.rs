//! Plugins that could not be parsed during inspection

use crate::context::GameContext;
use crate::finding::Finding;

/// Warns about each plugin the inspector could not read
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    ctx.unreadable_plugins
        .iter()
        .map(|p| {
            Finding::warning(format!("`{}` could not be read", p.name)).detail(p.reason.clone())
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/plugins.rs"]
mod tests;
