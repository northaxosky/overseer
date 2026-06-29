//! Opt-in **destructive** end-to-end test that deploys a synthetic mod into a real
//! Fallout 4 install and then purges it, proving the transaction leaves the game
//! directory byte-for-byte as it found it.
//!
//! This is `#[ignore]`d and gated on `OVERSEER_FO4_TESTBED`, so it never runs in the
//! normal suite. Run it deliberately against a **disposable copy**:
//!
//! ```text
//! cargo test -p overseer-core --test testbed_e2e -- --ignored --nocapture
//! ```
//!
//! ## Why it is safe to point at a real install
//!
//! 1. **Marker gate.** It refuses to run unless `<game_dir>/.overseer-e2e-testbed`
//!    exists with a fixed magic string. That marker is planted by hand only in the
//!    disposable copy, so a misconfigured `OVERSEER_FO4_TESTBED` can never fire against
//!    a real game.
//! 2. **Unique paths.** The synthetic mod lives in a namespace no base file uses
//!    (`Data/OverseerE2E.esp`, `Data/Textures/Overseer_E2E/`), so deploy only *adds*
//!    files and never overwrites — the base game is untouched even mid-flight.
//! 3. **Crash-safe + deterministic instance.** The Overseer instance lives at a fixed
//!    path on the same volume (so a panic-orphaned journal survives for the next run to
//!    reverse), and the engine itself backs up before any clobber.
//! 4. **Cross-process lock.** A lockfile in the game dir blocks two runs from racing the
//!    shared `Data/`.
//! 5. **Pristine assertion.** A `(path, len)` snapshot of `Data/` before deploy is
//!    compared after purge; any drift fails loudly.

use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::apply::{self, Status};
use overseer_core::deploy::NullSink;
use overseer_core::detect;
use overseer_core::game::GameKind;
use overseer_core::instance::Instance;
use overseer_core::test_support;
use std::collections::BTreeMap;
use walkdir::WalkDir;

/// Marker file that must sit in the testbed game dir for the destructive test to run.
const MARKER_FILE: &str = ".overseer-e2e-testbed";
/// Magic the marker must contain — the guard that keeps this off a real install.
const MARKER_MAGIC: &str = "OVERSEER_DESTRUCTIVE_E2E_TESTBED_DO_NOT_CREATE_IN_REAL_GAME";

/// Our reserved mod name and the unique paths it deploys into `Data/`.
const E2E_MOD: &str = "OverseerE2E";
const E2E_PLUGIN: &str = "OverseerE2E.esp";
const E2E_TEX_SUBDIR: &str = "Overseer_E2E";
const E2E_TEX_REL: &str = "Textures/Overseer_E2E/marker.dds";

/// The disposable testbed game dir from `OVERSEER_FO4_TESTBED`, or `None` (with a skip
/// note) when the var is unset. Set-but-unsafe (missing/forged marker) **panics** rather
/// than skipping, so a misconfigured path can never silently pass.
fn testbed_or_skip() -> Option<Utf8PathBuf> {
    let _ = dotenvy::dotenv();
    let Ok(dir) = std::env::var("OVERSEER_FO4_TESTBED") else {
        eprintln!("skipping: set OVERSEER_FO4_TESTBED to a disposable Fallout 4 copy to run");
        return None;
    };
    let dir = Utf8PathBuf::from(dir);
    if !dir.join("Fallout4.exe").exists() {
        eprintln!("skipping: no Fallout4.exe under OVERSEER_FO4_TESTBED={dir}");
        return None;
    }

    let marker = dir.join(MARKER_FILE);
    let contents = std::fs::read_to_string(&marker).unwrap_or_else(|_| {
        panic!(
            "refusing to run destructive e2e: testbed marker `{marker}` is missing.\n\
             This guard prevents running against a real install. If this really is your \
             disposable copy, create that file containing:\n  {MARKER_MAGIC}"
        )
    });
    assert!(
        contents.contains(MARKER_MAGIC),
        "refusing to run: testbed marker `{marker}` exists but lacks the magic string"
    );
    Some(dir)
}

/// A best-effort cross-process lock so two runs never mutate the shared `Data/` at once.
/// Created with `create_new`; removed on drop, so it clears even if the test panics.
struct TestbedLock {
    path: Utf8PathBuf,
}

impl TestbedLock {
    fn acquire(game_dir: &Utf8Path) -> Self {
        let path = game_dir.join(".overseer-e2e.lock");
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap_or_else(|e| {
                panic!(
                    "e2e lock `{path}` present or unwritable ({e}); another run may be active — \
                     remove it to proceed"
                )
            });
        Self { path }
    }
}

impl Drop for TestbedLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Snapshot every file under `data` as `lowercased-relative-path -> length`. Directories
/// are ignored so the engine creating/removing empty dirs doesn't register as drift.
fn snapshot_data(data: &Utf8Path) -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    for entry in WalkDir::new(data) {
        let entry = entry.expect("walk Data/");
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = Utf8Path::from_path(entry.path()).expect("utf8 Data/ path");
        let rel = abs
            .strip_prefix(data)
            .expect("under Data/")
            .as_str()
            .to_lowercase();
        map.insert(rel, entry.metadata().expect("file metadata").len());
    }
    map
}

/// Fail with a readable diff if `Data/` changed across a deploy/purge round-trip.
fn assert_pristine(before: &BTreeMap<String, u64>, after: &BTreeMap<String, u64>) {
    if before == after {
        return;
    }
    let added: Vec<_> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    let removed: Vec<_> = before.keys().filter(|k| !after.contains_key(*k)).collect();
    let resized: Vec<_> = before
        .iter()
        .filter(|(k, v)| after.get(*k).is_some_and(|w| w != *v))
        .map(|(k, _)| k)
        .collect();
    panic!(
        "Data/ not pristine after purge:\n  added:   {added:?}\n  removed: {removed:?}\n  \
         resized: {resized:?}"
    );
}

/// Reverse any deployment a previous (possibly crashed) run left behind, then scrub our
/// unique namespace so the baseline is clean before we deploy.
fn preflight_clean(instance_root: &Utf8Path, data: &Utf8Path, backup_root: &Utf8Path) {
    // (a) If the deterministic instance survived with a live or crashed deployment, let the
    //     engine reverse it properly: `status` runs crash-recovery; `purge` clears a committed one.
    if Instance::config_path(instance_root).exists()
        && let Ok(instance) = Instance::load(instance_root)
        && matches!(apply::status(&instance), Ok(Some(_)))
    {
        apply::purge(&instance, &NullSink).expect("preflight purge of leftover deployment");
    }

    // (b) Orphan catch for the case the instance dir itself was lost: delete our files directly.
    //     Safe because these paths are ours alone and never collide with base-game files.
    let _ = std::fs::remove_file(data.join(E2E_PLUGIN));
    let _ = std::fs::remove_dir_all(data.join("Textures").join(E2E_TEX_SUBDIR));

    // (c) The unique-path design never backs a base file aside, so the backup root must be empty.
    //     Anything else means a real file was clobbered — stop rather than guess.
    if backup_root.exists() {
        let has_entries = std::fs::read_dir(backup_root)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        assert!(
            !has_entries,
            "unexpected contents under `{backup_root}`; refusing to proceed — inspect or re-copy \
             the testbed"
        );
        let _ = std::fs::remove_dir(backup_root);
    }
}

#[test]
#[ignore = "destructive; run on-demand against the OVERSEER_FO4_TESTBED copy"]
fn deploy_purge_roundtrip_leaves_testbed_pristine() {
    let Some(game_dir) = testbed_or_skip() else {
        return;
    };
    let data = game_dir.join("Data");

    // The game dir's `Data/` must canonicalize to inside the testbed — rejects a junction
    // or reparse point that points `Data/` at a real install.
    let game_canon = game_dir.canonicalize_utf8().expect("canonicalize game dir");
    let data_canon = data.canonicalize_utf8().expect("canonicalize Data/");
    assert!(
        data_canon.starts_with(&game_canon),
        "`{data_canon}` is not inside the testbed game dir `{game_canon}`"
    );

    // Hold the cross-process lock for the whole test.
    let _lock = TestbedLock::acquire(&game_dir);

    // Deterministic working area on the same volume as the game (so hardlinks work and a
    // crash-orphaned journal survives). Lives inside the disposable copy, outside `Data/`.
    let work = game_dir.join(".overseer-e2e");
    let instance_root = work.join("instance");
    let local_dir = work.join("local");
    let ini_dir = work.join("ini");
    let backup_root = game_dir.join(".overseer-backup");

    preflight_clean(&instance_root, &data, &backup_root);
    // After cleanup, our namespace must be absent before we deploy.
    assert!(
        !data.join(E2E_PLUGIN).exists(),
        "stale `{E2E_PLUGIN}` survived cleanup"
    );
    assert!(
        !data.join("Textures").join(E2E_TEX_SUBDIR).exists(),
        "stale `{E2E_TEX_SUBDIR}/` survived cleanup"
    );

    // Fresh instance each run.
    let _ = std::fs::remove_dir_all(&work);
    let instance = {
        let mut inst = Instance::new(&instance_root, &game_dir);
        inst.config.local_dir = Some(local_dir.clone());
        inst.config.ini_dir = Some(ini_dir.clone());
        Instance::init(&instance_root, inst.config).expect("init e2e instance")
    };
    // Local/INI dirs must be explicitly redirected into the testbed, never the real
    // `%LOCALAPPDATA%` / `Documents\My Games`.
    assert_eq!(
        instance.config.local_dir.as_deref(),
        Some(local_dir.as_path())
    );
    assert_eq!(instance.config.ini_dir.as_deref(), Some(ini_dir.as_path()));
    assert!(local_dir.starts_with(&game_dir) && ini_dir.starts_with(&game_dir));

    // Sanity: the testbed really is a Fallout 4 install (and not contradictory about its store).
    let install = detect::detect(GameKind::Fallout4, &game_dir);
    assert_ne!(
        detect::edition(&install, &game_dir),
        detect::Edition::Undetermined
    );
    assert_ne!(install.store, detect::Store::Conflicting);

    // Stage the synthetic mod: a valid (master-free) plugin + a uniquely-pathed loose file.
    let mod_dir = instance.mods_dir().join(E2E_MOD);
    test_support::write_plugin(&mod_dir, E2E_PLUGIN, 0, &[]);
    test_support::install_mod(&instance, E2E_MOD, &[(E2E_TEX_REL, "overseer e2e marker")]);
    test_support::save_profile(&instance, "e2e", &[(E2E_MOD, true)]);

    let before = snapshot_data(&data);

    // --- Deploy ---
    apply::deploy_profile(&instance, "e2e", &NullSink).expect("deploy");

    let plugin_dest = data.join(E2E_PLUGIN);
    let tex_dest = data.join(E2E_TEX_REL);
    assert!(plugin_dest.exists(), "plugin not deployed into Data/");
    assert!(tex_dest.exists(), "texture not deployed into Data/");
    // Each deployed file is a hard link of its mod source (same inode), not a copy.
    assert!(
        same_file::is_same_file(&plugin_dest, mod_dir.join(E2E_PLUGIN)).unwrap(),
        "deployed plugin is not a hard link of its source"
    );
    assert!(
        same_file::is_same_file(&tex_dest, mod_dir.join(E2E_TEX_REL)).unwrap(),
        "deployed texture is not a hard link of its source"
    );
    // The active plugin lands in the real Plugins.txt.
    let plugins_txt = std::fs::read_to_string(local_dir.join("Plugins.txt")).expect("Plugins.txt");
    assert!(
        plugins_txt.contains(E2E_PLUGIN),
        "Plugins.txt missing our plugin:\n{plugins_txt}"
    );
    // Status reports a verified-live deployment.
    let status = apply::status(&instance)
        .expect("status")
        .expect("a live deployment");
    assert!(
        status.verified.is_ok(),
        "status reports missing files: {:?}",
        status.verified.missing
    );

    // --- Diagnose the live install --- the doctor pipeline must run end-to-end on a real
    // game without panicking. We don't assert "no errors" (our hermetic empty ini dir legitimately
    // trips ini-config); we assert it produced findings and the F4SE binary checks stayed clean.
    let report = overseer_diagnostics::diagnose(&instance, "e2e").expect("diagnose live install");
    assert!(!report.findings.is_empty(), "doctor produced no findings");
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "f4se" && f.severity == overseer_diagnostics::Severity::Error),
        "f4se flagged a runtime mismatch on the base testbed: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.check == "f4se")
            .collect::<Vec<_>>()
    );

    // --- Purge ---
    apply::purge(&instance, &NullSink).expect("purge");

    // Our files are gone, nothing was backed aside, and no journal remains.
    assert!(!plugin_dest.exists(), "plugin survived purge");
    assert!(
        !data.join("Textures").join(E2E_TEX_SUBDIR).exists(),
        "texture dir survived purge"
    );
    assert!(
        !backup_root.exists(),
        "a .overseer-backup remained after purge"
    );
    assert!(
        !matches!(apply::status(&instance), Ok(Some(s)) if s.deployment.status == Status::RecoveryFailed),
        "purge left a RecoveryFailed journal"
    );
    assert!(
        matches!(apply::status(&instance), Ok(None)),
        "a deployment journal survived purge"
    );

    // The headline guarantee: Data/ is byte-identical (by path+len) to before we touched it.
    let after = snapshot_data(&data);
    assert_pristine(&before, &after);

    // Clean pass leaves no trace behind.
    drop(instance);
    let _ = std::fs::remove_dir_all(&work);
}
