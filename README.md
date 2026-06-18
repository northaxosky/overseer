# Overseer

A Fallout 4 mod manager, written in Rust.

Overseer follows the Mod Organizer 2 model: explicit, non-destructive mod and load-order
management. It builds in the pieces MO2 leaves to plugins: root-folder deployment (script
extenders, ReShade, ENB), in-app plugin grouping, native tool integration (xEdit, Complex
Sorter, BethINI) and setup diagnostics.

## Status

Early development.

## Workspace

- `overseer-core`: UI-agnostic domain logic (the deployment engine lives here).
- `overseer-cli`: command-line front end (scriptable / one-shot).

The primary interactive front end will be a **TUI** (`overseer-tui`, built on `ratatui`) over the
same core; a desktop GUI (Tauri) is an optional later addition if broader adoption calls for it.

## Try it

```
cargo run -p overseer-cli -- demo
```

This stages two conflicting mods in a temporary directory, hard-links them into a target
`Data` folder in priority order, proves the deployed files are links (not copies), then
purges everything back to a clean state.

Run the tests with:

```
cargo test
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
