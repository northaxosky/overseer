//! Tests for archive extraction and the decompression-bomb guard

use super::*;

use std::fs::File;

use crate::test_support::temp;

/// `is_bomb` flags only archives that are both large (over the floor) and absurdly high-ratio
#[test]
fn is_bomb_flags_only_large_high_ratio_archives() {
    let mb: u64 = 1024 * 1024;
    // Under the 100 MB floor: never a bomb, even at an absurd ratio
    assert!(!is_bomb(50 * mb, 1));
    // Over the floor and over the 100x ratio: a bomb
    assert!(is_bomb(200 * mb, mb));
    // Over the floor but a plausible expansion: a legitimate large mod
    assert!(!is_bomb(200 * mb, 10 * mb));
    // Saturating math: an archive expanding ~1:1 is never a bomb, even at the u64 ceiling
    assert!(!is_bomb(u64::MAX, u64::MAX));
    // ...but a tiny archive declaring the maximum is
    assert!(is_bomb(u64::MAX, 1));
}

/// A normal `.7z` extracts through `extract`, covering the 7z happy path on the backend
#[test]
fn extracts_a_normal_7z_archive() {
    let (_t, base) = temp();
    let src = base.join("src");
    std::fs::create_dir_all(src.join("Textures")).expect("mk src");
    std::fs::write(src.join("Textures/a.dds"), b"tex").expect("write tex");
    std::fs::write(src.join("Cool.esp"), b"plugin").expect("write esp");
    let archive = base.join("mod.7z");
    sevenz_rust2::compress_to_path(src.as_std_path(), archive.as_std_path()).expect("compress");

    let dest = base.join("dest");
    extract(&archive, &dest).expect("extract");

    assert_eq!(
        std::fs::read_to_string(dest.join("Textures/a.dds")).unwrap(),
        "tex"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("Cool.esp")).unwrap(),
        "plugin"
    );
}

/// A crafted `.7z` whose entry name escapes the destination is rejected, writing nothing
/// outside `dest`. Regression for the sevenz-rust 0.6 path-traversal CVE (RUSTSEC-2023-0086);
/// `sevenz-rust2` confines each entry under `dest`, and `extract` surfaces that as an error
#[test]
fn rejects_a_path_traversal_7z_archive() {
    let (_t, base) = temp();
    // A real payload the malicious entry points at; its declared name escapes `dest`
    let payload = base.join("payload.txt");
    std::fs::write(&payload, b"pwned").expect("write payload");

    let archive = base.join("evil.7z");
    let entry =
        sevenz_rust2::ArchiveEntry::from_path(payload.as_std_path(), "../escape.txt".to_owned());
    let mut writer =
        sevenz_rust2::ArchiveWriter::new(File::create(&archive).expect("create archive"))
            .expect("archive writer");
    writer
        .push_archive_entry(entry, Some(File::open(&payload).expect("open payload")))
        .expect("push entry");
    writer.finish().expect("finish archive");

    let dest = base.join("dest");
    let err = extract(&archive, &dest).expect_err("traversal must be rejected");
    assert!(matches!(err, InstallError::SevenZip { .. }), "got {err:?}");

    // The escape target (a sibling of `dest`) must never be created
    assert!(
        !base.join("escape.txt").exists(),
        "a file escaped the destination directory"
    );
}
