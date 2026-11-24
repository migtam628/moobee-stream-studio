# Composed H.264 MP4 — macOS experimental acceptance

This diagnostic feeds the same native multi-display Program composition used
by the continuous AVI recorder into MooBee's in-process Swift AVFoundation
H.264 writer. It does not use an OBS, Streamlabs or FFmpeg capture process.
It is separate from the existing macOS single-display ScreenCaptureKit MP4
recorder. It is **video only**, not a production recorder or streamer.

## Supported test scope

- Installed Mac app (Apple Silicon or Intel); capture permission and a
  working native H.264 encoder are required. Hardware acceptance is pending.
- One to four enabled display sources in the selected Program scene.
- Baseline: 640×360, nominal 10fps; higher detail: 1280×720, nominal 15fps.
- 2–8 seconds; no more than 120 composed frames; finalized MP4 must be at most 64 MiB.
- No audio, webcam, overlays, transitions, native source PTS, or streaming.
  Nominal frame-index presentation times do not guarantee wall-clock
  accuracy or cross-device synchronization.

## Installed app acceptance

1. In Studio, select a Program scene with two enabled display sources. Set
   a different Preview scene if Studio Mode is enabled. On one-screen
   machines, a single-display Program scene is sufficient for an initial test.
2. In Quick Screen Capture, choose four seconds and the baseline profile.
   Click **Composed H.264 MP4 (Mac test)**. Move visible windows on the source
   displays. Approve system Screen Recording permission if needed.
3. Confirm the saved clip appears in Clips; open it in QuickTime or VLC.
   Confirm actual display content, aspect ratio, motion, playback, seeking
   and absence of audio. Repeat for the higher-detail profile.
4. Independently probe and fully decode the clip:

   ```bash
   ffprobe -v error -show_entries format=duration,size:stream=codec_name,codec_type,width,height,avg_frame_rate,nb_frames -of json "/path/to/clip-ID.mp4"
   ffmpeg -v error -xerror -i "/path/to/clip-ID.mp4" -f null -
   ```

   Four-second baseline target: H.264 640×360, nominal 10fps, 40 frames,
   one video stream, no audio. Higher detail: H.264 1280×720, nominal
   15fps, 60 frames, one video stream, no audio. Compare measured capture
   time and finalization time; they are distinct from the container duration.
5. Test permission denial, disconnected display, an unsupported Program
   source, and another capture already running. An incomplete output must
   never appear in Clips. A timed-out encoder may leave a hidden partial
   file for diagnosis; do not rename it while the writer is active.
6. For the native synthetic encoder test, run on a Mac with working H.264:

   ```bash
   cd apps/desktop/src-tauri
   cargo test --lib program_mp4_sink::tests -- --ignored
   ```

   This test composes moving synthetic source images and checks a finalized
   MP4 container and duration, not real monitor capture. Ordinary CI runs
   the FFI profile-rejection test and compiles the synthetic test; it does
   not imply that an installed-device encoding test has passed.

Submit sanitized metadata, OS and architecture, app commit/tag, and
pass/fail results to [hardware acceptance #14](https://github.com/migtam628/moobee-stream-studio/issues/14).
Do not post personal screen recordings or private paths.

The main Record and Go Live controls remain disabled. Full beta still
requires timestamp-aware sustained 1080p composition, native Windows H.264,
camera/overlay layers, synchronized AAC audio, authorized RTMPS output,
secure credentials, signing, and hardware acceptance.
