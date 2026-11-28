# Group A — broadcasting readiness and acceptance

Group A covers the four core workstreams 9–12: Live RTMPS, reliability, destination management, and multistream. The default/release build must not enable production Go Live until the acceptance and security evidence is reviewed. The explicitly enabled developer acceptance build remains the only real network streaming path.

## Merged engineering baseline

- Native macOS ARM64/Intel and Windows x64 H.264/AAC synthetic encrypted receiver decode: #93.
- Offline receiver capture verifier and sustained-test tooling: #94.
- Native session controls and renderer reload recovery: #96.
- Redacted local sender diagnostics: #97.
- Bounded DNS and cancellation: #98.
- Encrypted receiver disconnect recovery and independent multistream rejoin: #99.
- Secure destination editing and revision-bound key rotation: #100.
- Separate OS-native broadcast consent: #101 (all six CI workflows passed; manual interaction remains unverified).
- Individual destination retirement: #102 (six CI workflows passed).
- Global native sender status: #103 (six CI workflows passed).

## Individual destination retirement

During an authorized two- or three-output test, Analytics offers a confirmed Stop destination action for an active output. The native controller validates the exact session and destination against registered per-output stop permits. The last remaining active output uses global Stop. Revocation prevents future reconnect attempts and is checked by any in-flight recovery connector; the capture worker stops submitting new packets to the selected output on its next tick. A healthy worker drains already admitted packets; a recovering output is stopped without rejoining. In-flight network I/O is bounded but cannot be recalled. Retiring an output is irreversible for that session; adding or restarting outputs requires a new authorized test. A retired output disqualifies uninterrupted acceptance even when every surviving destination drains normally. The credential lease may remain held until the session and connector owners release it. Sender status can lag a request until the next capture tick and polling interval; a concurrent failure of another output may leave none viable. These are not remote receiver acknowledgements.

## Required acceptance evidence before production activation

Record sender commit, OS/architecture, actual device/source configuration, selected encoding profile and redacted sender report; retain corresponding original private receiver captures and independent verifier reports. Never include stream keys.

| Gate | Evidence required | Status |
|---|---|---|
| Native consent | Cancel/No/close, expiry, duplicate prompts, stale destination revisions, affirmative private test | Needs manual macOS and Windows QA |
| Credentials | Unlocked OS vault create/edit/rotate/delete, active lease protection, app restart, no key in logs or exported reports | Needs real OS vault QA |
| Capture and audio | Display, HDMI capture card, webcam, mic, desktop audio, real synchronization and source continuity | Needs physical QA |
| Short receiver test | 15-second native capture with matching receiver decode and expected dimensions, cadence, audio | Needs external private receiver |
| Sustained receiver tests | 5, 30, 120 minutes, no unexpected interruption, independent decode and manual sync checks | Needs hardware and receiver |
| Reliability | Controlled network interruption, explicit server stop, cancellation, bounded retry and no retry on denial; distinguish interrupted soak from pass | Needs real network QA |
| Multistream | Independent receivers, recovery isolation, stop one output, prevent last-output retirement, global stop | Needs real receiver QA |
| Security | Native prompt usability, local trust boundary, key/log inspection, external-vault limitations, dependency review | Needs human security review |
| Production activation | Explicit review of above evidence and gated production controls; separate release/signing acceptance | Not authorized by synthetic CI alone |

CI regression results do not constitute physical acceptance. Do not declare Group A complete, promote the formal workstream count, or enable default production broadcasting without the required evidence. Continue independent development increments only with explicit scope and green applicable CI.

## Local destination preflight
Before the renderer presents a broadcast review, native preflight rereads the selected OS-vault records, rejects missing or leased credentials and stale revisions, and checks that no two selections have the same RTMPS host (case-insensitive), port, application path and key. The actual leased records are checked again before multistream capture and connection. This checks only local configuration: aliases, different provider endpoints for the same account and remote credential acceptance cannot be inferred. No DNS, socket, publish, credential export, or automatic consent occurs during preflight. A new review is required after changes. Production Go Live remains gated.
