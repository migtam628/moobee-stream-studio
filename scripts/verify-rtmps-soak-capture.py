#!/usr/bin/env python3
"""Offline independent receiver-file checks. No network, credentials or app launch."""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

DURATIONS = (15, 300, 1800, 7200)
MAX_PACKET_REPORT = 128 * 1024 * 1024
MAX_PACKETS = 1000000

def require(condition, message):
    if not condition:
        raise ValueError(message)

def finite(value):
    try:
        n = float(value)
        return n if math.isfinite(n) else None
    except (ValueError, TypeError, OverflowError):
        return None

def scan_packets(lines, video_index, audio_index):
    tracks = {i: {"count": 0, "first": None, "last": None, "end": None,
                  "maxGap": 0.0, "nonIncreasing": 0, "missingPts": 0,
                  "missingDuration": 0} for i in (video_index, audio_index)}
    seen = 0
    for line in lines:
        if not line.startswith("stream_index="):
            continue
        parts = dict(field.split("=", 1) for field in line.strip().split("|") if "=" in field)
        index = int(parts["stream_index"])
        if index not in tracks:
            continue
        seen += 1
        require(seen <= MAX_PACKETS, "Receiver packet budget exceeded")
        t = tracks[index]
        t["count"] += 1
        pts, duration = finite(parts.get("pts_time")), finite(parts.get("duration_time"))
        if pts is None:
            t["missingPts"] += 1
            continue
        if duration is None or duration <= 0:
            t["missingDuration"] += 1
        if t["first"] is None:
            t["first"] = pts
        if t["last"] is not None:
            gap = pts - t["last"]
            if gap <= 0:
                t["nonIncreasing"] += 1
            t["maxGap"] = max(t["maxGap"], gap)
        t["last"] = pts
        end = pts + (duration if duration is not None and duration > 0 else 0)
        t["end"] = max(t["end"], end) if t["end"] is not None else end
    return tracks[video_index], tracks[audio_index]

def evaluate(video, audio, seconds, fps, max_av_gap_ms=200):
    # Packet timeline only; independent physical lip-sync measurements remain required.
    issues = []
    for label, t, cadence in (("video", video, 1/fps), ("audio", audio, 1024/48000)):
        if t["first"] is None or t["end"] is None:
            issues.append(label + ": no valid timestamped packets")
            continue
        if t["missingPts"] or t["missingDuration"] or t["nonIncreasing"]:
            issues.append(label + ": incomplete or nonmonotonic packet timing")
        span = t["end"] - t["first"]
        if abs(span - seconds) > 2:
            issues.append(label + ": duration differs from requested test by over 2 s")
        if t["maxGap"] > cadence * 2.5 + .002:
            issues.append(label + ": packet gap exceeds 2.5 nominal intervals")
        if t["count"] < max(2, math.floor((seconds - 2) / cadence)):
            issues.append(label + ": too few packets for nominal cadence")
    if all(t["first"] is not None and t["end"] is not None for t in (video, audio)):
        start_gap = abs(video["first"] - audio["first"]) * 1000
        end_gap = abs(video["end"] - audio["end"]) * 1000
        if max(start_gap, end_gap) > max_av_gap_ms:
            issues.append("Encoded track start/end difference exceeds threshold")
    else:
        start_gap = end_gap = None
    return {"packetTimingPass": not issues, "issues": issues,
            "startGapMs": start_gap, "endGapMs": end_gap,
            "video": video, "audio": audio}

def run(args, timeout, **kwargs):
    return subprocess.run(args, timeout=timeout, check=True, **kwargs)

def verify(path, seconds, fps, width, height, commit, platform, ffprobe, ffmpeg):
    require(path.is_file(), "Select an existing local receiver capture")
    require(path.suffix.lower() in (".flv", ".mp4", ".mkv"), "Use an FLV, MP4 or MKV receiver capture")
    require(len(commit) == 40 and all(c in "0123456789abcdef" for c in commit),
            "Provide the exact 40-character Git commit")
    require(path.stat().st_size <= 32 * 1024**3, "Receiver file exceeds 32 GiB test budget")
    probe = [ffprobe, "-v", "error", "-protocol_whitelist", "file,pipe", "-i", str(path.resolve())]
    meta = json.loads(run(probe + ["-show_streams", "-show_entries",
        "stream=index,codec_type,codec_name,width,height,sample_rate,channels",
        "-of", "json"], 60, capture_output=True, text=True).stdout)
    streams = meta["streams"]
    video = [s for s in streams if s["codec_type"] == "video"]
    audio = [s for s in streams if s["codec_type"] == "audio"]
    require(len(streams) == 2 and len(video) == len(audio) == 1,
            "Expected exactly one video and one audio stream")
    v, a = video[0], audio[0]
    require((v["codec_name"], v["width"], v["height"]) == ("h264", width, height),
            "Unexpected H.264 video dimensions")
    require((a["codec_name"], int(a["sample_rate"]), a["channels"]) == ("aac", 48000, 2),
            "Expected AAC 48 kHz stereo")
    # Spool packet metadata to a local temporary file; no media or secrets in the JSON.
    with tempfile.TemporaryFile(mode="w+t", encoding="utf-8") as output:
        run(probe + ["-show_packets", "-show_entries", "packet=stream_index,pts_time,duration_time",
            "-of", "compact=p=0:nk=0"], 1800, stdout=output, stderr=subprocess.PIPE, text=True)
        require(os.fstat(output.fileno()).st_size <= MAX_PACKET_REPORT, "Packet report exceeds test budget")
        output.seek(0)
        result = evaluate(*scan_packets(output, v["index"], a["index"]), seconds, fps)
    # Decode the whole received file, including both tracks; never re-encode it.
    run([ffmpeg, "-nostdin", "-v", "error", "-xerror", "-err_detect", "explode",
         "-protocol_whitelist", "file,pipe", "-i", str(path.resolve()),
         "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"],
        max(120, seconds * 3), capture_output=True)
    return {"schemaVersion": 1, "kind": "receiver-file-diagnostic",
            "commit": commit, "platform": platform, "requestedSeconds": seconds,
            "profile": {"width": width, "height": height, "fps": fps},
            "fullDecodePass": True, **result, "automaticFileChecksPass": result["packetTimingPass"],
            "physicalAcceptanceVerified": False,
            "note": "Offline receiver file checks only; no proof of capture-clock lip sync, actual sender identity, endpoint interoperability or uninterrupted session without corroborating evidence."}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--seconds", type=int, choices=DURATIONS, required=True)
    parser.add_argument("--platform", choices=("macos-arm64", "macos-x64", "windows-x64"), required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--profile", choices=("baseline", "720p15", "1080p30"), default="1080p30")
    args = parser.parse_args()
    ffprobe, ffmpeg = shutil.which("ffprobe"), shutil.which("ffmpeg")
    try:
        require(ffprobe and ffmpeg, "Install FFmpeg and FFprobe for offline QA")
        width, height, fps = {"baseline": (640,360,10), "720p15": (1280,720,15), "1080p30": (1920,1080,30)}[args.profile]
        report = verify(args.capture, args.seconds, fps, width, height, args.commit, args.platform, ffprobe, ffmpeg)
        print(json.dumps(report, indent=2, allow_nan=False))
        return 0 if report["automaticFileChecksPass"] else 1
    except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
        # Do not emit FFmpeg command arguments, raw logs or local file paths.
        print(json.dumps({"kind": "receiver-file-diagnostic", "automaticFileChecksPass": False,
            "physicalAcceptanceVerified": False,
            "error": str(error) if isinstance(error, ValueError) else type(error).__name__}), file=sys.stderr)
        return 1

if __name__ == "__main__":
    raise SystemExit(main())
