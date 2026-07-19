//! `overseer launch ...`: run a launch target, or list the available ones.

use anyhow::{Context, Result, bail};
use overseer_core::deploy::NullSink;
use overseer_core::instance::Instance;
use overseer_core::launch::{self, PrepareOutcome};

use crate::cli::InstanceArgs;
use crate::ui::{print_launch_targets, success};

pub fn run(
    name: Option<String>,
    clear: bool,
    redeploy: bool,
    instance: &InstanceArgs,
) -> Result<()> {
    let instance = instance.load_instance()?;
    if clear {
        let message = if launch::clear_launch_marker(&instance)? {
            "Cleared stale launch marker"
        } else {
            "No launch marker was present"
        };
        success(message);
        return Ok(());
    }
    match name {
        Some(name) => launch(&instance, &name, redeploy)?,
        None => list(&instance),
    }
    Ok(())
}

fn launch(instance: &Instance, name: &str, redeploy: bool) -> Result<()> {
    let profile = instance.config.default_profile.clone();
    let first = launch::prepare_and_launch(instance, &profile, name, None, &NullSink)
        .with_context(|| format!("preparing `{name}` for launch"))?;
    let outcome = match first {
        PrepareOutcome::NeedsRedeploy { token, .. } if redeploy => {
            launch::prepare_and_launch(instance, &profile, name, Some(token), &NullSink)
                .with_context(|| format!("redeploying before launching `{name}`"))?
        }
        outcome => outcome,
    };

    match outcome {
        PrepareOutcome::Launched { handle, .. } => {
            handle.detach();
            success(format!("Launched `{name}`"));
            Ok(())
        }
        PrepareOutcome::NeedsRedeploy { reason, .. } => {
            bail!("{reason}; rerun with --redeploy")
        }
        PrepareOutcome::NeedsRecovery { reason } => bail!("{reason}"),
    }
}

fn list(instance: &Instance) {
    print_launch_targets(&launch::tools(instance));
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use overseer_core::apply;
    use overseer_core::instance::UserTool;
    use overseer_core::test_support::{install_mod, save_profile, temp_instance};

    #[test]
    fn redeploy_flag_is_the_single_shot_stale_consent_path() {
        let (_temp, mut instance) = temp_instance();
        install_mod(&instance, "CoolMod", &[("Textures/a.dds", "pixels")]);
        save_profile(&instance, "Default", &[("CoolMod", true)]);
        apply::deploy_profile(&instance, "Default", &NullSink).expect("deploy");
        let program = Utf8PathBuf::from_path_buf(std::env::current_exe().expect("current exe"))
            .expect("utf8 current exe");
        instance.config.tools = vec![UserTool::new(
            "Test Runner",
            program,
            vec!["--list".to_owned()],
        )];
        std::fs::write(
            instance.local_dir().expect("local dir").join("Plugins.txt"),
            b"external\n",
        )
        .expect("make deployment stale");

        let error = launch(&instance, "test-runner", false).expect_err("consent required");
        assert!(error.to_string().contains("--redeploy"));
        assert!(!launch::has_launch_marker(&instance).expect("marker state"));

        launch(&instance, "test-runner", true).expect("redeploy and launch");
        assert!(launch::has_launch_marker(&instance).expect("marker state"));
        launch::clear_launch_marker(&instance).expect("clear marker");
    }
}
