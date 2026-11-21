export function cleanAlert(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Alert must be an object');
  const type = ['follow', 'subscription', 'tip', 'raid'].includes(value.type) ? value.type : 'follow';
  const name = String(value.name || 'Test Viewer').replace(/[<>\r\n]/g, '').trim().slice(0, 48);
  const message = String(value.message || 'Welcome to the stream!').replace(/[<>\r\n]/g, '').trim().slice(0, 120);
  return { id: crypto.randomUUID(), type, name, message, time: Date.now() };
}
