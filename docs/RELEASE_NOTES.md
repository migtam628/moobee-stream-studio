# MooBee Stream Studio v0.3.9-alpha.7 — Native Multi-Display Preview & Exact Build Identity

**Experimental prerelease, not full Program recording or livestreaming.**

## Improvements over alpha.6

- Added a first-party native **still-frame compositor** for Mac and Windows that captures up to four enabled display sources and arranges them into a bounded 960×540 composite PNG. It preserves aspect ratios and runs off the Tauri UI thread.
- Studio Preview and Program now request all enabled display sources in the selected scene instead of silently displaying only the first screen. Native capture errors are shown; the preview still refreshes approximately every five seconds.
- Added pixel-level Rust tests for two-display composition, letterboxing, and rejecting invalid source counts/IDs. The browser UI and native build are covered by existing CI checks.
- Sidebar and Settings now show the actual GitHub release tag and commit identifier built into the app, rather than reporting every experimental installer simply as `0.3.9 (DEV)`. Local and PR builds remain visibly marked as development builds.
- Includes all alpha.6 reliability improvements: measured MP4 duration rather than requested maximum, improved saved-clip persistence inventory, and independent audio-level verification scripts.

## Hardware acceptance (do not infer from CI)

1. Install ARM64 DMG on macOS 14.2, quit the previous instance, and verify sidebar/Settings show the alpha.7 release tag and short commit.
2. In Studio, create a scene with **two enabled display sources**, when two displays are available. Confirm both appear side-by-side in the still preview, while the Program and Preview selection behave correctly in Studio Mode. Do not claim multi-source acceptance on a single-display Mac.
3. Disable one source and confirm the remaining preview fills the canvas without distorted aspect ratio. Deny Screen Recording to confirm the app shows a capture error rather than falsely indicating live output.
4. Recheck a single-display MP4, then a 60-second maximum recording stopped at ~15 seconds. Validate actual duration, playback/seeking, and clip inventory after restarting the application.
5. Record system audio from a permitted audible source and verify the recorded AAC track independently. A merely present audio track does not establish audible content or synchronization.

## Beta blockers

This compositor generates **still frames, not a continuously synchronized video stream**. Real Program composition (camera and overlays), a sustained H.264/AAC scene recorder, live audio mixer, RTMP/RTMPS, secure credentials and real-device acceptance remain unfinished. Keep the main **Record** and **Go Live** controls disabled.

---

# MooBee Stream Studio v0.3.9-alpha.6 — Measured Recording Duration & Beta Acceptance Tools

**Experimental prerelease.** This update hardens the working first-party macOS H.264 MP4 recording workflow on Sonoma 14.2; it is not full Program recording or RTMP broadcasting.

## Recording reliability

- After MP4 finalization, MooBee now reads the **actual movie duration** from `moov/mvhd` instead of displaying the selected recording maximum as if it were the clip's duration. Early Stop and Save (for example, 60-second maximum stopped after 15 seconds) can now show an independently reported length.
- Studio Quick Screen Capture now displays the measured duration, requested maximum, file size, and a refreshed **saved local clips inventory**. The inventory reloads when Studio is opened again, making restart-persistence checks easier.
- Added **Use Program display** for a scene containing exactly one enabled screen source. It resolves the currently selected Program scene, not Studio Mode Preview, and refuses multi-source scenes instead of silently dropping layers. This is not a compositor or Program recorder.
- The native MP4 duration probe reads only the metadata boxes; no video footage needs to be uploaded or copied to the cloud. Rust tests cover both common MP4 movie-header versions and missing timing information.

## Audio and automated verification

- The offline `scripts/verify-native-mp4.mjs` test supports `--stopped-early` so early stop recordings are not incorrectly rejected for being shorter than the chosen maximum.
- Added optional `--expect-audible` verification. FFmpeg locally checks AAC track presence and measures peak level; an effectively silent track (below -50 dBFS) fails. **A measured signal is not proof of intended audio-source identity or A/V synchronization**.
- GitHub CI now synthesizes an H.264/AAC test MP4, checks full-file decoding, simulates early-stop duration validation, and confirms silent AAC is rejected when audible audio was requested.
- Recording hardware acceptance still requires **real macOS 14.2 tests**. CI synthesis is not a substitute for actual ScreenCaptureKit capture.

## Still blocking streaming beta

Real scene composition (display/webcam/overlay layers), continuous Program recording, synchronized microphone/system-audio mixing, native Windows H.264/AAC parity, RTMP/RTMPS streaming, and secure signed production updates remain unimplemented or unverified. The main Studio **Record** and **Go Live** buttons remain disabled.

## Suggested next device tests

1. On the installed Mac build, select a **60-second maximum** and click **Stop and save MP4** after approximately 15 seconds. Compare the reported *actual* MP4 duration with QuickTime.
2. Restart MooBee, open Studio, and use **Refresh saved clips**. Open the previous MP4 in QuickTime to confirm playback and seeking, not merely its presence.
3. Play permitted, clearly audible system sound while selecting **Request system audio**. Record ~15 seconds, listen to playback, and independently inspect the AAC audio track and signal. Do not enable mic on macOS 14; that requires a separate implementation.

See `docs/MACOS_MP4_ACCEPTANCE.md` for complete tests. No automation transmits user's media to GitHub.

---

# MooBee Stream Studio v0.3.9-alpha.5 — Sonoma Screen Recording Permission Recovery

**Experimental alpha — user consent remains required.** This update helps diagnose and recover from macOS screen recording permission denial.

## Changes since alpha.4

- Recognizes ScreenCaptureKit error `-3801` as a macOS Screen Recording privacy denial rather than an AVAssetWriter or H.264 encoder error.
- On the Studio Quick Capture screen, displays a permission-specific explanation and an **Open Screen Recording Settings** shortcut. The app cannot grant itself access or bypass macOS privacy controls.
- Retains the underlying native error to support accurate debugging and avoids automatic permission resets or shell scripts that affect other apps.
- Adds tests distinguishing actual `-3801` errors from unrelated ScreenCaptureKit/codec failures and documents Sonoma permission recovery.

## macOS 14.2 acceptance

Quit MooBee; enable it under **System Settings → Privacy & Security → Screen Recording**; reopen it from /Applications. Try a 15-second MP4 with audio off. If denied again, verify the installed bundle ID and consider resetting **only MooBee's** ScreenCapture consent (see `docs/MACOS_MP4_ACCEPTANCE.md`).

Success criteria are a playable, seekable H.264 recording, followed by early Stop and Save, system audio, repeated capture and relaunch checks. **A green GitHub build does not prove device acceptance.**

Full Program recording, scene compositor, microphone mixer, RTMP, and signed updates remain unavailable or incomplete. Record and Go Live remain gated.

---

# MooBee Stream Studio v0.3.9-alpha.4 — Sonoma MP4 Error Visibility and Temp-file Hardening

**Diagnostic alpha for macOS 14.2.** This release addresses a user-observed failure, but actual MP4 capture success is not yet verified on the affected Mac.

## Changes from alpha.3

- Fixed the **“Native recording failed: undefined”** message in Studio. Tauri `invoke()` can reject with a plain Rust error string; the frontend now safely normalizes strings, JavaScript Errors, and structured error objects so the actual failure reaches the screen. Regression tests cover these cases.
- Changed native recording's hidden temporary output from `.clip-ID.mp4.part` to `.clip-ID.part.mp4`. It remains hidden and unpublished during recording, while presenting a proper `.mp4` extension to the Sonoma `AVAssetWriter` backend. The Rust pipeline still validates the complete file and atomically renames it to the visible `clip-ID.mp4`.
- Added Swift native error domain/code and underlying OS error when the Sonoma or Sequoia recorder fails. This is local-only diagnostic information and does not upload recordings.
- Preserved the experimental macOS 14 H.264 MP4 backend and macOS 15+ recorder, including explicit early Stop and Save controls.
- No recorded video, audio, screenshots or credentials are uploaded to GitHub by these changes.

## What to verify on macOS 14.2 Apple Silicon

1. Download the alpha.4 **ARM64 DMG** from this release only after GitHub checks and artifact publication complete. Replace the old installed application and reopen MooBee.
2. Open Studio → Quick screen capture, choose the Built-in Retina Display, select **15 seconds**, leave both audio options disabled, and click **Capture screen test**.
3. If recording fails, copy the **complete error text** (including native domain/code) and report it; it should no longer say `undefined`. Do not repeatedly retry after a timeout without checking whether a recording is still finalizing.
4. If a file is saved, verify correct motion, 15-second approximate duration, H.264 codec and QuickTime seek/playback; then test a 60-second maximum with an early Stop and Save.
5. Test system audio only after silent recording works. The Sonoma microphone control remains disabled.
6. Repeat after reopening the application and check Clips persistence.

## Still unavailable

Program scene composition, microphone/system audio mixing, Windows MP4 parity, RTMP/RTMPS streaming, signed automatic updates, and long-duration soak acceptance have **not** passed. Record and Go Live remain disabled.

---

# MooBee Stream Studio v0.3.9-alpha.3 — Sonoma 14 H.264 Test Recorder

**Experimental prerelease for macOS 14.2 compatibility testing. Not a complete broadcasting studio.**

## What changed

- Added a first-party macOS **14.x Sonoma** MP4 capture backend. ScreenCaptureKit supplies video frames; AVFoundation's `AVAssetWriter` encodes H.264 and finalizes the MP4. It runs inside MooBee's native Tauri application and does **not** require OBS, Streamlabs, or an external FFmpeg service.
- macOS **15+** continues to use the existing `SCRecordingOutput` backend. The native capability report chooses the backend based on the actual OS version.
- Studio → Quick screen capture and Clips now offer an **experimental H.264 MP4** option on macOS 14+ with bounded 4/6/8/15/30/60/120-second selections and early Stop and Save.
- Optional **system audio** capture on Sonoma is experimental and must be independently verified for AAC, audibility and sync. **Microphone capture on macOS 14 is disabled**; it is available for experimental testing on macOS 15+ only.
- Updated macOS 14.2 hardware acceptance checklist, project documentation and deployment compatibility notes.
- Swift typecheck and Rust linking checks have passed on GitHub CI for Apple Silicon and Intel. This proves that the source compiles against the runner SDK; **it does not verify recording on a Mac running Sonoma 14.2**.

## macOS 14.2 manual acceptance (required before beta)

1. Install the matching Apple Silicon DMG, fully quit/reopen MooBee, and confirm **Settings → Runtime Service Diagnostics** reports the native MP4 backend available and the Sonoma AVAssetWriter route.
2. Choose one display; leave both audio options off; capture **15 seconds** as H.264 MP4. Check real motion, correct duration, seeking and playback in QuickTime.
3. Start a **60-second** maximum MP4 and select **Stop and save MP4** after approximately 15 seconds. Check that it finalizes and opens correctly, without exposing `.mp4.part`.
4. Repeat with only system audio enabled. Check for an AAC track and audible synchronized audio. macOS 14 microphone audio is **not supported by this implementation**.
5. Test consecutive captures, screen-permission denial, an app restart and Clips persistence. Save any error text and check the actual file with `scripts/verify-native-mp4.mjs` when possible.

## Still unavailable

- Full real-time Program scene compositor with saved overlays, webcam layers and transitions feeding the recorder.
- Synchronized production audio mixer, live device meters and microphone on the Sonoma path.
- Production-length recorded output, Windows native H.264/AAC, RTMP/RTMPS livestreaming and multistream destinations.
- Cryptographically signed automatic updates and signed/notarized production installers.

**Record** and **Go Live** for Program output stay disabled. The new H.264 functionality is an experimental **display-recording test**, not the finished production recorder.

---

# MooBee Stream Studio v0.3.9-alpha.2 — Native MP4 Availability Diagnostics

**Diagnostic prerelease, not a complete broadcaster.** This update does not bypass macOS requirements or enable unfinished streaming controls.

## Changes since v0.3.9-alpha.1

- macOS native Swift bridge now reports the actual operating-system version used to decide whether the ScreenCaptureKit H.264 MP4 recorder is available.
- Settings → Runtime Service Diagnostics and Clips → Capture MP4 explain whether the device is below macOS 15 or whether the native bridge unexpectedly rejected an otherwise supported Mac.
- Studio → Quick Screen Capture shows the same native availability reason and preserves silent AVI fallback when MP4 is disabled.
- Test evidence: one user-provided four-second 640×360/10fps MJPEG AVI successfully decoded all 40 frames; no audio was expected or present. This does not verify MP4 capability, video/audio sync, or complete scene recording.

## Acceptance required

1. In Terminal run `sw_vers -productVersion` and confirm the actual Mac operating-system version.
2. In Settings refresh the diagnostic report. It should identify a matching OS version and give a reason for MP4 being disabled if it is.
3. On a supported macOS 15+ device, try a 15-second silent MP4 from Studio, then Stop/Save and independently check actual MP4 duration, playback and audio tracks.
4. Keep the main Record and Go Live controls disabled until real program composition, audio mixing, and broadcasting pass separate hardware tests.

No production readiness or beta claim is implied.

---

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
