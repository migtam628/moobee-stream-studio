# Local RTMPS receiver acceptance — workstream 9.6

The native test suite starts an independent Rustls server bound only to 127.0.0.1 on an ephemeral port. It generates a temporary certificate authority and localhost server certificate in memory; the client trusts that root only inside the test. No system trust changes, stored stream keys, firewall configuration or public ingest endpoints are needed. Production endpoint validation and TLS settings are unchanged. The normal production application does not include a receiver listener.

The receiver separately parses the sender's supported RTMP fmt0/fmt3 subset, validates handshake and publish commands, issues publish authorization and a ping, checks AVC/AAC configuration and media framing, checks initial IDR, verifies strictly increasing per-track timestamps and checks receipt of deleteStream. Its bounded parser is not a general-purpose RTMP server or decoder. It compares received payload byte and packet totals to native sender counters after drain. It prints a credential-free JSON line prefixed LOOPBACK_RECEIVER_REPORT. The synthetic test exercises multi-chunk media and real encrypted TCP transport, not decoded media. Negative tests check untrusted root and wrong hostname, and malformed message handling.

## Run

From apps/desktop/src-tauri on macOS or Windows:

```sh
cargo test --lib rtmp_session::tls::tests::loopback_tls_independent_receiver_accepts_drained_synthetic_packets -- --exact --nocapture
cargo test --lib rtmp_session::tls::tests::loopback_tls_rejects_untrusted_root_and_wrong_hostname -- --exact
cargo test --lib rtmps_test_receiver
```

The optional native encoder test generates two seconds of synthetic RGBA and stereo PCM, sends native AVC/AAC packets through the same encrypted loopback receiver, and reports sender/receiver totals. It requires working native codecs and is intentionally opt-in:

```sh
cargo test --lib rtmp_session::tls::tests::loopback_tls_native_encoder_acceptance -- --ignored --exact --nocapture
```

Retain the CI log or local JSON report, platform/OS, commit SHA and exact test name. A passing framing test is not receiver decoding, external RTMPS interoperability, physical capture, A/V lip sync or a two-hour soak. The native test does not use a physical camera, display or microphone. Continue the staged private receiver tests in LIVE_SOAK_DIAGNOSTICS.md before production activation. No actual public broadcast, installer execution or release initiated.

## Independent decoded continuity check
On Mac or Windows with the native Rust toolchain, Python 3 and separately installed FFmpeg/ffprobe, run from the repository root:

```sh
python3 scripts/verify-native-rtmps-decode.py
```

The opt-in native test encodes 20 synthetic video frames and a 440 Hz stereo tone. The receiver writes its independently parsed media as FLV only when the explicit MOOBEE_RECEIVER_FLV path is set, with create-new file semantics and a 16 MiB cap. The Python harness supplies a private temporary directory and removes it on exit. It checks the sender/receiver accounting reported by the Rust test, independently probes decoded H.264/AAC frames, verifies 640x360 video, 48 kHz stereo, 20 decoded video frames, matching AAC block counts, 1024 samples per AAC block, nominal zero-based per-track cadence and a maximum 200 ms encoded tail difference. It forces full FFmpeg decoding with fail-on-error and prints DECODED_RECEIVER_REPORT JSON. The 200 ms tolerance is an initial diagnostic bound for the two-second synthetic fixture, not a production lip-sync threshold. No video/audio content comparison or physical source timing measurement is claimed. FFmpeg is strictly an optional QA dependency, not integrated into MooBee's runtime. The regular CI checks validate the FLV framing and mocked report decisions; the actual native encode-and-decode harness is opt-in and not yet executed. No public stream, installer or release.
