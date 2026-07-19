//! Validation and application of typed worker results

use super::super::sort::{sort_downloads, sort_saves};
use super::super::{
    App, Confirm, ConfirmAction, ConflictsStatus, DoctorReport, LaunchSession, ListCursor, Modal,
    Session,
};
use super::protocol::{
    LifecycleState, OperationContext, OperationKind, OperationOutput, OperationRecovery,
    WorkerCompletion, WorkerEvent,
};
use super::runner::{CompletedOperation, OperationProgress, OperationState, RunningOperation};
use overseer_core::deploy::ConflictSnapshot;
use overseer_core::install::DownloadEntry;
use overseer_core::launch::PrepareOutcome;
use overseer_core::saves::SaveInfo;
use overseer_diagnostics::Report;

impl App {
    /// Validate and apply one typed worker completion
    pub(super) fn apply_completion(&mut self, kind: OperationKind, completion: WorkerCompletion) {
        let WorkerCompletion { context, outcome } = completion;
        let context_matches = self.context_matches(&context);
        let outcome = match outcome {
            Ok(OperationOutput::PrepareLaunch {
                outcome: PrepareOutcome::Launched { handle, marker },
                instance,
                tool_name,
                status,
                ..
            }) => {
                if context_matches {
                    self.session.status = status;
                }
                self.track_launch(LaunchSession::new(
                    handle,
                    tool_name,
                    context.profile,
                    instance,
                    marker,
                ));
                self.operation = OperationState::Idle;
                return;
            }
            outcome => outcome,
        };

        if !context_matches {
            tracing::warn!(
                captured_root = %context.instance_root,
                captured_profile = %context.profile,
                active_root = %self.session.instance.root,
                active_profile = %self.session.profile.name,
                "discarding stale background result"
            );

            self.operation = OperationState::Completed(CompletedOperation::failed(
                kind,
                "Discarded background result because the active session changed",
            ));
            return;
        }

        match outcome {
            Ok(output) => {
                debug_assert_eq!(kind, output.kind(), "job kind/output mismatch");

                match output {
                    OperationOutput::PrepareLaunch {
                        outcome: PrepareOutcome::NeedsRedeploy { reason, token },
                        tool_key,
                        tool_name,
                        status,
                        ..
                    } => {
                        self.session.status = status;
                        self.modal = Some(Modal::Confirm(Confirm {
                            message: format!("{reason}. Redeploy before launching {tool_name}?"),
                            action: ConfirmAction::Redeploy {
                                tool_key,
                                tool_name,
                                token,
                            },
                        }));
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::PrepareLaunch {
                        outcome: PrepareOutcome::NeedsRecovery { reason },
                        status,
                        ..
                    } => {
                        self.session.status = status;
                        self.operation = OperationState::Completed(CompletedOperation::failed(
                            kind,
                            format!("Launch needs recovery: {reason}"),
                        ));
                    }
                    OperationOutput::PrepareLaunch {
                        outcome: PrepareOutcome::Launched { .. },
                        ..
                    } => {
                        tracing::error!("launched outcome reached context-validated reducer");
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::RefreshDownloads(entries) => {
                        self.accept_downloads(entries);
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::RefreshSaves(entries) => {
                        self.accept_saves(entries);
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::ScanConflicts(found) => {
                        self.accept_conflicts(found);
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::Doctor(report) => {
                        self.open_completed_doctor(report);
                        self.operation = OperationState::Idle;
                    }
                    OperationOutput::Install { name, state } => {
                        self.complete_lifecycle(kind, &name, state, "Installed")
                    }
                    OperationOutput::Remove { name, state } => {
                        self.complete_lifecycle(kind, &name, state, "Removed")
                    }
                    OperationOutput::Replace { name, state } => {
                        self.complete_lifecycle(kind, &name, state, "Replaced")
                    }
                    OperationOutput::Deploy { status, files } => {
                        self.session.status = status;
                        self.operation = OperationState::Completed(CompletedOperation::succeeded(
                            kind,
                            format!("Deployed {files} files"),
                        ));
                    }
                    OperationOutput::Purge { status, outcome } => {
                        self.session.status = status;
                        self.operation = OperationState::Completed(CompletedOperation::succeeded(
                            kind,
                            purge_summary(&outcome),
                        ));
                    }
                }
            }
            Err(failure) => {
                let message = failure.display_message();

                match (kind, failure.recovery) {
                    (
                        OperationKind::PrepareLaunch | OperationKind::Deploy | OperationKind::Purge,
                        Some(OperationRecovery::DeploymentStatus(status)),
                    ) => {
                        self.session.status = status.map(|status| *status);
                    }
                    (
                        OperationKind::Install | OperationKind::Remove | OperationKind::Replace,
                        Some(OperationRecovery::Session(session)),
                    ) => {
                        self.accept_install_session(*session);
                    }
                    _ => {}
                }

                self.operation =
                    OperationState::Completed(CompletedOperation::failed(kind, message));
            }
        }
    }

    /// Apply a finished lifecycle result and report it with the given past-tense verb
    fn complete_lifecycle(
        &mut self,
        kind: OperationKind,
        name: &str,
        state: LifecycleState,
        verb: &str,
    ) {
        let message = match state {
            LifecycleState::Refreshed { session, downloads } => {
                self.accept_install_session(*session);
                self.accept_downloads(downloads);
                format!("{verb} {name}")
            }
            LifecycleState::CommittedWithResidue(path) => {
                format!("{verb} {name}; resolve pending residue at {path}")
            }
        };
        self.operation = OperationState::Completed(CompletedOperation::succeeded(kind, message));
    }

    /// Replace the conflict cache and select its first row
    fn accept_conflicts(&mut self, snapshot: ConflictSnapshot) {
        self.conflicts.filter = None;
        self.conflicts.status = ConflictsStatus::Ready(snapshot);
        let len = self.conflicts.visible_indices().len();
        self.conflicts.list.select_first(len);
    }

    /// Accept the only operation result allowed to replace the active session
    fn accept_install_session(&mut self, session: Session) {
        self.session = session;
        self.mods.reconcile_model(self.session.profile.rows());
        self.plugins
            .reconcile_model(&self.session.order.plugins, &self.session.plugin_separators);

        self.mark_conflicts_stale();
    }

    /// Replace any open informational modal with the completed Doctor report
    fn open_completed_doctor(&mut self, report: Report) {
        let list = ListCursor::first(report.findings.len());
        self.modal = Some(Modal::Doctor(DoctorReport { report, list }));
    }

    /// Check whether captured instance and profile identifiers remain active
    fn context_matches(&self, captured: &OperationContext) -> bool {
        self.session
            .instance
            .root
            .as_str()
            .eq_ignore_ascii_case(captured.instance_root.as_str())
            && self
                .session
                .profile
                .name
                .eq_ignore_ascii_case(&captured.profile)
    }

    /// Replace Downloads using current sorting and stable path selection
    fn accept_downloads(&mut self, mut entries: Vec<DownloadEntry>) {
        let previous_index = self.downloads.list.index();
        let selected_path = previous_index
            .and_then(|index| self.downloads.entries.get(index))
            .map(|entry| entry.path.clone());

        sort_downloads(&mut entries, self.settings.downloads_sort);

        let selection = selected_path
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| entry.path == *path))
            .or_else(|| previous_index.map(|index| index.min(entries.len().saturating_sub(1))))
            .or_else(|| (!entries.is_empty()).then_some(0));

        self.downloads.entries = entries;
        self.downloads.list.select(selection);
        self.downloads.list.clamp(self.downloads.entries.len());
    }

    /// Replace Saves using current sorting and stable path selection
    fn accept_saves(&mut self, mut entries: Vec<SaveInfo>) {
        let previous_index = self.saves.list.index();

        let selected_path = previous_index
            .and_then(|index| self.saves.entries.get(index))
            .map(|entry| entry.path.clone());

        sort_saves(&mut entries, self.settings.saves_sort);

        let selection = selected_path
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| entry.path == *path))
            .or_else(|| previous_index.map(|index| index.min(entries.len().saturating_sub(1))))
            .or_else(|| (!entries.is_empty()).then_some(0));

        self.saves.entries = entries;
        self.saves.list.select(selection);
        self.saves.list.clamp(self.saves.entries.len());
    }
}

fn purge_summary(outcome: &overseer_core::apply::ReversalOutcome) -> String {
    let mut summary = format!(
        "Purged: {} removed · {} restored · {} captured · {} preserved",
        outcome.removed.len(),
        outcome.restored.len(),
        outcome.captured.len(),
        outcome.preserved_conflicts.len()
    );
    if outcome.plugins_txt == overseer_core::restore::Restore::Conflict {
        summary.push_str(" · Plugins.txt preserved");
    }
    if outcome.save_redirect == overseer_core::restore::Restore::Conflict {
        summary.push_str(" · save redirect preserved");
    }
    summary
}

/// Reduce one worker event into running operation state
pub(super) fn reduce_worker_event(running: &mut RunningOperation, event: WorkerEvent) {
    match event {
        WorkerEvent::Phase(phase) => {
            running.view.phase = phase;
        }
        WorkerEvent::Started(total) => {
            running.view.progress = Some(OperationProgress::new(total));
        }
        WorkerEvent::Deployed { index, relative } | WorkerEvent::Removed { index, relative } => {
            if let Some(progress) = &mut running.view.progress {
                progress.completed = index.saturating_add(1).min(progress.total);
                progress.current = Some(relative);
            }
        }
        WorkerEvent::Finished => {
            if let Some(progress) = &mut running.view.progress {
                progress.completed = progress.total;
                progress.finished = true;
            }
        }
        WorkerEvent::Completion(completion) => {
            running.completion = Some(completion);
        }
    }
}

#[cfg(test)]
#[path = "tests/reducer.rs"]
mod tests;
