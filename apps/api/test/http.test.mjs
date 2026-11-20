import { test } from 'node:test';
import assert from 'node:assert/strict';
process.env.NODE_ENV = 'test';
const { app } = await import('../server.mjs');
const server = await new Promise(resolve => {
  const running = app.listen(0, '127.0.0.1', () => resolve(running));
});
const root = `http://127.0.0.1:${server.address().port}`;

test('API reports the version', async () => {
  const response = await fetch(`${root}/api/health`);
  assert.equal(response.status, 200);
  assert.equal((await response.json()).version, '0.2.0');
});
test('remote OBS connection is rejected by default', async () => {
  const response = await fetch(`${root}/api/obs/connect`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ address: 'ws://untrusted.example:4455', password: 'foo' })
  });
  assert.equal(response.status, 400);
});
test('a test alert is published without needing OBS', async () => {
  const response = await fetch(`${root}/api/alerts/test`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ type: 'follow', name: 'Mig', message: 'Hello!' })
  });
  const data = await response.json();
  assert.equal(response.status, 200);
  assert.equal(data.alert.name, 'Mig');
});
test('close HTTP test listener', async () => { await new Promise(resolve => server.close(resolve)); });
