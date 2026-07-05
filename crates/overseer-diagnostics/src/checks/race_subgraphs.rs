//! Race subgraph (`SADD`) record counts: a heuristic for cell-transition stutter

use crate::context::GameContext;
use crate::finding::Finding;

// Mods adding more `SADD` records than this correlate with stutter apparently
const STUTTER_THRESHOLD: usize = 100;

/// Warns when active mods add many race subgraph records
pub fn run(ctx: &GameContext) -> Vec<Finding> {
    let total: usize = ctx.sadd_records.iter().map(|r| r.count).sum();
    if total <= STUTTER_THRESHOLD {
        return vec![Finding::info(
            "Race-subgraph record counts are within the safe range",
        )];
    }
    let plugins = ctx
        .sadd_records
        .iter()
        .map(|r| r.plugin.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    vec![
        Finding::warning(format!(
            "Mods add {total} race-subgraph records across {} plugins: {plugins}", ctx.sadd_records.len()
        )).detail("High counts can cause stutter between cells; removing or merging animation mods can help")
    ]
}

#[cfg(test)]
#[path = "tests/race_subgraphs.rs"]
mod tests;
