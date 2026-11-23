#!/usr/bin/env bash
# Synthetic media QA, not a real ScreenCaptureKit device test.
# Runs entirely inside CI. No user's recordings or audio are uploaded.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

ffmpeg -nostdin -hide_banner -loglevel error -y \
  -f lavfi -i 'testsrc2=size=320x180:rate=30' \
  -f lavfi -i 'sine=frequency=660:sample_rate=48000' \
  -t 6 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 128k \
  -movflags +faststart "$TMP/audible.mp4"
node "$ROOT/scripts/verify-native-mp4.mjs" --file "$TMP/audible.mp4" \
  --seconds 6 --expect-audible > "$TMP/audible.json"
node "$ROOT/scripts/verify-native-mp4.mjs" --file "$TMP/audible.mp4" \
  --seconds 60 --stopped-early --expect-audible > "$TMP/stopped.json"

ffmpeg -nostdin -hide_banner -loglevel error -y \
  -f lavfi -i 'testsrc2=size=320x180:rate=30' \
  -f lavfi -i 'anullsrc=channel_layout=stereo:sample_rate=48000' \
  -t 6 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 128k \
  -movflags +faststart "$TMP/silent.mp4"

if node "$ROOT/scripts/verify-native-mp4.mjs" --file "$TMP/silent.mp4" \
  --seconds 6 --expect-audible > "$TMP/unexpected-passing-audio.json"; then
  echo "::error::Silent AAC was incorrectly accepted as audible"
  exit 1
fi
echo "Synthetic MP4 QA passed: full decode, early stop tolerance, and audio silence rejection."
