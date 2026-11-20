import OBSWebSocket from 'obs-websocket-js'
import { invoke } from '@tauri-apps/api/core'
import { disconnected, type Activity, type Alert, type ObsStatus } from './types'

const obs = new OBSWebSocket()
let connected = false
let address = 'ws://127.0.0.1:4455'
let status: ObsStatus = { ...disconnected }
const history: Activity[] = []
type Body = Record<string, unknown>

function notify(event: 'status' | 'activity' | 'alert', data: ObsStatus | Activity | Alert) {
  window.dispatchEvent(new CustomEvent(`moobee:${event}`, { detail: data }))
}
function activity(type: string, message: string) {
  const item: Activity = { id: crypto.randomUUID(), type, message, time: Date.now() }
  history.unshift(item)
  if (history.length > 50) history.length = 50
  notify('activity', item)
}
function reset(message = '') {
  connected = false
  status = { ...disconnected, address, lastError: message }
  notify('status', status)
}
obs.on('ConnectionClosed', () => { reset('OBS disconnected'); activity('warning', 'OBS connection closed') })
obs.on('CurrentProgramSceneChanged', ({ sceneName }) => { activity('scene', `Switched to ${sceneName}`); void refresh() })
obs.on('CurrentPreviewSceneChanged', () => { void refresh() })
obs.on('StudioModeStateChanged', () => { void refresh() })
obs.on('StreamStateChanged', ({ outputActive }) => { activity('broadcast', outputActive ? 'Stream started' : 'Stream stopped'); void refresh() })
obs.on('RecordStateChanged', ({ outputActive }) => { activity('record', outputActive ? 'Recording started' : 'Recording stopped'); void refresh() })
obs.on('SceneItemEnableStateChanged', () => { void refresh() })
obs.on('InputMuteStateChanged', () => { void refresh() })

function validateUrl(raw: string) {
  let url: URL
  try { url = new URL(raw) } catch { throw new Error('Enter a valid OBS WebSocket address') }
  if (!['ws:', 'wss:'].includes(url.protocol) || !url.hostname ||
      url.username || url.password || url.search || url.hash) throw new Error('Enter a WebSocket URL without credentials, query, or fragment')

  const host = url.hostname.toLowerCase()
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(host)
  // OBS ships with unencrypted ws://. Allow it only to loopback or explicitly
  // private IPv4 networks (for mobile devices controlling OBS on a trusted LAN).
  const ipv4 = host.split('.').map(Number)
  const privateIpv4 = ipv4.length === 4 && ipv4.every((part) => Number.isInteger(part) && part >= 0 && part <= 255) && (
    ipv4[0] === 10 || (ipv4[0] === 172 && ipv4[1] >= 16 && ipv4[1] <= 31) ||
    (ipv4[0] === 192 && ipv4[1] === 168)
  )
  if (url.protocol === 'ws:' && !loopback && !privateIpv4) {
    throw new Error('Unencrypted ws:// connections are allowed only on loopback or a trusted private LAN. Use wss:// for remote OBS.')
  }
  return url.href
}

async function refresh(): Promise<ObsStatus> {
  if (!connected) return status
  const [scenesResult, inputsResult, streamResult, recordResult, modeResult] = await Promise.allSettled([
    obs.call('GetSceneList'), obs.call('GetInputList'), obs.call('GetStreamStatus'), obs.call('GetRecordStatus'), obs.call('GetStudioModeEnabled')
  ])
  const sceneData = scenesResult.status === 'fulfilled' ? scenesResult.value as unknown as {
    currentProgramSceneName: string; scenes: { sceneName: string }[]
  } : null
  const inputsData = inputsResult.status === 'fulfilled' ? inputsResult.value as unknown as {
    inputs: { inputName: string; inputKind: string }[]
  } : null
  const streamData = streamResult.status === 'fulfilled' ? streamResult.value as unknown as {
    outputActive: boolean; outputDuration: number; outputBytes: number; outputSkippedFrames: number
  } : null
  const recordData = recordResult.status === 'fulfilled' ? recordResult.value as unknown as {
    outputActive: boolean
  } : null
  const studioMode = modeResult.status === 'fulfilled' && Boolean(modeResult.value.studioModeEnabled)
  const currentScene = sceneData?.currentProgramSceneName || ''
  let previewScene = currentScene
  if (studioMode) {
    try { previewScene = (await obs.call('GetCurrentPreviewScene')).currentPreviewSceneName || currentScene }
    catch { /* Studio mode may have changed mid-refresh. */ }
  }
  let sources: ObsStatus['sources'] = []
  if (currentScene) {
    try {
      const raw = await obs.call('GetSceneItemList', { sceneName: currentScene })
      const items = raw.sceneItems as unknown as {
        sceneItemId: number; sourceName: string; sceneItemEnabled: boolean; inputKind?: string; sourceType?: string
      }[]
      sources = items.map(item => ({ id: item.sceneItemId, name: item.sourceName,
        enabled: item.sceneItemEnabled, kind: item.inputKind || item.sourceType || 'source' }))
    } catch { /* Scene may change while fetching its items. */ }
  }
  const audioCandidates = await Promise.all((inputsData?.inputs || []).slice(0, 35).map(async input => {
    let muted = false, volume = 1, hasAudio = false
    try { muted = Boolean((await obs.call('GetInputMute', { inputName: input.inputName })).inputMuted); hasAudio = true } catch {}
    try { volume = Number((await obs.call('GetInputVolume', { inputName: input.inputName })).inputVolumeMul); hasAudio = true } catch {}
    return hasAudio ? { name: input.inputName, kind: input.inputKind, muted, volume } : null
  }))
  status = {
    connected: true, address, lastError: '', currentScene, previewScene, studioMode,
    scenes: (sceneData?.scenes || []).map(scene => scene.sceneName).reverse(), sources,
    inputs: audioCandidates.filter((input): input is NonNullable<typeof input> => input !== null),
    streaming: !!streamData?.outputActive, recording: !!recordData?.outputActive,
    streamSeconds: Math.round((streamData?.outputDuration || 0) / 1000),
    bitrateKbps: streamData?.outputBytes != null && streamData.outputDuration > 0
      ? Math.round(streamData.outputBytes * 8 / streamData.outputDuration) : null,
    droppedFrames: streamData?.outputSkippedFrames ?? null, fps: null
  }
  notify('status', status)
  return status
}

async function connect(body: Body) {
  const nextAddress = validateUrl(String(body.address || address))
  const password = typeof body.password === 'string' ? body.password : ''
  if (password.length > 256) throw new Error('Password is too long')
  try { await obs.disconnect() } catch {}
  reset()
  address = nextAddress
  try {
    await obs.connect(address, password, { rpcVersion: 1 })
    connected = true
    activity('success', 'Connected to OBS Studio')
    return await refresh()
  } catch (error) {
    reset((error as Error).message)
    activity('error', 'OBS connection failed')
    throw new Error(`Could not connect to OBS: ${(error as Error).message}`)
  }
}

async function action(body: Body) {
  if (!connected) throw new Error('Connect to OBS first')
  if (body.action === 'scene' && typeof body.sceneName === 'string' && status.scenes.includes(body.sceneName)) {
    if (status.studioMode) await obs.call('SetCurrentPreviewScene', { sceneName: body.sceneName })
    else await obs.call('SetCurrentProgramScene', { sceneName: body.sceneName })
  } else if (body.action === 'studioMode' && typeof body.studioModeEnabled === 'boolean') {
    await obs.call('SetStudioModeEnabled', { studioModeEnabled: body.studioModeEnabled })
  } else if (body.action === 'transition' && status.studioMode) {
    await obs.call('TriggerStudioModeTransition')
  } else if (body.action === 'stream') {
    await obs.call(status.streaming ? 'StopStream' : 'StartStream')
  } else if (body.action === 'record') {
    await obs.call(status.recording ? 'StopRecord' : 'StartRecord')
  } else if (body.action === 'mute' && typeof body.inputName === 'string' && typeof body.muted === 'boolean' && status.inputs.some(i => i.name === body.inputName)) {
    await obs.call('SetInputMute', { inputName: body.inputName, inputMuted: body.muted })
  } else if (body.action === 'volume' && typeof body.inputName === 'string' && typeof body.volume === 'number' && Number.isFinite(body.volume) && body.volume >= 0 && body.volume <= 2 && status.inputs.some(i => i.name === body.inputName)) {
    await obs.call('SetInputVolume', { inputName: body.inputName, inputVolumeMul: body.volume })
  } else if (body.action === 'source' && Number.isInteger(body.sceneItemId) && typeof body.enabled === 'boolean' && status.sources.some(i => i.id === body.sceneItemId)) {
    await obs.call('SetSceneItemEnabled', { sceneName: status.currentScene, sceneItemId: Number(body.sceneItemId), sceneItemEnabled: body.enabled })
  } else throw new Error('Unknown or invalid OBS action')
  return { ok: true, status: await refresh() }
}

export async function nativeRequest(path: string, body: Body = {}): Promise<unknown> {
  if (path === '/health') return { ok: true, name: 'MooBee Live', version: '0.2.0' }
  if (path === '/activity') return history
  if (path === '/obs/status') return refresh()
  if (path === '/obs/connect') return connect(body)
  if (path === '/obs/disconnect') { try { await obs.disconnect() } catch {}; reset(); return { ok: true } }
  if (path === '/obs/action') return action(body)
  if (path.startsWith('/obs/snapshot')) {
    if (!connected) throw new Error('Connect to OBS first')
    const scene = new URL(path, 'http://localhost').searchParams.get('scene') || status.currentScene
    if (!status.scenes.includes(scene)) throw new Error('Unknown scene')
    const data = await obs.call('GetSourceScreenshot', { sourceName: scene, imageFormat: 'jpg', imageWidth: 800, imageHeight: 450, imageCompressionQuality: 65 })
    return { image: data.imageData }
  }
  if (path === '/alerts/test') {
    const alert = await invoke<Alert>('emit_test_alert', { alert: { type: body.type, name: body.name, message: body.message } })
    notify('alert', alert)
    activity('alert', `Test ${alert.type} alert for ${alert.name}`)
    return { ok: true, alert }
  }
  throw new Error(`Unsupported desktop API request: ${path}`)
}
