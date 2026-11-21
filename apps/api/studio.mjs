// MooBee's local scene-control model. No third-party broadcasting process or remote control protocol.
const DEFAULT_SCENES = ['Starting Soon', 'Gameplay', 'Be Right Back', 'Ending'];
export class StudioError extends Error {
  constructor(message, statusCode = 400) { super(message); this.statusCode = statusCode; }
}
const validName = value => typeof value === 'string' && value.trim().length > 0 && value.trim().length <= 64 && !/[<>\r\n]/.test(value);
export function createStudio({ onChange = () => {} } = {}) {
  const scenes = DEFAULT_SCENES.map(name => ({ name, sources: [] }));
  let currentScene = scenes[0].name;
  let previewScene = currentScene;
  let studioMode = false;
  let nextSourceId = 1;
  function status() {
    const scene = scenes.find(item => item.name === currentScene);
    return {
      connected: true, address: 'local', engine: 'moobee-native-alpha',
      captureAvailable: false, recordAvailable: false, broadcastAvailable: false,
      currentScene, previewScene, studioMode,
      scenes: scenes.map(item => item.name),
      sources: scene?.sources.map(item => ({ ...item })) || [], inputs: [],
      streaming: false, recording: false, streamSeconds: 0,
      bitrateKbps: null, droppedFrames: null, fps: null, lastError: ''
    };
  }
  function perform(body) {
    if (!body || typeof body !== 'object') throw new StudioError('Action must be an object');
    const { action, sceneName } = body;
    const active = scenes.find(item => item.name === currentScene);
    if (action === 'addScene') {
      if (!validName(sceneName)) throw new StudioError('Scene name must be 1–64 characters');
      if (scenes.some(item => item.name.toLowerCase() === sceneName.trim().toLowerCase())) throw new StudioError('Scene name already exists', 409);
      if (scenes.length >= 40) throw new StudioError('Scene limit reached');
      scenes.push({ name: sceneName.trim(), sources: [] });
    } else if (action === 'scene') {
      if (!scenes.some(item => item.name === sceneName)) throw new StudioError('Unknown scene');
      if (studioMode) previewScene = sceneName;
      else { currentScene = sceneName; previewScene = sceneName; }
    } else if (action === 'studioMode') {
      if (typeof body.studioModeEnabled !== 'boolean') throw new StudioError('Invalid Studio Mode state');
      studioMode = body.studioModeEnabled;
      if (!studioMode) previewScene = currentScene;
    } else if (action === 'transition') {
      if (!studioMode) throw new StudioError('Studio Mode is not enabled');
      currentScene = previewScene;
    } else if (action === 'addSource') {
      if (!active) throw new StudioError('No active scene');
      if (body.kind !== 'display') throw new StudioError('Only display sources are supported in this alpha');
      if (typeof body.deviceId !== 'string' || !/^\d{1,4}$/.test(body.deviceId) || !validName(body.name)) throw new StudioError('Invalid display source');
      if (active.sources.length >= 30) throw new StudioError('Source limit reached');
      active.sources.push({ id: nextSourceId++, kind: 'display', deviceId: body.deviceId, name: body.name.trim(), enabled: true });
    } else if (action === 'source') {
      const source = active?.sources.find(item => item.id === body.sceneItemId);
      if (!source || typeof body.enabled !== 'boolean') throw new StudioError('Invalid source');
      source.enabled = body.enabled;
    } else if (action === 'record' || action === 'stream') {
      throw new StudioError('Native video encoding and output are not implemented in this alpha', 501);
    } else {
      throw new StudioError('Unknown or unsupported studio action');
    }
    const next = status();
    onChange(next);
    return { ok: true, status: next };
  }
  return { status, perform };
}
