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
        #[command(flatten)]
        instance: InstanceArgs,
    },

    /// Install a mod from a Downloads archive (.7z or .zip)
    Install {
        /// Archive basename under the instance's downloads/ directory
        archive: String,
        #[command(flatten)]
        instance: InstanceArgs,
        /// Name for the installed mod (defaults to the archive's file name)
        #[arg(long)]
        name: Option<String>,
    },

    /// List the installable archives in the instance's downloads/ directory
    Downloads {
        #[command(flatten)]
        instance: InstanceArgs,
    },

    /// Manage installed mods and a profile's mod order
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
        #[command(flatten)]
        instance: InstanceArgs,
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
        #[command(flatten)]
        instance: InstanceArgs,
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

    /// Merge BA2 archives into one managed mod, reversibly
    Merge(MergeArgs),
}

/// The instance directory, shared by every instance-scoped subcommand
#[derive(Args)]
pub struct InstanceArgs {
    /// Instance directory (contains mods/ and profiles/)
    #[arg(long)]
    pub instance: Utf8PathBuf,
}

/// Arguments shared by every profile-scoped subcommand
#[derive(Args)]
pub struct ProfileArgs {
    #[command(flatten)]
    pub instance: InstanceArgs,
    /// Profile name (defaults to the instance's configured default profile)
    #[arg(long)]
    pub profile: Option<String>,
}

/// The apply/preview gate flags shared by every mutating command
#[derive(Args, Clone, Copy)]
pub struct ApplyGate {
    /// Show the plan without writing
    #[arg(long)]
    pub dry_run: bool,
    /// Apply the change (required since it mutates real files)
    #[arg(long)]
    pub yes: bool,
}

impl ApplyGate {
    /// The [`Gate`](crate::ui::Gate) these flags select
    pub fn gate(self) -> crate::ui::Gate {
        crate::ui::Gate::from_flags(self.dry_run, self.yes)
    }
}

/// The delta-source flags shared by the VCDIFF conversion commands
#[derive(Args, Clone)]
pub struct DeltaSourceArgs {
    /// Directory containing `.vcdiff` or `.xdelta` files with usable application headers
    #[arg(long, value_name = "DIR")]
    pub deltas: Option<Utf8PathBuf>,
    /// Instance directory; supplies the game dir when `--game-dir` is omitted
    #[arg(long, value_name = "DIR")]
    pub instance: Option<Utf8PathBuf>,
    /// Fallout 4 install directory; overrides the instance config
    #[arg(long, value_name = "DIR")]
    pub game_dir: Option<Utf8PathBuf>,
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
        #[command(flatten)]
        instance: InstanceArgs,
    },
    /// Remove an installed mod from the instance
    Remove {
        /// Installed mod name
        name: String,
        #[command(flatten)]
        instance: InstanceArgs,
    },
    /// Replace an installed mod from a Downloads archive
    Replace {
        /// Installed mod name
        name: String,
        /// Archive basename under the instance's downloads/ directory
        archive: String,
        #[command(flatten)]
        instance: InstanceArgs,
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
        #[command(flatten)]
        instance: InstanceArgs,
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

/// An on/off switch for a boolean setting
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
        #[command(flatten)]
        instance: InstanceArgs,
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
        #[command(flatten)]
        instance: InstanceArgs,
    },
    /// Remove a launch target by name
    Remove {
        /// The target's name
        name: String,
        #[command(flatten)]
        instance: InstanceArgs,
    },
}

#[derive(Subcommand)]
pub enum PatchCommand {
    /// Flip Fallout 4 BA2 archives between OG (v1) and NG (v7/8)
    Ba2 {
        /// A `.ba2` file, or a directory of them
        path: Utf8PathBuf,
        /// Target edition: `og` (v1) or `ng` (v8)
        #[arg(long, value_name = "og|ae", hide_possible_values = true)]
        to: GenerationArg,
        #[command(flatten)]
        gate: ApplyGate,
    },
    /// Convert a Fallout 4 install between verified binary generations
    Convert {
        /// Target edition
        #[arg(long, value_name = "og|ae", hide_possible_values = true)]
        to: GenerationArg,
        #[command(flatten)]
        source: DeltaSourceArgs,
        /// VCDIFF delta for `Fallout4.exe`
        #[arg(long, value_name = "PATH")]
        exe_delta: Option<Utf8PathBuf>,
        /// VCDIFF delta for `Fallout4Launcher.exe`
        #[arg(long, value_name = "PATH")]
        launcher_delta: Option<Utf8PathBuf>,
        /// VCDIFF delta for `steam_api64.dll`
        #[arg(long, value_name = "PATH")]
        steamapi_delta: Option<Utf8PathBuf>,
        /// Permit an incomplete repair instead of a complete generation conversion
        #[arg(long)]
        allow_incomplete_repair: bool,
        #[command(flatten)]
        gate: ApplyGate,
    },
    /// Bring the Fallout 4 DLC to the cross-storefront consistency revision
    DlcConsistency {
        #[command(flatten)]
        source: DeltaSourceArgs,
        /// Permit an incomplete repair instead of a complete consistency revision
        #[arg(long)]
        allow_incomplete_repair: bool,
        #[command(flatten)]
        gate: ApplyGate,
    },
}

/// Arguments for `overseer merge`
#[derive(Args)]
pub struct MergeArgs {
    #[command(flatten)]
    pub target: ProfileArgs,
    #[command(flatten)]
    pub source: MergeSource,
    /// Output mod name (required with --list, defaults to CCMerged with --cc)
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Uncompressed texture group cap in GiB before a split (default 4)
    #[arg(long, value_name = "GIB")]
    pub texture_cap: Option<u64>,
    #[command(flatten)]
    pub gate: ApplyGate,
}

/// The mutually exclusive selector for what `merge` acts on
#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct MergeSource {
    /// Merge the profile's Creation Club archive
    #[arg(long)]
    pub cc: bool,
    /// Merge the plugins listed in FILE (one filename per line, # comments allowed)
    #[arg(long, value_name = "FILE")]
    pub list: Option<Utf8PathBuf>,
    /// Reverse a previous merge by name, restoring its archives
    #[arg(long, value_name = "NAME")]
    pub restore: Option<String>,
}

/// A Fallout 4 generation as a CLI argument (`og` / `ae`), mapping to core's [`Generation`]
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum GenerationArg {
    Og,
    Ae,
}

impl GenerationArg {
    /// The core [`Generation`] this argument denotes
    pub fn into_core(self) -> Generation {
        match self {
            GenerationArg::Og => Generation::OldGen,
            GenerationArg::Ae => Generation::Anniversary,
        }
    }
}
