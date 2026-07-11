//! Tests for VCDIFF header parsing and delta auto-mapping

use super::*;
use crate::test_support::temp;

const CORE: &[&str] = &["Fallout4.exe", "Fallout4Launcher.exe", "steam_api64.dll"];
const DLC: &[&str] = &[
    "Data/DLCCoast.esm",
    "Data/DLCCoast - Textures.ba2",
    "Data/DLCNukaWorld.esm",
];

fn header_delta(app: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = vec![0xD6, 0xC3, 0xC4, 0x00, 0x00];
    if let Some(app) = app {
        bytes[4] = VCD_APPHEADER;
        write_varint(&mut bytes, app.len());
        bytes.extend_from_slice(app);
    }
    bytes
}

fn write_varint(out: &mut Vec<u8>, mut value: usize) {
    let mut stack = vec![(value & 0x7F) as u8];
    value >>= 7;
    while value > 0 {
        stack.push(((value & 0x7F) as u8) | 0x80);
        value >>= 7;
    }
    out.extend(stack.into_iter().rev());
}

/// Read a target basename from a VCDIFF application header
#[test]
fn reads_vcdiff_application_header_basename() {
    let (_tmp, root) = temp();
    let path = root.join("patch.vcdiff");
    std::fs::write(
        &path,
        header_delta(Some(br"C:\old\Fallout4.exe//C:\new\Fallout4.exe/")),
    )
    .unwrap();
    assert_eq!(target_from_header(&path, CORE).unwrap(), "Fallout4.exe");
}

#[test]
fn skips_a_length_prefixed_code_table_to_reach_the_app_header() {
    let mut bytes = vec![0xD6, 0xC3, 0xC4, 0x00, VCD_CODETABLE | VCD_APPHEADER];
    let code_table = [0x01, 0x02, 0x03, 0x04, 0x05];
    write_varint(&mut bytes, code_table.len());
    bytes.extend_from_slice(&code_table);
    let app = br"C:\new\Fallout4.exe/";
    write_varint(&mut bytes, app.len());
    bytes.extend_from_slice(app);
    let header = parse_app_header(Utf8Path::new("patch.vcdiff"), &bytes).unwrap();
    assert_eq!(header.as_deref(), Some(r"C:\new\Fallout4.exe/"));
}

/// A truncated or mislabeled .vcdiff (partial download, wrong file) is rejected, not misparsed
#[test]
fn malformed_delta_headers_are_rejected() {
    assert!(matches!(
        parse_app_header(Utf8Path::new("x.vcdiff"), b"VC"),
        Err(VcdiffError::TooShort { .. })
    ));
    assert!(matches!(
        parse_app_header(Utf8Path::new("x.vcdiff"), b"NOPE-"),
        Err(VcdiffError::BadMagic { .. })
    ));
}

/// The VCD_DECOMPRESS indicator inserts a secondary-compressor id byte before the application header
#[test]
fn skips_the_secondary_compressor_byte_to_reach_the_app_header() {
    let mut bytes = vec![0xD6, 0xC3, 0xC4, 0x00, VCD_DECOMPRESS | VCD_APPHEADER];
    bytes.push(0x00);
    let app = br"C:\new\Fallout4.exe/";
    write_varint(&mut bytes, app.len());
    bytes.extend_from_slice(app);

    let header = parse_app_header(Utf8Path::new("patch.vcdiff"), &bytes).unwrap();
    assert_eq!(header.as_deref(), Some(r"C:\new\Fallout4.exe/"));
}

#[test]
fn maps_a_dlc_delta_to_its_data_rel_path() {
    let (_tmp, root) = temp();
    let path = root.join("d.vcdiff");
    let header = br"C:\mods\Best DLC Data\DLCCoast - Textures.ba2//C:\Steam\Fallout 4\Data\DLCCoast - Textures.ba2/";
    std::fs::write(&path, header_delta(Some(header))).unwrap();
    assert_eq!(
        target_from_header(&path, DLC).unwrap(),
        "Data/DLCCoast - Textures.ba2"
    );
}

#[test]
fn headerless_delta_requires_explicit_mapping() {
    let (_tmp, root) = temp();
    let path = root.join("patch.vcdiff");
    std::fs::write(&path, header_delta(None)).unwrap();
    assert!(matches!(
        target_from_header(&path, CORE),
        Err(VcdiffError::MissingAppHeaderName { .. })
    ));
}

#[test]
fn an_off_scope_header_is_distinct_from_a_missing_one() {
    let (_tmp, root) = temp();
    let path = root.join("dlc.vcdiff");
    let header = br"old\DLCCoast.esm//new\Data\DLCCoast.esm/";
    std::fs::write(&path, header_delta(Some(header))).unwrap();
    assert!(matches!(
        target_from_header(&path, CORE),
        Err(VcdiffError::OffScopeTarget { .. })
    ));
}

#[test]
fn duplicate_basenames_are_rejected() {
    let (_tmp, root) = temp();
    for name in ["a.vcdiff", "b.vcdiff"] {
        std::fs::write(
            root.join(name),
            header_delta(Some(br"C:\old\steam_api64.dll//C:\new\steam_api64.dll/")),
        )
        .unwrap();
    }
    assert!(matches!(
        map_deltas(&root, CORE),
        Err(VcdiffError::DuplicateBinary { .. })
    ));
}

#[test]
fn maps_deltas_recursively_across_subdirs() {
    let (_tmp, root) = temp();
    std::fs::create_dir_all(root.join("DLCCoast/Data")).unwrap();
    std::fs::create_dir_all(root.join("core")).unwrap();
    std::fs::write(
        root.join("DLCCoast/Data/DLCCoast.esm.vcdiff"),
        header_delta(Some(br"old\DLCCoast.esm//new\Data\DLCCoast.esm/")),
    )
    .unwrap();
    std::fs::write(
        root.join("core/exe.vcdiff"),
        header_delta(Some(br"old\Fallout4.exe//new\Fallout4.exe/")),
    )
    .unwrap();
    let allowed = ["Data/DLCCoast.esm", "Fallout4.exe"];
    let map = map_deltas(&root, &allowed).unwrap();
    assert!(map.mapped.contains_key("Data/DLCCoast.esm"));
    assert!(map.mapped.contains_key("Fallout4.exe"));
    assert_eq!(map.mapped.len(), 2);
    assert!(map.ignored.is_empty());
}

#[test]
fn a_mixed_pack_ignores_off_scope_deltas() {
    let (_tmp, root) = temp();
    std::fs::write(
        root.join("core.vcdiff"),
        header_delta(Some(br"old\Fallout4.exe//new\Fallout4.exe/")),
    )
    .unwrap();
    std::fs::write(
        root.join("dlc.vcdiff"),
        header_delta(Some(br"old\DLCCoast.esm//new\Data\DLCCoast.esm/")),
    )
    .unwrap();
    let map = map_deltas(&root, DLC).unwrap();
    assert!(map.mapped.contains_key("Data/DLCCoast.esm"));
    assert_eq!(map.mapped.len(), 1);
    assert_eq!(map.ignored.len(), 1);
    assert!(map.ignored[0].as_str().ends_with("core.vcdiff"));
}

#[test]
fn real_downgrader_style_header_maps_launcher() {
    let (_tmp, root) = temp();
    let path = root.join("patch.xdelta");
    let header = br"C:\Users\KARV\Downloads\fo4patchy\fo4andsteamdll\old\Fallout4Launcher.exe//C:\Users\KARV\Downloads\fo4patchy\fo4andsteamdll\NEW\Fallout4Launcher.exe/";
    std::fs::write(&path, header_delta(Some(header))).unwrap();
    assert_eq!(
        target_from_header(&path, CORE).unwrap(),
        "Fallout4Launcher.exe"
    );
}
