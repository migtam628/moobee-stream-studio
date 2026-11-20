export function validateObsAddress(input, allowRemote = false) {
  if (typeof input !== 'string' || input.length > 256) throw new Error('Invalid WebSocket address');
  let url;
  try { url = new URL(input); } catch { throw new Error('Enter a valid ws:// or wss:// address'); }
  if (!['ws:', 'wss:'].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash) {
    throw new Error('Address must be a WebSocket URL without credentials, query, or fragment');
  }
  if (!allowRemote && !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname)) {
    throw new Error('Only local OBS connections are allowed. Set ALLOW_REMOTE_OBS=true for trusted remote OBS hosts.');
  }
  return url.href;
}

export function cleanAlert(value) {
  if (!value || typeof value !== 'object') throw new Error('Alert must be an object');
  const type = ['follow', 'subscription', 'tip', 'raid'].includes(value.type) ? value.type : 'follow';
  const name = String(value.name || 'Test Viewer').replace(/[<>]/g, '').trim().slice(0, 48);
  const message = String(value.message || 'Welcome to the stream!').replace(/[<>]/g, '').trim().slice(0, 120);
  return { id: crypto.randomUUID(), type, name, message, time: Date.now() };
}
