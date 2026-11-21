# MooBee Stream Studio — Production Release Gates

_Last audited against the source and CI in November 2026. Distinguish code presence from device acceptance. This is a living release checklist, not a claim of shipped features._

## Product contract

MooBee must operate as its own installation and service. It may **bundle properly licensed third-party libraries**, but shall not require users to install or launch OBS Studio, Streamlabs, or a separate capture/encoding executable. Its Rust/Tauri native media pipeline and local API remain first-party application code. Screen, camera, microphone and cloud permissions must be explicit and revocable.

**Release scope:** Desktop Windows/macOS broadcasting is P0. Linux capture, Android native mobile capture, and the public browser edition are separate capability tracks. Publishing an APK or web bundle is not evidence of feature parity with Windows/macOS.

## Current alpha baseline (verified in source / CI)

| Capability | Implementation | What is still unverified |
| --- | --- | --- |
| Standalone installers | Tauri CI builds macOS ARM64/x64, Windows x64, Linux DEB, Android debug APK and a self-hosted web bundle | Signed/notarized production distribution, secure updates, supported OS hardware acceptance |
| Scene editing and backup | Local scene state, basic source selection and validated JSON collections | Actual GPU-composited program/preview output |
| Mac/Windows display capture | Screenshot, bounded GIF experiment and XCap continuous raw-frame diagnostic | OS permissions, capture cadence/latency, multiple displays, hotplug, protected content handling |
| Native video-file experiment | Short silent 640x360 10fps MJPEG AVI; bounded disk-streaming AVI writer with atomic publish | Capture playback on real macOS/Windows, no audio, no production codec/container |
| Browser-local recording | Browser canvas + screen, optional PiP webcam/audio via MediaRecorder | Real-browser permission/AV-sync tests; separate from installed native broadcaster |
| API | Loopback Node control API, events and local persistence | Remote authentication/authorization, TLS, threat model, audit logging, recovery plans |
| Alerts | Local preview/SSE overlay | Inclusion in actual broadcast-encoded pixels |
| RTMP/RTMPS | Not implemented | All transport, receiving-side, interruption and security testing |

**Do not enable the main Record or Go Live controls until every relevant output gate below passes on actual devices.** A video-format test on CI is not a hardware acceptance test.

## P0: Native media functionality (block production)

- [ ] Implement a persistent, timestamped real-time frame pipeline using ScreenCaptureKit on macOS, Windows Graphics Capture on Windows, and an appropriate Linux capture backend for Linux support.
- [ ] Add webcam and device capture, compositor source ordering, transforms/crops, scenes and program/preview switching. Ensure program output matches the UI scene state.
- [ ] Add microphone and desktop/system audio capture, mixing, mute/levels, drift correction and audio meters.
- [ ] Integrate H.264 hardware encoding where supported (VideoToolbox / Media Foundation or NVENC/AMF/Quick Sync) and a documented software fallback.
- [ ] Create seekable, playable MP4 or MKV files with synced audio/video, graceful stop, disk-full errors, and recovery after process interruption. Use temporary files, fsync and atomic publish or a proven segmented container-recovery strategy.
- [ ] Add RTMP/RTMPS streaming to a **user-authorized test destination**, transport TLS handling, reconnection and replay-safe stream teardown, never logging a stream key.
- [ ] Only then enable Record and Go Live independently after successful integration and real-device acceptance.

### Recording acceptance

- [ ] 1080p30 display + webcam + microphone and system audio on a supported Windows machine and Apple Silicon Mac (1080p60 as a performance target, not an assumed baseline).
- [ ] 30-minute recording with correct duration, decodable entire file, no unexpected silence or audio/video drift outside a documented tolerance; check performance and thermal behavior.
- [ ] Repeated start/stop, permission denial, output path invalid, disk full and abrupt process termination; no corrupt file is silently labeled "complete".
- [ ] Real local files verified independently by FFprobe/FFmpeg **and** a mainstream media player. Keep a manual/automated evidence record.

### Streaming acceptance

- [ ] 2-hour transmission to controlled RTMP/RTMPS test ingress with receiving-side verification of video, audio, frame-rate stability and timestamp monotonicity.
- [ ] Disconnect/reconnect scenario, destination rejection, expired credentials, network switch and process restart with no stream-key leakage or stuck live status.
- [ ] Scene transitions, alerts and mic mute verified in the actual receiving video/audio, not merely in a UI status field.

## P0: Security and distribution (block stable release)

- [ ] Mac hardened-runtime code signing and Apple notarization; publish Team ID verification and SHA-256 checksum.
- [ ] Windows Authenticode signing of setup executable, correct uninstall and installer reputation process.
- [ ] Android release keystore + production-signed AAB/APK (current debug APK cannot be labeled production). Explicit Android permissions and per-platform capture limitations.
- [ ] Authenticated remote API, TLS, CSRF/CORS/Origin protections, per-action authorization, rate limiting and a tested secret-storage/rotation model. Until complete: **loopback only**, never publicly expose the Node API.
- [ ] Dependency license audit and bundled notices/attribution, codec patent/licensing review and SBOM. Do not reuse proprietary competitor brand assets.
- [ ] Dependency and secret scanning, stable update-channel integrity, rollback/recovery, support logs with redaction and crash telemetry opt-in/consent.

**External prerequisites:** developer signing certificates/keys, Apple Developer ID + notarization credentials, Windows code-signing credentials, Android release keystore, and owned streaming test ingress credentials. None should be committed into Git. Production release requires these through secured CI secrets or an equivalent hardware-backed signing service.

## P1: Release quality and operations

- [ ] Clean install, first-launch permission onboarding, logout/uninstall and upgrade migration on supported platform versions.
- [ ] Scene collection backups/restores, corrupted-state recovery and disk-usage controls (recording retention, cleanup, quota warnings).
- [ ] Creator UX: complete device selection and audio routing, scene editing/preview parity, source health, dropped frames, bitrate, recording time and clear error states.
- [ ] Local log viewer and diagnostic bundle that never includes streaming keys or device secrets.
- [ ] Offline/local-mode behavior, transparent unsupported-feature labels on Linux/Android/web, accessibility and mobile-safe layouts.
- [ ] Publish documented support/issue reporting, privacy policy, release notes and compatibility matrix.

## CI & promotion policy

1. PR: require Node API tests, TypeScript/build, Chromium UI smoke tests, Rust AVI writer unit/FFprobe/full decode tests, and native compile where code changes affect platform modules.
2. **Alpha:** publish prerelease artifacts only after all requested platform packaging succeeds, the release branch/version matches NPM+Tauri+Cargo, and native media-format tests pass. Mark unsigned/debug artifacts explicitly.
3. **Beta:** require recorded manual hardware acceptance evidence, integrated real MP4/MKV+audio and RTMP tests, signed builds on the intended supported desktop OSes.
4. **Release candidate:** freeze features, do security/licensing audit, soak/permission/regression testing, verified upgrade/rollback and stable-update configuration.
5. **Stable:** publish only after every P0 above has checked evidence and production signing is configured. Never relabel the current alpha as stable without functionality parity.

## Near-term engineering sequence

1. **0.3.7 reliability:** incremental AVI disk I/O, bounded memory, atomic finalization, complete video decode CI and accurate build/version reporting. *Does not unlock Record or Go Live.*
2. Next: native frame timestamping, AV capture and an encoder/muxer adapter interface with a playable 1080p30 H.264/AAC MP4 target.
3. Next: verified native scene compositor with camera and overlays, synchronized audio and real start/stop recording control.
4. Next: first RTMP/RTMPS stream with safe credential handling and test-ingress validation.
5. Next: signing, installer/updater hardening, sustained device QA, external security/licensing review and stable promotion.
