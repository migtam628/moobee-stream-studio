# MooBee Stream Studio v0.3.0-alpha.1 — Standalone Capture Foundation

**Experimental prerelease, not a functional live-streaming application yet.** This is the first new build line that no longer depends on OBS Studio, OBS WebSocket or Streamlabs.

## Included
- Tauri 2 native Windows and macOS desktop builds with a first-party Rust capture command for display enumeration and single-frame screenshot previews (hardware access needs OS permissions).
- In-house scene controller with scene creation, preview/program selection, Studio Mode transition state, and display-source visibility.
- Local test-alert overlay and event endpoints.
- Independent local-only Node API with scene/alert endpoints, plus a self-hosted web control-panel distribution.
- Linux and Android test packages compile but do **not** provide native display capture yet.
- GitHub Actions builds for macOS ARM64 and Intel, Windows x64, Linux x64, Android ARM64 and web.

## Not yet available
- Actual live video encoding and broadcast; **Go Live is disabled**.
- Playable video recording, full frame rate capture, synchronized audio, live GPU compositing and multistreaming.
- Live platform event integrations, production user authentication, signed/notarized production installers.
- Android and Linux native capture.

## Installing
- macOS: download the matching `macos-arm64-*.dmg` for Apple Silicon or `macos-x64-*.dmg` for Intel. Gatekeeper may warn because the build is unsigned.
- Windows: download `windows-x64-*-setup.exe`; Windows SmartScreen may warn because this alpha is unsigned.
- Linux: experimental `.deb`.
- Android: experimental debug-signed ARM64 `.apk`; no video capture yet.
- Web: local web/API tarball, **not a browser-based capture engine**. Do not expose the local control API directly to the public internet.

The native Mac/Windows build compilation has been verified by GitHub Actions. Real on-device capture permissions, source previews and all media-pipeline functionality still require device testing. Feedback and issues belong in the [repository](https://github.com/migtam628/moobee-stream-studio/issues).
