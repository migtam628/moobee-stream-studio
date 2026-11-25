import { validateSceneLayout, validVideoInputId, validateTransform, sceneTransforms, type SourceTransform, type SceneLayout } from './programDisplay'
import { invoke } from '@tauri-apps/api/core'
import { version as appVersion } from '../../../package.json'
import { offline, type Activity, type Alert, type StudioStatus } from './types'

type Body = Record<string, unknown>
type Source = StudioStatus['sources'][number]
type Scene = { name: string; layout?: SceneLayout; sources: Source[] }
type VideoInput = { id: string; name: string; isDefault: boolean }
let videoInputs: VideoInput[] = []
let testedVideo: { id: string; image: string } | null = null
const macHost = () => /Macintosh|Mac OS X/i.test(navigator.userAgent)
type MovingFrame = { image: string | null; frameKey: string | null; liveVideo: boolean; waiting: boolean }
const pendingPreview = new Set<Promise<MovingFrame>>()
let previewUsed = false
let captureActive = false
let meterUsed = false
let pendingMeter: Promise<unknown> | null = null
let meterStopping: Promise<void> | null = null
async function releaseAudioMeter(): Promise<void> {
  if (meterStopping) return meterStopping
  if (!meterUsed) return
  meterStopping = (async () => { if (pendingMeter) await pendingMeter.catch(() => {}); await invoke('stop_program_audio_levels'); meterUsed = false })()
  try { await meterStopping } finally { meterStopping = null }
}
let releasing: Promise<void> | null = null
async function releaseVideoPreviews(): Promise<void> {
  if (releasing) return releasing
  if (!previewUsed && pendingPreview.size === 0) return
  releasing = (async () => {
    await Promise.allSettled([...pendingPreview])
    await invoke('stop_video_previews')
    previewUsed = false
  })()
  try { await releasing } finally { releasing = null }
}
async function captureWithPreviewReleased<T>(command: string, args: Record<string,unknown>): Promise<T> {
  if (captureActive) throw new Error('Another native capture is active')
  captureActive = true
  try { await releaseAudioMeter(); await releaseVideoPreviews(); return await invoke<T>(command,args) }
  finally { captureActive = false }
}
async function movingPreview(ids: string[], layout: SceneLayout, previousKey: string | null, transforms?:SourceTransform[]): Promise<MovingFrame> {
  if (captureActive || releasing) return {image:null,frameKey:null,liveVideo:true,waiting:true}
  previewUsed = true
  const task = invoke<MovingFrame>('live_video_preview_frame',{deviceIds:ids,layout,previousKey,transforms:transforms??null})
  pendingPreview.add(task)
  try { return await task } finally { pendingPreview.delete(task) }
}
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
          ids.has(source.id) || typeof source.deviceId !== 'string' ||
          !((source.kind === 'display' && /^\d{1,4}$/.test(source.deviceId)) || (source.kind === 'video' && validVideoInputId(source.deviceId))) || !validName(source.name) || typeof source.enabled !== 'boolean') {
        throw new Error('Invalid capture source in collection')
      }
      ids.add(source.id)
      return { id: source.id, name: source.name.trim(), kind: source.kind, deviceId: source.deviceId, enabled: source.enabled, ...(source.transform===undefined?{}:{transform:validateTransform(source.transform)}) }
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
    captureAvailable: displays.length > 0 || videoInputs.length > 0,
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
    if (!active || !validName(body.name) || typeof body.deviceId !== 'string' ||
        !((body.kind === 'display' && displays.some(d => String(d.id) === body.deviceId)) ||
          (body.kind === 'video' && macHost() && validVideoInputId(body.deviceId) && videoInputs.some(d => d.id === body.deviceId)))) throw new Error('Select a valid available capture source')
    if (body.kind === 'video' && active.sources.some(source => source.kind === 'video' && source.deviceId === body.deviceId)) throw new Error('This video input is already in the scene')
    if (active.sources.length >= 30) throw new Error('Source limit reached')
    const id = Math.max(0, ...draft.scenes.flatMap(s => s.sources.map(item => item.id))) + 1
    active.sources.push({ id, name: body.name.trim(), kind: String(body.kind), deviceId: body.deviceId, enabled: true })
  } else if (action === 'sceneLayout') {
    if (!active) throw new Error('No active scene')
    if (body.layout === undefined) throw new Error('Choose a scene layout')
    active.layout = validateSceneLayout(body.layout)
  } else if (action === 'sourceTransform') {
    if (!active || body.sceneName !== active.name) throw new Error('The edited scene is no longer selected')
    const source=active.sources.find(item=>item.id===body.sceneItemId)
    if (!source) throw new Error('Unknown source')
    if(body.transform===null) delete source.transform
    else source.transform=validateTransform(body.transform)
  } else if (action === 'resetTransforms') {
    if (!active || body.sceneName !== active.name) throw new Error('The edited scene is no longer selected')
    for(const source of active.sources) delete source.transform
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
    return await captureWithPreviewReleased('capture_macos_mp4', {
      displayId: body.displayId, seconds: body.seconds,
      systemAudio: body.systemAudio, microphone: body.microphone
    })
  }
  if (path === '/clips/program-mp4-stats' || path === '/clips/pause-program-mp4') {
    if (typeof body.sessionId !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(body.sessionId)) throw new Error('Invalid Program session')
    if (path === '/clips/program-mp4-stats') return await invoke('continuous_program_stats', { sessionId: body.sessionId })
    if (typeof body.paused !== 'boolean') throw new Error('Invalid pause state')
    return { accepted: await invoke<boolean>('pause_continuous_program_mp4', { sessionId: body.sessionId, paused: body.paused }) }
  }
  if (path === '/clips/stop-program-mp4') {
    if (typeof body.sessionId !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(body.sessionId)) throw new Error('Invalid Program session')
    return { stopRequested: await invoke<boolean>('stop_continuous_program_mp4', { sessionId: body.sessionId }) }
  }
  if (path === '/clips/capture-program-continuous-avi' || path === '/clips/capture-program-continuous-mp4') {
    const ids = body.displayIds
    const videoDeviceId = body.videoDeviceId
    const mp4 = path.endsWith('-mp4')
    const hasVideo = videoDeviceId !== undefined && videoDeviceId !== null
    if (hasVideo && (!mp4 || !macHost() || !validVideoInputId(videoDeviceId) || body.webcam)) throw new Error('Select one Mac video input with webcam inset off')
    if (!Array.isArray(ids) || (hasVideo ? ids.length !== 0 : ids.length < 1) || ids.length > 4 ||
        !ids.every(id => typeof id === 'number' && Number.isSafeInteger(id) && id >= 0 && id <= 24) ||
        new Set(ids).size !== ids.length ||
        typeof body.seconds !== 'number' || !Number.isSafeInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > (mp4 ? 30 : 8) ||
        (body.profile !== 'baseline' && body.profile !== '720p15' && body.profile !== '1080p30')) {
      throw new Error('Choose 1–4 unique Program displays, a supported duration (MP4 up to 30 seconds, AVI up to 8) and capture profile')
    }
    if (body.profile === '1080p30' && !mp4) throw new Error('1080p30 requires Mac MP4')
    if (body.sessionId !== undefined && (!mp4 || typeof body.sessionId !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(body.sessionId))) throw new Error('Invalid Program session')
    if (body.webcam !== undefined && typeof body.webcam !== 'boolean') throw new Error('Invalid webcam selection')
    if (body.webcam && (!mp4 || ids.length > 3)) throw new Error('Webcam PiP requires macOS MP4 and one to three displays')
    if (body.microphone !== undefined && typeof body.microphone !== 'boolean') throw new Error('Invalid microphone selection')
    if (body.microphone && !mp4) throw new Error('Microphone AAC requires macOS MP4')
    const micGain = body.micGain ?? 1
    if (typeof micGain !== 'number' || !Number.isFinite(micGain) || micGain < 0 || micGain > 2) throw new Error('Invalid microphone gain')
    if (body.audioInputs !== undefined && (!mp4 || !Array.isArray(body.audioInputs) || body.audioInputs.length > 3)) throw new Error('Named audio sources require Mac MP4')
    if (body.overlay !== undefined && body.overlay !== null) {
      const o = body.overlay as { width: number; height: number; rgbaBase64: string; placement?: { xPercent: number; yPercent: number; widthPercent: number } }
      if (typeof o !== 'object' || !Number.isInteger(o.width) || !Number.isInteger(o.height) ||
          o.width < 1 || o.width > 480 || o.height < 1 || o.height > 270 ||
          typeof o.rgbaBase64 !== 'string' || o.rgbaBase64.length !== Math.ceil(o.width * o.height * 4 / 3) * 4) {
        throw new Error('Invalid bounded graphic overlay')
      }
      if (o.placement !== undefined) {
        const p = o.placement
        if (!p || typeof p !== 'object' || !Number.isInteger(p.xPercent) || p.xPercent < 0 || p.xPercent > 100 || !Number.isInteger(p.yPercent) || p.yPercent < 0 || p.yPercent > 100 || !Number.isInteger(p.widthPercent) || p.widthPercent < 10 || p.widthPercent > 50) throw new Error('Invalid graphic placement')
      }
    }
    const transforms = body.transforms === undefined || body.transforms === null ? undefined :
      Array.isArray(body.transforms) && body.transforms.length === (hasVideo?1:ids.length) ? body.transforms.map(validateTransform) : (()=>{throw new Error('Invalid source transform count')})()
    return await captureWithPreviewReleased(mp4 ? 'capture_continuous_program_mp4' : 'capture_continuous_program_avi', {
      displayIds: ids, seconds: body.seconds, profile: body.profile, layout: validateSceneLayout(body.layout), transforms:transforms??null, overlay: body.overlay ?? null, ...(mp4 ? { sessionId: body.sessionId ?? null, webcam: body.webcam === true, microphone: body.microphone === true, micGain, ...(body.audioInputs !== undefined ? { audioInputs: body.audioInputs } : {}), ...(hasVideo ? { videoDeviceId } : {}) } : {})
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
    const transforms=body.transforms===undefined||body.transforms===null?null:Array.isArray(body.transforms)&&body.transforms.length===ids.length?body.transforms.map(validateTransform):(()=>{throw new Error('Invalid source transform count')})()
    return await captureWithPreviewReleased('capture_program_test_avi', { displayIds: ids, seconds: body.seconds, layout: validateSceneLayout(body.layout),transforms })
  }
  if (path === '/clips/capture-avi') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 8) throw new Error('Native AVI test duration must be 2–8 seconds')
    return await captureWithPreviewReleased('capture_silent_avi', { displayId: body.displayId, seconds: body.seconds })
  }
  if (path === '/clips/capture') {
    if (typeof body.displayId !== 'number' || !Number.isSafeInteger(body.displayId) ||
        typeof body.seconds !== 'number' || !Number.isInteger(body.seconds) ||
        body.seconds < 2 || body.seconds > 10) throw new Error('Invalid test clip request')
    return await captureWithPreviewReleased('capture_test_clip', { displayId: body.displayId, seconds: body.seconds })
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
    return await captureWithPreviewReleased('probe_native_capture', { displayId: body.displayId, seconds: body.seconds })
  }
  if (path === '/capture/audio-inputs') {
    if (!macHost()) return { devices: [], supported: false }
    return { devices: await invoke('list_audio_inputs'), supported: true }
  }
  if (path === '/capture/audio-levels') {
    if (!macHost() || captureActive || meterStopping) return { levels: null }
    if (pendingMeter) return { levels: null }
    meterUsed = true
    const task = invoke('program_audio_levels',{inputs:body.inputs})
    pendingMeter = task
    try { return { levels: await task } } finally { pendingMeter = null }
  }
  if (path === '/capture/stop-audio-levels') { await releaseAudioMeter(); return { stopped: true } }
  if (path === '/capture/stop-video-previews') { await releaseVideoPreviews(); return { stopped: true } }
  if (path === '/capture/video-inputs') {
    if (!macHost()) return { devices: [], supported: false }
    videoInputs = await invoke<VideoInput[]>('list_video_inputs')
    if (testedVideo && !videoInputs.some(d => d.id === testedVideo?.id)) testedVideo = null
    status = snapshot()
    notify('status', status)
    return { devices: videoInputs, supported: true }
  }
  if (path === '/capture/test-video-input') {
    if (!macHost() || !validVideoInputId(body.deviceId)) throw new Error('Select an available Mac video input')
    testedVideo = null
    const image = await captureWithPreviewReleased<string>('test_video_input', { deviceId: body.deviceId })
    testedVideo = { id: body.deviceId, image }
    return { image }
  }
  if (path === '/capture/displays') {
    await refresh()
    return { displays, available: displays.length > 0 }
  }
  if (path.startsWith('/studio/snapshot')) {
    const params = new URL(path, 'http://localhost').searchParams
    const name = params.get('scene') || draft.currentScene
    const scene = draft.scenes.find(s => s.name === name)
    if (!scene) throw new Error('Scene not found')
    const enabled = scene.sources.filter(source => source.enabled)
    const transforms=sceneTransforms(scene.sources,validateSceneLayout(scene.layout))
    const video = enabled.find(source => source.kind === 'video')
    if (video) {
      if (params.get('mode') === 'live') {
        const ids = enabled.map(source => source.deviceId)
        if (!macHost() || enabled.length > 2 || enabled.some(source => source.kind !== 'video') || !ids.every(validVideoInputId) || new Set(ids).size !== ids.length) {
          throw new Error('Live video preview supports one or two distinct Mac video inputs; hide other sources.')
        }
        const key = params.get('key')
        return await movingPreview(ids,validateSceneLayout(scene.layout),key && key.length <= 2048 ? key : null,transforms)
      }
      if (transforms) throw new Error('Choose Live video inputs to preview manual video source transforms')
      if (enabled.length !== 1) throw new Error('Enable only the selected capture card or camera for this video input test')
      if (testedVideo?.id !== video.deviceId) throw new Error('Video input selected. Use Test video signal in Sources to acquire a still frame.')
      return { image: testedVideo?.image ?? null }
    }
    const enabledDisplays = scene.sources.filter(source => source.enabled && source.kind === 'display')
    if (!enabledDisplays.length) return { image: null }
    if (enabledDisplays.length > 4) {
      throw new Error('Still preview supports up to four display sources; full scene composition is not yet implemented')
    }
    const ids = enabledDisplays.map(source => Number(source.deviceId))
    if (ids.some(id => !Number.isSafeInteger(id) || id < 0 || id > 24)) {
      throw new Error('Scene includes an invalid display identity')
    }
    return { image: await invoke<string>('capture_scene_snapshot', { displayIds: ids, layout: validateSceneLayout(scene.layout),transforms:transforms??null }) }
  }
  if (path === '/alerts/test') {
    const alert = await invoke<Alert>('emit_test_alert', { alert: { type: body.type, name: body.name, message: body.message } })
    notify('alert', alert)
    activity('alert', `Test ${alert.type} alert for ${alert.name}`)
    return { ok: true, alert }
  }
  throw new Error(`Unknown local API: ${path}`)
}
