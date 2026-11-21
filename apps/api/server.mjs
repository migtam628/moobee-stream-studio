import express from 'express';
import { existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cleanAlert } from './validation.mjs';
import { createStudio, StudioError } from './studio.mjs';

const app = express();
const root = dirname(fileURLToPath(import.meta.url));
const webDist = join(root, '..', 'web', 'dist');
const overlayFile = join(root, '..', 'desktop', 'src-tauri', 'resources', 'overlay.html');
const host = process.env.API_HOST || '127.0.0.1';
const port = Number(process.env.API_PORT || 8787);
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
const studio = createStudio({ onChange: status => publish('status', status) });

app.disable('x-powered-by');
app.use(express.json({ limit: '32kb' }));
app.use((_req, res, next) => {
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('Referrer-Policy', 'no-referrer');
  next();
});
app.get('/api/health', (_req, res) => res.json({ ok: true, name: 'MooBee Studio', version: '0.3.0-alpha', engine: 'in-house' }));
app.get('/api/studio/status', (_req, res) => res.json(studio.status()));
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
