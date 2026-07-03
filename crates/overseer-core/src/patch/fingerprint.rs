//! Game-agnostic binary verification: measure a file and gate it against known-good hashes.

use crate::error::{IoError, io_err};
use crate::fs::size_opt;
use crate::patch::delta::crc32_file;
use camino::Utf8Path;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::Read;

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

/// Stream `path` through SHA-256 and return the lowercase hex digest
pub fn sha256_file(path: &Utf8Path) -> Result<String, IoError> {
    let mut file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Measure `path`'s size, CRC32 and SHA-256; `None` if it does not exist
pub fn fingerprint_file(path: &Utf8Path) -> Result<Option<FileFingerprint>, IoError> {
    let Some(size) = size_opt(path)? else {
        return Ok(None);
    };
    Ok(Some(FileFingerprint {
        size,
        crc32: crc32_file(path)?,
        sha256: sha256_file(path)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp;

    const SHA_GATED: ExpectedFingerprint = ExpectedFingerprint {
        size: 3,
        crc32: 0x1234_5678,
        sha256: Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
    };
    const CRC_GATED: ExpectedFingerprint = ExpectedFingerprint {
        size: 3,
        crc32: 0x1234_5678,
        sha256: None,
    };

    fn file(size: u64, crc32: u32, sha256: &str) -> FileFingerprint {
        FileFingerprint {
            size,
            crc32,
            sha256: sha256.to_owned(),
        }
    }

    #[test]
    fn sha256_file_matches_known_vector() {
        let (_tmp, root) = temp();
        let path = root.join("check.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn fingerprint_file_is_none_for_a_missing_path() {
        let (_tmp, root) = temp();
        assert!(fingerprint_file(&root.join("nope.bin")).unwrap().is_none());
    }

    #[test]
    fn sha_tier_clears_when_the_hash_matches_even_if_crc32_differs() {
        let f = file(
            3,
            0x0000_0000,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        );
        assert_eq!(SHA_GATED.verify(&f), Some(VerifiedBy::Sha256));
        assert_eq!(SHA_GATED.verified_by(), VerifiedBy::Sha256);
    }

    #[test]
    fn sha_gated_identity_rejects_a_crc32_collision() {
        // The point of SHA-256: a same-size, same-CRC32 file with the wrong hash must not verify.
        let forged = file(3, 0x1234_5678, &"00".repeat(32));
        assert_eq!(SHA_GATED.verify(&forged), None);
        assert!(!SHA_GATED.matches(&forged));
    }

    #[test]
    fn crc_tier_clears_only_when_no_hash_is_known() {
        let f = file(3, 0x1234_5678, "irrelevant");
        assert_eq!(CRC_GATED.verify(&f), Some(VerifiedBy::Crc32));
        assert_eq!(CRC_GATED.verified_by(), VerifiedBy::Crc32);
    }

    #[test]
    fn a_size_mismatch_never_verifies() {
        let f = file(4, 0x1234_5678, "irrelevant");
        assert_eq!(CRC_GATED.verify(&f), None);
    }
}
