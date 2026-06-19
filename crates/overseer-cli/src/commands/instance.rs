//! `overseer instance ...` subcommands: init & show

use crate::cli::InstanceCommand;
use crate::context::absolutize;
use crate::ui::{heading, success};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::deploy::DeployerKind;
use overseer_core::instance::{Instance, InstanceConfig};

pub fn run(command: InstanceCommand) -> Result<()> {
    match command {
        InstanceCommand::Init {
            path,
            game,
            local,
            profile,
        } => init(path, game, local, profile),
        InstanceCommand::Show { path } => show(path),
    }
}

fn init(
    path: Utf8PathBuf,
    game: Utf8PathBuf,
    local: Option<Utf8PathBuf>,
    profile: String,
) -> Result<()> {
    let path = absolutize(&path)?;
    let config = InstanceConfig {
        game_dir: absolutize(&game)?,
        local_dir: local.map(|l| absolutize(&l)).transpose()?,
        default_profile: profile,
        deployer: DeployerKind::default(),
    };

    let instance = Instance::init(&path, config).with_context(|| format!("Initializing {path}"))?;
    success(format!("Created instance at {}", instance.root));
    println!("  game:    {}", instance.game_dir());
    println!("  profile: {}", instance.config.default_profile);
    Ok(())
}

fn show(path: Utf8PathBuf) -> Result<()> {
    let path = absolutize(&path)?;
    let instance = Instance::load(&path).with_context(|| format!("Loading instance at {path}"))?;

    heading(format!("Instance at {}", instance.root));
    println!("  game dir:        {}", instance.game_dir());
    println!(
        "  local dir:       {}",
        instance
            .config
            .local_dir
            .as_deref()
            .map_or("(auto: %LOCALAPPDATA%\\Fallout4)", |p| p.as_str())
    );
    println!("  default profile: {}", instance.config.default_profile);
    println!("  deployer:        {}", instance.config.deployer);

    let mods = instance.installed_mods().context("Listing mods")?;
    let profiles = instance.profiles().context("Listing profiles")?;
    println!("  installed mods:  {}", mods.len());
    println!("  profiles:        {}", profiles.join(", "));
    Ok(())
}
