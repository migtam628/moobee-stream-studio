# Windows live AAC and offline Program FLV — Priority 7O part 2

Adds synchronous registered Media Foundation raw AAC-LC encoding at 48 kHz stereo / 128 kbps. Verifies the negotiated raw AAC AudioSpecificConfig, accumulates bounded 1024-frame PCM blocks, drains available output, validates native timestamp spacing, and generates nominal AAC packet timestamps. Unbounded codec delay, reordered output, incompatible framing and excess packet counts fail closed. Native AAC delay and the unflushed final partial block are not compensated.

The existing composed Program encoder now selects native Windows AVC/AAC or macOS VideoToolbox/AudioToolbox. The same offline FLV sink and native capture loop support Windows: select one capture card or up to four displays, select named audio and/or default WASAPI desktop capture, choose 2–8 seconds, and use Studio > Capture diagnostics > Offline streaming packet FLV test. No separate webcam inset, network, destination credential, real broadcast or pause/Take support in this diagnostic. Native sink validates packet framing/queue admission and publishes only finalized local files under its existing byte budget.

Unit regressions validate AAC sample-clock handling and browser mock Windows capture routing. Native synthetic encoder tests are opt-in:
```powershell
cd apps/desktop/src-tauri
cargo test --lib windows_aac::tests
cargo test --lib windows_aac::tests::native_windows_aac_produces_raw_packets -- --ignored --exact
cargo test --lib live_program_encoder::tests::composed_ticks_produce_queue_compatible_av_packets -- --ignored --exact
```
Actual encoder compatibility, real capture card audio, independent file decode, A/V drift, latency, sustained capture and receiver acceptance are not verified. Windows capture-to-network session integration remains pending. Actual Go Live remains disabled and no installer/release was created. This increment does not change the formal 8/20 core completion count.
