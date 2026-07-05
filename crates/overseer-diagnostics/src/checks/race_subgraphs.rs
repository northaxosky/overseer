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
    vec![
        Finding::warning(format!(
            "Mods add {total} race-subgraph records across {} plugins",
            ctx.sadd_records.len()
        ))
        .detail(
            "High counts can cause stutter between cells; removing or merging animation mods can help",
        ),
    ]
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SaddCount;
    use crate::finding::Severity;

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
    fn under_the_threshold_reports_a_clean_info() {
        let findings = super::run(&ctx(vec![sadd("A.esp", 50), sadd("B.esp", 40)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("within the safe range"));
    }

    #[test]
    fn exactly_at_the_threshold_reports_a_clean_info() {
        let findings = super::run(&ctx(vec![sadd("A.esp", 100)]));
        assert_eq!(findings.len(), 1, "the threshold is exclusive");
        assert_eq!(findings[0].severity, Severity::Info);
    }

    #[test]
    fn over_the_threshold_warns_with_the_total_and_plugin_count() {
        let findings = super::run(&ctx(vec![sadd("A.esp", 80), sadd("B.esp", 40)]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("120"), "80 + 40");
        assert!(findings[0].title.contains("2 plugins"));
    }

    #[test]
    fn no_records_reports_a_clean_info() {
        let findings = super::run(&ctx(Vec::new()));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
    }
}
