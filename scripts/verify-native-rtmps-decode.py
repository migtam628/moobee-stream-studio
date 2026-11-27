#!/usr/bin/env python3
"""Local synthetic native encoder/RTMPS QA. Never launches the desktop app."""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

TEST = "rtmp_session::tls::tests::loopback_tls_native_encoder_acceptance"
PREFIX = "LOOPBACK_RECEIVER_REPORT "
MAX_FILE = 16 * 1024 * 1024

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def run(args, timeout, **kwargs):
    result = subprocess.run(args, capture_output=True, text=True, timeout=timeout,
                            check=False, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} failed ({result.returncode}):\n{result.stderr[-4000:]}")
    return result.stdout

def validate(media, report, ffprobe, ffmpeg):
    require(media.is_file() and 13 <= media.stat().st_size <= MAX_FILE,
            "Missing or oversized receiver evidence")
    require(report["orderlyClose"] and report["pongReceived"], "Receiver did not close cleanly")
    require(report["videoConfig"] and report["audioConfig"], "Missing codec configuration")
    require(report["mediaPackets"] == report["videoFrames"] + report["audioBlocks"] + 2,
            "Unexpected receiver media packet totals")
    data = json.loads(run([ffprobe, "-v", "error", "-show_streams", "-show_frames",
                           "-show_entries",
                           "stream=index,codec_name,codec_type,width,height,sample_rate,channels:"
                           "frame=media_type,best_effort_timestamp_time,nb_samples",
                           "-of", "json", str(media)], 30))
    streams = data["streams"]
    require(len(streams) == 2, "Expected exactly two decoded tracks")
    video = next(s for s in streams if s["codec_type"] == "video")
    audio = next(s for s in streams if s["codec_type"] == "audio")
    require((video["codec_name"], video["width"], video["height"]) == ("h264", 640, 360),
            "Unexpected video format")
    require((audio["codec_name"], int(audio["sample_rate"]), audio["channels"]) == ("aac", 48000, 2),
            "Unexpected audio format")
    vf = [f for f in data["frames"] if f["media_type"] == "video"]
    af = [f for f in data["frames"] if f["media_type"] == "audio"]
    require(len(vf) == report["videoFrames"] == 20, "Decoded video frame count mismatch")
    require(len(af) == report["audioBlocks"] and 80 < len(af) <= 94,
            "Decoded AAC block count mismatch")
    def times(frames):
        pts = [float(f["best_effort_timestamp_time"]) for f in frames]
        require(all(math.isfinite(t) for t in pts), "Nonfinite decoded timestamps")
        return pts
    vt, at = times(vf), times(af)
    require(abs(vt[0]) <= .001 and abs(at[0]) <= .001, "Expected zero-based media timestamps")
    require(all(abs(b-a-.1) <= .002 for a,b in zip(vt, vt[1:])), "Video timestamp gap")
    require(all(int(f["nb_samples"]) == 1024 for f in af), "Unexpected decoded AAC block size")
    require(all(abs(b-a-1024/48000) <= .002 for a,b in zip(at,at[1:])), "AAC timestamp gap")
    # Force full independent decoding and fail on codec errors. No network,
    # playback window or output file is needed.
    run([ffmpeg, "-nostdin", "-v", "error", "-xerror", "-err_detect", "explode",
         "-i", str(media), "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"], 30)
    ve, ae = vt[-1]+.1, at[-1]+1024/48000
    require(abs(ve-ae) <= .20, "Encoded timeline tail gap exceeds 200 ms")
    require(abs(vt[-1]*1000-report["lastVideoMs"]) <= 2, "Receiver/video decoder timestamp mismatch")
    require(abs(at[-1]*1000-report["lastAudioMs"]) <= 2, "Receiver/audio decoder timestamp mismatch")
    return {"decodedVideoFrames":len(vf), "decodedAudioBlocks":len(af),
            "decodedAudioSamplesPerChannel":sum(int(f["nb_samples"]) for f in af),
            "videoEndMs":round(ve*1000, 3), "audioEndMs":round(ae*1000, 3),
            "encodedTailGapMs":round((ve-ae)*1000, 3), "receiver":report,
            "scope":"synthetic native encoding, local TLS, independent decoding; not physical lip sync"}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ffmpeg", default="ffmpeg")
    parser.add_argument("--ffprobe", default="ffprobe")
    args = parser.parse_args()
    ffmpeg, ffprobe = shutil.which(args.ffmpeg), shutil.which(args.ffprobe)
    require(ffmpeg and ffprobe, "Install FFmpeg/ffprobe separately for QA; not a runtime requirement")
    root = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="moobee-receiver-") as directory:
        media = Path(directory) / "received.flv"
        env = dict(os.environ, MOOBEE_RECEIVER_FLV=str(media))
        output = run(["cargo", "test", "--lib", TEST, "--", "--ignored", "--exact", "--nocapture"],
                     900, cwd=root/"apps/desktop/src-tauri", env=env)
        reports = [json.loads(line[len(PREFIX):]) for line in output.splitlines() if line.startswith(PREFIX)]
        require(len(reports) == 1, "Expected exactly one receiver report")
        result = validate(media, reports[0], ffprobe, ffmpeg)
        print("DECODED_RECEIVER_REPORT " + json.dumps(result, sort_keys=True))

if __name__ == "__main__":
    main()
