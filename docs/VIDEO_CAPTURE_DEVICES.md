# macOS video capture devices — hardware acceptance

MooBee enumerates AVFoundation video devices, including compatible USB UVC HDMI capture cards, by name and stable device ID. Discovery does not open the camera. This does not guarantee that proprietary-driver cards are supported.

1. Connect the capture card to the Mac via a data-capable USB port, and the console HDMI output to the card HDMI IN.
2. Set the console to a supported signal (start with SDR 1080p60, no VRR/HDR if necessary).
3. Open Studio → Sources; press Refresh capture devices. Choose the device under Video capture devices.
4. Click Test video signal. Grant macOS Camera access if requested. Verify the still shows actual gameplay; a received frame alone does not prove HDMI signal (some cards output a no-signal slate).
5. Add source. In Program, enable only the selected video device, disable the separate webcam inset, and choose a short Mac MP4 recording test. If Studio Mode is on, Take the scene into Program.
6. Play back the recorded file and verify console motion. Unplug the card, refresh, and verify no stale device is substituted. Reconnect and repeat.

The first implementation supports a single selected native video input for bounded Mac MP4 tests, with optional existing microphone AAC and PNG overlay. Input capture is bounded to 720p or lower; a larger encoded profile does not increase native input resolution. Still preview shows the last manually tested frame, not live video. USB/HDMI embedded audio, simultaneous display+card/webcam scenes, Windows capture-card backend, full-rate live preview, and broadcasting remain pending.

Actual device compatibility and playback are not established by simulated browser IPC tests or native compilation.
