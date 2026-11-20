# MooBee Live — Tauri 2 desktop edition

## What changed in alpha 0.2

The same React/Vite dashboard now has two execution backends:

| Edition | OBS bridge | Alert overlay | Runtime dependencies |
|---|---|---|---|
| Tauri desktop | `obs-websocket-js` in the native WebView | Rust HTTP + SSE on `127.0.0.1:8788` | The compiled desktop app and a separate OBS Studio installation |
| Web companion | Local Node API with `obs-websocket-js` | React browser-source page via local API | Node.js and OBS Studio |

The Tauri edition is designed to work without a Node runtime after installation. It does **not yet bundle OBS's video capture, compositing or encoding engine**. Users still install OBS Studio separately and enable its authenticated WebSocket server.

## Developing on macOS (Apple Silicon or Intel)

1. Install Xcode Command Line Tools, the Rust toolchain, Node.js 22+, npm and OBS Studio 33.x.
2. In the project root, run `npm install`.
3. Run `npm run desktop:dev`.
4. Open **Settings** in MooBee Live, enter the local OBS WebSocket password, and connect.
5. Test scene switching, mute/volume, recording, overlay alerts and streaming (to a private test destination).

Create a distributable unsigned app with `npm run desktop:build`. For broad public distribution, configure Apple Developer signing and notarization. The macOS CI runner builds both Apple Silicon and Intel packages.

## Developing on Windows

Follow the current [official Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for Rust, the Microsoft C++ build tools and WebView2, then:

```powershell
npm install
npm run desktop:dev
```

The GitHub Actions workflow produces an unsigned NSIS installer. Windows code signing is a later release task.

## Linux

Install the Tauri Linux WebKitGTK 4.1 dependencies, Rust and Node.js. Run `npm run desktop:dev`. CI builds a `.deb` package using Ubuntu 24.04.

## Test acceptance checklist (requires an actual OBS instance)

- [ ] Tauri app opens and shows the MooBee Live dashboard without running `apps/api/server.mjs`
- [ ] Incorrect OBS password gives an actionable error; no stream starts
- [ ] Correct password connects to OBS on `127.0.0.1:4455`
- [ ] Scene list, sources and audio inputs match OBS
- [ ] Scene switching changes OBS program output
- [ ] Source show/hide, audio mute and volume commands work
- [ ] Record button produces a file in OBS's configured recording location
- [ ] Go Live and End Stream work against a **private test destination**
- [ ] Browser Source `http://127.0.0.1:8788/overlay/alerts` shows an animated test alert
- [ ] Restarting the desktop app does not automatically start broadcasting
- [ ] Port 8788 conflicts are handled before public release
- [ ] Window layout, screenshots, mobile responsive states and native titlebar inspected
- [ ] Signed/notarized release pipeline verified before distribution

## Development status

- Verified in current environment: TypeScript frontend compiles and bundles, JavaScript backend tests pass, Tauri JS dependency installed, icons generated.
- Not yet verified: Rust compilation, Tauri package installation, native UI screenshot or real OBS hardware interaction. The environment lacks `cargo`, `rustc`, and WebKitGTK build dependencies.
- Security: OBS credentials are transient in webview memory. OBS addresses are restricted to loopback by default. The overlay service binds only to loopback. Tauri command input is bounded and sanitized. External OAuth, session auth and remote access are not present.

## Next architecture milestone

Evaluate an optional OBS plugin or libobs integration for a genuinely all-in-one desktop broadcast engine. That is separate from Tauri and must include OBS GPL compliance, packaging, codec licensing, native macOS/Windows capture permissions and upstream OBS update testing.
