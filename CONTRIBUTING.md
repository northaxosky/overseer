# Contributing to Overseer

Thanks for your interest! **Overseer** is a Fallout 4 / Bethesda mod manager written in
Rust, modeled on Mod Organizer 2. It is licensed **GPL-3.0-or-later**.

## Getting started

- Install Rust via [rustup](https://rustup.rs). The pinned toolchain — nightly, plus
  `rustfmt` and `clippy` — is declared in [`rust-toolchain.toml`](rust-toolchain.toml) and
  installed automatically the first time you run `cargo`.
- Build it: `cargo build`
- Try it: `cargo run -p overseer-cli -- demo` (a self-contained proof of the deployment engine)

## The gate

All four commands must pass before a change is considered done. CI runs the same gate on
every push and pull request.

```sh
cargo build
cargo test
cargo clippy --all-targets   # warnings are treated as failures in CI
cargo fmt --check
```

## Git hooks (recommended)

The repository ships hooks that run these checks for you. Enable them once per clone:

```sh
git config core.hooksPath .githooks
```

- **pre-commit** — `cargo fmt --all --check` (fast; catches the most common CI failure).
- **pre-push** — the full gate (fmt + clippy + build + test), so you never push a red build.

## Workspace layout

| Crate | Role |
|---|---|
| `overseer-core` | UI-agnostic domain logic. No UI/CLI dependencies. |
| `overseer-frontend` | Shared front-end support (logging, theming). |
| `overseer-cli` | The `overseer` command-line binary. |
| `overseer-tui` | The terminal UI — the primary interactive front end. |

## Conventions

- **Surgical, well-scoped changes.** Change exactly what the task needs; match the
  surrounding style; prefer the simplest design that fully solves the problem.
- **`overseer-core` stays UI-agnostic** — no `clap` / `ratatui` / `tauri` / `indicatif`.
  Report progress via the `ProgressSink` trait. Platform/VFS specifics go behind a trait
  (e.g. `Deployer`) so the core builds and tests on any OS.
- **Errors:** `thiserror` in library crates (attach context such as the offending path);
  `anyhow` with `.context(...)` in binaries. Avoid `unwrap()`/`expect()` outside tests.
- **Paths:** use `camino::Utf8Path` / `Utf8PathBuf` at API and serde boundaries.
- **Tests:** every non-trivial subsystem gets tests; use `tempfile` and fixtures — never
  depend on a real game install.
- **Vocabulary:** mods are *enabled* / *disabled*; plugins are *active* / *inactive*.

## Commits & pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org): `type(scope): summary`
  (e.g. `feat(tui): …`, `fix(core): …`, `ci: …`). Common types: `feat`, `fix`, `refactor`,
  `docs`, `test`, `ci`, `chore`.
- Write clear messages that explain *what* changed and *why*.
- Keep each pull request focused on one logical change where practical, and make sure the
  gate passes locally (the pre-push hook helps).

## License

By contributing, you agree that your contributions are licensed under **GPL-3.0-or-later**.
