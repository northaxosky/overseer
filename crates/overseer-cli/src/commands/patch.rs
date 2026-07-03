//! `overseer patch ...`: rewrite archive version headers in place

use crate::cli::{GenerationArg, PatchCommand};
use crate::ui::{Role, heading, styled, success};
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Header, Ba2Kind};
use overseer_core::detect::Generation;
use overseer_core::patch::delta::Xdelta3CliDecoder;
use overseer_core::patch::fallout4::convert::{self, ItemState, Outcome};
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome};

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
            game_dir,
            exe_delta,
            launcher_delta,
            steamapi_delta,
            xdelta3,
            dry_run,
            yes,
        } => {
            let target = to.into_core();
            if target != Generation::OldGen {
                bail!("`patch convert` only supports `--to og` (downgrade) for now");
            }
            convert_install(
                &game_dir,
                &exe_delta,
                &launcher_delta,
                &steamapi_delta,
                xdelta3.as_deref(),
                dry_run,
                yes,
            )
        }
    }
}

/// Downgrade a Fallout 4 install to Old-Gen by applying user-supplied xdelta3 deltas
fn convert_install(
    game_dir: &Utf8Path,
    exe_delta: &Utf8Path,
    launcher_delta: &Utf8Path,
    steamapi_delta: &Utf8Path,
    xdelta3: Option<&Utf8Path>,
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    if !game_dir.is_dir() {
        bail!("no such game directory: {game_dir}");
    }
    for (label, delta) in [
        ("Fallout4.exe", exe_delta),
        ("Fallout4Launcher.exe", launcher_delta),
        ("steam_api64.dll", steamapi_delta),
    ] {
        if !delta.is_file() {
            bail!("delta for {label} not found: {delta}");
        }
    }

    let writing = yes && !dry_run;
    if dry_run {
        heading("Dry run — nothing will be written");
    } else if !yes {
        heading("Preview — re-run with --yes to apply");
    } else {
        heading(format!("Converting {game_dir} to Old-Gen"));
    }

    if !writing {
        for (item, state) in convert::plan(game_dir, Generation::OldGen)? {
            print_plan(item.rel_path, state);
        }
        return Ok(());
    }

    let exe = xdelta3
        .map(Utf8Path::to_owned)
        .unwrap_or_else(|| Utf8PathBuf::from("xdelta3"));
    let decoder = Xdelta3CliDecoder::new(exe);
    let outcomes = convert::convert_to_old_gen(
        game_dir,
        exe_delta,
        launcher_delta,
        steamapi_delta,
        &decoder,
    )?;

    let mut converted = 0;
    for (name, outcome) in &outcomes {
        match outcome {
            Outcome::Converted => {
                converted += 1;
                println!(
                    "{}",
                    styled(Role::Added, format!("+ {name}: converted to Old-Gen"))
                );
            }
            Outcome::AlreadyTarget => {
                println!(
                    "{}",
                    styled(Role::Muted, format!("= {name}: already Old-Gen"))
                );
            }
            Outcome::Missing => {
                println!("{}", styled(Role::Muted, format!("- {name}: missing")));
            }
        }
    }
    success(format!("{converted} file(s) converted"));
    Ok(())
}

/// Print one dry-run/preview line for an item's classified state
fn print_plan(name: &str, state: ItemState) {
    let (role, msg) = match state {
        ItemState::NeedsConversion => (Role::Added, format!("+ {name}: will convert to Old-Gen")),
        ItemState::AlreadyTarget => (Role::Muted, format!("= {name}: already Old-Gen")),
        ItemState::Missing => (Role::Muted, format!("- {name}: missing")),
    };
    println!("{}", styled(role, msg));
}

/// Patch a single `.ba2`, or every top-level `.ba2` in a directory, toward `to`
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

    // Directory preview
    let writing = !dry_run && (!is_dir || yes);
    if dry_run {
        heading("Dry run — nothing will be written");
    } else if is_dir && !yes {
        heading(format!("Preview of {path} — re-run with --yes to apply"));
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
                    styled(Role::Muted, format!("- {name}: skipped — {reason}"))
                );
            }
            Line::Failed(msg) => {
                tally.errors += 1;
                println!("{}", styled(Role::Failure, format!("✗ {name}: {msg}")));
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

/// One file's classification, ready to print and tally
enum Line {
    Patched { from: u32, to: u32 },
    Already { version: u32 },
    Skipped(String),
    Failed(String),
}

/// Inspect, and when `writing`, patch one archive
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

/// Top-level `.ba2` files in `dir` sorted; Non-recursive
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
