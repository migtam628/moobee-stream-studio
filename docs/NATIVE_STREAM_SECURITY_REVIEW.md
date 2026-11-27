# Native streaming security review — workstream 9

Scope: source review of the native RTMPS connector, publisher, destination vault, recovery fanout, acceptance IPC and desktop configuration. Not an independent penetration test or full dependency audit.

| Finding | Treatment in this increment | Remaining verification |
| --- | --- | --- |
| Stop can occur during DNS, TLS or publish negotiation | Revocable permit checked before/after DNS, before TCP attempts, before TLS/RTMP negotiation and before underlying socket I/O | OS DNS and in-flight operations are not instantly interruptible; check actual cancellation under slow/faulty networks |
| Recovery connection can outlive its capture session | Recovery connector receives a permit checking cancellation and recovery deadline; initial connections receive session stop permit | A connection or publish command already in flight may have reached a server; never claim instantaneous cancellation |
| Credential lease can be released while a detached connection still exists | Share the native lease with connector and socket; final reference releases it | Process-local only; no external-vault modification or cross-process protection |
| Owned stream-key and vault buffers linger | Zeroize owned credential record, serialized vault buffers, single-session key clones and publish command/wire buffers | Not whole-process secret erasure: OS vault, TLS, serialization and IPC copies remain outside this guarantee |
| Credential exposure through status | Existing redacted destination views, generic transport errors and key-free telemetry retained | Actual platform log inspection and native credential persistence tests |
| Invalid TLS trust/hostname | Existing test-only independent loopback trust/hostname rejection retained; production native trust unchanged | Private external endpoint interoperability |
| Malicious local WebView/remote API | Existing default-disabled acceptance feature and no HTTP live-start endpoint retained | Typed confirmation is not an independent native consent boundary; WebView/CSP and authenticated API need deeper security review |

The negotiation permit is deliberately removed after successful authorization so an explicit stop can drain admitted media and send deleteStream. The transport keeps the credential lease until it is destroyed. No cancellation policy can unsend bytes already transmitted. Tests cover a pre-cancelled connection, revoked socket permit (local loopback only), and shared lease lifetime. Existing worker, fanout, receiver and credential tests remain in place. No actual public stream, installer execution or release. Production Go Live stays gated pending native runtime/physical receiver acceptance, two-hour soak, and further security validation.
