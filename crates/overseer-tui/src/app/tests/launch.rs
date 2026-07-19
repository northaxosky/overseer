//! Tests for tracked launch state.

use super::*;
use crate::app::operation::{DeployJob, RefreshDownloadsJob};
use overseer_core::deploy::{DeployError, LaunchHandle};
use std::collections::VecDeque;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

enum Step {
    Running,
    Exited(ExitStatus),
    Error,
}

struct FakeHandle {
    steps: VecDeque<Step>,
    detached: Arc<AtomicBool>,
}

impl FakeHandle {
    fn new(steps: impl IntoIterator<Item = Step>) -> (Self, Arc<AtomicBool>) {
        let detached = Arc::new(AtomicBool::new(false));
        (
            Self {
                steps: steps.into_iter().collect(),
                detached: Arc::clone(&detached),
            },
            detached,
        )
    }
}

impl LaunchHandle for FakeHandle {
    fn try_wait(&mut self) -> Result<Option<ExitStatus>, DeployError> {
        match self.steps.pop_front().unwrap_or(Step::Running) {
            Step::Running => Ok(None),
            Step::Exited(status) => Ok(Some(status)),
            Step::Error => Err(DeployError::Wait {
                program: "fake.exe".into(),
                source: std::io::Error::other("scripted wait failure"),
            }),
        }
    }

    fn detach(self: Box<Self>) {
        self.detached.store(true, Ordering::SeqCst);
    }
}

fn success_status() -> ExitStatus {
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
}

fn track(app: &mut App, steps: impl IntoIterator<Item = Step>) -> Arc<AtomicBool> {
    let (handle, detached) = FakeHandle::new(steps);
    app.track_launch(LaunchSession::new(
        Box::new(handle),
        "Fallout 4".to_owned(),
        "Default".to_owned(),
        app.session.instance.clone(),
        overseer_core::launch::LaunchMarker {
            launch_id: 1,
            tool: "Fallout 4".to_owned(),
            profile: "Default".to_owned(),
            timestamp: 1,
            launcher_pid: std::process::id(),
        },
    ));
    detached
}

#[test]
fn poll_keeps_running_then_surfaces_a_durable_exit() {
    let mut app = App::sample();
    track(&mut app, [Step::Running, Step::Exited(success_status())]);

    assert!(!app.poll_launch());
    assert!(app.game_running());
    assert!(app.poll_launch());
    assert!(!app.game_running());
    assert!(
        app.launch_notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("session ended"))
    );
    app.note("temporary");
    app.message = None;
    assert!(app.launch_status().is_some(), "exit notice remains durable");
    assert!(app.dismiss_launch_notice());
    assert!(app.launch_status().is_none());
}

#[test]
fn wait_errors_keep_the_gate_and_report_only_once() {
    let mut app = App::sample();
    track(&mut app, [Step::Error, Step::Error]);

    assert!(app.poll_launch());
    let first = app
        .launch_notice
        .as_ref()
        .map(|notice| notice.text.clone())
        .expect("error notice");
    assert!(!app.poll_launch());

    assert!(app.game_running());
    assert_eq!(
        app.launch_notice.as_ref().map(|notice| &notice.text),
        Some(&first)
    );
}

#[test]
fn play_unsafe_jobs_are_blocked_but_read_only_jobs_still_run() {
    let mut app = App::sample();
    track(&mut app, [Step::Running]);

    app.start_operation(DeployJob);
    assert!(!app.operation_running());
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.contains("Cannot deploy"))
    );

    app.start_operation(RefreshDownloadsJob);
    assert!(app.operation_running());
    app.finish_operation_after_terminal();
}

#[test]
fn play_unsafe_keys_are_blocked_before_opening_a_modal() {
    use crate::app::input::test_helpers::key;
    use ratatui::crossterm::event::KeyCode;

    let mut app = App::sample();
    track(&mut app, [Step::Running]);

    app.handle_main_key(key(KeyCode::Char('P')));

    assert!(app.modal.is_none());
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.contains("Cannot purge"))
    );
}

#[test]
fn synchronous_play_unsafe_actions_note_without_an_operation() {
    let mut app = App::sample();
    track(&mut app, [Step::Running]);

    assert!(app.block_while_playing("delete saves"));
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.contains("Cannot delete saves"))
    );
    assert!(!app.operation_running());
}

#[test]
fn play_unsafe_policy_covers_every_mutating_game_operation() {
    for kind in [
        OperationKind::Deploy,
        OperationKind::Purge,
        OperationKind::Install,
        OperationKind::Remove,
        OperationKind::Replace,
    ] {
        assert!(kind.is_play_unsafe(), "{kind:?}");
    }
    for kind in [
        OperationKind::ScanConflicts,
        OperationKind::Doctor,
        OperationKind::RefreshSaves,
        OperationKind::RefreshDownloads,
    ] {
        assert!(!kind.is_play_unsafe(), "{kind:?}");
    }
}

#[test]
fn quit_detaches_without_clearing_the_marker() {
    use crate::app::input::test_helpers::key;
    use ratatui::crossterm::event::KeyCode;

    let mut app = App::sample();
    let marker = overseer_core::launch::launch_marker_path(&app.session.instance);
    std::fs::create_dir_all(marker.parent().expect("marker parent")).expect("marker parent");
    std::fs::write(&marker, b"active").expect("marker");
    let detached = track(&mut app, [Step::Running]);

    app.handle_main_key(key(KeyCode::Char('q')));

    assert!(app.should_quit);
    assert!(!app.game_running());
    assert!(detached.load(Ordering::SeqCst));
    assert!(marker.exists(), "detach leaves the safety marker");
}

#[test]
fn exit_does_not_clear_a_newer_sessions_marker() {
    let mut app = App::sample();
    let marker = overseer_core::launch::launch_marker_path(&app.session.instance);
    std::fs::create_dir_all(marker.parent().expect("marker parent")).expect("marker parent");
    let newer_pid = std::process::id();
    std::fs::write(
        &marker,
        format!(
            r#"{{"launch_id":2,"tool":"Fallout 4","profile":"Default","timestamp":2,"launcher_pid":{newer_pid}}}"#
        ),
    )
    .expect("marker");
    track(&mut app, [Step::Exited(success_status())]);

    assert!(app.poll_launch());

    assert!(marker.exists(), "foreign marker remains");
}
