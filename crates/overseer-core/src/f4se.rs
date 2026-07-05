//! Static inspection of F4SE plugin DLLs (`Data/F4SE/Plugins/*.dll`): which Fallout 4 runtime each advertises

use crate::detect::Generation;
use pelite::pe64::{Pe, PeFile};

/// `F4SEPluginVersionData` byte offsets: the two independence bitfields at 520/524, then `compatibleVersions[16]` at 528
const ADDRESS_INDEPENDENCE_OFFSET: usize = 520;
const STRUCTURE_INDEPENDENCE_OFFSET: usize = 524;
const COMPAT_OFFSET: usize = 528;
const PREFIX_LEN: usize = 592;

// F4SE `addressIndependence` bits (PluginAPI.h): how a plugin finds its addresses
const ADDR_SIGNATURES: u32 = 1 << 0;
const ADDR_LIBRARY_NG: u32 = 1 << 1;
const ADDR_LIBRARY_AE: u32 = 1 << 2;

// F4SE `structureIndependence` bits (PluginAPI.h): which game struct layouts a plugin tolerates
const STRUCT_NONE: u32 = 1 << 0;
const STRUCT_LAYOUT_NG: u32 = 1 << 1;
const STRUCT_LAYOUT_AE: u32 = 1 << 2;

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
    /// F4SE `addressIndependence` bitfield — how the plugin finds addresses (signatures / address-library band)
    pub address_independence: u32,
    /// F4SE `structureIndependence` bitfield — which game struct layouts the plugin tolerates
    pub structure_independence: u32,
}

impl F4sePlugin {
    /// Whether this supports `runtime`; OG-only plugins are reported separately, so only version data is checked
    pub fn supports(&self, runtime: u32) -> bool {
        self.compatible.contains(&runtime)
    }

    /// Whether the plugin declares version-independence covering `generation`
    pub fn version_independent_for(&self, generation: Generation) -> bool {
        let (addr_lib, struct_layout) = match generation {
            Generation::OldGen => return false,
            Generation::NextGen => (ADDR_LIBRARY_NG, STRUCT_LAYOUT_NG),
            Generation::Anniversary => (ADDR_LIBRARY_AE, STRUCT_LAYOUT_AE),
        };
        self.address_independence & (ADDR_SIGNATURES | addr_lib) != 0
            && self.structure_independence & (STRUCT_NONE | struct_layout) != 0
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
        address_independence: 0,
        structure_independence: 0,
    };

    if let Ok(export) = by.name(b"F4SEPlugin_Version")
        && let Some(rva) = export.symbol()
    {
        plugin.supports_ngae = true;
        if let Ok(buf) = pe.derva_slice::<u8>(rva, PREFIX_LEN) {
            let read_u32 =
                |o: usize| u32::from_le_bytes(buf[o..o + 4].try_into().expect("4 bytes"));
            if read_u32(0) == 1 {
                plugin.address_independence = read_u32(ADDRESS_INDEPENDENCE_OFFSET);
                plugin.structure_independence = read_u32(STRUCTURE_INDEPENDENCE_OFFSET);
                for i in 0..16 {
                    let v = read_u32(COMPAT_OFFSET + i * 4);
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
#[path = "tests/f4se.rs"]
mod tests;
