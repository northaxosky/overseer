//! Tests for the Fallout 4 archive merge engine: extraction, dedupe, bucketing, and repack policy

use super::*;
use crate::archive::Ba2Kind;
use crate::game::GameKind;
use crate::test_support::temp;

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

/// A general file from a path and bytes
fn file(path: &str, bytes: &[u8]) -> Ba2File {
    Ba2File {
        path: path.to_owned(),
        bytes: bytes.to_vec(),
    }
}

/// Pack `files` into a GNRL source archive at `dir/name` and return its path
fn write_gnrl(dir: &Utf8Path, name: &str, files: &[Ba2File]) -> Utf8PathBuf {
    let img = ba2::pack_general(files, |_| false).expect("pack general source");
    let path = dir.join(name);
    std::fs::write(&path, img).expect("write general source");
    path
}

/// Pack `textures` into a DX10 source archive at `dir/name` and return its path
fn write_dx10(dir: &Utf8Path, name: &str, textures: &[Ba2Texture]) -> Utf8PathBuf {
    let img = ba2::pack_textures(textures).expect("pack texture source");
    let path = dir.join(name);
    std::fs::write(&path, img).expect("write texture source");
    path
}

/// Default merge options for `basename` with the standard texture cap
fn opts(basename: &str) -> MergeOptions {
    MergeOptions {
        basename: basename.to_owned(),
        texture_group_bytes: DEFAULT_TEXTURE_GROUP_BYTES,
    }
}

/// Extract a merged archive's general files
fn general_of(archive: &Utf8Path) -> Vec<Ba2File> {
    match ba2::extract(archive).expect("extract general") {
        ba2::Ba2Payload::General(files) => files,
        ba2::Ba2Payload::Textures(_) => panic!("expected a general archive"),
    }
}

/// Extract a merged archive's textures
fn textures_of(archive: &Utf8Path) -> Vec<Ba2Texture> {
    match ba2::extract(archive).expect("extract textures") {
        ba2::Ba2Payload::Textures(textures) => textures,
        ba2::Ba2Payload::General(_) => panic!("expected a texture archive"),
    }
}

#[test]
fn sounds_are_stored_in_the_general_archive_not_a_separate_one() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(
        &dir,
        "src.ba2",
        &[
            file("meshes\\a.nif", b"nif-bytes"),
            file("sound\\a.wav", b"wav-bytes"),
        ],
    );
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    assert_eq!(out.counts(), MergeCounts { gnrl: 1, dx10: 0 });
    assert_eq!(out.archives.len(), 1);
    let only = &out.archives[0];
    assert_eq!(only.kind, Ba2Kind::General);
    assert_eq!(only.carrier, "Merged_Main");
    assert_eq!(only.archive.file_name(), Some("Merged_Main - Main.ba2"));
    assert!(!out.archives.iter().any(|a| a.carrier.contains("Sounds")));

    let paths: Vec<String> = general_of(&only.archive)
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert!(paths.contains(&"meshes\\a.nif".to_owned()));
    assert!(paths.contains(&"sound\\a.wav".to_owned()));
}

#[test]
fn a_dds_in_a_general_source_stays_general() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(&dir, "src.ba2", &[file("textures\\x.dds", b"loose-dds")]);
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    assert_eq!(out.counts(), MergeCounts { gnrl: 1, dx10: 0 });
    let paths: Vec<String> = general_of(&out.archives[0].archive)
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(paths, vec!["textures\\x.dds".to_owned()]);
}

#[test]
fn strings_stage_loose_under_a_single_strings_dir() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(
        &dir,
        "src.ba2",
        &[file("Strings\\Foo_en.STRINGS", b"loc-bytes")],
    );
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    assert!(out.archives.is_empty());
    assert_eq!(out.strings.len(), 1);
    let staged = &out.strings[0];
    assert!(staged.exists());
    let parent = staged.parent().expect("strings parent");
    assert!(
        parent
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("strings"))
    );
    assert_eq!(parent.parent(), Some(staging.as_path()));
    assert!(
        staged
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("foo_en.strings"))
    );
}

#[test]
fn a_strings_path_that_escapes_its_root_is_rejected() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(&dir, "src.ba2", &[file("..\\evil.strings", b"x")]);
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    let err =
        merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect_err("must reject");
    assert!(matches!(err, MergeError::UnsafePath(_)));
}

#[test]
fn bad_basenames_are_rejected() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(&dir, "src.ba2", &[file("meshes\\a.nif", b"x")]);
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    for bad in ["../x", "a/b", "x.esp"] {
        let err =
            merge(&sources, &staging, &opts(bad), GameKind::Fallout4).expect_err("must reject");
        assert!(matches!(err, MergeError::InvalidBasename(_)), "for {bad}");
    }
}

#[test]
fn higher_override_rank_wins_a_path_clash() {
    let (_tmp, dir) = temp();
    let low = write_gnrl(&dir, "low.ba2", &[file("meshes\\a.nif", b"loser")]);
    let high = write_gnrl(&dir, "high.ba2", &[file("meshes\\a.nif", b"winner")]);
    let sources = [
        MergeSource {
            archive: low.clone(),
            override_rank: 0,
        },
        MergeSource {
            archive: high.clone(),
            override_rank: 5,
        },
    ];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    let files = general_of(&out.archives[0].archive);
    assert_eq!(files, vec![file("meshes\\a.nif", b"winner")]);
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(
        out.conflicts[0],
        MergeConflict {
            path: "meshes\\a.nif".to_owned(),
            winner: high,
            loser: low,
        }
    );
}

#[test]
fn an_equal_rank_clash_keeps_the_earlier_source() {
    let (_tmp, dir) = temp();
    let first = write_gnrl(&dir, "first.ba2", &[file("meshes\\a.nif", b"first")]);
    let second = write_gnrl(&dir, "second.ba2", &[file("meshes\\a.nif", b"second")]);
    let sources = [
        MergeSource {
            archive: first.clone(),
            override_rank: 3,
        },
        MergeSource {
            archive: second.clone(),
            override_rank: 3,
        },
    ];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    let files = general_of(&out.archives[0].archive);
    assert_eq!(files, vec![file("meshes\\a.nif", b"first")]);
    assert_eq!(out.conflicts.len(), 1);
    assert_eq!(out.conflicts[0].winner, first);
    assert_eq!(out.conflicts[0].loser, second);
}

#[test]
fn textures_split_into_capped_groups_each_with_a_carrier() {
    let (_tmp, dir) = temp();
    let source = write_dx10(
        &dir,
        "tex.ba2",
        &[
            Ba2Texture {
                path: "textures\\a.dds".to_owned(),
                dds: minimal_dx10_dds(),
            },
            Ba2Texture {
                path: "textures\\b.dds".to_owned(),
                dds: minimal_dx10_dds(),
            },
        ],
    );
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");
    let options = MergeOptions {
        basename: "Merged".to_owned(),
        texture_group_bytes: 100,
    };

    let out = merge(&sources, &staging, &options, GameKind::Fallout4).expect("merge");

    assert_eq!(out.counts(), MergeCounts { gnrl: 0, dx10: 2 });
    assert_eq!(out.archives[0].carrier, "Merged_Textures01");
    assert_eq!(out.archives[1].carrier, "Merged_Textures02");
    assert_eq!(out.carriers.len(), 2);

    let first: Vec<String> = textures_of(&out.archives[0].archive)
        .into_iter()
        .map(|t| t.path)
        .collect();
    let second: Vec<String> = textures_of(&out.archives[1].archive)
        .into_iter()
        .map(|t| t.path)
        .collect();
    assert_eq!(first, vec!["textures\\a.dds".to_owned()]);
    assert_eq!(second, vec!["textures\\b.dds".to_owned()]);
}

#[test]
fn every_archive_gets_one_carrier_esl() {
    let (_tmp, dir) = temp();
    let gnrl = write_gnrl(&dir, "g.ba2", &[file("meshes\\a.nif", b"nif")]);
    let dx10 = write_dx10(
        &dir,
        "t.ba2",
        &[Ba2Texture {
            path: "textures\\a.dds".to_owned(),
            dds: minimal_dx10_dds(),
        }],
    );
    let sources = [
        MergeSource {
            archive: gnrl,
            override_rank: 0,
        },
        MergeSource {
            archive: dx10,
            override_rank: 0,
        },
    ];
    let staging = dir.join("stage");

    let out = merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect("merge");

    assert_eq!(out.carriers.len(), out.archives.len());
    for archive in &out.archives {
        let carrier = staging.join(format!("{}.esl", archive.carrier));
        assert!(carrier.exists());
        assert!(out.carriers.contains(&carrier));
    }
}

#[test]
fn no_sources_is_empty() {
    let (_tmp, dir) = temp();
    let staging = dir.join("stage");
    let err = merge(&[], &staging, &opts("Merged"), GameKind::Fallout4).expect_err("must be empty");
    assert!(matches!(err, MergeError::Empty));
}

#[test]
fn a_source_with_no_entries_is_empty() {
    let (_tmp, dir) = temp();
    let source = write_gnrl(&dir, "empty.ba2", &[]);
    let sources = [MergeSource {
        archive: source,
        override_rank: 0,
    }];
    let staging = dir.join("stage");

    let err =
        merge(&sources, &staging, &opts("Merged"), GameKind::Fallout4).expect_err("must be empty");
    assert!(matches!(err, MergeError::Empty));
}

#[test]
fn merges_are_deterministic() {
    let (_tmp, dir) = temp();
    let gnrl = write_gnrl(&dir, "g.ba2", &[file("meshes\\a.nif", b"nif")]);
    let dx10 = write_dx10(
        &dir,
        "t.ba2",
        &[Ba2Texture {
            path: "textures\\a.dds".to_owned(),
            dds: minimal_dx10_dds(),
        }],
    );
    let sources = [
        MergeSource {
            archive: gnrl,
            override_rank: 0,
        },
        MergeSource {
            archive: dx10,
            override_rank: 0,
        },
    ];

    let one = merge(
        &sources,
        &dir.join("a"),
        &opts("Merged"),
        GameKind::Fallout4,
    )
    .expect("merge one");
    let two = merge(
        &sources,
        &dir.join("b"),
        &opts("Merged"),
        GameKind::Fallout4,
    )
    .expect("merge two");

    let shape = |o: &MergeOutput| -> Vec<(String, Ba2Kind)> {
        o.archives
            .iter()
            .map(|a| (a.carrier.clone(), a.kind))
            .collect()
    };
    assert_eq!(shape(&one), shape(&two));
}
