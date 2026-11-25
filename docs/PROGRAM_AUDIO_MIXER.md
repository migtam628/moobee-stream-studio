# Program audio mixer — development milestone

The Mac Studio Audio Mixer supports selecting named AVFoundation audio input
devices independently for Mic/Aux and HDMI/USB capture-card audio. HDMI audio
must be exposed by the card as a compatible macOS audio input; video-device
discovery does not establish audio availability. Desktop audio uses
ScreenCaptureKit with a low-resolution discarded screen stream, 48 kHz
stereo audio, and excludes MooBee's own process audio.

Each selected source is converted to float stereo 48 kHz PCM, with bounded
queues. Startup preroll is discarded before mixing. Independent gains and
mute controls feed the first-party mixer; per-source post-gain peak, mixed
peak and clipping count are shown by explicitly enabling live meters. Meter
capture is local, no playback monitoring or network output. Idle meter
sessions expire; leaving Studio or turning meters off stops them. Before
recording the UI releases meters and video preview, then opens the selected
sources for the composed Mac MP4 H.264/AAC diagnostic.

The shared sample-index timeline ensures requested PCM sample counts match
nominal video duration. This is NOT device-clock synchronization: input PTS,
resampling drift compensation, dropout recovery and latency calibration
are still needed. Queue overflow or underrun fails the short recording,
rather than silently inserting fabricated audio. Gain/mute selections are
snapshotted at recording start; controls and live meter are not yet a
real-time recording mixer. Windows audio capture is not implemented.

## Test order (when consolidated installer testing is authorized)

1. Enumerate devices with a USB capture card, microphone and built-in mic.
   Confirm selection uses the exact input ID, not a display index.
2. Test live meters separately: microphone speech, console gameplay audio,
   desktop audio. Confirm mute and gain change measured levels. Verify
   camera/microphone privacy indicators are released when leaving Studio.
3. Enable two named audio devices and desktop audio. Confirm each meter
   responds to the intended signal, duplicate devices are rejected and
   clipping is reported. Avoid feedback and duplicated system audio.
4. Record a baseline 4-second MP4. Expect 192000 PCM frames per channel;
   verify AAC 48 kHz stereo, full decode, actual audible source content and
   measured A/V sync. Repeat with each source alone and all sources mixed.
5. Test unplugging USB, permission denial, no samples, repeat captures,
   preview and meter handoff, and recording cleanup after failure.

CI compiles native Swift/Rust and exercises synthetic mixer and simulated
frontend IPC. It does not establish physical hardware compatibility,
desktop capture permissions, audio latency, or recording playback.

Do not create new release branches or installer release workflows until
the user authorizes a consolidated milestone. Existing release workflows
remain unchanged; the macOS acceptance app packaging step is manual only
on this integration branch. Main Record/Go Live remain gated.
