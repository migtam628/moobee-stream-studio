# MooBee Stream Studio v0.3.1-alpha.1 — Scene Workspace

**Experimental standalone prerelease, not yet a live video broadcaster.** No OBS or Streamlabs installation or integration is required.

## New in 0.3.1

- Rename, duplicate and delete scenes, with preview/program selection preserved when scenes are renamed.
- Remove individual sources, while retaining source visibility controls.
- Choose a specific native display on Windows or macOS rather than always adding the first detected monitor.
- Versioned JSON scene collection export/import in the Settings panel, with validation and confirmation before replacement.
- Local Node web/API edition saves scene collections atomically in a private data folder; Docker Compose persists its scene files in `./data`.
- Added regression tests for import rejection, scene transitions, source IDs, backup/export and deletions.

## Retained from 0.3.0

- Tauri 2 desktop and native display screenshot capture on macOS/Windows (requires system screen recording permission).
- Scene control state, independent local API, preview/program selections and local alert previews.
- GitHub Actions installers for macOS Intel/Apple Silicon, Windows, Linux, Android test APK and self-hosted web archive.

## Still unavailable

- Recording files, GPU video compositing, realtime full-framerate capture, audio capture/mixing, hardware encoding and broadcasting to streaming destinations. **Record / Go Live remain disabled.**
- Android and Linux screen capture, public API authorization, production signatures/notarization, live chat/platform integrations.

## Installation and caution

- macOS: choose arm64 (Apple Silicon) or x64 (Intel) DMG, unsigned experimental build.
- Windows: x64 setup EXE, unsigned experimental build.
- Linux / Android: build artifacts are experimental UI/controller packages without native display capture.
- Web: local API and browser UI. Never expose the loopback control API directly to the public internet.

An installer building successfully does not establish screen recording permission, camera/microphone functionality, or streaming capability. Validate local scenes and display snapshots on a test computer. Report issues in [GitHub](https://github.com/migtam628/moobee-stream-studio/issues).
