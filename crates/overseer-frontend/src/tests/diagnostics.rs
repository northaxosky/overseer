//! Tests for shared diagnostic presentation

use super::*;

#[test]
fn severities_map_to_shared_roles_and_glyphs() {
    assert_eq!(
        severity_presentation(Severity::Info),
        SeverityPresentation {
            role: Role::Success,
            glyph: "✓",
        }
    );
    assert_eq!(
        severity_presentation(Severity::Warning),
        SeverityPresentation {
            role: Role::Warning,
            glyph: "!",
        }
    );
    assert_eq!(
        severity_presentation(Severity::Error),
        SeverityPresentation {
            role: Role::Failure,
            glyph: "✗",
        }
    );
}
