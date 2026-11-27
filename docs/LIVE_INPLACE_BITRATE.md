# Workstream 10.8 — in-place bitrate downshift

The developer-only real-stream acceptance panel now offers **Adaptive bitrate during stream**, a separate opt-in from reconnect fallback. It is off by default and requires exactly one confirmed destination. The backend rejects multi-output requests with this flag. No normal production Go Live activation.

The existing sender-pressure advisor observes the same sending output for at least three seconds, with no sample gap over 1.5 seconds, at 4 MiB or 96 queued packets. At a completed two-second GOP boundary the capture worker may lower the target from 6000 to 3000 or from 3000 to 1500 kbps, with at least ten seconds between requests and at most two changes per encoder epoch. No automatic upshift within an epoch. Reconnect begins a fresh epoch with the independently chosen reconnect fallback policy. Resolution, frame rate, audio and publisher connection are not deliberately changed. These thresholds are conservative starting points, not validated network-capacity estimates.

macOS uses VideoToolbox AverageBitRate and checks the property readback. Windows uses ICodecAPI::IsModifiable and AVEncCommonMeanBitRate and checks GetValue. A definitively unsupported property disables further in-place requests for that epoch without changing the reported target. Failed or uncertain mutation stops the encoder/session rather than reporting a false success. Existing AVC parameter-set checks, IDR and timestamp validation remain in force; a driver that changes configuration or reorders frames fails closed. A successful property readback establishes the requested setting, not measured output throughput or decoder continuity. Analytics displays applied changes, unsupported/uncertain state and target rate. The monitoring UI is not a receiver measurement.

API references:
- https://developer.apple.com/documentation/videotoolbox/compression-properties
- https://developer.apple.com/documentation/videotoolbox/vtsession-api-collection
- https://learn.microsoft.com/en-us/windows/win32/codecapi/avenccommonmeanbitrate-property
- https://learn.microsoft.com/en-us/windows/win32/api/icodecapi/nf-icodecapi-icodecapi-ismodifiable

Pure policy tests cover opt-in, single output, fresh pressure, cooldown and unsupported handling. Browser QA mocks native IPC and verifies confirmed settings. An opt-in native synthetic test (not run by default) checks 40 video ticks and AAC continuity, one AVC config, and IDR across a 3000-to-1500 kbps property change:

```sh
cd apps/desktop/src-tauri
cargo test --lib live_program_encoder::tests::native_inplace_bitrate_preserves_packet_epoch -- --ignored --exact --nocapture
```

Real driver support, effective bitrate under varying content, receiver decoding after change, congestion fault injection, capture-device recovery and two-hour hardware soak remain unverified. The actual test is not a broadcast. No installer or release executed.
