# Native A/V integration — combined development checkpoint

Status: implementation branch, compile checks and installed-device testing
pending. This is not a production release.

## Integrated capture

- Saved Program grid/PiP and ordered display sources (already merged in #38).
- macOS optional default camera via AVFoundation, up to three displays plus
  webcam inset. Explicit capture opt-in; no camera in the still preview yet.
- Static transparent PNG graphic (top-left), selected locally by the user,
  downscaled and transmitted as bounded RGBA. Continuous AVI and MP4 only.
- macOS optional default microphone via AVAudioEngine and AVAudioConverter,
  bounded 48kHz PCM queue, gain/clipping statistics and AAC muxing with
  native H.264. Both camera and mic stop when the short diagnostic ends.
- Nominal rational video frame and audio sample-index timeline (not device
  PTS). A 4-second test expects exactly 192000 PCM frames per channel.
- Mac MP4 supports 360p10, 720p15 and experimental 1080p30 (2–4 seconds,
  120-frame cap). Lower profiles retain 2–8 seconds. Windows AVI stays silent.
- H.264/AAC FLV packet framing for a later RTMP session; **no network
  connection, stream key or live publishing is implemented by this module**.

## Debug in order

1. Build and launch native Mac and Windows installers; inspect privacy usage
   descriptions and confirm camera/mic are not requested on app startup.
2. Display-only capture, each profile. Check full decoding, motion, aspect
   ratio, timing and late-frame metrics.
3. Test webcam alone with a display, including permission denial, disconnect,
   first-frame timeout and privacy indicator release.
4. Test a transparent PNG overlay; validate actual encoded pixels and alpha.
5. Test microphone alone: permission, audible speech, expected AAC codec,
   48kHz stereo, PCM peak/clipping, gain and mute by setting gain to zero.
6. Combine display+camera+graphic+microphone; compare real-world clap/lip
   sync to decoded timestamps. The sample-count clock alone does not
   establish synchronization. Exercise 1080p30 separately.
7. Test failure cleanup and persisted clip playback. Run synthetic encoder
   fixtures explicitly on a suitable Mac:
   `cargo test --lib program_mp4_sink::tests -- --ignored`.
8. Inspect output offline using ffprobe and ffmpeg full decode. These tools
   are test tools only, not runtime encoder or capture dependencies.

## Remaining implementation and release gates

- System audio source and synchronized multi-input mix (not just mic gain).
- Device selection and live webcam/image/text/alert scene layers; scene
  transitions during recording; bounded long-running compositor.
- Windows native H.264/AAC, camera and microphone parity.
- Encoded packet extraction, authenticated RTMPS transport, reconnect and
  congestion handling, secure key storage, destinations and multistream.
- Sustained 30-minute 1080p recording and 2-hour authorized RTMPS test,
  signing/notarization and trusted updates.

Keep main Record and Go Live gated. Report only verified results and
sanitized metadata under issues #14 and #18. No production beta claim.
