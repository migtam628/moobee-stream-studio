# MooBee Stream Studio v0.3.7-alpha.1 — Recording Reliability

**Standalone alpha — not a production broadcaster.** This release strengthens experimental video-file integrity and release verification. It does not enable Go Live or the primary Record control.

## What changed

- Replaced whole-video in-memory AVI muxing with a first-party **incremental RIFF AVI writer** that writes one JPEG frame at a time to a stream.
- During native short AVI capture on Windows/macOS, MooBee writes to a UUID-named private `.avi.part` file. A successful capture finalizes RIFF/movi lengths and the AVI index, flushes and synchronizes the file, closes it, and renames it to `.avi`. Failed captures remove the partial file.
- Memory usage in the container writer is bounded to the current frame plus a compact index rather than the full video.
- Added Rust AVI unit tests for incremental writes, invalid inputs, double-finalization rejection and indexed playback.
- Added release verification: **FFprobe metadata and full FFmpeg decode**, not just container headers. FFmpeg is used only in CI runners, not required by MooBee.
- Release CI gates publication on muxer tests, npm/Tauri/Cargo version alignment, and successful packaging for macOS Intel/Apple Silicon, Windows, Linux, Android test APK and web.
- The desktop app and local health API now report the correct package version.
- Added `docs/PRODUCTION_READINESS.md` with P0 audio/video, RTMP, signing, security, hardware QA and promotion criteria.

## Retained capabilities

- Native Mac/Windows short **silent AVI** experiment (640×360, 10 fps, 2–8 seconds), silent GIF test capture and continuous raw-frame performance diagnostic.
- Browser-only 1280×720 canvas screen/webcam recording with optional audio through user-approved MediaRecorder where supported.
- Scene editing, collection backup/restore, local alerts and the creator interface.

## Not yet implemented

The native broadcaster still needs a real-time scene compositor, synchronized microphone and system audio, hardware H.264 encoding, reliable MP4/MKV recording, RTMP/RTMPS output and reconnect handling. Main **Record** and **Go Live** remain disabled. Android is a debug-signed UI build without native capture; Linux screen capture remains unimplemented. Mac/Windows alpha installers are unsigned.

**Do not present this release as production-ready.** Real-device capture, playable AV output, permissions, AV sync, endurance, public API authentication, signed installers, update integrity and licensing audits remain mandatory acceptance gates.

## Manual verification

1. Install the Windows or macOS alpha in a test environment; grant Screen Recording permission.
2. In Clips, select a display and a four-second duration, then choose Capture Video AVI.
3. Open the `.avi` file in VLC or another MJPEG-compatible player. Verify four seconds of visible movement, timing and dimensions.
4. Repeat on another monitor and with screen permission denied. Check failed capture does not appear as a completed file.
5. Abrupt termination may leave a hidden `.avi.part` file; automatic recovery of partial videos is not implemented.

Production gates: [Production Readiness](https://github.com/migtam628/moobee-stream-studio/blob/main/docs/PRODUCTION_READINESS.md). Native engine: [Issue #2](https://github.com/migtam628/moobee-stream-studio/issues/2).
