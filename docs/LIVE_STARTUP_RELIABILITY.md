# Workstream 10 — startup reliability, first increment

Native Mac and Windows live tests now open selected audio inputs before RTMPS negotiation, so a failed audio-device open does not start publishing. Local AVI/MP4/FLV capture keeps its previous writer/audio startup order. After successful authorization, live capture establishes an arrival-based startup barrier before starting its nominal media clock:

- Discard queued display frames until each receiver is observed empty, with a 256-discard budget per receiver.
- Obtain a subsequent arrival for each display; for a selected camera/capture card, consume its current available frame and wait for a subsequent sequence.
- Reset the macOS named-input mixer or discard pending Windows audio.
- Fail if cancelled, disconnected, over budget, or unable to complete the refresh within a shared 1.5-second window. No stale startup frames are intentionally replayed.
- Check cancellation again after the per-tick sleep. Fail closed on a saturated 64-frame per-source drain or more than 500 ms of scheduling lag, rather than deliberately attempting a long catch-up burst.

The native worker and credential lease are dropped on startup error; existing recovery classification applies only to classified transport failures, not failed device preparation or local overload. Server publish authorization may already have occurred before a post-negotiation refresh fails. The barrier observes receiver arrival and camera sequence, not device capture PTS; it does not prove physical A/V synchronization. The underlying XCap producer owns its channel, so this does not bound allocations during the negotiation itself; a sufficiently long handshake can still cause source-side backlog and abort. Windows audio discard can fail on a device backlog, and OS capture calls are not forcibly interrupted. The initial 256-frame, 1.5-second and 500-ms policies are conservative diagnostic defaults requiring real-device calibration. Unit tests cover purge, next-frame admission, cancellation, disconnect, deadline and lag bounds. No native hardware tests, actual public stream, installer or release performed. Production live controls remain gated.
