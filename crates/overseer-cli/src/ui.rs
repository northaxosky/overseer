//! Console presentation: styling roles, CLI theme, colour choice, and output helpers

use std::fmt::Display;

use overseer_core::deploy::{ProgressEvent, ProgressSink};
pub use overseer_frontend::style::{Color, Role};
use owo_colors::{OwoColorize, Stream::Stdout, Style};

fn role_style(role: Role) -> Style {
    let p = role.palette();
    let mut style = match p.color {
        Some(Color::Green) => Style::new().green(),
        Some(Color::Red) => Style::new().red(),
        Some(Color::Yellow) => Style::new().yellow(),
        None => Style::new(),
    };
    if p.bold {
        style = style.bold();
    }
    if p.dim {
        style = style.dimmed();
    }
    style
}

/// Paint `value` in `role`, honouring the active colour choice (`--color` / `NO_COLOR`)
pub fn styled(role: Role, value: impl Display) -> String {
    format!(
        "{}",
        value.if_supports_color(Stdout, |t| t.style(role_style(role)))
    )
}

/// When to emit ANSI colour
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum ColorChoice {
    /// Colour when stdout is a terminal and `NO_COLOR` is unset
    #[default]
    Auto,
    /// Always colour
    Always,
    /// Never colour
    Never,
}

/// Apply a [`ColorChoice`] globally for the rest of the process
pub fn apply_color_choice(choice: ColorChoice) {
    match choice {
        ColorChoice::Auto => owo_colors::unset_override(),
        ColorChoice::Always => owo_colors::set_override(true),
        ColorChoice::Never => owo_colors::set_override(false),
    }
}

/// A bold section heading
pub fn heading(msg: impl Display) {
    println!("{}", styled(Role::Heading, msg));
}

/// A success line prefixed with a green check mark
pub fn success(msg: impl Display) {
    println!("{} {msg}", styled(Role::Success, "✓"));
}

/// A numbered, checkbox-prefixed list item: success-coloured when `on`, muted when off
pub fn list_item(index: usize, on: bool, text: impl Display, suffix: &str) {
    let role = if on { Role::Success } else { Role::Muted };
    let mark = if on { "[x]" } else { "[ ]" };
    println!(
        "{}",
        styled(role, format!("{index:>3}. {mark} {text}{suffix}"))
    );
}

/// Print a labelled check result with a green PASS or red FAIL
pub fn check(label: &str, ok: bool) {
    let (role, mark) = if ok {
        (Role::Success, "PASS")
    } else {
        (Role::Failure, "FAIL")
    };
    println!("  {label:<54} [{}]", styled(role, mark));
}

/// Prints CLI-friendly progress lines for deploy/undeploy
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
