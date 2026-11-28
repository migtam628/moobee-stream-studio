# Group A — private acceptance build procedure

The default application and release workflow remain unchanged. This is an explicitly enabled developer-only acceptance build. Never use production credentials or a public channel for the first test.

## Prepare the application

After the dedicated workflow is reviewed and merged, open Actions → Prepare native broadcast acceptance builds → Run workflow on main. Enter BUILD PRIVATE QA. It compiles the three supported desktop architectures without DMG/MSI/NSIS installers, publishing a GitHub release, launching the application or starting a broadcast. The workflow produces a short-lived Actions artifact per architecture, containing a macOS app archive or Windows executable and a manifest with the full commit and SHA-256. This workflow has not yet been dispatched or validated end-to-end. Do not treat unsigned QA artifacts as distributable beta releases. Access is governed by the repository's Actions artifact permissions; use a private repository or local build for non-public testing. Native signing, Gatekeeper, Windows SmartScreen, WebView2 and platform runtime prerequisites may require separate authorized preparation.

Local alternative from a clean checkout of the reviewed commit, with Node 22, Rust, Tauri platform prerequisites and native compiler installed:

```sh
npm ci
# macOS: creates app bundle only, not a DMG
npm run desktop:build -- --features native-live-acceptance --bundles app
# Windows: creates executable only, not an installer
npm run desktop:build -- --features native-live-acceptance --no-bundle
```

Use the appropriate command for the actual OS. For local build provenance, set GITHUB_SHA to the exact checkout commit and GITHUB_REF_NAME to private-qa before building; do not claim CI provenance for a local build. Do not run a QA app concurrently with the normal app; both use the existing OS credential store and app identity. Use an isolated test OS account where practical. No changes to system trust or security bypass are required by this procedure.

## Staged test order

1. Verify manifest SHA-256 and exact sender commit. Launch only when the operator explicitly approves testing.
2. Save a disposable private RTMPS destination and verify editing, rotation and deletion in the native vault. Do not include the key in evidence.
3. Choose a display or video capture device, named audio input and baseline profile. Use a controlled private receiver with a valid trusted TLS certificate.
4. Run the consent negative cases (Cancel/No, expiry, stale destination) before affirmative authorization. Verify no media reaches the receiver on negative cases.
5. Approve a 15-second test. Capture the original receiver media. Export the redacted sender report from Analytics before closing or reloading. Observe actual video and audible audio.
6. Run the offline verifier using the real sender commit and capture profile:
```sh
python3 scripts/verify-rtmps-soak-capture.py receiver.flv --seconds 15 --platform macos-arm64 --commit FULL_40_CHARACTER_COMMIT --profile baseline
```
7. Repeat at 5, 30 and 120 minutes; collect real A/V sync observations at start, middle and end. Run interrupted recovery, explicit stop and multi-output cases separately. An interrupted test is never an uninterrupted acceptance pass.
8. Record source hardware, OS, architecture, build identity, manual results, redacted sender JSON and receiver verifier JSON. Have a human review the original receiver files and security checklist. Never mark a production gate verified based only on a build or automated report.

See GROUP_A_BROADCASTING_ACCEPTANCE.md for all gates and LIVE_SOAK_DIAGNOSTICS.md for resource and timing caveats. Production Go Live remains disabled.
