import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateObsAddress, cleanAlert } from '../validation.mjs';

test('local OBS WebSocket URLs are accepted', () => {
  assert.equal(validateObsAddress('ws://127.0.0.1:4455/'), 'ws://127.0.0.1:4455/');
  assert.equal(validateObsAddress('ws://localhost:4455'), 'ws://localhost:4455/');
});
test('untrusted remote OBS endpoints are rejected by default', () => {
  assert.throws(() => validateObsAddress('ws://example.com:4455'), /Only local/);
  assert.throws(() => validateObsAddress('http://localhost:4455'), /WebSocket/);
  assert.throws(() => validateObsAddress('ws://admin:pass@localhost:4455'), /without credentials/);
});
test('alerts are bounded and stripped of HTML tag brackets', () => {
  const item = cleanAlert({ type: 'tip', name: '<script>VeryLong', message: '<b>Hello</b>' });
  assert.equal(item.type, 'tip');
  assert.ok(!item.name.includes('<'));
  assert.ok(!item.message.includes('>'));
});
