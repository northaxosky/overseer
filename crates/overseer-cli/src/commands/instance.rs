//! `overseer instance ...` subcommands: init & show

use crate::cli::InstanceCommand;
use crate::context::absolutize;
use crate::ui::{heading, success};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::deploy::DeployerKind;
use overseer_core::game::GameKind;
use overseer_core::instance::{Instance, InstanceConfig};

pub fn run(command: InstanceCommand) -> Result<()> {
    match command {
        InstanceCommand::Init {
            path,
            game,
            game_dir,
            local,
            profile,
        } => init(path, game, game_dir, local, profile),
        InstanceCommand::Show { path } => show(path),
    }
}

fn init(
    path: Utf8PathBuf,
    game: GameKind,
    game_dir: Utf8PathBuf,
    local: Option<Utf8PathBuf>,
    profile: String,
) -> Result<()> {
    let path = absolutize(&path)?;
    let config = InstanceConfig {
        game_dir: absolutize(&game_dir)?,
        game,
        local_dir: local.map(|l| absolutize(&l)).transpose()?,
        default_profile: profile,
        deployer: DeployerKind::default(),
    };

    let instance = Instance::init(&path, config).with_context(|| format!("initializing {path}"))?;
    success(format!("Created instance at {}", instance.root));
    println!("  game:     {}", instance.config.game);
    println!("  game dir: {}", instance.config.game_dir);
    println!("  profile:  {}", instance.config.default_profile);
    Ok(())
}

fn show(path: Utf8PathBuf) -> Result<()> {
    let path = absolutize(&path)?;
    let instance = Instance::load(&path).with_context(|| format!("loading instance at {path}"))?;
    let local = instance.config.local_dir.as_deref().map_or_else(
        || {
            format!(
                "(auto: %LOCALAPPDATA%\\{})",
                instance.config.game.local_appdata_dir()
            )
        },
        |p| p.to_string(),
    );
    let mods = instance.installed_mods().context("listing mods")?;
    let profiles = instance.profiles().context("listing profiles")?;

    heading(format!("Instance at {}", instance.root));
    println!("  game:            {}", instance.config.game);
    println!("  game dir:        {}", instance.config.game_dir);
    println!("  local dir:       {local}");
    println!("  default profile: {}", instance.config.default_profile);
    println!("  deployer:        {}", instance.config.deployer);
    println!("  installed mods:  {}", mods.len());
    println!("  profiles:        {}", profiles.join(", "));
    Ok(())
}
