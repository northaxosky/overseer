//! Tests for owned deployment progress translation

use super::*;

use std::sync::mpsc::{TryRecvError, sync_channel};

use camino::Utf8PathBuf;

#[test]
fn file_events_own_their_relative_paths() {
    let (sender, receiver) = sync_channel(8);
    let sink = ChannelProgressSink::new(OperationKind::Deploy, sender);
    let mut relative = Utf8PathBuf::from("Textures/Original.dds");

    sink.on_event(ProgressEvent::Started { total: 1 });
    sink.on_event(ProgressEvent::Deployed {
        index: 0,
        relative: &relative,
    });
    relative.set_file_name("Changed.dds");

    assert!(matches!(
        receiver.recv().expect("start"),
        WorkerEvent::Started(1)
    ));
    assert!(matches!(
        receiver.recv().expect("phase"),
        WorkerEvent::Phase(OperationPhase::Deploying)
    ));
    assert!(matches!(
        receiver.recv().expect("file"),
        WorkerEvent::Deployed {
            index: 0,
            relative,
        } if relative == "Textures/Original.dds"
    ));
}

#[test]
fn full_channel_drops_file_telemetry_but_keeps_lifecycle_events() {
    let (sender, receiver) = sync_channel(1);
    let sink = ChannelProgressSink::new(OperationKind::Deploy, sender);

    sink.on_event(ProgressEvent::Started { total: 1 });
    assert!(matches!(
        receiver.recv().expect("lossless start"),
        WorkerEvent::Started(1)
    ));

    sink.on_event(ProgressEvent::Deployed {
        index: 0,
        relative: camino::Utf8Path::new("Textures/a.dds"),
    });
    assert!(matches!(
        receiver.recv().expect("lossless phase"),
        WorkerEvent::Phase(OperationPhase::Deploying)
    ));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

    sink.on_event(ProgressEvent::Finished);
    assert!(matches!(
        receiver.recv().expect("lossless finish"),
        WorkerEvent::Finished
    ));
}

#[test]
fn deploy_progress_supports_recovery_main_and_rollback_cycles() {
    let (sender, receiver) = sync_channel(32);
    let sink = ChannelProgressSink::new(OperationKind::Deploy, sender);
    let path = camino::Utf8Path::new("Meshes/a.nif");

    sink.on_event(ProgressEvent::Started { total: 1 });
    sink.on_event(ProgressEvent::Removed {
        index: 0,
        relative: path,
    });
    sink.on_event(ProgressEvent::Finished);
    sink.on_event(ProgressEvent::Started { total: 2 });
    sink.on_event(ProgressEvent::Deployed {
        index: 0,
        relative: path,
    });
    sink.on_event(ProgressEvent::Finished);
    sink.on_event(ProgressEvent::Started { total: 1 });
    sink.on_event(ProgressEvent::Removed {
        index: 0,
        relative: path,
    });

    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Started(1))));
    assert!(matches!(
        receiver.recv(),
        Ok(WorkerEvent::Phase(OperationPhase::Restoring))
    ));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Removed { .. })));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Finished)));
    assert!(matches!(
        receiver.recv(),
        Ok(WorkerEvent::Phase(OperationPhase::PlanningDeploy))
    ));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Started(2))));
    assert!(matches!(
        receiver.recv(),
        Ok(WorkerEvent::Phase(OperationPhase::Deploying))
    ));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Deployed { .. })));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Finished)));
    assert!(matches!(
        receiver.recv(),
        Ok(WorkerEvent::Phase(OperationPhase::Restoring))
    ));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Started(1))));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Removed { .. })));
}

#[test]
fn purge_removals_switch_to_purging_before_file_telemetry() {
    let (sender, receiver) = sync_channel(8);
    let sink = ChannelProgressSink::new(OperationKind::Purge, sender);

    sink.on_event(ProgressEvent::Started { total: 1 });
    sink.on_event(ProgressEvent::Removed {
        index: 0,
        relative: camino::Utf8Path::new("Scripts/a.pex"),
    });

    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Started(_))));
    assert!(matches!(
        receiver.recv(),
        Ok(WorkerEvent::Phase(OperationPhase::Purging))
    ));
    assert!(matches!(receiver.recv(), Ok(WorkerEvent::Removed { .. })));
}
