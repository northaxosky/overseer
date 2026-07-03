//! Static inspection of F4SE plugin DLLs (`Data/F4SE/Plugins/*.dll`): which Fallout 4 runtime each advertises

use pelite::pe64::{Pe, PeFile};

/// `compatibleVersions[16]` sits at byte 528 of `F4SEPluginVersionData`; read up to its end
const COMPAT_OFFSET: usize = 528;
const PREFIX_LEN: usize = 592;

/// What a DLL in `F4SE/Plugins/` turned out to be
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum F4seDll {
    /// Not a 64-bit PE we could parse
    NotPe,
    /// A PE, but exports no F4SE entry point; not an F4SE plugin
    NotF4se,
    /// An F4SE plugin and what it advertises
    Plugin(F4sePlugin),
}

/// What an F4SE plugin advertises about runtime support
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct F4sePlugin {
    /// Exports the legacy `F4SEPlugin_Query` (Old-Gen plugin API)
    pub supports_og: bool,
    /// Exports `F4SEPlugin_Version` (the NG/AE plugin API)
    pub supports_ngae: bool,
    /// Exact packed runtimes from `compatibleVersion`
    pub compatible: Vec<u32>,
}

impl F4sePlugin {
    /// Whether this supports `runtime`; OG-only plugins are reported separately, so only version data is checked.
    pub fn supports(&self, runtime: u32) -> bool {
        self.compatible.contains(&runtime)
    }
}

/// Classify a DLL's bytes. Doesn't load/execute; tolerates malformed PEs and 32 bit DLLs
pub fn parse_f4se_dll(bytes: &[u8]) -> F4seDll {
    let Ok(pe) = PeFile::from_bytes(bytes) else {
        return F4seDll::NotPe;
    };
    let Ok(by) = pe.exports().and_then(|e| e.by()) else {
        return F4seDll::NotF4se;
    };

    let is_f4se = by.name(b"F4SEPlugin_Load").is_ok() || by.name(b"F4SEPlugin_Preload").is_ok();
    if !is_f4se {
        return F4seDll::NotF4se;
    }

    let mut plugin = F4sePlugin {
        supports_og: by.name(b"F4SEPlugin_Query").is_ok(),
        supports_ngae: false,
        compatible: Vec::new(),
    };

    if let Ok(export) = by.name(b"F4SEPlugin_Version")
        && let Some(rva) = export.symbol()
    {
        plugin.supports_ngae = true;
        if let Ok(buf) = pe.derva_slice::<u8>(rva, PREFIX_LEN) {
            // dataVersion (first u32) must be 1, or else unreadable
            let data_version = u32::from_le_bytes(buf[0..4].try_into().expect("4 bytes"));
            if data_version == 1 {
                for i in 0..16 {
                    let o = COMPAT_OFFSET + i * 4;
                    let v = u32::from_le_bytes(buf[o..o + 4].try_into().expect("4 bytes"));
                    if v != 0 {
                        plugin.compatible.push(v);
                    }
                }
            }
        }
    }

    F4seDll::Plugin(plugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_is_not_a_pe() {
        assert_eq!(parse_f4se_dll(b"not a PE file at all"), F4seDll::NotPe);
        assert_eq!(parse_f4se_dll(&[]), F4seDll::NotPe);
    }

    // A real 64-bit PE with exports but no F4SE entry points classifies as NotF4se. Uses a; stock Windows DLL so we exercise pelite's export parsing on a genuine binary.
    #[cfg(windows)]
    #[test]
    fn a_real_non_f4se_dll_is_not_f4se() {
        let dll = r"C:\Windows\System32\kernel32.dll";
        if let Ok(bytes) = std::fs::read(dll) {
            assert_eq!(parse_f4se_dll(&bytes), F4seDll::NotF4se);
        }
    }
}
