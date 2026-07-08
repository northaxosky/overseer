//! Fallout 4 BA2 extract and repack, wrapping the `btdx` crate

use crate::error::{IoError, io_err};
use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;

/// One extracted general file: its archive path and decompressed bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ba2File {
    /// Backslash-separated archive path, lowercased by the writer
    pub path: String,
    /// The file's decompressed contents
    pub bytes: Vec<u8>,
}

/// One extracted texture: its archive path and reassembled DDS bytes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ba2Texture {
    /// Backslash-separated archive path, lowercased by the writer
    pub path: String,
    /// The full reassembled DDS file
    pub dds: Vec<u8>,
}

/// A BA2's contents, split by archive kind so each repacks with the matching writer
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ba2Payload {
    /// A `GNRL` archive's general files
    General(Vec<Ba2File>),
    /// A `DX10` archive's textures
    Textures(Vec<Ba2Texture>),
}

/// Why a BA2 could not be extracted or repacked
#[derive(Debug, Error)]
pub enum Ba2IoError {
    #[error(transparent)]
    Io(#[from] IoError),

    #[error("reading BA2 failed")]
    Read(#[from] btdx::ReadError),

    #[error("writing BA2 failed")]
    Write(#[from] btdx::WriteError),

    #[error("BA2 `{archive}` has no name table, so its entries cannot be re-keyed for merging")]
    NoNameTable { archive: Utf8PathBuf },
}

/// Read a whole BA2 from `path` and return its files or textures ready to repack
pub fn extract(path: &Utf8Path) -> Result<Ba2Payload, Ba2IoError> {
    let bytes = std::fs::read(path).map_err(|e| io_err(path, e))?;
    let archive = btdx::Archive::read(&bytes)?;
    match archive.entries() {
        btdx::Entries::General(entries) => {
            let mut files = Vec::with_capacity(entries.len());
            for entry in entries {
                let name = entry_path(entry.path.as_deref(), path)?;
                files.push(Ba2File {
                    path: name,
                    bytes: archive.extract(entry)?,
                });
            }
            Ok(Ba2Payload::General(files))
        }
        btdx::Entries::Texture(entries) => {
            let mut textures = Vec::with_capacity(entries.len());
            for entry in entries {
                let name = entry_path(entry.path.as_deref(), path)?;
                textures.push(Ba2Texture {
                    path: name,
                    dds: archive.extract_texture(entry)?,
                });
            }
            Ok(Ba2Payload::Textures(textures))
        }
    }
}

/// Pack general files into a Fallout 4 v1 GNRL archive, zlib-compressed when `compress` is set
pub fn pack_general(files: &[Ba2File], compress: bool) -> Result<Vec<u8>, Ba2IoError> {
    let mut writer = btdx::GnrlWriter::new();
    for file in files {
        if compress {
            writer.add_file(file.path.as_bytes(), file.bytes.clone())?;
        } else {
            writer.add_file_stored(file.path.as_bytes(), file.bytes.clone())?;
        }
    }
    Ok(writer.to_vec()?)
}

/// Pack textures into a Fallout 4 v1 DX10 archive; each DDS is re-chunked and compressed by `btdx`
pub fn pack_textures(textures: &[Ba2Texture]) -> Result<Vec<u8>, Ba2IoError> {
    let mut writer = btdx::Dx10Writer::new();
    for texture in textures {
        writer.add_texture(texture.path.as_bytes(), texture.dds.clone())?;
    }
    Ok(writer.to_vec()?)
}

/// Resolve an entry's archive path, erroring when the source archive lacked a name table
fn entry_path(path: Option<&str>, archive: &Utf8Path) -> Result<String, Ba2IoError> {
    path.map(str::to_owned)
        .ok_or_else(|| Ba2IoError::NoNameTable {
            archive: archive.to_owned(),
        })
}

#[cfg(test)]
#[path = "tests/ba2.rs"]
mod tests;
