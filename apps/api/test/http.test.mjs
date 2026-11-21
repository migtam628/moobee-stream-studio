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

test('scene collection API imports validated state and exports it without a backend dependency', async () => {
  const initialResponse = await fetch(`${root}/api/studio/collection`);
  assert.equal(initialResponse.status, 200);
  assert.equal(initialResponse.headers.get('cache-control'), 'no-store');
  const initial = await initialResponse.json();
  const imported = { ...initial, currentScene: 'Gameplay', previewScene: 'Gameplay',
    scenes: [...initial.scenes, { name: 'Dashboard Test', sources: [] }] };
  const response = await fetch(`${root}/api/studio/collection`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(imported)
  });
  assert.equal(response.status, 200);
  const exported = await (await fetch(`${root}/api/studio/collection`)).json();
  assert.ok(exported.scenes.some(scene => scene.name === 'Dashboard Test'));
  assert.equal(exported.currentScene, 'Gameplay');
});

test('invalid collection imports return 400 without replacing current state', async () => {
  const original = await (await fetch(`${root}/api/studio/collection`)).json();
  const response = await fetch(`${root}/api/studio/collection`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ...original, scenes: [] })
  });
  assert.equal(response.status, 400);
  assert.deepEqual(await (await fetch(`${root}/api/studio/collection`)).json(), original);
});

test('local API rejects cross-origin browser writes before reaching the scene controller', async () => {
  const response = await fetch(`${root}/api/studio/action`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: 'https://untrusted.example' },
    body: JSON.stringify({ action: 'addScene', sceneName: 'Untrusted' })
  });
  assert.equal(response.status, 403);
  const scenes = await (await fetch(`${root}/api/studio/status`)).json();
  assert.ok(!scenes.scenes.includes('Untrusted'));
});

test('web control API never pretends to have device-level clip capture', async () => {
  const list = await fetch(`${root}/api/clips`);
  assert.equal(list.status, 200);
  const clips = await list.json();
  assert.equal(clips.available, false);
  assert.deepEqual(clips.clips, []);
  const requestCapture = await fetch(`${root}/api/clips/capture`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ displayId: 0, seconds: 4 })
  });
  assert.equal(requestCapture.status, 501);
  const requestPreview = await fetch(`${root}/api/clips/preview?id=not-a-real-clip`);
  assert.equal(requestPreview.status, 501);
});
