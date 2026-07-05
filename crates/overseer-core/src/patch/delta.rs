//! Applying binary deltas (xdelta3 / VCDIFF) behind a swappable [`DeltaDecoder`] backend

use crate::error::IoError;
use camino::{Utf8Path, Utf8PathBuf};
use std::process::Command;
use thiserror::Error;

/// Applies a VCDIFF `delta` to `source`, writing the reconstructed file to `dest`
pub trait DeltaDecoder {
    fn apply(&self, source: &Utf8Path, delta: &Utf8Path, dest: &Utf8Path)
    -> Result<(), DeltaError>;
}

/// Failure applying a delta
#[derive(Debug, Error)]
pub enum DeltaError {
    /// The decoder binary could not be started (wrong path, not executable, ...)
    #[error("could not run xdelta3 at `{path}`")]
    Spawn {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The decoder ran but exited non-zero (bad source, corrupt delta, ...)
    #[error("xdelta3 exited with {code:?}: {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error(transparent)]
    Io(#[from] IoError),
}

/// A [`DeltaDecoder`] that shells out to an `xdelta3` executable
#[derive(Debug, Clone)]
pub struct Xdelta3CliDecoder {
    exe: Utf8PathBuf,
}

impl Xdelta3CliDecoder {
    /// Use the `xdelta3` binary at `exe`
    pub fn new(exe: impl Into<Utf8PathBuf>) -> Self {
        Self { exe: exe.into() }
    }
}

/// Decode-run arguments: `-d -f -s <source> <delta> <dest>` (decode, force-overwrite, source)
fn decode_args(source: &Utf8Path, delta: &Utf8Path, dest: &Utf8Path) -> [String; 6] {
    [
        "-d".to_owned(),
        "-f".to_owned(),
        "-s".to_owned(),
        source.to_string(),
        delta.to_string(),
        dest.to_string(),
    ]
}

impl DeltaDecoder for Xdelta3CliDecoder {
    fn apply(
        &self,
        source: &Utf8Path,
        delta: &Utf8Path,
        dest: &Utf8Path,
    ) -> Result<(), DeltaError> {
        let output = Command::new(&self.exe)
            .args(decode_args(source, delta, dest))
            .output()
            .map_err(|source| DeltaError::Spawn {
                path: self.exe.clone(),
                source,
            })?;
        if !output.status.success() {
            return Err(DeltaError::Failed {
                code: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        Ok(())
    }
}

/// CRC32 of a file, read in a streaming fashion (for verifying multi-MB game files)
#[cfg(test)]
fn crc32_file(path: &Utf8Path) -> Result<u32, IoError> {
    let mut hasher = crc32fast::Hasher::new();
    crate::fs::read_chunks(path, |chunk| hasher.update(chunk))?;
    Ok(hasher.finalize())
}

#[cfg(test)]
#[path = "tests/delta.rs"]
mod tests;
