import express from 'express';
import { existsSync, mkdirSync, readFileSync, writeFileSync, renameSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cleanAlert } from './validation.mjs';
import { createStudio, StudioError } from './studio.mjs';

const app = express();
const root = dirname(fileURLToPath(import.meta.url));
const webDist = join(root, '..', 'web', 'dist');
const overlayFile = join(root, '..', 'desktop', 'src-tauri', 'resources', 'overlay.html');
const host = process.env.API_HOST || '127.0.0.1';
const port = Number(process.env.API_PORT || 8787);
const stateDirectory = process.env.MOOBEE_DATA_DIR ? resolve(process.env.MOOBEE_DATA_DIR) : join(homedir(), '.moobee-stream-studio');
const stateFile = join(stateDirectory, 'scenes.v1.json');
const durable = process.env.NODE_ENV !== 'test';
function readCollection() {
  if (!durable || !existsSync(stateFile)) return null;
  try {
    return JSON.parse(readFileSync(stateFile, 'utf8'));
  } catch (error) {
    const recoveryFile = `${stateFile}.corrupt-${Date.now()}`;
    renameSync(stateFile, recoveryFile);
    console.warn(`MooBee could not parse saved scenes; the original data was preserved in ${recoveryFile}`, error.message);
    return null;
  }
}
function saveCollection(collection) {
  if (!durable) return;
  mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });
  const temporary = `${stateFile}.${process.pid}.tmp`;
  writeFileSync(temporary, JSON.stringify(collection, null, 2) + '\n', { encoding: 'utf8', mode: 0o600 });
  renameSync(temporary, stateFile);
}
const listeners = new Set();
const history = [];

function publish(event, data) {
  const payload = `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
  for (const stream of listeners) stream.write(payload);
}
function activity(type, message) {
  const item = { id: crypto.randomUUID(), type, message, time: Date.now() };
  history.unshift(item); history.splice(50);
  publish('activity', item);
}
let saved = readCollection();
let studio;
try {
  studio = createStudio({ initial: saved, onChange: (status, collection) => {
    saveCollection(collection);
    publish('status', status);
  } });
} catch (error) {
  if (durable && saved) {
    const recoveryFile = `${stateFile}.invalid-${Date.now()}`;
    renameSync(stateFile, recoveryFile);
    console.warn(`MooBee found an invalid saved scene collection; preserved original at ${recoveryFile}`);
    saved = null;
  }
  studio = createStudio({ onChange: (status, collection) => {
    saveCollection(collection);
    publish('status', status);
  } });
}

app.disable('x-powered-by');
// Guard the unauthenticated loopback API against DNS rebinding and hostile browser origins.
const trustedHostname = hostname => ['127.0.0.1', 'localhost', '[::1]'].includes(hostname.toLowerCase());
app.use((req, res, next) => {
  const hostName = (req.headers.host || '').replace(/:\d+$/, '');
  if (!trustedHostname(hostName)) return res.status(403).json({ error: 'MooBee local API only accepts loopback hosts' });
  if (req.headers.origin) {
    try {
      const origin = new URL(req.headers.origin);
      if (origin.protocol !== 'http:' || !trustedHostname(origin.hostname)) {
        return res.status(403).json({ error: 'Untrusted browser origin' });
      }
    } catch {
      return res.status(403).json({ error: 'Invalid browser origin' });
    }
  }
  next();
});
app.use(express.json({ limit: '128kb' }));
app.use((_req, res, next) => {
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Referrer-Policy', 'no-referrer');
  next();
});
app.get('/api/health', (_req, res) => res.json({ ok: true, name: 'MooBee Studio', version: '0.3.0-alpha', engine: 'in-house' }));
app.get('/api/studio/status', (_req, res) => res.json(studio.status()));
app.get('/api/studio/collection', (_req, res) => {
  res.setHeader('Cache-Control', 'no-store');
  res.json(studio.collection());
});
app.post('/api/studio/collection', (req, res) => {
  try {
    const result = studio.replaceCollection(req.body);
    activity('studio', 'Scene collection imported');
    res.json(result);
  } catch (error) {
    res.status(error instanceof StudioError ? error.statusCode : 500).json({ error: error.message });
  }
});
app.post('/api/studio/action', (req, res) => {
  try {
    const result = studio.perform(req.body);
    activity('studio', `Studio action: ${req.body.action}`);
    res.json(result);
  } catch (error) {
    res.status(error instanceof StudioError ? error.statusCode : 500).json({ error: error.message });
  }
});
app.get('/api/studio/snapshot', (_req, res) => {
  res.setHeader('Cache-Control', 'no-store');
  res.json({ image: null, available: false, reason: 'Display capture requires the native desktop app' });
});
// Media capture is device-local and not exposed by the browser control API.
app.get('/api/clips', (_req, res) => res.json({
  available: false, clips: [], reason: 'GIF test clips are available in the installed macOS/Windows desktop app'
}));
app.post('/api/clips/capture', (_req, res) => res.status(501).json({
  error: 'Install MooBee Studio for local test capture. Browser and cloud capture are not available.'
}));
app.get('/api/clips/preview', (_req, res) => res.status(501).json({
  error: 'Test clips are private files on the installed desktop app'
}));
app.get('/api/capture/displays', (_req, res) => res.json({ displays: [], available: false, reason: 'Screen access is only available in the installed desktop app' }));
app.get('/api/activity', (_req, res) => res.json(history));
app.get('/api/events', (req, res) => {
  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache, no-transform');
  res.setHeader('Connection', 'keep-alive');
  res.flushHeaders();
  res.write(': MooBee Studio event stream connected\n\n');
  listeners.add(res);
  const timer = setInterval(() => res.write(': ping\n\n'), 25000);
  req.on('close', () => { clearInterval(timer); listeners.delete(res); });
});
app.get('/overlay/alerts', (_req, res) => res.sendFile(overlayFile));
app.post('/api/alerts/test', (req, res) => {
  try {
    const alert = cleanAlert(req.body || {});
    publish('alert', alert);
    activity('alert', `Test ${alert.type} alert for ${alert.name}`);
    res.json({ ok: true, alert });
  } catch (error) { res.status(400).json({ error: error.message }); }
});

if (existsSync(webDist)) {
  app.use(express.static(webDist));
  app.get('/{*path}', (_req, res) => res.sendFile(join(webDist, 'index.html')));
}
if (process.env.NODE_ENV !== 'test') {
  if (host !== '127.0.0.1' && host !== 'localhost' && host !== '::1') {
    throw new Error('MooBee local control API must bind to loopback; a public server requires authentication and HTTPS');
  }
  app.listen(port, host, () => console.log(`MooBee Studio API: http://${host}:${port}`));
}
export { app };
