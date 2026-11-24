# Native scene layouts (experimental)

The selected scene has a saved layout: **Grid** (legacy default) or
**Picture-in-picture**. For PiP the first enabled display is the primary
aspect-fitted image and subsequent enabled displays appear in opaque inset
tiles stacked from the lower right. Move sources up/down to change the
primary and inset order. Up to four unique enabled displays are supported.

The local desktop and self-hosted scene API preserve layout and source order
when exporting, importing, duplicating and transitioning scenes. Existing v1
collections without layout continue using Grid. Invalid layout imports fail
before replacing the existing collection.

The same Rust composition function is used by native still previews,
sampled AVI and continuous AVI/MP4. Recording snapshots the Program scene
configuration when the test begins; it does not dynamically follow scene
changes while recording. This feature supports **display layers only**.
No webcam, text, alerts, transparent image overlays, freeform positioning,
live transitions, synchronized audio or RTMPS yet.

## Acceptance

- Add two displays to Program and choose PiP. Verify the first is primary
  and the second is inset. Reverse the source order and verify the result.
- Enable Studio Mode, select another Preview scene and choose Grid. Verify
  Program keeps its own PiP layout until Take.
- Record the sampled AVI, continuous AVI and on Mac the composed MP4 test.
  Verify the expected layout, aspect fit, motion, full decode and seeking.
- Export, restart and import a scene collection; confirm layout and order.
- Try invalid layout and duplicate display IDs; no incomplete clip should
  be published. Test three and four displays and disabled sources.
- Report only sanitized metadata in hardware issue #14.

All outputs remain experimental and silent. Main Record and Go Live stay
disabled. Layout presets are a foundation for camera and overlay geometry,
not proof of production video composition or synchronized audio.
