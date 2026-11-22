# MooBee Stream Studio v0.3.8-alpha.3 — macOS Compatibility Fix

**Experimental alpha, not production-ready.** This release fixes the macOS system-version requirement that prevented users on macOS 13/14 from opening the application.

## Fixes since alpha.2

- Lowered the bundled macOS minimum from **15.0 to 13.0** (Ventura), matching the Swift compilation deployment target on Intel and Apple Silicon.
- Kept the experimental **H.264 MP4 capture feature restricted to macOS 15+** through a native Swift availability check. On macOS 13/14, MooBee shows the MP4 test as unavailable while leaving scenes, screenshot capture, silent GIF/AVI test recordings and release information accessible.
- Expanded macOS CI to validate the app bundle's minimum-version declaration and linked Mach-O deployment target, in addition to compilation and MP4 structural tests.
- Platform installer and release-manifest workflows remain in GitHub Actions, with SHA-256 asset hashes and prerelease safeguards.

## Important limitations

- **macOS 13/14 compatibility requires real-device launch and capture testing.** GitHub CI can inspect bundle metadata, but cannot prove behavior on those older versions.
- Install the `macos-arm64` DMG on Apple Silicon or `macos-x64` on Intel. Replacing the old DMG download alone will not update an existing installed application.
- MP4/audio device recording, Windows H.264/AAC, scene composition, RTMP, automatic signed updates, and stable distribution still require additional implementation or acceptance testing.
- **Record and Go Live remain disabled.** This release is only a compatibility and hardening update.

## Existing alpha features

Short local capture experiments, native screen diagnostics, independent scene management, local API and GitHub Release update discovery remain available where supported.

See `docs/PRODUCTION_READINESS.md`, `docs/MACOS_MP4_ACCEPTANCE.md`, and `docs/GITHUB_RELEASES.md`.

---

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
