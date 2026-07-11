//! Ignored pure-Rust VCDIFF adapter acceptance against the Nuka-World textures delta

use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::patch::delta::{DeltaDecoder, RustDeltaDecoder};
use sha1::{Digest as _, Sha1};
use sha2::Sha256;
use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use tempfile::Builder;

const SOURCE_ENV: &str = "OVERSEER_VCDIFF_NUKAWORLD_SOURCE";
const DELTA_ENV: &str = "OVERSEER_VCDIFF_NUKAWORLD_DELTA";
const WORK_DIR_ENV: &str = "OVERSEER_VCDIFF_WORK_DIR";
const EXPECTED_TARGET_ENV: &str = "OVERSEER_VCDIFF_NUKAWORLD_EXPECTED_TARGET";

const SOURCE_SIZE: u64 = 2_348_553_100;
const SOURCE_SHA1: &str = "38f0a5cd198c5096a87f32164d0d42c2bd9eb457";
const DELTA_SIZE: u64 = 1_108_023_470;
const DELTA_SHA256: &str = "902ecac497b4a53fc8b9594a2004362a5bc2c074c04b490bd40c39009a5d8abb";
const TARGET_SIZE: u64 = 1_794_590_010;
const TARGET_CRC32: u32 = 0xB9FD_1CD6;
const TARGET_SHA256: &str = "0083e633cfdc9fc34882e41e795f8d16093440197c953d4a68b963fabf62446b";
const BUFFER_SIZE: usize = 64 * 1024;

struct Inputs {
    source: Utf8PathBuf,
    delta: Utf8PathBuf,
    work_dir: Utf8PathBuf,
    expected_target: Option<Utf8PathBuf>,
}

struct Fingerprint {
    size: u64,
    crc32: u32,
    sha256: String,
}

/// Build an invalid-data error with a concise acceptance failure
fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Encode a digest as lowercase hexadecimal
fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut hex = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Read one optional UTF-8 path from the environment
fn env_path(name: &str) -> io::Result<Option<Utf8PathBuf>> {
    let Some(value) = env::var_os(name) else {
        return Ok(None);
    };
    Utf8PathBuf::from_path_buf(PathBuf::from(value))
        .map(Some)
        .map_err(|_| invalid_data(format!("{name} must be a UTF-8 path")))
}

/// Resolve the required environment trio or skip when all three are absent
fn inputs_or_skip() -> io::Result<Option<Inputs>> {
    let _ = dotenvy::dotenv();
    let source = env_path(SOURCE_ENV)?;
    let delta = env_path(DELTA_ENV)?;
    let work_dir = env_path(WORK_DIR_ENV)?;
    let expected_target = env_path(EXPECTED_TARGET_ENV)?;

    match (source, delta, work_dir) {
        (None, None, None) => {
            eprintln!(
                "skipping Nuka-World VCDIFF adapter acceptance: required environment is unset"
            );
            Ok(None)
        }
        (Some(source), Some(delta), Some(work_dir)) => Ok(Some(Inputs {
            source,
            delta,
            work_dir,
            expected_target,
        })),
        _ => Err(invalid_data(format!(
            "set {SOURCE_ENV}, {DELTA_ENV}, and {WORK_DIR_ENV} together"
        ))),
    }
}

/// Stream one file into SHA-1 while retaining its measured size
fn sha1_identity(path: &Utf8Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = Sha1::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok((size, hex_digest(hasher.finalize())))
}

/// Stream one file into SHA-256 while retaining its measured size
fn sha256_identity(path: &Utf8Path) -> io::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok((size, hex_digest(hasher.finalize())))
}

/// Verify the source identity before any delta or output access
fn verify_source(path: &Utf8Path) -> io::Result<()> {
    let (size, sha1) = sha1_identity(path)?;
    if size != SOURCE_SIZE {
        return Err(invalid_data(format!(
            "source size mismatch: expected {SOURCE_SIZE}, got {size}"
        )));
    }
    if sha1 != SOURCE_SHA1 {
        return Err(invalid_data(format!(
            "source SHA-1 mismatch: expected {SOURCE_SHA1}, got {sha1}"
        )));
    }
    Ok(())
}

/// Verify the extracted plain delta before creating adapter output
fn verify_delta(path: &Utf8Path) -> io::Result<()> {
    let (size, sha256) = sha256_identity(path)?;
    if size != DELTA_SIZE {
        return Err(invalid_data(format!(
            "delta size mismatch: expected {DELTA_SIZE}, got {size}"
        )));
    }
    if sha256 != DELTA_SHA256 {
        return Err(invalid_data(format!(
            "delta SHA-256 mismatch: expected {DELTA_SHA256}, got {sha256}"
        )));
    }
    Ok(())
}

/// Stream one decoded target into size, CRC32, and SHA-256
fn fingerprint(path: &Utf8Path) -> io::Result<Fingerprint> {
    let mut file = File::open(path)?;
    let mut crc32 = crc32fast::Hasher::new();
    let mut sha256 = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        crc32.update(&buffer[..count]);
        sha256.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| invalid_data("target size overflow"))?;
    }
    Ok(Fingerprint {
        size,
        crc32: crc32.finalize(),
        sha256: hex_digest(sha256.finalize()),
    })
}

/// Require the independently recorded target identity
fn verify_target(label: &str, fingerprint: &Fingerprint) -> io::Result<()> {
    if fingerprint.size != TARGET_SIZE {
        return Err(invalid_data(format!(
            "{label} size mismatch: expected {TARGET_SIZE}, got {}",
            fingerprint.size
        )));
    }
    if fingerprint.crc32 != TARGET_CRC32 {
        return Err(invalid_data(format!(
            "{label} CRC32 mismatch: expected {TARGET_CRC32:08X}, got {:08X}",
            fingerprint.crc32
        )));
    }
    if fingerprint.sha256 != TARGET_SHA256 {
        return Err(invalid_data(format!(
            "{label} SHA-256 mismatch: expected {TARGET_SHA256}, got {}",
            fingerprint.sha256
        )));
    }
    Ok(())
}

/// Compare two verified targets with bounded buffers
fn compare_files(left: &Utf8Path, right: &Utf8Path) -> io::Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    left.seek(SeekFrom::Start(0))?;
    right.seek(SeekFrom::Start(0))?;
    let mut left_buffer = vec![0_u8; BUFFER_SIZE];
    let mut right_buffer = vec![0_u8; BUFFER_SIZE];
    let mut remaining = TARGET_SIZE;
    while remaining > 0 {
        let count = usize::try_from(remaining.min(BUFFER_SIZE as u64))
            .map_err(|_| invalid_data("comparison size exceeds usize"))?;
        left.read_exact(&mut left_buffer[..count])?;
        right.read_exact(&mut right_buffer[..count])?;
        if left_buffer[..count] != right_buffer[..count] {
            return Ok(false);
        }
        remaining -= count as u64;
    }
    Ok(true)
}

/// Decode and verify the real Nuka-World textures delta through Overseer's adapter
#[test]
#[ignore = "requires external Nuka-World source and delta"]
fn nukaworld_textures_matches_oracle() -> Result<(), Box<dyn Error>> {
    let Some(inputs) = inputs_or_skip()? else {
        return Ok(());
    };
    if !inputs.work_dir.is_dir() {
        return Err(invalid_data(format!("{WORK_DIR_ENV} must name an existing directory")).into());
    }

    verify_source(&inputs.source)?;
    verify_delta(&inputs.delta)?;

    let owned = Builder::new()
        .prefix("overseer-vcdiff-nukaworld-")
        .tempdir_in(inputs.work_dir.as_std_path())?;
    let owned_path = Utf8PathBuf::from_path_buf(owned.path().to_owned())
        .map_err(|_| invalid_data("owned work directory must be UTF-8"))?;
    let output = owned_path.join("DLCNukaWorld - Textures.ba2");

    RustDeltaDecoder::new(TARGET_SIZE).apply(&inputs.source, &inputs.delta, &output)?;
    let output_fingerprint = fingerprint(&output)?;
    verify_target("decoded target", &output_fingerprint)?;

    if let Some(expected) = inputs.expected_target {
        let expected_fingerprint = fingerprint(&expected)?;
        verify_target("expected target", &expected_fingerprint)?;
        if !compare_files(&output, &expected)? {
            return Err(invalid_data("decoded target differs from the expected target").into());
        }
    }

    eprintln!(
        "Nuka-World VCDIFF adapter matched {TARGET_SIZE} bytes, CRC32 {TARGET_CRC32:08X}, SHA-256 {TARGET_SHA256}"
    );
    owned.close()?;
    Ok(())
}
