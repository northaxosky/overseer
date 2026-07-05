//! Active full/light plugin counts vs the engine limits

use crate::context::GameContext;
use crate::finding::Finding;

use super::{LimitTier, limit_tier};

/// Fallout 4's hard limits on simultaneously-loaded plugins
const MAX_FULL: usize = 254;
const MAX_LIGHT: usize = 4096;

/// Counts active Full and Light (ESL) plugins against the engine limits
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let light = ctx.loaded_plugins.iter().filter(|p| p.is_light).count();
    let full = ctx.loaded_plugins.len() - light;
    vec![
        count_finding("Full (ESM/ESP)", full, MAX_FULL),
        count_finding("Light (ESL)", light, MAX_LIGHT),
    ]
}

/// One finding for a plugin tier: error over the limit, warn when near it, else info
fn count_finding(label: &str, count: usize, limit: usize) -> Finding {
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
