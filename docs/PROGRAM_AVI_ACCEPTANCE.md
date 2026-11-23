# MooBee native composed Program AVI — experimental acceptance

**Status: source implementation awaiting PR CI and real-device playback tests. This is not a streaming beta.**

This feature is a bounded first-party proof of **source selection → native
multi-display capture → grid composition → JPEG encoding → seekable local AVI**.
It shares the same still-frame compositor layout used in the Studio preview.
It is **not** synchronized real-time Program video. Captured screenshots are
sampled sequentially; there is no webcam, alert/image/text layer, audio,
hardware H.264/AAC encoding, or RTMP/RTMPS broadcaster.

## How to test on macOS or Windows

1. Install a fresh experimental build containing this PR. Grant Screen
   Recording permission on macOS. On Windows confirm access to the target
   displays. Note OS version, CPU, app release/commit and display layout.
2. In Studio create or select a **Program** scene and add one or two enabled
   display sources (up to four are supported). If Studio Mode is on, select
   a **different Preview** scene so the distinction can be tested.
3. Under Studio → Quick screen capture select **Capture Program AVI test**.
   Choose 4 seconds. The app should capture the selected *Program* sources,
   not the Studio Preview, without accidentally enabling Go Live or Record.
4. Check the saved local AVI in Clips. Inspect in VLC or a player supporting
   MJPEG AVI. Confirm each display appears in the expected bounded 640×360
   grid, retains its aspect ratio, and has moving content. Check seeking and
   end-of-file playback.
5. For a second test, disable one source and repeat. Verify that the output
   changes. Try an empty Program scene, a scene containing five enabled
   sources, an unplugged display and an invalid device ID; these must return
   actionable errors rather than silently producing an incorrect file.
6. During the test attempt an ordinary capture; the exclusive capture guard
   should prevent overlapping recordings. Deny screen permission and confirm
   no public clip is marked complete; failed capture temporary files should
   not be listed in Clips.

## Independent playback/decode validation

On your test machine, run the commands below on the finalized **AVI**. The
FFmpeg tools are for **offline testing only**, not used to capture the video.

```bash
ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,avg_frame_rate,nb_frames -of json "/path/to/clip-ID.avi"
ffmpeg -v error -xerror -i "/path/to/clip-ID.avi" -f null -
```

Expected: MJPEG, 640×360, 10 fps **nominal encoded timeline**, 40 frames for
4 seconds, and no audio. Separately compare the **measured sampling FPS and
wall-clock elapsed duration shown by MooBee**: if capturing two displays is
slow, real capture may exceed the AVI timeline. This test does not establish
timestamp synchronization or 1080p30 capability.

Upload only sanitized metadata, passed/failed tests and environment details
to [hardware acceptance issue #14](https://github.com/migtam628/moobee-stream-studio/issues/14)
and link the result from [beta tracker #18](https://github.com/migtam628/moobee-stream-studio/issues/18).
Do not upload personal screenshots, recorded screen content, credentials or
private home folder paths.

## Remaining full beta gates

- Continuous GPU or timestamp-aware compositing at 1080p30, not repeated
  sequential screenshots, with scene transitions and webcam/overlay layers.
- Encoded H.264/AAC Program recording with synchronized microphone/system audio
  and validated 30-minute stability on Mac and Windows.
- Secure stream-key management, RTMPS broadcast to an authorized receiver,
  reconnection and two-hour soak tests.
- Signed/notarized distribution and release acceptance.

Until these pass, the primary **Record** and **Go Live** controls stay disabled.
