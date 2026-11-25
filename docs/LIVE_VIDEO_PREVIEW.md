# Native live video preview — experimental macOS milestone

Studio → Preview refresh → Live video inputs (Mac) explicitly opts in to
camera/capture-card sessions. Select a named USB/UVC capture card in Sources
and add it to a scene. Optionally add a different named webcam. Hide other
sources. Grid shows the two video feeds side by side; Picture-in-picture
uses the first enabled video source as primary and the second as an inset.
Use Studio Mode and Take to verify independent Preview and Program scenes.

The persistent native AVFoundation sessions retain the latest source frame.
Preview and Program share sessions by stable device ID, with at most two
distinct active video devices across both monitors. The CPU-composited
preview is 640×360 JPEG via local Tauri IPC with an upper target of 10 fps.
It is not a 1080p60 GPU preview, encoded Program output, or a stream. Native
source geometry remains limited to 720p or lower. Actual fps depends on
hardware and encoding/IPC performance. Display-only scenes still use
periodic still images. Mixed display+video scenes are not supported here.

Privacy and lifecycle: preview is opt-in and local; no file writes or network
upload. Pause, leave Studio, hide the window, or switch to another refresh
mode to release sessions. An inactive worker also expires after five
seconds without requests. Startup permission requests can take longer.
Before native recording or signal testing, all preview workers are stopped
and joined; preview automatically resumes after recording. The native
capture lock prevents overlapping preview and recording capture ownership.
Existing recording remains single-video-source; hide the second source to
record a capture-card test. Simultaneous card+webcam preview does not imply
simultaneous recording or embedded HDMI audio.

## Hardware acceptance

1. Connect console HDMI OUT to card HDMI IN and USB to Mac. Use a supported
   SDR input mode. Refresh video devices. Add the capture card.
2. Select Live video inputs. Grant Camera permission if required. Confirm
   moving gameplay, not just the card's no-signal image.
3. Add a distinct webcam, choose PiP, and confirm both feeds move and source
   order is correct. Verify both Studio monitors can share the same device.
4. Pause, leave Studio, and reconnect USB. Check camera indicator release
   and stale-frame errors. Pause/resume to restart after a failed session.
5. Hide the second device; record a short MP4 with the card as sole enabled
   Program source. Confirm preview releases and resumes, and play the clip.
6. Repeat on Apple Silicon and Intel. Record actual observed latency, frame
   update rate, CPU usage and failures without uploading personal video.

Automated tests cover invalid device IDs and simulated UI/IPC flow. They
do not prove physical hardware compatibility, measured performance, live
audio, or A/V sync. Windows live UVC support, HDMI audio, recording both
devices, full-rate rendering and RTMPS remain separate milestones.
