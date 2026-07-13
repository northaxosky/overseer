//! Tests for low-level archive candidate preparation

use super::*;

use std::io::Write;

use crate::test_support::temp;

/// Build a `.zip` at `path` from path and content pairs
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

/// Prepare one archive into a fresh bundle and return its candidate
fn prepare(base: &Utf8Path, archive: &Utf8Path) -> Result<Utf8PathBuf, InstallError> {
    let bundle = base.join("bundle");
    std::fs::create_dir(&bundle).expect("create bundle");
    prepare_candidate(archive, &bundle)
}

#[test]
fn prepares_flat_zip_as_candidate() {
    let (_temp, base) = temp();
    let archive = base.join("CoolMod.zip");
    make_zip(
        &archive,
        &[("Textures/a.dds", b"tex"), ("CoolMod.esp", b"plugin")],
    );

    let candidate = prepare(&base, &archive).expect("prepare");

    assert_eq!(
        std::fs::read_to_string(candidate.join("Textures/a.dds")).expect("read texture"),
        "tex"
    );
    assert!(candidate.join("CoolMod.esp").exists());
}

#[test]
fn strips_data_wrapper_from_candidate() {
    let (_temp, base) = temp();
    let archive = base.join("Wrapped.zip");
    make_zip(
        &archive,
        &[
            ("Data/Textures/a.dds", b"tex"),
            ("Data/Wrapped.esp", b"plugin"),
        ],
    );

    let candidate = prepare(&base, &archive).expect("prepare");

    assert!(candidate.join("Textures/a.dds").exists());
    assert!(candidate.join("Wrapped.esp").exists());
    assert!(!candidate.join("Data").exists());
}

#[test]
fn strips_single_name_wrapper_from_candidate() {
    let (_temp, base) = temp();
    let archive = base.join("Named.zip");
    make_zip(&archive, &[("NamedMod/Meshes/a.nif", b"mesh")]);

    let candidate = prepare(&base, &archive).expect("prepare");

    assert!(candidate.join("Meshes/a.nif").exists());
    assert!(!candidate.join("NamedMod").exists());
}

#[test]
fn rejects_unsupported_archive_format() {
    let (_temp, base) = temp();
    let archive = base.join("mod.rar");
    std::fs::write(&archive, b"not really a rar").expect("write archive");

    let error = prepare(&base, &archive).expect_err("unsupported");

    assert!(matches!(
        error,
        InstallError::UnsupportedFormat { extension } if extension == "rar"
    ));
}

#[test]
fn refuses_fomod_candidate() {
    let (_temp, base) = temp();
    let archive = base.join("Scripted.zip");
    make_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );

    let error = prepare(&base, &archive).expect_err("refuse FOMOD");

    assert!(matches!(error, InstallError::Fomod));
}

#[test]
fn refuses_empty_candidate() {
    let (_temp, base) = temp();
    let archive = base.join("Empty.zip");
    make_zip(&archive, &[]);

    let error = prepare(&base, &archive).expect_err("refuse empty");

    assert!(matches!(error, InstallError::EmptyArchive));
}

#[test]
fn fomod_detection_is_case_insensitive() {
    let (_temp, base) = temp();
    let archive = base.join("Loud.zip");
    make_zip(
        &archive,
        &[
            ("FOMOD/MODULECONFIG.XML", b"<config/>"),
            ("Textures/a.dds", b"tex"),
        ],
    );

    let error = prepare(&base, &archive).expect_err("refuse FOMOD");

    assert!(matches!(error, InstallError::Fomod));
}

#[test]
fn refuses_fomod_wrapper_beside_data() {
    let (_temp, base) = temp();
    let archive = base.join("Wrapped.zip");
    make_zip(
        &archive,
        &[
            ("fomod/ModuleConfig.xml", b"<config/>"),
            ("Data/Textures/a.dds", b"tex"),
        ],
    );

    let error = prepare(&base, &archive).expect_err("refuse FOMOD");

    assert!(matches!(error, InstallError::Fomod));
}

#[test]
fn fomod_folder_without_module_config_is_content() {
    let (_temp, base) = temp();
    let archive = base.join("Plain.zip");
    make_zip(
        &archive,
        &[("fomod/readme.txt", b"notes"), ("Plain.esp", b"plugin")],
    );

    let candidate = prepare(&base, &archive).expect("prepare");

    assert!(candidate.join("Plain.esp").exists());
}
