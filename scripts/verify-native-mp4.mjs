#!/usr/bin/env node
// Offline acceptance check for MooBee's native H.264 MP4 experiment.
// FFprobe/FFmpeg are QA tools only: the installed app does not require them.
// No capture files are uploaded or inspected over a network.
import { execFileSync } from 'node:child_process';
import { statSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';
import { fileURLToPath } from 'node:url';

function numeric(value) {
  const n = Number(value);
  return Number.isFinite(n) ? n : NaN;
}

export function assessNativeMp4(probe, { targetSeconds = 4, expectAudio = false } = {}) {
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
    hardwarePlaybackVerified: false,
    audioSyncVerified: false,
  };
}

export function parseOptions(argv) {
  const options = { file: '', targetSeconds: 4, expectAudio: false, report: '' };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--expect-audio') { options.expectAudio = true; continue; }
    if (!['--file', '--seconds', '--report'].includes(flag) || i + 1 >= argv.length) {
      throw new Error('Usage: node scripts/verify-native-mp4.mjs --file <clip.mp4> --seconds 4 [--expect-audio] [--report evidence.json]');
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
    if (checked.passed) {
      // A successful ffprobe is not proof that every encoded frame can decode.
      try {
        execFileSync('ffmpeg', [
          '-hide_banner', '-nostdin', '-v', 'error', '-xerror',
          '-i', options.file, '-f', 'null', '-',
        ], { encoding: 'utf8', timeout: 120_000, maxBuffer: 2 * 1024 * 1024 });
      } catch (error) {
        checked.errors.push('FFmpeg full-file decode failed: ' + String(error.message).slice(0, 200));
        checked.passed = false;
      }
    }
    const report = {
      ...checked, fileName: basename(options.file), fileBytes: info.size,
      checkedAt: new Date().toISOString(), platform: process.platform, arch: process.arch,
      fullyDecodedWithFFmpeg: checked.passed,
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
