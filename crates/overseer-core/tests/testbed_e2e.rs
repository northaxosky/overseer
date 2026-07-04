//! Opt-in **destructive** end-to-end tests that deploy mods into a real Fallout 4
//! install and then purge, proving the transaction leaves the game directory
//! byte-for-byte as it found it.
//!
//! These are `#[ignore]`d and gated on `OVERSEER_FO4_TESTBED`, so they never run in the
//! normal suite. Run them deliberately against a **disposable copy** (single-threaded, since
//! both tests share the one `Data/`):
//!
//! ```text
//! cargo test -p overseer-core --test testbed_e2e -- --ignored --nocapture --test-threads=1
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
//!
//! ## Two round-trips
//!
//! - `deploy_purge_roundtrip_leaves_testbed_pristine` deploys a **synthetic** mod in a private
//!   namespace (the unique-paths guarantee above).
//! - `deploy_purge_roundtrip_with_real_mods_leaves_testbed_pristine` copies a **curated set of
//!   real mods** from the standing testbed instance (`OVERSEER_TESTBED`) and deploys those — real
//!   plugins, `.ba2` archives, loose F4SE DLLs, and a genuine two-mod file conflict. Its safety
//!   rests on the marker gate, the lock, and the pristine snapshot rather than unique paths, so
//!   keep the ignored set single-threaded to hold the two off the shared `Data/`.

use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::apply::{self, Status};
use overseer_core::deploy::{DeployPlan, NullSink, detect_conflicts};
use overseer_core::detect;
use overseer_core::game::GameKind;
use overseer_core::instance::{Instance, Profile};
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

/// The synthetic round-trip's deterministic working dir, relative to the game dir.
const SYNTHETIC_WORK: &str = ".overseer-e2e";
/// The curated real-mod round-trip's deterministic working dir, relative to the game dir.
const REAL_WORK: &str = ".overseer-e2e-real";

/// The `OVERSEER_FO4_TESTBED` dir; unset skips, but set-and-unsafe panics so misconfiguration cannot pass.
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

/// A best-effort cross-process `create_new` lock, removed on drop so panicking tests still clear it.
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

/// Snapshot files under `data` as `lowercased-relative-path -> length`, ignoring dirs so empty-dir churn is not drift.
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

/// Reverse a committed deployment left by *either* destructive round-trip (they share one `Data/`+lock) so a crash can't leak into the next run's baseline.
fn recover_leftover_deployments(game_dir: &Utf8Path) {
    for work in [SYNTHETIC_WORK, REAL_WORK] {
        let root = game_dir.join(work).join("instance");
        // `status` runs crash-recovery; `purge` clears a committed deployment via its journal.
        if Instance::config_path(&root).exists()
            && let Ok(prev) = Instance::load(&root)
            && matches!(apply::status(&prev), Ok(Some(_)))
        {
            apply::purge(&prev, &NullSink).expect("preflight purge of a leftover deployment");
        }
    }
}

/// Scrub the synthetic namespace before deploy (reversal already ran via `recover_leftover_deployments`; this only clears orphans if the instance dir was lost).
fn preflight_clean(data: &Utf8Path, backup_root: &Utf8Path) {
    // (a) Orphan catch for the case the instance dir itself was lost: delete our files directly. Safe because these paths are ours alone and never collide with base-game files.
    let _ = std::fs::remove_file(data.join(E2E_PLUGIN));
    let _ = std::fs::remove_dir_all(data.join("Textures").join(E2E_TEX_SUBDIR));

    // (b) The unique-path design never backs a base file aside, so the backup root must be empty. Anything else means a real file was clobbered — stop rather than guess.
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

    // The game dir's `Data/` must canonicalize to inside the testbed — rejects a junction; or reparse point that points `Data/` at a real install.
    let game_canon = game_dir.canonicalize_utf8().expect("canonicalize game dir");
    let data_canon = data.canonicalize_utf8().expect("canonicalize Data/");
    assert!(
        data_canon.starts_with(&game_canon),
        "`{data_canon}` is not inside the testbed game dir `{game_canon}`"
    );

    // Hold the cross-process lock for the whole test.
    let _lock = TestbedLock::acquire(&game_dir);

    // Deterministic working area on the same volume as the game (so hardlinks work and a; crash-orphaned journal survives). Lives inside the disposable copy, outside `Data/`.
    let work = game_dir.join(SYNTHETIC_WORK);
    let instance_root = work.join("instance");
    let local_dir = work.join("local");
    let ini_dir = work.join("ini");
    let backup_root = game_dir.join(".overseer-backup");

    recover_leftover_deployments(&game_dir);
    preflight_clean(&data, &backup_root);
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
    // Local/INI dirs must be explicitly redirected into the testbed, never the real; `%LOCALAPPDATA%` / `Documents\My Games`.
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

    // --- Diagnose the live install --- the doctor pipeline must run end-to-end on a real; game without panicking. We don't assert "no errors" (our hermetic empty ini dir legitimately; trips ini-config); we assert it produced findings and the F4SE binary checks stayed clean.
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

// ---------------------------------------------------------------------------; Curated real-mod round-trip; ---------------------------------------------------------------------------

/// Curated mods from the standing testbed instance (`OVERSEER_TESTBED`) covering a real conflict (`Interface/MCM.swf`), an ESM master, `.ba2` archives, and loose F4SE plugin DLLs; highest-priority first so the first entry wins.
const CURATED_MODS: &[&str] = &[
    "Mod Configuration Menu", // Interface/MCM.swf — conflict winner (highest priority)
    "Fallout 76 Style Main Menu", // Interface/MCM.swf — conflict loser
    "Extended Dialogue Interface", // XDI.esm (a master) + .ba2
    "Community Fixes Merged", // .esp + .ba2
    "Cell Offset Generator",  // .esp + F4SE plugin DLL
    "House Rules",            // .esp + F4SE plugin DLL
];

/// The standing testbed instance whose `mods/` supplies the curated subset, from `OVERSEER_TESTBED`; unset skips.
fn testbed_source_or_skip() -> Option<Utf8PathBuf> {
    let _ = dotenvy::dotenv();
    let Ok(dir) = std::env::var("OVERSEER_TESTBED") else {
        eprintln!(
            "skipping: set OVERSEER_TESTBED to the standing testbed instance to source curated mods"
        );
        return None;
    };
    let dir = Utf8PathBuf::from(dir);
    if !dir.join("mods").is_dir() {
        eprintln!("skipping: no mods/ under OVERSEER_TESTBED={dir}");
        return None;
    }
    Some(dir)
}

/// Recursively copy a directory tree; stages a read-only MO2 mod onto the testbed volume so hardlink deploy works.
fn copy_tree(src: &Utf8Path, dst: &Utf8Path) {
    for entry in WalkDir::new(src) {
        let entry = entry.expect("walk mod source");
        let abs = Utf8Path::from_path(entry.path()).expect("utf8 mod path");
        let rel = abs.strip_prefix(src).expect("under src");
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).expect("mkdir mod subdir");
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("mkdir mod parent");
            }
            std::fs::copy(abs, &target).expect("copy mod file");
        }
    }
}

/// Top-level plugin filenames (`.esp`/`.esm`/`.esl`) directly under a staged mod dir.
fn mod_plugins(mod_dir: &Utf8Path) -> Vec<String> {
    let Ok(read_dir) = std::fs::read_dir(mod_dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let l = name.to_lowercase();
            l.ends_with(".esp") || l.ends_with(".esm") || l.ends_with(".esl")
        })
        .collect()
}

#[test]
#[ignore = "destructive; run against OVERSEER_FO4_TESTBED with OVERSEER_TESTBED set"]
fn deploy_purge_roundtrip_with_real_mods_leaves_testbed_pristine() {
    let Some(game_dir) = testbed_or_skip() else {
        return;
    };
    let Some(source) = testbed_source_or_skip() else {
        return;
    };
    let data = game_dir.join("Data");

    // The game dir's `Data/` must canonicalize inside the testbed — rejects a junction pointing; `Data/` at a real install.
    let game_canon = game_dir.canonicalize_utf8().expect("canonicalize game dir");
    let data_canon = data.canonicalize_utf8().expect("canonicalize Data/");
    assert!(
        data_canon.starts_with(&game_canon),
        "`{data_canon}` is not inside the testbed game dir `{game_canon}`"
    );

    let _lock = TestbedLock::acquire(&game_dir);

    // Deterministic working area on the game's volume, in its own dir (distinct from the synthetic; test) so a crash-orphaned journal survives for the next run to reverse.
    let work = game_dir.join(REAL_WORK);
    let instance_root = work.join("instance");
    let local_dir = work.join("local");
    let ini_dir = work.join("ini");
    let backup_root = game_dir.join(".overseer-backup");

    // Reverse any leftover deployment from a crashed prior run of *either* round-trip before we; snapshot the baseline, so leaked files can't masquerade as pristine.
    recover_leftover_deployments(&game_dir);
    let backup_empty = !backup_root.exists()
        || std::fs::read_dir(&backup_root).map_or(true, |mut d| d.next().is_none());
    assert!(
        backup_empty,
        "`{backup_root}` is non-empty before deploy — inspect or re-copy the testbed"
    );

    // Fresh instance each run, local/INI redirected into the testbed (never the real profile dirs).
    let _ = std::fs::remove_dir_all(&work);
    let instance = {
        let mut inst = Instance::new(&instance_root, &game_dir);
        inst.config.local_dir = Some(local_dir.clone());
        inst.config.ini_dir = Some(ini_dir.clone());
        Instance::init(&instance_root, inst.config).expect("init real-mod e2e instance")
    };
    assert!(local_dir.starts_with(&game_dir) && ini_dir.starts_with(&game_dir));

    // Sanity: the testbed really is a Fallout 4 install.
    let install = detect::detect(GameKind::Fallout4, &game_dir);
    assert_ne!(
        detect::edition(&install, &game_dir),
        detect::Edition::Undetermined
    );

    // Stage the curated subset onto the testbed volume — hardlink deploy needs the staging dir on; the same volume as `Data/`. Skip any the user no longer has; require enough for a real run.
    let mut staged: Vec<&str> = Vec::new();
    for &name in CURATED_MODS {
        let src = source.join("mods").join(name);
        if !src.is_dir() {
            eprintln!("note: curated mod `{name}` absent from the MO2 source; skipping it");
            continue;
        }
        copy_tree(&src, &instance.mods_dir().join(name));
        staged.push(name);
    }
    assert!(
        staged.len() >= 4,
        "only {} curated mods present; need >=4 for a meaningful run",
        staged.len()
    );

    // Profile in curated order (highest priority first) so the first conflict provider wins.
    let enabled: Vec<(&str, bool)> = staged.iter().map(|&n| (n, true)).collect();
    test_support::save_profile(&instance, "real", &enabled);

    let before = snapshot_data(&data);

    // --- Deploy ---
    apply::deploy_profile(&instance, "real", &NullSink).expect("deploy real mods");

    // (1) Every planned file is deployed as a hard link of its mod source, not a copy — covers; plugins, archives, loose textures, and each conflict winner in one sweep.
    let profile = Profile::load(&instance, "real").expect("reload profile");
    let sources = profile.deploy_sources(&instance);
    let plan = DeployPlan::from_rooted_mods(&game_dir, &sources).expect("build deploy plan");
    for file in plan.files() {
        let dest = game_dir.join(&file.relative);
        assert!(
            dest.exists(),
            "planned file not deployed: {}",
            file.relative
        );
        assert!(
            same_file::is_same_file(&dest, &file.source).unwrap(),
            "deployed `{}` (from `{}`) is not a hard link of its source",
            file.relative,
            file.winner
        );
    }

    // (2) The real conflict resolves to the higher-priority mod. The curated set makes the two; MCM mods share `Interface/MCM.swf`; the one listed first must win.
    let conflicts = detect_conflicts(&sources).expect("detect conflicts on real mods");
    assert!(
        !conflicts.is_empty(),
        "curated set produced no file conflict"
    );
    if staged.contains(&"Mod Configuration Menu") && staged.contains(&"Fallout 76 Style Main Menu")
    {
        let mcm = conflicts
            .iter()
            .find(|c| c.relative.as_str().to_lowercase().ends_with("mcm.swf"))
            .expect("the two MCM mods must conflict on MCM.swf");
        assert_eq!(
            mcm.providers.last().map(String::as_str),
            Some("Mod Configuration Menu"),
            "highest-priority mod should win the MCM.swf conflict; providers were {:?}",
            mcm.providers
        );
    }

    // (3) Every staged plugin lands in the real Plugins.txt as its own active entry. Parse line by; line (stripping the `*` active marker) so a name can't false-match as a substring.
    let plugins_txt =
        std::fs::read_to_string(local_dir.join("Plugins.txt")).expect("Plugins.txt written");
    let active: std::collections::BTreeSet<String> = plugins_txt
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim_start_matches('*').trim().to_lowercase())
        .collect();
    for &name in &staged {
        for plugin in mod_plugins(&instance.mods_dir().join(name)) {
            assert!(
                active.contains(&plugin.to_lowercase()),
                "Plugins.txt has no active entry `{plugin}` from `{name}`; active set: {active:?}"
            );
        }
    }

    // (4) Status reports a verified-live deployment.
    let status = apply::status(&instance)
        .expect("status")
        .expect("a live deployment");
    assert!(
        status.verified.is_ok(),
        "status reports missing files: {:?}",
        status.verified.missing
    );

    // (5) The doctor pipeline runs end-to-end on the real deployed install without panicking and; stays clean on the F4SE binary checks (as in the synthetic round-trip).
    let report =
        overseer_diagnostics::diagnose(&instance, "real").expect("diagnose live real-mod install");
    assert!(!report.findings.is_empty(), "doctor produced no findings");
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "f4se" && f.severity == overseer_diagnostics::Severity::Error),
        "f4se flagged a runtime mismatch on the testbed: {:?}",
        report
            .findings
            .iter()
            .filter(|f| f.check == "f4se")
            .collect::<Vec<_>>()
    );

    // --- Purge ---
    apply::purge(&instance, &NullSink).expect("purge real mods");

    for file in plan.files() {
        assert!(
            !game_dir.join(&file.relative).exists(),
            "deployed file survived purge: {}",
            file.relative
        );
    }
    assert!(
        !backup_root.exists(),
        "a .overseer-backup remained after purge"
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
