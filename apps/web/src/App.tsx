import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  Activity, AudioLines, Bell, Bot, Check, CheckCircle2, CircleHelp, Clapperboard,
  Copy, Eye, EyeOff, Film, Gift, Layers, LayoutGrid, Link2, ListVideo, Monitor,
  MoreHorizontal, Play, Radio, RadioTower, RefreshCcw, Settings2, Shield, SlidersHorizontal,
  Sparkles, Square, Terminal, Video, Volume2, VolumeX, WandSparkles, X
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { isStaticPreview, request, runningInTauri, subscribeEvents } from './api'
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

function ScenePreview({ status, label, previewScene }: { status: StudioStatus; label: string; previewScene: string }) {
  const [image, setImage] = useState('')
  useEffect(() => {
    if (!status.connected || !previewScene) { setImage(''); return }
    let cancelled = false
    async function load() {
      try {
        const response = await request<{ image: string }>(`/studio/snapshot?scene=${encodeURIComponent(previewScene)}`)
        if (!cancelled) setImage(response.image)
      } catch { if (!cancelled) setImage('') }
    }
    void load()
    const timer = setInterval(() => void load(), 5000)
    return () => { cancelled = true; clearInterval(timer) }
  }, [previewScene, status.connected])
  return <div className="preview-block">
    <div className="preview-heading"><span><Monitor size={15}/>{label}</span><span>{status.connected ? 'Native screenshot · every 5 sec' : 'Local studio offline'}</span></div>
    <div className="preview-canvas">
      {image ? <img src={image} alt={`MooBee scene: ${previewScene}`} /> : <div className="preview-empty"><Radio size={34}/><strong>{status.connected ? 'No preview available' : 'Your stream starts here'}</strong><span>{status.connected ? 'Add or enable a display source in this scene' : 'Add a native display source to preview this scene'}</span></div>}
      {status.streaming && <span className="live-flag">LIVE</span>}
    </div>
  </div>
}

function ScenesPanel({ status, perform, pending }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean }) {
  return <Panel title="Scenes" icon={Layers} action={<button disabled={pending} className="secondary-button" onClick={() => { const name = window.prompt('New scene name'); if (name?.trim()) void perform('/studio/action', { action: 'addScene', sceneName: name.trim() }) }}>+ Add Scene</button>}>
    <div className="rows-scroll">{status.scenes.length ? status.scenes.map((scene) => <button disabled={pending} key={scene} className={`scene-row ${scene === (status.studioMode ? status.previewScene : status.currentScene) ? 'selected' : ''}`} onClick={() => void perform('/studio/action', { action: 'scene', sceneName: scene })}>
      <span className="scene-thumbnail"><Monitor size={18}/></span><span className="truncate">{scene}</span>{scene === status.currentScene && <span className="scene-on-air">ON AIR</span>}{scene === status.previewScene && status.studioMode && <Check size={15}/>}</button>) : <div className="empty-list">No scenes yet. Create one to get started.</div>}</div>
  </Panel>
}

function SourcesPanel({ status, perform, pending }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean }) {
  return <Panel title="Sources" icon={ListVideo} action={<button disabled={pending || !status.captureAvailable || !runningInTauri()} title={runningInTauri() ? 'Add the primary desktop display' : 'Native display sources require the desktop app'} className="secondary-button" onClick={() => { void (async () => {
      const result = await request<{ displays: { id: number; name: string }[] }>('/capture/displays')
      const display = result.displays[0]
      if (!display) throw new Error('No display is available. Check screen recording permission.')
      await perform('/studio/action', { action: 'addSource', kind: 'display', deviceId: String(display.id), name: display.name })
    })().catch(error => window.alert((error as Error).message)) }}>+ Add Display</button>}>
    <div className="rows-scroll">{status.sources.length ? status.sources.map((source) => <div className="source-row" key={source.id}>
      <Video size={16}/><span className="truncate">{source.name}</span><button disabled={pending} title={source.enabled ? 'Hide source' : 'Show source'} className="icon-button" onClick={() => void perform('/studio/action', { action: 'source', sceneItemId: source.id, enabled: !source.enabled })}>{source.enabled ? <Eye size={18}/> : <EyeOff size={18}/>}</button>
    </div>) : <div className="empty-list">Add a display source in the desktop app to begin capturing.</div>}</div>
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

function Studio({ status, perform, pending, setTab }: { status: StudioStatus; perform: (path: string, body: object) => Promise<boolean>; pending: boolean; setTab: (t: Tab) => void }) {
  return <><div className="workspace-heading"><div><h1>Studio</h1><p>{isStaticPreview ? 'Explore the independent MooBee Stream Studio interface.' : 'Broadcast, manage scenes, audio, and your stream tools.'}</p></div><span className={`status-pill ${status.connected ? 'positive' : ''}`}><span className="status-dot"/>{isStaticPreview ? 'Public preview' : status.connected ? 'Local studio ready' : 'Local studio offline'}</span></div>
    <div className="preview-grid"><ScenePreview status={status} label={status.studioMode ? 'Preview · next scene' : 'Preview · mirrored'} previewScene={status.previewScene || status.currentScene}/><div className="transition-slot"><button className="transition-button" disabled={!status.studioMode || !status.connected || pending} title="Take preview scene live" onClick={() => void perform('/studio/action', { action: 'transition' })}><RefreshCcw size={20}/><span>Take</span></button></div><ScenePreview status={status} label="Program · live output" previewScene={status.currentScene}/></div>
    <div className="broadcast-toolbar"><button className={`tool-button ${status.studioMode ? 'tool-active' : ''}`} disabled={!status.connected || pending} onClick={() => void perform('/studio/action', { action: 'studioMode', studioModeEnabled: !status.studioMode })}><Layers size={17}/>{status.studioMode ? 'Studio Mode On' : 'Enable Studio Mode'}</button><button className={`tool-button ${status.recording ? 'tool-active' : ''}`} disabled={!status.recordAvailable || pending} title="Native recorder is under development" onClick={() => void perform('/studio/action', { action: 'record' })}><span className="record-dot"/>{status.recording ? 'Stop Recording' : 'Record (planned)'}</button><button className="tool-button" onClick={() => setTab('Overlays')}><LayoutGrid size={16}/>Overlays</button><button className="tool-button" onClick={() => setTab('Alerts')}><Bell size={16}/>Test Alerts</button><button className="tool-button" onClick={() => setTab('Settings')}><Settings2 size={16}/>Engine Settings</button></div>
    <div className="control-grid"><ScenesPanel status={status} perform={perform} pending={pending}/><SourcesPanel status={status} perform={perform} pending={pending}/><MixerPanel status={status} perform={perform} pending={pending}/></div>
  </>
}

function Settings({ status }: { status: StudioStatus }) {
  const [displays, setDisplays] = useState<{ id: number; name: string; width: number; height: number }[]>([])
  const [captureError, setCaptureError] = useState('')
  const reload = useCallback(async () => {
    try {
      const result = await request<{ displays: typeof displays }>('/capture/displays')
      setDisplays(result.displays)
      setCaptureError('')
    } catch (error) { setCaptureError((error as Error).message); setDisplays([]) }
  }, [])
  useEffect(() => { void reload() }, [reload])
  return <><div className="workspace-heading"><div><h1>Settings</h1><p>MooBee's own local production engine. No other broadcaster or WebSocket service required.</p></div></div>
    <div className="settings-layout"><Panel title="Native Capture" icon={Monitor}>
      <div className="info-block"><CheckCircle2 size={22}/><div><strong>Self-contained desktop studio</strong><p>Scene control and test alerts run inside MooBee. Display screenshots use packaged native Rust capture code on macOS and Windows.</p></div></div>
      <div className="info-block"><Video size={22}/><div><strong>Available displays</strong><p>{displays.length ? displays.map(d => `${d.name} (${d.width} × ${d.height})`).join(', ') : 'No displays detected in this edition. Use the installed Mac/Windows app, and grant screen recording permission where prompted.'}</p></div></div>
      <button className="secondary-button" onClick={() => void reload()} disabled={isStaticPreview}><RefreshCcw size={16}/>Refresh displays</button>
      {captureError && <p className="help-text" role="alert">{captureError}</p>}
    </Panel><Panel title="Broadcast Readiness" icon={Shield}>
      <div className="info-block"><Link2 size={22}/><div><strong>Local scene API</strong><p>{status.connected ? 'Ready for scene control' : 'Start the desktop app or local API to edit scenes'} — no Native engine settings or third-party service.</p></div></div>
      <div className="info-block"><CircleHelp size={22}/><div><strong>Encoding and streaming</strong><p>Not implemented in this alpha. Recording and Go Live remain disabled until MooBee can produce and verify actual encoded video output.</p></div></div>
      <p className="help-text">The web preview has no local device permission. macOS needs Screen Recording access; Windows may request screen or device permissions.</p>
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

function FeatureBullet({ title, detail, ready = false }: { title: string; detail: string; ready?: boolean }) {
  return <div className="feature-bullet"><span className={ready ? 'feature-icon-ready' : 'feature-icon-planned'}>{ready ? <Check size={16}/> : <MoreHorizontal size={16}/>}</span><div><strong>{title}</strong><p>{detail}</p></div><span className="feature-status">{ready ? 'Available' : 'Planned'}</span></div>
}

const moduleDetails: Record<'Multistream' | 'AI Sidekick' | 'Clips' | 'Chatbot' | 'Analytics', { lead: string; features: string[] }> = {
  Multistream: { lead: 'One incoming stream, multiple outgoing destinations through a managed relay.', features: ['MediaMTX RTMP relay and destination manager', 'Twitch, YouTube, Kick and custom RTMP destinations', 'Per-destination stream health and reconnects', 'Secure platform OAuth and encrypted stream credentials'] },
  'AI Sidekick': { lead: 'An optional local-first co-producer for live broadcasting.', features: ['Automation triggers and scene switching', 'Ollama provider and optional GPU acceleration', 'Chat-aware co-host and content suggestions', 'Stream event detection with human approval controls'] },
  Clips: { lead: 'Turn broadcasts and recordings into polished short-form content.', features: ['MooBee native replay buffer and recording ingest', 'ClipForge integration and automatic highlight detection', 'Vertical reframing, subtitles, and video editor', 'Export and publish workflows'] },
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
  const { status, activity, alert, pending, error, setError, perform, refresh } = useStudio()
  const go = (target: Tab) => { setTab(target); setDrawerOpen(false) }
  return <div className="app-shell">
    <header className="app-topbar"><div className="brand"><RadioTower size={27}/><span>MooBee <em>Studio</em></span></div><div className="top-divider"/><div className="project-select"><span>Workspace</span><strong>My Studio</strong></div><div className="top-spacer"/><div className="destination-pills"><span className="dest-label">Destinations</span><span className="destination"><span className="dest-icon twitch">T</span>Twitch <small>Not linked</small></span><span className="destination"><span className="dest-icon youtube">▶</span>YouTube <small>Not linked</small></span><span className="destination"><span className="dest-icon kick">K</span>Kick <small>Not linked</small></span></div><div className="top-divider"/><span className="profile">MT</span><button className="mobile-menu" onClick={() => setDrawerOpen(!drawerOpen)}><LayoutGrid size={22}/></button></header>
    <aside className={`sidebar ${drawerOpen ? 'drawer-open' : ''}`}><nav>{navigation.map(({ label, icon: Icon }) => <button key={label} className={`nav-item ${tab === label ? 'active' : ''}`} onClick={() => go(label)}><Icon size={19}/><span>{label}</span></button>)}</nav><div className="sidebar-bottom"><div className="release-label">ALPHA 0.3 (DEV)</div><button onClick={() => go('Settings')}><CircleHelp size={16}/>Engine guide</button></div></aside>
    <main className="workspace">
      {error && <div className="error-banner" role="alert"><span>{error}</span><button onClick={() => setError('')}><X size={17}/></button></div>}
      {(tab === 'Studio' || tab === 'Scenes') && <Studio status={status} perform={perform} pending={pending} setTab={go}/>}
      {tab === 'Settings' && <Settings status={status}/>}
      {tab === 'Alerts' && <Alerts perform={perform}/>}
      {tab === 'Overlays' && <Overlays setTab={go}/>}
      {(tab === 'Multistream' || tab === 'AI Sidekick' || tab === 'Clips' || tab === 'Chatbot' || tab === 'Analytics') && <PlannedModule tab={tab}/>}
    </main>
    <aside className="right-rail"><div className="rail-tabs"><span><Activity size={16}/> Activity</span><span className="live-marker"/></div><div className="assistant-summary"><div><Sparkles size={21}/><strong>AI Sidekick</strong><span className="small-tag">Coming soon</span></div><p>Scene automations and an optional local AI co-host are planned for a later milestone.</p></div>
      <Panel title="Stream Activity" icon={Activity} action={<button className="icon-button" onClick={() => void refresh()} title="Refresh"><RefreshCcw size={15}/></button>}><div className="activity-list">{activity.length ? activity.slice(0, 14).map((item) => <div className="activity-item" key={item.id}><span className="activity-dot"/><div><p>{item.message}</p><small>{new Date(item.time).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}</small></div></div>) : <div className="empty-list">Local scene changes and test alerts will appear here.</div>}</div></Panel>
      <Panel title="Quick Actions" icon={SlidersHorizontal}><div className="quick-actions"><button onClick={() => go('Alerts')}><Bell size={17}/>Test an alert</button><button onClick={() => go('Overlays')}><LayoutGrid size={17}/>Browser overlay</button><button onClick={() => go('Settings')}><Terminal size={17}/>Native engine settings</button></div></Panel>
      {alert && <div className="recent-test"><Bell size={18}/>Last test: {alert.name}</div>}
    </aside>
    <footer className="status-footer"><div className={`health ${status.connected ? 'healthy' : ''}`}><span className="status-dot"/>{status.connected ? (status.streaming ? 'Live Broadcast' : 'Studio Ready (No Output)') : 'Local engine offline'}</div><div className="footer-data"><span>Encoder <b>In development</b></span><span>Bitrate <b>{status.bitrateKbps === null ? '—' : `${status.bitrateKbps} kbps`}</b></span><span>Uptime <b>{formatElapsed(status.streamSeconds)}</b></span><span>Dropped <b>{status.droppedFrames === null ? '—' : status.droppedFrames}</b></span></div><button className={`go-live ${status.streaming ? 'stop-live' : ''}`} disabled={!status.broadcastAvailable || pending} title="The native streaming encoder is not implemented yet" onClick={() => void perform('/studio/action', { action: 'stream' })}>{status.streaming ? <Square size={18}/> : <Radio size={20}/>} {status.streaming ? 'End Stream' : 'Go Live (planned)'}</button></footer>
  </div>
}
