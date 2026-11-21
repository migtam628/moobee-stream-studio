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
  await page.screenshot({ path: new URL('desktop.png', dir).pathname, fullPage: true });
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
  await page.getByRole('button', { name: 'Clips' }).first().click();
  await page.getByRole('heading', { name: 'Clips' }).waitFor();
  await page.getByRole('heading', { name: 'Browser screen recording' }).waitFor();
  await page.getByRole('button', { name: 'Record shared screen' }).waitFor();
  await page.getByText('Native test capture is only available in the installed Windows/macOS app', { exact: false }).waitFor();
  if (await page.getByRole('button', { name: 'Capture Silent GIF' }).isEnabled()) {
    throw new Error('Native device capture must remain disabled in the browser edition');
  }
  await page.screenshot({ path: new URL('clips.png', dir).pathname, fullPage: true });
  const mobile = await browser.newPage({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  await mobile.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'domcontentloaded' });
  await mobile.getByRole('heading', { name: 'Studio' }).waitFor();
  await mobile.screenshot({ path: new URL('mobile.png', dir).pathname, fullPage: true });
  const overflow = await mobile.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 2);
  if (overflow) throw new Error('Mobile viewport has unintended horizontal overflow');
  if (errors.length) throw new Error(`Browser errors: ${errors.join('; ')}`);
  console.log('Visual QA complete: desktop, overlay, mobile; alert event roundtrip verified');
} finally { if (browser) await browser.close(); server.kill(); }
