//! Console presentation: headings, success lines, list items, check marks, progress sink.

use std::fmt::Display;

use overseer_core::deploy::{ProgressEvent, ProgressSink};
use owo_colors::{OwoColorize, Stream::Stdout, Style};

/// A bold section heading.
pub fn heading(msg: impl Display) {
    let style = Style::new().bold();
    println!("{}", msg.if_supports_color(Stdout, |t| t.style(style)));
}

/// A green success line prefixed with a check mark.
pub fn success(msg: impl Display) {
    let style = Style::new().green().bold();
    println!(
        "{} {msg}",
        "✓".if_supports_color(Stdout, |t| t.style(style))
    );
}

/// A numbered, checkbox-prefixed list item, coloured green when `on` and dimmed when off.
pub fn list_item(index: usize, on: bool, text: impl Display, suffix: &str) {
    let style = if on {
        Style::new().green()
    } else {
        Style::new().dimmed()
    };
    let mark = if on { "[x]" } else { "[ ]" };
    let line = format!("{index:>3}. {mark} {text}{suffix}");
    println!("{}", line.if_supports_color(Stdout, |t| t.style(style)));
}

/// Print a labelled check result with a green PASS or red FAIL; returns `ok` for chaining.
pub fn check(label: &str, ok: bool) -> bool {
    let style = if ok {
        Style::new().green().bold()
    } else {
        Style::new().red().bold()
    };
    let mark = if ok { "PASS" } else { "FAIL" };
    println!(
        "  {label:<54} [{}]",
        mark.if_supports_color(Stdout, |t| t.style(style))
    );
    ok
}

/// Prints CLI-friendly progress lines for deploy/undeploy.
pub struct CliProgress;

impl ProgressSink for CliProgress {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Started { total } => {
                let style = Style::new().dimmed();
                println!(
                    "  {}",
                    format!("({total} files)").if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Deployed { relative, .. } => {
                let style = Style::new().green().bold();
                println!(
                    "  {} {relative}",
                    "+".if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Removed { relative, .. } => {
                let style = Style::new().yellow().bold();
                println!(
                    "  {} {relative}",
                    "-".if_supports_color(Stdout, |t| t.style(style))
                );
            }
            ProgressEvent::Finished => {
                let style = Style::new().green().bold();
                println!(
                    "  {}",
                    "✓ done".if_supports_color(Stdout, |t| t.style(style))
                );
            }
        }
    }
}
