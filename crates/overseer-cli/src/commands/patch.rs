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
use overseer_core::patch::fallout4::convert::{self, ItemState, Outcome};
use overseer_core::patch::fallout4::vcdiff;
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome};
use overseer_core::patch::fingerprint::VerifiedBy;
use std::collections::HashMap;
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
    let delta_map = resolve_deltas(args.deltas, args)?;
    if args.yes && !args.dry_run {
        convert::recover_install(&game_dir, args.target)?;
    }
    let item_plans = convert::plan(&game_dir, args.target)?;
    validate_edition_for_auto_convert(edition, &item_plans)?;
    let (jobs, complete_noop) = build_jobs(
        args.target,
        &item_plans,
        &delta_map,
        args.allow_incomplete_repair,
    )?;
    let view = ConvertPlanView {
        game_dir: &game_dir,
        edition,
        target: args.target,
        xdelta3: &xdelta3,
        plans: &item_plans,
        deltas: &delta_map,
        allow_incomplete_repair: args.allow_incomplete_repair,
        dry_run: args.dry_run,
        yes: args.yes,
    };
    print_convert_plan(view);
    if complete_noop {
        bail!("all core binaries already match {}", args.target.label());
    }
    if args.dry_run || !args.yes {
        return Ok(());
    }
    let decoder = Xdelta3CliDecoder::new(xdelta3.path);
    let outcomes = convert::convert(&game_dir, &jobs, &decoder)?;
    let converted = outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, Outcome::Converted))
        .count();
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
    if args.allow_incomplete_repair {
        success(format!("{converted} file(s) repaired"));
    } else {
        success(format!(
            "{converted} file(s) converted to {}",
            args.target.label()
        ));
    }
    Ok(())
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

fn resolve_deltas(
    deltas: Option<&Utf8Path>,
    args: ConvertArgs<'_>,
) -> Result<HashMap<String, Utf8PathBuf>> {
    let mut mapped = if let Some(dir) = deltas {
        if !dir.is_dir() {
            bail!("no such delta directory: {dir}");
        }
        vcdiff::map_deltas(dir).with_context(|| format!("mapping deltas in {dir}"))?
    } else {
        HashMap::new()
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
            mapped.insert(name.to_owned(), path.to_owned());
        }
    }
    Ok(mapped)
}

fn build_jobs(
    target: Generation,
    plans: &[convert::ItemPlan],
    deltas: &HashMap<String, Utf8PathBuf>,
    allow_incomplete_repair: bool,
) -> Result<(Vec<convert::ConvertJob>, bool)> {
    let mut jobs = Vec::new();
    let mut already = 0usize;
    for plan in plans {
        match plan.state {
            ItemState::AlreadyTarget => already += 1,
            ItemState::Missing if !allow_incomplete_repair => bail!(
                "{} is missing; refusing partial conversion",
                plan.item.rel_path
            ),
            ItemState::Missing => {}
            ItemState::NeedsConversion => match deltas.get(plan.item.rel_path) {
                Some(delta) => jobs.push(convert::ConvertJob {
                    item: plan.item,
                    delta: delta.clone(),
                }),
                None if allow_incomplete_repair => {}
                None => bail!(
                    "missing delta for {}; refusing partial conversion",
                    plan.item.rel_path
                ),
            },
        }
    }
    if jobs.is_empty() && already != plans.len() && !allow_incomplete_repair {
        bail!("no files can be converted to {}", target.label());
    }
    Ok((jobs, already == plans.len()))
}

fn validate_edition_for_auto_convert(edition: Edition, plans: &[convert::ItemPlan]) -> Result<()> {
    if matches!(
        edition,
        Edition::OldGen | Edition::NextGen | Edition::Anniversary
    ) {
        return Ok(());
    }
    if plans
        .iter()
        .filter(|plan| !matches!(plan.state, ItemState::Missing))
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
    plans: &'a [convert::ItemPlan],
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
            "Converting {} to {}",
            view.game_dir,
            view.target.label()
        ));
    }
    println!(
        "Detected edition: {:?} -> target: {}",
        view.edition,
        view.target.label()
    );
    println!("xdelta3: {} ({})", view.xdelta3.path, view.xdelta3.version);
    println!("Backups: <binary>.overseer-bak beside each converted file");
    for plan in view.plans {
        print_plan_line(plan, view.deltas.get(plan.item.rel_path));
    }
}

fn print_plan_line(plan: &convert::ItemPlan, delta: Option<&Utf8PathBuf>) {
    let source = plan
        .known_source
        .map(|fp| fp.label())
        .unwrap_or_else(|| "unknown source".to_owned());
    let delta_label = delta
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "no delta".to_owned());
    let gate = match plan.item.target.verified_by() {
        VerifiedBy::Sha256 => String::new(),
        VerifiedBy::Crc32 => format!("; verified by {}", VerifiedBy::Crc32.label()),
    };
    let (role, msg) = match plan.state {
        ItemState::AlreadyTarget => (
            Role::Muted,
            format!(
                "= {}: already {}{}",
                plan.item.rel_path,
                plan.item.target.label(),
                gate
            ),
        ),
        ItemState::Missing => (
            Role::Muted,
            format!("- {}: missing{}", plan.item.rel_path, gate),
        ),
        ItemState::NeedsConversion => (
            Role::Added,
            format!(
                "+ {}: {source} -> {}; delta: {delta_label}{}",
                plan.item.rel_path,
                plan.item.target.label(),
                gate
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
    if path.is_absolute() || path.parent().is_some() {
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
