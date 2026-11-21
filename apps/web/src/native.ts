import { invoke } from '@tauri-apps/api/core'
import { offline, type Activity, type Alert, type StudioStatus } from './types'

type Body = Record<string, unknown>
type Source = StudioStatus['sources'][number]
type Scene = { name: string; sources: Source[] }
type Display = { id: number; name: string; width: number; height: number }
type Saved = { scenes: Scene[]; currentScene: string; previewScene: string; studioMode: boolean }
const STORAGE_KEY = 'moobee-native-studio.v1'
const defaults = (): Saved => ({
  scenes: ['Starting Soon', 'Gameplay', 'Be Right Back', 'Ending'].map(name => ({ name, sources: [] })),
  currentScene: 'Starting Soon', previewScene: 'Starting Soon', studioMode: false
})
function load(): Saved {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null') as Saved | null
    if (saved && Array.isArray(saved.scenes) && saved.scenes.length && saved.scenes.length <= 40 &&
        saved.scenes.every(s => typeof s.name === 'string' && Array.isArray(s.sources)) &&
        saved.scenes.some(s => s.name === saved.currentScene)) {
      return {
        ...saved,
        previewScene: saved.scenes.some(s => s.name === saved.previewScene) ? saved.previewScene : saved.currentScene,
        studioMode: Boolean(saved.studioMode)
      }
    }
  } catch { /* Corrupt local draft; start with defaults. */ }
  return defaults()
}
let draft = load()
let status: StudioStatus = { ...offline, connected: true, address: 'local', engine: 'moobee-native-alpha' }
let displays: Display[] = []
const history: Activity[] = []
function notify(event: 'status' | 'activity' | 'alert', data: StudioStatus | Activity | Alert) {
  window.dispatchEvent(new CustomEvent(`moobee:${event}`, { detail: data }))
}
function activity(type: string, message: string) {
  const item: Activity = { id: crypto.randomUUID(), type, message, time: Date.now() }
  history.unshift(item)
  if (history.length > 50) history.length = 50
  notify('activity', item)
}
function snapshot(): StudioStatus {
  const active = draft.scenes.find(scene => scene.name === draft.currentScene)
  return {
    ...offline,
    connected: true, address: 'local', engine: 'moobee-native-alpha',
    captureAvailable: displays.length > 0,
    recordAvailable: false, broadcastAvailable: false,
    scenes: draft.scenes.map(scene => scene.name),
    currentScene: draft.currentScene, previewScene: draft.previewScene, studioMode: draft.studioMode,
    sources: active?.sources.map(source => ({ ...source })) ?? [], inputs: []
  }
}
async function refresh(): Promise<StudioStatus> {
  try { displays = await invoke<Display[]>('list_displays') } catch { displays = [] }
  status = snapshot()
  notify('status', status)
  return status
}
function validName(value: unknown): value is string {
  return typeof value === 'string' && value.trim().length > 0 && value.trim().length <= 64 && !/[<>\r\n]/.test(value)
}
function perform(body: Body) {
  const action = body.action
  const sceneName = body.sceneName
  const active = draft.scenes.find(scene => scene.name === draft.currentScene)
  if (action === 'addScene') {
    if (!validName(sceneName)) throw new Error('Scene names must contain 1–64 characters')
    if (draft.scenes.some(s => s.name.toLowerCase() === sceneName.trim().toLowerCase())) throw new Error('Scene already exists')
    if (draft.scenes.length >= 40) throw new Error('Scene limit reached')
    draft.scenes.push({ name: sceneName.trim(), sources: [] })
  } else if (action === 'scene') {
    if (typeof sceneName !== 'string' || !draft.scenes.some(s => s.name === sceneName)) throw new Error('Unknown scene')
    if (draft.studioMode) draft.previewScene = sceneName
    else { draft.currentScene = sceneName; draft.previewScene = sceneName }
  } else if (action === 'studioMode') {
    if (typeof body.studioModeEnabled !== 'boolean') throw new Error('Invalid Studio Mode value')
    draft.studioMode = body.studioModeEnabled
    if (!draft.studioMode) draft.previewScene = draft.currentScene
  } else if (action === 'transition') {
    if (!draft.studioMode) throw new Error('Enable Studio Mode to take a preview')
    draft.currentScene = draft.previewScene
  } else if (action === 'addSource') {
    if (!active || !validName(body.name) || body.kind !== 'display' ||
        typeof body.deviceId !== 'string' || !/^\d{1,4}$/.test(body.deviceId) ||
        !displays.some(d => String(d.id) === body.deviceId)) throw new Error('Select a valid available display')
    if (active.sources.length >= 30) throw new Error('Source limit reached')
    const id = Math.max(0, ...draft.scenes.flatMap(s => s.sources.map(item => item.id))) + 1
    active.sources.push({ id, name: body.name.trim(), kind: 'display', deviceId: body.deviceId, enabled: true })
  } else if (action === 'source') {
    const source = active?.sources.find(item => item.id === body.sceneItemId)
    if (!source || typeof body.enabled !== 'boolean') throw new Error('Invalid source')
    source.enabled = body.enabled
  } else if (action === 'stream' || action === 'record') {
    throw new Error('Native video encoding and output are not implemented in this alpha')
  } else {
    throw new Error('Unsupported MooBee Studio action')
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(draft))
  status = snapshot()
  notify('status', status)
  activity('scene', `Studio: ${String(action)}`)
  return { ok: true, status }
}
export async function nativeRequest(path: string, body: Body = {}): Promise<unknown> {
  if (path === '/health') return { ok: true, name: 'MooBee Studio', version: '0.3.0-alpha', engine: 'in-house' }
  if (path === '/activity') return history
  if (path === '/studio/status') return refresh()
  if (path === '/studio/action') return perform(body)
  if (path === '/capture/displays') {
    await refresh()
    return { displays, available: displays.length > 0 }
  }
  if (path.startsWith('/studio/snapshot')) {
    const name = new URL(path, 'http://localhost').searchParams.get('scene') || draft.currentScene
    const scene = draft.scenes.find(s => s.name === name)
    if (!scene) throw new Error('Scene not found')
    const display = scene.sources.find(source => source.enabled && source.kind === 'display')
    if (!display?.deviceId) return { image: null }
    return { image: await invoke<string>('capture_display', { displayId: Number(display.deviceId) }) }
  }
  if (path === '/alerts/test') {
    const alert = await invoke<Alert>('emit_test_alert', { alert: { type: body.type, name: body.name, message: body.message } })
    notify('alert', alert)
    activity('alert', `Test ${alert.type} alert for ${alert.name}`)
    return { ok: true, alert }
  }
  throw new Error(`Unknown local API: ${path}`)
}
