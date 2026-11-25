import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import {
  Activity, AudioLines, Bell, Bot, Check, CheckCircle2, CircleHelp, Clapperboard,
  Copy, Eye, EyeOff, Film, Gift, Layers, LayoutGrid, Link2, ListVideo, Monitor,
  MoreHorizontal, Play, Radio, RadioTower, RefreshCcw, Settings2, Shield, SlidersHorizontal,
  Sparkles, Square, Terminal, Video, Volume2, VolumeX, WandSparkles, X
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { isStaticPreview, request, runningInTauri, subscribeEvents } from './api'
import { version as appVersion } from '../../../package.json'
import { buildIdentity } from './buildIdentity'
import BrowserRecorder from './BrowserRecorder'
import QuickCapture from './QuickCapture'
import ServiceDiagnostics from './ServiceDiagnostics'
import GithubUpdates from './GithubUpdates'
import { offline, type Activity as ActivityItem, type Alert, type StudioStatus } from './types'

type Tab = 'Studio' | 'Scenes' | 'Overlays' | 'Alerts' | 'Multistream' | 'AI Sidekick' | 'Clips' | 'Chatbot' | 'Analytics' | 'Settings'

const navigation: { label: Tab; icon: LucideIcon }[] = [
  { label: 'Studio', icon: Play }, { label: 'Scenes', icon: Layers }, { label: 'Overlays', icon: Monitor },
  { label: 'Alerts', icon: Bell }, { label: 'Multistream', icon: RadioTower }, { label: 'AI Sidekick', icon: Sparkles },
  { label: 'Clips', icon: Film }, { label: 'Chatbot', icon: Bot }, { label: 'Analytics', icon: Activity },
  { label: 'Settings', icon: Settings2 }
]

function useStudio() {
  const [status, setStatus] = useState<StudioStatus>(offline)
  const [activity, setActivity] = useState<ActivityItem[]>([])
  const [alert, setAlert] = useState<Alert | null>(null)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const refresh = useCallback(async () => {
    try { setStatus(await request<StudioStatus>('/studio/status')) }
    catch { setStatus(offline) }
  }, [])
  useEffect(() => {
    void refresh()
    void request<ActivityItem[]>('/activity').then(setActivity).catch(() => {})
    const polling = setInterval(() => void refresh(), 7000)
    const unsubscribe = subscribeEvents({
      status: setStatus,
      activity: (item) => setActivity((previous) => [item, ...previous].slice(0, 30)),
      alert: setAlert
    })
    return () => { clearInterval(polling); unsubscribe() }
  }, [refresh])
  const perform = useCallback(async (path: string, body: object) => {
    setPending(true)
    setError('')
    try {
      const result = await request<{ status?: StudioStatus }> (path, body)
      if (result.status) setStatus(result.status)
      await refresh()
      return true
    } catch (err) { setError((err as Error).message); return false }
    finally { setPending(false) }
  }, [refresh])
  return { status, activity, alert, pending, error, setError, perform, refresh }
}

function Panel({ title, icon: Icon, children, action, className = '' }: {
  title: string; icon: LucideIcon; children: ReactNode; action?: ReactNode; className?: string
}) {
  return <section className={`panel ${className}`}><div className="panel-head"><div className="panel-title"><Icon size={17}/>{title}</div>{action}</div>{children}</section>
}

function formatElapsed(seconds: number) {
  const h = Math.floor(seconds / 3600).toString().padStart(2, '0')
  const m = Math.floor((seconds % 3600) / 60).toString().padStart(2, '0')
  const s = Math.floor(seconds % 60).toString().padStart(2, '0')
  return `${h}:${m}:${s}`
}

type PreviewRefresh = 'live' | 'fast' | 'normal' | 'paused'

function ScenePreview({ status, label, previewScene, active, refreshRate, revision }: { status: StudioStatus; label: string; previewScene: string; active: boolean; refreshRate: PreviewRefresh; revision: string }) {
  const [image, setImage] = useState('')
  const [previewError, setPreviewError] = useState('')
  const [lastFrame, setLastFrame] = useState<number | null>(null)
  const [moving, setMoving] = useState(false)
  const inFlight = useRef(false)
  useEffect(() => { setImage(''); setPreviewError(''); setLastFrame(null); setMoving(false) }, [previewScene, revision, refreshRate])
  useEffect(() => {
    if (!active || !status.connected || !previewScene || refreshRate === 'paused') return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let key: string | null = null
    let delay = refreshRate === 'normal' ? 5000 : 1000
    async function load() {
      if (cancelled) return
      if (document.hidden || inFlight.current) { timer = setTimeout(load, 250); return }
      inFlight.current = true
      try {
        const params = new URLSearchParams({scene:previewScene,mode:refreshRate})
        if (key) params.set('key',key)
        const response = await request<{ image: string | null; liveVideo?: boolean; frameKey?: string | null; waiting?: boolean }>(`/studio/snapshot?${params}`)
        if (!cancelled) {
          setMoving(response.liveVideo === true)
          setPreviewError('')
          delay = response.liveVideo ? 100 : refreshRate === 'normal' ? 5000 : 1000
          if (response.waiting) { setImage(''); setLastFrame(null); key = null }
          else if (response.image) { setImage(response.image); setLastFrame(Date.now()); key = response.frameKey ?? null }
          else if (!response.liveVideo || !response.frameKey) { setImage(''); setLastFrame(null); key = null }
        }
      } catch (error) {
        if (!cancelled) { setImage(''); setLastFrame(null); setMoving(false); key = null; delay = 1000; setPreviewError((error as Error).message || 'Could not render native scene preview') }
      } finally {
        inFlight.current = false
        if (!cancelled) timer = setTimeout(load, delay)
      }
    }
    void load()
    return () => { cancelled = true; if (timer !== undefined) clearTimeout(timer) }
  }, [previewScene, status.connected, active, refreshRate, revision])
  return <div className="preview-block">
    <div className="preview-heading"><span><Monitor size={15}/>{label}</span><span>{!status.connected ? 'Local studio offline' : refreshRate === 'paused' ? 'Preview paused' : moving ? 'Native video · up to 10 fps' : refreshRate === 'live' ? 'Display stills / waiting for video' : refreshRate === 'fast' ? 'Still frames · 1s interval' : 'Still frames · 5s interval'}</span></div>
    <div className="preview-canvas">
      {image ? <img src={image} alt={`MooBee scene: ${previewScene}`} /> : <div className="preview-empty"><Radio size={34}/><strong>{refreshRate === 'paused' ? 'Preview paused' : moving ? 'Waiting for native video' : 'No preview available'}</strong><span>{status.connected ? 'Add or enable a compatible source in this scene' : 'Add a native display source to preview this scene'}</span></div>}
      {status.streaming && <span className="live-flag">LIVE</span>}
      {lastFrame !== null && <span className="snapshot-timestamp" title="Time the last preview frame was received">{new Date(lastFrame).toLocaleTimeString()}</span>}
    </div>
    {previewError && <small className="preview-error" role="status">{previewError}</small>}
  </div>
}

function ScenesPanel({ status, perform, pending }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean }) {
  const selected = status.studioMode ? status.previewScene : status.currentScene
  return <Panel title="Scenes" icon={Layers} action={<button disabled={pending || isStaticPreview} className="secondary-button" onClick={() => {
    const name = window.prompt('New scene name')
    if (name?.trim()) void perform('/studio/action', { action: 'addScene', sceneName: name.trim() })
  }}>+ Add Scene</button>}>
    <div className="rows-scroll">{status.scenes.length ? status.scenes.map(scene => <div className="scene-item" key={scene}>
      <button disabled={pending || isStaticPreview} className={`scene-row ${scene === selected ? 'selected' : ''}`} onClick={() => void perform('/studio/action', { action: 'scene', sceneName: scene })}>
        <span className="scene-thumbnail"><Monitor size={18}/></span><span className="truncate">{scene}</span>
        {scene === status.currentScene && <span className="scene-on-air">PROGRAM</span>}
        {scene === status.previewScene && status.studioMode && <Check size={15}/>}
      </button>
      <div className="scene-actions">
        <button className="icon-button" title={`Duplicate ${scene}`} aria-label={`Duplicate ${scene}`} disabled={pending || isStaticPreview || status.scenes.length >= 40} onClick={() => void perform('/studio/action', { action: 'duplicateScene', sceneName: scene })}><Copy size={15}/></button>
        <button className="icon-button" title={`Rename ${scene}`} aria-label={`Rename ${scene}`} disabled={pending || isStaticPreview} onClick={() => {
          const name = window.prompt('Rename scene', scene)
          if (name?.trim() && name.trim() !== scene) void perform('/studio/action', { action: 'renameScene', sceneName: scene, newName: name.trim() })
        }}><Settings2 size={15}/></button>
        <button className="icon-button" title={`Delete ${scene}`} aria-label={`Delete ${scene}`} disabled={pending || isStaticPreview || status.scenes.length < 2} onClick={() => {
          if (window.confirm(`Delete scene "${scene}" and its sources?`)) void perform('/studio/action', { action: 'deleteScene', sceneName: scene })
        }}><X size={15}/></button>
      </div>
    </div>) : <div className="empty-list">Create a scene to get started.</div>}</div>
  </Panel>
}

function SourcesPanel({ status, perform, pending }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean }) {
  const [displays, setDisplays] = useState<{ id: number; name: string; width: number; height: number }[]>([])
  const [devices, setDevices] = useState<{ id: string; name: string; isDefault: boolean }[]>([])
  const [selected, setSelected] = useState('')
  const [scanning, setScanning] = useState(false)
  const [testing, setTesting] = useState(false)
  const [signal, setSignal] = useState('')
  const [deviceError, setDeviceError] = useState('')
  const mac = runningInTauri() && /Macintosh|Mac OS X/i.test(navigator.userAgent)
  const refreshSources = useCallback(async () => {
    if (!runningInTauri()) return
    setScanning(true); setDeviceError(''); setSignal('')
    try {
      const result = await request<{ displays: typeof displays }>('/capture/displays')
      setDisplays(result.displays)
      let next: typeof devices = []
      if (mac) {
        try { next = (await request<{ devices: typeof devices }>('/capture/video-inputs')).devices }
        catch (cause) { setDeviceError('Video discovery: ' + String(cause)) }
      }
      setDevices(next)
      const keys = [...result.displays.map(d => 'display:' + d.id), ...next.map(d => 'video:' + d.id)]
      setSelected(value => keys.includes(value) ? value : keys[0] ?? '')
    } catch (cause) { setDeviceError(String(cause)) }
    finally { setScanning(false) }
  }, [mac])
  useEffect(() => { void refreshSources() }, [status.connected, refreshSources])
  const selectedVideo = devices.find(item => 'video:' + item.id === selected)
  const selectedDisplay = displays.find(item => 'display:' + item.id === selected)
  const addSource = async () => {
    const device = selectedVideo ?? selectedDisplay
    if (!device) return
    await perform('/studio/action', { action: 'addSource', kind: selectedVideo ? 'video' : 'display', deviceId: String(device.id), name: device.name.slice(0,64) })
  }
  const testSignal = async () => {
    if (!selectedVideo || testing) return
    setTesting(true); setSignal(''); setDeviceError('')
    try {
      await request<{ image: string }>('/capture/test-video-input', { deviceId: selectedVideo.id })
      setSignal('Video frame received · verify actual HDMI content. Select Live video inputs for moving preview.')
    } catch (cause) { setDeviceError('Video signal test failed: ' + String(cause)) }
    finally { setTesting(false) }
  }
  return <Panel title="Sources" icon={ListVideo} action={<div className="source-picker">
    <button className="icon-button" aria-label="Refresh capture devices" title="Refresh monitors, capture cards and cameras" disabled={scanning || testing || !runningInTauri()} onClick={() => void refreshSources()}><RefreshCcw size={14}/></button>
  </div>}>
    <div className="clip-form">
      <label>Capture source
        <select aria-label="Choose capture source" value={selected} disabled={pending || scanning || testing || !runningInTauri()} onChange={event => { setSelected(event.target.value); setSignal(''); setDeviceError('') }}>
          {!displays.length && !devices.length && <option value="">No capture devices</option>}
          {displays.length > 0 && <optgroup label="Desktop displays">{displays.map(d => <option key={d.id} value={'display:' + d.id}>{d.name} ({d.width}×{d.height})</option>)}</optgroup>}
          {devices.length > 0 && <optgroup label="Video capture devices · HDMI / cameras">{devices.map(d => <option key={d.id} value={'video:' + d.id}>{d.name}{d.isDefault ? ' (default)' : ''}</option>)}</optgroup>}
        </select>
      </label>
      <div className="dock-action-row">
        <button className="secondary-button" disabled={pending || scanning || testing || !selected} onClick={() => void addSource()}>+ Add source</button>
        {mac && <button className="secondary-button" disabled={!selectedVideo || testing || scanning} onClick={() => void testSignal()}>{testing ? 'Testing…' : 'Test video signal'}</button>}
      </div>
      {mac && <small className="dock-note">{devices.length} video input(s) detected. USB/UVC devices appear by their macOS names. Detection does not confirm HDMI signal.</small>}
      {signal && <small className="dock-note" role="status">{signal}</small>}
      {deviceError && <small className="preview-error" role="alert">{deviceError}</small>}
      {selectedVideo && <small className="dock-note">Mac recording test: enable just this video source. Disable the separate webcam inset. Choose Live video inputs in Preview refresh for moving video. For preview only, two distinct video inputs may share Grid or PiP.</small>}
    </div>
    <div className="clip-form">
      <label>Scene layout
        <select aria-label="Scene layout" value={status.sceneLayout ?? 'grid'} disabled={pending || isStaticPreview || !status.connected}
          onChange={event => void perform('/studio/action', { action: 'sceneLayout', layout: event.target.value })}>
          <option value="grid">Grid · equal display tiles</option>
          <option value="pip">Picture-in-picture · first source primary</option>
        </select>
      </label>
      <small className="help-text">Reorder sources to choose the primary display and inset order. Applies to the selected scene preview and supported Program recordings. Live video mode accepts two named video inputs, but recording currently accepts one.</small>
    </div>
    <div className="rows-scroll">{status.sources.length ? status.sources.map((source, index) => <div className="source-row" key={source.id}>
      <Video size={16}/><span className="truncate">{source.name}</span>
      <button className="icon-button" disabled={pending || isStaticPreview || index === 0} aria-label={`Move ${source.name} up`} title="Move source up" onClick={() => void perform('/studio/action', { action: 'moveSource', sceneItemId: source.id, direction: 'up' })}>↑</button>
      <button className="icon-button" disabled={pending || isStaticPreview || index === status.sources.length - 1} aria-label={`Move ${source.name} down`} title="Move source down" onClick={() => void perform('/studio/action', { action: 'moveSource', sceneItemId: source.id, direction: 'down' })}>↓</button>
      <button disabled={pending || isStaticPreview} title={source.enabled ? 'Hide source' : 'Show source'} className="icon-button" onClick={() => void perform('/studio/action', { action: 'source', sceneItemId: source.id, enabled: !source.enabled })}>{source.enabled ? <Eye size={18}/> : <EyeOff size={18}/>}</button>
      <button disabled={pending || isStaticPreview} title={`Remove ${source.name}`} aria-label={`Remove ${source.name}`} className="icon-button" onClick={() => {
        if (window.confirm(`Remove source "${source.name}" from this scene?`)) void perform('/studio/action', { action: 'removeSource', sceneItemId: source.id })
      }}><X size={17}/></button>
    </div>) : <div className="empty-list">Choose a display or supported video capture device and add it to your scene.</div>}</div>
  </Panel>
}

function MixerPanel({ status, perform, pending }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean }) {
  return <Panel title="Audio Mixer" icon={AudioLines}>
    <div className="rows-scroll">{status.inputs.length ? status.inputs.map((input) => <div className="mixer-row" key={input.name}>
      <div className="mixer-title"><Volume2 size={17}/><span className="truncate">{input.name}</span><span className="subdued">{input.muted ? 'Muted' : `${Math.round(input.volume * 100)}%`}</span></div>
      <div className="mixer-control"><div className="fake-meter" aria-label="Audio input (level meter unavailable via WebSocket)"/><div className="volume-controls"><input disabled={pending || input.muted} aria-label={`${input.name} volume`} type="range" min="0" max="2" step="0.05" value={input.volume} onChange={(e) => {
        const updatedVolume = Number(e.target.value)
        void perform('/studio/action', { action: 'volume', inputName: input.name, volume: updatedVolume })
      }}/><button disabled={pending} title={input.muted ? 'Unmute' : 'Mute'} className="icon-button" onClick={() => void perform('/studio/action', { action: 'mute', inputName: input.name, muted: !input.muted })}>{input.muted ? <VolumeX size={17}/> : <Volume2 size={17}/>}</button></div></div>
    </div>) : <div className="empty-list">Audio capture and mixing are scheduled for the native media engine.</div>}</div>
  </Panel>
}

function Studio({ status, perform, pending, setTab, active }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean; setTab: (t: Tab) => void; active: boolean }) {
  const [refreshRate, setRefreshRate] = useState<PreviewRefresh>('fast')
  const macVideoPreview = runningInTauri() && /Macintosh|Mac OS X/i.test(navigator.userAgent)
  useEffect(() => {
    if (!active || refreshRate !== 'live' || !macVideoPreview) return
    const release = () => { void request('/capture/stop-video-previews', {}).catch(() => { /* native lease is a second cleanup path */ }) }
    const onVisibility = () => { if (document.hidden) release() }
    document.addEventListener('visibilitychange',onVisibility)
    return () => { document.removeEventListener('visibilitychange',onVisibility); release() }
  }, [active, refreshRate, macVideoPreview])
  const selectedScene = status.studioMode ? status.previewScene : status.currentScene
  const sceneRevision = JSON.stringify([status.sceneLayout, status.sources.map(source => [source.id, source.enabled, source.deviceId])])
  const revisionFor = (name: string) => name === selectedScene ? sceneRevision : name
  return <div className="production-studio">
    <div className="production-toolbar">
      <div className="production-heading"><h1>Studio</h1><span className="production-mode">{status.connected ? 'Local workspace' : 'Offline'}</span></div>
      <div className="production-toolbar-actions">
        <label className="preview-rate-control">Preview refresh <select aria-label="Preview refresh" value={refreshRate} onChange={e => setRefreshRate(e.target.value as PreviewRefresh)}>{macVideoPreview && <option value="live">Live video inputs (Mac)</option>}<option value="fast">1 second</option><option value="normal">5 seconds</option><option value="paused">Paused</option></select></label>
        <button className={`tool-button ${status.studioMode ? 'tool-active' : ''}`} aria-pressed={status.studioMode} disabled={!status.connected || pending} onClick={() => void perform('/studio/action', { action: 'studioMode', studioModeEnabled: !status.studioMode })}><Layers size={15}/>Studio Mode</button>
        {status.studioMode && <button className="tool-button take-button" disabled={!status.connected || pending} onClick={() => void perform('/studio/action', { action: 'transition' })}><RefreshCcw size={15}/>Take</button>}
      </div>
    </div>
    <div className={`production-monitor-grid ${status.studioMode ? 'dual-monitor' : 'single-monitor'}`}>
      {status.studioMode && <div className="production-monitor preview-monitor"><ScenePreview status={status} label="Preview" previewScene={status.previewScene || status.currentScene} active={active} refreshRate={refreshRate} revision={revisionFor(status.previewScene || status.currentScene)}/><span className="monitor-scene-name">{status.previewScene || status.currentScene || 'No scene'}</span></div>}
      <div className="production-monitor program-monitor"><ScenePreview status={status} label="Program" previewScene={status.currentScene} active={active} refreshRate={refreshRate} revision={revisionFor(status.currentScene)}/><span className="monitor-scene-name">{status.currentScene || 'No scene'} <small>· Not live output</small></span></div>
    </div>
    <QuickCapture active={active} scenes={<ScenesPanel status={status} perform={perform} pending={pending}/>}
      sources={<SourcesPanel status={status} perform={perform} pending={pending}/>}
      otherAudio={status.inputs.length ? <MixerPanel status={status} perform={perform} pending={pending}/> : undefined}
      openClips={() => setTab('Clips')} openSettings={() => setTab('Settings')}
      streamControl={<button className={`go-live dock-stream ${status.streaming ? 'stop-live' : ''}`} disabled={!status.broadcastAvailable || pending} title="Native RTMPS transport is not implemented" onClick={() => void perform('/studio/action', { action: 'stream' })}>{status.streaming ? <Square size={15}/> : <Radio size={16}/>} {status.streaming ? 'End Stream' : 'Go Live (unavailable)'}</button>}/>
  </div>
}

function Settings({ status }: { status: StudioStatus }) {
  const [displays, setDisplays] = useState<{ id: number; name: string; width: number; height: number }[]>([])
  const [captureError, setCaptureError] = useState('')
  const [collectionMessage, setCollectionMessage] = useState('')
  const collectionInput = useRef<HTMLInputElement>(null)
  async function exportScenes() {
    try {
      const data = await request<object>('/studio/collection')
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
      const href = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = href
      anchor.download = 'moobee-studio-scenes.json'
      document.body.append(anchor)
      anchor.click()
      anchor.remove()
      setTimeout(() => URL.revokeObjectURL(href), 5000)
      setCollectionMessage('Scene collection exported.')
    } catch (error) { setCollectionMessage(`Export failed: ${(error as Error).message}`) }
  }
  async function importScenes(file?: File) {
    if (!file) return
    if (file.size > 128 * 1024) { setCollectionMessage('Scene file exceeds the 128 KB safety limit.'); return }
    if (!window.confirm('Replace all current scenes with this imported collection? Export a backup first.')) return
    try {
      const data: unknown = JSON.parse(await file.text())
      await request('/studio/collection', data as object)
      setCollectionMessage('Scene collection imported and validated.')
    } catch (error) { setCollectionMessage(`Import rejected: ${(error as Error).message}`) }
  }
  const reload = useCallback(async () => {
    try {
      const result = await request<{ displays: typeof displays }>('/capture/displays')
      setDisplays(result.displays)
      setCaptureError('')
    } catch (error) { setCaptureError((error as Error).message); setDisplays([]) }
  }, [])
  useEffect(() => { void reload() }, [reload])
  return <><div className="workspace-heading"><div><h1>Settings</h1><p>Native services, capture devices and build readiness. Scene editing works locally; live output is not ready.</p></div></div>
    <Panel title="Runtime Service Diagnostics" icon={Activity}>
      <ServiceDiagnostics sceneConnected={status.connected}/>
    </Panel>
    <div className="settings-layout"><Panel title="Native Capture" icon={Monitor}>
      <div className="info-block"><CheckCircle2 size={22}/><div><strong>Self-contained desktop studio</strong><p>Scene control and test alerts run inside MooBee. Display screenshots use packaged native Rust capture code on macOS and Windows.</p></div></div>
      <div className="info-block"><Video size={22}/><div><strong>Available displays</strong><p>{displays.length ? displays.map(d => `${d.name} (${d.width} × ${d.height})`).join(', ') : 'No displays detected in this edition. Use the installed Mac/Windows app, and grant screen recording permission where prompted.'}</p></div></div>
      <button className="secondary-button" onClick={() => void reload()} disabled={isStaticPreview}><RefreshCcw size={16}/>Refresh displays</button>
      {captureError && <p className="help-text" role="alert">{captureError}</p>}
    </Panel><Panel title="Broadcast Readiness" icon={Shield}>
      <div className="info-block"><Link2 size={22}/><div><strong>Local scene API</strong><p>{status.connected ? 'Ready for scene control' : 'Start the desktop app or local API to edit scenes'} — no Native engine settings or third-party service.</p></div></div>
      <div className="info-block"><CircleHelp size={22}/><div><strong>Encoding and streaming</strong><p>Bounded native Program recording tests are available in Studio. Continuous production recording and Go Live remain unavailable pending hardware and RTMPS validation.</p></div></div>
      <p className="help-text">The web preview has no local device permission. macOS needs Screen Recording access; Windows may request screen or device permissions.</p>
    </Panel><Panel title="GitHub Releases & Updates" icon={RefreshCcw}>
      <GithubUpdates appVersion={appVersion}/>
      <p className="help-text">Installed frontend build: <code>{buildIdentity}</code>. Include this identifier when reporting recording errors or hardware-test results.</p>
    </Panel><Panel title="Scene Collection Backup" icon={Layers}>
      <div className="collection-backup">
        <p>Back up, restore or move scenes between MooBee installations. Imports are validated before replacing your current collection.</p>
        <div className="collection-actions">
          <button className="secondary-button" disabled={isStaticPreview || !status.connected} onClick={() => void exportScenes()}><Copy size={16}/>Export JSON</button>
          <button className="secondary-button" disabled={isStaticPreview || !status.connected} onClick={() => collectionInput.current?.click()}><Layers size={16}/>Import JSON</button>
        </div>
        <input ref={collectionInput} type="file" accept=".json,application/json" hidden aria-label="Choose scene collection file" onChange={event => {
          const file = event.currentTarget.files?.[0]
          event.currentTarget.value = ''
          void importScenes(file)
        }}/>
        {collectionMessage && <p className="collection-feedback" role="status">{collectionMessage}</p>}
      </div>
    </Panel></div>
  </>
}

function Alerts({ perform }: { perform: (path: string, body: object) => Promise<boolean> }) {
  const [name, setName] = useState('ViceCityViewer')
  const [type, setType] = useState('follow')
  const [message, setMessage] = useState('Thanks for supporting the stream!')
  return <><div className="workspace-heading"><div><h1>Alerts</h1><p>Test MooBee's own animated alert notifications.</p></div></div>
    <div className="settings-layout"><Panel title="Alert Test Bench" icon={Bell}><form className="settings-form" onSubmit={(e) => { e.preventDefault(); void perform('/alerts/test', { name, type, message }) }}>
      <label>Alert type<select value={type} onChange={(e) => setType(e.target.value)}><option value="follow">Follow</option><option value="subscription">Subscription</option><option value="tip">Tip</option><option value="raid">Raid</option></select></label>
      <label>Display name<input value={name} onChange={(e) => setName(e.target.value)} maxLength={48}/></label>
      <label>Message<input value={message} onChange={(e) => setMessage(e.target.value)} maxLength={120}/></label>
      <button type="submit" className="primary-button" disabled={isStaticPreview}><WandSparkles size={17}/>{isStaticPreview ? 'Test alerts need the local app' : 'Send Test Alert'}</button>
    </form></Panel><Panel title="How Alerts Work" icon={Sparkles}><div className="step"><span>1</span><div><strong>View the local overlay</strong><p>Open Overlays to preview MooBee's own alert visuals.</p></div></div><div className="step"><span>2</span><div><strong>Native compositing is next</strong><p>The media engine will render alert layers directly into MooBee's scene output.</p></div></div><div className="step"><span>3</span><div><strong>Send a test</strong><p>The test appears in MooBee's local overlay endpoint. Live platform events and output compositing follow.</p></div></div></Panel></div>
  </>
}

function Overlays({ setTab }: { setTab: (tab: Tab) => void }) {
  const url = isStaticPreview ? 'Run the local web edition to get an overlay URL' : runningInTauri() ? 'http://127.0.0.1:8788/overlay/alerts' : `${location.origin}/overlay/alerts`
  const [copied, setCopied] = useState(false)
  return <><div className="workspace-heading"><div><h1>Overlays</h1><p>Animated overlays built and controlled by MooBee.</p></div></div><div className="settings-layout"><Panel title="Local Alert Overlay" icon={LayoutGrid}>
    <div className="overlay-preview"><div className="overlay-example"><HeartIcon/><span>NEW FOLLOW</span><strong>ViceCityViewer</strong><small>Thanks for joining the stream!</small></div></div>
    <div className="url-row"><code>{url}</code><button className="secondary-button" disabled={isStaticPreview} onClick={() => { void navigator.clipboard.writeText(url).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1800) }) }}><Copy size={16}/>{copied ? 'Copied' : 'Copy URL'}</button></div>
    <p className="help-text">{isStaticPreview ? 'This is only a public preview. The animated browser source is included in the desktop and self-hosted web editions.' : 'This URL is available on the same computer while MooBee Live is running. Broadcast composition and remote delivery are not implemented yet.'}</p>
    <button className="text-button" onClick={() => setTab('Alerts')}>Open alert test bench</button>
  </Panel><Panel title="Overlay Studio Roadmap" icon={LayoutGrid}><FeatureBullet title="Alert browser source" detail="Working test alerts and animated overlay" ready/><FeatureBullet title="Theme designer" detail="Editable colors, typography, animations"/><FeatureBullet title="Goals and widgets" detail="Follower goals, chat box, donation progress"/><FeatureBullet title="Reactive overlay engine" detail="Game events and AI-powered responses"/></Panel></div>
  </>
}

function HeartIcon() { return <Gift size={28}/> }

type LocalClip = { id: string; path: string; bytes: number; format?: string }
type CapturedClip = LocalClip & { format: string; durationSeconds: number; frameCount: number; width: number; height: number; sourceFrameCount?: number; fps?: number }
type Screen = { id: number; name: string; width: number; height: number }
type CaptureProbe = { displayId: number; durationMs: number; frameCount: number; width: number; height: number; averageFps: number; firstFrameMs: number | null; longestFrameGapMs: number | null; meanFrameGapMs: number | null }

function ClipLab() {
  const [screens, setScreens] = useState<Screen[]>([])
  const [displayId, setDisplayId] = useState('')
  const [seconds, setSeconds] = useState(4)
  const [clips, setClips] = useState<LocalClip[]>([])
  const [selectedClip, setSelectedClip] = useState('')
  const [previewUrl, setPreviewUrl] = useState('')
  const [lastCapture, setLastCapture] = useState<CapturedClip | null>(null)
  const [working, setWorking] = useState(false)
  const [probing, setProbing] = useState(false)
  const [probeResult, setProbeResult] = useState<CaptureProbe | null>(null)
  const [error, setError] = useState('')
  const [macMp4Working, setMacMp4Working] = useState(false)
  const [macMp4Supported, setMacMp4Supported] = useState(false)
  const [macMicSupported, setMacMicSupported] = useState(false)
  const [macMp4Reason, setMacMp4Reason] = useState('Checking native MP4 compatibility…')
  const [captureSystemAudio, setCaptureSystemAudio] = useState(true)
  const [captureMicrophone, setCaptureMicrophone] = useState(false)
  const [macMp4Result, setMacMp4Result] = useState<{ path: string; bytes: number; durationSeconds: number; audioTrackVerified: boolean } | null>(null)
  const desktop = runningInTauri()
  const isMacDesktop = desktop && /Macintosh|Mac OS X/i.test(navigator.userAgent)

  const reload = useCallback(async () => {
    if (!desktop) return
    try {
      const [screenData, clipData] = await Promise.all([
        request<{ displays: Screen[] }>('/capture/displays'),
        request<{ clips: LocalClip[] }>('/clips')
      ])
      setScreens(screenData.displays)
      setDisplayId(previous => screenData.displays.some(item => String(item.id) === previous)
        ? previous : String(screenData.displays[0]?.id ?? ''))
      setClips(clipData.clips)
    } catch (cause) { setError((cause as Error).message) }
  }, [desktop])
  useEffect(() => { void reload() }, [reload])
  useEffect(() => {
    if (!isMacDesktop) return
    let active = true
    void request<{ supported: boolean; requiredMacos: string; osVersion?: string; reason?: string; microphoneSupported?: boolean }>('/clips/macos-mp4-availability')
      .then(result => {
        if (active) {
          setMacMp4Supported(result.supported)
          setMacMicSupported(Boolean(result.microphoneSupported))
          setMacMp4Reason(result.reason || 'Native MP4 availability not confirmed')
        }
      })
      .catch(cause => {
        if (active) {
          setMacMp4Supported(false)
          setMacMp4Reason('Could not check native MP4 compatibility: ' + (cause as Error).message)
        }
      })
    return () => { active = false }
  }, [isMacDesktop])

  async function selectClip(id: string, format: string = 'gif') {
    setError('')
    setSelectedClip(id)
    setPreviewUrl('')
    if (format === 'avi') return
    try {
      const data = await request<{ dataUrl: string }>(`/clips/preview?id=${encodeURIComponent(id)}`)
      setPreviewUrl(data.dataUrl)
    } catch (cause) { setError((cause as Error).message) }
  }
  async function capture() {
    if (!desktop || !displayId || probing) return
    setWorking(true)
    setError('')
    try {
      const clip = await request<CapturedClip>('/clips/capture', {
        displayId: Number(displayId), seconds
      })
      setLastCapture(clip)
      await reload()
      await selectClip(clip.id)
    } catch (cause) {
      setError((cause as Error).message)
    } finally { setWorking(false) }
  }
  async function captureAvi() {
    if (!desktop || !displayId || probing || working || seconds > 8) return
    setWorking(true)
    setError('')
    setProbeResult(null)
    try {
      const clip = await request<CapturedClip>('/clips/capture-avi', {
        displayId: Number(displayId), seconds
      })
      setLastCapture(clip)
      setSelectedClip(clip.id)
      setPreviewUrl('')
      await reload()
    } catch (cause) {
      setError('Native AVI recording failed: ' + (cause as Error).message)
    } finally { setWorking(false) }
  }
  async function captureMacMP4() {
    if (!isMacDesktop || !macMp4Supported || !displayId || working || probing || macMp4Working || seconds < 3 || seconds > 15) return
    setMacMp4Working(true)
    setMacMp4Result(null)
    setError('')
    try {
      const result = await request<{ id: string; path: string; bytes: number; durationSeconds: number; audioTrackVerified: boolean }>('/clips/capture-mp4', {
        displayId: Number(displayId),
        seconds, systemAudio: captureSystemAudio, microphone: captureMicrophone
      })
      setMacMp4Result(result)
      setSelectedClip(result.id)
      setPreviewUrl('')
      await reload()
    } catch (cause) {
      setError('Native macOS MP4 capture failed: ' + (cause as Error).message)
    } finally {
      setMacMp4Working(false)
    }
  }
  async function measureCapture() {
    if (!desktop || !displayId || working || probing) return
    setProbing(true)
    setProbeResult(null)
    setError('')
    try {
      const report = await request<CaptureProbe>('/capture/probe', {
        displayId: Number(displayId), seconds: 3
      })
      setProbeResult(report)
    } catch (cause) {
      setError('Native frame diagnostic failed: ' + (cause as Error).message)
    } finally {
      setProbing(false)
    }
  }
  const selected = clips.find(item => item.id === selectedClip)
  return <><div className="workspace-heading"><div><h1>Clips</h1><p>Local recording: native GIF or AVI test files, plus browser-composited video with optional webcam and audio.</p></div><span className="status-pill">Native AVI test · 10 fps</span></div>
    {!desktop && <BrowserRecorder/>}
    <div className="settings-layout clip-lab-grid">
      <Panel title="Capture a test clip" icon={Clapperboard}>
        <div className="clip-form">
          <p>Create a silent GIF from sampled screenshots, or a short <strong>MJPEG AVI</strong> video from consecutive native frames. AVI is limited to 640×360 at 10 fps. Neither mode records audio or streams live.</p>
          <label>Display
            <select value={displayId} disabled={!desktop || working || screens.length === 0} onChange={event => setDisplayId(event.target.value)}>
              {screens.length ? screens.map(item => <option key={item.id} value={item.id}>{item.name} · {item.width}×{item.height}</option>) : <option value="">No supported desktop display</option>}
            </select>
          </label>
          <label>Clip length
            <select value={seconds} disabled={!desktop || working} onChange={event => setSeconds(Number(event.target.value))}>
              {[2, 4, 6, 8, 10].map(value => <option key={value} value={value}>{value} seconds</option>)}
            </select>
          </label>
          {isMacDesktop && <div className="clip-mp4-options">
            <p><strong>Experimental native H.264 MP4 (macOS 14+)</strong> — display capture using ScreenCaptureKit. It does not yet capture MooBee’s saved scenes, webcam overlay, or provide streaming.</p>
            <label><input type="checkbox" checked={captureSystemAudio} disabled={working || probing || macMp4Working}
              onChange={event => setCaptureSystemAudio(event.target.checked)}/> Include system audio when available</label>
            <label><input type="checkbox" checked={captureMicrophone} disabled={working || probing || macMp4Working || !macMicSupported}
              onChange={event => setCaptureMicrophone(event.target.checked)}/> Request microphone audio (macOS 15+ only)</label>
            <small>Mac screen and microphone permissions may be required. Requesting audio does not guarantee an audio track; check the saved file independently.</small>
          </div>}
          <div className="clip-actions">
            <button className="primary-button" disabled={!desktop || !displayId || working || probing} onClick={() => void capture()}>
              <Film size={17}/>{working ? 'Capturing frames…' : 'Capture Silent GIF'}
            </button>
            <button className="secondary-button" disabled={!desktop || !displayId || working || probing || seconds > 8} onClick={() => void captureAvi()} title={seconds > 8 ? 'AVI test recording supports 2–8 seconds' : 'Save a silent MJPEG AVI video'}>
              <Video size={17}/>Capture Video AVI
            </button>
            {isMacDesktop && <button className="secondary-button" disabled={!macMp4Supported || !displayId || working || probing || macMp4Working || seconds < 3 || seconds > 15}
              onClick={() => void captureMacMP4()} title={!macMp4Supported ? macMp4Reason : seconds < 3 ? 'Select 4–10 seconds' : 'Native H.264 MP4 test'}>
              <Video size={17}/>{macMp4Working ? 'Capturing native MP4…' : 'Capture MP4 (macOS 14+ test)'}
            </button>}
            {isMacDesktop && !macMp4Supported && <small role="status">{macMp4Reason} · GIF and silent AVI capture remain available.</small>}
            <button className="secondary-button" disabled={!desktop || working} onClick={() => void reload()}><RefreshCcw size={16}/>Refresh</button>
          </div>
          <p className="help-text">{desktop ? 'All files stay on your computer. Grant Screen Recording permission on macOS if prompted. AVI playback is available in external players such as VLC.' : 'Native test capture is only available in the installed Windows/macOS app. Web and Android editions cannot use native GIF/AVI capture.'}</p>
          {error && <p role="alert" className="clip-error">{error}{error.startsWith('Native macOS MP4 capture failed:') && <><br/>This attempt did not save a new recording. Any selected video on the right is an earlier saved clip.</>}</p>}
          {macMp4Result && <div className="clip-result" role="status"><CheckCircle2 size={18}/><div>
            <strong>MP4 file saved — playback/audio unverified</strong>
            <small>{macMp4Result.durationSeconds}s requested · {(macMp4Result.bytes / (1024 * 1024)).toFixed(2)} MB · H.264 test</small>
            <code>{macMp4Result.path}</code>
            <button className="secondary-button" onClick={() => void navigator.clipboard.writeText(macMp4Result.path)}>
              <Copy size={15}/>Copy file path</button>
            <p>Open the MP4 in QuickTime or VLC to verify duration, image, and audio tracks. MooBee has not yet verified the encoded audio.</p>
          </div></div>}
          {lastCapture && <div className="clip-result"><CheckCircle2 size={18}/><div><strong>Saved {lastCapture.frameCount} frames · {lastCapture.format.toUpperCase()}</strong><small>{lastCapture.durationSeconds}s · {lastCapture.width}×{lastCapture.height}{lastCapture.fps ? ` · ${lastCapture.fps} fps` : ''} · {(lastCapture.bytes / (1024 * 1024)).toFixed(2)} MB</small><code>{lastCapture.path}</code><button className="secondary-button" onClick={() => void navigator.clipboard.writeText(lastCapture.path)}><Copy size={15}/>Copy file path</button></div></div>}
        </div>
      </Panel>
      <Panel title="Local test clips" icon={Film}>
        <div className="clip-preview">
          {(selected?.format === 'avi' || selected?.format === 'mp4') ? <div className="preview-empty"><Film size={32}/><strong>Video file saved</strong><span>Use the local path below to open AVI in VLC or MP4 in QuickTime/VLC for playback and audio checks.</span></div> :
            previewUrl ? <img src={previewUrl} alt="Animated local test capture preview"/> :
            <div className="preview-empty"><Film size={32}/><strong>No clip selected</strong><span>Capture a GIF or AVI test clip and select it from the list</span></div>}
        </div>
        <div className="clip-list">
          {clips.length ? clips.map((clip, index) => <button key={clip.id} className={`clip-list-row ${selectedClip === clip.id ? 'active' : ''}`} disabled={working} onClick={() => void selectClip(clip.id, clip.format)}>
            <Film size={16}/><span className="truncate">Test clip {clips.length - index} · {(clip.format ?? 'gif').toUpperCase()}</span><small>{(clip.bytes / (1024 * 1024)).toFixed(2)} MB</small>
          </button>) : <p className="empty-list">No test clips have been saved on this device.</p>}
        </div>
        {selected && <div className="clip-path"><span>Saved locally</span><code>{selected.path}</code>
          <div className="clip-actions">
            <button className="secondary-button" onClick={() => void navigator.clipboard.writeText(selected.path)}><Copy size={15}/>Copy path</button>
            {isMacDesktop && <button className="secondary-button" onClick={() => {
              void request('/clips/reveal', { clipId: selected.id })
                .catch(cause => setError('Finder could not reveal clip: ' + (cause as Error).message))
            }}><Film size={15}/>Show in Finder</button>}
          </div></div>}
      </Panel>
    </div>
    {desktop && <Panel title="Native Capture Input Diagnostic" icon={Activity}>
        <div className="capture-diagnostic">
          <p>Measure frames arriving from the independent native screen-capture source, without saving video. This is not an encoder or a Go Live test.</p>
          <button className="secondary-button" disabled={!displayId || working || probing} onClick={() => void measureCapture()}>
            <Activity size={16}/>{probing ? 'Measuring input frames…' : 'Measure 3-second input FPS'}
          </button>
          {probeResult && <div className="capture-diagnostic-results" role="status">
            <div><span>Incoming frames</span><strong>{probeResult.frameCount}</strong></div>
            <div><span>Measured input FPS</span><strong>{probeResult.averageFps.toFixed(1)}</strong></div>
            <div><span>Frame size</span><strong>{probeResult.width}×{probeResult.height}</strong></div>
            <div><span>Longest frame gap</span><strong>{probeResult.longestFrameGapMs === null ? '—' : probeResult.longestFrameGapMs + ' ms'}</strong></div>
            <p>First frame: {probeResult.firstFrameMs ?? '—'} ms · Observed duration: {(probeResult.durationMs / 1000).toFixed(2)}s. This is a source performance measurement, not encoded output FPS.</p>
          </div>}
        </div>
      </Panel>}
    <div className="clip-roadmap"><strong>Still to build:</strong> independent Windows MP4/audio recording, cross-platform synchronized A/V and webcam scene composition, sustained capture, recoverable containers, and RTMP/RTMPS livestreaming. The main Record and Go Live controls remain disabled until those features are tested.</div>
  </>
}

function FeatureBullet({ title, detail, ready = false }: { title: string; detail: string; ready?: boolean }) {
  return <div className="feature-bullet"><span className={ready ? 'feature-icon-ready' : 'feature-icon-planned'}>{ready ? <Check size={16}/> : <MoreHorizontal size={16}/>}</span><div><strong>{title}</strong><p>{detail}</p></div><span className="feature-status">{ready ? 'Available' : 'Planned'}</span></div>
}

const moduleDetails: Record<'Multistream' | 'AI Sidekick' | 'Clips' | 'Chatbot' | 'Analytics', { lead: string; features: string[] }> = {
  Multistream: { lead: 'One incoming stream, multiple outgoing destinations through a managed relay.', features: ['MooBee-native RTMP relay and destination manager', 'Twitch, YouTube, Kick and custom RTMP destinations', 'Per-destination stream health and reconnects', 'Secure platform OAuth and encrypted stream credentials'] },
  'AI Sidekick': { lead: 'An optional local-first co-producer for live broadcasting.', features: ['Automation triggers and scene switching', 'Locally managed model runtime with optional GPU acceleration', 'Chat-aware co-host and content suggestions', 'Stream event detection with human approval controls'] },
  Clips: { lead: 'Turn broadcasts and recordings into polished short-form content.', features: ['MooBee native replay buffer and recording ingest', 'Built-in highlight detection, replay clipper and timeline editor', 'Vertical reframing, subtitles, and video editor', 'Export and publish workflows'] },
  Chatbot: { lead: 'Moderate and engage viewers across connected streaming channels.', features: ['Platform chat connectors and role permissions', 'Commands, timers, loyalty, giveaways', 'Custom named bot with moderation rules', 'Shared activity feed and audit logs'] },
  Analytics: { lead: 'A consolidated view of broadcast performance and creator growth.', features: ['MooBee native encoder and capture health', 'Destination stream diagnostics', 'Audience, engagement, and alert histories', 'Historical reports and exports'] }
}

function PlannedModule({ tab }: { tab: keyof typeof moduleDetails }) {
  const content = moduleDetails[tab]
  return <><div className="workspace-heading"><div><h1>{tab}</h1><p>{content.lead}</p></div><span className="status-pill">Planned milestone</span></div>
    <div className="module-intro"><div className="module-symbol"><Clapperboard size={36}/></div><h2>{tab} is on the roadmap</h2><p>We’re building on top of the independent scene controller and local overlay foundation. This module is not connected yet.</p><div className="module-list">{content.features.map((feature) => <div key={feature}><CheckCircle2 size={17}/>{feature}</div>)}</div></div>
  </>
}

export default function App() {
  const [tab, setTab] = useState<Tab>('Studio')
  const [drawerOpen, setDrawerOpen] = useState(false)
  const [activityOpen, setActivityOpen] = useState(false)
  const productionTab = tab === 'Studio' || tab === 'Scenes'
  const { status, activity, alert, pending, error, setError, perform, refresh } = useStudio()
  const go = (target: Tab) => { setTab(target); setDrawerOpen(false) }
  return <div className={`app-shell production-shell ${productionTab ? 'studio-shell' : ''} ${activityOpen ? 'activity-open' : 'activity-closed'}`}>
    <header className="app-topbar"><div className="brand"><RadioTower size={27}/><span>MooBee <em>Studio</em></span></div><div className="top-divider"/><div className="project-select"><span>Workspace</span><strong>My Studio</strong></div><div className="top-spacer"/><div className="destination-pills"><span className="dest-label">Destinations</span><span className="destination"><span className="dest-icon twitch">T</span>Twitch <small>Not linked</small></span><span className="destination"><span className="dest-icon youtube">▶</span>YouTube <small>Not linked</small></span><span className="destination"><span className="dest-icon kick">K</span>Kick <small>Not linked</small></span></div><div className="top-divider"/><button className="top-activity-toggle tool-button" type="button" aria-pressed={activityOpen} onClick={() => setActivityOpen(value => !value)}><Activity size={15}/>Activity</button><span className="profile">MT</span><button className="mobile-menu" type="button" aria-label={drawerOpen ? 'Close navigation' : 'Open navigation'} aria-expanded={drawerOpen} onClick={() => setDrawerOpen(!drawerOpen)}><LayoutGrid size={22}/></button></header>
    <aside className={`sidebar ${drawerOpen ? 'drawer-open' : ''}`}><nav>{navigation.map(({ label, icon: Icon }) => <button key={label} className={`nav-item ${tab === label ? 'active' : ''}`} onClick={() => go(label)}><Icon size={19}/><span>{label}</span></button>)}</nav><div className="sidebar-bottom"><div className="release-label" title="Release tag and immutable GitHub commit"><span>BUILD {buildIdentity}</span></div><button onClick={() => go('Settings')}><CircleHelp size={16}/>Engine guide</button></div></aside>
    <main className={`workspace ${productionTab ? 'production-workspace' : ''}`}>
      {error && <div className="error-banner" role="alert"><span>{error}</span><button onClick={() => setError('')}><X size={17}/></button></div>}
      <div className="retained-studio" hidden={!productionTab}><Studio status={status} perform={perform} pending={pending} setTab={go} active={productionTab}/></div>
      {tab === 'Settings' && <Settings status={status}/>}
      {tab === 'Alerts' && <Alerts perform={perform}/>}
      {tab === 'Overlays' && <Overlays setTab={go}/>}
      {tab === 'Clips' && <ClipLab/>}
      {(tab === 'Multistream' || tab === 'AI Sidekick' || tab === 'Chatbot' || tab === 'Analytics') && <PlannedModule tab={tab}/>}
    </main>
    <aside className="right-rail" aria-label="Activity dock"><div className="rail-tabs"><span><Activity size={16}/> Activity</span><button className="icon-button" aria-label="Close activity" onClick={() => setActivityOpen(false)}><X size={15}/></button></div><div className="assistant-summary"><div><Sparkles size={21}/><strong>AI Sidekick</strong><span className="small-tag">Coming soon</span></div><p>Scene automations and an optional local AI co-host are planned for a later milestone.</p></div>
      <Panel title="Stream Activity" icon={Activity} action={<button className="icon-button" onClick={() => void refresh()} title="Refresh"><RefreshCcw size={15}/></button>}><div className="activity-list">{activity.length ? activity.slice(0, 14).map((item) => <div className="activity-item" key={item.id}><span className="activity-dot"/><div><p>{item.message}</p><small>{new Date(item.time).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</small></div></div>) : <div className="empty-list">Local scene changes and test alerts will appear here.</div>}</div></Panel>
      <Panel title="Quick Actions" icon={SlidersHorizontal}><div className="quick-actions"><button onClick={() => go('Alerts')}><Bell size={17}/>Test an alert</button><button onClick={() => go('Overlays')}><LayoutGrid size={17}/>Browser overlay</button><button onClick={() => go('Clips')}><Film size={17}/>Capture a test GIF</button><button onClick={() => go('Settings')}><Terminal size={17}/>Native engine settings</button></div></Panel>
      {alert && activityOpen && <div className="recent-test"><Bell size={18}/>Last test: {alert.name}</div>}
    </aside>
    <footer className="status-footer"><div className={`health ${status.connected ? 'healthy' : ''}`}><span className="status-dot"/>{status.connected ? (status.streaming ? 'Live Broadcast' : 'Scene Editor Ready · No Live Output') : 'Local engine offline'}</div><div className="footer-data"><span>Encoder <b>In development</b></span><span>Bitrate <b>{status.bitrateKbps === null ? '—' : `${status.bitrateKbps} kbps`}</b></span><span>Uptime <b>{formatElapsed(status.streamSeconds)}</b></span><span>Dropped <b>{status.droppedFrames === null ? '—' : status.droppedFrames}</b></span></div>{alert && !activityOpen && <span className="footer-alert">Last test: {alert.name}</span>}<span className="footer-build" title={buildIdentity}>{buildIdentity}</span></footer>
  </div>
}
