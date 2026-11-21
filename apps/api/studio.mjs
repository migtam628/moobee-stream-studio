// First-party MooBee scene state. Recording and streaming stay explicitly unsupported.
const DEFAULT_SCENES = ['Starting Soon', 'Gameplay', 'Be Right Back', 'Ending'];
export class StudioError extends Error {
  constructor(message, statusCode = 400) { super(message); this.statusCode = statusCode; }
}
const validName = value => typeof value === 'string' && value.trim().length >= 1 && value.trim().length <= 64 && !/[<>\r\n]/.test(value);
const validId = value => Number.isSafeInteger(value) && value > 0 && value <= 1_000_000_000;

export function validateCollection(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || value.version !== 1) {
    throw new StudioError('Invalid or unsupported scene collection version');
  }
  if (!Array.isArray(value.scenes) || value.scenes.length < 1 || value.scenes.length > 40) {
    throw new StudioError('Scene collection must contain between 1 and 40 scenes');
  }
  const names = new Set(), ids = new Set();
  const scenes = value.scenes.map(scene => {
    if (!scene || !validName(scene.name) || !Array.isArray(scene.sources) || scene.sources.length > 30) {
      throw new StudioError('Invalid scene or source count');
    }
    const name = scene.name.trim(), lower = name.toLowerCase();
    if (names.has(lower)) throw new StudioError('Scene names must be unique');
    names.add(lower);
    const sources = scene.sources.map(source => {
      if (!source || !validId(source.id) || ids.has(source.id) || source.kind !== 'display' ||
          typeof source.deviceId !== 'string' || !/^\d{1,4}$/.test(source.deviceId) ||
          !validName(source.name) || typeof source.enabled !== 'boolean') {
        throw new StudioError('Invalid display source in collection');
      }
      ids.add(source.id);
      return { id: source.id, kind: 'display', deviceId: source.deviceId, name: source.name.trim(), enabled: source.enabled };
    });
    return { name, sources };
  });
  if (typeof value.currentScene !== 'string' || !scenes.some(s => s.name === value.currentScene) ||
      typeof value.previewScene !== 'string' || !scenes.some(s => s.name === value.previewScene) ||
      typeof value.studioMode !== 'boolean') {
    throw new StudioError('Invalid scene selection in collection');
  }
  return { version: 1, scenes, currentScene: value.currentScene,
    previewScene: value.studioMode ? value.previewScene : value.currentScene,
    studioMode: value.studioMode };
}

export function createStudio({ onChange = () => {}, initial = null } = {}) {
  let draft = initial ? validateCollection(initial) : {
    version: 1,
    scenes: DEFAULT_SCENES.map(name => ({ name, sources: [] })),
    currentScene: DEFAULT_SCENES[0], previewScene: DEFAULT_SCENES[0], studioMode: false
  };
  const collection = () => JSON.parse(JSON.stringify(draft));
  const selected = () => draft.scenes.find(scene => scene.name === (draft.studioMode ? draft.previewScene : draft.currentScene));
  function status() {
    return {
      connected: true, address: 'local', engine: 'moobee-native-alpha',
      captureAvailable: false, recordAvailable: false, broadcastAvailable: false,
      currentScene: draft.currentScene, previewScene: draft.previewScene, studioMode: draft.studioMode,
      scenes: draft.scenes.map(scene => scene.name),
      sources: selected()?.sources.map(source => ({ ...source })) || [], inputs: [],
      streaming: false, recording: false, streamSeconds: 0,
      bitrateKbps: null, droppedFrames: null, fps: null, lastError: ''
    };
  }
  function commit() {
    const next = status();
    onChange(next, collection());
    return { ok: true, status: next };
  }
  function replaceCollection(value) {
    const next = validateCollection(value); // Validate completely before mutating existing data
    draft = next;
    return commit();
  }
  function perform(body) {
    if (!body || typeof body !== 'object' || Array.isArray(body)) throw new StudioError('Action must be an object');
    const { action, sceneName } = body;
    const active = selected();
    if (action === 'addScene') {
      if (!validName(sceneName)) throw new StudioError('Scene name must be 1–64 characters');
      if (draft.scenes.some(s => s.name.toLowerCase() === sceneName.trim().toLowerCase())) throw new StudioError('Scene name already exists', 409);
      if (draft.scenes.length >= 40) throw new StudioError('Scene limit reached');
      draft.scenes.push({ name: sceneName.trim(), sources: [] });
    } else if (action === 'renameScene') {
      const nextName = body.newName;
      const scene = draft.scenes.find(s => s.name === sceneName);
      if (!scene || !validName(nextName)) throw new StudioError('Invalid scene rename');
      if (draft.scenes.some(s => s !== scene && s.name.toLowerCase() === nextName.trim().toLowerCase())) {
        throw new StudioError('Scene name already exists', 409);
      }
      const old = scene.name;
      scene.name = nextName.trim();
      if (draft.currentScene === old) draft.currentScene = scene.name;
      if (draft.previewScene === old) draft.previewScene = scene.name;
    } else if (action === 'deleteScene') {
      if (draft.scenes.length < 2) throw new StudioError('Cannot delete the last scene');
      const index = draft.scenes.findIndex(s => s.name === sceneName);
      if (index < 0) throw new StudioError('Unknown scene');
      draft.scenes.splice(index, 1);
      if (draft.currentScene === sceneName) draft.currentScene = draft.scenes[0].name;
      if (draft.previewScene === sceneName) draft.previewScene = draft.currentScene;
    } else if (action === 'duplicateScene') {
      const scene = draft.scenes.find(s => s.name === sceneName);
      if (!scene) throw new StudioError('Unknown scene');
      if (draft.scenes.length >= 40) throw new StudioError('Scene limit reached');
      const base = scene.name.slice(0, 48);
      let name = `${base} Copy`, n = 2;
      while (draft.scenes.some(s => s.name.toLowerCase() === name.toLowerCase())) {
        name = `${base} Copy ${n++}`;
      }
      let nextId = Math.max(0, ...draft.scenes.flatMap(s => s.sources.map(src => src.id))) + 1;
      draft.scenes.push({ name, sources: scene.sources.map(src => ({ ...src, id: nextId++ })) });
    } else if (action === 'scene') {
      if (typeof sceneName !== 'string' || !draft.scenes.some(s => s.name === sceneName)) throw new StudioError('Unknown scene');
      if (draft.studioMode) draft.previewScene = sceneName;
      else { draft.currentScene = sceneName; draft.previewScene = sceneName; }
    } else if (action === 'studioMode') {
      if (typeof body.studioModeEnabled !== 'boolean') throw new StudioError('Invalid Studio Mode state');
      draft.studioMode = body.studioModeEnabled;
      if (!draft.studioMode) draft.previewScene = draft.currentScene;
    } else if (action === 'transition') {
      if (!draft.studioMode) throw new StudioError('Studio Mode is not enabled');
      draft.currentScene = draft.previewScene;
    } else if (action === 'addSource') {
      if (!active) throw new StudioError('No active scene');
      if (body.kind !== 'display') throw new StudioError('Only display sources are supported in this alpha');
      if (typeof body.deviceId !== 'string' || !/^\d{1,4}$/.test(body.deviceId) || !validName(body.name)) {
        throw new StudioError('Invalid display source');
      }
      if (active.sources.length >= 30) throw new StudioError('Source limit reached');
      const id = Math.max(0, ...draft.scenes.flatMap(s => s.sources.map(src => src.id))) + 1;
      active.sources.push({ id, kind: 'display', deviceId: body.deviceId, name: body.name.trim(), enabled: true });
    } else if (action === 'source') {
      const source = active?.sources.find(src => src.id === body.sceneItemId);
      if (!source || typeof body.enabled !== 'boolean') throw new StudioError('Invalid source');
      source.enabled = body.enabled;
    } else if (action === 'removeSource') {
      const index = active?.sources.findIndex(src => src.id === body.sceneItemId) ?? -1;
      if (index < 0 || !active) throw new StudioError('Unknown source');
      active.sources.splice(index, 1);
    } else if (action === 'record' || action === 'stream') {
      throw new StudioError('Native video encoding and output are not implemented in this alpha', 501);
    } else {
      throw new StudioError('Unknown or unsupported studio action');
    }
    return commit();
  }
  return { status, collection, replaceCollection, perform };
}
