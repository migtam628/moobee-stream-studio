# Implementation sprint target — 75% feature coverage

Proposed planning metric: 15 of the following 20 equally counted workstreams have their scoped core functionality wired into the application, reviewed and merged with passing CI. This is a development coverage metric, not a time estimate, feature-parity percentage, or production readiness score. A primitive, mock, prototype, open PR or isolated codec does not satisfy an integration gate by itself. A workstream may be implemented for a bounded diagnostic while hardware acceptance remains pending; the documented scope must be visible in the UI.

| Workstream | Required core integration | Snapshot (2026-11-26) |
|---|---|---|
| 1. Studio workspace and scenes | create/edit/persist/import/export collections, Preview/Program, Studio Mode | Implemented, bounded |
| 2. macOS display and video devices | display selection, webcam/HDMI enumeration and preview | Implemented, bounded |
| 3. Windows display and video devices | display selection, webcam/HDMI enumeration and preview | Implemented, bounded (#58) |
| 4. Scene transforms and filters | persisted native transforms, crop, opacity, chroma key | Implemented, bounded (#47/#52) |
| 5. Audio mixer | named mic/HDMI and desktop inputs, gain/mute, meters on both desktop platforms | Implemented, bounded (#46/#56/#57) |
| 6. Local recording | native composed H.264/AAC on Mac and Windows, stop/pause, file recovery | Implemented, bounded (#54/#55) |
| 7. Native transitions | encoded cut/fade/slide/wipe on same opened inputs | Implemented, bounded (#53) |
| 8. Local clips and browser recording | clip inventory, reveal/save, browser-local shared capture | Implemented, bounded |
| 9. Live RTMPS | authorized connection, continuous native media source and Go Live/Stop | Partial: protocol/TLS, queue, codecs; worker and producer under development |
| 10. Live reliability | connection recovery, independent inbound servicing, health and output timing | Partial: queue and pacing; reconnect/inbound worker missing |
| 11. Destination management | secure storage, explicit per-destination authorization and configuration | Pending |
| 12. Multistream | independent destination delivery and per-destination failure isolation | Pending |
| 13. Overlays and alerts | configurable dynamic scene layers, alert events in actual encoded output | Partial: static overlay/local alert test |
| 14. Unified chat and moderation | authenticated platform integration, chat and moderation controls | Pending |
| 15. Replay and clipping | rolling buffer, hotkeys, export and local editing | Partial: local clip inventory only |
| 16. Expanded media sources | window/game capture, mixed display/video scenes, text/media/browser source | Partial |
| 17. Remote API and accounts | authenticated versioned API, RBAC, TLS and durable persistence | Partial: local-only API |
| 18. Linux native capture | supported capture/audio/encoding and permission workflow | Pending |
| 19. Android and web | mobile capture and web control with clearly scoped capabilities | Partial: browser-local capture, build targets |
| 20. Advanced creator tools | extensibility, captions, collaboration and automation | Pending |

Initial bounded core coverage: 8/20 (40%), a proposed conservative planning baseline. Partial rows are not counted. Real-device acceptance, security review, code signing, installers and sustained stream/recording tests are separate formal gates (see BETA_READINESS.json and PRODUCTION_READINESS.md). Do not change those gates based on this checklist. Reevaluate scope and weights with the product owner if the target includes complete OBS/Streamlabs feature parity.

## Worker/producer increment
The dedicated publishing worker takes ownership of an already authorized Session with a finite-deadline transport. It uses a 128-packet/8 MiB bounded mailbox, atomic producer batch admission, independent pacing ticks, explicit stop/failure, 5-second producer inactivity failure, nonblocking status/reaping, and fail-closed queue overflow. The macOS producer combines VideoToolbox AVC and native AAC on a nominal frame/sample timeline, supplies codec configurations before media and forces an IDR at startup and nominal two-second intervals. Capture and device ownership remain with the future orchestrator. Tests use a simulated server; the combined native synthetic encoder test is opt-in, not a physical test. The worker does not autonomously connect to any destination. Inbound signaling requires an owner that can ensure a complete chunk can be read; socket/TLS readiness, connection startup/cancellation, live capture integration, actual streaming, Windows live packets and receiving-side verification remain outstanding. Stop is cooperative and may wait for an existing bounded I/O call. No installer or release is authorized.

## Sprint policy
Keep coding independent increments on dependent branches while CI runs. Merge only green PRs in dependency order; retarget onto main when parents merge. Keep live output disabled until integration and receiving-side acceptance; never send a real stream without explicit destination authorization. Do not publish installers or production releases during this sprint.

## Native Program live-packet diagnostic
The Mac-only offline FLV test now routes the existing native Program capture worker's composed RGBA and aligned optional PCM into the actual VideoToolbox AVC / AAC packet producer. The diagnostic uses the same packet validation rules as the live publishing queue. It writes validated FLV tags incrementally to a private UUID temporary file and publishes only after successful finalization with at least two video packets, required AAC media when selected, file sync and no-replace publication. Limit: 2–8 seconds, 64 MiB, one named video device or up to four displays; separate webcam inset off. Existing source layout/transforms/filters, static overlay, and named audio mixing are reused. No network connections, stream keys, remote destinations, live worker invocation, pause/take controls, or live UI activation. Reported duration is nominal submitted-frame duration, not independently measured file playback; AAC startup/tail delay is not flushed or corrected. Independent full decode (e.g. ffprobe/FFmpeg) and physical capture QA remain necessary. Use Studio > Capture diagnostics > Offline streaming packet FLV test; reveal the private clip and test it externally. Failed diagnostics never publish incomplete media. This advances workstream 9 but does not complete it or increase the 8/20 core coverage count. 
