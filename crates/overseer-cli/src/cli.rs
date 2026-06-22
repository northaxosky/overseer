//! Command-line interface definition (clap derive).

use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use overseer_core::game::GameKind;

#[derive(Parser)]
#[command(
    name = "overseer",
    version,
    about = "Overseer: a Fallout 4 mod manager written in Rust"
)]
pub struct Cli {
    /// When to use colour in output
    #[arg(long, default_value = "auto", global = true)]
    pub color: crate::ui::ColorChoice,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Run a self-contained proof of the hardlink deployment engine in a temp directory
    Demo,

    /// Deploy a profile's enabled mods into the instance's game `Data/` directory
    Deploy {
        #[command(flatten)]
        target: ProfileArgs,
    },

    /// Removes the instance's live deployment, restoring the game directory
    Purge {
        #[arg(long)]
        instance: Utf8PathBuf,
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

    /// Show the instance's deployment status
    Status {
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },

    /// Launch the game, its script extender, or a configured tool
    Launch {
        /// Target name (omit to list the available targets)
        name: Option<String>,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },

    /// Manage an instance's launch targets (executables)
    Exe {
        #[command(subcommand)]
        command: ExeCommand,
    },

    /// Run setup health checks and report any problems
    Doctor {
        #[command(flatten)]
        target: ProfileArgs,
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
    Activate {
        /// Plugin file name (e.g. MyMod.esp)
        name: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Deactivate a plugin
    Deactivate {
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
        /// Game to manage [possible values: fallout4, skyrimse, starfield]
        #[arg(long, default_value = "fallout4")]
        game: GameKind,
        /// Game install directory (contains the game exe & Data/)
        #[arg(long)]
        game_dir: Utf8PathBuf,
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

#[derive(Subcommand)]
pub enum ExeCommand {
    /// List the instance's configured launch targets
    List {
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },
    /// Add a launch target (an external tool, e.g. FO4Edit)
    Add {
        /// Display name and lookup key (e.g. FO4Edit)
        #[arg(long)]
        name: String,
        /// Path to the executable
        #[arg(long)]
        path: Utf8PathBuf,
        /// An argument to pass when launching (repeat for multiple)
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },
    /// Remove a launch target by name
    Remove {
        /// The target's name
        name: String,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },
}
