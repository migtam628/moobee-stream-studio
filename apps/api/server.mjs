import 'node:process';
import express from 'express';
import OBSWebSocket from 'obs-websocket-js';
import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cleanAlert, validateObsAddress } from './validation.mjs';

const app = express();
const serverDir = dirname(fileURLToPath(import.meta.url));
const webDist = join(serverDir, '..', 'web', 'dist');
const host = process.env.API_HOST || '127.0.0.1';
const port = Number(process.env.API_PORT || 8787);
const allowRemoteObs = process.env.ALLOW_REMOTE_OBS === 'true';
const obs = new OBSWebSocket();
const listeners = new Set();
const history = [];
let address = process.env.OBS_WS_URL || 'ws://127.0.0.1:4455';
let password = process.env.OBS_WS_PASSWORD || '';
let connected = false;
let lastError = '';
let status = initialStatus();

function initialStatus() {
  return { connected: false, address, currentScene: '', previewScene: '', studioMode: false, scenes: [], sources: [], inputs: [],
    streaming: false, recording: false, streamSeconds: 0, bitrateKbps: null, droppedFrames: null, fps: null, lastError: '' };
}

function publish(event, data) {
  const serialized = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
  for (const stream of listeners) stream.write(serialized);
}

function record(type, message) {
  const item = { id: crypto.randomUUID(), type, message, time: Date.now() };
  history.unshift(item);
  history.splice(50);
  publish('activity', item);
}

async function refresh() {
  if (!connected) return status;
  try {
    const [scenesResult, inputsResult, streamResult, recordResult, modeResult] = await Promise.allSettled([
      obs.call('GetSceneList'), obs.call('GetInputList'), obs.call('GetStreamStatus'), obs.call('GetRecordStatus'), obs.call('GetStudioModeEnabled')
    ]);
    const scenesData = scenesResult.status === 'fulfilled' ? scenesResult.value : null;
    const inputsData = inputsResult.status === 'fulfilled' ? inputsResult.value : null;
    const streamData = streamResult.status === 'fulfilled' ? streamResult.value : null;
    const recordData = recordResult.status === 'fulfilled' ? recordResult.value : null;
    const studioMode = modeResult.status === 'fulfilled' && !!modeResult.value.studioModeEnabled;
    const sceneName = scenesData?.currentProgramSceneName || '';
    let previewScene = sceneName;
    if (studioMode) {
      try { previewScene = (await obs.call('GetCurrentPreviewScene')).currentPreviewSceneName || sceneName; }
      catch { /* Studio mode may have been disabled between requests. */ }
    }
    let sources = [];
    if (sceneName) {
      try {
        const items = await obs.call('GetSceneItemList', { sceneName });
        sources = items.sceneItems.map(item => ({ id: item.sceneItemId, name: item.sourceName,
          enabled: item.sceneItemEnabled, kind: item.inputKind || item.sourceType || 'source' }));
      } catch { /* Scene may change between calls. */ }
    }
    const inputsWithCapabilities = await Promise.all((inputsData?.inputs || []).slice(0, 35).map(async input => {
      let muted = false;
      let volume = 1;
      let hasAudio = false;
      try { muted = (await obs.call('GetInputMute', { inputName: input.inputName })).inputMuted; hasAudio = true; } catch {}
      try { volume = (await obs.call('GetInputVolume', { inputName: input.inputName })).inputVolumeMul; hasAudio = true; } catch {}
      return hasAudio ? { name: input.inputName, kind: input.inputKind, muted, volume } : null;
    }));
    const inputs = inputsWithCapabilities.filter(Boolean);
    status = { connected: true, address, lastError: '', currentScene: sceneName, previewScene, studioMode,
      scenes: (scenesData?.scenes || []).map(scene => scene.sceneName).reverse(), sources, inputs,
      streaming: !!streamData?.outputActive, recording: !!recordData?.outputActive,
      streamSeconds: Math.round((streamData?.outputDuration || 0) / 1000),
      bitrateKbps: streamData?.outputBytes != null && streamData?.outputDuration > 0
        ? Math.round(streamData.outputBytes * 8 / streamData.outputDuration) : null,
      droppedFrames: streamData?.outputSkippedFrames ?? null,
      fps: null };
    publish('status', status);
  } catch (error) {
    lastError = String(error.message || error);
  }
  return status;
}

obs.on('ConnectionClosed', () => {
  connected = false;
  status = { ...initialStatus(), lastError: 'OBS disconnected' };
  publish('status', status);
  record('warning', 'OBS connection closed');
});
obs.on('CurrentProgramSceneChanged', ({ sceneName }) => {
  record('scene', `Live scene changed to ${sceneName}`);
  void refresh();
});
obs.on('CurrentPreviewSceneChanged', () => { void refresh(); });
obs.on('StudioModeStateChanged', () => { void refresh(); });
obs.on('StreamStateChanged', ({ outputActive }) => {
  record('broadcast', outputActive ? 'Stream started' : 'Stream stopped');
  void refresh();
});
obs.on('RecordStateChanged', ({ outputActive }) => {
  record('record', outputActive ? 'Recording started' : 'Recording stopped');
  void refresh();
});
obs.on('SceneItemEnableStateChanged', () => { void refresh(); });
obs.on('InputMuteStateChanged', () => { void refresh(); });

async function connectToObs(nextAddress = address, nextPassword = password) {
  const normalized = validateObsAddress(nextAddress, allowRemoteObs);
  await obs.disconnect().catch(() => {});
  connected = false;
  address = normalized;
  password = nextPassword;
  try {
    await obs.connect(address, password, { rpcVersion: 1 });
    connected = true;
    lastError = '';
    record('success', 'Connected to OBS Studio');
    return await refresh();
  } catch (error) {
    lastError = String(error.message || error);
    status = { ...initialStatus(), lastError };
    record('error', `OBS connection failed: ${lastError}`);
    return status;
  }
}

// The first milestone is strictly local. A future remote deployment needs auth and HTTPS.
app.use(express.json({ limit: '32kb' }));
app.disable('x-powered-by');
app.use((req, res, next) => {
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Referrer-Policy', 'no-referrer');
  next();
});

app.get('/api/health', (_req, res) => res.json({ ok: true, name: 'MooBee Live', version: '0.2.0' }));
app.get('/api/obs/status', async (_req, res) => res.json(await refresh()));
app.post('/api/obs/connect', async (req, res) => {
  try {
    const nextAddress = validateObsAddress(req.body?.address || address, allowRemoteObs);
    const nextPassword = typeof req.body?.password === 'string' ? req.body.password : password;
    if (nextPassword.length > 256) return res.status(400).json({ error: 'Password is too long' });
    const result = await connectToObs(nextAddress, nextPassword);
    res.status(result.connected ? 200 : 502).json(result);
  } catch (error) { res.status(400).json({ error: error.message }); }
});
app.post('/api/obs/disconnect', async (_req, res) => {
  await obs.disconnect().catch(() => {});
  connected = false;
  status = initialStatus();
  res.json({ ok: true });
});
app.post('/api/obs/action', async (req, res) => {
  if (!connected) return res.status(409).json({ error: 'Connect to OBS first' });
  const { action, sceneName, inputName, muted, volume, sceneItemId, enabled } = req.body || {};
  try {
    if (action === 'scene' && typeof sceneName === 'string' && status.scenes.includes(sceneName)) {
      if (status.studioMode) await obs.call('SetCurrentPreviewScene', { sceneName });
      else await obs.call('SetCurrentProgramScene', { sceneName });
    } else if (action === 'studioMode' && typeof req.body.studioModeEnabled === 'boolean') {
      await obs.call('SetStudioModeEnabled', { studioModeEnabled: req.body.studioModeEnabled });
    } else if (action === 'transition' && status.studioMode) {
      await obs.call('TriggerStudioModeTransition');
    } else if (action === 'stream') {
      await obs.call(status.streaming ? 'StopStream' : 'StartStream');
    } else if (action === 'record') {
      await obs.call(status.recording ? 'StopRecord' : 'StartRecord');
    } else if (action === 'mute' && typeof inputName === 'string' && status.inputs.some(i => i.name === inputName) && typeof muted === 'boolean') {
      await obs.call('SetInputMute', { inputName, inputMuted: muted });
    } else if (action === 'volume' && typeof inputName === 'string' && status.inputs.some(i => i.name === inputName) && typeof volume === 'number' && Number.isFinite(volume) && volume >= 0 && volume <= 2) {
      await obs.call('SetInputVolume', { inputName, inputVolumeMul: volume });
    } else if (action === 'source' && Number.isInteger(sceneItemId) && typeof enabled === 'boolean' && status.sources.some(i => i.id === sceneItemId)) {
      await obs.call('SetSceneItemEnabled', { sceneName: status.currentScene, sceneItemId, sceneItemEnabled: enabled });
    } else return res.status(400).json({ error: 'Unknown or invalid OBS action' });
    res.json({ ok: true, status: await refresh() });
  } catch (error) { res.status(502).json({ error: String(error.message || error) }); }
});
app.get('/api/obs/snapshot', async (req, res) => {
  if (!connected) return res.status(409).json({ error: 'Connect to OBS first' });
  const scene = String(req.query.scene || status.currentScene);
  if (!status.scenes.includes(scene)) return res.status(400).json({ error: 'Unknown scene' });
  try {
    const result = await obs.call('GetSourceScreenshot', { sourceName: scene,
      imageFormat: 'jpg', imageWidth: 800, imageHeight: 450, imageCompressionQuality: 65 });
    res.setHeader('Cache-Control', 'no-store');
    res.json({ image: result.imageData });
  } catch (error) { res.status(502).json({ error: String(error.message || error) }); }
});
app.get('/api/activity', (_req, res) => res.json(history));
app.get('/api/events', (req, res) => {
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache, no-transform');
  res.setHeader('Connection', 'keep-alive');
  res.flushHeaders();
  res.write(': MooBee Live connected\n\n');
  listeners.add(res);
  const timer = setInterval(() => res.write(': ping\n\n'), 25000);
  req.on('close', () => { clearInterval(timer); listeners.delete(res); });
});
app.post('/api/alerts/test', (req, res) => {
  try {
    const alert = cleanAlert(req.body || {});
    publish('alert', alert);
    record('alert', `Test ${alert.type} alert for ${alert.name}`);
    res.json({ ok: true, alert });
  } catch (error) { res.status(400).json({ error: error.message }); }
});

if (existsSync(webDist)) {
  app.use(express.static(webDist));
  app.get('/{*path}', (_req, res) => res.sendFile(join(webDist, 'index.html')));
}

if (process.env.NODE_ENV !== 'test') {
  app.listen(port, host, () => {
    console.log(`MooBee Live API at http://${host}:${port}`);
    void connectToObs().catch(error => console.warn(`OBS unavailable: ${error.message}`));
  });
  setInterval(() => { if (connected) void refresh(); }, 5000);
}

export { app };
