//! `overseer merge`: combine a plugin list's BA2 archives into one managed mod, reversibly

use crate::cli::MergeArgs;
use crate::ui::{Gate, Role, preview_heading, styled, success};
use anyhow::{Context, Result, bail};
use camino::Utf8Path;
use overseer_core::merge::DEFAULT_TEXTURE_GROUP_BYTES;
use overseer_core::merge::transaction::{self, MergeReport, MergeRequest, ResolvedPlan};
use overseer_core::patch::fallout4::cc;
use overseer_core::plugins::PluginLoadOrder;

pub fn run(args: MergeArgs) -> Result<()> {
    if let Some(name) = &args.source.restore {
        let instance = args.target.load_instance()?;
        transaction::restore(&instance, name)?;
        success(format!("Restored merge `{name}`"));
        return Ok(());
    }

    let (instance, profile) = args.target.load_context()?;
    let (plugins, name) = if args.source.cc {
        let order = PluginLoadOrder::load(&instance, &profile.name)?;
        let name = args.name.clone().unwrap_or_else(|| "CCMerged".to_owned());
        (cc::cc_plugins(&order), name)
    } else {
        let list = args
            .source
            .list
            .as_deref()
            .expect("clap group guarantees --cc, --list, or --restore");
        let Some(name) = args.name.clone() else {
            bail!("--name is required with --list");
        };
        (read_plugin_list(list)?, name)
    };

    let texture_group_bytes = match args.texture_cap {
        Some(gib) => gib
            .checked_mul(1024 * 1024 * 1024)
            .context("--texture-cap is too large")?,
        None => DEFAULT_TEXTURE_GROUP_BYTES,
    };

    let gate = args.gate.gate();
    if gate.is_preview() {
        let plan = transaction::resolve(&instance, &profile, &plugins)?;
        print_merge_plan(&plan, &name, gate);
        return Ok(());
    }

    let req = MergeRequest {
        name,
        plugins,
        texture_group_bytes,
    };

    let report = transaction::run(&instance, &profile, &req)?;
    print_merge_report(&report);
    Ok(())
}

/// Read a plugin list file: one filename per line, trimmed, skipping blanks and `#` comments
fn read_plugin_list(file: &Utf8Path) -> Result<Vec<String>> {
    let text =
        std::fs::read_to_string(file).with_context(|| format!("reading plugin list {file}"))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn print_merge_plan(plan: &ResolvedPlan, name: &str, gate: Gate) {
    preview_heading(gate);
    let mut sources: usize = 0;
    for item in &plan.items {
        let mut kinds = Vec::new();
        if item.main.is_some() {
            kinds.push("Main");
            sources += 1;
        }
        if item.textures.is_some() {
            kinds.push("Textures");
            sources += 1;
        }
        println!(
            "{}",
            styled(
                Role::Added,
                format!("+ {}: {}", item.plugin, kinds.join(" + "))
            )
        );
    }
    for plugin in &plan.inactive {
        println!("{}", styled(Role::Muted, format!("- {plugin}: inactive")));
    }
    for plugin in &plan.orphaned {
        println!(
            "{}",
            styled(Role::Muted, format!("- {plugin}: no archives"))
        );
    }
    for (plugin, owner) in &plan.already_merged {
        println!(
            "{}",
            styled(
                Role::Muted,
                format!("= {plugin}: already merged into {owner}")
            )
        );
    }
    for plugin in &plan.missing {
        println!(
            "{}",
            styled(Role::Warning, format!("~ {plugin}: not in load order"))
        );
    }
    println!(
        "{} plugin(s), {sources} source archive(s) will be merged into `{name}`",
        plan.items.len()
    );
}

fn print_merge_report(report: &MergeReport) {
    success(format!("Merged into `{}`", report.name));
    println!("mod: {}", report.mod_dir);
    println!(
        "archives produced: {} general, {} texture",
        report.archives.gnrl, report.archives.dx10
    );
    println!("carriers: {}", report.carriers.len());
    for conflict in &report.conflicts {
        println!(
            "{}",
            styled(
                Role::Warning,
                format!(
                    "~ {}: {} over {}",
                    conflict.path, conflict.winner, conflict.loser
                )
            )
        );
    }
    let produced = report.archives.gnrl + report.archives.dx10;
    println!(
        "archives: {} removed -> {produced} produced",
        report.sources_removed
    );
    println!(
        "{}",
        styled(Role::Muted, "run `overseer deploy` to apply the merge")
    );
}
