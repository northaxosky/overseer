//! Active full/light plugin counts vs the engine limits

use super::Check;
use crate::context::GameContext;
use crate::finding::{Finding, Severity};

/// Fallout 4's hard limits on simultaneously-loaded plugins
const MAX_FULL: usize = 254;
const MAX_LIGHT: usize = 4096;

/// Counts active Full and Light (ESL) plugins against the engine limits
pub struct PluginCount;

impl Check for PluginCount {
    fn id(&self) -> &'static str {
        "plugin-count"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let light = ctx.active_plugins.iter().filter(|p| p.is_light).count();
        let full = ctx.active_plugins.len() - light;
        vec![
            self.count_finding("Full (ESM/ESP)", full, MAX_FULL),
            self.count_finding("Light (ESL)", light, MAX_LIGHT),
        ]
    }
}

impl PluginCount {
    /// One finding for a plugin tier: error over the limit, warn within ~5%, else info
    fn count_finding(&self, label: &str, count: usize, limit: usize) -> Finding {
        let (severity, detail) = if count > limit {
            (
                Severity::Error,
                Some("Over the limit — the game won't start"),
            )
        } else if count >= limit - limit / 20 {
            (Severity::Warning, Some("Approaching the limit"))
        } else {
            (Severity::Info, None)
        };
        Finding {
            check: self.id(),
            severity,
            title: format!("{label} plugins: {count} / {limit}"),
            detail: detail.map(String::from),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use overseer_core::plugins::PluginMeta;
    use std::collections::BTreeSet;

    fn plugin(is_light: bool) -> PluginMeta {
        PluginMeta {
            name: if is_light { "Light.esl" } else { "Full.esp" }.to_owned(),
            is_master: false,
            is_light,
            masters: Vec::new(),
        }
    }

    fn ctx(full: usize, light: usize) -> GameContext {
        let mut active_plugins = vec![plugin(false); full];
        active_plugins.extend(vec![plugin(true); light]);
        GameContext {
            active_plugins,
            present_plugins: BTreeSet::new(),
            data_files: Vec::new(),
        }
    }

    #[test]
    fn within_limits_is_info() {
        let findings = PluginCount.run(&ctx(10, 10));
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
        assert!(findings[0].title.contains("10 / 254"));
        assert!(findings[1].title.contains("10 / 4096"));
    }

    #[test]
    fn over_the_full_limit_is_an_error() {
        let findings = PluginCount.run(&ctx(255, 0));
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].title.contains("255 / 254"));
    }

    #[test]
    fn approaching_the_full_limit_warns() {
        // 254 - 254/20 = 242 is the warning threshold.
        let findings = PluginCount.run(&ctx(245, 0));
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn light_plugins_count_against_the_light_limit() {
        let findings = PluginCount.run(&ctx(0, 4097));
        assert_eq!(findings[0].severity, Severity::Info, "no full plugins");
        assert_eq!(findings[1].severity, Severity::Error);
        assert!(findings[1].title.contains("4097 / 4096"));
    }
}
