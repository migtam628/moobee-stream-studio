import test from 'node:test';
import assert from 'node:assert/strict';
import { assessNativeMp4, parseOptions, parseAudioPeakDbfs } from '../../../scripts/verify-native-mp4.mjs';

const probe = {
  format: { duration: '4.03', size: '1320000' },
  streams: [
    { codec_type: 'video', codec_name: 'h264', width: 1920, height: 1080, duration: '4.03' },
    { codec_type: 'audio', codec_name: 'aac', sample_rate: '48000', channels: 2 },
  ],
};

test('acceptance report requires H.264, bounded dimensions and plausible duration', () => {
  const result = assessNativeMp4(probe, { targetSeconds: 4, expectAudio: true });
  assert.equal(result.passed, true);
  assert.equal(result.audioStreams.length, 1);
  assert.equal(result.hardwarePlaybackVerified, false);
  assert.equal(result.audioSyncVerified, false);
});

test('audio presence is mandatory only when the test explicitly requested it', () => {
  const silent = { ...probe, streams: probe.streams.slice(0, 1) };
  assert.equal(assessNativeMp4(silent, { targetSeconds: 4 }).passed, true);
  assert.equal(assessNativeMp4(silent, { targetSeconds: 4, expectAudio: true }).passed, false);
});

test('rejects unsupported codec, absent video, invalid geometry and incomplete duration', () => {
  assert.equal(assessNativeMp4({ ...probe, streams: [{ ...probe.streams[0], codec_name: 'hevc' }] }).passed, false);
  assert.equal(assessNativeMp4({ ...probe, streams: [probe.streams[1]] }).passed, false);
  assert.equal(assessNativeMp4({ ...probe, streams: [{ ...probe.streams[0], width: 5000 }] }).passed, false);
  assert.equal(assessNativeMp4({ ...probe, format: { duration: '1.2' } }).passed, false);
  assert.equal(assessNativeMp4({ ...probe, format: { duration: 'N/A' } }).passed, false);
});

test('rejects invalid requested durations and badly formed metadata', () => {
  assert.equal(assessNativeMp4(probe, { targetSeconds: 0 }).passed, false);
  assert.equal(assessNativeMp4(probe, { targetSeconds: 121 }).passed, false);
  assert.equal(assessNativeMp4(probe, { targetSeconds: 120 }).passed, false); // sample duration is only 4 seconds
  assert.equal(assessNativeMp4(null).passed, false);
});

test('acceptance CLI parses explicit files and protects against partial options', () => {
  const options = parseOptions(['--file', '/tmp/test.mp4', '--seconds', '5', '--expect-audio', '--report', '/tmp/evidence.json']);
  assert.equal(options.file, '/tmp/test.mp4');
  assert.equal(options.targetSeconds, 5);
  assert.equal(options.expectAudio, true);
  assert.equal(options.report, '/tmp/evidence.json');
  assert.throws(() => parseOptions([]), /--file/);
  assert.throws(() => parseOptions(['--file']), /Usage/);
});

test('an early Stop and Save uses actual duration instead of the requested maximum', () => {
  const short = { ...probe, format: { duration: '15.25' } };
  const accepted = assessNativeMp4(short, { targetSeconds: 60, stoppedEarly: true });
  assert.equal(accepted.passed, true);
  assert.equal(accepted.stoppedEarly, true);
  assert.equal(accepted.durationSeconds, 15.25);
  assert.equal(assessNativeMp4(short, { targetSeconds: 60 }).passed, false);
  assert.equal(assessNativeMp4({ ...probe, format: { duration: '69.8' } },
    { targetSeconds: 60, stoppedEarly: true }).passed, false);
});

test('audible audio QA recognizes FFmpeg levels including effectively silent AAC', () => {
  assert.equal(parseAudioPeakDbfs('[Parsed_volumedetect_0] max_volume: -91.0 dB'), -91);
  assert.equal(parseAudioPeakDbfs('[Parsed_volumedetect_0] max_volume: -12.5 dB'), -12.5);
  assert.equal(parseAudioPeakDbfs('max_volume: -inf dB'), -Infinity);
  assert.equal(parseAudioPeakDbfs('no audio data'), null);
  const parsed = parseOptions(['--file', '/tmp/test.mp4', '--seconds', '60', '--stopped-early',
    '--expect-audible']);
  assert.equal(parsed.stoppedEarly, true);
  assert.equal(parsed.expectAudio, true);
  assert.equal(parsed.expectAudible, true);
});
