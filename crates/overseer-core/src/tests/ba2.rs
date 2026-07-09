//! Tests for the BA2 extract/repack wrapper over `btdx`

use super::*;
use crate::test_support::temp;

/// Write `bytes` to `dir/name` and return its path
fn stage(dir: &Utf8Path, name: &str, bytes: &[u8]) -> Utf8PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write archive");
    path
}

/// A minimal valid DX10 DDS: a 148-byte header plus one 4x4 BC1 block (8 bytes)
fn minimal_dx10_dds() -> Vec<u8> {
    let mut words = [0u32; 37];
    words[0] = 0x2053_4444; // "DDS " magic
    words[1] = 124; // header size
    words[2] = 0x1007 | 0x8_0000; // caps/height/width/pixelformat + linear size
    words[3] = 4; // height
    words[4] = 4; // width
    words[5] = 8; // linear size: one 4x4 BC1 block
    words[7] = 1; // mip count
    words[19] = 32; // pixel-format size
    words[20] = 0x4; // DDPF_FOURCC
    words[21] = 0x3031_5844; // "DX10" fourCC
    words[27] = 0x1000; // DDSCAPS_TEXTURE
    words[32] = 71; // DXGI_FORMAT_BC1_UNORM
    words[33] = 3; // 2D texture dimension
    words[35] = 1; // array size
    let mut dds = Vec::with_capacity(156);
    for w in words {
        dds.extend_from_slice(&w.to_le_bytes());
    }
    dds.extend_from_slice(&[0xABu8; 8]);
    dds
}

#[test]
fn general_files_round_trip_compressed() {
    let (_tmp, dir) = temp();
    let files = vec![
        Ba2File {
            path: "meshes\\a.nif".to_owned(),
            bytes: b"the quick brown fox".repeat(8),
        },
        Ba2File {
            path: "scripts\\b.pex".to_owned(),
            bytes: vec![1, 2, 3, 4, 5],
        },
    ];

    let img = pack_general(&files, |_| false).expect("pack");
    let path = stage(&dir, "Merged.ba2", &img);
    let payload = extract(&path).expect("extract");

    assert_eq!(payload, Ba2Payload::General(files));
}

#[test]
fn general_files_round_trip_stored() {
    let (_tmp, dir) = temp();
    let files = vec![Ba2File {
        path: "data\\raw.bin".to_owned(),
        bytes: vec![7u8; 64],
    }];

    let img = pack_general(&files, |_| true).expect("pack");
    let path = stage(&dir, "Stored.ba2", &img);
    let payload = extract(&path).expect("extract");

    assert_eq!(payload, Ba2Payload::General(files));
}

#[test]
fn textures_round_trip_through_the_wrapper() {
    let (_tmp, dir) = temp();
    let seed = Ba2Texture {
        path: "textures\\t.dds".to_owned(),
        dds: minimal_dx10_dds(),
    };

    // First pass yields btdx's canonical DDS reconstruction
    let img = pack_textures(std::slice::from_ref(&seed)).expect("pack");
    let path = stage(&dir, "Textures.ba2", &img);
    let Ba2Payload::Textures(first) = extract(&path).expect("extract") else {
        panic!("expected textures");
    };
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].path, "textures\\t.dds");

    // Re-packing the canonical DDS and extracting again must be idempotent
    let img2 = pack_textures(&first).expect("repack");
    let path2 = stage(&dir, "Textures2.ba2", &img2);
    let Ba2Payload::Textures(second) = extract(&path2).expect("extract") else {
        panic!("expected textures");
    };
    assert_eq!(first, second);
}

#[test]
fn extract_rejects_an_archive_without_a_name_table() {
    let (_tmp, dir) = temp();
    let mut writer = btdx::GnrlWriter::new();
    writer.names(false);
    writer
        .add_file_stored("a.bin", b"hi".to_vec())
        .expect("add file");
    let img = writer.to_vec().expect("to_vec");
    let path = stage(&dir, "Nameless.ba2", &img);

    assert!(matches!(
        extract(&path),
        Err(Ba2IoError::NoNameTable { .. })
    ));
}

#[test]
fn extract_reports_io_for_a_missing_file() {
    let (_tmp, dir) = temp();
    let path = dir.join("does-not-exist.ba2");
    assert!(matches!(extract(&path), Err(Ba2IoError::Io(_))));
}
