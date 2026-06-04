> [!NOTE]
> ## 🪟 OpenLogi for Windows — a fork
>
> This is a **Windows port of OpenLogi**, maintained by [**ChrisPrefect**](https://github.com/ChrisPrefect).
> Upstream OpenLogi is macOS-first; **this fork is focused exclusively on Windows** — all
> work here targets Windows 10 (1703+) / 11. The macOS code paths are inherited from
> upstream and carried along unchanged, but they are not the focus of this fork. Linux
> remains on the upstream roadmap.
>
> **Original project:** https://github.com/AprilNEA/OpenLogi
> **Windows downloads:** [Releases](https://github.com/ChrisPrefect/OpenLogi/releases) · branch `feature/windows-port`
>
> ### What this fork added / changed for Windows
> - **Native Windows input layer** — a `WH_MOUSE_LL` low-level mouse hook (in place of macOS's `CGEventTap`) and `SendInput`-based action synthesis (keyboard chords, media & volume keys, mouse clicks, scroll), with macOS-only actions mapped to their closest Windows shell equivalent.
> - **HID++ side-button capture** — Back/Forward are diverted over HID++ (`0x1b04` reprogrammable controls). On this hardware the standard mouse button is delivered as a momentary pulse even while held, so the HID++ event is the only way to detect a real hold.
> - **Hold-to-repeat (key repeat)** — holding a side button bound to volume or scrolling repeats the action, with a configurable on/off switch, start delay and rate (Settings → *Key repeat*).
> - **Scriptable DPI** — `OpenLogi.exe --set-dpi <N>` sets the mouse DPI from a `.cmd`/script. It is routed to the already-running instance over a localhost control channel (no second device handle), or opens the device directly when nothing is running.
> - **Self-update** — `OpenLogi.exe --update` (and *About → Check for Updates*) downloads the latest GitHub release and self-replaces.
> - **Windows tray + autostart** — notification-area icon (`Shell_NotifyIcon`), close-to-tray (window close keeps the app running), launch-at-login via the `HKCU…\Run` registry key.
> - **Self-contained build** — statically linked CRT (no VC++ redistributable needed), GUI subsystem (no console window pop-up), embedded app/window/tray icon.
> - **Robustness / self-healing** — the HID++ capture session auto-restarts if it drops; lingering device state is reset on startup.
>
> The original README continues below.

---

> [!WARNING]
> **OpenLogi is under active development** and not yet stable — features and config may still change. Give the repo a **Star** ⭐ and **Watch** 👀 it to get notified the moment a release lands.

<h4 align="right"><strong>English</strong> | <a href="README_CN.md">简体中文</a></h4>

<p align="center">
    <img src="https://assets.openlogi.org/brand/openlogi-animated.svg" width="138" alt="OpenLogi"/>
</p>

<h1 align="center">OpenLogi</h1>
<p align="center"><strong>⚡️ A native, local-first alternative to Logitech Options+, written in Rust 🦀<br/>Remap buttons, DPI, and SmartShift over HID++. No account, no telemetry.</strong></p>


<div align="center">
    <a href="https://twitter.com/AprilNEA" target="_blank">
    <img alt="twitter" src="https://img.shields.io/badge/follow-AprilNEA-green?style=social&logo=Twitter"></a>
    <a href="https://t.me/+pCVJtHAgI3hjYTkx" target="_blank">
    <img alt="telegram" src="https://img.shields.io/badge/chat-telegram-blueviolet?style=flat&logo=Telegram"></a>
    <a href="https://github.com/AprilNEA/OpenLogi/releases" target="_blank">
    <img alt="GitHub downloads" src="https://img.shields.io/github/downloads/AprilNEA/OpenLogi/total.svg?style=flat"></a>
    <a href="https://github.com/AprilNEA/OpenLogi/commits" target="_blank">
    <img alt="GitHub commit" src="https://img.shields.io/github/commit-activity/m/AprilNEA/OpenLogi?style=flat"></a>
    <img alt="Hits" src="https://hits.aprilnea.com/hits?url=https://github.com/aprilnea/openlogi">
</div>

> **Options+ ? Try OpenLogi.**

Remap buttons, drive DPI and SmartShift, and switch profiles per app — without a Logitech account, telemetry, or the official Options+ install. No cloud, plain TOML config; the only network calls are device-image fetches and an opt-in, off-by-default update check.

---

## What it is

OpenLogi talks to Logitech HID++ mice over a Logi Bolt receiver — or a
Bluetooth-direct / wired connection — without running Logi Options+. It ships
two binaries:

- **[OpenLogi GUI](crates/openlogi-gui)** — a GPUI desktop app: an interactive mouse diagram with clickable hotspots, a per-button action picker (37 built-in actions plus recorded custom shortcuts), DPI presets, a SmartShift toggle, per-application profile overlays, and a device carousel that switches between paired devices live.
- **[OpenLogi CLI](crates/openlogi-cli)** — a CLI for headless inventory (`list`) plus asset-sync and on-device diagnostic subcommands.

Everything is local: bindings live in a plain TOML file, button presses are remapped through the OS event tap, and DPI / SmartShift changes are written straight to the device over HID++.

macOS and Windows are supported today (this fork adds the Windows port); Linux
is coming soon — see [Roadmap](#roadmap).

## Roadmap

| Capability | State |
|---|---|
| Discover Bolt receivers + list paired devices (CLI + GUI) | ✅ |
| Bluetooth-direct / wired devices (no receiver) | ✅ |
| Battery percentage / charge state | ✅ (online devices) |
| Interactive GUI: carousel, mouse diagram, action picker | ✅ macOS · ✅ Windows |
| Button remapping via the OS event tap (side Back / Forward today) | ✅ macOS · ✅ Windows |
| 37-action catalog + recorded custom keyboard shortcuts | ✅ macOS¹ · ✅ Windows² |
| DPI control + presets + Cycle / Set-preset actions (HID++ `0x2201`) | ✅ macOS · ✅ Windows |
| SmartShift wheel-mode toggle (HID++ `0x2111`) | ✅ macOS · ✅ Windows |
| Per-application profile overlays (auto-switch on app focus) | ✅ macOS · ✅ Windows |
| Launch-at-login + opt-in update check | ✅ (TOML only — no settings UI yet) |
| Gesture-button per-direction bindings | 🟡 configurable; hardware capture pending |
| Middle / mode-shift / thumbwheel button capture | 🟡 configurable; hook owns side buttons only |
| macOS event tap (`CGEventTap`) / Windows event hook (`WH_MOUSE_LL`) | ✅ implemented |
| Linux event hook | ❌ stub (`Unsupported`) |
| Unifying receivers | ❌ (not yet in `hidpp 0.2`) |

¹ A few actions (e.g. the media keys) currently log their intended event rather than posting it — tracked as a follow-up.
² On Windows the macOS-only WindowServer actions map to their closest shell equivalent (Mission Control / App Exposé → Task View, Show Desktop → Win+D, Launchpad → Start, Screenshot → Win+Shift+S).

## Install

> [!IMPORTANT]
> Quit **Logi Options+** first — the two applications fight over HID++ access and only one can own a given receiver at a time.

### macOS

Download the signed, notarized `.dmg` from the [latest release](https://github.com/AprilNEA/OpenLogi/releases/latest) and drag `OpenLogi.app` to `/Applications`.

Or install via [Homebrew](https://brew.sh):

```sh
brew install --cask aprilnea/tap/openlogi
```

### Windows

Build the GUI from source (this fork) with a static C runtime so the binary runs
on any Windows 10 (1703+) / 11 machine without a Visual C++ redistributable:

```powershell
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --release -p openlogi-gui
# → target\release\openlogi-gui.exe (self-contained, ~20 MB)
```

No installer is needed — copy the `.exe` anywhere and run it. For autostart, drop
a shortcut into `shell:startup`.

To build from source, see [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Usage (CLI)

See [USAGE.md](docs/USAGE.md)

## Configuration

See [CONFIGURATION.md](docs/CONFIGURATION.md)

## Developing

See [DEVELOPMENT.md](docs/DEVELOPMENT.md)

## Acknowledgments

- [`hidpp`](https://crates.io/crates/hidpp) by [@lus](https://github.com/lus)
- [Solaar](https://github.com/pwr-Solaar/Solaar)
- [Mouser](https://github.com/TomBadash/Mouser) by Tom Badash

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

---

**Not affiliated with Logitech.** "Logitech", "MX Master", and "Options+" are trademarks of Logitech International S.A.
