# macOS ScreenCaptureKit MP4 — Hardware acceptance

## Purpose and scope

This experiment adds a **native H.264 MP4 screen recorder on macOS 15+** with optional system audio and optional microphone requested through ScreenCaptureKit. It compiles Swift into the installed Tauri binary, without OBS, Streamlabs or an FFmpeg capture executable. It does **not** yet record the MooBee program scene, webcam layer or RTMP stream.

The Rust command records for **3–15 seconds** into a private `.mp4.part` file and publishes the finished `.mp4` only after the ScreenCaptureKit completion delegate, bounded output-size checks, top-level MP4 box validation (`ftyp`, `moov`, non-empty `mdat`, and box sizes), and file synchronization. A successful command means the file was finalized, **not** that microphone/system audio was independently verified.

## Required manual tests (perform on a real Mac)

- [ ] Install on **macOS 15 or later**, with Screen Recording permission granted when requested in System Settings > Privacy & Security. Verify the installed app's Info.plist includes the permission reasons.
- [ ] Test **Apple Silicon and Intel** separately if both are supported. Record OS version, CPU, screen resolution/scale and connected display IDs.
- [ ] In Clips choose one display and a **4-second** capture. Leave microphone off first; system audio may be enabled while playing known, permitted sound.
- [ ] Open the saved `.mp4` in QuickTime and VLC. Ensure correct monitor captured, recognizable motion, duration and seekability.
- [ ] Probe video/audio using a separate diagnostic copy of FFprobe (not needed to record):

```bash
ffprobe -v error -show_entries format=duration,size:stream=index,codec_type,codec_name,width,height,avg_frame_rate,sample_rate,channels -of json "/path/to/clip-UUID.mp4"
```

- [ ] Confirm **H.264 video**, plausible frame rate and dimensions, file duration within a documented tolerance. Check whether the **audio** stream is actually present; an audio checkbox does not guarantee that it is.
- [ ] Record again with a microphone enabled after granting microphone permission; confirm both expected audio sources and measure sync against on-screen clicks and spoken taps. Report missing or corrupted audio.
- [ ] Connect two monitors and confirm MooBee records exactly the selected display. The bridge maps XCap monitor identity to ScreenCaptureKit's macOS display ID rather than relying on array ordering.
- [ ] Deny/revoke screen or mic permission, unplug a selected display, interrupt capture and repeat capture. Confirm errors do not produce a completed file or a false-success UI message.
- [ ] Confirm a failed capture doesn't leave a public `.mp4`; hidden `.mp4.part` files may require manual cleanup after abrupt termination/timeouts.
- [ ] Inspect CPU/GPU, memory and thermal use; this prototype is short and is **not** a long-duration stress test.

## Important limitations

- Mac-only; Windows MP4 and microphone/system audio are **not implemented** in the native engine.
- Output uses Apple's H.264 screen recording API; no camera/source overlay, compositor, saved scene selection or instant replay.
- System + microphone audio mixing differs across OS versions and may fail or produce an unusable file; do not assume success without inspecting decoded tracks.
- Stream keys, recording permissions, code signing, updates, audit logs and privacy policy need production security work.
- The main **Record** and **Go Live** buttons must remain disabled until production-quality recording and RTMP pass supported-device acceptance.

Report evidence in [Hardware Acceptance Issue #14](https://github.com/migtam628/moobee-stream-studio/issues/14) and [Native MP4 Issue #15](https://github.com/migtam628/moobee-stream-studio/issues/15).
