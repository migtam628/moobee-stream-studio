MooBee Live 0.2 is an **early alpha** combining a Tauri desktop/mobile shell with an OBS Studio control dashboard and a self-hosted web companion.

Included today: OBS WebSocket connection, scenes, source visibility, audio mute/volume, stream/record controls, still scene snapshots, animated test alerts and the desktop browser-source overlay.

- **macOS:** DMG for Apple Silicon and Intel, ad-hoc signed; not Apple notarized.
- **Windows:** unsigned NSIS setup EXE; Windows SmartScreen may warn.
- **Linux:** x64 Debian `.deb`; requires compatible system libraries.
- **Android:** ARM64 **debug-signed test APK** for controlling OBS on the same trusted LAN. Not a Google Play production build; the Android device does not capture/encode the stream.
- **Web:** `.tar.gz` containing the dashboard and local Node API. Extract, run `npm ci`, then `npm start`, and open `http://127.0.0.1:8787`. The separate public GitHub Pages website is a preview without an OBS backend.

OBS Studio must be installed, configured and running on the streaming computer. Test alerts are synthetic; real platform alerts, AI automations, multistream relay and native capture are not implemented yet. Keep OBS WebSocket authentication enabled. Do not expose the local web API directly to the public internet.
