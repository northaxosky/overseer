//! Console presentation: semantic styling roles, the CLI theme, the colour
//! choice, and the output helpers (headings, success lines, list items, checks,
//! progress sink).
//!
//! Style by *meaning*: a call site picks a [`Role`] and the theme maps it to a
//! concrete style — no raw colours at call sites. The same role set will map to
//! ratatui styles in the TUI, so the palette is defined once. See
//! `.agents/conventions/output-style.md`.

use std::fmt::Display;

use overseer_core::deploy::{ProgressEvent, ProgressSink};
use owo_colors::{OwoColorize, Stream::Stdout, Style};

/// A semantic styling role. Front ends map roles to concrete styles; the CLI
/// theme lives in [`Role::style`].
#[derive(Debug, Clone, Copy)]
pub enum Role {
    /// A section heading.
    Heading,
    /// A completed action, good result, or enabled item.
    Success,
    /// An error or a failed check.
    Failure,
    /// A caution: something missing or removed the user should notice.
    Warning,
    /// Secondary information: counts, hints, disabled items.
    Muted,
    /// A deployed / added entry.
    Added,
    /// A removed entry.
    Removed,
}

impl Role {
    /// The CLI (owo-colors) style for this role — the one place colours are chosen.
    fn style(self) -> Style {
        match self {
            Role::Heading => Style::new().bold(),
            Role::Success | Role::Added => Style::new().green().bold(),
            Role::Failure => Style::new().red().bold(),
            Role::Warning | Role::Removed => Style::new().yellow().bold(),
            Role::Muted => Style::new().dimmed(),
        }
    }
}

/// Paint `value` in `role`, honouring the active colour choice (`--color` / `NO_COLOR`).
pub fn styled(role: Role, value: impl Display) -> String {
    format!(
        "{}",
        value.if_supports_color(Stdout, |t| t.style(role.style()))
    )
}

/// When to emit ANSI colour.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum ColorChoice {
    /// Colour when stdout is a terminal and `NO_COLOR` is unset.
    #[default]
    Auto,
    /// Always colour.
    Always,
    /// Never colour.
    Never,
}

/// Apply a [`ColorChoice`] globally for the rest of the process.
pub fn apply_color_choice(choice: ColorChoice) {
    match choice {
        ColorChoice::Auto => owo_colors::unset_override(),
        ColorChoice::Always => owo_colors::set_override(true),
        ColorChoice::Never => owo_colors::set_override(false),
    }
}

/// A bold section heading.
pub fn heading(msg: impl Display) {
    println!("{}", styled(Role::Heading, msg));
}

/// A success line prefixed with a green check mark.
pub fn success(msg: impl Display) {
    println!("{} {msg}", styled(Role::Success, "✓"));
}

/// A numbered, checkbox-prefixed list item: success-coloured when `on`, muted when off.
pub fn list_item(index: usize, on: bool, text: impl Display, suffix: &str) {
    let role = if on { Role::Success } else { Role::Muted };
    let mark = if on { "[x]" } else { "[ ]" };
    println!(
        "{}",
        styled(role, format!("{index:>3}. {mark} {text}{suffix}"))
    );
}

/// Print a labelled check result with a green PASS or red FAIL; returns `ok` for chaining.
pub fn check(label: &str, ok: bool) -> bool {
    let (role, mark) = if ok {
        (Role::Success, "PASS")
    } else {
        (Role::Failure, "FAIL")
    };
    println!("  {label:<54} [{}]", styled(role, mark));
    ok
}

/// Prints CLI-friendly progress lines for deploy/undeploy.
pub struct CliProgress;

impl ProgressSink for CliProgress {
    fn on_event(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::Started { total } => {
                println!("  {}", styled(Role::Muted, format!("({total} files)")));
            }
            ProgressEvent::Deployed { relative, .. } => {
                println!("  {} {relative}", styled(Role::Added, "+"));
            }
            ProgressEvent::Removed { relative, .. } => {
                println!("  {} {relative}", styled(Role::Removed, "-"));
            }
            ProgressEvent::Finished => {
                println!("  {}", styled(Role::Success, "✓ done"));
            }
        }
    }
}
