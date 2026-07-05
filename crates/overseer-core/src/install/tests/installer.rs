//! Tests for installing mod archives into an instance

use super::*;

use std::io::Write;

use crate::test_support::temp;

fn instance_in(base: &Utf8Path) -> Instance {
    Instance::new(base.join("instance"), base.join("game"))
}

/// Build a `.zip` at `path` from `(entry path, contents)` pairs, preserving nested entry paths
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
    // The Data/ wrapper is gone; its contents sit directly under the mod dir
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

#[test]
fn refuses_a_fomod_installer_and_stages_nothing() {
    let (_t, base) = temp();
    let instance = instance_in(&base);
    let archive = base.join("Scripted.zip");
    // A `fomod/ModuleConfig.xml` at the content root marks a scripted installer; the sibling Textures/ keeps find_content_root from descending past it
    make_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );

    let err = install(&instance, &archive, "Scripted").expect_err("should refuse");
    assert!(matches!(err, InstallError::Fomod), "got {err:?}");
    assert!(
        !instance.mods_dir().join("Scripted").exists(),
        "a refused FOMOD stages nothing"
    );
}

/// A truncated or empty archive extracts to nothing installable and is refused, staging nothing
#[test]
fn refuses_an_empty_archive_and_stages_nothing() {
    let (_t, base) = temp();
    let instance = instance_in(&base);
    let archive = base.join("Empty.zip");
    make_zip(&archive, &[]);

    let err = install(&instance, &archive, "Empty").expect_err("should refuse");
    assert!(matches!(err, InstallError::EmptyArchive), "got {err:?}");
    assert!(!instance.mods_dir().join("Empty").exists());
}

#[test]
fn fomod_detection_is_case_insensitive() {
    let (_t, base) = temp();
    let instance = instance_in(&base);
    let archive = base.join("Loud.zip");
    make_zip(
        &archive,
        &[
            ("FOMOD/MODULECONFIG.XML", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );

    let err = install(&instance, &archive, "Loud").expect_err("should refuse");
    assert!(matches!(err, InstallError::Fomod), "got {err:?}");
}

#[test]
fn refuses_a_fomod_wrapped_beside_a_data_folder() {
    // find_content_root descends into Data/, stepping past the fomod/ marker; the refusal must still fire by scanning the whole wrapper chain
    let (_t, base) = temp();
    let instance = instance_in(&base);
    let archive = base.join("Wrapped.zip");
    make_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Data/Textures/a.dds", b"tex"),
        ],
    );

    let err = install(&instance, &archive, "Wrapped").expect_err("should refuse");
    assert!(matches!(err, InstallError::Fomod), "got {err:?}");
    assert!(
        !instance.mods_dir().join("Wrapped").exists(),
        "a wrapped FOMOD stages nothing"
    );
}

#[test]
fn a_fomod_folder_without_module_config_still_installs() {
    // Only a `fomod/ModuleConfig.xml` triggers the refusal; a stray fomod/ folder without it is just data and installs normally
    let (_t, base) = temp();
    let instance = instance_in(&base);
    let archive = base.join("Plain.zip");
    make_zip(
        &archive,
        &[("fomod/readme.txt", b"notes"), ("Plain.esp", b"plugin")],
    );

    install(&instance, &archive, "Plain").expect("install");
    assert!(instance.mods_dir().join("Plain").join("Plain.esp").exists());
}
