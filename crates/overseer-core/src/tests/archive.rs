//! Tests for the BA2 archive header reader

use super::*;
use crate::test_support::temp;

/// Build a 24-byte BA2 header with the given fields (name-table offset = 0)
fn header(version: u32, tag: &[u8; 4], file_count: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(HEADER_LEN);
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(tag);
    b.extend_from_slice(&file_count.to_le_bytes());
    b.extend_from_slice(&0u64.to_le_bytes());
    b
}

#[test]
fn parses_a_general_v1_header() {
    let h = Ba2Header::parse(&header(1, b"GNRL", 42)).expect("parse");
    assert_eq!(
        h,
        Ba2Header {
            version: 1,
            kind: Ba2Kind::General,
            file_count: 42,
        }
    );
}

#[test]
fn parses_a_texture_header() {
    let h = Ba2Header::parse(&header(1, b"DX10", 7)).expect("parse");
    assert_eq!(h.kind, Ba2Kind::Texture);
    assert_eq!(h.file_count, 7);
}

#[test]
fn reads_next_gen_versions_verbatim() {
    for v in [7u32, 8] {
        let h = Ba2Header::parse(&header(v, b"GNRL", 0)).expect("parse");
        assert_eq!(h.version, v);
    }
}

#[test]
fn reads_starfield_versions_from_the_shared_24_byte_prefix() {
    // Starfield headers are longer, but their first 24 bytes parse identically
    for v in [2u32, 3] {
        let h = Ba2Header::parse(&header(v, b"GNRL", 1)).expect("parse");
        assert_eq!(h.version, v);
    }
}

#[test]
fn an_unknown_tag_is_preserved_as_other() {
    let h = Ba2Header::parse(&header(1, b"GNMF", 3)).expect("parse");
    assert_eq!(h.kind, Ba2Kind::Other(*b"GNMF"));
}

#[test]
fn rejects_a_bad_magic() {
    let mut bytes = header(1, b"GNRL", 1);
    bytes[0..4].copy_from_slice(b"BSA\0");
    assert!(matches!(Ba2Header::parse(&bytes), Err(Ba2Error::BadMagic)));
}

#[test]
fn rejects_a_buffer_shorter_than_the_header() {
    assert!(matches!(Ba2Header::parse(b"BTDX"), Err(Ba2Error::TooShort)));
}

#[test]
fn read_parses_only_the_header_of_a_larger_file() {
    let (_tmp, dir) = temp();
    let path = dir.join("Textures.ba2");
    let mut bytes = header(1, b"DX10", 99);
    bytes.extend_from_slice(&[0u8; 4096]); // a "body" the reader must not need
    std::fs::write(&path, &bytes).expect("write");

    let h = Ba2Header::read(&path).expect("read");
    assert_eq!(
        h,
        Ba2Header {
            version: 1,
            kind: Ba2Kind::Texture,
            file_count: 99,
        }
    );
}

#[test]
fn read_reports_too_short_for_a_truncated_file() {
    let (_tmp, dir) = temp();
    let path = dir.join("Tiny.ba2");
    std::fs::write(&path, b"BTDX").expect("write");
    assert!(matches!(Ba2Header::read(&path), Err(Ba2Error::TooShort)));
}
