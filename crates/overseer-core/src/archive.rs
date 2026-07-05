//! Reading Bethesda archive (BA2 / "BTDX") headers

use crate::error::{IoError, io_err};
use camino::Utf8Path;
use std::io::Read;
use thiserror::Error;

/// The fixed BA2 header is 24 bytes for every version
pub(crate) const HEADER_LEN: usize = 24;
const MAGIC: &[u8; 4] = b"BTDX";

/// What a BA2 archive holds, from its 4 byte type tag
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ba2Kind {
    /// `GNRL`: general files (meshes, scripts, sounds, ...)
    General,
    /// `DX10`: DirectX textures
    Texture,
    /// Any other tag, preserved for reporting
    Other([u8; 4]),
}

/// The facts a diagnostic needs from a BA2 header. No file records are read
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ba2Header {
    /// Format version: 1 = FO4 OG, 7/8 = FO4 NG/AE, 2/3 = Starfield
    pub version: u32,
    /// The archive's content kind
    pub kind: Ba2Kind,
    /// Number of files the archive contains
    pub file_count: u32,
}

/// Why a BA2 header could not be read
#[derive(Debug, Error)]
pub enum Ba2Error {
    #[error("not a BA2 archive (missing BTDX)")]
    BadMagic,

    #[error("file is too short to hold a BA2 header")]
    TooShort,

    #[error(transparent)]
    Io(#[from] IoError),
}

/// Read a little-endian u32 at `offset`, or `TooShort` if it ends first
fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, Ba2Error> {
    let slice = bytes.get(offset..offset + 4).ok_or(Ba2Error::TooShort)?;
    Ok(u32::from_le_bytes(slice.try_into().expect("4-byte slice")))
}

impl Ba2Header {
    /// Parse a header from the start of a BA2
    pub fn parse(bytes: &[u8]) -> Result<Self, Ba2Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Ba2Error::TooShort);
        }
        if &bytes[0..4] != MAGIC {
            return Err(Ba2Error::BadMagic);
        }
        let version = read_u32_le(bytes, 4)?;
        let tag: [u8; 4] = bytes[8..12].try_into().expect("4 bytes");
        let kind = match &tag {
            b"GNRL" => Ba2Kind::General,
            b"DX10" => Ba2Kind::Texture,
            _ => Ba2Kind::Other(tag),
        };
        let file_count = read_u32_le(bytes, 12)?;
        Ok(Self {
            version,
            kind,
            file_count,
        })
    }

    /// Read the header (first 24 bytes) of a BA2 file, without reading archive body
    pub fn read(path: &Utf8Path) -> Result<Self, Ba2Error> {
        let mut file = std::fs::File::open(path).map_err(|e| io_err(path, e))?;
        let mut buf = [0u8; HEADER_LEN];
        file.read_exact(&mut buf).map_err(|e| match e.kind() {
            std::io::ErrorKind::UnexpectedEof => Ba2Error::TooShort,
            _ => Ba2Error::Io(io_err(path, e)),
        })?;
        Self::parse(&buf)
    }
}

#[cfg(test)]
#[path = "tests/archive.rs"]
mod tests;
