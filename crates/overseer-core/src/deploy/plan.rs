//! Deployment plan generation and file resolution for mod deployment.

use std::collections::BTreeMap;

use super::error::{DeployError, non_utf8};
use super::layout::{DATA_DIR, ROOT_DIR};
use camino::{Utf8Path, Utf8PathBuf};
use walkdir::WalkDir;

/// Where a deployed file comes from: a managed mod or the global overwrite
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderOrigin {
    Mod { name: String },
    Overwrite,
}

impl ProviderOrigin {
    /// A display label, `Overwrite` for the global overwrite
    pub fn display_name(&self) -> &str {
        match self {
            ProviderOrigin::Mod { name } => name,
            ProviderOrigin::Overwrite => "Overwrite",
        }
    }
}

/// A deploy source on disk: its typed origin + staging directory
#[derive(Debug, Clone)]
pub struct ModSource {
    pub origin: ProviderOrigin,
    pub staging_dir: Utf8PathBuf,
}

impl ModSource {
    /// A managed mod source named `name`
    pub fn new(name: impl Into<String>, staging_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            origin: ProviderOrigin::Mod { name: name.into() },
            staging_dir: staging_dir.into(),
        }
    }

    /// The global overwrite source
    pub fn overwrite(staging_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            origin: ProviderOrigin::Overwrite,
            staging_dir: staging_dir.into(),
        }
    }

    /// A display label for this source, `Overwrite` for the global overwrite
    pub fn display_name(&self) -> &str {
        self.origin.display_name()
    }

    /// The managed mod name, or `None` for the overwrite
    pub fn mod_name(&self) -> Option<&str> {
        match &self.origin {
            ProviderOrigin::Mod { name } => Some(name),
            ProviderOrigin::Overwrite => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannedFile {
    /// Path relative to the target root
    pub relative: Utf8PathBuf,
    /// Absolute path to the source file in the winning mod's staging dir
    pub source: Utf8PathBuf,
    /// Name of the mod that won this path
    pub winner: String,
}

#[derive(Debug, Clone)]
pub struct DeployPlan {
    pub target_root: Utf8PathBuf,
    files: Vec<PlannedFile>,
}

impl DeployPlan {
    /// Build a plan from an ordered list of mods. When two mods provide the same path, the higher-priority one wins
    pub fn from_mods(
        target_root: impl Into<Utf8PathBuf>,
        mods: &[ModSource],
    ) -> Result<Self, DeployError> {
        let target_root = target_root.into();
        let mut winners: BTreeMap<String, PlannedFile> = BTreeMap::new();

        for m in mods {
            walk_mod_files(m, |relative, abs| {
                let key = logical_path_key(&relative);
                winners.insert(
                    key,
                    PlannedFile {
                        relative,
                        source: abs,
                        winner: m.display_name().to_owned(),
                    },
                );
                Ok(())
            })?;
        }

        Ok(Self {
            target_root,
            files: winners.into_values().collect(),
        })
    }

    /// Build a plan rooted at the game directory, honoring the `Root/` convention
    pub fn from_rooted_mods(
        game_dir: impl Into<Utf8PathBuf>,
        mods: &[ModSource],
    ) -> Result<Self, DeployError> {
        let files = enumerate_destinations(mods)?
            .into_values()
            .map(|entry| {
                let winner = entry
                    .providers
                    .last()
                    .expect("a destination always has at least one provider");
                PlannedFile {
                    relative: entry.destination,
                    source: winner.source.clone(),
                    winner: winner.origin.display_name().to_owned(),
                }
            })
            .collect();
        Ok(Self {
            target_root: game_dir.into(),
            files,
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

/// One mod's contribution t oa destination
#[derive(Debug, Clone)]
pub struct Provider {
    pub origin: ProviderOrigin,
    pub source: Utf8PathBuf,
}

/// A final destination and every mod that provides it, low->high (winner last)
#[derive(Debug, Clone)]
pub struct DestinationEntry {
    pub destination: Utf8PathBuf,
    pub providers: Vec<Provider>,
}

/// Map each final game-relative destination to its ordered providers; shared by deploy + inspect
pub(crate) fn enumerate_destinations(
    mods: &[ModSource],
) -> Result<BTreeMap<String, DestinationEntry>, DeployError> {
    let mut map: BTreeMap<String, DestinationEntry> = BTreeMap::new();
    for m in mods {
        walk_mod_files(m, |relative, abs| {
            let destination = map_root_relative(m.display_name(), &relative)?;
            let entry = map
                .entry(logical_path_key(&destination))
                .or_insert_with(|| DestinationEntry {
                    destination: destination.clone(),
                    providers: Vec::new(),
                });
            entry.destination = destination;
            // one provider per origin: drop any earlier occurrence, keep the highest-priority one
            entry
                .providers
                .retain(|p| !same_origin(&p.origin, &m.origin));
            entry.providers.push(Provider {
                origin: m.origin.clone(),
                source: abs,
            });
            Ok(())
        })?;
    }
    Ok(map)
}

/// Whether two origins are the same provider (mod names compared case-insensitively)
fn same_origin(a: &ProviderOrigin, b: &ProviderOrigin) -> bool {
    match (a, b) {
        (ProviderOrigin::Overwrite, ProviderOrigin::Overwrite) => true,
        (ProviderOrigin::Mod { name: x }, ProviderOrigin::Mod { name: y }) => {
            x.eq_ignore_ascii_case(y)
        }
        _ => false,
    }
}

/// Build the case-folded key shared by planning, ownership matching, and recovery
pub(crate) fn logical_path_key(path: &Utf8Path) -> String {
    path.as_str().to_lowercase()
}

/// Walk a mod's staging dir, invoking `f(relative, absolute)` for every file in `WalkDir` order
pub(super) fn walk_mod_files(
    m: &ModSource,
    mut f: impl FnMut(Utf8PathBuf, Utf8PathBuf) -> Result<(), DeployError>,
) -> Result<(), DeployError> {
    if !m.staging_dir.is_dir() {
        return Err(DeployError::MissingStaging {
            mod_name: m.display_name().to_owned(),
            path: m.staging_dir.clone(),
        });
    }
    for entry in WalkDir::new(&m.staging_dir) {
        let entry = entry.map_err(|source| DeployError::Walk {
            path: m.staging_dir.clone(),
            source,
        })?;
        if !entry.file_type().is_file() {
            continue;
        }

        let abs = Utf8Path::from_path(entry.path())
            .ok_or_else(|| DeployError::NonUtf8Path(non_utf8(entry.path())))?;
        let relative = abs
            .strip_prefix(&m.staging_dir)
            .expect("walked entry is always under a staging dir")
            .to_owned();

        // Exclude root metadata that is not game content
        if ["meta.ini", ".overseer-mod.toml"]
            .iter()
            .any(|name| relative.as_str().eq_ignore_ascii_case(name))
        {
            continue;
        }
        f(relative, abs.to_owned())?;
    }
    Ok(())
}

/// Map a staged file's path to its deploy destination, relative to the game root
fn map_root_relative(mod_name: &str, relative: &Utf8Path) -> Result<Utf8PathBuf, DeployError> {
    let mut components = relative.components();
    let under_root = match components.next() {
        Some(first) if first.as_str().eq_ignore_ascii_case(ROOT_DIR) => components.as_path(),
        _ => return Ok(Utf8Path::new(DATA_DIR).join(relative)),
    };

    // A top level file literally named "Root"
    if under_root.as_str().is_empty() {
        return Ok(Utf8Path::new(DATA_DIR).join(relative));
    }

    if under_root
        .components()
        .next()
        .is_some_and(|c| c.as_str().eq_ignore_ascii_case(DATA_DIR))
    {
        return Err(DeployError::RootDataConflict {
            name: mod_name.to_owned(),
            path: relative.to_owned(),
        });
    }

    Ok(under_root.to_owned())
}

#[cfg(test)]
#[path = "tests/plan.rs"]
mod tests;
