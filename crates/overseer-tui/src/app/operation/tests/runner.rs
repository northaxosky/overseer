//! Tests for the single-job background runner

use super::*;

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver as GateReceiver, SyncSender as GateSender};

use camino::Utf8PathBuf;
use overseer_core::deploy::{ProgressEvent, ProgressSink};
use overseer_core::install::DownloadEntry;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Focus, Modal, Workspace};
use crate::test_support::download_entry;

struct OutputJob {
    entries: Vec<DownloadEntry>,
}

impl BackgroundJob for OutputJob {
    const KIND: OperationKind = OperationKind::RefreshDownloads;

    fn run(
        self,
        _context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::ListingDownloads);
        Ok(OperationOutput::RefreshDownloads(self.entries))
    }
}

struct MismatchedOutputJob;

impl BackgroundJob for MismatchedOutputJob {
    const KIND: OperationKind = OperationKind::Deploy;

    fn run(
        self,
        _context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        reporter.phase(OperationPhase::ListingDownloads);
        Ok(OperationOutput::RefreshDownloads(Vec::new()))
    }
}

struct GatedReadOnlyJob {
    ready: GateSender<()>,
    release: GateReceiver<()>,
}

impl BackgroundJob for GatedReadOnlyJob {
    const KIND: OperationKind = OperationKind::RefreshDownloads;

    fn run(
        self,
        _context: &OperationContext,
        _reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        self.ready.send(()).expect("test receives ready");
        self.release.recv().expect("test releases worker");
        Err(OperationFailure::new("test worker stopped"))
    }
}

struct GatedMutatingJob {
    ready: GateSender<()>,
    release: GateReceiver<()>,
}

impl BackgroundJob for GatedMutatingJob {
    const KIND: OperationKind = OperationKind::Deploy;

    fn run(
        self,
        _context: &OperationContext,
        _reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        self.ready.send(()).expect("test receives ready");
        self.release.recv().expect("test releases worker");
        Err(OperationFailure::new("test worker stopped"))
    }
}

struct PanicJob;

impl BackgroundJob for PanicJob {
    const KIND: OperationKind = OperationKind::RefreshDownloads;

    fn run(
        self,
        _context: &OperationContext,
        _reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        panic!("synthetic worker panic")
    }
}

struct SaturatingJob;

impl BackgroundJob for SaturatingJob {
    const KIND: OperationKind = OperationKind::RefreshDownloads;

    fn run(
        self,
        _context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        for index in 0..CHANNEL_CAPACITY * 2 {
            let phase = if index % 2 == 0 {
                OperationPhase::ListingDownloads
            } else {
                OperationPhase::ReadingSaves
            };
            reporter.phase(phase);
        }
        Ok(OperationOutput::RefreshDownloads(Vec::new()))
    }
}

struct SaturatingTelemetryJob {
    ready: GateSender<()>,
    release: GateReceiver<()>,
}

impl BackgroundJob for SaturatingTelemetryJob {
    const KIND: OperationKind = OperationKind::Deploy;

    fn run(
        self,
        _context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure> {
        let progress = reporter.progress_sink();
        let total = CHANNEL_CAPACITY * 2;
        progress.on_event(ProgressEvent::Started { total });
        for index in 0..total {
            progress.on_event(ProgressEvent::Deployed {
                index,
                relative: camino::Utf8Path::new("Textures/a.dds"),
            });
        }
        self.ready.send(()).expect("test receives saturation");
        self.release.recv().expect("test releases completion");
        progress.on_event(ProgressEvent::Finished);
        Ok(OperationOutput::Deploy {
            status: None,
            files: total,
        })
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl_c() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
}

fn start_read_only_gated(app: &mut App) -> GateSender<()> {
    let (ready_tx, ready_rx) = sync_channel(0);
    let (release_tx, release_rx) = sync_channel(0);
    app.start_operation(GatedReadOnlyJob {
        ready: ready_tx,
        release: release_rx,
    });
    ready_rx.recv().expect("worker starts");
    release_tx
}

fn start_mutating_gated(app: &mut App) -> GateSender<()> {
    let (ready_tx, ready_rx) = sync_channel(0);
    let (release_tx, release_rx) = sync_channel(0);
    app.start_operation(GatedMutatingJob {
        ready: ready_tx,
        release: release_rx,
    });
    ready_rx.recv().expect("worker starts");
    release_tx
}

fn release_and_finish(app: &mut App, release: GateSender<()>) {
    release.send(()).expect("release worker");
    app.finish_operation_after_terminal();
}

fn running_from_parts(
    kind: OperationKind,
    context: OperationContext,
    receiver: Receiver<WorkerEvent>,
    handle: JoinHandle<()>,
) -> OperationState {
    OperationState::Running(RunningOperation {
        context,
        view: OperationView::new(kind),
        receiver,
        handle: Some(handle),
        completion: None,
    })
}

#[test]
fn synthetic_job_runs_through_operation_agnostic_worker() {
    let context = OperationContext::capture(&App::sample().session);
    let request = WorkerRequest::new(
        context.clone(),
        OutputJob {
            entries: vec![download_entry("Mod.zip", 1, 2, false)],
        },
    );
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);

    run_worker(request, sender);

    assert!(matches!(
        receiver.recv().expect("phase"),
        WorkerEvent::Phase(OperationPhase::ListingDownloads)
    ));
    let WorkerEvent::Completion(completion) = receiver.recv().expect("completion") else {
        panic!("worker sends one completion")
    };
    assert_eq!(completion.context, context);
    assert!(matches!(
        completion.outcome,
        Ok(OperationOutput::RefreshDownloads(ref entries)) if entries.len() == 1
    ));
    assert!(
        receiver.recv().is_err(),
        "worker owns the only completion path"
    );
}
#[test]
fn mismatched_dispatch_and_output_kind_panics_before_completion() {
    let context = OperationContext::capture(&App::sample().session);
    let request = WorkerRequest::new(context, MismatchedOutputJob);
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);

    let result = catch_unwind(AssertUnwindSafe(|| run_worker(request, sender)));

    assert!(result.is_err(), "debug assertion rejects the mismatch");
    assert!(matches!(
        receiver.recv().expect("phase remains queued"),
        WorkerEvent::Phase(_)
    ));
    assert!(receiver.recv().is_err(), "no completion follows a mismatch");
}
#[test]
fn named_thread_is_installed_only_after_successful_spawn() {
    let mut app = App::sample();
    let release = start_read_only_gated(&mut app);
    let running = app.operation.running().expect("running state");
    assert_eq!(
        running
            .handle
            .as_ref()
            .and_then(|handle| handle.thread().name()),
        Some("overseer-downloads")
    );
    release_and_finish(&mut app, release);
}
#[test]
fn spawn_failure_never_enters_running() {
    let mut app = App::sample();
    let context = OperationContext::capture(&app.session);
    let (_sender, receiver) = sync_channel(CHANNEL_CAPACITY);

    app.accept_spawn_result(
        OperationKind::RefreshDownloads,
        context,
        receiver,
        Err(io::Error::other("synthetic spawn failure")),
    );

    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            succeeded: false,
            ref message,
            ..
        }) if message.contains("synthetic spawn failure")
    ));
}
#[test]
fn worker_panic_becomes_persistent_failure() {
    let mut app = App::sample();
    app.start_operation(PanicJob);

    app.finish_operation_after_terminal();

    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            succeeded: false,
            ref message,
            ..
        }) if message.contains("panicked")
    ));
}
#[test]
fn completion_is_discarded_when_join_panics() {
    let mut app = App::sample();
    let context = OperationContext::capture(&app.session);
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
    sender
        .send(WorkerEvent::Completion(Box::new(WorkerCompletion {
            context: context.clone(),
            outcome: Ok(OperationOutput::RefreshDownloads(vec![download_entry(
                "Ignored.zip",
                1,
                1,
                false,
            )])),
        })))
        .expect("queue completion");
    drop(sender);
    let handle = thread::spawn(|| panic!("panic after queued completion"));
    app.operation = running_from_parts(OperationKind::RefreshDownloads, context, receiver, handle);

    app.poll_operation();

    assert!(app.downloads.entries.is_empty(), "output was not accepted");
    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            ref message,
            ..
        }) if message.contains("panicked")
    ));
}
#[test]
fn clean_disconnect_without_completion_has_distinct_failure() {
    let mut app = App::sample();
    let context = OperationContext::capture(&app.session);
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
    drop(sender);
    let handle = thread::spawn(|| {});
    app.operation = running_from_parts(OperationKind::RefreshDownloads, context, receiver, handle);

    app.finish_operation_after_terminal();

    assert!(matches!(
        app.operation,
        OperationState::Completed(CompletedOperation {
            ref message,
            ..
        }) if message.contains("disconnected before completion")
    ));
}
#[test]
fn queued_completion_is_reduced_before_disconnect() {
    let mut app = App::sample();
    let context = OperationContext::capture(&app.session);
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
    sender
        .send(WorkerEvent::Phase(OperationPhase::ListingDownloads))
        .expect("queue phase");
    sender
        .send(WorkerEvent::Completion(Box::new(WorkerCompletion {
            context: context.clone(),
            outcome: Ok(OperationOutput::RefreshDownloads(vec![download_entry(
                "Applied.zip",
                1,
                1,
                false,
            )])),
        })))
        .expect("queue completion");
    drop(sender);
    let handle = thread::spawn(|| {});
    app.operation = running_from_parts(OperationKind::RefreshDownloads, context, receiver, handle);

    app.poll_operation();

    assert!(matches!(app.operation, OperationState::Idle));
    assert_eq!(app.downloads.entries[0].name, "Applied.zip");
}
#[test]
fn blocked_busy_key_table_covers_every_domain_action() {
    let blocked = [
        KeyCode::Char(' '),
        KeyCode::Enter,
        KeyCode::Char('J'),
        KeyCode::Char('K'),
        KeyCode::Char('R'),
        KeyCode::Char('A'),
        KeyCode::Char('x'),
        KeyCode::Delete,
        KeyCode::Char('X'),
        KeyCode::Char('L'),
        KeyCode::Char('D'),
        KeyCode::Char('P'),
        KeyCode::Char('l'),
        KeyCode::Char('p'),
        KeyCode::Char('s'),
        KeyCode::Char('d'),
        KeyCode::Char('r'),
    ];

    for code in blocked {
        let mut app = App::sample();
        let mods = app.session.profile.mods.clone();
        let plugins = app.session.order.plugins.clone();
        let release = start_read_only_gated(&mut app);

        app.handle_key(key(code));

        assert_eq!(app.session.profile.mods, mods, "{code:?} preserves mods");
        assert_eq!(
            app.session.order.plugins, plugins,
            "{code:?} preserves plugins"
        );
        assert!(app.modal.is_none(), "{code:?} opens no modal");
        assert!(
            app.message.as_ref().is_some_and(|notice| {
                notice.text.contains("running") || notice.text.contains("already")
            }),
            "{code:?} explains why it was blocked"
        );
        release_and_finish(&mut app, release);
    }
}
#[test]
fn navigation_focus_workspace_and_help_stay_available_while_busy() {
    for code in [KeyCode::Char('j'), KeyCode::Down] {
        let mut app = App::sample();
        app.mods.select(Some(0));
        let release = start_read_only_gated(&mut app);
        app.handle_key(key(code));
        assert_eq!(app.mods.index(), Some(1), "{code:?} moves down");
        release_and_finish(&mut app, release);
    }
    for code in [KeyCode::Char('k'), KeyCode::Up] {
        let mut app = App::sample();
        app.mods.select(Some(1));
        let release = start_read_only_gated(&mut app);
        app.handle_key(key(code));
        assert_eq!(app.mods.index(), Some(0), "{code:?} moves up");
        release_and_finish(&mut app, release);
    }

    let mut app = App::sample();
    let release = start_read_only_gated(&mut app);
    app.handle_key(key(KeyCode::Tab));
    assert_eq!(app.focus, Focus::Workspace);
    app.handle_key(key(KeyCode::Char('4')));
    assert_eq!(app.workspace, Workspace::Saves);
    app.handle_key(key(KeyCode::Char('[')));
    assert_eq!(app.workspace, Workspace::Downloads);
    app.handle_key(key(KeyCode::Char(']')));
    assert_eq!(app.workspace, Workspace::Saves);
    app.handle_key(key(KeyCode::Char('?')));
    assert!(matches!(app.modal, Some(Modal::Info(_))));
    app.handle_key(key(KeyCode::Char('j')));
    app.handle_key(key(KeyCode::Esc));
    assert!(app.modal.is_none(), "Help remains dismissible");
    release_and_finish(&mut app, release);
}
#[test]
fn all_workspace_digit_keys_remain_available_while_busy() {
    let cases = [
        ('1', Workspace::Plugins),
        ('2', Workspace::Conflicts),
        ('3', Workspace::Downloads),
        ('4', Workspace::Saves),
    ];
    for (digit, expected) in cases {
        let mut app = App::sample();
        app.workspace = Workspace::Saves;
        app.downloads.entries = vec![download_entry("Cached.zip", 1, 1, false)];
        let release = start_read_only_gated(&mut app);

        app.handle_key(key(KeyCode::Char(digit)));

        assert_eq!(app.workspace, expected, "{digit} switches workspace");
        assert_eq!(
            app.downloads.entries[0].name, "Cached.zip",
            "cached content remains"
        );
        release_and_finish(&mut app, release);
    }
}
#[test]
fn quit_keys_are_allowed_for_read_only_and_blocked_for_mutating_jobs() {
    let policies = [
        (OperationKind::Deploy, true),
        (OperationKind::Purge, true),
        (OperationKind::Install, true),
        (OperationKind::ScanConflicts, false),
        (OperationKind::Doctor, false),
        (OperationKind::RefreshSaves, false),
        (OperationKind::RefreshDownloads, false),
    ];
    for (kind, expected) in policies {
        assert_eq!(kind.is_mutating(), expected, "{kind:?} mutation policy");
    }

    for quit in [key(KeyCode::Char('q')), key(KeyCode::Esc), ctrl_c()] {
        let mut app = App::sample();
        let release = start_read_only_gated(&mut app);
        app.handle_key(quit);
        assert!(app.should_quit, "{quit:?} quits during read-only work");
        release_and_finish(&mut app, release);
    }

    for quit in [key(KeyCode::Char('q')), key(KeyCode::Esc), ctrl_c()] {
        let mut app = App::sample();
        let release = start_mutating_gated(&mut app);
        app.handle_key(quit);
        assert!(!app.should_quit, "{quit:?} is blocked during mutating work");
        assert!(
            app.message
                .as_ref()
                .is_some_and(|notice| notice.text.contains("running"))
        );
        release_and_finish(&mut app, release);
    }
}
#[test]
fn repeated_and_different_requests_are_not_queued() {
    let mut app = App::sample();
    app.workspace = Workspace::Downloads;
    let release = start_read_only_gated(&mut app);

    app.handle_key(key(KeyCode::Char('r')));
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("Refresh already running")
    );
    app.handle_key(key(KeyCode::Char('d')));
    assert!(
        app.message
            .as_ref()
            .is_some_and(|notice| notice.text.contains("Downloads refresh is running"))
    );
    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::RefreshDownloads)
    );
    release_and_finish(&mut app, release);
}
#[test]
fn spinner_ticks_without_touching_transient_message() {
    let mut app = App::sample();
    app.note("keep me");
    let release = start_read_only_gated(&mut app);

    app.tick_operation();

    assert_eq!(
        app.operation.running().map(|running| running.view.spinner),
        Some(1)
    );
    assert_eq!(
        app.message.as_ref().map(|notice| notice.text.as_str()),
        Some("keep me")
    );
    release_and_finish(&mut app, release);
}
#[test]
fn terminal_shutdown_drains_a_full_control_channel_before_join() {
    let mut app = App::sample();
    app.start_operation(SaturatingJob);

    app.finish_operation_after_terminal();

    assert!(matches!(app.operation, OperationState::Idle));
}

#[test]
fn saturated_file_telemetry_preserves_lifecycle_and_completion() {
    let context = OperationContext::capture(&App::sample().session);
    let (ready_sender, ready_receiver) = sync_channel(0);
    let (release_sender, release_receiver) = sync_channel(0);
    let request = WorkerRequest::new(
        context,
        SaturatingTelemetryJob {
            ready: ready_sender,
            release: release_receiver,
        },
    );
    let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
    let handle = thread::spawn(move || run_worker(request, sender));

    ready_receiver.recv().expect("worker saturates channel");
    let mut started = false;
    let mut deployed = 0;
    while let Ok(event) = receiver.try_recv() {
        match event {
            WorkerEvent::Started(_) => started = true,
            WorkerEvent::Deployed { .. } => deployed += 1,
            WorkerEvent::Phase(_) => {}
            WorkerEvent::Removed { .. } | WorkerEvent::Finished | WorkerEvent::Completion(_) => {
                panic!("worker is gated before terminal events")
            }
        }
    }
    assert!(started, "Started remains lossless");
    assert!(
        deployed < CHANNEL_CAPACITY * 2,
        "full-channel file events are dropped"
    );

    release_sender.send(()).expect("release worker");
    assert!(matches!(
        receiver.recv().expect("finish"),
        WorkerEvent::Finished
    ));
    assert!(matches!(
        receiver.recv().expect("completion"),
        WorkerEvent::Completion(_)
    ));
    handle.join().expect("worker joins");
}

fn event_running(kind: OperationKind) -> RunningOperation {
    let context = OperationContext::capture(&App::sample().session);
    let (_sender, receiver) = sync_channel(1);
    RunningOperation {
        context,
        view: OperationView::new(kind),
        receiver,
        handle: Some(thread::spawn(|| {})),
        completion: None,
    }
}

#[test]
fn every_started_event_resets_the_progress_cycle() {
    let mut running = event_running(OperationKind::Deploy);
    reduce_worker_event(&mut running, WorkerEvent::Started(3));
    reduce_worker_event(
        &mut running,
        WorkerEvent::Deployed {
            index: 1,
            relative: Utf8PathBuf::from("Textures/a.dds"),
        },
    );
    reduce_worker_event(&mut running, WorkerEvent::Finished);
    reduce_worker_event(&mut running, WorkerEvent::Started(0));

    let progress = running.view.progress.as_ref().expect("fresh progress");
    assert_eq!(progress.completed, 0);
    assert_eq!(progress.total, 0);
    assert!(progress.current.is_none());
    assert!(!progress.finished);
    running.handle.take().expect("handle").join().expect("join");
}

#[test]
fn file_events_clamp_completion_and_replace_the_current_path() {
    let mut running = event_running(OperationKind::Deploy);
    reduce_worker_event(&mut running, WorkerEvent::Started(2));
    reduce_worker_event(
        &mut running,
        WorkerEvent::Deployed {
            index: usize::MAX,
            relative: Utf8PathBuf::from("Textures/last.dds"),
        },
    );
    let progress = running.view.progress.as_ref().expect("progress");
    assert_eq!(progress.completed, 2);
    assert_eq!(
        progress.current.as_deref(),
        Some(camino::Utf8Path::new("Textures/last.dds"))
    );

    reduce_worker_event(
        &mut running,
        WorkerEvent::Removed {
            index: 0,
            relative: Utf8PathBuf::from("Meshes/first.nif"),
        },
    );
    let progress = running.view.progress.as_ref().expect("progress");
    assert_eq!(progress.completed, 1);
    assert_eq!(
        progress.current.as_deref(),
        Some(camino::Utf8Path::new("Meshes/first.nif"))
    );
    running.handle.take().expect("handle").join().expect("join");
}

#[test]
fn finished_marks_zero_total_progress_complete_without_division() {
    let mut running = event_running(OperationKind::Purge);
    reduce_worker_event(&mut running, WorkerEvent::Started(0));
    assert_eq!(
        running.view.progress.as_ref().expect("progress").fraction(),
        0.0
    );

    reduce_worker_event(&mut running, WorkerEvent::Finished);
    let progress = running.view.progress.as_ref().expect("progress");
    assert_eq!(progress.completed, 0);
    assert!(progress.finished);
    assert_eq!(progress.fraction(), 1.0);
    running.handle.take().expect("handle").join().expect("join");
}

#[test]
fn completed_state_survives_ordinary_keys_and_enter_dismisses_it() {
    let mut app = App::sample();
    let release = start_read_only_gated(&mut app);
    release_and_finish(&mut app, release);
    assert!(matches!(app.operation, OperationState::Completed(_)));

    app.handle_key(key(KeyCode::Char('j')));
    assert!(matches!(app.operation, OperationState::Completed(_)));
    app.handle_key(key(KeyCode::Enter));
    assert!(matches!(app.operation, OperationState::Idle));
}
#[test]
fn starting_a_new_job_replaces_completed_state() {
    let mut app = App::sample();
    app.operation = OperationState::Completed(CompletedOperation::failed(
        OperationKind::Deploy,
        "old failure",
    ));

    let release = start_read_only_gated(&mut app);

    assert_eq!(
        app.running_operation_kind(),
        Some(OperationKind::RefreshDownloads)
    );
    release_and_finish(&mut app, release);
}
