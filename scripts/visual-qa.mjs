import { chromium } from 'playwright';
import { mkdir } from 'node:fs/promises';
import { spawn } from 'node:child_process';

const dir = new URL('../.qa/', import.meta.url);
await mkdir(dir, { recursive: true });
const root = new URL('../', import.meta.url);
const port = '18887';
const server = spawn(process.execPath, ['apps/api/server.mjs'], {
  cwd: root.pathname,
  env: { ...process.env, API_HOST: '127.0.0.1', API_PORT: port },
  stdio: 'ignore'
});
let browser;
try {
  let ready = false;
  for (let i = 0; i < 35; i++) {
    try { const res = await fetch(`http://127.0.0.1:${port}/api/health`); if (res.ok) { ready = true; break; } } catch {}
    await new Promise(resolve => setTimeout(resolve, 150));
  }
  if (!ready) throw new Error('Local API failed to start');
  browser = await chromium.launch({ headless: true, ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}) });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'domcontentloaded' });
  await page.getByRole('heading', { name: 'Studio' }).waitFor();
  await page.getByRole('button', { name: 'Record Program test' }).waitFor();
  await page.getByText('Local workspace', { exact: true }).waitFor();
  const program = page.getByRole('button', { name: 'Record Program test', exact: true });
  if (await program.isEnabled()) throw new Error('Native Program record must be disabled in browser-only edition');
  if (await page.getByRole('button', { name: 'Go Live (unavailable)', exact: true }).isEnabled()) throw new Error('Streaming gate must remain disabled');
  if (await page.getByRole('checkbox', { name: 'Enable microphone', exact: true }).isEnabled()) throw new Error('Native microphone cannot run in browser');
  const dockBounds = await page.locator('.production-docks').boundingBox();
  const recordBounds = await program.boundingBox();
  if (!dockBounds || !recordBounds || dockBounds.y + dockBounds.height > 901 || recordBounds.y + recordBounds.height > 901) {
    throw new Error('Production controls should fit in the desktop viewport');
  }
  const studioMode = page.getByRole('button', { name: 'Studio Mode', exact: true });
  if (await studioMode.getAttribute('aria-pressed') === 'true') await studioMode.click();
  await page.locator('.single-monitor').waitFor();
  if (await page.locator('.production-monitor').count() !== 1) throw new Error('Single Program monitor layout incorrect');
  await page.screenshot({ path: new URL('desktop.png', dir).pathname, fullPage: true });
  await studioMode.click();
  await page.locator('.dual-monitor').waitFor();
  if (await page.locator('.production-monitor').count() !== 2) throw new Error('Studio mode requires two monitors');
  await page.getByRole('button', { name: 'Take', exact: true }).waitFor();
  await page.screenshot({ path: new URL('desktop-studio-mode.png', dir).pathname, fullPage: true });
  await page.getByRole('button', { name: 'Activity', exact: true }).click();
  await page.getByRole('complementary', { name: 'Activity dock' }).waitFor();
  await page.getByRole('button', { name: 'Close activity', exact: true }).click();
  await page.getByRole('button', { name: 'Capture diagnostics', exact: true }).click();
  await page.getByRole('heading', { name: 'Quick screen capture — native test' }).waitFor();
  const nativeQuickCapture = page.locator('.quick-capture').getByRole('button', { name: 'Capture screen test' });
  if (await nativeQuickCapture.isEnabled()) throw new Error('Native Quick Capture must be disabled in browser-only edition');
  await page.getByRole('button', { name: 'Close capture diagnostics' }).click();
  if (await page.locator('dialog[open]').count()) throw new Error('Diagnostics dialog did not close');

  await page.getByRole('button', { name: 'Settings', exact: true }).first().click();
  await page.getByText('Runtime Service Diagnostics').waitFor();
  await page.getByText('RTMP / RTMPS broadcast').waitFor();
  await page.getByText('Not implemented', { exact: true }).first().waitFor();
  await page.getByRole('button', { name: 'Overlays' }).first().click();
  await page.getByRole('heading', { name: 'Overlays' }).waitFor();
  await page.getByRole('button', { name: 'Open alert test bench' }).click();
  await page.getByRole('button', { name: 'Send Test Alert' }).click();
  await page.getByText('Last test: ViceCityViewer').waitFor({ state: 'visible', timeout: 8000 });
  const overlay = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  await overlay.goto(`http://127.0.0.1:${port}/overlay/alerts`);
  await overlay.locator('html[data-overlay-connected="true"]').waitFor({ state: 'attached', timeout: 8000 });
  await page.getByRole('button', { name: 'Send Test Alert' }).click();
  await overlay.getByText('ViceCityViewer').waitFor();
  await overlay.screenshot({ path: new URL('overlay.png', dir).pathname });
  await page.getByRole('button', { name: 'Clips', exact: true }).first().click();
  await page.getByRole('heading', { name: 'Clips' }).waitFor();
  await page.getByRole('heading', { name: 'Browser screen recording' }).waitFor();
  await page.getByRole('button', { name: 'Record shared screen' }).waitFor();
  await page.getByText('Native test capture is only available in the installed Windows/macOS app', { exact: false }).waitFor();
  if (await page.getByRole('button', { name: 'Capture Silent GIF' }).isEnabled()) {
    throw new Error('Native device capture must remain disabled in the browser edition');
  }
  await page.screenshot({ path: new URL('clips.png', dir).pathname, fullPage: true });
  // Simulated browser stream tests UI wiring without attempting permissionless real capture.
  const recordingPage = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  await recordingPage.addInitScript(() => {
    const screenCanvas = document.createElement('canvas');
    screenCanvas.width = 640;
    screenCanvas.height = 360;
    const screenContext = screenCanvas.getContext('2d');
    screenContext.fillStyle = '#6839d0';
    screenContext.fillRect(0, 0, 640, 360);
    const cameraCanvas = document.createElement('canvas');
    cameraCanvas.width = 320;
    cameraCanvas.height = 180;
    const cameraContext = cameraCanvas.getContext('2d');
    cameraContext.fillStyle = '#3ba8b1';
    cameraContext.fillRect(0, 0, 320, 180);
    Object.defineProperty(navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getDisplayMedia: async () => screenCanvas.captureStream(30),
        getUserMedia: async options => {
          if (!options.video) return new MediaStream([]);
          return cameraCanvas.captureStream(30);
        }
      }
    });
    class FakeRecorder {
      static isTypeSupported(type) { return type.startsWith('video/webm'); }
      state = 'inactive';
      mimeType = 'video/webm';
      ondataavailable = null;
      onstop = null;
      onerror = null;
      start() { this.state = 'recording'; }
      stop() {
        if (this.state === 'inactive') return;
        this.state = 'inactive';
        this.ondataavailable?.({ data: new Blob(['sample-webm'], { type: 'video/webm' }) });
        this.onstop?.();
      }
    }
    Object.defineProperty(window, 'MediaRecorder', { configurable: true, value: FakeRecorder });
  });
  await recordingPage.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'domcontentloaded' });
  await recordingPage.getByRole('button', { name: 'Clips', exact: true }).first().click();
  await recordingPage.getByLabel('Webcam picture-in-picture').check();
  await recordingPage.getByLabel('Camera position').selectOption('left');
  await recordingPage.getByRole('button', { name: 'Record shared screen' }).click();
  await recordingPage.getByLabel('Live composited screen and camera preview').waitFor();

  await recordingPage.getByRole('button', { name: /Stop ·/ }).click();
  await recordingPage.getByText('Local recording ready').waitFor({ timeout: 5000 });
  const saveLink = recordingPage.getByRole('link', { name: 'Save recording' });
  const filename = await saveLink.getAttribute('download');
  if (!filename?.endsWith('.webm')) throw new Error('Browser recorder did not expose a WebM download');
  await recordingPage.screenshot({ path: new URL('browser-recorder.png', dir).pathname, fullPage: true });
  const mobile = await browser.newPage({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  await mobile.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'domcontentloaded' });
  await mobile.getByRole('heading', { name: 'Studio' }).waitFor();
  await mobile.screenshot({ path: new URL('mobile.png', dir).pathname, fullPage: true });
  if (await mobile.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 2)) throw new Error('Studio mobile horizontal overflow');
  await mobile.getByRole('button', { name: 'Open navigation' }).click();
  await mobile.getByRole('button', { name: 'Clips', exact: true }).click();
  await mobile.getByRole('heading', { name: 'Browser screen recording' }).waitFor();
  await mobile.screenshot({ path: new URL('mobile-clips.png', dir).pathname, fullPage: true });

  const overflow = await mobile.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 2);
  if (overflow) throw new Error('Mobile viewport has unintended horizontal overflow');
  if (errors.length) throw new Error(`Browser errors: ${errors.join('; ')}`);
  console.log('Visual QA complete: desktop, overlay, mobile; alert event roundtrip verified');
} finally { if (browser) await browser.close(); server.kill(); }
