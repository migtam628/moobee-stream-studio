# MooBee Live — product roadmap and technical architecture

## Product decision

Build an independently branded platform that brings together the strongest product patterns of OBS Studio and Streamlabs. Keep OBS as an external engine through v0.1/v0.2; later evaluate a native OBS plugin or a clearly GPL-compliant bundled/forked desktop application.

## Phase 1 — local working broadcast control (alpha 0.1)

- [x] Responsive React control center and basic module navigation
- [x] Local backend integration with OBS WebSocket 5.x
- [x] Connect, disconnect, scene switching and input source visibility
- [x] Audio mute and volume adjustments
- [x] Start/stop recording and streaming
- [x] Still-image preview of current scene (not full-frame-rate)
- [x] SSE activity feed and test alert browser-source overlay
- [x] Local-only security default and basic automated tests
- [ ] Test against user's actual OBS installation on macOS and Windows
- [ ] Add actual preview/program frames or a stream ingest endpoint
- [ ] Add per-device audio meter levels and smooth controls

**Acceptance:** An OBS installation on a trusted local computer responds to the app's scene, mixer, record and broadcast controls; a test alert appears inside OBS; API and UI tests pass. All items involving real OBS are pending hardware validation.

## Phase 1B — Tauri desktop edition (alpha 0.2)

- [x] Tauri 2 shell around the existing React UI
- [x] JS OBS WebSocket bridge operating without the Node API in native mode
- [x] Rust overlay-service source for local Browser Source and SSE alerts
- [x] Custom MooBee Live desktop icon set
- [x] Windows/macOS/Linux GitHub Actions build workflow
- [ ] Validate Rust compilation on a machine with the Tauri prerequisites
- [ ] End-to-end tests on the user's Apple Silicon Mac and Windows streaming PC
- [ ] Signed/notarized installers and release channel
- [ ] Integrate libobs or an OBS plugin for a true single-install broadcast engine

The Tauri edition is a native control application, not a compiled fork of OBS Studio. `docs/TAURI.md` tracks real hardware acceptance.

## Phase 2 — creator foundation (alpha 0.2–0.4)

- PostgreSQL, Redis and persistent tenant/workspace settings
- Authentication and roles for dashboard access
- Signed browser-overlay URLs and secure live event distribution
- Scene collection templates, export/import and automatic profiles
- Visual overlay editor, alert themes, chat box, goals and widgets
- OAuth for Twitch/YouTube and event subscriptions where permitted
- Overlay simulation/testing tools
- GitHub Actions release pipeline, Windows/macOS smoke tests

## Phase 3 — multistream and collaboration (beta 0.5–0.7)

- MediaMTX ingest and server-side RTMP fan-out (with FFmpeg workers where required)
- Destination credentials encryption, per-target status/retries, throughput caps
- Streaming platform OAuth/connectors, respecting API/platform access requirements
- WebRTC SFU guest collaboration; roles, invite links, permissions
- Up to 11 guest feeds subject to server resources and network tests
- Stream recording management, object storage and retention controls

## Phase 4 — AI automation and creator suite (beta 0.8–0.9)

- Rule builder for scene transitions and stream events
- Local Ollama-based assistant, optional GPU if detected, CPU fallback
- Vision/game event detection opt-in with clear GPU performance benchmarks
- Configurable 1000-interaction/month quotas and 10 concurrent automations if desired
- Replay buffer/recording integration with user's ClipForge workflow
- Video and podcast editor integration; captioning, reframing, highlight selection
- Cloudbot: moderation, commands, timed messages, loyalty, giveaways
- Merchandise/tips with approved payments integrations

## Phase 5 — desktop, mobile, and full ecosystem (1.0+)

- Native OBS plugin/dock with first-party UI integration
- One-click installers and updates for Windows/macOS
- Evaluate a fully branded OBS fork **only after licensing and maintenance review**
- Mobile control/broadcast apps, console pathways subject to platform APIs
- Developer SDK, plugin marketplace and genuinely original overlay library
- Multi-user SaaS hosting, billing, audit logs, backup/restore, incident response

## Suggested service layout

| Component | Implementation candidate | Responsibility |
|---|---|---|
| Desktop engine | OBS Studio / libobs | Capture, compositing, audio, encoding, recording |
| Integration bridge | Node.js OBS WebSocket bridge | Source/scene control and event processing |
| Creator web app | React + TypeScript | Control center, overlays, AI/rule editor |
| Control plane | Node.js API, PostgreSQL, Redis | Auth, workspaces, settings, jobs, event bus |
| Media service | MediaMTX + FFmpeg | RTMP/WebRTC ingest, streaming relays, recording |
| AI workers | Ollama and optional GPU runtime | Vision, streaming assistant, highlight processing |
| Editors | ClipForge/MooBee Studio adapters | Clips, transcripts, short-form editing |
| Storage | S3-compatible object store | Videos, templates, media assets |

## Important limitations

1. Streamlabs premium subscriptions are not bypassed. Equivalent features must be independently implemented.
2. Streamlabs-branded graphics and proprietary templates are not included.
3. YouTube, Twitch, Kick, TikTok, Facebook and consoles have different API permissions and platform conditions. These integrations cannot be promised before platform verification.
4. Premium overlay volume (1000+) and app library volume (60+) require original assets and actual compatible apps; a number in the UI is not a deliverable.
5. Self-hosted multistream consumes upstream bandwidth and server egress; downstream transcoding may need hardware acceleration.
6. OBS plugin/fork distribution triggers licensing, signing, packaging and upstream compatibility work.

## Primary technical references

- [OBS source and GPL license](https://github.com/obsproject/obs-studio)
- [OBS developer guide](https://obsproject.com/kb/developer-guide)
- [OBS WebSocket protocol](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md)
- [OBS plugin starter and CI](https://github.com/obsproject/obs-plugintemplate)
- [MediaMTX forwarding](https://mediamtx.org/docs/features/forward)
- [MediaMTX authentication](https://mediamtx.org/docs/features/authentication)
