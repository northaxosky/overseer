//! Race subgraph (`SADD`) record counts: a heuristic for cell-transition stutter

use super::Check;
use crate::context::GameContext;
use crate::finding::{Finding, Severity};

// Mods adding more `SADD` records than this correlate with stutter apparently
const STUTTER_THRESHOLD: usize = 100;

/// Warns when active mods add many race subgraph records
pub struct RaceSubgraphs;

impl Check for RaceSubgraphs {
    fn id(&self) -> &'static str {
        "race-subgraphs"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let total: usize = ctx.sadd_records.iter().map(|r| r.count).sum();
        if total <= STUTTER_THRESHOLD {
            return Vec::new();
        }
        vec![Finding::new(
            Severity::Warning,
            format!("Mods add {total} race-subgraph records across {} plugins", ctx.sadd_records.len()),
            Some("High counts can cause stutter between cells; removing or merging animation mods can help".to_owned()),
        )]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SaddCount;

    fn ctx(records: Vec<SaddCount>) -> GameContext {
        GameContext {
            sadd_records: records,
            ..GameContext::default()
        }
    }

    fn sadd(plugin: &str, count: usize) -> SaddCount {
        SaddCount {
            plugin: plugin.to_owned(),
            count,
        }
    }

    #[test]
    fn under_the_threshold_is_silent() {
        let findings = RaceSubgraphs.run(&ctx(vec![sadd("A.esp", 50), sadd("B.esp", 40)]));
        assert!(findings.is_empty());
    }

    #[test]
    fn exactly_at_the_threshold_is_silent() {
        let findings = RaceSubgraphs.run(&ctx(vec![sadd("A.esp", 100)]));
        assert!(findings.is_empty(), "the threshold is exclusive");
    }

    #[test]
    fn over_the_threshold_warns_with_the_total_and_plugin_count() {
        let findings = RaceSubgraphs.run(&ctx(vec![sadd("A.esp", 80), sadd("B.esp", 40)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("120"), "80 + 40");
        assert!(findings[0].title.contains("2 plugins"));
    }

    #[test]
    fn no_records_is_silent() {
        assert!(RaceSubgraphs.run(&ctx(Vec::new())).is_empty());
    }
}
