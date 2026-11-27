# Windows direct H.264 live packet encoder — Priority 7O part 1

This adds a same-thread native Media Foundation H.264 encoder transform, not a container sink writer. It requests a synchronous registered NV12-to-H.264 transform, baseline profile, low-latency mode and two-second GOP configuration; the caller may force an IDR. The adapter uses the existing RGBA-to-NV12 conversion and nominal 640x360/10, 1280x720/15 or 1920x1080/30 profiles with 1500/3000/6000 kbps targets. It accepts only complete immediate output samples, exact PTS and matching DTS when provided, with a 2 MiB access-unit bound. Delayed/reordered/dynamic-format output fails closed rather than fabricating timing.

The Rust packetizer parses Annex B start codes, extracts SPS/PPS from the output type and/or first access unit, emits AVC configuration before the first IDR, rejects parameter set changes and converts media to the existing length-prefixed RTMP AVC format. No file, network connection, external encoder executable or OBS integration. A compatible synchronous OS-registered encoder is required; no guaranteed hardware acceleration or all-transform compatibility.

Unit tests exercise malformed Annex B, IDR/configuration, parameter-set changes and queue admission. The native synthetic encoder regression is opt-in (run on a Windows installation with Media Foundation H.264 support):
```powershell
cd apps/desktop/src-tauri
cargo test --lib windows_avc::tests
cargo test --lib windows_avc::tests::native_windows_avc_into_live_queue -- --ignored --exact
```

This is the first Windows live-encoder increment, not a complete Windows streaming path. Raw AAC, combined A/V producer, real Program integration, offline FLV playback, sustained output, hardware capture, receiving-side RTMPS acceptance and actual UI authorization are still pending. Windows N may require Media Feature Pack. No production readiness gates are changed. No installer or real livestream was initiated.
