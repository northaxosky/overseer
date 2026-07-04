//! `overseer patch ...`: rewrite game binaries and archive headers in place.

use crate::cli::{GenerationArg, PatchCommand};
use crate::context::{absolutize, open_instance};
use crate::ui::{Role, heading, styled, success};
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Header, Ba2Kind};
use overseer_core::detect::{self, Edition, Generation};
use overseer_core::game::GameKind;
use overseer_core::patch::delta::Xdelta3CliDecoder;
use overseer_core::patch::fallout4::engine::{
    self, ConvertJob, ItemPlan, ItemState, Outcome, Policy,
};
use overseer_core::patch::fallout4::vcdiff::{self, DeltaMap};
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome, convert, dlc};
use overseer_core::patch::fingerprint::VerifiedBy;
use std::collections::{HashMap, HashSet};
use std::env;
use std::process::Command;

pub fn run(command: PatchCommand) -> Result<()> {
    match command {
        PatchCommand::Ba2 {
            path,
            to,
            dry_run,
            yes,
        } => ba2(&path, to, dry_run, yes),
        PatchCommand::Convert {
            to,
            deltas,
            instance,
            game_dir,
            exe_delta,
            launcher_delta,
            steamapi_delta,
            xdelta3,
            allow_incomplete_repair,
            dry_run,
            yes,
        } => convert_install(ConvertArgs {
            target: to.into_core(),
            deltas: deltas.as_deref(),
            instance: instance.as_deref(),
            game_dir: game_dir.as_deref(),
            exe_delta: exe_delta.as_deref(),
            launcher_delta: launcher_delta.as_deref(),
            steamapi_delta: steamapi_delta.as_deref(),
            xdelta3: xdelta3.as_deref(),
            allow_incomplete_repair,
            dry_run,
            yes,
        }),
        PatchCommand::DlcConsistency {
            deltas,
            instance,
            game_dir,
            xdelta3,
            allow_incomplete_repair,
            dry_run,
            yes,
        } => dlc_consistency_install(DlcArgs {
            deltas: deltas.as_deref(),
            instance: instance.as_deref(),
            game_dir: game_dir.as_deref(),
            xdelta3: xdelta3.as_deref(),
            allow_incomplete_repair,
            dry_run,
            yes,
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
    xdelta3: Option<&'a Utf8Path>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

#[derive(Clone, Copy)]
struct DlcArgs<'a> {
    deltas: Option<&'a Utf8Path>,
    instance: Option<&'a Utf8Path>,
    game_dir: Option<&'a Utf8Path>,
    xdelta3: Option<&'a Utf8Path>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

#[derive(Debug)]
struct ResolvedXdelta3 {
    path: Utf8PathBuf,
    version: String,
}

fn convert_install(args: ConvertArgs<'_>) -> Result<()> {
    if args.allow_incomplete_repair && !args.yes && !args.dry_run {
        bail!("--allow-incomplete-repair requires --yes");
    }
    let game_dir = resolve_game_dir(args.instance, args.game_dir)?;
    if !game_dir.is_dir() {
        bail!("no such game directory: {game_dir}");
    }
    if !convert::target_is_complete(args.target) {
        bail!(
            "target {} is incomplete; refusing conversion",
            args.target.label()
        );
    }
    let xdelta3 = resolve_xdelta3(args.xdelta3)?;
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
    if args.yes && !args.dry_run {
        engine::recover_install(&game_dir, &policy)?;
    }
    let item_plans = engine::plan(&game_dir, &policy)?;
    validate_edition_for_auto_convert(edition, &item_plans)?;
    print_convert_plan(ConvertPlanView {
        game_dir: &game_dir,
        edition,
        target,
        xdelta3: &xdelta3,
        plans: &item_plans,
        deltas: &delta_map.mapped,
        allow_incomplete_repair: args.allow_incomplete_repair,
        dry_run: args.dry_run,
        yes: args.yes,
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
    if args.dry_run || !args.yes {
        return Ok(());
    }
    let decoder = Xdelta3CliDecoder::new(xdelta3.path);
    let outcomes = engine::convert(&game_dir, &jobs, &decoder)?;
    print_outcomes(&outcomes);
    let converted = count_converted(&outcomes);
    if args.allow_incomplete_repair {
        success(format!("{converted} file(s) repaired"));
    } else {
        success(format!(
            "{converted} file(s) converted to {}",
            target.label()
        ));
    }
    Ok(())
}

fn dlc_consistency_install(args: DlcArgs<'_>) -> Result<()> {
    if args.allow_incomplete_repair && !args.yes && !args.dry_run {
        bail!("--allow-incomplete-repair requires --yes");
    }
    let game_dir = resolve_game_dir(args.instance, args.game_dir)?;
    if !game_dir.is_dir() {
        bail!("no such game directory: {game_dir}");
    }
    let xdelta3 = resolve_xdelta3(args.xdelta3)?;
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
    if args.yes && !args.dry_run {
        engine::recover_install(&game_dir, &policy)?;
    }
    let item_plans = engine::plan(&game_dir, &policy)?;
    print_dlc_plan(DlcPlanView {
        game_dir: &game_dir,
        xdelta3: &xdelta3,
        plans: &item_plans,
        deltas: &delta_map.mapped,
        allow_incomplete_repair: args.allow_incomplete_repair,
        dry_run: args.dry_run,
        yes: args.yes,
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
    if args.dry_run || !args.yes {
        return Ok(());
    }
    let decoder = Xdelta3CliDecoder::new(xdelta3.path);
    let outcomes = engine::convert(&game_dir, &jobs, &decoder)?;
    print_outcomes(&outcomes);
    let converted = count_converted(&outcomes);
    success(format!(
        "{converted} file(s) brought to the DLC consistency revision"
    ));
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
    xdelta3: &'a ResolvedXdelta3,
    plans: &'a [ItemPlan],
    deltas: &'a HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

fn print_convert_plan(view: ConvertPlanView<'_>) {
    if view.dry_run {
        heading("Dry run - nothing will be written");
    } else if !view.yes {
        heading("Preview - re-run with --yes to apply");
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
    println!("xdelta3: {} ({})", view.xdelta3.path, view.xdelta3.version);
    println!("Backups: <binary>.overseer-bak beside each converted file");
    let label = view.target.label().to_owned();
    print_plan_lines(view.plans, view.deltas, |_| label.clone());
}

struct DlcPlanView<'a> {
    game_dir: &'a Utf8Path,
    xdelta3: &'a ResolvedXdelta3,
    plans: &'a [ItemPlan],
    deltas: &'a HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
    dry_run: bool,
    yes: bool,
}

fn print_dlc_plan(view: DlcPlanView<'_>) {
    if view.dry_run {
        heading("Dry run - nothing will be written");
    } else if !view.yes {
        heading("Preview - re-run with --yes to apply");
    } else if view.allow_incomplete_repair {
        heading(format!("Repairing selected DLC files in {}", view.game_dir));
    } else {
        heading(format!(
            "Applying the DLC consistency revision in {}",
            view.game_dir
        ));
    }
    println!("xdelta3: {} ({})", view.xdelta3.path, view.xdelta3.version);
    println!("Backups: <file>.overseer-bak beside each converted file");
    print_plan_lines(view.plans, view.deltas, |plan| {
        format!(
            "consistency ({})",
            dlc::dlc_note(plan.item.rel_path).unwrap_or("revision")
        )
    });
}

fn print_plan_lines(
    plans: &[ItemPlan],
    deltas: &HashMap<String, Utf8PathBuf>,
    label_for: impl Fn(&ItemPlan) -> String,
) {
    let selected = selected_groups(plans, deltas);
    for plan in plans {
        print_plan_line(
            plan,
            deltas.get(plan.item.rel_path),
            selected.contains(plan.item.group),
            &label_for(plan),
        );
    }
}

fn print_plan_line(
    plan: &ItemPlan,
    delta: Option<&Utf8PathBuf>,
    selected: bool,
    target_label: &str,
) {
    let source = plan
        .known_source
        .clone()
        .unwrap_or_else(|| "unknown source".to_owned());
    let delta_label = delta
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "no delta".to_owned());
    let gate = match plan.item.target.expected.verified_by() {
        VerifiedBy::Sha256 => String::new(),
        VerifiedBy::Crc32 => format!("; verified by {}", VerifiedBy::Crc32.label()),
    };
    let (role, msg) = match plan.state {
        ItemState::AlreadyTarget => (
            Role::Muted,
            format!("= {}: already {target_label}{gate}", plan.item.rel_path),
        ),
        ItemState::Missing => (
            Role::Muted,
            format!("- {}: missing{gate}", plan.item.rel_path),
        ),
        ItemState::NeedsConversion if !selected => (
            Role::Muted,
            format!(
                "~ {}: {source}, leaving as-is (no delta supplied)",
                plan.item.rel_path
            ),
        ),
        ItemState::NeedsConversion => (
            Role::Added,
            format!(
                "+ {}: {source} -> {target_label}; delta: {delta_label}{gate}",
                plan.item.rel_path
            ),
        ),
    };
    println!("{}", styled(role, msg));
}

fn resolve_xdelta3(cli_path: Option<&Utf8Path>) -> Result<ResolvedXdelta3> {
    let path = if let Some(path) = cli_path {
        resolve_executable(path)?
    } else if let Ok(env_path) = env::var("OVERSEER_XDELTA3") {
        resolve_executable(Utf8Path::new(&env_path))?
    } else {
        find_on_path("xdelta3")
            .context("xdelta3 not found; pass --xdelta3 or set OVERSEER_XDELTA3")?
    };
    let output = Command::new(path.as_std_path())
        .arg("-V")
        .output()
        .with_context(|| format!("running {path} -V"))?;
    if !output.status.success() {
        bail!("xdelta3 at {path} did not run successfully with -V");
    }
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    }
    Ok(ResolvedXdelta3 {
        path,
        version: first_line(&text),
    })
}

fn resolve_executable(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let has_dir = path.parent().is_some_and(|p| !p.as_str().is_empty());
    if path.is_absolute() || has_dir {
        return Ok(absolutize(path)?);
    }
    find_on_path(path.as_str()).with_context(|| format!("{path} not found on PATH"))
}

fn find_on_path(name: &str) -> Option<Utf8PathBuf> {
    let path_var = env::var_os("PATH")?;
    let extensions = path_extensions(name);
    for dir in env::split_paths(&path_var) {
        for ext in &extensions {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file()
                && let Ok(path) = candidate.canonicalize()
                && let Ok(path) = Utf8PathBuf::from_path_buf(path)
            {
                return Some(path);
            }
        }
    }
    None
}

fn path_extensions(name: &str) -> Vec<String> {
    if Utf8Path::new(name).extension().is_some() {
        return vec![String::new()];
    }
    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.BAT;.CMD".to_owned());
    let mut exts = vec![String::new()];
    exts.extend(pathext.split(';').map(str::to_ascii_lowercase));
    exts
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("unknown version").to_owned()
}

fn ba2(path: &Utf8Path, to: GenerationArg, dry_run: bool, yes: bool) -> Result<()> {
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

    let writing = !dry_run && (!is_dir || yes);
    if dry_run {
        heading("Dry run - nothing will be written");
    } else if is_dir && !yes {
        heading(format!("Preview of {path} - re-run with --yes to apply"));
    }

    let generation = to.into_core();
    let target = Ba2Edition::from_generation(generation)
        .with_context(|| format!("BA2 archives have no {} edition", generation.label()))?;
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
mod tests {
    use super::*;

    const COAST: &[&str] = &[
        "Data/DLCCoast.esm",
        "Data/DLCCoast.cdx",
        "Data/DLCCoast - Geometry.csg",
        "Data/DLCCoast - Main.ba2",
        "Data/DLCCoast - Textures.ba2",
    ];

    fn core_plan_for(rel: &str, state: ItemState) -> ItemPlan {
        ItemPlan {
            item: convert::explicit_item(Generation::OldGen, rel).expect("known core rel_path"),
            state,
            current: None,
            known_source: None,
        }
    }

    fn dlc_plan_for(rel: &str, state: ItemState) -> ItemPlan {
        ItemPlan {
            item: dlc::explicit_item(rel).expect("known dlc rel_path"),
            state,
            current: None,
            known_source: None,
        }
    }

    fn deltas(rels: &[&str]) -> HashMap<String, Utf8PathBuf> {
        rels.iter()
            .map(|r| ((*r).to_owned(), Utf8PathBuf::from("d.vcdiff")))
            .collect()
    }

    #[test]
    fn dlc_only_deltas_convert_dlc_and_leave_core_untouched() {
        let mut plans = vec![core_plan_for("Fallout4.exe", ItemState::NeedsConversion)];
        plans.extend(
            COAST
                .iter()
                .map(|r| dlc_plan_for(r, ItemState::NeedsConversion)),
        );
        let (jobs, noop) = build_jobs(
            "the DLC consistency revision",
            &plans,
            &deltas(COAST),
            false,
        )
        .unwrap();
        assert!(!noop);
        assert_eq!(jobs.len(), COAST.len());
        assert!(jobs.iter().all(|j| j.item.group == "DLCCoast"));
    }

    #[test]
    fn a_partial_dlc_group_is_refused() {
        let plans: Vec<_> = COAST
            .iter()
            .map(|r| dlc_plan_for(r, ItemState::NeedsConversion))
            .collect();
        let err = build_jobs(
            "the DLC consistency revision",
            &plans,
            &deltas(&["Data/DLCCoast.esm"]),
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("refusing partial group"));
    }

    #[test]
    fn no_deltas_means_nothing_to_convert() {
        let plans = vec![core_plan_for("Fallout4.exe", ItemState::NeedsConversion)];
        assert!(build_jobs("Old-Gen", &plans, &HashMap::new(), false).is_err());
    }

    #[test]
    fn a_fully_converted_selected_group_is_a_noop() {
        let plans: Vec<_> = COAST
            .iter()
            .map(|r| dlc_plan_for(r, ItemState::AlreadyTarget))
            .collect();
        let (jobs, noop) = build_jobs(
            "the DLC consistency revision",
            &plans,
            &deltas(COAST),
            false,
        )
        .unwrap();
        assert!(jobs.is_empty());
        assert!(noop);
    }

    #[test]
    fn a_bare_tool_name_is_searched_on_path_not_absolutized() {
        // A bare name (empty parent) must go through PATH lookup, not be absolutized against the CWD
        let result = resolve_executable(Utf8Path::new("overseer-nonexistent-tool-xyz"));
        assert!(
            result.is_err(),
            "a bare name not on PATH must error, not silently absolutize to the CWD"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not found on PATH"),
            "the error must come from the PATH lookup branch"
        );
    }
}
