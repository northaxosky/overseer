use std::fmt::Display;
use std::fs;

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use overseer_core::deploy::{
    DeployManifest, DeployPlan, Deployer, HardlinkDeployer, ModSource, ProgressEvent, ProgressSink,
};
use overseer_core::instance::{Instance, Profile};
use overseer_core::plugins::{PluginLoadOrder, discover_plugins};
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

    /// Install a mod from an archive into an instance's mods/ directory
    Install {
        /// Path to the mod archive
        archive: Utf8PathBuf,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
        /// Name for the installed mod (defaults to archive's file name)
        #[arg(long)]
        name: Option<String>,
    },

    /// List the profile's mods in priority order (highest first)
    List {
        #[arg(long)]
        instance: Utf8PathBuf,
        #[arg(long, default_value = "Default")]
        profile: String,
    },

    /// Enable a mod in the profile
    Enable {
        /// Mod name (the folder name under mods/)
        name: String,
        #[arg(long)]
        instance: Utf8PathBuf,
        #[arg(long, default_value = "Default")]
        profile: String,
    },

    /// Disable a mod in the profile
    Disable {
        name: String,
        #[arg(long)]
        instance: Utf8PathBuf,
        #[arg(long, default_value = "Default")]
        profile: String,
    },

    /// List the profile's plugin load order (top loads first)
    Plugins {
        #[arg(long)]
        instance: Utf8PathBuf,
        #[arg(long, default_value = "Default")]
        profile: String,
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
        Command::Install {
            archive,
            instance,
            name,
        } => install(archive, instance, name),
        Command::List { instance, profile } => list(instance, profile),
        Command::Enable {
            name,
            instance,
            profile,
        } => set_status(instance, profile, &name, true),
        Command::Disable {
            name,
            instance,
            profile,
        } => set_status(instance, profile, &name, false),
        Command::Plugins { instance, profile } => plugins(instance, profile),
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

fn install(archive: Utf8PathBuf, instance_dir: Utf8PathBuf, name: Option<String>) -> Result<()> {
    let archive = absolutize(&archive)?;
    let instance_dir = absolutize(&instance_dir)?;

    // TODO: game_dir is unused by install; a placeholder until an instance config exists.
    let instance = Instance::new(&instance_dir, instance_dir.join("game"));

    let name = match name {
        Some(name) => name,
        None => archive
            .file_stem()
            .ok_or_else(|| anyhow!("Could not derive a mod name from `{archive}`; pass --name"))?
            .to_owned(),
    };

    heading(format!("Installing {archive} as `{name}`"));
    let installed = overseer_core::install::install(&instance, &archive, &name)
        .with_context(|| format!("Installing {archive}"))?;
    success(format!(
        "Installed `{}` to {}",
        installed.name,
        instance.mods_dir().join(&installed.name)
    ));
    Ok(())
}

fn plugins(instance_dir: Utf8PathBuf, profile_name: String) -> Result<()> {
    let instance = Instance::new(&instance_dir, instance_dir.join("game"));

    // Reconcile the mod list first, so plugin discovery sees new mods
    let profile = load_reconciled(&instance, &profile_name)?;
    let discovered = discover_plugins(&instance, &profile).context("Discovering plugins")?;
    let mut load_order = PluginLoadOrder::load(&instance, &profile_name)
        .with_context(|| format!("Loading plugins.txt for `{profile_name}`"))?;
    if load_order.reconcile(&discovered) {
        load_order.save(&instance).context("Saving plugins.txt")?;
    }

    if load_order.plugins.is_empty() {
        println!("No plugins. (Install mods with plugins and enable them)");
        return Ok(());
    }

    heading(format!(
        "{} - {} plugins (load order; masters first)",
        profile.name,
        load_order.plugins.len()
    ));

    // Build a quick lookup of which discovered plugins are masters, for the tag
    let active_style = Style::new().green();
    let inactive_style = Style::new().dimmed();

    for (i, entry) in load_order.plugins.iter().enumerate() {
        let is_master = discovered
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&entry.name) && m.is_master);
        let mark = if entry.active { "[x]" } else { "[ ]" };
        let tag = if is_master { " (master)" } else { "" };
        let style = if entry.active {
            active_style
        } else {
            inactive_style
        };
        let line = format!("{:>3}. {mark} {}{tag}", i + 1, entry.name);
        println!("{}", line.if_supports_color(Stdout, |t| t.style(style)));
    }
    Ok(())
}

/// Load a profile and reconcile it against whats installed
fn load_reconciled(instance: &Instance, profile: &str) -> Result<Profile> {
    let mut p =
        Profile::load(instance, profile).with_context(|| format!("Loading profile `{profile}`"))?;
    if p.reconcile(instance)
        .context("Reconciling with installed mods")?
    {
        p.save(instance).context("Saving reconciled profile")?;
    }
    Ok(p)
}

fn list(instance_dir: Utf8PathBuf, profile: String) -> Result<()> {
    let instance = Instance::new(&instance_dir, instance_dir.join("game"));
    let profile = load_reconciled(&instance, &profile)?;

    if profile.mods.is_empty() {
        println!("No mods installed.");
        return Ok(());
    }

    heading(format!(
        "{} - {} mods (highest priority first)",
        profile.name,
        profile.mods.len()
    ));

    let enabled_style = Style::new().green();
    let disabled_style = Style::new().dimmed();

    for (i, entry) in profile.mods.iter().enumerate() {
        let mark = if entry.enabled { "[x]" } else { "[ ]" };
        let style = if entry.enabled {
            enabled_style
        } else {
            disabled_style
        };
        let line = format!("{:>3}. {mark} {}", i + 1, entry.name);
        println!("{}", line.if_supports_color(Stdout, |t| t.style(style)));
    }
    Ok(())
}

fn set_status(
    instance_dir: Utf8PathBuf,
    profile_name: String,
    mod_name: &str,
    enabled: bool,
) -> Result<()> {
    let instance = Instance::new(&instance_dir, instance_dir.join("game"));
    let mut profile = load_reconciled(&instance, &profile_name)?;

    if enabled {
        profile.enable(mod_name)
    } else {
        profile.disable(mod_name)
    }
    .with_context(|| {
        format!(
            "{} `{mod_name}`",
            if enabled { "enabling" } else { "disabling" }
        )
    })?;

    profile.save(&instance).context("Saving profile")?;
    success(format!(
        "{} `{mod_name}` in profile `{}`",
        if enabled { "Enabled" } else { "Disabled" },
        profile.name
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
