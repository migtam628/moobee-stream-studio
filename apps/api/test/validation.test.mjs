import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cleanAlert } from '../validation.mjs';
import { createStudio, StudioError } from '../studio.mjs';

test('alerts are bounded and stripped of markup', () => {
  const item = cleanAlert({ type: 'tip', name: '<script>VeryLong', message: '<b>Hello</b>' });
  assert.equal(item.type, 'tip');
  assert.ok(!item.name.includes('<'));
  assert.ok(!item.message.includes('>'));
});
test('standalone scene engine selects, previews, and transitions', () => {
  const engine = createStudio();
  const starting = engine.status().currentScene;
  engine.perform({ action: 'addScene', sceneName: 'Game' });
  engine.perform({ action: 'studioMode', studioModeEnabled: true });
  engine.perform({ action: 'scene', sceneName: 'Game' });
  assert.equal(engine.status().currentScene, starting);
  assert.equal(engine.status().previewScene, 'Game');
  engine.perform({ action: 'transition' });
  assert.equal(engine.status().currentScene, 'Game');
});
test('preview scene sources can be edited before taking them live', () => {
  const engine = createStudio();
  engine.perform({ action: 'addScene', sceneName: 'Live View' });
  engine.perform({ action: 'studioMode', studioModeEnabled: true });
  engine.perform({ action: 'scene', sceneName: 'Live View' });
  engine.perform({ action: 'addSource', kind: 'display', deviceId: '0', name: 'My Display' });
  assert.equal(engine.status().sources[0].name, 'My Display');
  engine.perform({ action: 'transition' });
  assert.equal(engine.status().currentScene, 'Live View');
});
test('output controls never fake a recording or live broadcast', () => {
  const engine = createStudio();
  for (const action of ['record', 'stream']) assert.throws(() => engine.perform({ action }), error => error instanceof StudioError && error.statusCode === 501);
  assert.equal(engine.status().streaming, false);
  assert.equal(engine.status().recording, false);
});
test('source input is constrained to valid display sources', () => {
  const engine = createStudio();
  assert.throws(() => engine.perform({ action: 'addSource', kind: 'display', deviceId: 'invalid', name: 'Screen' }), /Invalid display source/);
  engine.perform({ action: 'addSource', kind: 'display', deviceId: '0', name: 'Screen' });
  assert.equal(engine.status().sources[0].kind, 'display');
  engine.perform({ action: 'source', sceneItemId: engine.status().sources[0].id, enabled: false });
  assert.equal(engine.status().sources[0].enabled, false);
});

test('scene rename updates preview and program references without changing unrelated scenes', () => {
  const studio = createStudio();
  studio.perform({ action: 'studioMode', studioModeEnabled: true });
  studio.perform({ action: 'scene', sceneName: 'Gameplay' });
  studio.perform({ action: 'renameScene', sceneName: 'Gameplay', newName: 'Gameplay Live' });
  assert.equal(studio.status().previewScene, 'Gameplay Live');
  assert.equal(studio.status().currentScene, 'Starting Soon');
  studio.perform({ action: 'renameScene', sceneName: 'Starting Soon', newName: 'Lobby' });
  assert.equal(studio.status().currentScene, 'Lobby');
  assert.throws(() => studio.perform({ action: 'renameScene', sceneName: 'Lobby', newName: 'ENDING' }), /already exists/);
});

test('duplicated scenes use independent source IDs; source deletion preserves the original', () => {
  const studio = createStudio();
  studio.perform({ action: 'addSource', kind: 'display', deviceId: '0', name: 'Monitor' });
  const originalId = studio.status().sources[0].id;
  studio.perform({ action: 'duplicateScene', sceneName: 'Starting Soon' });
  studio.perform({ action: 'scene', sceneName: 'Starting Soon Copy' });
  const copyId = studio.status().sources[0].id;
  assert.notEqual(copyId, originalId);
  studio.perform({ action: 'removeSource', sceneItemId: copyId });
  assert.equal(studio.status().sources.length, 0);
  studio.perform({ action: 'scene', sceneName: 'Starting Soon' });
  assert.equal(studio.status().sources[0].id, originalId);
});

test('deleting selected scenes updates program/preview and prevents deleting the final scene', () => {
  const studio = createStudio();
  studio.perform({ action: 'scene', sceneName: 'Gameplay' });
  studio.perform({ action: 'deleteScene', sceneName: 'Gameplay' });
  assert.equal(studio.status().currentScene, 'Starting Soon');
  const left = studio.status().scenes;
  for (const scene of left.slice(1)) studio.perform({ action: 'deleteScene', sceneName: scene });
  assert.throws(() => studio.perform({ action: 'deleteScene', sceneName: left[0] }), /last scene/);
});

test('scene collection export and import round trip retains sources and studio mode', () => {
  const first = createStudio();
  first.perform({ action: 'addSource', kind: 'display', deviceId: '0', name: 'Left Display' });
  first.perform({ action: 'studioMode', studioModeEnabled: true });
  first.perform({ action: 'scene', sceneName: 'Gameplay' });
  const collection = first.collection();
  assert.equal(collection.version, 1);
  const second = createStudio({ initial: collection });
  assert.deepEqual(second.collection(), collection);
  second.perform({ action: 'transition' });
  assert.equal(second.status().currentScene, 'Gameplay');
});

test('invalid scene imports leave existing collection unchanged', () => {
  const studio = createStudio();
  const original = studio.collection();
  const invalids = [
    { ...original, version: 999 },
    { ...original, scenes: [] },
    { ...original, currentScene: 'missing' },
    { ...original, scenes: [...original.scenes, original.scenes[0]] },
    { ...original, scenes: [{ ...original.scenes[0], sources: [{ id: 1, kind: 'url', name: 'Injected', enabled: true }] }, ...original.scenes.slice(1)] }
  ];
  for (const value of invalids) {
    assert.throws(() => studio.replaceCollection(value), StudioError);
    assert.deepEqual(studio.collection(), original);
  }
});
