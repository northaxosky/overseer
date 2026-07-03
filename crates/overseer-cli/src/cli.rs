//! Command-line interface definition (clap derive).

use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use overseer_core::detect::Generation;
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
    /// Deploy a profile's enabled mods into the instance's game `Data/` directory
    Deploy {
        #[command(flatten)]
        target: ProfileArgs,
    },

    /// Remove the instance's live deployment, restoring the game directory
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

    /// List the installable archives in the instance's downloads/ directory
    Downloads {
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
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

    /// Manage profile-level settings
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },

    /// List or delete a profile's save files
    Saves {
        #[command(subcommand)]
        command: SaveCommand,
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

    /// List files that more than one enabled mod provides (winner + overridden)
    Conflicts {
        #[command(flatten)]
        target: ProfileArgs,
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

    /// Patch game archives in place
    Patch {
        #[command(subcommand)]
        command: PatchCommand,
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
    /// Rename an installed mod (updates every profile that uses it)
    Rename {
        /// Current mod name (the folder under mods/)
        name: String,
        /// New name for the mod
        new_name: String,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
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
pub enum ProfileCommand {
    /// Show or set whether this profile keeps its own saves (MO2 `LocalSaves`)
    Saves {
        /// `on` or `off`; omit to show the current setting
        state: Option<Toggle>,
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Create a new profile
    New {
        /// Name for the new profile
        name: String,
        /// Instance directory (contains mods/ and profiles/)
        #[arg(long)]
        instance: Utf8PathBuf,
    },
}

#[derive(Subcommand)]
pub enum SaveCommand {
    /// List the profile's saves, newest first
    List {
        #[command(flatten)]
        target: ProfileArgs,
    },
    /// Delete a save (and its script-extender co-save) by file name
    Delete {
        /// The save's file name, e.g. `Save7_...fos`
        file: String,
        #[command(flatten)]
        target: ProfileArgs,
    },
}

/// An on/off switch for a boolean setting.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Toggle {
    On,
    Off,
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
        /// Where the game reads its INIs (default: Documents\My Games\<game>)
        #[arg(long)]
        ini_dir: Option<Utf8PathBuf>,
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

#[derive(Subcommand)]
pub enum PatchCommand {
    /// Flip Fallout 4 BA2 archives between OG (v1) and NG (v7/8)
    Ba2 {
        /// A `.ba2` file, or a directory of them
        path: Utf8PathBuf,
        /// Target edition: `og` (v1) or `ng` (v8)
        #[arg(long, value_name = "og|ng", hide_possible_values = true)]
        to: GenerationArg,
        /// Show what would change without writing
        #[arg(long)]
        dry_run: bool,
        /// Patch a while directory without the preview confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Convert a Fallout 4 install between verified binary generations
    Convert {
        /// Target edition
        #[arg(long, value_name = "og|ng|ae", hide_possible_values = true)]
        to: GenerationArg,
        /// Directory containing `.vcdiff` or `.xdelta` files with usable app headers
        #[arg(long, value_name = "DIR")]
        deltas: Option<Utf8PathBuf>,
        /// Instance directory; supplies the game dir when `--game-dir` is omitted
        #[arg(long, value_name = "DIR")]
        instance: Option<Utf8PathBuf>,
        /// Fallout 4 install directory; overrides the instance config
        #[arg(long, value_name = "DIR")]
        game_dir: Option<Utf8PathBuf>,
        /// xdelta3 delta for `Fallout4.exe`
        #[arg(long, value_name = "PATH")]
        exe_delta: Option<Utf8PathBuf>,
        /// xdelta3 delta for `Fallout4Launcher.exe`
        #[arg(long, value_name = "PATH")]
        launcher_delta: Option<Utf8PathBuf>,
        /// xdelta3 delta for `steam_api64.dll`
        #[arg(long, value_name = "PATH")]
        steamapi_delta: Option<Utf8PathBuf>,
        /// Path to the `xdelta3` executable
        #[arg(long, value_name = "PATH")]
        xdelta3: Option<Utf8PathBuf>,
        /// Permit an incomplete repair instead of a complete generation conversion
        #[arg(long)]
        allow_incomplete_repair: bool,
        /// Show the plan without writing
        #[arg(long)]
        dry_run: bool,
        /// Apply the conversion (required since it mutates the real install)
        #[arg(long)]
        yes: bool,
    },
}

/// A Fallout 4 generation as a CLI argument (`og` / `ng` / `ae`), mapping to core's [`Generation`].
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum GenerationArg {
    Og,
    Ng,
    Ae,
}

impl GenerationArg {
    /// The core [`Generation`] this argument denotes.
    pub fn into_core(self) -> Generation {
        match self {
            GenerationArg::Og => Generation::OldGen,
            GenerationArg::Ng => Generation::NextGen,
            GenerationArg::Ae => Generation::Anniversary,
        }
    }
}
