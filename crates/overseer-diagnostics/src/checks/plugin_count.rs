//! Active full/light plugin counts vs the engine limits

use crate::context::GameContext;
use crate::finding::Finding;

use super::{LimitTier, limit_tier};

/// Counts active Full and Light (ESL) plugins against the engine limits
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let Some(limits) = ctx.game.engine_limits() else {
        return Vec::new();
    };
    let light = ctx.loaded_plugins.iter().filter(|p| p.is_light).count();
    let full = ctx.loaded_plugins.len() - light;
    vec![
        limit_finding("FULL (ESM/ESP)", full, limits.plugins_full),
        limit_finding("Light (ESL)", light, limits.plugins_light),
    ]
}

/// One finding for a plugin tier: error over the limit, warn when near it, else info
fn limit_finding(label: &str, count: usize, limit: usize) -> Finding {
    let title = format!("{label} plugins: {count} / {limit}");
    match limit_tier(count, limit) {
        LimitTier::Over => Finding::error(title).detail("Over the limit — the game won't start"),
        LimitTier::Near => Finding::warning(title).detail("Approaching the limit"),
        LimitTier::Under => Finding::info(title),
    }
}

#[cfg(test)]
#[path = "tests/plugin_count.rs"]
mod tests;
