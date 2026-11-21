#!/usr/bin/env bash
# Single source of truth for native AVI muxer release acceptance.
# FFmpeg is a TEST-ONLY dependency; MooBee's shipped desktop recorder does
# not invoke FFmpeg or OBS to create a recording.
set -euo pipefail
command -v rustc >/dev/null || { echo "Rust toolchain missing" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "FFmpeg fixture tool missing" >&2; exit 1; }
command -v ffprobe >/dev/null || { echo "FFprobe validator missing" >&2; exit 1; }

temp_dir="$(mktemp -d)"
trap 'rm -rf "$temp_dir"' EXIT
root="$(cd "$(dirname "$0")/.." && pwd)"
rustc --edition=2021 --test "$root/apps/desktop/src-tauri/src/avi.rs" -o "$temp_dir/avi-tests"
"$temp_dir/avi-tests" --nocapture

# Use a valid JPEG fixture rather than fabricated JPEG magic bytes.
ffmpeg -hide_banner -loglevel error \
  -f lavfi -i color=c=blue:s=640x360:r=1 -frames:v 1 -q:v 3 \
  -y "$temp_dir/source.jpg"
test -s "$temp_dir/source.jpg"

cp "$root/apps/desktop/src-tauri/src/avi.rs" "$temp_dir/avi.rs"
cat > "$temp_dir/create_avi.rs" <<'RUST'
mod avi;
use std::{env, fs::{self, File}};
fn main() {
    let args: Vec<String> = env::args().collect();
    let jpeg = fs::read(&args[1]).expect("JPEG fixture missing");
    let file = File::create(&args[2]).expect("AVI output cannot be created");
    let mut writer = avi::AviWriter::new(file, 640, 360, 10)
        .expect("AVI writer initialization failed");
    for _ in 0..20 {
        writer.write_frame(&jpeg).expect("Incremental frame write failed");
    }
    writer.finish().expect("AVI index/finalization failed");
}
RUST

rustc --edition=2021 "$temp_dir/create_avi.rs" -o "$temp_dir/create_avi"
"$temp_dir/create_avi" "$temp_dir/source.jpg" "$temp_dir/preview.avi"

ffprobe -hide_banner -v error \
  -show_entries stream=codec_name,codec_type,width,height,avg_frame_rate,nb_frames:format=duration \
  -of json "$temp_dir/preview.avi" > "$temp_dir/probe.json"
python3 - "$temp_dir/probe.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as handle:
    data = json.load(handle)
video = next(item for item in data["streams"] if item["codec_type"] == "video")
assert video["codec_name"] == "mjpeg", video
assert (video["width"], video["height"]) == (640, 360), video
assert video["nb_frames"] == "20", video
assert video["avg_frame_rate"] == "10/1", video
assert abs(float(data["format"]["duration"]) - 2.0) < 0.05, data
print("FFprobe recognizes the incremental Rust AVI: 640x360 MJPEG, 10fps, 2 seconds")
PY
# Parsing metadata is insufficient: actually decode every JPEG sample.
ffmpeg -hide_banner -loglevel error -xerror -i "$temp_dir/preview.avi" -f null -
echo "AVI writer unit tests, index, metadata and complete video decode: PASS"
