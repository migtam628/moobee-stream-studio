# MooBee Stream Studio v0.3.5-alpha.1 — Native Frame Source Diagnostics

**Experimental development prerelease, not an operational broadcaster.** No OBS Studio or Streamlabs installation is required.

## New in 0.3.5
- On Mac and Windows, the installed Clips workspace can run a **3-second native raw-video-frame diagnostic** on a selected display.
- Uses the first-party application's existing XCap 0.9.10 dependency: `Monitor::video_recorder()`, `VideoRecorder::start/stop`, and the incoming raw-frame receiver.
- Reports incoming frame count, average input FPS, frame dimensions, first-frame latency, and largest frame-to-frame timing gap.
- Does not save, encode, or upload any of the sampled frames. Does not use a new external process.
- Shares the capture guard with GIF test clips so overlapping native acquisition tests are rejected.
- Duration validation (2–5 seconds), no-frame detection, permission/unsupported-platform errors, and Rust unit tests.
- Browser/API edition returns HTTP 501 for the native-only performance test.

## Retained from 0.3.4
- Browser screen recording: a 1280×720 canvas composed from a user-approved shared screen and optional webcam PiP, with optional microphone/shared-tab audio and a local WebM/MP4 download where supported.
- Mac/Windows GIF capture-to-file prototype, scene/source management, test alerts, import/export, local API and platform installers.

## Important limitations
- **Raw input FPS is not an encoded recording or streaming FPS.** This probe is a performance/permissions diagnostic only.
- XCap continuous video recording is documented upstream as work-in-progress, and requires real-device validation on Windows and macOS. A successful CI compile cannot verify actual raw frames arrive.
- The main **Record** and **Go Live** controls remain disabled. A native GPU compositor, native synchronized microphone/system audio, hardware video encoder, MP4/MKV muxer, RTMP/RTMPS, multistream and reliable reconnect have not been implemented.
- Experimental Linux/Android builds do not expose the native capture probe. Installers are unsigned and require OS permission review.

## Manual acceptance checklist
1. Install Mac or Windows Alpha 0.3.5 on a test machine **without OBS/Streamlabs**.
2. Grant operating-system screen capture permission. Choose a monitor in **Clips**.
3. Press **Measure 3-second input FPS**, observe the reported frame count, FPS, dimensions and first-frame latency.
4. Repeat with moving content, another monitor, and after temporarily denying capture permission to confirm useful failures.
5. Compare measured FPS with system CPU/GPU/permission status; preserve the results for actual real-time video engine design.

Source capture API: [XCap documentation](https://docs.rs/crate/xcap/0.9.10). Ongoing native media build: [GitHub Issue #2](https://github.com/migtam628/moobee-stream-studio/issues/2).
