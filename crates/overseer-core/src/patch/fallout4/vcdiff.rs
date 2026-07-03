//! Minimal VCDIFF header parsing for delta auto-mapping

use super::catalog::GROUPS;
use crate::error::{IoError, io_err};
use crate::fs::read_dir_opt;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::HashMap;
use thiserror::Error;

const VCD_DECOMPRESS: u8 = 0x01;
const VCD_CODETABLE: u8 = 0x02;
const VCD_APPHEADER: u8 = 0x04;

/// Something went wrong reading a delta or mapping it to a core binary
#[derive(Debug, Error)]
pub enum VcdiffError {
    #[error("VCDIFF file `{path}` is too short")]
    TooShort { path: Utf8PathBuf },

    #[error("VCDIFF file `{path}` has a bad magic header")]
    BadMagic { path: Utf8PathBuf },

    #[error("VCDIFF file `{path}` has a malformed header")]
    Malformed { path: Utf8PathBuf },

    #[error("delta `{path}` does not contain an app-header basename; pass an explicit delta flag")]
    MissingAppHeaderName { path: Utf8PathBuf },

    #[error("delta `{path}` app-header names more than one core binary: {names}")]
    AmbiguousAppHeader { path: Utf8PathBuf, names: String },

    #[error("more than one delta maps to {name}: `{first}` and `{second}`")]
    DuplicateBinary {
        name: String,
        first: Utf8PathBuf,
        second: Utf8PathBuf,
    },

    #[error(transparent)]
    Io(#[from] IoError),
}

/// Bytes of a delta scanned for its app-header; xdelta3 path headers are well under a KiB, so this bounds reads of multi-GB bodies
const HEADER_SCAN_LEN: u64 = 64 * 1024;

/// Read the VCDIFF app-header string from `path`, if it carries one
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

/// Map every delta under `dir` (recursively) to the catalog file its app-header names
pub fn map_deltas(dir: &Utf8Path) -> Result<HashMap<String, Utf8PathBuf>, VcdiffError> {
    let mut mapped = HashMap::new();
    if read_dir_opt(dir)?.is_none() {
        return Ok(mapped);
    }
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.map_err(|e| {
            let io = e
                .into_io_error()
                .unwrap_or_else(|| std::io::Error::other("directory walk failed"));
            io_err(dir, io)
        })?;
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
        let name = target_from_header(&path)?;
        if let Some(first) = mapped.insert(name.clone(), path.clone()) {
            return Err(VcdiffError::DuplicateBinary {
                name,
                first,
                second: path,
            });
        }
    }
    Ok(mapped)
}

/// The catalog rel-path a single delta's app header names
pub fn target_from_header(path: &Utf8Path) -> Result<String, VcdiffError> {
    let Some(header) = app_header(path)? else {
        return Err(VcdiffError::MissingAppHeaderName {
            path: path.to_owned(),
        });
    };
    let matches = matching_catalog_files(&header);
    match matches.as_slice() {
        [name] => Ok((*name).to_owned()),
        [] => Err(VcdiffError::MissingAppHeaderName {
            path: path.to_owned(),
        }),
        many => Err(VcdiffError::AmbiguousAppHeader {
            path: path.to_owned(),
            names: many.join(", "),
        }),
    }
}

/// Parse the app-header string out of raw VCDIFF `bytes` per the RFC 3284 header layout
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
        idx = idx.checked_add(1).ok_or_else(|| malformed(path))?;
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

fn matching_catalog_files(header: &str) -> Vec<&'static str> {
    let tokens = header_tokens(header);
    let mut out = Vec::new();
    for group in GROUPS {
        for &rel in group.files {
            let base = rel.rsplit('/').next().unwrap_or(rel);
            if tokens.iter().any(|token| token.eq_ignore_ascii_case(base)) {
                out.push(rel);
            }
        }
    }
    out
}

fn header_tokens(header: &str) -> Vec<String> {
    header
        .split(['/', '\\', '\0', '\n', '\r', '\t'])
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
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
mod tests {
    use super::*;
    use crate::test_support::temp;

    fn header_delta(app: Option<&[u8]>) -> Vec<u8> {
        let mut bytes = vec![0xD6, 0xC3, 0xC4, 0x00, 0x00];
        if let Some(app) = app {
            bytes[4] = VCD_APPHEADER;
            write_varint(&mut bytes, app.len());
            bytes.extend_from_slice(app);
        }
        bytes
    }

    fn write_varint(out: &mut Vec<u8>, mut value: usize) {
        let mut stack = vec![(value & 0x7F) as u8];
        value >>= 7;
        while value > 0 {
            stack.push(((value & 0x7F) as u8) | 0x80);
            value >>= 7;
        }
        out.extend(stack.into_iter().rev());
    }

    #[test]
    fn reads_xdelta3_app_header_basename() {
        let (_tmp, root) = temp();
        let path = root.join("patch.vcdiff");
        std::fs::write(
            &path,
            header_delta(Some(br"C:\old\Fallout4.exe//C:\new\Fallout4.exe/")),
        )
        .unwrap();
        assert_eq!(target_from_header(&path).unwrap(), "Fallout4.exe");
    }

    #[test]
    fn maps_a_dlc_delta_to_its_data_rel_path() {
        let (_tmp, root) = temp();
        let path = root.join("d.vcdiff");
        let header = br"C:\mods\Best DLC Data\DLCCoast - Textures.ba2//C:\Steam\Fallout 4\Data\DLCCoast - Textures.ba2/";
        std::fs::write(&path, header_delta(Some(header))).unwrap();
        assert_eq!(
            target_from_header(&path).unwrap(),
            "Data/DLCCoast - Textures.ba2"
        );
    }

    #[test]
    fn headerless_delta_requires_explicit_mapping() {
        let (_tmp, root) = temp();
        let path = root.join("patch.vcdiff");
        std::fs::write(&path, header_delta(None)).unwrap();
        assert!(matches!(
            target_from_header(&path),
            Err(VcdiffError::MissingAppHeaderName { .. })
        ));
    }

    #[test]
    fn duplicate_basenames_are_rejected() {
        let (_tmp, root) = temp();
        for name in ["a.vcdiff", "b.vcdiff"] {
            std::fs::write(
                root.join(name),
                header_delta(Some(br"C:\old\steam_api64.dll//C:\new\steam_api64.dll/")),
            )
            .unwrap();
        }
        assert!(matches!(
            map_deltas(&root),
            Err(VcdiffError::DuplicateBinary { .. })
        ));
    }

    #[test]
    fn maps_deltas_recursively_across_subdirs() {
        let (_tmp, root) = temp();
        std::fs::create_dir_all(root.join("DLCCoast/Data")).unwrap();
        std::fs::create_dir_all(root.join("core")).unwrap();
        std::fs::write(
            root.join("DLCCoast/Data/DLCCoast.esm.vcdiff"),
            header_delta(Some(br"old\DLCCoast.esm//new\Data\DLCCoast.esm/")),
        )
        .unwrap();
        std::fs::write(
            root.join("core/exe.vcdiff"),
            header_delta(Some(br"old\Fallout4.exe//new\Fallout4.exe/")),
        )
        .unwrap();
        let mapped = map_deltas(&root).unwrap();
        assert!(mapped.contains_key("Data/DLCCoast.esm"));
        assert!(mapped.contains_key("Fallout4.exe"));
        assert_eq!(mapped.len(), 2);
    }

    #[test]
    fn real_downgrader_style_header_maps_launcher() {
        let (_tmp, root) = temp();
        let path = root.join("patch.xdelta");
        let header = br"C:\Users\KARV\Downloads\fo4patchy\fo4andsteamdll\old\Fallout4Launcher.exe//C:\Users\KARV\Downloads\fo4patchy\fo4andsteamdll\NEW\Fallout4Launcher.exe/";
        std::fs::write(&path, header_delta(Some(header))).unwrap();
        assert_eq!(target_from_header(&path).unwrap(), "Fallout4Launcher.exe");
    }
}
