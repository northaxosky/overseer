//! Command-line interface definition (clap derive).

use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "overseer",
    version,
    about = "Overseer: a Fallout 4 mod manager written in Rust"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run a self-contained proof of the hardlink deployment engine in a temp directory
    Demo,

    /// Deploy mod staging dirs into a target directory (low-level; lowest priority first)
    Deploy {
        /// Target directory (e.g. the game's Data folder)
        #[arg(long)]
        target: Utf8PathBuf,
        /// A mod staging directory; repeat in priority order (the last one wins conflicts)
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

    /// Install a mod from an archive (.7z or .zip) into an instance's mods/ directory
    Install {
        /// Path to the mod archive
        archive: Utf8PathBuf,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
        /// Name for the installed mod (defaults to the archive's file name)
        #[arg(long)]
        name: Option<String>,
    },

    /// Manage the mods in a profile
    Mod {
        #[command(subcommand)]
        command: ModCommand,
    },

    /// Manage the plugin load order in a profile
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },

    /// Create or inspect an Overseer instance
    Instance {
        #[command(subcommand)]
        command: InstanceCommand,
    },
}

/// Arguments shared by every profile-scoped subcommand.
#[derive(Args)]
pub struct ProfileArgs {
    /// Instance directory (contains mods/ and profiles/)
    #[arg(long)]
    pub instance: Utf8PathBuf,
    /// Profile name
    #[arg(long, default_value = "Default")]
    pub profile: String,
}

#[derive(Subcommand)]
pub enum ModCommand {
    /// List the profile's mods (highest priority first)
    List {
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Enable a mod
    Enable {
        /// Mod name (the folder name under mods/)
        name: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Disable a mod
    Disable {
        /// Mod name (the folder name under mods/)
        name: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Move a mod to a new priority position (1 = highest priority)
    Move {
        /// Mod name (the folder name under mods/)
        name: String,
        /// New 1-based position (1 = highest priority)
        #[arg(long)]
        to: usize,
        #[command(flatten)]
        target: ProfileArgs,
    },
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List the plugin load order (top loads first; masters first)
    List {
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Activate a plugin
    Enable {
        /// Plugin file name (e.g. MyMod.esp)
        name: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Deactivate a plugin
    Disable {
        /// Plugin file name (e.g. MyMod.esp)
        name: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
}

#[derive(Subcommand)]
pub enum InstanceCommand {
    /// Create a new instance (writes overseer.toml, mods/ and profiles/)
    Init {
        /// Directory to create the instance in
        #[arg(long)]
        path: Utf8PathBuf,
        /// Game install directory (contains Fallout4.exe and Data/)
        #[arg(long)]
        game: Utf8PathBuf,
        /// Where the real Plugins.txt lives (default: %LOCALAPPDATA%\Fallout4)
        #[arg(long)]
        local: Option<Utf8PathBuf>,
        /// Name of the default profile
        #[arg(long, default_value = "Default")]
        profile: String,
    },
    /// Show an instance's configuration and status
    Show {
        /// The instance directory
        #[arg(long)]
        path: Utf8PathBuf,
    },
}
