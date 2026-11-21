# MooBee Stream Studio

**Standalone Alpha 0.3.2 (native test-clip milestone)** — a native streaming studio foundation, independent of OBS Studio and Streamlabs. The old 0.2 installers are legacy OBS-controller builds and are **not** standalone.

![Early visual direction](docs/design-reference.webp)

## What works on this branch

- Tauri 2 desktop application and original React creator control center.
- Native display enumeration and screenshot previews on macOS and Windows, using the bundled Rust XCap library. Screen Recording permission is required on macOS.
- **Clips → Capture Silent GIF**: a first-party 2–10 second, approximately 2 fps local display test clip. The saved GIF can be previewed and the file path copied inside the app.
- Our own local scene management: create/rename/duplicate/delete scenes, select program/preview scenes, enable Studio Mode, take transitions, add/remove display sources, and control source visibility.
- Native scene drafts persist in Tauri's local storage; the web/API edition now saves scene collections atomically on disk, with Docker volume persistence.
- Independent Node local-only API with `/api/studio/status`, `/api/studio/action`, `/api/studio/collection`, `/api/capture/displays`, SSE events, and alert-testing endpoints.
- Rust local alert service, animated overlay test page, and local web edition.
- No OBS WebSocket connection, OBS installer, OBS credentials, or Streamlabs dependency.

**Capture is currently screenshot-based, with a limited 2 fps silent GIF test-clip export, not full-frame-rate video.** Audio mixing, full-frame-rate capture/compositing, MP4/MKV recording/encoding, replay buffers, RTMP/SRT, platform integrations, multistreaming, and genuine Go Live are **not implemented**. Their controls must remain disabled until validated. The web app cannot capture local displays through the Node API; it needs the installed native desktop app.

## Development

Requires Node.js 24+, npm, Rust, and the [Tauri 2 native prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm ci
npm test
npm run build
npm run desktop:dev
```

Open **Studio → Add Scene** to create a scene. Each row can be renamed, duplicated or deleted. In the installed Mac/Windows desktop edition select a specific screen in **Sources** and choose **Add**; enable Screen Recording permission where prompted. The preview updates roughly every five seconds with a single captured frame. Use **Studio Mode → select scene → Take** to change the program selection (this is state only, not yet a live encoded broadcast).

Scene collection backup: open **Settings → Scene Collection Backup** to export or import `moobee-studio-scenes.json`. The format is versioned (`version: 1`), constrained to 40 scenes/30 display sources per scene, and validated before replacement. Export a backup before importing. Old native Alpha 0.3 drafts are migrated automatically when valid.

The standalone local web/API edition runs with `npm run dev`; see `docs/TAURI.md`. Its API persists to `~/.moobee-stream-studio/scenes.v1.json`, or the `MOOBEE_DATA_DIR` override. Docker Compose binds `./data` to the internal state folder. Restrict access to this private data and back it up. The API binds to 127.0.0.1 by default and has no remote authentication. **Never expose it directly to the internet.**

## Native test clip acceptance

Open **Clips** on an installed Mac/Windows build and select the desktop display. Choose a duration of 2, 4, 6, or 10 seconds and press **Capture Silent GIF**. MooBee samples native frames and saves a limited-size GIF under its private app-data `test-clips` folder. Select a saved clip to preview it. The app does not record audio, encode MP4 or go live; this is a test milestone for the native capture-to-file pipeline. Linux, Android and web display a clear unavailable message. Files remain on your computer.

## Architecture

| Component | Role |
|---|---|
| Desktop shell | Tauri 2 / Rust; packaged Mac/Windows runtime |
| Native screen acquisition | XCap in this alpha; OS-specific high-frame-rate capture planned |
| Scene model | MooBee's own local state machine (no OBS) |
| Graphics composition | Planned native GPU compositor (Metal / D3D11 / Vulkan or wgpu) |
| Audio | Planned CoreAudio / WASAPI / PipeWire capture and mix |
| Encode/output | Planned hardware video encoders and first-party media transport |
| Backend | MooBee Node API and event stream, later accounts/permissions/DB |
| Web/mobile | Creator management clients, separate device-capability implementations |

See `docs/ROADMAP.md` for objective feature gates and `docs/TAURI.md` for release/OS readiness. Existing GitHub Actions targets macOS Apple Silicon and Intel, Windows x64, Linux x64, Android ARM64, and web. **Build success is not media-functionality acceptance.**

## Licensing

XCap is Apache-2.0 licensed. Future media dependencies and codec packaging must be audited before distribution; FFmpeg distributions may have LGPL/GPL obligations depending on compile options. We are building our own product and UI, not copying proprietary Streamlabs assets or redistributing OBS source.
