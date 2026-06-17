# Overseer

A Fallout 4 mod manager, written in Rust.

Overseer follows the Mod Organizer 2 model — explicit, non-destructive mod and load-order
management — and builds in the pieces MO2 leaves to plugins: root-folder deployment (script
extenders, ReShade, ENB), in-app plugin grouping, native tool integration (xEdit, Complex
Sorter, BethINI, LOOT), and setup diagnostics.

## Status

Early development. This is the **Phase 0 spike**: a working hardlink deployment engine that
proves the core approach.

## Workspace

- `overseer-core` — UI-agnostic domain logic (the deployment engine lives here).
- `overseer-cli` — command-line front end.

A desktop app (Tauri) will sit on top of `overseer-core` later, sharing the same core.

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
