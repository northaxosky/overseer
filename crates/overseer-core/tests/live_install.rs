//! Opt-in integration tests against a REAL Fallout 4 install.
//!
//! Set `OVERSEER_FO4_DIR` to a real Fallout 4 game directory (the folder containing
//! `Fallout4.exe`) and these run; unset, they skip so the normal suite stays game-free. They are
//! strictly **read-only** — nothing is written to the game. Their job is to validate the pieces
//! synthetic fixtures can't reproduce: a real PE version resource and real base-game BA2 headers.

use camino::Utf8PathBuf;
use overseer_core::archive::Ba2Header;
use overseer_core::detect;
use overseer_core::game::GameKind;

/// The real Fallout 4 dir from `OVERSEER_FO4_DIR`, or `None` (with a skip note) when it is unset
/// or doesn't point at a Fallout 4 install.
fn fo4_dir_or_skip() -> Option<Utf8PathBuf> {
    // Load `.env` (machine-specific harness paths) if present; real shell env vars still win.
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
fn detects_a_real_fallout4_install() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };

    let install = detect::detect(GameKind::Fallout4, &dir);
    // A real Fallout4.exe must parse to a version, which makes the edition determinable.
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
fn the_store_is_not_contradictory() {
    let Some(dir) = fo4_dir_or_skip() else {
        return;
    };

    let install = detect::detect(GameKind::Fallout4, &dir);
    // A hand-copied install may be `Unknown`, but a real one must never look like both Steam *and*
    // GOG at once.
    assert_ne!(install.store, detect::Store::Conflicting);
    eprintln!("store {:?}", install.store);
}

#[test]
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
