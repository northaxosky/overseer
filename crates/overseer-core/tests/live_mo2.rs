//! Opt-in READ-ONLY integration test against a real Mod Organizer 2 instance.
//!
//! Overseer reads MO2's on-disk layout directly (`mods/<name>/`, `profiles/<name>/modlist.txt`), so
//! this points an `Instance` straight at a real MO2 instance and exercises the whole read/analyse
//! stack — load the profile, detect conflicts, run diagnostics — against real, rich data. Set
//! `OVERSEER_MO2_INSTANCE` to the instance root (the folder holding `ModOrganizer.ini`); unset, it
//! skips so the normal suite stays install-free. It is strictly **read-only** — it never deploys,
//! purges, or writes — so it is safe to run against a live instance.

use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::deploy::{ConflictSnapshot, ModSource};
use overseer_core::game::GameKind;
use overseer_core::instance::{Instance, ModKind, Profile};

/// The MO2 instance root from `OVERSEER_MO2_INSTANCE`, or `None` with a skip note when unset/invalid
fn mo2_instance_or_skip() -> Option<Utf8PathBuf> {
    // Load `.env` (machine-specific harness paths) if present; real shell env vars still win
    let _ = dotenvy::dotenv();
    let Ok(dir) = std::env::var("OVERSEER_MO2_INSTANCE") else {
        eprintln!("skipping: set OVERSEER_MO2_INSTANCE to a real MO2 instance root to run");
        return None;
    };
    let dir = Utf8PathBuf::from(dir);
    if !dir.join("ModOrganizer.ini").exists() {
        eprintln!("skipping: no ModOrganizer.ini under OVERSEER_MO2_INSTANCE={dir}");
        return None;
    }
    Some(dir)
}

/// Parse `gamePath=@ByteArray(...)` out of `ModOrganizer.ini`, unescaping MO2's doubled backslashes
fn game_dir_from_ini(instance_root: &Utf8Path) -> Option<Utf8PathBuf> {
    let ini = std::fs::read_to_string(instance_root.join("ModOrganizer.ini")).ok()?;
    let line = ini
        .lines()
        .find(|l| l.trim_start().starts_with("gamePath="))?;
    let value = line.split_once('=')?.1.trim();
    let inner = value
        .strip_prefix("@ByteArray(")
        .and_then(|v| v.strip_suffix(')'))
        .unwrap_or(value);
    Some(Utf8PathBuf::from(inner.replace("\\\\", "\\")))
}

/// A read-only `Instance` over the MO2 layout (shared `mods/`+`profiles/`, per-profile `plugins.txt`/inis, game dir from `ModOrganizer.ini`)
fn mo2_instance(root: &Utf8Path) -> Instance {
    let game_dir = game_dir_from_ini(root).unwrap_or_else(|| root.join("__no_game__"));
    let mut instance = Instance::new(root, game_dir);
    instance.config.game = GameKind::Fallout4;
    instance
}

/// Point the instance's `local_dir`/`ini_dir` at a profile's dir (where MO2 keeps `plugins.txt`)
fn with_profile_dirs(mut instance: Instance, profile: &str) -> Instance {
    let profile_dir = instance.profile_dir(profile);
    instance.config.local_dir = Some(profile_dir.clone());
    instance.config.ini_dir = Some(profile_dir);
    instance
}

/// The first profile under `profiles/`
fn first_profile(instance: &Instance) -> String {
    instance
        .profiles()
        .expect("read profiles/")
        .into_iter()
        .next()
        .expect("the MO2 instance has at least one profile")
}

/// Enabled, managed mods whose staging dir exists (skips separators, foreign, and orphaned entries)
fn deployable_sources(instance: &Instance, profile: &Profile) -> Vec<ModSource> {
    profile
        .items()
        .rev()
        .filter(|e| e.enabled && e.kind == ModKind::Managed)
        .map(|e| (e.name.clone(), instance.mods_dir().join(&e.name)))
        .filter(|(_, dir)| dir.is_dir())
        .map(|(name, dir)| ModSource::new(name, dir))
        .collect()
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_MO2_INSTANCE set"]
fn loads_a_real_mo2_profile() {
    let Some(root) = mo2_instance_or_skip() else {
        return;
    };
    let instance = mo2_instance(&root);
    let profile_name = first_profile(&instance);
    let profile = Profile::load(&instance, &profile_name).expect("load the MO2 profile");

    let managed = profile
        .items()
        .filter(|m| m.kind == ModKind::Managed)
        .count();
    let separators = profile
        .rows()
        .iter()
        .filter(|row| matches!(row, overseer_core::instance::ModRow::Separator(_)))
        .count();
    eprintln!(
        "profile `{profile_name}`: {} entries ({managed} managed, {separators} separators)",
        profile.rows().len()
    );
    assert!(
        managed > 10,
        "a real MO2 profile should list many mods, got {}",
        managed
    );
    // Every enabled managed mod resolves to a real dir under mods/
    for entry in profile
        .items()
        .filter(|m| m.enabled && m.kind == ModKind::Managed)
    {
        assert!(
            instance.mods_dir().join(&entry.name).is_dir(),
            "enabled mod `{}` has no mods/ dir",
            entry.name
        );
    }
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_MO2_INSTANCE set"]
fn detects_conflicts_across_real_mods() {
    let Some(root) = mo2_instance_or_skip() else {
        return;
    };
    let instance = mo2_instance(&root);
    let profile_name = first_profile(&instance);
    let profile = Profile::load(&instance, &profile_name).expect("load");

    let sources = deployable_sources(&instance, &profile);
    assert!(!sources.is_empty(), "expected some deployable mods");

    // The planning-layer conflict detector must run cleanly over a real, many-mod load order
    let snapshot = ConflictSnapshot::build(&sources).expect("conflict detection runs on real mods");
    eprintln!(
        "{} conflicting file path(s) across {} deployable mods",
        snapshot.len(),
        sources.len()
    );
}

#[test]
#[ignore = "read-only live harness; run with OVERSEER_MO2_INSTANCE set"]
fn diagnoses_a_real_mo2_instance() {
    let Some(root) = mo2_instance_or_skip() else {
        return;
    };
    let profile_name = first_profile(&mo2_instance(&root));
    let instance = with_profile_dirs(mo2_instance(&root), &profile_name);

    // The full doctor pipeline must run end-to-end on a real instance without panicking
    let report = overseer_diagnostics::diagnose(&instance, &profile_name)
        .expect("diagnose the MO2 instance");
    assert!(
        !report.findings.is_empty(),
        "diagnostics produced no findings"
    );
    for f in &report.findings {
        eprintln!("[{:?}] {}: {}", f.severity, f.check, f.title);
    }
}
