use super::archive::extract;
use super::error::{InstallError, io_err};
use super::root::find_content_root;
use crate::instance::{InstalledMod, Instance};
use camino::Utf8Path;
use walkdir::WalkDir;

/// Install a mod from an archive into the instance's `mods/<name>/` directory
pub fn install(
    instance: &Instance,
    archive: &Utf8Path,
    name: &str,
) -> Result<InstalledMod, InstallError> {
    let dest = instance.mods_dir().join(name);
    if dest.exists() {
        return Err(InstallError::AlreadyInstalled(name.to_owned()));
    }

    let staging = tempfile::tempdir().map_err(|e| io_err(&instance.mods_dir(), e))?;
    let staging_root = Utf8Path::from_path(staging.path())
        .ok_or_else(|| InstallError::NonUtf8Path(staging.path().display().to_string()))?;

    extract(archive, staging_root)?;
    let content_root = find_content_root(staging_root)?;

    if read_dir_is_empty(&content_root)? {
        return Err(InstallError::EmptyArchive);
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    move_dir(&content_root, &dest)?;

    Ok(InstalledMod {
        name: name.to_owned(),
    })
}

fn read_dir_is_empty(dir: &Utf8Path) -> Result<bool, InstallError> {
    let mut entries = std::fs::read_dir(dir).map_err(|e| io_err(dir, e))?;
    Ok(entries.next().is_none())
}

/// Move `from` to `to`, falling back to a recursive copy + remove when rename doesnt work
fn move_dir(from: &Utf8Path, to: &Utf8Path) -> Result<(), InstallError> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| io_err(from, e))?;
    Ok(())
}

/// Recursively copy `from`'s tree into `to` — the cross-volume fallback for a move.
fn copy_dir(from: &Utf8Path, to: &Utf8Path) -> Result<(), InstallError> {
    for entry in WalkDir::new(from) {
        let entry = entry.map_err(|e| io_err(from, e.into()))?;
        let src = Utf8Path::from_path(entry.path())
            .ok_or_else(|| InstallError::NonUtf8Path(entry.path().display().to_string()))?;
        let relative = src
            .strip_prefix(from)
            .expect("walked entry is under `from`");
        let dest = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| io_err(&dest, e))?;
        } else {
            std::fs::copy(src, &dest).map_err(|e| io_err(&dest, e))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use crate::test_support::temp;

    fn instance_in(base: &Utf8Path) -> Instance {
        Instance::new(base.join("instance"), base.join("game"))
    }

    /// Build a `.zip` at `path` from `(entry path, contents)` pairs. Nested paths
    /// (`Data/Textures/a.dds`) create their directories on extraction.
    fn make_zip(path: &Utf8Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for &(name, data) in entries {
            zip.start_file(name.to_string(), opts).expect("start_file");
            zip.write_all(data).expect("write entry");
        }
        zip.finish().expect("finish zip");
    }

    #[test]
    fn installs_flat_zip_into_mods_dir() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        let archive = base.join("CoolMod.zip");
        make_zip(
            &archive,
            &[("Textures/a.dds", b"tex"), ("CoolMod.esp", b"plugin")],
        );

        let installed = install(&instance, &archive, "CoolMod").expect("install");
        assert_eq!(installed.name, "CoolMod");

        let mod_dir = instance.mods_dir().join("CoolMod");
        assert_eq!(
            std::fs::read_to_string(mod_dir.join("Textures/a.dds")).unwrap(),
            "tex"
        );
        assert!(mod_dir.join("CoolMod.esp").exists());
    }

    #[test]
    fn strips_data_wrapper_when_installing() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        let archive = base.join("Wrapped.zip");
        make_zip(
            &archive,
            &[
                ("Data/Textures/a.dds", b"tex"),
                ("Data/Wrapped.esp", b"plugin"),
            ],
        );

        install(&instance, &archive, "Wrapped").expect("install");
        let mod_dir = instance.mods_dir().join("Wrapped");
        // The Data/ wrapper is gone; its contents sit directly under the mod dir.
        assert!(mod_dir.join("Textures/a.dds").exists());
        assert!(mod_dir.join("Wrapped.esp").exists());
        assert!(!mod_dir.join("Data").exists());
    }

    #[test]
    fn strips_single_name_wrapper_when_installing() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        let archive = base.join("Named.zip");
        make_zip(&archive, &[("NamedMod/Meshes/a.nif", b"mesh")]);

        install(&instance, &archive, "Named").expect("install");
        let mod_dir = instance.mods_dir().join("Named");
        assert!(mod_dir.join("Meshes/a.nif").exists());
        assert!(!mod_dir.join("NamedMod").exists());
    }

    #[test]
    fn installed_mod_shows_up_in_instance_discovery() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        let archive = base.join("Disc.zip");
        make_zip(&archive, &[("Textures/a.dds", b"x")]);

        install(&instance, &archive, "Disc").expect("install");
        let names: Vec<String> = instance
            .installed_mods()
            .expect("discover")
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert_eq!(names, ["Disc"]);
    }

    #[test]
    fn refuses_to_overwrite_existing_mod() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        std::fs::create_dir_all(instance.mods_dir().join("Existing")).unwrap();
        let archive = base.join("Existing.zip");
        make_zip(&archive, &[("Textures/a.dds", b"x")]);

        let err = install(&instance, &archive, "Existing").expect_err("should refuse");
        assert!(matches!(err, InstallError::AlreadyInstalled(name) if name == "Existing"));
    }

    #[test]
    fn rejects_unsupported_archive_format() {
        let (_t, base) = temp();
        let instance = instance_in(&base);
        let archive = base.join("mod.rar");
        std::fs::write(&archive, b"not really a rar").unwrap();

        let err = install(&instance, &archive, "X").expect_err("should reject");
        assert!(matches!(err, InstallError::UnsupportedFormat { extension } if extension == "rar"));
    }
}
