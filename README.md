<div align="center">

<img src="site/assets/app-icon.png" alt="Overseer" width="120" height="120" />

<h1>Overseer</h1>

<p>
  <b>A Fallout 4 mod manager, in Rust.</b><br/>
  The Mod Organizer 2 model, done right.
</p>

<p>
  <a href="https://github.com/northaxosky/overseer/actions/workflows/ci.yml">
    <img alt="CI" src="https://img.shields.io/github/actions/workflow/status/northaxosky/overseer/ci.yml?branch=main&style=for-the-badge&labelColor=0b0b0b&label=CI" />
  </a>
  <img alt="Rust 2024" src="https://img.shields.io/badge/Rust-2024-00ee00?style=for-the-badge&logo=rust&logoColor=00ee00&labelColor=0b0b0b" />
  <img alt="Platform: Windows" src="https://img.shields.io/badge/platform-Windows-00ee00?style=for-the-badge&logo=windows&logoColor=00ee00&labelColor=0b0b0b" />
  <a href="LICENSE">
    <img alt="License: GPL-3.0-or-later" src="https://img.shields.io/badge/license-GPL--3.0--or--later-00ee00?style=for-the-badge&labelColor=0b0b0b" />
  </a>
  <img alt="Status: in development" src="https://img.shields.io/badge/status-in_development-00ee00?style=for-the-badge&labelColor=0b0b0b" />
</p>

<p>
  <a href="#features">Features</a> &nbsp;&middot;&nbsp;
  <a href="#quick-start">Quick start</a> &nbsp;&middot;&nbsp;
  <a href="#architecture">Architecture</a> &nbsp;&middot;&nbsp;
  <a href="https://northaxosky.github.io/overseer/">Website</a>
</p>

</div>

<!--
<div align="center">
  <img src=".github/assets/overseer-tui.gif" alt="Overseer terminal UI" width="820" />
</div>
-->

---

## Overview

Overseer follows the **Mod Organizer 2** model of explicit, non-destructive mod and load-order
management, and builds in the pieces MO2 leaves to third-party plugins: root-folder deployment
(script extenders, ReShade, ENB), in-app plugin grouping, native tool integration (xEdit, Complex
Sorter, BethINI), and setup diagnostics. One UI-agnostic Rust core drives every front end.

Deployment is **non-destructive and crash-safe**. Any pre-existing files a mod would overwrite are
backed up first and restored exactly on purge. The apply is journaled as a transaction, so an
interrupted run is rolled back on the next command instead of leaving `Data/` half-written.

## Features

| Feature | What it does |
| :-- | :-- |
| **Crash-safe deployment** | Non-destructive hardlink deploy, journaled as a transaction. Overwritten files are backed up first and restored exactly on purge; an interrupted run rolls back cleanly. |
| **Real load order** | Writes the actual `Plugins.txt` through `libloadorder` on deploy and restores it on purge, so the game and external tools see exactly what you set. |
| **Root-folder deployment** | Script extenders, ReShade and ENB files that live beside the executable are handled first-class, not just what lands in `Data/`. |
| **In-app plugin grouping** | Organise your load order into named groups inside the app, with no external plugin needed to keep a large setup readable. |
| **Setup diagnostics** | `overseer doctor` runs read-only health checks on your install and surfaces the same results in the terminal UI. |
| **TUI-first, scriptable CLI** | A `ratatui` terminal UI for daily driving, plus a one-shot CLI for automation, both over the same core. |

## Quick start

Launch the terminal UI:

```sh
cargo run -p overseer-tui
```

Or drive a full Fallout 4 workflow from the CLI:

```sh
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

## Status

Under active development. The UI-agnostic core (`overseer-core`) and the CLI (`overseer-cli`)
already support a full Fallout 4 workflow: create an instance, install mods from archives, manage a
profile's mod list and plugin load order, and deploy or purge into the game's `Data/` directory.
The setup health checker (`overseer doctor`) runs the diagnostics from the command line, and the
terminal UI (`overseer-tui`) presents the mods and plugins and surfaces the same checks in an
in-app panel. The TUI is the primary interactive front end; a desktop GUI (Tauri) is a possible
later addition over the same core.

## Architecture

<details>
<summary>Workspace layout (crates)</summary>

<br/>

- **`overseer-core`**: UI-agnostic domain logic. Modules: `deploy` (the hardlink engine behind a
  `Deployer` trait), `install` (archive extraction and staging), `instance` (instances, profiles,
  mod lists), `plugins` (metadata, discovery, load order, the real `Plugins.txt`), `apply` (the
  orchestrator that turns a profile into a live deployment), `patch` (BA2 header patching and
  pure-Rust VCDIFF edition conversion), plus `game`, `launch`, and `settings`.
- **`overseer-frontend`**: shared support for the front ends (file logging, theming, path helpers).
- **`overseer-cli`**: command-line front end (scriptable and one-shot).
- **`overseer-diagnostics`**: read-only setup health checks, surfaced by `overseer doctor`.
- **`overseer-tui`**: the interactive terminal UI, built on `ratatui`, with a mods and plugins view
  and an in-app diagnostics panel.

</details>

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and the gate every
change must pass:

```sh
cargo build
cargo test
cargo clippy --all-targets   # warnings are treated as failures in CI
cargo fmt --check
```

## License

[GPL-3.0-or-later](LICENSE).
