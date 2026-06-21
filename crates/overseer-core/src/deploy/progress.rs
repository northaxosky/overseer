//! Progress tracking for deploy/undeploy operations.

use camino::Utf8Path;

/// A progress event emitted during deploy/undeploy
#[derive(Debug)]
pub enum ProgressEvent<'a> {
    Started {
        total: usize,
    },
    Deployed {
        index: usize,
        relative: &'a Utf8Path,
    },
    Removed {
        index: usize,
        relative: &'a Utf8Path,
    },
    Finished,
}

/// Sink for progress events
pub trait ProgressSink {
    fn on_event(&self, event: ProgressEvent<'_>);
}

/// A [`ProgressSink`] that discards everything
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl ProgressSink for NullSink {
    fn on_event(&self, _event: ProgressEvent<'_>) {}
}
