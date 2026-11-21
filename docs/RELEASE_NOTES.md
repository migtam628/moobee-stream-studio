# MooBee Stream Studio v0.3.6-alpha.1 — Native Video File Experiment

**Experimental standalone media milestone, not yet a production recording or livestreaming application.** No OBS/Streamlabs installation is required.

## New in 0.3.6
- **Mac/Windows Clips → Capture Video AVI** saves a 2–8 second local silent video from the continuous XCap capture source.
- The source delivers raw RGBA video; MooBee letterboxes frames to 640×360 and encodes them as individual Motion JPEG images with the Rust `image` crate.
- MooBee's own Rust RIFF AVI muxer writes a seekable, indexed `MJPG` AVI container at 10 fps. JPEG frames are duplicated to maintain AVI timing if the source capture stalls.
- Self-contained: no FFmpeg command, OBS process, Streamlabs connection, external capture program, network transfer or cloud encoding requirement.
- Duration, frame count, memory/output-size cap (64 MiB), exclusive capture guard and UUID-only app-data storage.
- The Clips library lists GIF and AVI formats. AVI files must be played in an external compatible media player (VLC is a good test player); the built-in webview does not decode AVI.
- The browser control API returns HTTP 501 for native AVI capture and never simulates video production.

## Retained
- Native short GIF recording tests and continuous frame-rate diagnostics on Windows/macOS.
- Browser-local screen/webcam picture-in-picture composition with optional microphone/shared-tab audio and MediaRecorder WebM/MP4 output, where supported.
- Scene and display management, alerts/overlays, local API, scene backups and cross-platform alpha packaging.

## Important limitations
- **AVI is silent, limited to 640×360 / 10 fps / 8 seconds.** It is meant to verify raw capture → JPEG encoding → video-file muxing and playback, not to replace full-quality recording.
- Actual input frame availability, camera/system/microphone audio capture, A/V sync, real-time GPU composition, hardware encoders, MP4/MKV muxing, RTMP/RTMPS streaming, multistreaming and reconnect are **not yet implemented in the native desktop engine**.
- The main **Record** and **Go Live** controls remain disabled. Linux/Android native media capture is not supported.
- No built-in AVI player is provided. Playback depends on availability of external MJPEG AVI support. The Chrome/WebView embedded preview is GIF-only.
- Continuous XCap capture is WIP; compilation alone does not validate capture timing, permissions or playback on a user's actual display.
- Alpha installers are unsigned.

## Validation steps
1. Install the Mac or Windows alpha without OBS/Streamlabs.
2. In **Clips**, select a display, 4 seconds, and choose **Capture Video AVI**.
3. Copy the saved file path and open the AVI externally using VLC or another MJPEG AVI-capable media player.
4. Confirm a 640×360, 10 fps silent video lasting approximately 4 seconds; inspect moving screen content, crop/letterbox, and any stalls.
5. Repeat on another monitor and after changing screen-recording permission. Test GIF mode and the 3-second frame diagnostic for regressions.

The AVI container structure was independently sanity-tested with FFprobe using sample MJPEG frames; **real native capture and file playback still require device tests**.

Track the real-time audio/video/broadcast engine in [Issue #2](https://github.com/migtam628/moobee-stream-studio/issues/2).
