//! Generic thread, channel, and operation lifecycle machinery

use std::mem;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::thread::{self, JoinHandle};

use super::super::App;
use super::protocol::{
    OperationContext, OperationFailure, OperationKind, OperationOutput, OperationPhase,
    WorkerCompletion, WorkerEvent,
};

const CHANNEL_CAPACITY: usize = 64;

pub(crate) trait BackgroundJob: Send + 'static {
    const KIND: OperationKind;

    /// Execute this job using owned context and worker reporting
    fn run(
        self,
        context: &OperationContext,
        reporter: &OperationReporter,
    ) -> Result<OperationOutput, OperationFailure>;
}

pub(crate) struct WorkerRequest<J: BackgroundJob> {
    context: OperationContext,
    job: J,
}

impl<J: BackgroundJob> WorkerRequest<J> {
    /// Create a request for one concrete background job
    pub(crate) fn new(context: OperationContext, job: J) -> Self {
        Self { context, job }
    }

    /// Return the captured operation context
    pub(crate) fn context(&self) -> &OperationContext {
        &self.context
    }
}

pub(crate) struct OperationReporter {
    sender: SyncSender<WorkerEvent>,
}

impl OperationReporter {
    /// Create a reporter over the worker event channel
    fn new(sender: SyncSender<WorkerEvent>) -> Self {
        Self { sender }
    }

    /// Send a lossless phase update to the UI
    pub(crate) fn phase(&self, phase: OperationPhase) {
        if self.sender.send(WorkerEvent::Phase(phase)).is_err() {
            tracing::debug!("operation receiver closed before phase update");
        }
    }
}

/// Execute one request and send at most one completion
fn run_worker<J: BackgroundJob>(request: WorkerRequest<J>, sender: SyncSender<WorkerEvent>) {
    let kind = J::KIND;
    let WorkerRequest { context, job } = request;
    let reporter = OperationReporter::new(sender.clone());
    let outcome = job.run(&context, &reporter);

    if let Ok(output) = &outcome {
        debug_assert_eq!(kind, output.kind(), "job kind/output mismatch");
    }

    let completion = WorkerCompletion { context, outcome };
    if sender.send(WorkerEvent::Completion(completion)).is_err() {
        tracing::debug!("operation receiver closed before completion");
    }
}

#[derive(Debug)]
pub(crate) struct OperationView {
    pub(crate) kind: OperationKind,
    pub(crate) phase: OperationPhase,
    pub(crate) spinner: usize,
}

impl OperationView {
    /// Create the initial rendering state for an operation
    fn new(kind: OperationKind) -> Self {
        Self {
            kind,
            phase: OperationPhase::initial(kind),
            spinner: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RunningOperation {
    context: OperationContext,
    pub(crate) view: OperationView,
    receiver: Receiver<WorkerEvent>,
    handle: Option<JoinHandle<()>>,
    completion: Option<WorkerCompletion>,
}

#[derive(Debug)]
pub(crate) struct CompletedOperation {
    pub(crate) kind: OperationKind,
    pub(crate) succeeded: bool,
    pub(crate) message: String,
}

impl CompletedOperation {
    /// Build a persistent failed completion
    pub(super) fn failed(kind: OperationKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            succeeded: false,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) enum OperationState {
    #[default]
    Idle,
    Running(RunningOperation),
    Completed(CompletedOperation),
}

impl OperationState {
    /// Return the active operation when a worker is running
    pub(crate) fn running(&self) -> Option<&RunningOperation> {
        match self {
            Self::Running(running) => Some(running),
            Self::Idle | Self::Completed(_) => None,
        }
    }

    /// Report whether an operation kind is currently running
    pub(crate) fn is_running_kind(&self, kind: OperationKind) -> bool {
        self.running()
            .is_some_and(|running| running.view.kind == kind)
    }
}

impl App {
    /// Report whether any background operation is running
    pub(crate) fn operation_running(&self) -> bool {
        self.operation.running().is_some()
    }

    /// Return the active operation kind
    pub(crate) fn running_operation_kind(&self) -> Option<OperationKind> {
        self.operation.running().map(|running| running.view.kind)
    }

    /// Start one owned job on a named background thread
    pub(crate) fn start_operation<J: BackgroundJob>(&mut self, job: J) {
        let kind = J::KIND;
        if self.operation_running() {
            self.note_blocked_operation(kind);
            return;
        }
        let request = WorkerRequest::new(OperationContext::capture(&self.session), job);
        let context = request.context().clone();
        let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);

        let result = thread::Builder::new()
            .name(format!("overseer-{}", kind.thread_label()))
            .spawn(move || run_worker(request, sender));

        self.accept_spawn_result(kind, context, receiver, result);
    }

    /// Enter Running only after the worker thread spawns successfully
    fn accept_spawn_result(
        &mut self,
        kind: OperationKind,
        context: OperationContext,
        receiver: Receiver<WorkerEvent>,
        result: std::io::Result<JoinHandle<()>>,
    ) {
        match result {
            Ok(handle) => {
                self.operation = OperationState::Running(RunningOperation {
                    context,
                    view: OperationView::new(kind),
                    receiver,
                    handle: Some(handle),
                    completion: None,
                });
            }
            Err(error) => {
                self.operation = OperationState::Completed(CompletedOperation::failed(
                    kind,
                    format!("Could not start worker: {error}"),
                ));
            }
        }
    }

    /// Explain why another operation cannot start
    pub(crate) fn note_blocked_operation(&mut self, requested: OperationKind) {
        let Some(active) = self.running_operation_kind() else {
            return;
        };

        if active == requested {
            if requested.is_refresh() {
                self.note("Refresh already running");
            } else {
                self.note(format!("{} already running", requested.label()));
            }
        } else {
            self.note(format!(
                "{} is running; try again when it finishes",
                active.label()
            ));
        }
    }

    /// Explain why an ordinary action is blocked
    pub(crate) fn note_busy(&mut self) {
        let Some(active) = self.running_operation_kind() else {
            return;
        };

        self.note(format!(
            "{} is running; try again when it finishes",
            active.label()
        ));
    }

    /// Dismiss a persistent completion result
    pub(crate) fn dismiss_completed_operation(&mut self) -> bool {
        if matches!(self.operation, OperationState::Completed(_)) {
            self.operation = OperationState::Idle;
            return true;
        }

        false
    }

    /// Advance the running operation spinner
    pub(crate) fn tick_operation(&mut self) {
        if let OperationState::Running(running) = &mut self.operation {
            running.view.spinner = running.view.spinner.wrapping_add(1);
        }
    }

    /// Drain worker events and finalize a terminal worker state
    pub(crate) fn poll_operation(&mut self) -> bool {
        let mut changed = false;
        let mut disconnected = false;

        {
            let OperationState::Running(running) = &mut self.operation else {
                return false;
            };

            loop {
                match running.receiver.try_recv() {
                    Ok(event) => {
                        changed = true;
                        reduce_worker_event(running, event);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        let terminal = match &self.operation {
            OperationState::Running(running) => running.completion.is_some() || disconnected,
            OperationState::Idle | OperationState::Completed(_) => false,
        };

        if terminal {
            self.finish_running(disconnected);
            changed = true;
        }

        changed
    }

    /// Drain and join a worker after terminal restoration
    pub(crate) fn finish_operation_after_terminal(&mut self) {
        loop {
            let received = {
                let OperationState::Running(running) = &self.operation else {
                    return;
                };

                running.receiver.recv()
            };

            match received {
                Ok(event) => {
                    if let OperationState::Running(running) = &mut self.operation {
                        reduce_worker_event(running, event);

                        if running.completion.is_some() {
                            self.finish_running(false);
                            return;
                        }
                    }
                }
                Err(_) => {
                    self.finish_running(true);
                    return;
                }
            }
        }
    }

    /// Join the worker and apply its captured completion
    fn finish_running(&mut self, disconnected: bool) {
        let OperationState::Running(mut running) =
            mem::replace(&mut self.operation, OperationState::Idle)
        else {
            return;
        };

        let handle = running.handle.take();
        let joined = handle.is_some_and(|handle| handle.join().is_ok());

        if !joined {
            self.operation = OperationState::Completed(CompletedOperation::failed(
                running.view.kind,
                "Background worker panicked",
            ));
            return;
        }

        let Some(completion) = running.completion else {
            let message = if disconnected {
                "Background worker disconnected before completion"
            } else {
                "Background worker ended without completion"
            };

            self.operation =
                OperationState::Completed(CompletedOperation::failed(running.view.kind, message));
            return;
        };

        debug_assert_eq!(running.context, completion.context);
        self.apply_completion(running.view.kind, completion);
    }
}

/// Reduce one worker event into the running operation state
fn reduce_worker_event(running: &mut RunningOperation, event: WorkerEvent) {
    match event {
        WorkerEvent::Phase(phase) => running.view.phase = phase,
        WorkerEvent::Completion(completion) => {
            running.completion = Some(completion);
        }
    }
}

#[cfg(test)]
#[path = "tests/runner.rs"]
mod tests;
