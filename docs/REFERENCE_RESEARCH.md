# MooBee Live — OBS Studio × Streamlabs reference research

Reviewed November 2026. Goal: an original streaming workspace combining OBS's production controls with Streamlabs' integrated creator tools, without copying third-party branding or proprietary designs.

## Feature comparison and MooBee strategy

| Area | OBS Studio | Streamlabs | MooBee direction |
| --- | --- | --- | --- |
| Capture / streaming | Native source capture, output, encoding, recording | Creator-friendly integrated broadcaster | Keep OBS as the separate engine initially; connect through authenticated WebSocket. A Tauri controller is not itself a capture engine. |
| Studio Mode | Independent preview, program and transitions | Flexible production workspace | **Implemented:** real Studio Mode toggle, preview scene selection, Take transition. Screenshots are snapshots, not live feeds. |
| Scenes / sources | Composition, transforms, filters and scene collections | Stream themes, overlays and apps | Implemented scene switch and visibility; next: source editor and original theme library. |
| Audio | Mixers, monitoring, true level meters, filters | Simplified creator mixer | Implemented mute/volume; next: actual level telemetry and peak warnings. Current striped meter is not live data. |
| Alerts / widgets | Browser Source integrations | Animated alerts, reactive overlays, widgets | Original animated test alerts work. Need platform event adapters and editor before live alerts. |
| Multistream | Output configuration and plugin ecosystem | Cloud relay to multiple destinations | Plan relay/output orchestration with health, platform policy and cost controls. Platform badges are not linked accounts. |
| Automation / AI | WebSocket, hotkeys, scripts, plugins | Scene automations, Sidekick, game-driven overlays | Plan opt-in local rules engine, event adapters and optional AI with human confirmation for going live. |
| Community | Chat docks/plugins | Cloudbot, tips, loyalty, commands | Plan OAuth accounts, moderated chat, bot command permissions and audited tipping integration. |
| Guests | Sources and third-party plugins | Collab Cam and Talk Studio | Plan WebRTC room and guest consent/audio separation. |
| Clips / editing | Recording and replay buffer | Cross Clip, Video Editor, Podcast Editor | Plan recording ingest, vertical crop, subtitle pipeline and export queue. |
| Mobile | OBS desktop engine, remote WebSocket | Android broadcaster and stream handoff | **Android controller first**: trusted LAN remote control. On-device capture/handoff are separate milestones. |
| Web | No hosted OBS control panel built in | Creator web suite and browser studio | Local Node-backed web controller now; public Pages is a static preview until a secured backend exists. |

Primary sources: [OBS overview](https://obsproject.com/kb/obs-studio-overview), [OBS quick start](https://obsproject.com/kb/quick-start-guide), [OBS audio mixer](https://obsproject.com/kb/audio-mixer-guide), [OBS WebSocket protocol](https://github.com/obsproject/obs-websocket/blob/master/docs/generated/protocol.md), [Streamlabs](https://streamlabs.com/), [Streamlabs Desktop](https://streamlabs.com/desktop-app), [Streamlabs Ultra](https://streamlabs.com/ultra), [Streamlabs AI](https://streamlabs.com/streamlabs-ai), [Streamlabs Mobile](https://streamlabs.com/mobile-app).

## Design principles

1. **Broadcast-safe defaults:** distinguish Preview and Program; show real connection state; never imply platform accounts are linked or begin a stream automatically.
2. **One coherent workflow:** capture/setup, scenes, audio, events, alerts, recording, chat and clip creation in one navigable workspace.
3. **Local-first security:** OBS authentication, loopback-bound local API, trusted LAN-only unencrypted mobile connections; remote web access requires a new authenticated/HTTPS service.
4. **Truthful capability states:** label future features as planned, screenshot updates as stills, and meter placeholders as placeholders.
5. **Platform-aware experiences:** desktop production controller, Android touch remote, local functional web, hosted static preview.
6. **Independent identity:** original MooBee assets and integrations; no proprietary Streamlabs templates or embedded OBS binaries in this alpha.

## Milestones

### Alpha 0.2 — implemented foundation

- OBS WebSocket 5 authentication, scene/source listing, program/preview and Take transition.
- Volume/mute, recording and stream actions, periodic still snapshots, OBS event activity.
- Original browser-source alert overlay and manual synthetic alert tests.
- Tauri desktop source, Android test APK release job, Node web distribution and static preview workflow.

### Next — production-controls quality

- True audio peaks (not possible from the current WebSocket values alone), audio monitoring and filters.
- Scene transforms, source properties, transition presets, hotkeys and project/session configuration.
- Stream health, dropped frames, network preflight, reconnection and clear failure recovery.
- Physical-device build testing, signing/notarization, Android pairing and protected credential storage.

### Later — full creator suite

- OAuth destinations, unified chat, actual stream events, overlays/theme editor and bot moderation.
- Multistream relay, per-channel controls, resilience and cost/usage reporting.
- Local rules engine + optional AI Sidekick and reactive overlays.
- Clips and captions, video/podcast editing, collaboration WebRTC and monetization tools.

## UX changes made from code review

- Replaced misleading duplicate preview/program scene state with actual OBS Studio Mode behavior.
- Replaced a fake named project with neutral `My Studio`, fixed stale Alpha 0.1 label.
- Added a visible public-preview indicator and disabled unsupported connection in the hosted build.
- Added native WSS and trusted-LAN WebSocket validation for Android controllers.

Remaining UX validation: actual browser screenshot walkthrough, contrast/keyboard testing, Android WebView networking tests, and real OBS hardware integration. This is an official-source feature comparison plus code review, **not** an exhaustive visual audit of every competitor screen.
