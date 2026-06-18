use super::error::{InstanceError, io_err};
use camino::{Utf8Path, Utf8PathBuf};

/// A managed Overseer instance: a `mods/` folder and `profiles/`, plus target game
#[derive(Debug, Clone)]
pub struct Instance {
    pub root: Utf8PathBuf,
    pub game_dir: Utf8PathBuf,
}

/// An installed mod: a named staging folder under the instance's `mods/` directory.
#[derive(Debug, Clone)]
pub struct InstalledMod {
    pub name: String,
}

impl Instance {
    pub fn new(root: impl Into<Utf8PathBuf>, game_dir: impl Into<Utf8PathBuf>) -> Self {
        Self {
            root: root.into(),
            game_dir: game_dir.into(),
        }
    }

    pub fn mods_dir(&self) -> Utf8PathBuf {
        self.root.join("mods")
    }

    pub fn profiles_dir(&self) -> Utf8PathBuf {
        self.root.join("profiles")
    }

    pub fn profile_dir(&self, name: &str) -> Utf8PathBuf {
        self.profiles_dir().join(name)
    }

    /// Installed mods: the immediate subdirectories of `mods/`, sorted by name
    pub fn installed_mods(&self) -> Result<Vec<InstalledMod>, InstanceError> {
        let names = read_subdirs(&self.mods_dir())?;
        Ok(names
            .into_iter()
            .map(|name| InstalledMod { name })
            .collect())
    }

    /// Profile names: the immediate subdirectories of `profiles/`, sorted
    pub fn profiles(&self) -> Result<Vec<String>, InstanceError> {
        read_subdirs(&self.profiles_dir())
    }
}

/// Names of the immediate subdirectories of `dir`, sorted; a missing dir is an empty list
fn read_subdirs(dir: &Utf8Path) -> Result<Vec<String>, InstanceError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(dir, e)),
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        if entry.file_type().map_err(|e| io_err(dir, e))?.is_dir() {
            let os_name = entry.file_name();
            match os_name.to_str() {
                Some(name) => names.push(name.to_owned()),
                None => {
                    return Err(InstanceError::NonUtf8Path(
                        os_name.to_string_lossy().into_owned(),
                    ));
                }
            }
        }
    }
    names.sort();
    Ok(names)
}
