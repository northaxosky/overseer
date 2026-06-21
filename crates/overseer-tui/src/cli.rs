//! Command-line argument parsing for the TUI.

use anyhow::{Context, Result, bail};
use camino::Utf8PathBuf;

/// Parse `overseer-tui <instance-dir> [--profile NAME]`.
pub(crate) fn parse_args() -> Result<(Option<Utf8PathBuf>, String)> {
    let mut instance: Option<Utf8PathBuf> = None;
    let mut profile = "Default".to_owned();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => profile = args.next().context("--profile needs a value")?,
            _ if instance.is_none() => instance = Some(Utf8PathBuf::from(arg)),
            _ => bail!("unexpected argument: {arg}"),
        }
    }
    Ok((instance, profile))
}
