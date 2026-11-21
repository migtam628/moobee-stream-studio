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
