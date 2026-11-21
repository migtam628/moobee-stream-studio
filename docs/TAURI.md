# Tauri 2 standalone desktop — Alpha 0.3

This branch is a **native MooBee media prototype**, not an OBS controller.

## Build

Install Node 24+, Rust stable, and [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).
```sh
npm ci
npm test
npm run build
npm run desktop:dev
```

macOS: Screen Recording permission may be required under System Settings → Privacy & Security. Windows: allow capture permissions for MooBee. Try **Add Scene**, then **Add Display** in the Studio panel, and confirm a screen-frame snapshot appears. Other platforms currently return no native displays.

The initial Tauri release builds Mac DMG (ARM64/Intel), Windows NSIS, Linux DEB and Android test APK. Linux/Android do not yet implement capture; web is a local control-plane prototype.

## Technical acceptance checklist

- [ ] Successful Rust build for macOS Apple Silicon and Intel
- [ ] Successful Rust build for Windows x64
- [ ] Native display enumeration with permission handling
- [ ] Capture a visible frame from the correct display
- [ ] Confirm source visibility and preview/program transitions
- [ ] No external OBS or Streamlabs installation
- [ ] Signed and notarized production packages

**Not built:** GPU compositor, audio engine, full-fps capture, encoder, recorder and streaming output. Do not treat a successful installer build as these features working.
