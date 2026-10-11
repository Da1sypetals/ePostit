# Build

Rust + GPUI macOS menu bar note app. Rust toolchain must be stable 1.99 or newer: the pinned `gpui-pre` crates use `std::hint::cold_path`, which older toolchains reject.

## Commands

- `cargo run` builds and runs the app directly from `target/debug/epostit`, without installing. The process registers the global `Cmd+N` hotkey and stays resident in the menu bar.
- `cargo test` runs the full suite headlessly. Tests drive real UI through `TestAppContext` and `VisualTestContext` and never inject keyboard or mouse events into the user's session.
- `./install.sh` builds release, packages, installs, and launches. Run it after every feature change.

## Install script

`install.sh` does, in order:

1. `cargo build --release`.
2. Assembles `target/ePostit.app` from `packaging/Info.plist`, `assets/AppIcon.icns` (copied to `Contents/Resources/`), and the release binary, which is copied to `Contents/MacOS/ePostit`.
3. Kills any running `epostit` process.
4. Replaces `/Applications/ePostit.app` with the new bundle.
5. Launches the installed app via `open`.

## Icons

`assets/` holds the icon sources and outputs. `python3 scripts/make_icons.py` regenerates all of them from `assets/icon-src.png`: the 1024px app icon (squircle plate, artwork at 80% per Apple's sizing rules), the `epostit.iconset` + `AppIcon.icns`, and `tray-icon.rgba`, a 44x44 binary mask embedded in the binary for the menu bar template icon.

The bundle declares `LSUIElement`, so the app registers as a menu bar utility with LaunchServices. The in-code accessory activation policy remains as a fallback for `cargo run`.

## Data and configuration

- Notes live in `~/Library/Application Support/epostit/notes/<ULID>.md`. Titles are stored in YAML front matter, so duplicate titles are legal. Deleted notes move to `notes/.trash/`.
- Configuration lives in `~/Library/Application Support/epostit/config.toml`. Editable keys: `hotkey`, `notes_dir`, `hide_on_blur`, `sidebar_width`, `sidebar_collapsed`, `sort`.
