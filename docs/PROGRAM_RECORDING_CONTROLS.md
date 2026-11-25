# Program MP4 pause/resume and live statistics (experimental)

Mac Program H.264/AAC diagnostic only. The active recording cap remains 30 seconds, with at most 30 nominal seconds of paused input and an active-limit-plus-30-second wall-clock cutoff checked between complete frame ticks. Startup and encoder finalization may take additional time. AVI limits are unchanged.

Pause leaves devices open and drains their queues, but does not submit paused video or audio to the encoder. The nominal media clock advances only on written A/V ticks. Stop works while paused. Commands and status are scoped to a unique session ID; stale commands cannot affect the next recording. Permission refusal, source stalls, audio failures and excess file size remain errors, not completed clips. The existing private temporary file is published only after final container validation. No interrupted-file recovery is promised.

Live counters are native snapshots polled by the UI: submitted frame count, nominal media duration, elapsed capture time, nominal pause time, buffered on-disk bytes and approximate file-average bitrate, late output slots and repeated source slots. These are not independently decoded frames, transport bitrate, true device-drop counts or A/V sync measurements. File size may lag the writer. The 64 MiB safety limit is checked during recording and after finalization.

## Hardware acceptance (not yet performed)

1. On macOS, choose 1080p30 and 30 seconds, enable a named microphone and optional HDMI/desktop audio. Record a visible clock and audible periodic cue.
2. Pause for five seconds; verify native phase changes and media frame count freezes while wall time advances. Resume and record a distinct cue. Stop early; verify saved file and absence of paused interval, with continuous playback and AAC.
3. Repeat stop while paused, repeated pause/resume, automatic duration/paused-budget cutoff, permission refusal, device disconnect and output failure. A failed capture must not appear as a completed clip.
4. Inspect duration, codec, streams and timestamps with FFprobe and decode the entire file. Compare visible/audio cues before and after pauses and check drift. Record CPU, memory, late/repeated slots and actual bytes.
5. Test all supported profiles, compare file counters against actual saved size, and repeat on Apple Silicon and Intel where supported. Windows AVI regression must pass; this does not enable Windows MP4.

No beta promotion, new release branch or installer generation is requested by this change.
