# MooBee Stream Studio v0.3.8-alpha.1 — Native macOS MP4 Experiment

**Experimental standalone alpha — NOT production-ready.** The main Record and Go Live features remain disabled.

## New: First-party Mac recording path

- macOS 15+ now has an optional **Clips → Capture MP4 (macOS test)** action for a short local native H.264/MP4 screen recording (3–15 seconds).
- Uses Apple's ScreenCaptureKit and bundled Swift code statically linked into MooBee's Tauri/Rust binary. No OBS, Streamlabs, extra capture app or FFmpeg executable is required for recording.
- Optional system audio and experimental microphone audio requests. The file is not labeled as audio-verified until independently checked on real hardware.
- Selected monitor is resolved through the actual macOS display ID, not a potentially different ScreenCaptureKit enumeration index.
- Saves to a hidden private MP4 temporary file and only exposes the completed MP4 in the Clips library after native recording finalization and disk synchronization.
- Added macOS Screen Recording/Microphone/Camera permission usage descriptions and macOS 15.0 minimum system version.
- CI typechecks and compiles the native Swift-Rust bridge on Apple Silicon and Intel macOS.

## Retained

- Windows/macOS short silent 640x360 10fps AVI test clips, GIFs and raw input frame diagnostics.
- Browser-local screen/webcam canvas recorder with optional audio where supported.
- Scene management, overlays, local-only API and cross-platform build/test infrastructure.

## Known limitations and acceptance requirements

- **Mac only.** Windows native H.264/AAC/MP4 recording still needs implementation.
- This captures an OS display, not MooBee's saved scene graph, program overlays, camera picture-in-picture or browser layers.
- System and microphone audio track availability, mixing, codec/container details and A/V synchronization have NOT been verified on actual machines. Some macOS versions may reject or corrupt combined audio. Do not assume the MP4 contains audio merely because options were enabled.
- Limited to 3–15 seconds, requires explicit OS capture permissions, and is not a stable long-running recording pipeline.
- Main **Record** and **Go Live** remain disabled. Native Windows parity, production-quality MP4/MKV, recovery, RTMP streaming, signer certificates and security audit are still blocking.
- Windows/macOS alpha installers are unsigned; Android ships a debug APK and does not offer native recording. Linux desktop capture remains unsupported.

## Manual verification

Use [Mac MP4 Hardware Acceptance](https://github.com/migtam628/moobee-stream-studio/blob/main/docs/MACOS_MP4_ACCEPTANCE.md). Record a 4-second clip, play it in QuickTime/VLC, and inspect H.264 video, duration, selected display and actual audio streams. Test permission denial, hotplug, double capture and a second display. Compilation is not device acceptance.

Production gates: [Issue #15](https://github.com/migtam628/moobee-stream-studio/issues/15), [Issue #14](https://github.com/migtam628/moobee-stream-studio/issues/14), [Issue #2](https://github.com/migtam628/moobee-stream-studio/issues/2).
