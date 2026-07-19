//! Tracked launch state outside the operation runner.

use super::{App, Notice, OperationKind};
use overseer_core::deploy::{DeployError, LaunchHandle};
use overseer_core::instance::Instance;
use overseer_core::launch;
use overseer_frontend::style::Role;
use std::fmt;

/// One launched process tree and its instance context.
pub(crate) struct LaunchSession {
    handle: Box<dyn LaunchHandle>,
    pub(crate) tool_name: String,
    pub(crate) profile: String,
    instance: Instance,
    marker: launch::LaunchMarker,
    error_reported: bool,
}

impl LaunchSession {
    /// Build a tracked session around a core launch handle.
    pub(crate) fn new(
        handle: Box<dyn LaunchHandle>,
        tool_name: String,
        profile: String,
        instance: Instance,
        marker: launch::LaunchMarker,
    ) -> Self {
        Self {
            handle,
            tool_name,
            profile,
            instance,
            marker,
            error_reported: false,
        }
    }
}

impl fmt::Debug for LaunchSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchSession")
            .field("tool_name", &self.tool_name)
            .field("profile", &self.profile)
            .field("instance", &self.instance.root)
            .field("error_reported", &self.error_reported)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Report whether this TUI is tracking a live game.
    pub(crate) fn game_running(&self) -> bool {
        self.launch.is_some()
    }

    /// Report whether the tracked launch belongs to an instance.
    pub(crate) fn tracks_launch_in(&self, instance: &Instance) -> bool {
        self.launch
            .as_ref()
            .is_some_and(|session| session.instance.root == instance.root)
    }

    /// Install a newly returned launch handle.
    pub(crate) fn track_launch(&mut self, session: LaunchSession) {
        self.launch = Some(session);
        self.launch_notice = None;
        self.message = None;
    }

    /// Poll the tracked process tree once.
    pub(crate) fn poll_launch(&mut self) -> bool {
        let result = match self.launch.as_mut() {
            Some(session) => session.handle.try_wait(),
            None => return false,
        };
        match result {
            Ok(None) => false,
            Ok(Some(status)) => {
                let session = self.launch.take().expect("polled launch remains present");
                let cleared = launch::clear_launch_marker_if(&session.instance, &session.marker);
                let (text, role) = match cleared {
                    Err(error) => (
                        format!(
                            "{} exited, but its launch marker could not be cleared: {error}",
                            session.tool_name
                        ),
                        Role::Failure,
                    ),
                    Ok(_) if status.success() => (
                        format!("{} session ended", session.tool_name),
                        Role::Heading,
                    ),
                    Ok(_) => (
                        format!("{} exited with {status}", session.tool_name),
                        Role::Failure,
                    ),
                };
                self.launch_notice = Some(Notice { text, role });
                self.message = None;
                true
            }
            Err(error) => self.report_launch_error(error),
        }
    }

    fn report_launch_error(&mut self, error: DeployError) -> bool {
        let Some(session) = self.launch.as_mut() else {
            return false;
        };
        if session.error_reported {
            return false;
        }
        session.error_reported = true;
        self.launch_notice = Some(Notice {
            text: format!(
                "Could not query {}: {error}; purge remains blocked",
                session.tool_name
            ),
            role: Role::Failure,
        });
        self.message = None;
        true
    }

    /// Stop tracking without killing the game or clearing its marker.
    pub(crate) fn detach_launch(&mut self) {
        if let Some(session) = self.launch.take() {
            session.handle.detach();
        }
    }

    /// Return the persistent launch line for the footer.
    pub(crate) fn launch_status(&self) -> Option<Notice> {
        self.launch.as_ref().map_or_else(
            || self.launch_notice.clone(),
            |session| {
                Some(Notice {
                    text: format!(
                        "Playing: {} · profile {}",
                        session.tool_name, session.profile
                    ),
                    role: Role::Heading,
                })
            },
        )
    }

    /// Dismiss a completed launch notice.
    pub(crate) fn dismiss_launch_notice(&mut self) -> bool {
        self.launch_notice.take().is_some()
    }

    /// Explain why an operation cannot run during play.
    pub(crate) fn note_game_running(&mut self, requested: OperationKind) {
        let tool = self
            .launch
            .as_ref()
            .map(|session| session.tool_name.as_str())
            .unwrap_or("the game");
        self.note(format!(
            "Cannot {} while {tool} is running",
            requested.label().to_lowercase()
        ));
    }

    /// Block a synchronous play-unsafe action.
    pub(crate) fn block_while_playing(&mut self, action: &str) -> bool {
        let Some(session) = self.launch.as_ref() else {
            return false;
        };
        let tool = session.tool_name.clone();
        self.note(format!("Cannot {action} while {tool} is running"));
        true
    }

    /// Clear the marker confirmed by the startup prompt.
    pub(crate) fn clear_stale_launch_marker(&mut self) {
        match launch::clear_launch_marker(&self.session.instance) {
            Ok(true) => self.ok("Cleared stale launch marker"),
            Ok(false) => self.note("Launch marker was already clear"),
            Err(error) => self.fail(format!("Could not clear launch marker: {error}")),
        }
    }
}

#[cfg(test)]
#[path = "tests/launch.rs"]
mod tests;
