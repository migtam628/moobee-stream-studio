# GitHub-owned builds, releases and updates

MooBee Stream Studio uses GitHub as the source of truth for source code, PR checks,
build artifacts, release notes, prerelease installers, and update discovery.

## Distribution at a glance

| Platform | GitHub Actions output | Native broadcast engine |
| --- | --- | --- |
| macOS Apple Silicon | Unsigned alpha DMG | Native capture/experimental short MP4; not full studio output |
| macOS Intel | Unsigned alpha DMG | Native capture/experimental short MP4; device QA pending |
| Windows x64 | Unsigned alpha NSIS setup EXE | Silent AVI diagnostics only |
| Linux x64 | Unsigned alpha DEB | Native screen capture unsupported |
| Android ARM64 | Debug-signed APK | Native broadcasting unsupported |
| Web + local API | Self-hosted tar.gz and GitHub Pages UI preview | Browser permission-based experiments only |

No build should be presented as a production broadcaster merely because GitHub Actions
completed or a package installed. Record and Go Live remain disabled.

## How an alpha is published

1. Work in a feature branch. A PR runs web/API tests, browser smoke tests,
   AVI muxer checks, and native macOS compile/link checks where applicable.
2. Update `docs/RELEASE_NOTES.md` for the new alpha. Ensure the code and all
   package versions agree. The legacy 0.3.x alpha series identifies its binaries
   by their core `0.3.x` version; GitHub release tags distinguish alpha build numbers.
3. Create a **new** branch named `release/vX.Y.Z-alpha.N` pointing at the
   specific tested code. Never move an existing published release branch/tag.
4. The `Build and release MooBee Stream Studio` workflow automatically:
   - verifies API/frontend, web compilation, Rust AVI/full-decode checks, and
     version consistency across root, web, API, desktop, Cargo and Tauri;
   - fails if the release tag already exists;
   - builds self-hosted web distribution, macOS ARM64/x64 DMGs, Windows NSIS
     setup EXE, Linux DEB and Android debug APK;
   - blocks publication unless **all four desktop installers and the web bundle** pass; an Android dependency HTTP 429 failure can only be exempted for **experimental alpha** releases after three failed rate-limited attempts, with an uploaded reason marker verified by the publish job. Other Android build failures block publication;
   - generates `RELEASE-MANIFEST.json` (commit, asset sizes, SHA-256 hashes),
     and `SHA256SUMS.txt`;
   - publishes available artifacts as a GitHub **prerelease** under the new tag. A documented Android HTTP 429 exception produces a **partial-platform** alpha with no Android APK, a machine-readable `unavailableTargets` entry and an explicit warning in the release notes. Its workflow may still report a failed Android job; the desktop artifacts do not become signed or hardware-verified by publication.
5. Inspect the release page, download each installer and perform real-device
   permission/recording/playback tests. Record evidence under Issue #14/#15.
   Github Actions cannot substitute for physical hardware acceptance.

**Published releases are append-only in this workflow.** A correction must use a
new alpha identifier and new release branch, not `gh release upload --clobber`.
If a published tag already exists, the pipeline refuses to reuse it.

## GitHub-based update discovery

In the app, open **Settings → GitHub Releases & Updates → Check for releases**.
The app queries the public GitHub Releases API only when you click the button,
then presents the official release page and downloadable installers. It does not
send scene data, local captures or streaming credentials to GitHub.

The current experimental app does **not** install an update automatically and
does **not** label a GitHub prerelease as a stable upgrade. Existing 0.3.x
prerelease binaries report core package versions, so their version number alone
cannot reliably distinguish `alpha.1` from `alpha.2`.

### Enabling genuine automatic desktop updates

Tauri 2 requires cryptographically signed updater bundles, an embedded public
verification key, a protected private signing key, and an authenticated HTTPS
manifest endpoint. **A GitHub Release download URL and SHA256SUMS alone do not
qualify as a safe in-app updater.**

Before enabling unattended or one-click in-app installation:

1. Generate a dedicated Tauri updater key pair securely. Store the **private**
   signing key and password in GitHub Actions secrets, not repository files;
   ship only its public key in the desktop configuration.
2. Configure the Tauri updater plugin and signed bundle generation for
   macOS/Windows/Linux; publish appropriately signed update bundles and a
   platform-specific update manifest to GitHub Releases or a static
   GitHub Pages endpoint. Verify URLs/signatures/platform mappings in CI.
3. Set each app release's embedded semantic version to a unique version
   (including prerelease suffix) and verify update ordering on older clients.
4. Sign/notarize macOS distributions and Authenticode-sign Windows installers
   with developer credentials; enable rollback and test upgrade migrations.
5. Require real-device update/rollback acceptance on all supported platforms.
   No silent installs, permissive signature bypass, or unofficial mirrors.

Android debug APKs are test packages, not a production update channel.

## Dependency updates

`.github/dependabot.yml` creates **weekly pull requests** for npm,
Cargo/Rust and GitHub Actions dependencies. GitHub Actions validates those PRs;
they are *not* auto-merged, released or deployed without review.

## GitHub links

- [Workflow runs](https://github.com/migtam628/moobee-stream-studio/actions)
- [All releases](https://github.com/migtam628/moobee-stream-studio/releases)
- [Project issues](https://github.com/migtam628/moobee-stream-studio/issues)
- [Mac MP4 acceptance](MACOS_MP4_ACCEPTANCE.md)
- [Production readiness gates](PRODUCTION_READINESS.md)

Stable or beta promotion requires the gates in `PRODUCTION_READINESS.md`,
including actual recording/broadcasting, endpoint security, signing, and
hardware tests. Until then, releases are explicitly experimental alpha builds.
