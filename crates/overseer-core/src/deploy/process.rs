//! The launched child process, tracked to exit.

use super::{DeployError, LaunchHandle, LaunchTarget};
use camino::Utf8PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};

/// Spawn a target and return a handle that tracks it to exit.
pub(super) fn spawn(target: &LaunchTarget) -> Result<Box<dyn LaunchHandle>, DeployError> {
    let child = Command::new(target.program.as_std_path())
        .current_dir(target.working_dir.as_std_path())
        .args(&target.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| launch_error(target, source))?;
    Ok(Box::new(ChildHandle {
        child,
        program: target.program.clone(),
    }))
}

fn launch_error(target: &LaunchTarget, source: std::io::Error) -> DeployError {
    DeployError::Launch {
        program: target.program.clone(),
        source,
    }
}

/// A launched process, polled to completion without blocking.
struct ChildHandle {
    child: Child,
    program: Utf8PathBuf,
}

impl LaunchHandle for ChildHandle {
    fn try_wait(&mut self) -> Result<Option<ExitStatus>, DeployError> {
        self.child.try_wait().map_err(|source| DeployError::Wait {
            program: self.program.clone(),
            source,
        })
    }

    fn detach(self: Box<Self>) {}
}
