//! Minimal VCDIFF header parsing for delta auto-mapping

use crate::error::{IoError, io_err, walk_io_err};
use crate::fs::read_dir_opt;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::HashMap;
use thiserror::Error;

const VCD_DECOMPRESS: u8 = 0x01;
const VCD_CODETABLE: u8 = 0x02;
const VCD_APPHEADER: u8 = 0x04;

/// Something went wrong reading a delta or mapping it to a known target file
#[derive(Debug, Error)]
pub enum VcdiffError {
    #[error("VCDIFF file `{path}` is too short")]
    TooShort { path: Utf8PathBuf },

    #[error("VCDIFF file `{path}` has a bad magic header")]
    BadMagic { path: Utf8PathBuf },

    #[error("VCDIFF file `{path}` has a malformed header")]
    Malformed { path: Utf8PathBuf },

    #[error(
        "delta `{path}` does not contain an application-header basename; pass an explicit delta flag"
    )]
    MissingAppHeaderName { path: Utf8PathBuf },

    #[error("delta `{path}` application header names a target outside the allowed set")]
    OffScopeTarget { path: Utf8PathBuf },

    #[error("delta `{path}` application header names more than one known target: {names}")]
    AmbiguousAppHeader { path: Utf8PathBuf, names: String },

    #[error("more than one delta maps to {name}: `{first}` and `{second}`")]
    DuplicateBinary {
        name: String,
        first: Utf8PathBuf,
        second: Utf8PathBuf,
    },

    #[error("the target file set has duplicate basenames: {names}")]
    AmbiguousCatalog { names: String },

    #[error(transparent)]
    Io(#[from] IoError),
}

/// The result of mapping a delta directory against an allowed file set
#[derive(Debug, Default)]
pub struct DeltaMap {
    /// Deltas that map onto an allowed target, keyed by that target's rel-path
    pub mapped: HashMap<String, Utf8PathBuf>,
    /// Valid deltas whose app-header names no allowed target (e.g. a mixed pack's off-scope deltas)
    pub ignored: Vec<Utf8PathBuf>,
}

/// Bytes scanned for an application header while bounding reads of multi-GB delta bodies
const HEADER_SCAN_LEN: u64 = 64 * 1024;

/// Read the VCDIFF application-header string from `path`, if it carries one
pub fn app_header(path: &Utf8Path) -> Result<Option<String>, VcdiffError> {
    let bytes = read_prefix(path, HEADER_SCAN_LEN).map_err(|e| io_err(path, e))?;
    parse_app_header(path, &bytes)
}

/// Read up to `max` leading bytes of `path`
fn read_prefix(path: &Utf8Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)?.take(max).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Map every delta under `dir` (recursively) against `allowed`, splitting on- and off-scope deltas
pub fn map_deltas(dir: &Utf8Path, allowed: &[&str]) -> Result<DeltaMap, VcdiffError> {
    assert_unique_basenames(allowed)?;
    let mut out = DeltaMap::default();
    if read_dir_opt(dir)?.is_none() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| walk_io_err(dir, e))?;
        let path = Utf8PathBuf::from_path_buf(entry.into_path()).map_err(|p| {
            io_err(
                dir,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("non-utf8 path: {}", p.display()),
                ),
            )
        })?;
        if !is_delta_file(&path) {
            continue;
        }
        match header_target(&path, allowed)? {
            Some(name) => {
                if let Some(first) = out.mapped.insert(name.clone(), path.clone()) {
                    return Err(VcdiffError::DuplicateBinary {
                        name,
                        first,
                        second: path,
                    });
                }
            }
            None => out.ignored.push(path),
        }
    }
    Ok(out)
}

/// The allowed rel-path a single delta's app header names; errors when it names none
pub fn target_from_header(path: &Utf8Path, allowed: &[&str]) -> Result<String, VcdiffError> {
    match header_target(path, allowed)? {
        Some(name) => Ok(name),
        None => Err(VcdiffError::OffScopeTarget {
            path: path.to_owned(),
        }),
    }
}

/// The allowed rel-path a delta names, `None` for a valid header that names no allowed target
fn header_target(path: &Utf8Path, allowed: &[&str]) -> Result<Option<String>, VcdiffError> {
    let Some(header) = app_header(path)? else {
        return Err(VcdiffError::MissingAppHeaderName {
            path: path.to_owned(),
        });
    };
    let matches = matching_catalog_files(&header, allowed);
    match matches.as_slice() {
        [name] => Ok(Some(name.clone())),
        [] => Ok(None),
        many => Err(VcdiffError::AmbiguousAppHeader {
            path: path.to_owned(),
            names: many.join(", "),
        }),
    }
}

/// Fail if two allowed files share a basename, which would make app-header mapping ambiguous
fn assert_unique_basenames(allowed: &[&str]) -> Result<(), VcdiffError> {
    let mut seen = std::collections::HashSet::new();
    let mut dups = Vec::new();
    for &rel in allowed {
        let base = basename(rel).to_ascii_lowercase();
        if !seen.insert(base.clone()) {
            dups.push(base);
        }
    }
    if dups.is_empty() {
        Ok(())
    } else {
        Err(VcdiffError::AmbiguousCatalog {
            names: dups.join(", "),
        })
    }
}

/// Parse the application-header string from raw VCDIFF `bytes` per RFC 3284
fn parse_app_header(path: &Utf8Path, bytes: &[u8]) -> Result<Option<String>, VcdiffError> {
    if bytes.len() < 5 {
        return Err(VcdiffError::TooShort {
            path: path.to_owned(),
        });
    }
    // VCDIFF magic: 0xD6C3C4 ("VCD" with high bits set) or plain "VCD", then v0
    if !matches!(
        &bytes[..4],
        [0xD6, 0xC3, 0xC4, 0x00] | [b'V', b'C', b'D', 0x00],
    ) {
        return Err(VcdiffError::BadMagic {
            path: path.to_owned(),
        });
    }
    let mut idx: usize = 5;
    let indicator = bytes[4];
    if indicator & VCD_DECOMPRESS != 0 {
        idx = idx.checked_add(1).ok_or_else(|| malformed(path))?;
    }
    if indicator & VCD_CODETABLE != 0 {
        let len = read_varint(bytes, &mut idx).ok_or_else(|| malformed(path))?;
        idx = idx.checked_add(len).ok_or_else(|| malformed(path))?;
    }
    if indicator & VCD_APPHEADER == 0 {
        return Ok(None);
    }
    let len = read_varint(bytes, &mut idx).ok_or_else(|| malformed(path))?;
    let end = idx.checked_add(len).ok_or_else(|| malformed(path))?;
    if end > bytes.len() {
        return Err(malformed(path));
    }
    Ok(Some(String::from_utf8_lossy(&bytes[idx..end]).into_owned()))
}

/// Read a VCDIFF big-endian base-128 variant, advancing `idx`
fn read_varint(bytes: &[u8], idx: &mut usize) -> Option<usize> {
    let mut value = 0usize;
    for _ in 0..10 {
        let byte = *bytes.get(*idx)?;
        *idx += 1;
        value = value
            .checked_mul(128)?
            .checked_add((byte & 0x7F) as usize)?;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

/// Every allowed rel-path whose basename appears in `header`'s path tokens
fn matching_catalog_files(header: &str, allowed: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for &rel in allowed {
        let base = basename(rel);
        if header_tokens(header).any(|token| token.eq_ignore_ascii_case(base)) {
            out.push(rel.to_owned());
        }
    }
    out
}

fn basename(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn header_tokens(header: &str) -> impl Iterator<Item = &str> {
    header
        .split(['/', '\\', '\0', '\n', '\r', '\t'])
        .filter(|token| !token.is_empty())
}

fn is_delta_file(path: &Utf8Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("vcdiff") || ext.eq_ignore_ascii_case("xdelta"))
        && path.is_file()
}

fn malformed(path: &Utf8Path) -> VcdiffError {
    VcdiffError::Malformed {
        path: path.to_owned(),
    }
}

#[cfg(test)]
#[path = "tests/vcdiff.rs"]
mod tests;
