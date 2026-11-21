# MooBee Stream Studio v0.3.3-alpha.1 — Browser-Native Local Recording

**Experimental alpha: not yet a complete broadcaster.** Neither the desktop nor the browser edition needs OBS or Streamlabs.

## New in 0.3.3
- Web **Clips → Browser screen recording** uses browser-native `getDisplayMedia` and `MediaRecorder`, with explicit screen-sharing approval.
- User-selectable 5, 10, or 30 seconds of silent local screen/window/tab capture where supported.
- Downloadable WebM or MP4 depending on browser encoder capabilities; local playback preview in the Clips page.
- Video blobs stay on the user's device and are not uploaded to a server.
- Automatic cleanup on stop/share-ended, a conservative 64 MB in-memory limit, and capability/error messages on unsupported browsers.
- Browser UI smoke checks cover recorder affordances and synthetic capture/download state. Real screen sharing still requires human browser permission testing.

## Retained from 0.3.2
- macOS/Windows native Tauri test-clip experiment: bounded 2–10-second **silent GIF** from XCap frames.
- Scene management, preview/program state, sources, test alerts, JSON scene import/export, persistent local control API and private clip storage.
- Builds for Mac Apple Silicon / Intel, Windows, Linux, Android and self-hosted web.

## Limitations / important safety
- The **browser recorder** only records the screen/window/tab a user explicitly selects. It has no mic/system audio mixing, MooBee scene composition, remote storage or livestream output.
- Web APIs require a secure origin (HTTPS or localhost) and browser/platform support. Mobile browsers commonly lack `getDisplayMedia`.
- The native **desktop recorder** remains a very low-rate GIF proof of concept, not MP4/MKV recording.
- Both the main **Record** and **Go Live** controls remain disabled. A true broadcasting engine, synchronized audio, hardware encoding, real-time GPU compositor, RTMP/RTMPS, multistreaming and production authentication are **not implemented**.
- Linux/Android experimental binaries do not provide native desktop capture; unsigned installers require testing and production signing.

## Verification
Node API tests, React/TypeScript build and simulated browser UI tests must pass. GitHub Actions then builds installer packages. **Native hardware capture, browser screen-share permissions, actual media playback and platform compatibility require device testing** and are not established by compilation alone.

See [Native media milestone #2](https://github.com/migtam628/moobee-stream-studio/issues/2) for the next real video/audio pipeline work.
