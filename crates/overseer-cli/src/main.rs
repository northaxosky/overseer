use std::fs;

use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use overseer_core::deploy::{
    DeployManifest, DeployPlan, Deployer, HardlinkDeployer, ModSource, ProgressEvent, ProgressSink,
};

#[derive(Parser)]
#[command(
    name = "overseer",
    version,
    about = "Overseer — Fallout 4 mod manager (Phase 0 spike)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a self-contained proof of the hardlink deployment engine in a temp directory.
    Demo,
    /// Deploy mods (staging dirs, lowest priority first) into a target directory.
    Deploy {
        /// Target directory, e.g. the game's `Data` folder.
        #[arg(long)]
        target: Utf8PathBuf,
        /// A mod staging directory; repeat in priority order (the last one wins conflicts).
        #[arg(long = "mod", value_name = "DIR", required = true)]
        mods: Vec<Utf8PathBuf>,
        /// Where to write the deploy manifest (needed later to purge).
        #[arg(long, default_value = "overseer-manifest.json")]
        manifest: Utf8PathBuf,
    },
    /// Reverse a deployment using a manifest written by `deploy`.
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

/// Prints CLI-friendly progress lines.
struct CliProgress;

impl ProgressSink for CliProgress {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Started { total } => println!("  ({total} files)"),
            ProgressEvent::Deployed { relative, .. } => println!("  + {relative}"),
            ProgressEvent::Removed { relative, .. } => println!("  - {relative}"),
            ProgressEvent::Finished => {}
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

    let plan = DeployPlan::from_mods(&target, &sources).context("building deploy plan")?;
    println!("Deploying {} files to {target}", plan.len());

    let deployer = HardlinkDeployer::new();
    let manifest = deployer.deploy(&plan, &CliProgress).context("deploying")?;

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, json).with_context(|| format!("writing {manifest_path}"))?;
    println!("Done. Manifest written to {manifest_path}");
    Ok(())
}

fn purge(manifest_path: Utf8PathBuf) -> Result<()> {
    let json =
        fs::read_to_string(&manifest_path).with_context(|| format!("reading {manifest_path}"))?;
    let manifest: DeployManifest = serde_json::from_str(&json).context("parsing manifest")?;

    let deployer = HardlinkDeployer::new();
    deployer
        .undeploy(&manifest, &CliProgress)
        .context("purging")?;
    println!(
        "Purged {} files from {}",
        manifest.files.len(),
        manifest.target_root
    );
    Ok(())
}

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

    println!("Overseer — Phase 0 hardlink deployment proof\n");
    println!("Staging (priority order, last wins):");
    println!("  [1] AlphaTextures  -> Textures/shared.dds, Textures/only_a.dds");
    println!("  [2] BetterTextures -> Textures/shared.dds\n");

    let mods = [
        ModSource::new("AlphaTextures", mod_a.clone()),
        ModSource::new("BetterTextures", mod_b.clone()),
    ];
    let plan = DeployPlan::from_mods(&data, &mods)?;
    let deployer = HardlinkDeployer::new();

    println!("Deploying to {data}");
    let manifest = deployer.deploy(&plan, &CliProgress)?;

    let shared = data.join("Textures/shared.dds");
    let shared_contents = fs::read_to_string(&shared)?;
    let winner_ok = shared_contents == "B-shared";
    println!(
        "\nConflict resolution : shared.dds = {shared_contents:?} (want \"B-shared\")  [{}]",
        pass(winner_ok)
    );

    // Hard-link proof: edit the staged source, then read the deployed file.
    fs::write(mod_b.join("Textures/shared.dds"), "B-edited")?;
    let after = fs::read_to_string(&shared)?;
    let link_ok = after == "B-edited";
    println!(
        "Hard link (not copy): edited source, deployed reads {after:?}             [{}]",
        pass(link_ok)
    );

    let verify_ok = deployer.verify(&manifest).is_ok();
    println!(
        "Verify deployed     : all files present                              [{}]",
        pass(verify_ok)
    );

    deployer.undeploy(&manifest, &CliProgress)?;
    let purge_ok = !shared.exists() && !data.join("Textures").exists();
    println!(
        "Purge               : target clean, created dirs removed             [{}]",
        pass(purge_ok)
    );

    let staging_ok = mod_b.join("Textures/shared.dds").exists();
    println!(
        "Staging intact      : sources untouched by purge                     [{}]",
        pass(staging_ok)
    );

    let all = winner_ok && link_ok && verify_ok && purge_ok && staging_ok;
    println!(
        "\n{}",
        if all {
            "ALL CHECKS PASSED"
        } else {
            "SOME CHECKS FAILED"
        }
    );
    if !all {
        std::process::exit(1);
    }
    Ok(())
}

fn write_file(path: &Utf8Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
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

fn pass(b: bool) -> &'static str {
    if b {
        "PASS"
    } else {
        "FAIL"
    }
}
