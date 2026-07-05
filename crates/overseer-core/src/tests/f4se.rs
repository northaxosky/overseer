//! Tests for F4SE DLL parsing and version-independence

use super::*;

#[test]
fn garbage_is_not_a_pe() {
    assert_eq!(parse_f4se_dll(b"not a PE file at all"), F4seDll::NotPe);
    assert_eq!(parse_f4se_dll(&[]), F4seDll::NotPe);
}

fn indep(addr: u32, structure: u32) -> F4sePlugin {
    F4sePlugin {
        address_independence: addr,
        structure_independence: structure,
        ..Default::default()
    }
}

#[test]
fn an_ae_address_library_plugin_is_independent_on_anniversary_and_nextgen() {
    // Matches the real Hydra/CellOffset flags (AddrLib 980+137, layout 980+137)
    let p = indep(
        ADDR_LIBRARY_NG | ADDR_LIBRARY_AE,
        STRUCT_LAYOUT_NG | STRUCT_LAYOUT_AE,
    );
    assert!(p.version_independent_for(Generation::Anniversary));
    assert!(p.version_independent_for(Generation::NextGen));
    assert!(!p.version_independent_for(Generation::OldGen));
}

#[test]
fn a_nextgen_only_plugin_is_not_independent_on_anniversary() {
    let p = indep(ADDR_LIBRARY_NG, STRUCT_LAYOUT_NG);
    assert!(!p.version_independent_for(Generation::Anniversary));
    assert!(p.version_independent_for(Generation::NextGen));
}

#[test]
fn a_signature_structless_plugin_is_independent_on_all_post_og() {
    let p = indep(ADDR_SIGNATURES, STRUCT_NONE);
    assert!(p.version_independent_for(Generation::Anniversary));
    assert!(p.version_independent_for(Generation::NextGen));
    assert!(!p.version_independent_for(Generation::OldGen));
}

#[test]
fn an_address_dependent_plugin_is_never_independent() {
    // Structure-independent but hardcoded addresses → still version-locked
    let p = indep(0, STRUCT_LAYOUT_AE);
    assert!(!p.version_independent_for(Generation::Anniversary));
}

// A real 64-bit PE with exports but no F4SE entry points classifies as NotF4se. Uses a; stock Windows DLL so we exercise pelite's export parsing on a genuine binary
#[cfg(windows)]
#[test]
fn a_real_non_f4se_dll_is_not_f4se() {
    let dll = r"C:\Windows\System32\kernel32.dll";
    if let Ok(bytes) = std::fs::read(dll) {
        assert_eq!(parse_f4se_dll(&bytes), F4seDll::NotF4se);
    }
}
