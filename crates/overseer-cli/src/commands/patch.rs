//! `overseer patch ...`: rewrite archive version headers in place

use crate::cli::{PatchCommand, PatchTo};
use crate::ui::{Role, heading, styled, success};
use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::archive::{Ba2Header, Ba2Kind};
use overseer_core::patch::fallout4::{self, Ba2Edition, PatchOutcome};

pub fn run(command: PatchCommand) -> Result<()> {
    match command {
        PatchCommand::Ba2 {
            path,
            to,
            dry_run,
            yes,
        } => ba2(&path, to, dry_run, yes),
    }
}

/// Patch a single `.ba2`, or every top-level `.ba2` in a directory, toward `to`
fn ba2(path: &Utf8Path, to: PatchTo, dry_run: bool, yes: bool) -> Result<()> {
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

    let target = target_edition(to);
    let label = target_label(to);
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

fn target_edition(to: PatchTo) -> Ba2Edition {
    match to {
        PatchTo::Og => Ba2Edition::OldGen,
        PatchTo::Ng => Ba2Edition::NextGen,
    }
}

fn target_label(to: PatchTo) -> &'static str {
    match to {
        PatchTo::Og => "og",
        PatchTo::Ng => "ng",
    }
}
