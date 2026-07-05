//! Active full/light plugin counts vs the engine limits

use crate::context::GameContext;
use crate::finding::{Finding, Severity};

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

/// One finding for a plugin tier: error over the limit, warn within ~5%, else info
fn count_finding(label: &str, count: usize, limit: usize) -> Finding {
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
    let f = Finding::new(severity, format!("{label} plugins: {count} / {limit}"));
    match detail {
        Some(d) => f.detail(d),
        None => f,
    }
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use overseer_core::plugins::PluginMeta;

    fn plugin(is_light: bool) -> PluginMeta {
        overseer_core::test_support::plugin_meta(
            if is_light { "Light.esl" } else { "Full.esp" },
            false,
            is_light,
            &[],
        )
    }

    fn ctx(full: usize, light: usize) -> GameContext {
        let mut loaded = vec![plugin(false); full];
        loaded.extend(vec![plugin(true); light]);
        GameContext {
            loaded_plugins: loaded,
            ..GameContext::default()
        }
    }

    #[test]
    fn within_limits_is_info() {
        let findings = super::run(&ctx(10, 10));
        assert!(findings.iter().all(|f| f.severity == Severity::Info));
        assert!(findings[0].title.contains("10 / 254"));
        assert!(findings[1].title.contains("10 / 4096"));
    }

    #[test]
    fn over_the_full_limit_is_an_error() {
        let findings = super::run(&ctx(255, 0));
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].title.contains("255 / 254"));
    }

    #[test]
    fn approaching_the_full_limit_warns() {
        // 254 - 254/20 = 242 is the warning threshold
        let findings = super::run(&ctx(245, 0));
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn light_plugins_count_against_the_light_limit() {
        let findings = super::run(&ctx(0, 4097));
        assert_eq!(findings[0].severity, Severity::Info, "no full plugins");
        assert_eq!(findings[1].severity, Severity::Error);
        assert!(findings[1].title.contains("4097 / 4096"));
    }
}
