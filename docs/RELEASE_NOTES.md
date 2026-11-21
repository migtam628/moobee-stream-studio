# MooBee Stream Studio v0.3.2-alpha.1 — Native Test Clip Lab

**Experimental standalone prerelease, not a complete live-broadcast application.**
No OBS Studio, Streamlabs, external encoder executable or external capture API is required.

## New in Alpha 0.3.2

- Built-in first-party capture-to-file vertical slice on macOS/Windows.
- Select any detected display in the **Clips** workspace and capture a **2, 4, 6, or 10 second silent animated GIF**.
- Rust XCap screenshot sampling at approximately **2 frames per second**, GIF encoding via the embedded Rust `image` crate, automatic output under MooBee's app-data `test-clips` folder.
- Animated preview, recent local clip listing, saved file path and clipboard-copy action.
- bounded capture duration, output size checks (32 MB), single-capture concurrency guard, UUID-only local clip access.
- Browser/API capability gates: no fake remote capture, encoding, or Go Live state.

This work exercises successive native acquisition, image encoding and file persistence, and is **not** equivalent to full-frame-rate MP4/MKV recording or a production replay buffer. Actual FPS may vary with capture performance and operating-system permissions.

## Retained capabilities

- Standalone Tauri 2 application, macOS/Windows single-frame capture, scene management, display selection, preview/program state.
- Local API, versioned scene collections, JSON backup/restore, test alerts and local overlays.
- Experimental Mac Intel/Apple Silicon, Windows, Linux, Android ARM64 and self-hosted web distributions.

## Still unavailable

- Microphone/desktop audio, real-time multi-source GPU composition, 1080p60 capture, hardware video encoding, playable MP4/MKV videos, RTMP/RTMPS broadcasting, reconnect/multistreaming.
- Main Record and Go Live remain disabled. Android and Linux cannot capture local test clips in this alpha.
- Signed/notarized production installers and remote-user authenticated API.

## Testing

On macOS or Windows: install this alpha, grant Screen Recording permission as needed, open **Clips**, select a display, choose 4 seconds and **Capture Silent GIF**. Verify that the saved file is playable and the local preview animates. The resulting file is private to your device. Repeat with another display, and verify errors when permission is denied.

The platform builds validate compilation, not actual devices, audio/video fidelity, streaming, or performance. Report issues in the [GitHub repository](https://github.com/migtam628/moobee-stream-studio/issues).
