# MooBee Stream Studio v0.3.8-alpha.2 — GitHub Release Center and Mac MP4 Hardening

**Experimental standalone alpha. Not production-ready.** All installers and the self-hosted web bundle are distributed through GitHub Releases. The main **Record** and **Go Live** controls remain disabled.

## Changes since v0.3.8-alpha.1

- Hardened the native macOS H.264 MP4 recording prototype: checks ISO BMFF top-level boxes before publishing a clip, rejects truncated/missing `ftyp`, `moov` and `mdat` data, and includes regression tests.
- Fixed Swift Concurrency runtime lookup by explicitly linking the macOS system Swift runtime path into the Rust/Tauri application.
- Added native macOS Intel and Apple Silicon linking checks and Apple Silicon app-bundle/privacy metadata checks in GitHub Actions.
- Added an offline FFprobe/FFmpeg test script (`scripts/verify-native-mp4.mjs`) that reports H.264 codec, dimensions, duration, optional audio-track presence, and full-file decode. **It does not verify actual playback or A/V synchronization.**
- Added **Settings → GitHub Releases & Updates** for read-only checks of the official published GitHub releases and manual downloads.
- GitHub Actions now verify release versions across all workspaces, refuse existing published release tags, require all supported platform packages, and publish `RELEASE-MANIFEST.json` plus `SHA256SUMS.txt`.
- Weekly Dependabot PRs are configured for npm, Rust/Cargo, and GitHub Actions dependencies. Changes are never auto-merged.
- Added `docs/GITHUB_RELEASES.md` to describe the full GitHub release/update lifecycle.

## Experimental capabilities retained

- **macOS 15+**: 3–15 second native H.264 MP4 screen test capture, with optional system/microphone audio requests. Correct capture and audio tracks still require real-device testing.
- **macOS/Windows**: short silent 640×360 10fps MJPEG AVI test clips, GIFs, screenshots and continuous frame diagnostics.
- **Web**: consent-based browser-local screen/webcam recorder where the browser allows it.
- **Core UI**: scene management, local API, overlays, clip library and development preview.

## Installers and platform limitations

- macOS ARM64 DMG, macOS Intel DMG, Windows x64 setup EXE, Linux x64 DEB, Android ARM64 debug APK, and self-hosted web/API bundle are built by GitHub Actions.
- Desktop installers are **unsigned/unnotarized experimental alpha packages**. Android is **debug-signed** and has no native broadcasting engine. Linux native screen capture is unsupported.
- GitHub Release downloads are the current update path. Secure automatic *installation* is **not enabled**; it needs Tauri updater signing keys and signed manifests, code-signing/notarization credentials, and hardware upgrade acceptance.

## Still blocking production

Native Windows H.264/AAC MP4, sustained synchronized desktop audio/video recording, actual GPU scene composition and webcam overlays, RTMP/RTMPS streaming, credential management, security audit, code signing, and physical Mac/Windows hardware tests remain open. A CI pass means the code builds and checks pass, **not** that the installed app broadcasts or records reliably.

See the tracked [Mac hardware test issue](https://github.com/migtam628/moobee-stream-studio/issues/14), [MP4 implementation issue](https://github.com/migtam628/moobee-stream-studio/issues/15), and [production readiness checklist](https://github.com/migtam628/moobee-stream-studio/blob/release/v0.3.8-alpha.2/docs/PRODUCTION_READINESS.md).
