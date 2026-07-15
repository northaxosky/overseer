//! Tests for background conflict scanning

use super::*;

use overseer_core::test_support::{self, TestbedSpec};

use crate::app::{App, ConflictsStatus, OperationKind, OperationState};

#[test]
fn scan_job_reloads_the_profile_and_preserves_provider_priority() {
    let (_temp, root) = test_support::temp();
    let spec = TestbedSpec::new()
        .managed("Winner", true, |m| {
            m.loose("Textures/shared.dds", b"winner")
        })
        .managed("Loser", true, |m| m.loose("Textures/shared.dds", b"loser"))
        .managed("Disabled", false, |m| {
            m.loose("Textures/shared.dds", b"disabled")
        });
    let instance = test_support::build_testbed(&root, &spec);
    instance.save().expect("persist instance");
    let mut app = App::sample();
    app.session.instance = instance;

    app.start_operation(ScanConflictsJob);

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::ScanConflicts)
    );
    assert!(
        matches!(app.conflicts.status, ConflictsStatus::Stale),
        "worker output is not applied synchronously"
    );
    app.finish_operation_after_terminal();

    let ConflictsStatus::Ready(found) = &app.conflicts.status else {
        panic!("conflict output reaches the reducer: {:?}", app.operation)
    };
    assert_eq!(found.len(), 1);
    let providers: Vec<&str> = found.conflicts()[0]
        .providers
        .iter()
        .map(|p| p.origin.display_name())
        .collect();
    assert_eq!(providers, ["Loser", "Winner"]);
    assert!(matches!(app.operation, OperationState::Idle));
}
