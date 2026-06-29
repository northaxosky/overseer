//! F4SE health: the loader must match the game's runtime, and deployed F4SE plugins need
//! the matching Address Library.

use super::Check;
use crate::context::{AddressLibraryStatus, GameContext};
use crate::finding::{Finding, Severity};

/// Reports F4SE setup problems: a loader for the wrong runtime, or a missing Address Library
pub struct F4se;

impl Check for F4se {
    fn id(&self) -> &'static str {
        "f4se"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        // A loader for the wrong runtime fails to launch the game. Only flag when both the game
        // and loader families are known and disagree.
        if let (Some(game), Some(loader)) = (ctx.runtime_family, ctx.loader_family)
            && game != loader
        {
            findings.push(Finding::new(
                Severity::Error,
                format!("F4SE is for {loader:?} but the game is {game:?}"),
                Some("F4SE won't launch; install the F4SE build for your game version".to_owned()),
            ));
        }

        if let AddressLibraryStatus::Missing { expected } = &ctx.address_library {
            findings.push(Finding::new(
                Severity::Warning,
                format!("Address Library `{expected}` is missing"),
                Some("F4SE plugins need it; install Address Library (Nexus mod 47327)".to_owned()),
            ));
        }

        findings
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use overseer_core::detect::RuntimeFamily;

    fn run(
        game: Option<RuntimeFamily>,
        loader: Option<RuntimeFamily>,
        address: AddressLibraryStatus,
    ) -> Vec<Finding> {
        F4se.run(&GameContext {
            runtime_family: game,
            loader_family: loader,
            address_library: address,
            ..GameContext::default()
        })
    }

    #[test]
    fn a_matching_loader_is_silent() {
        let findings = run(
            Some(RuntimeFamily::OldGen),
            Some(RuntimeFamily::OldGen),
            AddressLibraryStatus::NotApplicable,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn a_mismatched_loader_errors() {
        let findings = run(
            Some(RuntimeFamily::OldGen),
            Some(RuntimeFamily::NextGen),
            AddressLibraryStatus::NotApplicable,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Error);
        assert!(findings[0].title.contains("NextGen"));
        assert!(findings[0].title.contains("OldGen"));
    }

    #[test]
    fn an_unknown_family_does_not_warn() {
        assert!(
            run(
                None,
                Some(RuntimeFamily::NextGen),
                AddressLibraryStatus::NotApplicable
            )
            .is_empty()
        );
        assert!(
            run(
                Some(RuntimeFamily::NextGen),
                None,
                AddressLibraryStatus::NotApplicable
            )
            .is_empty()
        );
    }

    #[test]
    fn a_missing_address_library_warns() {
        let findings = run(
            Some(RuntimeFamily::OldGen),
            Some(RuntimeFamily::OldGen),
            AddressLibraryStatus::Missing {
                expected: "version-1-10-163-0.bin".to_owned(),
            },
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("version-1-10-163-0.bin"));
    }

    #[test]
    fn a_present_address_library_is_silent() {
        assert!(
            run(
                Some(RuntimeFamily::OldGen),
                Some(RuntimeFamily::OldGen),
                AddressLibraryStatus::Present
            )
            .is_empty()
        );
    }
}
