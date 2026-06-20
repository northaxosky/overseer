//! The self-contained `demo` proof of the deployment engine.

use std::fs;

use anyhow::{Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use overseer_core::deploy::{
    DeployPlan, DeployRecord, Deployer, DeployerKind, HardlinkDeployer, ModSource,
};

use crate::ui::{CliProgress, check, heading, success};

pub fn run() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let base = Utf8PathBuf::from_path_buf(tmp.path().to_path_buf())
        .map_err(|_| anyhow!("temp dir path is not valid UTF-8"))?;

    let mod_a = base.join("mods/AlphaTextures");
    let mod_b = base.join("mods/BetterTextures");
    let data = base.join("game/Data");

    write_file(&mod_a.join("Textures/shared.dds"), "A-shared")?;
    write_file(&mod_a.join("Textures/only_a.dds"), "A-only")?;
    write_file(&mod_b.join("Textures/shared.dds"), "B-shared")?;

    heading("Overseer — hardlink deployment proof");
    println!("\nStaging (priority order, last wins):");
    println!("  [1] AlphaTextures  -> Textures/shared.dds, Textures/only_a.dds");
    println!("  [2] BetterTextures -> Textures/shared.dds\n");

    let mods = [
        ModSource::new("AlphaTextures", mod_a.clone()),
        ModSource::new("BetterTextures", mod_b.clone()),
    ];
    let plan = DeployPlan::from_mods(&data, &mods)?;
    let deployer = HardlinkDeployer::new();
    deployer.check_supported(&plan)?;
    let record =
        DeployRecord::from_plan(&plan, base.join(".overseer-backup"), DeployerKind::HardLink)?;

    heading(format!("Deploying to {data}"));
    deployer.deploy(&record, &CliProgress)?;
    println!();

    let shared = data.join("Textures/shared.dds");
    let winner_ok = fs::read_to_string(&shared)? == "B-shared";

    // Hard-link proof: editing the staged source must show through the deployed file.
    fs::write(mod_b.join("Textures/shared.dds"), "B-edited")?;
    let link_ok = fs::read_to_string(&shared)? == "B-edited";

    let verify_ok = deployer.verify(&record).is_ok();

    let reversed = deployer.undeploy(&record, &CliProgress).is_fully_resolved();
    let purge_ok = reversed && !shared.exists() && !data.join("Textures").exists();
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

fn write_file(path: &Utf8Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}
