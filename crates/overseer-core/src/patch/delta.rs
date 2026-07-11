//! Applying VCDIFF deltas behind a swappable [`DeltaDecoder`] backend

use camino::{Utf8Path, Utf8PathBuf};
use std::fs::{File, OpenOptions};
use thiserror::Error;
use vcdiff_rs::DecodeOptions;

/// Applies a VCDIFF `delta` to `source`, writing the reconstructed file to `dest`
pub trait DeltaDecoder {
    #[allow(
        clippy::result_large_err,
        reason = "preserves structured decoder context"
    )]
    fn apply(&self, source: &Utf8Path, delta: &Utf8Path, dest: &Utf8Path)
    -> Result<(), DeltaError>;
}

/// Failure applying a delta
#[derive(Debug, Error)]
pub enum DeltaError {
    /// The source file could not be opened
    #[error("could not open delta source `{path}`")]
    OpenSource {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The VCDIFF file could not be opened
    #[error("could not open VCDIFF delta `{path}`")]
    OpenDelta {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The destination file could not be created
    #[error("could not create delta output `{path}`")]
    CreateDestination {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The VCDIFF stream could not be decoded
    #[error(
        "could not decode VCDIFF `{delta_path}` with source `{source_path}` into `{dest_path}`"
    )]
    Decode {
        source_path: Utf8PathBuf,
        delta_path: Utf8PathBuf,
        dest_path: Utf8PathBuf,
        #[source]
        source: vcdiff_rs::DecodeError,
    },
}

/// A path-based pure-Rust VCDIFF decoder
#[derive(Debug)]
pub struct RustDeltaDecoder {
    options: DecodeOptions,
}

impl RustDeltaDecoder {
    /// Decode with an explicit cumulative target-size limit
    pub fn new(max_target_size: u64) -> Self {
        let mut options = DecodeOptions::default();
        options.max_target_size = max_target_size;
        Self { options }
    }
}

impl DeltaDecoder for RustDeltaDecoder {
    /// Decode one VCDIFF file into a newly created destination
    fn apply(
        &self,
        source: &Utf8Path,
        delta: &Utf8Path,
        dest: &Utf8Path,
    ) -> Result<(), DeltaError> {
        let mut source_file = File::open(source).map_err(|error| DeltaError::OpenSource {
            path: source.to_owned(),
            source: error,
        })?;

        let mut delta_file = File::open(delta).map_err(|error| DeltaError::OpenDelta {
            path: delta.to_owned(),
            source: error,
        })?;

        let mut dest_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(dest)
            .map_err(|error| DeltaError::CreateDestination {
                path: dest.to_owned(),
                source: error,
            })?;

        vcdiff_rs::decode_to(
            &mut source_file,
            &mut delta_file,
            &mut dest_file,
            &self.options,
        )
        .map_err(|error| DeltaError::Decode {
            source_path: source.to_owned(),
            delta_path: delta.to_owned(),
            dest_path: dest.to_owned(),
            source: error,
        })
    }
}

#[cfg(test)]
#[path = "tests/delta.rs"]
mod tests;
