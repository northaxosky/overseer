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
mod tests {
    use super::*;
    use crate::test_support::temp;

    #[test]
    fn decode_args_are_force_decode_with_source() {
        let args = decode_args(
            Utf8Path::new("src.exe"),
            Utf8Path::new("patch.vcdiff"),
            Utf8Path::new("out.exe"),
        );
        assert_eq!(
            args,
            ["-d", "-f", "-s", "src.exe", "patch.vcdiff", "out.exe"].map(String::from)
        );
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error() {
        let decoder = Xdelta3CliDecoder::new("definitely-not-a-real-xdelta3-binary");
        let err = decoder
            .apply(Utf8Path::new("s"), Utf8Path::new("d"), Utf8Path::new("o"))
            .expect_err("a missing binary can't be spawned");
        assert!(matches!(err, DeltaError::Spawn { .. }));
    }

    #[test]
    fn crc32_file_matches_the_standard_vector() {
        let (_tmp, root) = temp();
        let path = root.join("check.bin");
        std::fs::write(&path, b"123456789").unwrap();
        // The canonical CRC-32/ISO-HDLC check value for "123456789"
        assert_eq!(crc32_file(&path).unwrap(), 0xCBF4_3926);
    }

    // A real xdelta3 round-trip, gated on `OVERSEER_XDELTA3` (CI has no xdelta3):; encode with the binary, decode through the trait, assert byte-exact
    #[test]
    fn xdelta3_round_trip_is_byte_exact() {
        let Ok(exe) = std::env::var("OVERSEER_XDELTA3") else {
            return;
        };
        let (_tmp, root) = temp();
        let (source, target) = (root.join("source.bin"), root.join("target.bin"));
        let (delta, dest) = (root.join("patch.vcdiff"), root.join("out.bin"));

        std::fs::write(&source, vec![7u8; 20_000]).unwrap();
        let mut t = vec![7u8; 20_000];
        t.splice(5_000..5_000, *b"OVERSEER"); // an insertion, so the delta is non-trivial
        std::fs::write(&target, &t).unwrap();

        let status = Command::new(&exe)
            .args([
                "-e",
                "-f",
                "-s",
                source.as_str(),
                target.as_str(),
                delta.as_str(),
            ])
            .status()
            .expect("run xdelta3 encode");
        assert!(status.success(), "encode should succeed");

        Xdelta3CliDecoder::new(exe)
            .apply(&source, &delta, &dest)
            .expect("decode should succeed");
        assert_eq!(
            crc32_file(&dest).unwrap(),
            crc32_file(&target).unwrap(),
            "decoded output must match the target byte-for-byte"
        );
    }
}
