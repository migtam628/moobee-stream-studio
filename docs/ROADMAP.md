# MooBee Stream Studio — Standalone product architecture

## Non-negotiable product requirements

MooBee is a separate broadcasting product. Users should install MooBee, sign in when accounts are implemented, choose devices, set up scenes, and broadcast without installing or connecting OBS, Streamlabs, or their APIs. Open-source libraries can be bundled in our own distribution if their licenses and source/binary obligations are met.

## Implementation state and gates

| Feature | Alpha 0.3 scope | Gate before calling it complete |
|---|---|---|
| Scene data model | Implemented for native desktop and local Node API | Add persistent shared schema + migrations and tests |
| Preview/program scene selection | Implemented as state | Verify GPU compositor shows accurate preview and program pixels |
| Add screen source | Implemented for first display on macOS/Windows | Device chooser, permission flow, hotplug and multi-display testing |
| Single-frame capture | Rust screenshot command via XCap on macOS/Windows | Live hardware QA, performance and permission acceptance |
| Preview | Still screenshot approximately every 5 seconds | Replace with synchronized low latency continuous frames |
| Audio mixer | Not implemented | Actual capture, level meters, mute, mixing, sync |
| Recording | Not implemented | Produce playable files, crash-safe muxing, seek/test across OS |
| Streaming | Not implemented | Encode and RTMP/SRT push to test endpoint; confirm receiving side |
| Alerts | Local test overlay service | Composite into actual video output |
| Web/API | Local scene control and event SSE | Authentication, persistence, RBAC, remote TLS, audited endpoints |
| Android/Linux capture | Not implemented | Implement and hardware-test platform-native capture |

**Do not mark output capabilities live/working until the corresponding media tests pass.**

## Phase A — native capture and rendering

1. macOS: ScreenCaptureKit, Metal, AVFoundation and permissions.
2. Windows: Windows Graphics Capture, Media Foundation / Direct3D 11, WASAPI.
3. Linux: PipeWire / xdg-desktop-portal, EGL/Vulkan, audio.
4. GPU compositor: source ordering, transforms, crops, browser overlays, camera/game/screen/text/images.
5. Audio mixing: device enumerations, independent faders, ducking, meter levels, sync drift handling.

**Acceptance:** real-time preview and program video at 1080p60, multiple sources, preview/program switching, screen/webcam/audio with verified measured FPS, CPU/GPU and lip-sync. No external broadcasting process.

## Phase B — encode, record, and broadcast

1. Hardware encoder adapters (VideoToolbox, NVENC, AMF, Quick Sync) with software fallback.
2. Local MP4/MKV recording, audio/video muxing, segmented recovery and instant replay.
3. RTMP/RTMPS ingest output first; reconnect and clean end-session transitions.
4. Destination manager, streaming keys encrypted locally, credentials never logged.
5. RTMP fan-out and independent stream health; later SRT and WebRTC.

**Acceptance:** install MooBee alone, capture camera+display+microphone, record playable video, go live to approved test destination, validate receiving stream and handle network loss.

## Phase C — creator API and cloud services

- PostgreSQL data, auth/RBAC, WebSocket/SSE subscriptions, encrypted secrets, versioned APIs.
- User profiles, templates, theme/widget library, alert delivery, unified chat, moderation, tipping.
- Stream health, analytics, clip exports and storage policies; multistream service.
- Mobile broadcasting and web control with explicit platform-native feature limitations.

## Phase D — advanced creator suite

- Local AI Sidekick, triggers, approved automations, scene intelligence.
- Built-in editor, automatic highlight selection, captions and smart framing.
- Guest collaboration through WebRTC and audience engagement widgets.
- Plugin/extension SDK, secure permissions, monetization tools.

## Release policy

Keep legacy v0.2 installers separate. Use feature branch `feature/standalone-broadcast-core` for current migration, then ship v0.3.0-alpha only after native CI and manual Mac/Windows capture testing. Package installers unsigned until signing/notarization are configured. Alpha 0.3 does **not** promise broadcasting.

## Compliance

Do not assume embedded capture/encoding components are free of redistribution obligations. Verify every dependency and codec/SDK license, distribute notices and required source where applicable, and track third-party code provenance.
