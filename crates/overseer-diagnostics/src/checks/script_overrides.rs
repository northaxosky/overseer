//! Loose `Data/Scripts/*.pex` from a mod that overrides a base F4SE script

use super::Check;
use crate::context::{GameContext, ScriptOverrideScan};
use crate::finding::{Finding, Severity};

/// Flags mods that override a base F4SE Papyrus script
pub struct ScriptOverrides;

impl Check for ScriptOverrides {
    fn id(&self) -> &'static str {
        "script-overrides"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        ctx.script_overrides.iter().map(warn).collect()
    }
}

/// A warning that a mod overrides a base F4SE script
fn warn(scan: &ScriptOverrideScan) -> Finding {
    Finding::new(
        Severity::Warning,
        format!(
            "`{}` (from `{}`) overrides a base F4SE script",
            scan.name, scan.mod_name
        ),
        Some(
            "This isn't the mod that provides F4SE's scripts, so it's replacing one of them — which \
             usually breaks F4SE unless the mod is built for your exact game version. If it isn't, \
             remove this file."
                .to_owned(),
        ),
    )
}

// ────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(name: &str, mod_name: &str) -> ScriptOverrideScan {
        ScriptOverrideScan {
            name: name.to_owned(),
            mod_name: mod_name.to_owned(),
        }
    }

    fn run(scans: Vec<ScriptOverrideScan>) -> Vec<Finding> {
        ScriptOverrides.run(&GameContext {
            script_overrides: scans,
            ..GameContext::default()
        })
    }

    #[test]
    fn an_override_warns_and_names_the_file_and_mod() {
        let findings = run(vec![scan("Actor.pex", "Some Mod")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("Actor.pex"));
        assert!(findings[0].title.contains("Some Mod"));
        assert!(findings[0].title.contains("base F4SE script"));
        assert!(findings[0].detail.as_deref().unwrap().contains("F4SE"));
    }

    #[test]
    fn every_override_is_flagged() {
        let findings = run(vec![scan("Game.pex", "A"), scan("Form.pex", "B")]);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.severity == Severity::Warning));
    }

    #[test]
    fn no_overrides_reports_nothing() {
        assert!(run(vec![]).is_empty());
    }
}
