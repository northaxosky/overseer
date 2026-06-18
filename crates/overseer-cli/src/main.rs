use std::fmt::Display;
use std::fs;

use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use overseer_core::deploy::{
    DeployManifest, DeployPlan, Deployer, HardlinkDeployer, ModSource, ProgressEvent, ProgressSink,
};
use owo_colors::{OwoColorize, Stream::Stdout, Style};

/// A bold section heading
fn heading(msg: impl Display) {
    let style = Style::new().bold();
    println!("{}", msg.if_supports_color(Stdout, |t| t.style(style)));
}

/// A green success line
fn success(msg: impl Display) {
    let style = Style::new().green().bold();
    println!(
        "{} {msg}",
        "✓".if_supports_color(Stdout, |t| t.style(style))
    );
}

#[derive(Parser)]
#[command(
    name = "overseer",
    version,
    about = "Overseer: Fallout 4 Mod Manager Written In Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a self-contained proof of the hardlink deployment engine in a temp directory
    Demo,
    /// Deploy mods into a target directory
    Deploy {
        /// Target Directory (`Data` folder)
        #[arg(long)]
        target: Utf8PathBuf,
        /// Mod Staging Directory (last one wins conflicts)
        #[arg(long = "mod", value_name = "DIR", required = true)]
        mods: Vec<Utf8PathBuf>,
        /// Where to write the deploy manifest (needed to purge)
        #[arg(long, default_value = "overseer-manifest.json")]
        manifest: Utf8PathBuf,
    },

    /// Reverse a deployment using a manifest written by `deploy`
    Purge {
        #[arg(long)]
        manifest: Utf8PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Demo => demo(),
        Command::Deploy {
            target,
            mods,
            manifest,
        } => deploy(target, mods, manifest),
        Command::Purge { manifest } => purge(manifest),
    }
}

/// Prints CLI friendly progress lines
struct CliProgress;

impl ProgressSink for CliProgress {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Started { total } => {
                let style = Style::new().dimmed();
                println!(
                    "  {}",
                    format!("({total} files)").if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Deployed { relative, .. } => {
                let style = Style::new().green().bold();
                println!(
                    "  {} {relative}",
                    "+".if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Removed { relative, .. } => {
                let style = Style::new().yellow().bold();
                println!(
                    "  {} {relative}",
                    "-".if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Finished => {
                let style = Style::new().green().bold();
                println!(
                    "  {}",
                    "✓ done".if_supports_color(Stdout, |t| t.style(style))
                );
            }
        }
    }
}

fn deploy(target: Utf8PathBuf, mods: Vec<Utf8PathBuf>, manifest_path: Utf8PathBuf) -> Result<()> {
    let target = absolutize(&target)?;
    let sources = mods
        .iter()
        .map(|p| {
            let abs = absolutize(p)?;
            let name = abs.file_name().unwrap_or("mod").to_string();
            Ok(ModSource::new(name, abs))
        })
        .collect::<Result<Vec<_>>>()?;

    let plan = DeployPlan::from_mods(&target, &sources).context("Building deploy plan")?;
    heading(format!("Deploying {} files to {target}", plan.len()));

    let deployer = HardlinkDeployer::new();
    let manifest = deployer.deploy(&plan, &CliProgress).context("Deploying")?;

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, json).with_context(|| format!("Writing {manifest_path}"))?;
    success(format!("Manifest written to {manifest_path}"));
    Ok(())
}

fn purge(manifest_path: Utf8PathBuf) -> Result<()> {
    let json =
        fs::read_to_string(&manifest_path).with_context(|| format!("Reading {manifest_path}"))?;
    let manifest: DeployManifest = serde_json::from_str(&json).context("Parsing manifest")?;

    let deployer = HardlinkDeployer::new();
    deployer
        .undeploy(&manifest, &CliProgress)
        .context("Purging")?;

    success(format!(
        "Purged {} files from {}",
        manifest.files.len(),
        manifest.target_root
    ));
    Ok(())
}

fn absolutize(path: &Utf8Path) -> Result<Utf8PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    let cwd = std::env::current_dir()?;
    let cwd = Utf8PathBuf::from_path_buf(cwd).map_err(|_| anyhow!("cwd is not valid UTF-8"))?;
    Ok(cwd.join(path))
}

/// A self-contained proof of the deployment engine: stage two conflicting mods,
/// deploy them in priority order, prove the deployed files are hard links (not
/// copies), then purge back to a clean state — all in a throwaway temp directory.
fn demo() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf())
        .map_err(|_| anyhow!("temp dir path is not valid UTF-8"))?;

    let mod_a = base.join("mods/AlphaTextures");
    let mod_b = base.join("mods/BetterTextures");
    let data = base.join("game/Data");

    write_file(&mod_a.join("Textures/shared.dds"), "A-shared")?;
    write_file(&mod_a.join("Textures/only_a.dds"), "A-only")?;
    write_file(&mod_b.join("Textures/shared.dds"), "B-shared")?;

    heading("Overseer — Phase 0 hardlink deployment proof");
    println!("\nStaging (priority order, last wins):");
    println!("  [1] AlphaTextures  -> Textures/shared.dds, Textures/only_a.dds");
    println!("  [2] BetterTextures -> Textures/shared.dds\n");

    let mods = [
        ModSource::new("AlphaTextures", mod_a.clone()),
        ModSource::new("BetterTextures", mod_b.clone()),
    ];
    let plan = DeployPlan::from_mods(&data, &mods)?;
    let deployer = HardlinkDeployer::new();

    heading(format!("Deploying to {data}"));
    let manifest = deployer.deploy(&plan, &CliProgress)?;
    println!();

    let shared = data.join("Textures/shared.dds");
    let winner_ok = fs::read_to_string(&shared)? == "B-shared";

    // Hard-link proof: editing the staged source must show through the deployed file.
    fs::write(mod_b.join("Textures/shared.dds"), "B-edited")?;
    let link_ok = fs::read_to_string(&shared)? == "B-edited";

    let verify_ok = deployer.verify(&manifest).is_ok();

    deployer.undeploy(&manifest, &CliProgress)?;
    let purge_ok = !shared.exists() && !data.join("Textures").exists();
    let staging_ok = mod_b.join("Textures/shared.dds").exists();

    println!();
    let all = [
        check("Conflict resolution (higher priority wins)", winner_ok),
        check(
            "Hard link, not copy (edit source, deployed sees it)",
            link_ok,
        ),
        check("Verify deployed (all files present)", verify_ok),
        check("Purge (target clean, created dirs removed)", purge_ok),
        check("Staging intact (sources untouched by purge)", staging_ok),
    ]
    .into_iter()
    .all(|ok| ok);

    println!();
    if all {
        success("ALL CHECKS PASSED");
        Ok(())
    } else {
        Err(anyhow!("some checks failed"))
    }
}

/// Print a labeled check result with a green PASS or red FAIL.
fn check(label: &str, ok: bool) -> bool {
    let style = if ok {
        Style::new().green().bold()
    } else {
        Style::new().red().bold()
    };
    let mark = if ok { "PASS" } else { "FAIL" };
    println!(
        "  {label:<54} [{}]",
        mark.if_supports_color(Stdout, |t| t.style(style))
    );
    ok
}

fn write_file(path: &Utf8Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}
