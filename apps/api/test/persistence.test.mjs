import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';

const serverFile = join(dirname(fileURLToPath(import.meta.url)), '..', 'server.mjs');
async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}
async function startServer(port, directory) {
  const { NODE_TEST_CONTEXT: _testContext, ...environment } = process.env;
  const child = spawn(process.execPath, [serverFile], {
    env: { ...environment, NODE_ENV: 'production', API_HOST: '127.0.0.1',
      API_PORT: String(port), MOOBEE_DATA_DIR: directory },
    stdio: ['ignore', 'ignore', 'pipe']
  });
  let errors = '';
  child.stderr.on('data', chunk => { errors += String(chunk).slice(0, 4000); });
  const root = `http://127.0.0.1:${port}`;
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (child.exitCode !== null) break;
    try {
      if ((await fetch(`${root}/api/health`)).ok) { ready = true; break; }
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  if (!ready) { child.kill(); throw new Error(`Local MooBee API did not start: ${errors}`); }
  return { child, root };
}
async function stopServer(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  await new Promise(resolve => {
    const killer = setTimeout(() => child.kill('SIGKILL'), 4000);
    child.once('exit', () => { clearTimeout(killer); resolve(); });
    child.kill('SIGTERM');
  });
}

test('local scene collections persist across API restart and invalid files are retained', { timeout: 60000 }, async () => {
  const folder = mkdtempSync(join(tmpdir(), 'moobee-scene-test-'));
  const port = await freePort();
  let process1, process2, process3;
  try {
    process1 = await startServer(port, folder);
    const action = await fetch(`${process1.root}/api/studio/action`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action: 'addScene', sceneName: 'Persistent Studio' })
    });
    assert.equal(action.status, 200);
    assert.ok(JSON.parse(readFileSync(join(folder, 'scenes.v1.json'), 'utf8')).scenes
      .some(scene => scene.name === 'Persistent Studio'));
    await stopServer(process1.child);
    process2 = await startServer(port, folder);
    const restored = await (await fetch(`${process2.root}/api/studio/collection`)).json();
    assert.ok(restored.scenes.some(scene => scene.name === 'Persistent Studio'));
    await stopServer(process2.child);

    writeFileSync(join(folder, 'scenes.v1.json'), '{not-json');
    process3 = await startServer(port, folder);
    const reset = await (await fetch(`${process3.root}/api/studio/collection`)).json();
    assert.equal(reset.version, 1);
    assert.ok(reset.scenes.length > 0);
    assert.ok(readdirSync(folder).some(name => name.startsWith('scenes.v1.json.corrupt-')));
  } finally {
    for (const process of [process1, process2, process3]) if (process) await stopServer(process.child);
    rmSync(folder, { recursive: true, force: true });
  }
});
