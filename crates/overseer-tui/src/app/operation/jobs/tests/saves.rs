//! Tests for the background Saves refresh

use super::*;

use overseer_core::instance::Instance;
use overseer_core::test_support::{self, temp_instance};

use crate::app::{App, OperationKind, OperationState};

#[test]
fn refresh_job_parses_save_headers_on_the_worker() {
    let (_tmp, scaffold) = temp_instance();
    let instance = Instance::init(scaffold.root.clone(), scaffold.config.clone())
        .expect("initialize instance");
    let dir = instance.saves_dir("Default").expect("saves directory");
    test_support::write_fos(
        &dir.join("Worker.fos"),
        7,
        "Nora",
        23,
        "The Castle",
        "Day 12",
    );
    let mut app = App::sample();
    app.session.instance = instance;

    app.start_operation(RefreshSavesJob);

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::RefreshSaves)
    );
    assert!(
        app.saves.entries.is_empty(),
        "worker output is not applied synchronously"
    );
    app.finish_operation_after_terminal();

    let meta = app.saves.entries[0].meta.as_ref().expect("parsed header");
    assert_eq!(meta.save_number, 7);
    assert_eq!(meta.character, "Nora");
    assert_eq!(meta.level, 23);
    assert_eq!(meta.location, "The Castle");
    assert_eq!(meta.game_date, "Day 12");
    assert!(matches!(app.operation, OperationState::Idle));
}
