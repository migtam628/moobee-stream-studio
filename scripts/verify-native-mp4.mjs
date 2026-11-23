#!/usr/bin/env node
// Offline acceptance check for MooBee's native H.264 MP4 experiment.
// FFprobe/FFmpeg are QA tools only: the installed app does not require them.
// No capture files are uploaded or inspected over a network.
import { execFileSync, spawnSync } from 'node:child_process';
import { statSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';
import { fileURLToPath } from 'node:url';

function numeric(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : NaN;
}

export function assessNativeMp4(probe, { targetSeconds = 4, expectAudio = false, stoppedEarly = false } = {}) {
  const errors = [];
  const streams = Array.isArray(probe?.streams) ? probe.streams : [];
  const video = streams.find((s) => s.codec_type === 'video');
  const audio = streams.filter((s) => s.codec_type === 'audio');
  const duration = numeric(probe?.format?.duration ?? video?.duration);
  const width = numeric(video?.width);
  const height = numeric(video?.height);
  const codec = video?.codec_name ?? null;

  if (!video || codec !== 'h264') errors.push('Expected an H.264 video stream');
  if (!Number.isFinite(width) || !Number.isFinite(height) ||
      width < 16 || height < 16 || width > 1920 || height > 1080) {
    errors.push('Video dimensions are missing or exceed the 1080p prototype limit');
  }
  const requested = numeric(targetSeconds);
  if (!Number.isFinite(requested) || requested < 3 || requested > 120) {
    errors.push('Expected test duration must be between 3 and 120 seconds');
  } else if (stoppedEarly) {
    // A 60-second maximum interrupted at 15 seconds should not fail because
    // the recorded file is shorter than the originally selected maximum.
    if (!Number.isFinite(duration) || duration < 2 ||
        duration > requested + Math.max(1.5, requested * 0.1)) {
      errors.push('Stopped recording duration is invalid or exceeded its maximum');
    }
  } else if (!Number.isFinite(duration) || Math.abs(duration - requested) > Math.max(1.5, requested * 0.25)) {
    errors.push('Recorded duration differs materially from requested duration');
  }
  if (expectAudio && audio.length === 0) {
    errors.push('Audio was requested but no audio stream is present');
  }
  // Audio presence is only metadata evidence. Manual playback must verify
  // both sound sources, A/V synchronization, and permission behavior.
  return {
    passed: errors.length === 0,
    errors,
    video: { codec, width: Number.isFinite(width) ? width : null,
      height: Number.isFinite(height) ? height : null },
    durationSeconds: Number.isFinite(duration) ? duration : null,
    audioStreams: audio.map((stream) => ({
      codec: stream.codec_name ?? null,
      sampleRate: stream.sample_rate ?? null,
      channels: stream.channels ?? null,
    })),
    expectAudio,
    stoppedEarly,
    hardwarePlaybackVerified: false,
    audioSyncVerified: false,
  };
}

export function parseOptions(argv) {
  const options = { file: '', targetSeconds: 4, expectAudio: false, expectAudible: false, stoppedEarly: false, report: '' };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--expect-audio') { options.expectAudio = true; continue; }
    if (flag === '--expect-audible') { options.expectAudio = true; options.expectAudible = true; continue; }
    if (flag === '--stopped-early') { options.stoppedEarly = true; continue; }
    if (!['--file', '--seconds', '--report'].includes(flag) || i + 1 >= argv.length) {
      throw new Error('Usage: node scripts/verify-native-mp4.mjs --file <clip.mp4> --seconds 4 [--stopped-early] [--expect-audio] [--expect-audible] [--report evidence.json]');
    }
    const value = argv[++i];
    if (flag === '--file') options.file = value;
    else if (flag === '--seconds') options.targetSeconds = Number(value);
    else options.report = value;
  }
  if (!options.file) throw new Error('Provide a local clip path with --file');
  return options;
}

function run() {
  let options;
  try {
    options = parseOptions(process.argv.slice(2));
    const info = statSync(options.file);
    if (!info.isFile() || info.size < 4096 || info.size > 256 * 1024 * 1024) {
      throw new Error('Clip is empty, too small, or exceeds the 256 MiB prototype limit');
    }
    const metadata = JSON.parse(execFileSync('ffprobe', [
      '-v', 'error', '-show_entries',
      'format=duration,size:stream=codec_type,codec_name,width,height,duration,sample_rate,channels',
      '-of', 'json', options.file,
    ], { encoding: 'utf8', timeout: 30_000, maxBuffer: 2 * 1024 * 1024 }));
    const checked = assessNativeMp4(metadata, options);
    let fullyDecodedWithFFmpeg = false;
    if (checked.passed) {
      // A successful ffprobe is not proof that every encoded frame can decode.
      try {
        execFileSync('ffmpeg', [
          '-hide_banner', '-nostdin', '-v', 'error', '-xerror',
          '-i', options.file, '-f', 'null', '-',
        ], { encoding: 'utf8', timeout: 120_000, maxBuffer: 2 * 1024 * 1024 });
        fullyDecodedWithFFmpeg = true;
      } catch (error) {
        checked.errors.push('FFmpeg full-file decode failed: ' + String(error.message).slice(0, 200));
        checked.passed = false;
      }
    }
    // Metadata alone does not prove audible sound. When specifically requested,
    // measure the first audio track's peak level; an AAC track near -90 dBFS is
    // effectively silent and must not be counted as a successful audio test.
    // This uses local FFmpeg as a diagnostic only, not an app runtime dependency.
    let audioPeakDbfs = null;
    let audioLevelMeasured = false;
    if (checked.passed && options.expectAudible) {
      const result = spawnSync('ffmpeg', [
        '-hide_banner', '-nostdin', '-i', options.file, '-map', '0:a:0',
        '-af', 'volumedetect', '-f', 'null', '-',
      ], { encoding: 'utf8', timeout: 120_000, maxBuffer: 4 * 1024 * 1024 });
      const output = String(result.stderr ?? '');
      const peak = output.match(/max_volume:\s*(-?(?:\d+(?:\.\d+)?|inf))\s*dB/i);
      if (!result.error && result.status === 0 && peak) {
        const db = peak[1].toLowerCase() === '-inf' ? -Infinity : Number(peak[1]);
        audioPeakDbfs = Number.isFinite(db) ? db : null;
        audioLevelMeasured = true;
        if (db < -50) {
          checked.passed = false;
          checked.errors.push('Audio track is effectively silent (peak below -50 dBFS)');
        }
      } else {
        checked.passed = false;
        checked.errors.push('Could not measure local AAC audio volume');
      }
    }
    const report = {
      ...checked, fileName: basename(options.file), fileBytes: info.size,
      checkedAt: new Date().toISOString(), platform: process.platform, arch: process.arch,
      fullyDecodedWithFFmpeg,
      expectAudible: options.expectAudible,
      audioPeakDbfs, audioLevelMeasured,
      // Peak level is signal evidence, not proof of the intended source or A/V synchronization.
      // This script never claims real hardware audio, sight/sound, or sync acceptance.
    };
    const serialized = JSON.stringify(report, null, 2) + '\n';
    if (options.report) writeFileSync(options.report, serialized, { flag: 'wx', mode: 0o600 });
    process.stdout.write(serialized);
    if (!report.passed) process.exitCode = 1;
  } catch (error) {
    process.stderr.write(String(error.message ?? error) + '\n');
    process.exitCode = 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) run();
