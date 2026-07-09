//! The official Creation Club plugin allow-list, loaded from a bundled catalog

use std::collections::HashSet;
use std::sync::LazyLock;

/// The bundled catalog: one CC plugin filename per line, `#` comments and blank lines allowed
const CATALOG_TEXT: &str = include_str!("cc_catalog.txt");

/// The catalog parsed once into lowercased full filenames for exact case-insensitive matching
static CATALOG: LazyLock<HashSet<String>> = LazyLock::new(|| parse(CATALOG_TEXT));

/// Whether `plugin_file` is an official Creation Club plugin
pub fn is_cc(plugin_file: &str) -> bool {
    CATALOG.contains(&plugin_file.to_ascii_lowercase())
}

/// Parse catalog text: drop blank lines and `#` comments
fn parse(text: &str) -> HashSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
#[path = "tests/catalog.rs"]
mod tests;
