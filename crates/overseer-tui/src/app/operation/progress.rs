//! Owned progress translation from core deployment events to worker messages

use super::protocol::{OperationKind, OperationPhase, WorkerEvent};
use overseer_core::deploy::{ProgressEvent, ProgressSink};
use std::cell::Cell;
use std::sync::mpsc::{SyncSender, TrySendError};

pub(super) struct ChannelProgressSink {
    kind: OperationKind,
    sender: SyncSender<WorkerEvent>,
    phase: Cell<OperationPhase>,
    deployment_began: Cell<bool>,
}

impl ChannelProgressSink {
    /// Create a same-thread progress adapter for one deployment operation
    pub(super) fn new(kind: OperationKind, sender: SyncSender<WorkerEvent>) -> Self {
        Self {
            kind,
            sender,
            phase: Cell::new(OperationPhase::initial(kind)),
            deployment_began: Cell::new(false),
        }
    }

    /// Send a phase transition once without dropping it under channel pressure
    fn transition(&self, phase: OperationPhase) {
        if self.phase.replace(phase) == phase {
            return;
        }

        self.send_lossless(WorkerEvent::Phase(phase), "phase");
    }

    /// Send lifecycle telemetry without dropping it under channel pressure
    fn send_lossless(&self, event: WorkerEvent, label: &str) {
        if self.sender.send(event).is_err() {
            tracing::debug!(
                event = label,
                "operation receiver closed before progress update"
            );
        }
    }

    /// Attempt file telemetry without delaying the deployment worker
    fn try_send_file(&self, event: WorkerEvent) {
        match self.sender.try_send(event) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                tracing::debug!("operation receiver closed before file progress update");
            }
        }
    }
}

impl ProgressSink for ChannelProgressSink {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Started { total } => {
                let phase = match self.kind {
                    OperationKind::Deploy if self.deployment_began.get() => {
                        OperationPhase::Restoring
                    }
                    kind => OperationPhase::initial(kind),
                };

                self.transition(phase);
                self.send_lossless(WorkerEvent::Started(total), "start");
            }
            ProgressEvent::Deployed { index, relative } => {
                self.transition(OperationPhase::Deploying);
                self.deployment_began.set(true);

                self.try_send_file(WorkerEvent::Deployed {
                    index,
                    relative: relative.to_owned(),
                });
            }
            ProgressEvent::Removed { index, relative } => {
                let phase = match self.kind {
                    OperationKind::Deploy => OperationPhase::Restoring,
                    OperationKind::Purge => OperationPhase::Purging,
                    kind => OperationPhase::initial(kind),
                };

                self.transition(phase);
                self.try_send_file(WorkerEvent::Removed {
                    index,
                    relative: relative.to_owned(),
                });
            }
            ProgressEvent::Finished => {
                self.send_lossless(WorkerEvent::Finished, "finish");
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/progress.rs"]
mod tests;
