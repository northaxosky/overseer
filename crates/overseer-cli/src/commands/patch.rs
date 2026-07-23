//! `overseer patch ...`: rewrite game binaries and archive headers in place.

use crate::cli::{GenerationArg, PatchCommand};
use crate::context::{absolutize, open_instance};
use crate::ui::{Gate, Role, heading, preview_heading, styled, success};
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Header, Ba2Kind};
use overseer_core::detect::{self, Edition, Generation};
use overseer_core::game::GameKind;
use overseer_core::patch::delta::RustDeltaDecoder;
use overseer_core::patch::engine::{self, ConvertJob, ItemPlan, ItemState, Outcome, Policy};
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome, convert, dlc};
use overseer_core::patch::fingerprint::VerifiedBy;
use overseer_core::patch::vcdiff::{self, DeltaMap};
use std::collections::{HashMap, HashSet};

pub fn run(command: PatchCommand) -> Result<()> {
    match command {
        PatchCommand::Ba2 { path, to, gate } => ba2(&path, to, gate.gate()),
        PatchCommand::Convert {
            to,
            source,
            exe_delta,
            launcher_delta,
            steamapi_delta,
            allow_incomplete_repair,
            gate,
        } => convert_install(ConvertArgs {
            target: to.into_core(),
            deltas: source.deltas.as_deref(),
            instance: source.instance.as_deref(),
            game_dir: source.game_dir.as_deref(),
            exe_delta: exe_delta.as_deref(),
            launcher_delta: launcher_delta.as_deref(),
            steamapi_delta: steamapi_delta.as_deref(),
            allow_incomplete_repair,
            dry_run: gate.dry_run,
            yes: gate.yes,
        }),
        PatchCommand::DlcConsistency {
            source,
            allow_incomplete_repair,
            gate,
        } => dlc_consistency_install(DlcArgs {
            deltas: source.deltas.as_deref(),
            instance: source.instance.as_deref(),
            game_dir: source.game_dir.as_deref(),
            allow_incomplete_repair,
            dry_run: gate.dry_run,
            yes: gate.yes,
        }),
    }
}

#[derive(Clone, Copy)]
struct ConvertArgs<'a> {
    target: Generation,
    deltas: Option<&'a Utf8Path>,
    instance: Option<&'a Utf8Path>,
    game_dir: Option<&'a Utf8Path>,
    exe_delta: Option<&'a Utf8Path>,
    launcher_delta: Option<&'a Utf8Path>,
    steamapi_delta: Option<&'a Utf8Path>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

#[derive(Clone, Copy)]
struct DlcArgs<'a> {
    deltas: Option<&'a Utf8Path>,
    instance: Option<&'a Utf8Path>,
    game_dir: Option<&'a Utf8Path>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

/// Shared convert/dlc front matter: validate gate flags and resolve the game directory
fn conversion_env(
    instance: Option<&Utf8Path>,
    game_dir: Option<&Utf8Path>,
    allow_incomplete_repair: bool,
    gate: Gate,
) -> Result<Utf8PathBuf> {
    if allow_incomplete_repair && gate == Gate::Preview {
        bail!("--allow-incomplete-repair requires --yes");
    }
    let game_dir = resolve_game_dir(instance, game_dir)?;
    if !game_dir.is_dir() {
        bail!("no such game directory: {game_dir}");
    }
    Ok(game_dir)
}

/// Apply the built jobs when the gate says so; returns the converted count, or None on a preview
fn apply_conversion(game_dir: &Utf8Path, gate: Gate, jobs: &[ConvertJob]) -> Result<Option<usize>> {
    if gate.is_preview() {
        return Ok(None);
    }
    let max_target_size = jobs
        .iter()
        .map(|job| job.item.target.expected.size)
        .max()
        .unwrap_or(0);
    let decoder = RustDeltaDecoder::new(max_target_size);
    let outcomes = engine::convert(game_dir, jobs, &decoder)?;
    print_outcomes(&outcomes);
    Ok(Some(count_converted(&outcomes)))
}

fn convert_install(args: ConvertArgs<'_>) -> Result<()> {
    let gate = Gate::from_flags(args.dry_run, args.yes);
    let game_dir = conversion_env(
        args.instance,
        args.game_dir,
        args.allow_incomplete_repair,
        gate,
    )?;
    let install = detect::detect(GameKind::Fallout4, &game_dir);
    let edition = detect::edition(&install, &game_dir);
    let delta_map = resolve_core_deltas(args.deltas, args)?;
    let target = args.target;
    let target_for = |rel: &str| convert::core_target_spec(target, rel);
    let policy = Policy {
        groups: convert::CORE_GROUPS,
        target_for: &target_for,
        any_known_size: &convert::core_any_known_size,
        known_source: &convert::core_known_source,
    };
    if gate == Gate::Apply {
        engine::recover_install(&game_dir, &policy)?;
    }
    let item_plans = engine::plan(&game_dir, &policy)?;
    validate_edition_for_auto_convert(edition, &item_plans)?;
    print_convert_plan(ConvertPlanView {
        game_dir: &game_dir,
        edition,
        target,
        plans: &item_plans,
        deltas: &delta_map.mapped,
        allow_incomplete_repair: args.allow_incomplete_repair,
        gate,
    });
    warn_ignored_deltas(
        &delta_map.ignored,
        "core binaries",
        "overseer patch dlc-consistency",
        "DLC deltas",
    );
    let (jobs, complete_noop) = build_jobs(
        target.label(),
        &item_plans,
        &delta_map.mapped,
        args.allow_incomplete_repair,
    )?;
    if complete_noop {
        bail!("the selected files already match {}", target.label());
    }
    if let Some(converted) = apply_conversion(&game_dir, gate, &jobs)? {
        if args.allow_incomplete_repair {
            success(format!("{converted} file(s) repaired"));
        } else {
            success(format!(
                "{converted} file(s) converted to {}",
                target.label()
            ));
        }
    }
    Ok(())
}

fn dlc_consistency_install(args: DlcArgs<'_>) -> Result<()> {
    let gate = Gate::from_flags(args.dry_run, args.yes);
    let game_dir = conversion_env(
        args.instance,
        args.game_dir,
        args.allow_incomplete_repair,
        gate,
    )?;
    let allowed: Vec<&str> = dlc::DLC_GROUPS
        .iter()
        .flat_map(|g| g.files.iter().copied())
        .collect();
    let delta_map = resolve_dlc_deltas(args.deltas, &allowed)?;
    let policy = Policy {
        groups: dlc::DLC_GROUPS,
        target_for: &dlc::dlc_target,
        any_known_size: &dlc_no_known_size,
        known_source: &dlc_no_known_source,
    };
    if gate == Gate::Apply {
        engine::recover_install(&game_dir, &policy)?;
    }
    let item_plans = engine::plan(&game_dir, &policy)?;
    print_dlc_plan(DlcPlanView {
        game_dir: &game_dir,
        plans: &item_plans,
        deltas: &delta_map.mapped,
        allow_incomplete_repair: args.allow_incomplete_repair,
        gate,
    });
    warn_ignored_deltas(
        &delta_map.ignored,
        "DLC files",
        "overseer patch convert",
        "core deltas",
    );
    let (jobs, complete_noop) = build_jobs(
        "the DLC consistency revision",
        &item_plans,
        &delta_map.mapped,
        args.allow_incomplete_repair,
    )?;
    if complete_noop {
        bail!("the selected files already match the DLC consistency revision");
    }
    if let Some(converted) = apply_conversion(&game_dir, gate, &jobs)? {
        success(format!(
            "{converted} file(s) brought to the DLC consistency revision"
        ));
    }
    Ok(())
}

/// The DLC policy has no source table; its only identity is the target, handled by the size check
fn dlc_no_known_size(_: &str, _: u64) -> bool {
    false
}

fn dlc_no_known_source(
    _: &str,
    _: &overseer_core::patch::fingerprint::FileFingerprint,
) -> Option<String> {
    None
}

fn count_converted(outcomes: &[(String, Outcome)]) -> usize {
    outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, Outcome::Converted))
        .count()
}

fn print_outcomes(outcomes: &[(String, Outcome)]) {
    for (name, outcome) in outcomes {
        match outcome {
            Outcome::Converted => {
                println!("{}", styled(Role::Added, format!("+ {name}: converted")))
            }
            Outcome::AlreadyTarget => println!(
                "{}",
                styled(Role::Muted, format!("= {name}: already target"))
            ),
            Outcome::Missing => println!("{}", styled(Role::Muted, format!("- {name}: missing"))),
        }
    }
}

fn resolve_game_dir(
    instance: Option<&Utf8Path>,
    game_dir: Option<&Utf8Path>,
) -> Result<Utf8PathBuf> {
    if let Some(game_dir) = game_dir {
        return Ok(absolutize(game_dir)?);
    }
    let Some(instance_dir) = instance else {
        bail!("provide --game-dir or --instance");
    };
    Ok(open_instance(instance_dir)?.config.game_dir)
}

fn resolve_core_deltas(deltas: Option<&Utf8Path>, args: ConvertArgs<'_>) -> Result<DeltaMap> {
    let mut map = if let Some(dir) = deltas {
        if !dir.is_dir() {
            bail!("no such delta directory: {dir}");
        }
        vcdiff::map_deltas(dir, convert::core_binary_names())
            .with_context(|| format!("mapping deltas in {dir}"))?
    } else {
        DeltaMap::default()
    };
    for (name, path) in [
        ("Fallout4.exe", args.exe_delta),
        ("Fallout4Launcher.exe", args.launcher_delta),
        ("steam_api64.dll", args.steamapi_delta),
    ] {
        if let Some(path) = path {
            if !path.is_file() {
                bail!("delta for {name} not found: {path}");
            }
            map.mapped.insert(name.to_owned(), path.to_owned());
        }
    }
    Ok(map)
}

fn resolve_dlc_deltas(deltas: Option<&Utf8Path>, allowed: &[&str]) -> Result<DeltaMap> {
    let Some(dir) = deltas else {
        return Ok(DeltaMap::default());
    };
    if !dir.is_dir() {
        bail!("no such delta directory: {dir}");
    }
    vcdiff::map_deltas(dir, allowed).with_context(|| format!("mapping deltas in {dir}"))
}

fn warn_ignored_deltas(
    ignored: &[Utf8PathBuf],
    this_kind: &str,
    other_cmd: &str,
    other_kind: &str,
) {
    if ignored.is_empty() {
        return;
    }
    println!(
        "{}",
        styled(
            Role::Warning,
            format!(
                "~ {} delta(s) ignored (not {this_kind}); use `{other_cmd}` for {other_kind}",
                ignored.len()
            )
        )
    );
}

fn selected_groups<'a>(
    plans: &'a [ItemPlan],
    deltas: &HashMap<String, Utf8PathBuf>,
) -> HashSet<&'a str> {
    plans
        .iter()
        .filter(|plan| deltas.contains_key(plan.item.rel_path))
        .map(|plan| plan.item.group)
        .collect()
}

fn build_jobs(
    target_label: &str,
    plans: &[ItemPlan],
    deltas: &HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
) -> Result<(Vec<ConvertJob>, bool)> {
    let selected = selected_groups(plans, deltas);
    let mut jobs = Vec::new();
    let mut selected_files = 0usize;
    let mut already = 0usize;
    for plan in plans {
        // A group converts only when the user supplied a delta for one of its files
        if !selected.contains(plan.item.group) {
            continue;
        }
        selected_files += 1;
        match plan.state {
            ItemState::AlreadyTarget => already += 1,
            ItemState::Missing if allow_incomplete_repair => {}
            ItemState::Missing => bail!(
                "group {}: {} is missing; refusing partial group",
                plan.item.group,
                plan.item.rel_path
            ),
            ItemState::NeedsConversion => match deltas.get(plan.item.rel_path) {
                Some(delta) => jobs.push(ConvertJob {
                    item: plan.item,
                    delta: delta.clone(),
                }),
                None if allow_incomplete_repair => {}
                None => bail!(
                    "group {}: missing delta for {}; refusing partial group",
                    plan.item.group,
                    plan.item.rel_path
                ),
            },
        }
    }
    if jobs.is_empty() {
        if selected_files > 0 && already == selected_files {
            return Ok((jobs, true));
        }
        if !allow_incomplete_repair {
            bail!("no files can be converted to {target_label}");
        }
    }
    Ok((jobs, false))
}

fn validate_edition_for_auto_convert(edition: Edition, plans: &[ItemPlan]) -> Result<()> {
    if matches!(
        edition,
        Edition::OldGen | Edition::NextGen | Edition::Anniversary
    ) {
        return Ok(());
    }
    // Only the core binaries define the install edition; the target-hash gate is the real safety net
    if plans
        .iter()
        .filter(|plan| plan.item.group == "core" && !matches!(plan.state, ItemState::Missing))
        .all(|plan| plan.known_source.is_some())
    {
        return Ok(());
    }
    bail!(
        "detected {edition:?}; refusing auto-convert because at least one source binary is unknown"
    )
}

struct ConvertPlanView<'a> {
    game_dir: &'a Utf8Path,
    edition: Edition,
    target: Generation,
    plans: &'a [ItemPlan],
    deltas: &'a HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
    gate: Gate,
}

fn print_convert_plan(view: ConvertPlanView<'_>) {
    if view.gate.is_preview() {
        preview_heading(view.gate);
    } else if view.allow_incomplete_repair {
        heading(format!("Repairing selected binaries in {}", view.game_dir));
    } else {
        heading(format!(
            "Converting the core binaries to {} in {}",
            view.target.label(),
            view.game_dir
        ));
    }
    println!("Detected edition (Fallout4.exe): {:?}", view.edition);
    println!("Backups: <binary>.overseer-bak beside each converted file");
    let label = view.target.label().to_owned();
    print_plan_lines(view.plans, view.deltas, true, |_| label.clone());
}

struct DlcPlanView<'a> {
    game_dir: &'a Utf8Path,
    plans: &'a [ItemPlan],
    deltas: &'a HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
    gate: Gate,
}

fn print_dlc_plan(view: DlcPlanView<'_>) {
    if view.gate.is_preview() {
        preview_heading(view.gate);
    } else if view.allow_incomplete_repair {
        heading(format!("Repairing selected DLC files in {}", view.game_dir));
    } else {
        heading(format!(
            "Applying the DLC consistency revision in {}",
            view.game_dir
        ));
    }
    println!("Backups: <file>.overseer-bak beside each converted file");
    print_plan_lines(view.plans, view.deltas, false, |plan| {
        format!(
            "consistency ({})",
            dlc::dlc_note(plan.item.rel_path).unwrap_or("revision")
        )
    });
}

fn print_plan_lines(
    plans: &[ItemPlan],
    deltas: &HashMap<String, Utf8PathBuf>,
    show_source: bool,
    label_for: impl Fn(&ItemPlan) -> String,
) {
    let selected = selected_groups(plans, deltas);
    for plan in plans {
        let (role, msg) = plan_line(
            plan,
            deltas.get(plan.item.rel_path),
            selected.contains(plan.item.group),
            &label_for(plan),
            show_source,
        );
        println!("{}", styled(role, msg));
    }
}

/// Build the styled plan line for one item; `show_source` names the identified source edition
/// (core edition flips), and stays off for source-agnostic policies like the DLC revision
fn plan_line(
    plan: &ItemPlan,
    delta: Option<&Utf8PathBuf>,
    selected: bool,
    target_label: &str,
    show_source: bool,
) -> (Role, String) {
    let source = show_source.then(|| plan.known_source.as_deref().unwrap_or("unknown source"));
    let delta_label = delta
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "no delta".to_owned());
    let gate = match plan.item.target.expected.verified_by() {
        VerifiedBy::Sha256 => String::new(),
        VerifiedBy::Crc32 => format!("; verified by {}", VerifiedBy::Crc32.label()),
    };
    match plan.state {
        ItemState::AlreadyTarget => (
            Role::Muted,
            format!("= {}: already {target_label}{gate}", plan.item.rel_path),
        ),
        ItemState::Missing => (
            Role::Muted,
            format!("- {}: missing{gate}", plan.item.rel_path),
        ),
        ItemState::NeedsConversion if !selected => {
            let prefix = source.map(|s| format!("{s}, ")).unwrap_or_default();
            (
                Role::Muted,
                format!(
                    "~ {}: {prefix}leaving as-is (no delta supplied)",
                    plan.item.rel_path
                ),
            )
        }
        ItemState::NeedsConversion => {
            let prefix = source.map(|s| format!("{s} -> ")).unwrap_or_default();
            (
                Role::Added,
                format!(
                    "+ {}: {prefix}{target_label}; delta: {delta_label}{gate}",
                    plan.item.rel_path
                ),
            )
        }
    }
}

fn ba2(path: &Utf8Path, to: GenerationArg, gate: Gate) -> Result<()> {
    if !path.exists() {
        bail!("no such file or directory: {path}")
    }

    let is_dir = path.is_dir();
    let candidates = if is_dir {
        collect_ba2(path)?
    } else {
        vec![path.to_owned()]
    };
    if candidates.is_empty() {
        println!("No .ba2 files found in {path}");
        return Ok(());
    }

    let writing = gate == Gate::Apply;
    preview_heading(gate);

    let generation = to.into_core();
    let target = Ba2Edition::from_generation(generation);
    let label = generation.tag();
    let mut tally = Tally::default();

    for file in &candidates {
        let name = if is_dir {
            file.file_name().unwrap_or_else(|| file.as_str())
        } else {
            file.as_str()
        };
        match classify(file, target, writing) {
            Line::Patched { from, to } => {
                tally.patched += 1;
                let verb = if writing { "patched" } else { "would patch" };
                println!(
                    "{}",
                    styled(Role::Added, format!("+ {name}: {verb} v{from} -> v{to}"))
                );
            }
            Line::Already { version } => {
                tally.already += 1;
                println!(
                    "{}",
                    styled(
                        Role::Muted,
                        format!("= {name}: already {label} (v{version})")
                    )
                );
            }
            Line::Skipped(reason) => {
                tally.skipped += 1;
                println!(
                    "{}",
                    styled(Role::Muted, format!("- {name}: skipped - {reason}"))
                );
            }
            Line::Failed(msg) => {
                tally.errors += 1;
                println!("{}", styled(Role::Failure, format!("x {name}: {msg}")));
            }
        }
    }

    if candidates.len() > 1 {
        print_summary(&tally);
    }
    if tally.errors > 0 {
        bail!("{} archive(s) could not be patched", tally.errors);
    }
    Ok(())
}

enum Line {
    Patched { from: u32, to: u32 },
    Already { version: u32 },
    Skipped(String),
    Failed(String),
}

fn classify(file: &Utf8Path, target: Ba2Edition, writing: bool) -> Line {
    if is_symlink(file) {
        return Line::Skipped("symlink".to_owned());
    }
    let outcome = if writing {
        fallout4::set_edition(file, target)
    } else {
        Ba2Header::read(file).map(|h| fallout4::plan(&h, target))
    };
    match outcome {
        Ok(PatchOutcome::Patched { from, to }) => Line::Patched { from, to },
        Ok(PatchOutcome::AlreadyTarget { version }) => Line::Already { version },
        Ok(PatchOutcome::Unsupported { version, kind }) => Line::Skipped(format!(
            "unsupported BA2 (v{version}, {})",
            kind_label(kind)
        )),
        Err(e) => Line::Failed(e.to_string()),
    }
}

#[derive(Debug, Default)]
struct Tally {
    patched: u32,
    already: u32,
    skipped: u32,
    errors: u32,
}

fn print_summary(t: &Tally) {
    let line = format!(
        "{} patched, {} already, {} skipped",
        t.patched, t.already, t.skipped
    );
    if t.errors > 0 {
        println!(
            "{}",
            styled(Role::Warning, format!("{line}, {} error(s)", t.errors))
        );
    } else {
        success(line);
    }
}

fn collect_ba2(dir: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut out = Vec::new();
    for entry in dir
        .read_dir_utf8()
        .with_context(|| format!("reading an entry in {dir}"))?
    {
        let entry = entry.with_context(|| format!("reading an entry in {dir}"))?;
        let path = entry.path();
        let is_ba2 = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ba2"));
        if is_ba2 && path.is_file() {
            out.push(path.to_owned());
        }
    }
    out.sort();
    Ok(out)
}

fn is_symlink(path: &Utf8Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

fn kind_label(kind: Ba2Kind) -> String {
    match kind {
        Ba2Kind::General => "GNRL".to_owned(),
        Ba2Kind::Texture => "DX10".to_owned(),
        Ba2Kind::Other(tag) => String::from_utf8_lossy(&tag).into_owned(),
    }
}

#[cfg(test)]
#[path = "tests/patch.rs"]
mod tests;
