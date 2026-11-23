# macOS ScreenCaptureKit MP4 — Hardware acceptance

## Purpose and scope

This experiment adds a **native H.264 MP4 screen recorder on macOS 15+** with optional system audio and optional microphone requested through ScreenCaptureKit. It compiles Swift into the installed Tauri binary, without OBS, Streamlabs or an FFmpeg capture executable. It does **not** yet record the MooBee program scene, webcam layer or RTMP stream.

The Rust command records for a user-selected maximum of **3–120 seconds** (or stops earlier when requested) into a private `.mp4.part` file and publishes the finished `.mp4` only after the ScreenCaptureKit completion delegate, bounded output-size checks, top-level MP4 box validation (`ftyp`, `moov`, non-empty `mdat`, and box sizes), and file synchronization. A successful command means the file was finalized, **not** that microphone/system audio was independently verified.

## Screen Recording permission recovery (Sonoma 14.2)

If a capture fails with `com.apple.ScreenCaptureKit.SCStreamErrorDomain:-3801`
or `The user declined TCCs for application, window, display capture`,
the immediate blocker is **macOS Screen Recording consent**, not proof of a
codec or AVAssetWriter failure.

1. Quit MooBee completely. Go to **System Settings → Privacy & Security →
   Screen Recording** (later macOS may label it Screen & System Audio Recording).
2. Enable **MooBee Stream Studio**. If already enabled, switch it off and on,
   then quit and reopen MooBee from **/Applications**.
3. If no MooBee entry appears, or the stale consent persists, first verify its
   installed bundle ID: `defaults read "/Applications/MooBee Stream Studio.app/Contents/Info" CFBundleIdentifier`.
   The expected ID is `com.mttechservices.moobeestreamstudio`.
4. Only after verifying that ID, reset **MooBee's** screen-capture consent:
   `tccutil reset ScreenCapture com.mttechservices.moobeestreamstudio`.
   This does not grant access; it clears the decision so macOS can prompt again.
   Do **not** use `tccutil reset ScreenCapture` without an app ID, which resets
   unrelated applications' access.
5. Reopen MooBee, allow Screen Recording when macOS prompts, and retry a
   15-second silent MP4. If the exact -3801 error persists, note whether the
   new beta binary was launched from /Applications or another folder and
   preserve the error string.

The Studio recovery panel now recognizes -3801 and offers a shortcut to the
macOS Screen Recording privacy settings. It cannot bypass TCC or grant access.

## Sonoma 14.2 first-party MP4 fallback (experimental)

macOS 14 does **not** provide `SCRecordingOutput`. MooBee's experimental
fallback registers `SCStreamOutput`, receives ScreenCaptureKit video and
system-audio sample buffers and writes H.264/AAC MP4 using `AVAssetWriter`.
The full program scene, webcam overlay and RTMP broadcaster are not included.

- [ ] On the user's **macOS 14.2 Apple Silicon MacBook Pro**, confirm Settings
  reports OS `14.2.x`, MP4 engine `sonoma-avassetwriter`, and an available
  test backend. Confirm the microphone checkbox is disabled with a macOS 15+
  explanation.
- [ ] Grant macOS Screen Recording permission when requested; restart MooBee
  when required by the system privacy dialog.
- [ ] Record a **15-second screen-only MP4**, with microphone and system audio
  unchecked. Verify H.264, roughly expected duration, motion, full decode,
  QuickTime playback, and seeking. Do not infer success from file existence.
- [ ] Start a **60-second maximum MP4** and click **Stop and save MP4**
  after about 15 seconds. Verify that only a finalized file is published,
  contains nonzero video frames and remains playable/seekable.
- [ ] Request **system audio** and separately verify an AAC track and audible
  synchronized content, then repeat with no audio requested and confirm no
  unwanted track. Mac 14 does not capture microphone through this pathway.
- [ ] Repeat recording twice, restart the application, and reopen clips;
  deny screen permissions and verify the app surfaces an actionable error.
- [ ] Monitor CPU/dropped frames and memory usage. A two-minute bound is
  **not** equivalent to a 30-minute/2-hour production recording acceptance.
- [ ] Keep full studio Record/Go Live gated until composed frames, microphone
  mixing, secure stream credentials and RTMPS output have been implemented.

If the file fails decoding or a callback reports an error, keep the diagnostic
log and any partial `.mp4.part` out of the published clips list. Never call the
experimental recorder beta-ready based only on GitHub compile status.

## macOS compatibility validation

The MooBee desktop shell is now packaged with macOS **13.0+** minimum deployment
metadata. Native `SCRecordingOutput` MP4 remains available only on **macOS 15+**.

- [ ] Launch the correct ARM64/Intel app on a real macOS **13 Ventura** machine.
  Confirm scene editor, screenshot/GIF/AVI test tools and disabled MP4 control.
- [ ] Repeat on macOS **14 Sonoma**; the app must not crash while resolving the
  weakly linked macOS 15 ScreenCaptureKit recording symbols.
- [ ] On macOS **15+**, confirm MP4 control enables and complete the playback/audio
  tests below.
- [ ] Verify `LSMinimumSystemVersion=13.0` and Mach-O deployment metadata in
  CI's installed app bundle. CI cannot replace actual 13/14 launch tests.

## Required manual tests (perform on a real Mac)

- [ ] Install on **macOS 15 or later**, with Screen Recording permission granted when requested in System Settings > Privacy & Security. Verify the installed app's Info.plist includes the permission reasons.
- [ ] Test **Apple Silicon and Intel** separately if both are supported. Record OS version, CPU, screen resolution/scale and connected display IDs.
- [ ] In Clips choose one display and a **4-second** capture. Leave microphone off first; system audio may be enabled while playing known, permitted sound.
- [ ] In **Studio → Quick screen capture**, test 15-, 30-, 60-, and 120-second maximum recording sessions in sequence. Observe requested vs actual runtime; stop the 120-second session early (after approximately 15 seconds) with **Stop and save MP4**, wait for finalization, then open the resulting file.
- [ ] Verify that an early stop produces a finalized, seekable MP4 and that pressing Stop before or after a session does not leave a corrupted public file. Actual file duration must be measured independently and must not be inferred from the requested maximum.
- [ ] Repeat start/stop several times and test low disk space, permission rejection, screen disconnect, and forced app closure. A transient `.mp4.part` must never be presented as a completed clip.
- [ ] Open the saved `.mp4` in QuickTime and VLC. Ensure correct monitor captured, recognizable motion, duration and seekability.
- [ ] Probe video/audio using a separate diagnostic copy of FFprobe (not needed to record):

```bash
ffprobe -v error -show_entries format=duration,size:stream=index,codec_type,codec_name,width,height,avg_frame_rate,sample_rate,channels -of json "/path/to/clip-UUID.mp4"
```

- [ ] Confirm **H.264 video**, plausible frame rate and dimensions, file duration within a documented tolerance. Check whether the **audio** stream is actually present; an audio checkbox does not guarantee that it is.
- [ ] Run the repeatable **offline** metadata and full-file decode acceptance check (Node.js + locally installed FFprobe/FFmpeg; not dependencies of MooBee itself). Supply the duration actually requested, and include `--expect-audio` only when audio was enabled:

```bash
node scripts/verify-native-mp4.mjs --file "/path/to/clip-UUID.mp4" --seconds 4 --expect-audio --report macos-mp4-evidence.json
```

  The JSON report contains the base filename, stream details, duration, decode result and explicit `hardwarePlaybackVerified: false` / `audioSyncVerified: false` fields. Do not claim those last two checks passed until you manually watch/listen. The script does **not upload** footage or evidence and refuses to overwrite an existing report. Redact local filenames and machine details before sharing evidence.
- [ ] Record again with a microphone enabled after granting microphone permission; confirm both expected audio sources and measure sync against on-screen clicks and spoken taps. Report missing or corrupted audio.
- [ ] Connect two monitors and confirm MooBee records exactly the selected display. The bridge maps XCap monitor identity to ScreenCaptureKit's macOS display ID rather than relying on array ordering.
- [ ] Deny/revoke screen or mic permission, unplug a selected display, interrupt capture and repeat capture. Confirm errors do not produce a completed file or a false-success UI message.
- [ ] Confirm a failed capture doesn't leave a public `.mp4`; hidden `.mp4.part` files may require manual cleanup after abrupt termination/timeouts.
- [ ] Inspect CPU/GPU, memory and thermal use; the maximum is a bounded two-minute test and is **not** proof of a 30-minute production recording or thermal stability.

## Important limitations

- Mac-only; Windows MP4 and microphone/system audio are **not implemented** in the native engine.
- Output uses Apple's H.264 screen recording API; no camera/source overlay, compositor, saved scene selection or instant replay.
- System + microphone audio mixing differs across OS versions and may fail or produce an unusable file; do not assume success without inspecting decoded tracks.
- Stream keys, recording permissions, code signing, updates, audit logs and privacy policy need production security work.
- The main **Record** and **Go Live** buttons must remain disabled until production-quality recording and RTMP pass supported-device acceptance.

Report evidence in [Hardware Acceptance Issue #14](https://github.com/migtam628/moobee-stream-studio/issues/14) and [Native MP4 Issue #15](https://github.com/migtam628/moobee-stream-studio/issues/15).
