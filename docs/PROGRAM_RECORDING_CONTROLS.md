# Program MP4 pause/resume and live statistics (experimental)

Mac Program H.264/AAC diagnostic only. The active recording cap is 60 seconds, with at most 30 nominal seconds of paused input and an active-limit-plus-30-second wall-clock cutoff checked between complete frame ticks. Startup and encoder finalization may take additional time. AVI limits are unchanged.

Pause leaves devices open and drains their queues, but does not submit paused video or audio to the encoder. The nominal media clock advances only on written A/V ticks. Stop works while paused. Commands and status are scoped to a unique session ID; stale commands cannot affect the next recording. Permission refusal, source stalls, audio failures and excess file size remain errors, not completed clips. The existing private temporary file is published only after final container validation. No interrupted-file recovery is promised.

Live counters are native snapshots polled by the UI: submitted frame count, nominal media duration, elapsed capture time, nominal pause time, buffered on-disk bytes and approximate file-average bitrate, late output slots and repeated source slots. These are not independently decoded frames, transport bitrate, true device-drop counts or A/V sync measurements. File size may lag the writer. The 64 MiB safety limit is checked during recording and after finalization.

## Hardware acceptance (not yet performed)

1. On macOS, choose 1080p30 and 60 seconds, enable a named microphone and optional HDMI/desktop audio. Record a visible clock and audible periodic cue.
2. Pause for five seconds; verify native phase changes and media frame count freezes while wall time advances. Resume and record a distinct cue. Stop early; verify saved file and absence of paused interval, with continuous playback and AAC.
3. Repeat stop while paused, repeated pause/resume, automatic duration/paused-budget cutoff, permission refusal, device disconnect and output failure. A failed capture must not appear as a completed clip.
4. Inspect duration, codec, streams and timestamps with FFprobe and decode the entire file. Compare visible/audio cues before and after pauses and check drift. Record CPU, memory, late/repeated slots and actual bytes.
5. Test all supported profiles, compare file counters against actual saved size, and repeat on Apple Silicon and Intel where supported. Windows AVI regression must pass; this does not enable Windows MP4.

No beta promotion, new release branch or installer generation is requested by this change.

## Priority 4C: storage and bitrate safeguards

Select Automatic, 1.5, 3 or 6 Mbps H.264 target bitrate. Automatic retains 6 Mbps for Full HD and 3 Mbps for lower profiles. The target is not a guaranteed measured bitrate; AAC, container overhead and encoder variability also affect file size. The Mac Program diagnostic supports up to 60 active seconds (1,800 submitted frames at 30 fps), with the existing 30-second nominal pause allowance. The 64 MiB file cap is unchanged. Before opening the writer, check at least 96 MiB free on the destination volume; while submitting video, check for at least 16 MiB approximately each nominal second. Capacity checks are snapshots, not reservations. A disk or encoder failure remains an error and does not publish unfinished media.

The private UUID MP4 is finalized, structurally validated and synced, then staged under a hidden .ready.mp4 name before no-clobber publication. In Capture diagnostics choose Recover finalized Program recordings to retry publication of those ready files. Recovery rejects invalid files, ignores raw .part.mp4 files, and does not overwrite existing clips. A crash before the ready staging step or during encoding is NOT recoverable through this action. A structurally valid container still requires independent full media decode. This is finalized-file publication recovery, not fragmented MP4 or automatic segmentation. The existing application-managed Clips folder is unchanged.

Additional hardware acceptance: run a full 60-second test at each bitrate, check actual file size and decode; simulate low destination space and a failed publish; verify the ready file remains recoverable and the final clip cannot be overwritten. Test recovery of corrupt ready files and ensure raw partial recordings remain hidden. Do not force actual disk exhaustion on a system volume.

## Priority 4D: bounded segmented recording

Select a 2, 3, 4 or 5 minute Program MP4 test to automatically record separate, independently finalized 60-second clips. The native worker retains the capture lock and session ID for the sequence. At each boundary the previous clip is validated and published before the next opens; sources and encoder restart. **This is not gapless continuous recording**: capture gaps and audio timing discontinuities are expected at boundaries. The per-clip 64 MiB cap, disk checks, codec presets and existing source restrictions remain in effect. Pause/resume and the existing per-clip pause allowance operate within each segment. Stop ends the active clip gracefully (when possible) and prevents starting subsequent segments. A stop during a boundary can still permit the in-flight startup to produce a short final clip.

The results show the last clip's details plus a separate session summary containing ordered saved clip IDs, paths, bytes, media duration, frame counts and any later-segment failure. Live counters are per current segment, with segment number and count of previously finalized clips. If a later segment fails, already-published clips remain in the Clips library; a raw interrupted segment is not repaired or marked saved. A validated ready file may be recovered via the existing explicit recovery action. On process interruption the prior published clips remain discoverable individually; there is no persistent session manifest or automatic concatenation yet. The requested media duration excludes startup, finalization, pause and inter-segment gaps. No unrestricted duration or installer release is enabled.

Hardware acceptance: record 2-minute and 5-minute sequences at each supported profile with visible clocks and audible markers; verify all saved clips independently with FFprobe and full decode, quantify boundary gaps and lip sync, verify stop during recording/pause/boundary, and induce a later-segment device failure to confirm earlier clips remain. Verify restart behavior after a controlled interruption without destroying existing files. Check disk usage and the per-file cap. This remains a diagnostic feature until real-device evidence passes.
