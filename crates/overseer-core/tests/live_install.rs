//! Opt-in integration tests against a REAL Fallout 4 install.
//!
//! Set `OVERSEER_FO4_DIR` to a real Fallout 4 game directory (the folder containing
//! `Fallout4.exe`) and these run; unset, they skip so the normal suite stays game-free. The
//! game directory itself is never written — the one test that patches works on a temp **copy**
//! of a base archive. Their job is to validate the pieces synthetic fixtures can't reproduce: a
//! real PE version resource, real base-game BA2 headers, and a version flip on real archive bytes.

use camino::Utf8PathBuf;
use overseer_core::archive::Ba2Header;
use overseer_core::detect;
use overseer_core::game::GameKind;
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome};
use overseer_core::test_support;

/// The real Fallout 4 dir from `OVERSEER_FO4_DIR`, or `None` with a skip note when unset/invalid
fn fo4_dir_or_skip() -> Option<Utf8PathBuf> {
    // Load `.env` (machine-specific harness paths) if present; real shell env vars still win
    let _ = dotenvy::dotenv();
    let Ok(dir) = std::env::var("OVERSEER_FO4_DIR") else {
        eprintln!("skipping: set OVERSEER_FO4_DIR to a real Fallout 4 install to run");
        return None;
    };
    let dir = Utf8PathBuf::from(dir);
    if !dir.join("Fallout4.exe").exists() {
        eprintln!("skipping: no Fallout4.exe under OVERSEER_FO4_DIR={dir}");
        return None;
    }
    Some(dir)
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_FO4_DIR set"]
fn detects_a_real_fallout4_install() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };

    let install = detect::detect(GameKind::Fallout4, &dir);
    // A real Fallout4.exe must parse to a version, which makes the edition determinable
    let version = install
        .version
        .expect("Fallout4.exe should yield a PE file version");
    let edition = detect::edition(&install, &dir);
    assert_ne!(edition, detect::Edition::Undetermined);

    eprintln!(
        "detected {edition:?}, store {:?}, version {version}",
        install.store
    );
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_FO4_DIR set"]
fn the_store_is_not_contradictory() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };

    let install = detect::detect(GameKind::Fallout4, &dir);
    // A hand-copied install may be `Unknown`, but a real one must never look like both Steam *and*; GOG at once
    assert_ne!(install.store, detect::Store::Conflicting);
    eprintln!("store {:?}", install.store);
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_FO4_DIR set"]
fn base_game_archives_parse() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };

    let data = dir.join("Data");
    let mut checked = 0;
    for name in [
        "Fallout4 - Startup.ba2",
        "Fallout4 - Textures1.ba2",
        "Fallout4 - Meshes.ba2",
    ] {
        let path = data.join(name);
        if !path.exists() {
            continue;
        }
        let header = Ba2Header::read(&path).unwrap_or_else(|e| panic!("{name} should parse: {e}"));
        assert!(
            matches!(header.version, 1 | 7 | 8),
            "{name}: unexpected BA2 version {}",
            header.version
        );
        eprintln!("{name}: {header:?}");
        checked += 1;
    }
    assert!(checked > 0, "no base-game BA2s found under {data}");
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_FO4_DIR set"]
fn patching_a_real_base_archive_changes_only_the_version_field() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };
    let data = dir.join("Data");

    // Find a base archive that parses as a patchable FO4 archive (v1/7/8, GNRL/DX10)
    let candidates = [
        "Fallout4 - Startup.ba2",
        "Fallout4 - Shaders.ba2",
        "Fallout4 - Interface.ba2",
        "Fallout4 - Materials.ba2",
        "Fallout4 - Meshes.ba2",
    ];
    let found = candidates.into_iter().find_map(|name| {
        let path = data.join(name);
        let header = Ba2Header::read(&path).ok()?;
        let edition = Ba2Edition::from_version(header.version)?;
        Some((path, name, header, edition))
    });
    let Some((src, name, header, current)) = found else {
        eprintln!("skipping: no patchable base BA2 found under {data}");
        return;
    };

    // Work on a COPY in a temp dir — the real install is never written
    let (_tmp, root) = test_support::temp();
    let copy = root.join(name);
    std::fs::copy(&src, &copy).expect("copy archive to temp");
    let original = std::fs::read(&copy).expect("read copy");

    // Flip to the opposite edition and prove the body survived untouched
    let opposite = match current {
        Ba2Edition::OldGen => Ba2Edition::NextGen,
        Ba2Edition::NextGen => Ba2Edition::OldGen,
    };
    let outcome = fallout4::set_edition(&copy, opposite).expect("patch to opposite edition");
    assert!(
        matches!(outcome, PatchOutcome::Patched { .. }),
        "expected a patch, got {outcome:?}"
    );

    let after = std::fs::read(&copy).expect("read patched");
    assert_eq!(
        after.len(),
        original.len(),
        "patch must not resize the archive"
    );
    assert_eq!(&after[0..4], &original[0..4], "magic must be unchanged");
    assert_ne!(
        &after[4..8],
        &original[4..8],
        "version field must have changed"
    );
    assert_eq!(
        &after[8..],
        &original[8..],
        "the entire archive body must be byte-for-byte preserved"
    );

    // Flip back. The body is always preserved; a canonical archive (v1/v8, not v7) returns; byte-identical, since patching back to its own edition rewrites the same version
    fallout4::set_edition(&copy, current).expect("patch back to original edition");
    let restored = std::fs::read(&copy).expect("read restored");
    assert_eq!(
        &restored[8..],
        &original[8..],
        "body still preserved after round-trip"
    );
    if header.version == current.target_version() {
        assert_eq!(
            restored, original,
            "a canonical archive round-trips byte-identical"
        );
    }

    eprintln!(
        "patched {name}: v{} <-> {opposite:?}, {} body bytes preserved",
        header.version,
        original.len() - 8
    );
}
