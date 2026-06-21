# Overseer

A Fallout 4 mod manager, written in Rust.

Overseer follows the Mod Organizer 2 model: explicit, non-destructive mod and load-order
management. It builds in the pieces MO2 leaves to plugins: root-folder deployment (script
extenders, ReShade, ENB), in-app plugin grouping, native tool integration (xEdit, Complex
Sorter, BethINI) and setup diagnostics.

## Status

Under active development. The UI-agnostic core (`overseer-core`) and the CLI (`overseer-cli`)
already support a full Fallout 4 workflow: create an instance, install mods from archives,
manage a profile's mod list and plugin load order, and deploy/purge into the game's `Data/`
directory — writing the real `Plugins.txt` via `libloadorder` and restoring it on purge.

Deployment is **non-destructive and crash-safe**: any pre-existing files a mod would overwrite
are backed up first and restored exactly on purge, and the apply is journaled as a transaction —
so an interrupted run is rolled back on the next command instead of leaving `Data/` half-written.

## Workspace

- `overseer-core`: UI-agnostic domain logic. Modules: `deploy` (the hardlink engine behind a
  `Deployer` trait), `install` (archive extraction + staging), `instance` (instances, profiles,
  mod lists), `plugins` (metadata, discovery, load order, real `Plugins.txt`), and `apply` (the
  orchestrator that turns a profile into a live deployment).
- `overseer-cli`: command-line front end (scriptable / one-shot).

The primary interactive front end will be a **TUI** (`overseer-tui`, built on `ratatui`) over the
same core; a desktop GUI (Tauri) is an optional later addition if broader adoption calls for it.

## Try it

A self-contained proof of the deployment engine (no game install required):

```
cargo run -p overseer-cli -- demo
```

This stages two conflicting mods in a temporary directory, hard-links them into a target
`Data` folder in priority order, proves the deployed files are links (not copies), then
purges everything back to a clean state.

### Manage a Fallout 4 install

```
# create an instance pointing at your game
overseer instance init --path <instance-dir> --game-dir "<FO4 install>"

# install a mod archive, then inspect the mod list and plugin order
overseer install <mod.7z> --instance <instance-dir>
overseer mod list    --instance <instance-dir>
overseer plugin list --instance <instance-dir>

# deploy into the game's Data/ (writes the real Plugins.txt), check status, then undo
overseer deploy --instance <instance-dir>
overseer status --instance <instance-dir>
overseer purge  --instance <instance-dir>
```

Run the tests with:

```
cargo test
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
