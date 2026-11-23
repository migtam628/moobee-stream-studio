# Continuous Program Input — First Native Multi-Display Prototype

**Experimental source ingest only. Not real-time Program output, not a streaming beta.**

The installed Mac/Windows app now has two independent Program AVI diagnostics
under Studio → Quick Screen Capture. Both select the **current Program** scene,
not the Studio Mode Preview scene. Both are 2–8 second, silent,
640×360, nominal-10fps MJPEG AVIs; neither requires OBS or Streamlabs.

| Diagnostic | Native input | What it proves |
| --- | --- | --- |
| Sampled Program AVI | Synchronous screenshots from 1–4 displays | Source selection, grid geometry and local indexed video file creation |
| Continuous-source Program AVI | XCap's per-display native video frame receivers (work in progress) | Continuous input callbacks, bounded per-tick frame drain, latest-frame scene composition, capture/encoding timing and error recovery |

For the continuous-input test, MooBee starts one native frame receiver per
display, waits for initial data, then encodes the newest complete frame from
each input at a nominal ten-frame-per-second output schedule. On every tick it
drains at most 64 queued frames per source. It keeps only the latest selected
source frames for rendering and the current encoded JPEG frame, and it refuses
to publish an AVI if an input disconnects or becomes stale (over 1.5 seconds).
Capture is bounded to four sources and a combined 36-million-pixel input
budget. The queued frame allocation inside the upstream XCap implementation
is **not** under MooBee's direct control.

Timing diagnostics include wall-clock duration, observed output sampling FPS,
source frame counts, repeated input slots, late composite frames, and maximum
skew between **frame arrival times**. Arrival times are not source capture
presentation timestamps. No native video clock, cross-device synchronization,
accurate dropped-frame count, 1080p60 capability, or sound has been verified.
The AVI header retains a nominal 10fps playback timeline even if wall-clock
capture runs longer.

## Hardware acceptance

1. Run a Mac or Windows experimental build containing this feature. Record
   app SHA, OS, architecture and monitor layout. Grant system capture consent.
2. In Studio, place two enabled display sources in the Program scene and a
   different source in Preview. Run both diagnostics for four seconds.
3. Confirm the continuous-source clip shows **Program**, not Preview. Verify
   expected source layout, aspect ratios, visible motion, playback/seeking and
   complete decode using VLC and FFmpeg.
4. Compare the two diagnostics' wall-clock elapsed time, measured sampling FPS
   and repeated-source slots. Test high-motion content, different resolutions,
   one and two displays, and monitor hotplug.
5. Deny capture consent, disconnect a display, force an input stall and attempt
   overlapping recordings. Each failure must show an explicit error and not
   publish a completed clip. Restart MooBee and confirm saved clips persist.
6. Run `ffprobe -v error -show_entries format=duration,size:stream=codec_name,width,height,avg_frame_rate,nb_frames -of json "/path/to/clip-ID.avi"`
   then `ffmpeg -v error -xerror -i "/path/to/clip-ID.avi" -f null -`.
   For four seconds expect nominal 10fps, 40 MJPEG frames at 640×360,
   no audio. Report any actual duration discrepancy separately.
7. Share only sanitized QA metadata under
   [hardware acceptance #14](https://github.com/migtam628/moobee-stream-studio/issues/14),
   with a link from [beta tracker #18](https://github.com/migtam628/moobee-stream-studio/issues/18).
   Never upload private screen content or credentials.

## Beta gap

This does not replace the required timestamp-aware 1080p30 or 1080p60
scene compositor, webcam/overlay layers, H.264/AAC Program encoder,
microphone/system audio sync, two-hour RTMPS validation, secure keys or
code-signed installations. The primary Record and Go Live controls stay
disabled. A successful CI compile, a playable silent AVI, or a single
successful hardware test is **not** full beta acceptance.
