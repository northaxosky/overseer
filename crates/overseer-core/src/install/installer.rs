//! Low-level archive preparation for installed-mod lifecycle operations

use super::archive::extract;
use super::error::InstallError;
use super::root::find_content_root;
use crate::error::{io_err, non_utf8};
use crate::fs;
use camino::{Utf8Path, Utf8PathBuf};
use walkdir::WalkDir;

/// Extract and normalize an archive into `bundle/new`
pub(crate) fn prepare_candidate(
    archive: &Utf8Path,
    bundle: &Utf8Path,
) -> Result<Utf8PathBuf, InstallError> {
    let work = bundle.join("work");
    let candidate = bundle.join("new");
    extract(archive, &work)?;
    let content_root = find_content_root(&work)?;
    if fomod_in_chain(&work, &content_root)? {
        return Err(InstallError::Fomod);
    }
    if child_named(&content_root, ".overseer-mod.toml", false)?.is_some() {
        return Err(InstallError::ReservedMetadata);
    }
    if read_dir_is_empty(&content_root)? {
        return Err(InstallError::EmptyArchive);
    }
    move_dir(&content_root, &candidate)?;
    fs::remove_dir_all_opt(&work)?;
    Ok(candidate)
}

fn read_dir_is_empty(dir: &Utf8Path) -> Result<bool, InstallError> {
    let mut entries = std::fs::read_dir(dir).map_err(|e| io_err(dir, e))?;
    Ok(entries.next().is_none())
}

/// Whether any directory from `content_root` up to `top` is a FOMOD root
fn fomod_in_chain(top: &Utf8Path, content_root: &Utf8Path) -> Result<bool, InstallError> {
    for dir in content_root.ancestors() {
        if is_fomod(dir)? {
            return Ok(true);
        }
        if dir == top {
            break;
        }
    }
    Ok(false)
}

/// Whether `content_root` looks like a FOMOD installer: `fomod` dir holding `ModuleConfig.xml`
fn is_fomod(content_root: &Utf8Path) -> Result<bool, InstallError> {
    let Some(fomod) = child_named(content_root, "fomod", true)? else {
        return Ok(false);
    };
    Ok(child_named(&fomod, "ModuleConfig.xml", false)?.is_some())
}

/// The path of `dir`'s child named `name` of the wanted kind
fn child_named(
    dir: &Utf8Path,
    name: &str,
    want_dir: bool,
) -> Result<Option<Utf8PathBuf>, InstallError> {
    for entry in std::fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let entry = entry.map_err(|e| io_err(dir, e))?;
        if entry.file_type().map_err(|e| io_err(dir, e))?.is_dir() != want_dir {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name.eq_ignore_ascii_case(name) {
            return Ok(Some(dir.join(file_name.as_ref())));
        }
    }
    Ok(None)
}

/// Move `from` to `to`, falling back to a recursive copy + remove when rename doesn't work
fn move_dir(from: &Utf8Path, to: &Utf8Path) -> Result<(), InstallError> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| io_err(from, e))?;
    Ok(())
}

/// Recursively copy `from`'s tree into `to` — the cross-volume fallback for a move
fn copy_dir(from: &Utf8Path, to: &Utf8Path) -> Result<(), InstallError> {
    for entry in WalkDir::new(from) {
        let entry = entry.map_err(|e| io_err(from, e.into()))?;
        let src = Utf8Path::from_path(entry.path())
            .ok_or_else(|| InstallError::NonUtf8Path(non_utf8(entry.path())))?;
        let relative = src
            .strip_prefix(from)
            .expect("walked entry is under `from`");
        let dest = to.join(relative);
        if entry.file_type().is_dir() {
            fs::ensure_dir(&dest)?;
        } else {
            std::fs::copy(src, &dest).map_err(|e| io_err(&dest, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/installer.rs"]
mod tests;
