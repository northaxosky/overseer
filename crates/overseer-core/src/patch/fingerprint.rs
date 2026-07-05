//! Game-agnostic binary verification: measure a file and gate it against known-good hashes.

use crate::error::IoError;
use crate::fs::size_opt;
use camino::Utf8Path;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// The measured identity of a file on disk
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub size: u64,
    pub crc32: u32,
    pub sha256: String,
}

/// The known-good identity a file is expected to have
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedFingerprint {
    pub size: u64,
    pub crc32: u32,
    /// Full-strength gate; `None` falls back to CRC32 until the hash is recorded
    pub sha256: Option<&'static str>,
}

/// Which hash tier actually cleared a verification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedBy {
    Sha256,
    Crc32,
}

impl VerifiedBy {
    /// Human-readable label for plans and logs
    pub fn label(self) -> &'static str {
        match self {
            Self::Sha256 => "SHA-256",
            Self::Crc32 => "CRC32 (SHA-256 pending)",
        }
    }
}

impl ExpectedFingerprint {
    /// The strongest gate this identity can currently enforce
    pub fn verified_by(self) -> VerifiedBy {
        if self.sha256.is_some() {
            VerifiedBy::Sha256
        } else {
            VerifiedBy::Crc32
        }
    }

    /// Verify `file` against this identity, reporting which hash tier cleared it
    pub fn verify(self, file: &FileFingerprint) -> Option<VerifiedBy> {
        if self.size != file.size {
            return None;
        }
        match self.sha256 {
            Some(sha256) if file.sha256.eq_ignore_ascii_case(sha256) => Some(VerifiedBy::Sha256),
            None if file.crc32 == self.crc32 => Some(VerifiedBy::Crc32),
            _ => None,
        }
    }

    /// Whether `file` matches this identity at any available tier
    pub fn matches(self, file: &FileFingerprint) -> bool {
        self.verify(file).is_some()
    }
}

/// Lowercase hex encoding of a digest
fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Measure `path`'s size, CRC32 and SHA-256 in one read pass; `None` if it does not exist
pub fn fingerprint_file(path: &Utf8Path) -> Result<Option<FileFingerprint>, IoError> {
    let Some(size) = size_opt(path)? else {
        return Ok(None);
    };
    let mut crc = crc32fast::Hasher::new();
    let mut sha = Sha256::new();
    crate::fs::read_chunks(path, |chunk| {
        crc.update(chunk);
        sha.update(chunk);
    })?;
    Ok(Some(FileFingerprint {
        size,
        crc32: crc.finalize(),
        sha256: hex_digest(sha.finalize()),
    }))
}

#[cfg(test)]
#[path = "tests/fingerprint.rs"]
mod tests;
