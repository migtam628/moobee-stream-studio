# MooBee Stream Studio — v0.4.0 Beta Promotion Plan

**State on 2026-11-23: beta preparation only. No full streaming beta approved or published.**

The source of truth for gate tracking is [BETA_READINESS.json](BETA_READINESS.json).
Run `node scripts/check-beta-readiness.mjs` for a readable report,
`node scripts/check-beta-readiness.mjs --json` for a machine-readable report,
or `node scripts/check-beta-readiness.mjs --enforce` to require all
engineering evidence metadata to be present. A successful metadata audit
**does not** certify a functioning physical device, code signing or media quality;
a responsible maintainer must still review the underlying evidence.

## Supported intended beta scope

The first full broadcasting beta will target supported macOS and Windows
desktop systems with native, in-house video/audio capture and secure RTMPS
output. Linux, Android, and the self-hosted web edition can continue as
separately labeled experimental/controller clients; a packaged installer for
those platforms is not evidence of equal native streaming capability.

## Current checkpoint

- Native macOS 14+ H.264 direct-screen MP4 test recording is implemented on
  the feature branch, including an AVAssetWriter Sonoma fallback. A user
  reported recording after macOS permission recovery. Formal real-device
  decoding, duration, repeated capture and audible audio acceptance have not
  all been signed off.
- The native multi-display preview compositor assembles **still images**.
  It does not produce continuous encoded Program output.
- Native Windows H.264/AAC capture, webcam + overlay Program encoding,
  synchronized production audio, live RTMP/RTMPS transport, secure stream-key
  management and trusted desktop signing remain beta blockers.
- PR build checks that compile and package applications are valuable
  engineering evidence but **not hardware acceptance**.
- The primary Record and Go Live buttons must not be enabled until their
  corresponding end-to-end feature has passed production-quality QA.

## Required before v0.4.0-beta.1

1. **Native Program output** — continuous timestamped Program frames including
   scene switches, enabled display sources, webcam PiP and overlays, tested
   independently of the still preview.
2. **Audio** — native mic and system-audio capture, mix, meters, drift correction
   and expected mute behavior; verify encoded tracks with FFprobe and by ear.
3. **Native recording parity** — 30-minute seekable H.264/AAC recordings at
   1080p30 on Apple Silicon Mac and Windows 11 x64, full-file decode, measured
   frame drops, A/V sync and recovery from permission, device and disk errors.
4. **Broadcast transport** — 2-hour RTMPS test to a destination authorized
   by the repository owner; confirm receiving-side audio/video, reconnect and
   clean stop. Never publish or log stream URLs with keys.
5. **Security** — protect keys in a platform secret store; local-only API
   remains bound to loopback until authenticated, TLS-protected remote control
   passes security review.
6. **Distribution** — macOS Developer ID signing/notarization and Windows
   Authenticode signing verified on clean installations. Configure CI
   credentials through secured secrets, not checked-in files.
7. **Release gates** — full PR test suite green, hardware issue links attached
   to every verified gate, a reviewed compatibility matrix, release notes,
   artifact hashes, and separate beta tags (never overwrite an existing tag).

## Hardware evidence format

For each physical-machine test, add the sanitized report to
[Issue #14](https://github.com/migtam628/moobee-stream-studio/issues/14)
and any codec-specific evidence to
[Issue #15](https://github.com/migtam628/moobee-stream-studio/issues/15):

- Exact app release tag and commit SHA; OS version and architecture.
- Feature and permissions requested, available sources/devices and test steps.
- Expected versus actual duration, resolution/FPS, encoding format and audio
  streams. For media, include offline FFprobe + full FFmpeg decode results
  (see [Mac acceptance guide](MACOS_MP4_ACCEPTANCE.md)).
- Real playback/seek and audible A/V synchronization result, and any errors.
- Memory/CPU/thermal, dropped-frame metrics and repeated-run behavior.
- Link to a non-sensitive test summary. Do **not** post private captures,
  streaming credentials, private screen content or full home directory paths.

## Promotion workflow

- Keep active engineering changes in reviewed PRs; do not merge unfinished
  implementations merely to change the displayed product channel.
- When all eight gates in `BETA_READINESS.json` have real evidence, set the
  relevant statuses to `verified` and link the evidence under the corresponding
  GitHub issue/PR. The audit checks references, **not their truth**.
- Require an independent human review of each linked test record and verify
  valid signing identities on the actual installers. Store the approval in
  the release review record.
- Only then implement/enable the dedicated signed beta publishing path and
  create an immutable `v0.4.0-beta.1` tag with supported-platform assets.
  Never repurpose unsigned alpha DMGs, debug APKs or silent AVIs as beta media
  acceptance.
- Follow with a release-candidate soak/security/license audit before stable.

The existing `release/v*-alpha.N` workflow remains intentionally restricted
to experimental alpha publication. The new readiness auditor is informational
for ongoing alpha work; `--enforce` is available for the later beta promotion
gate and must be paired with manual acceptance/signing verification.
