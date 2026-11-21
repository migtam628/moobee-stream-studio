import { test, after } from 'node:test';
import assert from 'node:assert/strict';
process.env.NODE_ENV = 'test';
const { app } = await import('../server.mjs');
const server = await new Promise(resolve => {
  const running = app.listen(0, '127.0.0.1', () => resolve(running));
});
const root = `http://127.0.0.1:${server.address().port}`;
after(async () => { await new Promise(resolve => server.close(resolve)); });

test('standalone API health and studio status', async () => {
  const health = await (await fetch(`${root}/api/health`)).json();
  assert.equal(health.engine, 'in-house');
  const response = await fetch(`${root}/api/studio/status`);
  assert.equal(response.status, 200);
  const studio = await response.json();
  assert.equal(studio.connected, true);
  assert.equal(studio.recordAvailable, false);
  assert.equal(studio.broadcastAvailable, false);
  assert.ok(studio.scenes.length);
});
test('scene action is controlled by MooBee, not a third-party studio', async () => {
  const response = await fetch(`${root}/api/studio/action`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action: 'addScene', sceneName: 'Community' })
  });
  const data = await response.json();
  assert.equal(response.status, 200);
  assert.ok(data.status.scenes.includes('Community'));
});
test('broadcast remains unavailable until implemented', async () => {
  const response = await fetch(`${root}/api/studio/action`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action: 'stream' })
  });
  assert.equal(response.status, 501);
});
test('local API test alerts need no external streaming software', async () => {
  const response = await fetch(`${root}/api/alerts/test`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ type: 'follow', name: 'Mig', message: 'Hello!' })
  });
  assert.equal(response.status, 200);
  assert.equal((await response.json()).alert.name, 'Mig');
});
