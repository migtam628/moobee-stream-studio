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
  // Exercise the production dock's real frontend IPC route with a simulated
  // macOS host. This checks request wiring, not camera/encoder hardware.
  const macPage = await browser.newPage({ viewport: { width: 1440, height: 900 },
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 14_2) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36' });
  macPage.on('pageerror', error => errors.push(error.message));
  await macPage.addInitScript(() => {
    window.isTauri = true;
    localStorage.setItem('moobee-native-studio.v1', JSON.stringify({
      version: 1, currentScene: 'Gameplay', previewScene: 'Preview', studioMode: true,
      scenes: [
        { name: 'Gameplay', layout: 'pip', sources: [{id: 1, name: 'Test display', kind: 'display', deviceId: '0', enabled: true}] },
        { name: 'Preview', layout: 'grid', sources: [{id: 2, name: 'Other display', kind: 'display', deviceId: '1', enabled: true}] }
      ]
    }));
    window.__programRequests = [];
    window.__snapshotCount = 0;
    window.__movingCalls = [];
    window.__captureOrder = [];
    window.__audioCalls = [];
    const clip = {id:'e9525c58-0e63-40b3-94b6-a9d0f0de4e99', path:'/test/clip.mp4', format:'mp4', bytes:200000, durationSeconds:4, displayCount:1, microphoneIncluded:true, webcamIncluded:true, overlayIncluded:true, audioPeak:0.42, audioSamplesWritten:192000, width:1920, height:1080, fps:30};
    const canvas = document.createElement('canvas'); canvas.width=960; canvas.height=540;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle='#132738'; ctx.fillRect(0,0,960,540);
    ctx.fillStyle='#396986'; ctx.fillRect(60,90,840,360);
    ctx.font='36px sans-serif'; ctx.fillStyle='white'; ctx.fillText('Simulated display · UI wiring test',100,285);
    const image = canvas.toDataURL('image/png');
    window.__TAURI_INTERNALS__ = { invoke: async (cmd,args) => {
      if (cmd === 'list_audio_inputs') return [{id:'mic-test',name:'USB Microphone',isDefault:true},{id:'hdmi-audio-test',name:'HDMI Capture Audio',isDefault:false}];
      if (cmd === 'program_audio_levels') { window.__audioCalls.push(args.inputs); return {inputPeaks:args.inputs.map(x=>x.muted?0:0.25),mixedPeak:0.5,clippedSamples:0}; }
      if (cmd === 'stop_program_audio_levels') { window.__captureOrder.push('stop-audio'); return null; }
      if (cmd === 'list_video_inputs') return [{id:'uvc-card-test',name:'USB HDMI Capture (simulated)',isDefault:false},{id:'webcam-test',name:'FaceTime Camera (simulated)',isDefault:true}];
      if (cmd === 'live_video_preview_frame') { window.__movingCalls.push(args); return {image,frameKey:String(window.__movingCalls.length),liveVideo:true,waiting:false}; }
      if (cmd === 'stop_video_previews') { window.__captureOrder.push('stop-preview'); return null; }
      if (cmd === 'test_video_input') { window.__testedDevice = args.deviceId; return image; }
      if (cmd === 'list_displays') return [{id:0,name:'Test display',width:1920,height:1080},{id:1,name:'Other display',width:1280,height:720}];
      if (cmd === 'macos_mp4_availability') return {supported:true,requiredMacos:'14.0',microphoneSupported:true};
      if (cmd === 'capture_scene_snapshot') { window.__snapshotCount++; return image; }
      if (cmd === 'list_test_clips') return window.__programRequests.length ? [clip] : [];
      if (cmd === 'capture_continuous_program_mp4') {
        window.__captureOrder.push('record'); window.__programRequests.push(args);
        if (args.seconds === 60) return await new Promise(resolve => { window.__pendingProgram = {id:args.sessionId, resolve, paused:false}; });
        return clip;
      }
      if (cmd === 'continuous_program_stats') {
        const pending = window.__pendingProgram;
        if (!pending || pending.id !== args.sessionId) return null;
        return {phase:pending.paused?'paused':'recording',recordedFrames:60,fps:30,elapsedMs:2500,pausedMs:pending.paused?500:0,fileBytes:262144,lateFrames:1,repeatedSourceSlots:2};
      }
      if (cmd === 'pause_continuous_program_mp4') {
        if (window.__pendingProgram?.id !== args.sessionId) return false;
        window.__pendingProgram.paused = args.paused;
        return true;
      }
      if (cmd === 'recover_finalized_program_recordings') return {recovered:1,skipped:0,failed:0};
      if (cmd === 'stop_continuous_program_mp4') {
        if (window.__pendingProgram?.id !== args.sessionId) return false;
        const pending = window.__pendingProgram;
        window.__pendingProgram = null;
        pending.resolve({...clip, requestedMaxSeconds:30, stoppedEarly:true});
        return true;
      }
      throw new Error('Unexpected simulated command: '+cmd);
    }};
  });
  await macPage.goto(`http://127.0.0.1:${port}/`, {waitUntil:'domcontentloaded'});
  await macPage.getByRole('checkbox',{name:'Webcam inset',exact:true}).check();
  await macPage.getByRole('checkbox',{name:'Enable microphone',exact:true}).check();
  await macPage.getByLabel('Microphone device',{exact:true}).selectOption('mic-test');
  await macPage.getByLabel('Microphone gain',{exact:true}).fill('0.6');
  await macPage.getByRole('checkbox',{name:'Enable HDMI audio',exact:true}).check();
  await macPage.getByLabel('Capture card audio device',{exact:true}).selectOption('hdmi-audio-test');
  await macPage.getByLabel('HDMI gain',{exact:true}).fill('0.8');
  await macPage.getByRole('checkbox',{name:'Enable desktop audio',exact:true}).check();
  await macPage.getByLabel('Desktop gain',{exact:true}).fill('0.4');
  await macPage.getByRole('checkbox',{name:'Live audio meters',exact:true}).check();
  await macPage.waitForFunction(()=>window.__audioCalls.some(x=>x.length===3));
  await macPage.waitForFunction(()=>document.querySelector('[aria-label="Program mix live peak"]')?.getAttribute('aria-valuenow')==='50');
  await macPage.getByRole('button',{name:'Mute HDMI',exact:true}).click();
  await macPage.waitForFunction(()=>window.__audioCalls.at(-1)?.[1]?.muted===true);
  await macPage.getByRole('button',{name:'Unmute HDMI',exact:true}).click();
  await macPage.getByLabel('HDMI gain',{exact:true}).fill('0.8');
  await macPage.getByLabel('Continuous Program test profile').selectOption('1080p30');
  const png = await macPage.evaluate(() => {
    const canvas=document.createElement('canvas');canvas.width=20;canvas.height=20;
    const ctx=canvas.getContext('2d');ctx.fillStyle='rgba(255,0,0,0.5)';ctx.fillRect(0,0,20,20);
    return canvas.toDataURL('image/png').split(',')[1];
  });
  await macPage.getByLabel('Import PNG overlay',{exact:true}).setInputFiles({name:'test-overlay.png',mimeType:'image/png',buffer:Buffer.from(png,'base64')});
  await macPage.getByRole('button',{name:'Remove graphic',exact:true}).waitFor();
  await macPage.getByLabel('Graphic horizontal position',{exact:true}).fill('100');
  await macPage.getByLabel('Graphic vertical position',{exact:true}).fill('75');
  await macPage.getByLabel('Graphic width',{exact:true}).fill('30');
  await macPage.getByLabel('Preview refresh',{exact:true}).selectOption('paused');
  await macPage.getByText('Preview paused',{exact:true}).first().waitFor();
  const pausedCount = await macPage.evaluate(() => window.__snapshotCount);
  await macPage.waitForTimeout(1200);
  if (await macPage.evaluate(() => window.__snapshotCount) !== pausedCount) throw new Error('Paused snapshots continued');
  await macPage.getByRole('navigation').getByRole('button',{name:'Settings',exact:true}).click();
  await macPage.getByRole('heading',{name:'Settings',exact:true}).waitFor();
  await macPage.getByRole('navigation').getByRole('button',{name:'Studio',exact:true}).click();
  if (!(await macPage.getByRole('checkbox',{name:'Enable microphone',exact:true}).isChecked())) throw new Error('Mic selection reset during navigation');
  if (!(await macPage.getByRole('checkbox',{name:'Webcam inset',exact:true}).isChecked())) throw new Error('Camera selection reset during navigation');
  if (await macPage.getByLabel('Graphic width',{exact:true}).inputValue() !== '30') throw new Error('Overlay settings reset');
  await macPage.getByLabel('Preview refresh',{exact:true}).selectOption('fast');
  await macPage.waitForFunction(count => window.__snapshotCount > count, pausedCount);
  await macPage.getByRole('button',{name:'Record Program test',exact:true}).click();
  await macPage.getByRole('button',{name:'Saved MP4 · View result',exact:true}).waitFor();
  const [request] = await macPage.evaluate(() => window.__programRequests);
  if (!request || request.displayIds.join() !== '0' || request.layout !== 'pip' || request.profile !== '1080p30' ||
      request.seconds !== 4 || request.webcam !== true || request.microphone !== true || Math.abs(request.micGain-0.6)>0.001 ||
      request.overlay?.width !== 20 || request.overlay?.height !== 20 || request.overlay?.placement?.xPercent !== 100 || request.overlay?.placement?.yPercent !== 75 || request.overlay?.placement?.widthPercent !== 30) {
    throw new Error('Production controls did not route Program scene, webcam, microphone, gain and graphic to native IPC');
  }
  if (request.audioInputs?.length !== 3 || request.audioInputs[0].deviceId !== 'mic-test' ||
      request.audioInputs[1].deviceId !== 'hdmi-audio-test' || request.audioInputs[1].gain !== 0.8 ||
      request.audioInputs[2].kind !== 'desktop' || request.audioInputs[2].gain !== 0.4) {
    throw new Error('Named audio mix was not sent to native recording');
  }
  if (!(await macPage.evaluate(()=>window.__captureOrder.indexOf('stop-audio')>=0 && window.__captureOrder.indexOf('stop-audio')<window.__captureOrder.indexOf('record')))) throw new Error('Audio meter was not released before recording');
  await macPage.getByRole('checkbox',{name:'Live audio meters',exact:true}).uncheck();
  if (await macPage.getByRole('meter',{name:'Last recorded microphone peak'}).getAttribute('aria-valuenow') !== '42') {
    throw new Error('Microphone meter did not reflect the returned capture result');
  }
  await macPage.screenshot({path:new URL('desktop-native-simulated.png',dir).pathname,fullPage:true});
  await macPage.getByRole('button',{name:'Saved MP4 · View result',exact:true}).click();
  await macPage.getByText('Static graphic overlay included in output.',{exact:true}).waitFor();
  await macPage.getByRole('button',{name:'Close capture diagnostics'}).click();
  await macPage.getByRole('button',{name:'Studio Mode',exact:true}).click();
  await macPage.getByLabel('Choose capture source',{exact:true}).selectOption('video:uvc-card-test');
  await macPage.getByRole('button',{name:'Test video signal',exact:true}).click();
  await macPage.getByText(/Video frame received/).waitFor();
  if (await macPage.evaluate(() => window.__testedDevice) !== 'uvc-card-test') throw new Error('Wrong video input tested');
  await macPage.getByRole('button',{name:'+ Add source',exact:true}).click();
  const displayRow = macPage.locator('.source-row').filter({hasText:'Test display'});
  await displayRow.getByTitle('Hide source').click();
  await macPage.getByRole('checkbox',{name:'Webcam inset',exact:true}).uncheck();
  await macPage.getByLabel('Preview refresh',{exact:true}).selectOption('live');
  await macPage.waitForFunction(() => window.__movingCalls.length >= 3);
  if (!(await macPage.evaluate(() => window.__movingCalls.every(x => x.deviceIds.join() === 'uvc-card-test')))) throw new Error('Wrong moving preview device');
  await macPage.getByLabel('Choose capture source',{exact:true}).selectOption('video:webcam-test');
  await macPage.getByRole('button',{name:'+ Add source',exact:true}).click();
  await macPage.waitForFunction(() => window.__movingCalls.some(x => x.deviceIds.join() === 'uvc-card-test,webcam-test'));
  await macPage.locator('.source-row').filter({hasText:'FaceTime Camera (simulated)'}).getByTitle('Hide source').click();
  await macPage.waitForFunction(() => window.__movingCalls.at(-1)?.deviceIds.join() === 'uvc-card-test');
  await macPage.getByRole('button',{name:'Record Program test',exact:true}).click();
  await macPage.waitForFunction(() => window.__programRequests.length === 2);
  const cardRequest = await macPage.evaluate(() => window.__programRequests[1]);
  if (cardRequest.videoDeviceId !== 'uvc-card-test' || cardRequest.displayIds.length || cardRequest.webcam) throw new Error('Capture card Program input not routed correctly');
  if (!(await macPage.evaluate(() => window.__captureOrder.slice(-2).join() === 'stop-preview,record'))) throw new Error('Recording started before preview released video devices');
  const afterRecordCalls = await macPage.evaluate(() => window.__movingCalls.length);
  await macPage.waitForFunction(count => window.__movingCalls.length > count, afterRecordCalls);
  await macPage.getByLabel('Preview refresh',{exact:true}).selectOption('paused');
  await macPage.waitForFunction(() => window.__captureOrder.at(-1) === 'stop-preview');
  const liveCount = await macPage.evaluate(() => window.__movingCalls.length);
  await macPage.waitForTimeout(350);
  if (await macPage.evaluate(count => window.__movingCalls.length !== count, liveCount)) throw new Error('Paused video preview still requested frames');
  await macPage.getByRole('button',{name:'Edit sources',exact:true}).click();
  await macPage.getByLabel('Width percent',{exact:true}).fill('50');
  await macPage.getByLabel('Height percent',{exact:true}).fill('50');
  await macPage.getByLabel('X position percent',{exact:true}).fill('25');
  await macPage.getByLabel('Y position percent',{exact:true}).fill('25');
  await macPage.getByLabel('Crop left percent',{exact:true}).fill('10');
  await macPage.getByLabel('Source rotation',{exact:true}).selectOption('90');
  await macPage.getByRole('button',{name:'Apply transform',exact:true}).click();
  await macPage.waitForFunction(()=>JSON.parse(localStorage.getItem('moobee-native-studio.v1')).scenes.find(s=>s.name==='Gameplay')?.sources.find(s=>s.deviceId==='uvc-card-test')?.transform?.rotation===90);
  await macPage.getByLabel('Preview refresh',{exact:true}).selectOption('live');
  await macPage.waitForFunction(()=>window.__movingCalls.some(x=>x.transforms?.[0]?.rotation===90&&x.transforms[0].width===5000));
  await macPage.getByRole('button',{name:'Record Program test',exact:true}).click();
  await macPage.waitForFunction(()=>window.__programRequests.length===3);
  const transformed=await macPage.evaluate(()=>window.__programRequests[2].transforms?.[0]);
  if(transformed?.x!==2500||transformed?.height!==5000||transformed?.cropLeft!==1000||transformed?.rotation!==90)throw new Error('Manual source geometry did not reach native recorder');
  await macPage.getByRole('button',{name:'Saved MP4 · View result',exact:true}).waitFor();
  await macPage.getByLabel('Composed Program video test duration',{exact:true}).selectOption('60');
  await macPage.getByLabel('Program MP4 target bitrate',{exact:true}).selectOption('3000');
  await macPage.getByRole('button',{name:'Record Program test',exact:true}).click();
  await macPage.waitForFunction(()=>!!window.__pendingProgram);
  await macPage.getByText('recording · 2.0s media',{exact:true}).waitFor();
  await macPage.getByRole('button',{name:'Pause Program recording',exact:true}).click();
  await macPage.getByText('paused · 2.0s media',{exact:true}).waitFor();
  await macPage.getByRole('button',{name:'Resume Program recording',exact:true}).click();
  await macPage.getByText('recording · 2.0s media',{exact:true}).waitFor();
  await macPage.getByRole('button',{name:'Stop and save Program MP4',exact:true}).click();
  await macPage.getByRole('button',{name:'Saved MP4 · View result',exact:true}).waitFor();
  const longer = await macPage.evaluate(()=>window.__programRequests.at(-1));
  if (longer.seconds!==60 || longer.bitrateKbps!==3000 || !longer.sessionId) throw new Error('Long Program capture did not route session');
  await macPage.close();
  const mobile = await browser.newPage({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1 });
  await mobile.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'domcontentloaded' });
  await mobile.getByRole('heading', { name: 'Studio' }).waitFor();
  await mobile.screenshot({ path: new URL('mobile.png', dir).pathname, fullPage: true });
  if (await mobile.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 2)) throw new Error('Studio mobile horizontal overflow');
  await mobile.getByRole('button', { name: 'Open navigation' }).click();
  await mobile.getByRole('navigation').getByRole('button', { name: 'Clips', exact: true }).click();
  await mobile.getByRole('heading', { name: 'Browser screen recording' }).waitFor();
  await mobile.screenshot({ path: new URL('mobile-clips.png', dir).pathname, fullPage: true });

  const overflow = await mobile.evaluate(() => document.documentElement.scrollWidth > window.innerWidth + 2);
  if (overflow) throw new Error('Mobile viewport has unintended horizontal overflow');
  if (errors.length) throw new Error(`Browser errors: ${errors.join('; ')}`);
  console.log('Visual QA complete: desktop, overlay, mobile; alert event roundtrip verified');
} finally { if (browser) await browser.close(); server.kill(); }
