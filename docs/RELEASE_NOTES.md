# MooBee Stream Studio v0.3.9-alpha.1 — Mac Recording Sessions & Service Diagnostics

**Experimental alpha.** This is a functional expansion of the native Mac capture workflow, not a production broadcaster. The primary program **Record** and **Go Live** actions remain disabled.

## Shipped improvements (code and CI validation required)

- **Longer Mac display recordings:** the built-in ScreenCaptureKit H.264 MP4 adapter now accepts bounded 3–120-second sessions on macOS 15+. Studio offers 4/6/8/15/30/60/120-second maximum durations.
- **Stop and save:** while a native Mac MP4 is capturing, a Stop button signals the Swift recorder to finish early; the existing Rust workflow waits for ScreenCaptureKit finalization, checks MP4 box structure, and only then atomically exposes the saved file.
- **Finder integration:** saved MP4 files can be revealed using the existing validated private clip-ID interface.
- **Runtime Service Diagnostics:** Settings now checks local monitor detection and the embedded overlay health endpoint, shows whether the Mac recording backend is supported, and explicitly identifies unavailable RTMP, real-time composition, audio mixing and signed automatic updates.
- **Truthful status indicators:** the application distinguishes a functioning scene editor from actual live output; it does not claim a broadcast is active when no stream exists.
- **Version alignment:** web, API, desktop, Cargo, Tauri, and npm lockfile now identify the separate `0.3.9` build rather than reusing the earlier `0.3.8` installer version.

## What must be tested on a real Mac

1. Install the Apple Silicon macOS DMG, verify app launch, displays and Settings diagnostics.
2. In Studio, record a 15-second MP4 without audio; check the file in QuickTime/VLC.
3. Start a 60- or 120-second maximum test; press **Stop and save MP4** early; wait for a complete file, then independently verify seekability, playback and actual recorded duration.
4. Test requested microphone/system audio separately and verify the resulting track(s). A checked option does not prove audio capture.
5. Repeat after denying screen access and restarting the app. Failed recordings must not show up as completed clips.

## Unfinished production functions

- No real-time program scene compositor or webcam/image/text layers rendered into encoded video.
- No verified synchronized microphone/system-audio mixer or production-length recordings.
- No native Windows H.264/AAC parity.
- No RTMP/RTMPS broadcasting to Twitch, YouTube, Kick, or custom destinations.
- No developer-signed/notarized installers or cryptographically signed in-app update bundles.
- The Android APK remains a debug/test package, not a mobile live production engine.

All native device claims remain **unverified pending hardware acceptance** even after GitHub Actions compiles successfully. See `docs/MACOS_MP4_ACCEPTANCE.md` and `docs/PRODUCTION_READINESS.md`.

---

# MooBee Stream Studio v0.3.8-alpha.4 — Usable Native Screen Capture in Studio

**Experimental alpha — not a finished livestreaming application.** This update makes existing standalone recording test features easier to use directly from the Studio workspace.

## What works in this update

- **Studio → Capture screen test** opens a new native Quick Screen Capture section with display selection, 4/6/8-second recording length, explicit progress and error messages, and saved file information.
- **macOS 15+** uses the existing native ScreenCaptureKit H.264 MP4 adapter. System audio and microphone requests are optional and default off until compatibility is verified on actual hardware.
- **Windows and macOS 13–14** use the existing short silent MJPEG AVI fallback (640×360/10 fps). This does not record the saved scene, webcam, overlays or mixed audio.
- **Show in Finder** reveals validated UUID-named MP4/AVI/GIF files from MooBee's own private clips folder. No arbitrary filesystem path is accepted.
- Studio preview labels now distinguish periodic display snapshots from an actual livestream or composited Program.
- Quick Capture provides direct access to the Clips library and a copyable file path.

## Important limitations

- Native MP4 encoder output and audio tracks must still be checked with a real Mac and a media player; CI only verifies build, linking and container structure.
- Main **Record** and **Go Live** controls remain deliberately disabled. Full real-time compositor, webcam/audio mixing, streaming destinations, secure keys, RTMP/RTMPS and device-soak tests are not implemented.
- App targets macOS 13+ for desktop scene tools; experimental ScreenCaptureKit MP4 remains macOS 15+ only.
- Unsigned alpha installers are for local testing. No signed automatic installer updates.

See `docs/PRODUCTION_READINESS.md` and `docs/GITHUB_RELEASES.md` for remaining blockers.

---

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
