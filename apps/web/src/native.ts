import { validateSceneLayout, type SceneLayout } from './programDisplay'
import { invoke } from '@tauri-apps/api/core'
import { version as appVersion } from '../../../package.json'
import { offline, type Activity, type Alert, type StudioStatus } from './types'

type Body = Record<string, unknown>
type Source = StudioStatus['sources'][number]
type Scene = { name: string; layout?: SceneLayout; sources: Source[] }
type Display = { id: number; name: string; width: number; height: number }
type Saved = { version: 1; scenes: Scene[]; currentScene: string; previewScene: string; studioMode: boolean }
const STORAGE_KEY = 'moobee-native-studio.v1'
const defaults = (): Saved => ({
  version: 1, scenes: ['Starting Soon', 'Gameplay', 'Be Right Back', 'Ending'].map(name => ({ name, sources: [] })),
  currentScene: 'Starting Soon', previewScene: 'Starting Soon', studioMode: false
})
function validateCollection(value: unknown): Saved {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid scene collection')
  const raw = value as Saved
  if (raw.version !== 1 || !Array.isArray(raw.scenes) || raw.scenes.length < 1 || raw.scenes.length > 40) {
    throw new Error('Unsupported scene collection version or scene count')
  }
  const names = new Set<string>(), ids = new Set<number>()
  const scenes = raw.scenes.map((scene) => {
    if (!scene || !validName(scene.name) || !Array.isArray(scene.sources) || scene.sources.length > 30) {
      throw new Error('Invalid scene in collection')
    }
    const name = scene.name.trim()
    if (names.has(name.toLowerCase())) throw new Error('Duplicate scene name')
    names.add(name.toLowerCase())
    const sources = scene.sources.map(source => {
      if (!source || !Number.isSafeInteger(source.id) || source.id < 1 || source.id > 1_000_000_000 ||
          ids.has(source.id) || source.kind !== 'display' || typeof source.deviceId !== 'string' ||
          !/^\d{1,4}$/.test(source.deviceId) || !validName(source.name) || typeof source.enabled !== 'boolean') {
        throw new Error('Invalid display source in collection')
      }
      ids.add(source.id)
      return { id: source.id, name: source.name.trim(), kind: 'display', deviceId: source.deviceId, enabled: source.enabled }
    })
    const layout = validateSceneLayout(scene.layout)
    return { name, sources, ...(scene.layout === undefined ? {} : { layout }) }
  })
  if (!scenes.some(s => s.name === raw.currentScene) || !scenes.some(s => s.name === raw.previewScene) ||
      typeof raw.studioMode !== 'boolean') throw new Error('Invalid current/preview scene selection')
  return { version: 1, scenes, currentScene: raw.currentScene,
    previewScene: raw.studioMode ? raw.previewScene : raw.currentScene, studioMode: raw.studioMode }
}
function load(): Saved {
  try {
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null') as Saved | null
    if (!raw) return defaults()
    // Migrate existing Alpha 0.3 scene drafts, which did not include a schema version.
    return validateCollection(raw.version === undefined ? { ...raw, version: 1 } : raw)
  } catch {
    // Keep the original data for recovery if the saved file is unreadable.
    const backup = localStorage.getItem(STORAGE_KEY)
    if (backup) localStorage.setItem(`${STORAGE_KEY}.backup`, backup)
    return defaults()
  }
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
  const active = draft.scenes.find(scene => scene.name === (draft.studioMode ? draft.previewScene : draft.currentScene))
  return {
    ...offline,
    connected: true, address: 'local', engine: 'moobee-native-alpha',
    captureAvailable: displays.length > 0,
    recordAvailable: false, broadcastAvailable: false,
    scenes: draft.scenes.map(scene => scene.name),
    currentScene: draft.currentScene, previewScene: draft.previewScene, studioMode: draft.studioMode,
    sceneLayout: active?.layout ?? 'grid',
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
  const active = draft.scenes.find(scene => scene.name === (draft.studioMode ? draft.previewScene : draft.currentScene))
  if (action === 'addScene') {
    if (!validName(sceneName)) throw new Error('Scene names must contain 1–64 characters')
    if (draft.scenes.some(s => s.name.toLowerCase() === sceneName.trim().toLowerCase())) throw new Error('Scene already exists')
    if (draft.scenes.length >= 40) throw new Error('Scene limit reached')
    draft.scenes.push({ name: sceneName.trim(), sources: [] })
  } else if (action === 'renameScene') {
    const old = sceneName
    const newName = body.newName
    const scene = draft.scenes.find(s => s.name === old)
    if (!scene || !validName(newName)) throw new Error('Invalid scene rename')
    if (draft.scenes.some(s => s !== scene && s.name.toLowerCase() === newName.trim().toLowerCase())) {
      throw new Error('Scene name already exists')
    }
    scene.name = newName.trim()
    if (draft.currentScene === old) draft.currentScene = scene.name
    if (draft.previewScene === old) draft.previewScene = scene.name
  } else if (action === 'deleteScene') {
    if (draft.scenes.length < 2) throw new Error('Cannot delete the last scene')
    const index = draft.scenes.findIndex(s => s.name === sceneName)
    if (index < 0) throw new Error('Unknown scene')
    draft.scenes.splice(index, 1)
    if (draft.currentScene === sceneName) draft.currentScene = draft.scenes[0].name
    if (draft.previewScene === sceneName) draft.previewScene = draft.currentScene
  } else if (action === 'duplicateScene') {
    const scene = draft.scenes.find(s => s.name === sceneName)
    if (!scene) throw new Error('Unknown scene')
    if (draft.scenes.length >= 40) throw new Error('Scene limit reached')
    const base = scene.name.slice(0, 48)
    let name = `${base} Copy`
    let n = 2
    while (draft.scenes.some(s => s.name.toLowerCase() === name.toLowerCase())) name = `${base} Copy ${n++}`
    let nextId = Math.max(0, ...draft.scenes.flatMap(s => s.sources.map(item => item.id))) + 1
    draft.scenes.push({ name, ...(scene.layout ? { layout: scene.layout } : {}), sources: scene.sources.map(src => ({ ...src, id: nextId++ })) })
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
  } else if (action === 'sceneLayout') {
    if (!active) throw new Error('No active scene')
    if (body.layout === undefined) throw new Error('Choose a scene layout')
    active.layout = validateSceneLayout(body.layout)
  } else if (action === 'moveSource') {
    const index = active?.sources.findIndex(src => src.id === body.sceneItemId) ?? -1
    if (!active || index < 0 || (body.direction !== 'up' && body.direction !== 'down')) throw new Error('Invalid source order')
    const next = index + (body.direction === 'up' ? -1 : 1)
    if (next < 0 || next >= active.sources.length) throw new Error('Source is already at the edge')
    ;[active.sources[index], active.sources[next]] = [active.sources[next], active.sources[index]]
  } else if (action === 'source') {
    const source = active?.sources.find(item => item.id === body.sceneItemId)
    if (!source || typeof body.enabled !== 'boolean') throw new Error('Invalid source')
    source.enabled = body.enabled
  } else if (action === 'removeSource') {
    const index = active?.sources.findIndex(s => s.id === body.sceneItemId) ?? -1
    if (!active || index < 0) throw new Error('Unknown source')
    active.sources.splice(index, 1)
  } else if (action === 'stream' || action === 'record') {
    throw new Error('Native video encoding and output are not implemented in this alpha')
  } else {
    throw new Error('Unsupported MooBee Studio action')
  }
  return commit(`Studio: ${String(action)}`)
}
function commit(message: string) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(draft))
  status = snapshot()
  notify('status', status)
  activity('scene', message)
  return { ok: true, status }
}
function replaceCollection(value: unknown) {
  const next = validateCollection(value)
  draft = next
  return commit('Scene collection imported')
}
function collection() {
  return JSON.parse(JSON.stringify(draft)) as Saved
}
export async function nativeRequest(path: string, body: Body = {}): Promise<unknown> {
  if (path === '/health') return { ok: true, name: 'MooBee Studio', version: appVersion, engine: 'in-house' }
  if (path === '/services/diagnostics') return await invoke('native_service_diagnostics')
  if (path === '/macos/open-screen-recording-settings') return await invoke('open_screen_recording_settings')
  if (path === '/activity') return history
  if (path === '/studio/status') return refresh()
  if (path === '/studio/collection') return Object.keys(body).length ? replaceCollection(body) : collection()
  if (path === '/studio/action') return perform(body)
  if (path === '/clips') {
    const clips = await invoke<{ id: string; path: string; bytes: number }[]>('list_test_clips')
    return { available: true, clips }
  }
  if (path === '/clips/macos-mp4-availability') {
    return await invoke('macos_mp4_availability')
  }
  if (path === '/clips/reveal') {
    if (typeof body.clipId !== 'string' ||
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(body.clipId)) {
      throw new Error('Choose a saved MooBee clip to reveal in Finder')
    }
    return await invoke('reveal_test_clip', { clipId: body.clipId })
  }
  if (path === '/clips/stop-mp4') {
    return { stopRequested: await invoke<boolean>('stop_macos_mp4') }
  }
  if (path === '/clips/capture-mp4') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 3 || body.seconds > 120 ||
        typeof body.systemAudio !== 'boolean' || typeof body.microphone !== 'boolean') {
      throw new Error('Choose a valid 3–120-second native MP4 capture and audio options')
    }
    return await invoke('capture_macos_mp4', {
      displayId: body.displayId, seconds: body.seconds,
      systemAudio: body.systemAudio, microphone: body.microphone
    })
  }
  if (path === '/clips/capture-program-continuous-avi' || path === '/clips/capture-program-continuous-mp4') {
    const ids = body.displayIds
    if (!Array.isArray(ids) || ids.length < 1 || ids.length > 4 ||
        !ids.every(id => typeof id === 'number' && Number.isSafeInteger(id) && id >= 0 && id <= 24) ||
        new Set(ids).size !== ids.length ||
        typeof body.seconds !== 'number' || !Number.isSafeInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 8 ||
        (body.profile !== 'baseline' && body.profile !== '720p15')) {
      throw new Error('Choose 1–4 unique Program displays, a 2–8 second duration and a supported capture profile')
    }
    const mp4 = path.endsWith('-mp4')
    if (body.webcam !== undefined && typeof body.webcam !== 'boolean') throw new Error('Invalid webcam selection')
    if (body.webcam && (!mp4 || ids.length > 3)) throw new Error('Webcam PiP requires macOS MP4 and one to three displays')
    return await invoke(mp4 ? 'capture_continuous_program_mp4' : 'capture_continuous_program_avi', {
      displayIds: ids, seconds: body.seconds, profile: body.profile, layout: validateSceneLayout(body.layout), ...(mp4 ? { webcam: body.webcam === true } : {})
    })
  }
  if (path === '/clips/capture-program-test-avi') {
    const ids = body.displayIds
    if (!Array.isArray(ids) || ids.length < 1 || ids.length > 4 ||
        !ids.every(id => typeof id === 'number' && Number.isSafeInteger(id) && id >= 0 && id <= 24) ||
        new Set(ids).size !== ids.length ||
        typeof body.seconds !== 'number' || !Number.isSafeInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 8) {
      throw new Error('Choose one to four unique Program displays and a 2–8 second test duration')
    }
    return await invoke('capture_program_test_avi', { displayIds: ids, seconds: body.seconds, layout: validateSceneLayout(body.layout) })
  }
  if (path === '/clips/capture-avi') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 8) throw new Error('Native AVI test duration must be 2–8 seconds')
    return await invoke('capture_silent_avi', { displayId: body.displayId, seconds: body.seconds })
  }
  if (path === '/clips/capture') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 10) throw new Error('Invalid test clip request')
    return await invoke('capture_test_clip', { displayId: body.displayId, seconds: body.seconds })
  }
  if (path.startsWith('/clips/preview')) {
    const clipId = new URL(path, 'http://localhost').searchParams.get('id')
    if (!clipId) throw new Error('Choose a saved test clip')
    const dataUrl = await invoke<string>('read_test_clip', { clipId })
    return { dataUrl }
  }
  if (path === '/capture/probe') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 5) {
      throw new Error('Select a valid display and a 2–5 second capture diagnostic')
    }
    return await invoke('probe_native_capture', { displayId: body.displayId, seconds: body.seconds })
  }
  if (path === '/capture/displays') {
    await refresh()
    return { displays, available: displays.length > 0 }
  }
  if (path.startsWith('/studio/snapshot')) {
    const name = new URL(path, 'http://localhost').searchParams.get('scene') || draft.currentScene
    const scene = draft.scenes.find(s => s.name === name)
    if (!scene) throw new Error('Scene not found')
    const enabledDisplays = scene.sources.filter(source => source.enabled && source.kind === 'display')
    if (!enabledDisplays.length) return { image: null }
    if (enabledDisplays.length > 4) {
      throw new Error('Still preview supports up to four display sources; full scene composition is not yet implemented')
    }
    const ids = enabledDisplays.map(source => Number(source.deviceId))
    if (ids.some(id => !Number.isSafeInteger(id) || id < 0 || id > 24)) {
      throw new Error('Scene includes an invalid display identity')
    }
    return { image: await invoke<string>('capture_scene_snapshot', { displayIds: ids, layout: validateSceneLayout(scene.layout) }) }
  }
  if (path === '/alerts/test') {
    const alert = await invoke<Alert>('emit_test_alert', { alert: { type: body.type, name: body.name, message: body.message } })
    notify('alert', alert)
    activity('alert', `Test ${alert.type} alert for ${alert.name}`)
    return { ok: true, alert }
  }
  throw new Error(`Unknown local API: ${path}`)
}
