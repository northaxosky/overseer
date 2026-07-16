//! `overseer instance ...` subcommands: init & show

use crate::cli::InstanceCommand;
use crate::context::absolutize;
use crate::ui::{heading, success};
use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use overseer_core::deploy::DeployerKind;
use overseer_core::game::GameKind;
use overseer_core::instance::{Instance, InstanceConfig};
use overseer_core::launch;

pub fn run(command: InstanceCommand) -> Result<()> {
    match command {
        InstanceCommand::Init {
            path,
            game,
            game_dir,
            local,
            ini_dir,
            profile,
        } => init(path, game, game_dir, local, ini_dir, profile),
        InstanceCommand::Show { path } => show(path),
    }
}

fn init(
    path: Utf8PathBuf,
    game: GameKind,
    game_dir: Utf8PathBuf,
    local: Option<Utf8PathBuf>,
    ini_dir: Option<Utf8PathBuf>,
    profile: String,
) -> Result<()> {
    let path = absolutize(&path)?;
    let game_dir = absolutize(&game_dir)?;
    let config = InstanceConfig {
        game_dir,
        game,
        local_dir: local.map(|l| absolutize(&l)).transpose()?,
        ini_dir: ini_dir.map(|d| absolutize(&d)).transpose()?,
        default_profile: profile,
        deployer: DeployerKind::default(),
        tools: Vec::new(),
    };

    let instance = Instance::init(&path, config).with_context(|| format!("initializing {path}"))?;
    instance
        .create_profile(&instance.config.default_profile)
        .context("creating the default profile")?;
    success(format!("Created instance at {}", instance.root));
    println!("  game:     {}", instance.config.game);
    println!("  game dir: {}", instance.config.game_dir);
    println!("  profile:  {}", instance.config.default_profile);
    let resolved = launch::tools(&instance);
    let targets: Vec<&str> = resolved.iter().map(|tool| tool.key.as_str()).collect();
    println!("  targets:  {}", targets.join(", "));
    Ok(())
}

fn show(path: Utf8PathBuf) -> Result<()> {
    let path = absolutize(&path)?;
    let instance = Instance::load(&path).with_context(|| format!("loading instance at {path}"))?;
    let local = match instance.local_dir() {
        Ok(dir) if instance.config.local_dir.is_some() => dir.to_string(),
        Ok(dir) => format!("(auto: {dir})"),
        Err(_) => "(auto: unresolved; set local_dir in overseer.toml)".to_owned(),
    };
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
