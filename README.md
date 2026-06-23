# Overseer

A Fallout 4 mod manager, written in Rust.

Overseer follows the Mod Organizer 2 model: explicit, non-destructive mod and load-order
management. It builds in the pieces MO2 leaves to plugins: root-folder deployment (script
extenders, ReShade, ENB), in-app plugin grouping, native tool integration (xEdit, Complex
Sorter, BethINI) and setup diagnostics.

## Status

Under active development. The UI-agnostic core (`overseer-core`) and the CLI (`overseer-cli`)
already support a full Fallout 4 workflow: create an instance, install mods from archives,
manage a profile's mod list and plugin load order, and deploy or purge into the game's `Data/`
directory. Deploying writes the real `Plugins.txt` through `libloadorder` and restores it on
purge. A setup health checker (`overseer doctor`) runs the diagnostics from the command line, and
the terminal UI (`overseer-tui`) presents the mods and plugins and surfaces the same checks in an
in-app panel. Both are in active development on top of the same core.

Deployment is **non-destructive and crash-safe**. Any pre-existing files a mod would overwrite
are backed up first and restored exactly on purge. The apply is journaled as a transaction, so an
interrupted run is rolled back on the next command instead of leaving `Data/` half-written.

## Workspace

- `overseer-core`: UI-agnostic domain logic. Modules: `deploy` (the hardlink engine behind a
  `Deployer` trait), `install` (archive extraction and staging), `instance` (instances, profiles,
  mod lists), `plugins` (metadata, discovery, load order, the real `Plugins.txt`), `apply` (the
  orchestrator that turns a profile into a live deployment), plus `game`, `launch`, and `settings`.
- `overseer-frontend`: shared support for the front ends (file logging, theming, path helpers).
- `overseer-cli`: command-line front end (scriptable and one-shot).
- `overseer-diagnostics`: read-only setup health checks, surfaced by `overseer doctor`.
- `overseer-tui`: the interactive terminal UI, built on `ratatui`, with a mods and plugins view
  and an in-app diagnostics panel.

The TUI is the primary interactive front end. A desktop GUI (Tauri) is a possible later addition
over the same core.

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

# run the setup health checks
overseer doctor --instance <instance-dir>
```

Run the tests with:

```
cargo test
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
