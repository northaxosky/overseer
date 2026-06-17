//! Mod deployment: turning an ordered set of mods into files that are visible in the
//! game directory.
//!
//! Overseer's v1 strategy is hardlink deployment (see [`HardlinkDeployer`]): every
//! file in an enabled mod's staging folder is hard-linked into the game's `Data`
//! directory in mod-priority order, so higher-priority mods win conflicts and the
//! files physically exist for any tool (the game, F4SE, xEdit, LOOT) to see.
//!
//! The [`Deployer`] trait abstracts the mechanism, so alternative backends (a USVFS
//! FFI backend, ProjFS, or plain copy) can be added later without changing callers.

mod hardlink;

pub use hardlink::HardlinkDeployer;

use std::collections::BTreeMap;

use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use walkdir::WalkDir;

/// Identifies which deployment backend produced a manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeployerKind {
    /// NTFS hard links (the v1 strategy).
    HardLink,
}

/// A mod as it exists on disk: a name plus a staging directory whose contents mirror
/// the layout they should have under the game's target directory.
#[derive(Debug, Clone)]
pub struct ModSource {
    pub name: String,
    pub staging_dir: Utf8PathBuf,
}

impl ModSource {
    pub fn new(name: impl Into<String>, staging_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            name: name.into(),
            staging_dir: staging_dir.into(),
        }
    }
}

/// One resolved file in a deploy plan: the winning source for a given destination.
#[derive(Debug, Clone)]
pub struct PlannedFile {
    /// Path relative to the target root (e.g. `Textures/foo.dds`).
    pub relative: Utf8PathBuf,
    /// Absolute path to the source file in the winning mod's staging dir.
    pub source: Utf8PathBuf,
    /// Name of the mod that won this path.
    pub winner: String,
}

/// A fully resolved plan: which source file maps to each destination under
/// `target_root`, after conflict resolution by mod priority.
#[derive(Debug, Clone)]
pub struct DeployPlan {
    pub target_root: Utf8PathBuf,
    files: Vec<PlannedFile>,
}

impl DeployPlan {
    /// Build a plan from an ordered list of mods (lowest priority first, highest
    /// priority last). When two mods provide the same relative path, the later
    /// (higher-priority) mod wins — matching Mod Organizer 2's overlay order.
    ///
    /// Path comparison is case-insensitive, because the game's filesystem is.
    pub fn from_mods(
        target_root: impl Into<Utf8PathBuf>,
        mods: &[ModSource],
    ) -> Result<Self, DeployError> {
        let target_root = target_root.into();
        // Keyed by lowercased relative path; the value keeps the original casing.
        let mut winners: BTreeMap<String, PlannedFile> = BTreeMap::new();

        for m in mods {
            if !m.staging_dir.is_dir() {
                return Err(DeployError::MissingStaging {
                    mod_name: m.name.clone(),
                    path: m.staging_dir.clone(),
                });
            }
            for entry in WalkDir::new(&m.staging_dir) {
                let entry = entry.map_err(DeployError::Walk)?;
                if !entry.file_type().is_file() {
                    continue;
                }
                let abs = Utf8Path::from_path(entry.path())
                    .ok_or_else(|| DeployError::NonUtf8Path(entry.path().display().to_string()))?;
                let relative = abs
                    .strip_prefix(&m.staging_dir)
                    .expect("walked entry is always under staging_dir")
                    .to_owned();
                let key = relative.as_str().to_lowercase();
                winners.insert(
                    key,
                    PlannedFile {
                        relative,
                        source: abs.to_owned(),
                        winner: m.name.clone(),
                    },
                );
            }
        }

        Ok(Self {
            target_root,
            files: winners.into_values().collect(),
        })
    }

    pub fn files(&self) -> &[PlannedFile] {
        &self.files
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// Record of what a deployment actually wrote, so it can be cleanly reversed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployManifest {
    pub backend: DeployerKind,
    pub target_root: Utf8PathBuf,
    /// Relative paths that were deployed, in deploy order.
    pub files: Vec<Utf8PathBuf>,
    /// Directories created under the target root (shallowest first), so they can be
    /// removed in reverse order if they end up empty after an undeploy.
    pub created_dirs: Vec<Utf8PathBuf>,
}

/// Result of checking that a manifest's files are still present on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub expected: usize,
    pub missing: Vec<Utf8PathBuf>,
}

impl VerifyReport {
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
    }
}

/// A progress event emitted during deploy/undeploy. Borrowed so emitting is cheap.
#[derive(Debug)]
pub enum ProgressEvent<'a> {
    Started {
        total: usize,
    },
    Deployed {
        index: usize,
        relative: &'a Utf8Path,
    },
    Removed {
        index: usize,
        relative: &'a Utf8Path,
    },
    Finished,
}

/// Sink for progress events. Keeping this a trait means the core never depends on a
/// specific UI: the CLI can render a progress bar, the desktop app can forward events
/// to its frontend, and tests can ignore them.
pub trait ProgressSink {
    fn on_event(&self, event: ProgressEvent<'_>);
}

/// A [`ProgressSink`] that discards everything.
pub struct NullSink;

impl ProgressSink for NullSink {
    fn on_event(&self, _event: ProgressEvent<'_>) {}
}

/// A mod deployment backend.
pub trait Deployer {
    fn kind(&self) -> DeployerKind;

    /// Check whether this backend can satisfy the plan (e.g. the same-volume
    /// requirement for hard links). Called automatically by [`Deployer::deploy`].
    fn check_supported(&self, plan: &DeployPlan) -> Result<(), DeployError>;

    /// Deploy every file in the plan, returning a manifest describing what was written.
    fn deploy(
        &self,
        plan: &DeployPlan,
        progress: &dyn ProgressSink,
    ) -> Result<DeployManifest, DeployError>;

    /// Reverse a previous deployment using its manifest.
    fn undeploy(
        &self,
        manifest: &DeployManifest,
        progress: &dyn ProgressSink,
    ) -> Result<(), DeployError>;

    /// Check that every file recorded in the manifest is still present.
    fn verify(&self, manifest: &DeployManifest) -> VerifyReport;
}

/// Errors produced by the deployment engine.
#[derive(Debug, Error)]
pub enum DeployError {
    #[error("mod `{mod_name}` has no staging directory at `{path}`")]
    MissingStaging { mod_name: String, path: Utf8PathBuf },

    #[error(
        "mods and target are on different volumes; hardlink deployment requires the \
         same drive (source `{source_path}`, target `{target}`)"
    )]
    CrossVolume {
        source_path: Utf8PathBuf,
        target: Utf8PathBuf,
    },

    #[error("path is not valid UTF-8: {0}")]
    NonUtf8Path(String),

    #[error("failed to walk a staging directory")]
    Walk(#[source] walkdir::Error),

    #[error("io error at `{path}`")]
    Io {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Helper to attach the offending path to an [`std::io::Error`].
pub(crate) fn io_err(path: &Utf8Path, source: std::io::Error) -> DeployError {
    DeployError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Whether two paths live on the same Windows volume (drive letter). When the volume
/// can't be determined (for example, relative paths), this returns `true` so it never
/// produces a false [`DeployError::CrossVolume`].
///
/// A production implementation should compare NTFS volume serial numbers via
/// `GetVolumeInformation`; the drive-letter check is sufficient for the v1 spike.
pub(crate) fn same_volume(a: &Utf8Path, b: &Utf8Path) -> bool {
    match (volume_id(a), volume_id(b)) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(&y),
        _ => true,
    }
}

fn volume_id(path: &Utf8Path) -> Option<String> {
    use std::path::{Component, Prefix};

    for component in path.as_std_path().components() {
        if let Component::Prefix(prefix) = component {
            return Some(match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    (letter as char).to_ascii_uppercase().to_string()
                }
                other => format!("{other:?}"),
            });
        }
    }
    None
}
