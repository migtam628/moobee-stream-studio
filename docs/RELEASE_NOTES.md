# MooBee Stream Studio v0.3.4-alpha.1 — Browser Composited Recording

**Development prerelease, not a complete streaming/broadcasting product.** No OBS or Streamlabs installation is required.

## New in 0.3.4
- Browser-based **Clips → Browser screen recording** adds first-party canvas composition (1280×720, up to 30 fps depending on browser performance).
- User-approved display/window/tab as the main source.
- Optional webcam picture-in-picture (bottom-left or bottom-right), sized and cropped by MooBee's canvas compositor.
- Optional microphone audio and available shared-tab/system audio mixed through Web Audio to a single MediaRecorder audio track.
- User-selectable 5, 10 or 30-second local recording, in-browser playback, and WebM/MP4 download based on supported browser encoders.
- Recording stays in the browser until the user saves it. No server upload, platform credentials or external capture/encoder executable.
- Cleanup of browser media tracks, AudioContext, animation timers, previews and URLs; guard against duplicate permission prompts and stale unmounted sessions.
- Web UI smoke tests run a **synthetic** screen + webcam stream and assert the local download workflow; this does not establish media quality on real hardware.

## Retained
- Tauri 2 Windows/macOS standalone app and low-frame-rate native GIF test-clip export.
- Local scene collections, preview/program selection, display sources, overlays, test alerts and local API.
- Existing macOS/Windows/Linux/Android and web CI packaging.

## Limits and verification
- **The browser compositor is not yet wired to MooBee's saved scene graph** and is not an installed-desktop GPU compositor. It does not stream to Twitch, YouTube, Kick or any other destination.
- Desktop full-frame-rate native capture, synced CoreAudio/WASAPI system audio, hardware encoding, MP4/MKV recording and RTMP/RTMPS broadcasting **are not implemented**. The main **Record** and **Go Live** controls remain disabled.
- Shared-tab/system audio is often missing on macOS, Safari and other platforms. The browser warns when requested shared audio is absent.
- Actual frame rates and A/V sync vary with system performance. Recording in background tabs can be throttled.
- Browsers require HTTPS or localhost, explicit share/camera/microphone permission and support for \`getDisplayMedia\`, \`canvas.captureStream\`, Web Audio and \`MediaRecorder\`.
- Alpha installers are unsigned. Android/Linux native capture is not available.
- Device tests must verify saved files really play with audio and camera in Chrome/Edge/Safari as applicable.

## Suggested manual test
1. On HTTPS in a supported desktop browser open **Clips → Browser screen recording**.
2. Enable webcam and microphone, choose camera position, and approve permissions.
3. Record 10 seconds with visible motion and speaking; stop, preview and save the local file.
4. Inspect the resulting file in a media player: check 720p video, microphone audio, camera position, duration, dropped frames and A/V sync.
5. Repeat with shared-tab audio and browser/system audio options. Browsers may omit this track; the interface should warn accordingly.
6. Repeat after denying a permission to verify all active tracks are released without partial uploads.

Code and ongoing work: [Native media engine issue #2](https://github.com/migtam628/moobee-stream-studio/issues/2).
