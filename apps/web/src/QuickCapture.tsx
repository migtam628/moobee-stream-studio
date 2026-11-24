import { useEffect, useRef, useState, type ReactNode } from 'react'
import { CheckCircle2, Copy, Film, FolderOpen, Monitor, Square, Video, AudioLines, Settings2, X, Volume2, Image as ImageIcon } from 'lucide-react'
import { request, runningInTauri } from './api'
import { isScreenCapturePermissionDenied } from './capturePermission'
import { singleProgramDisplayId, programTestDisplayIds, programTestLayout } from './programDisplay'

type Display = { id: number; name: string; width: number; height: number }
type Availability = { supported: boolean; requiredMacos: string; osVersion?: string | null; reason?: string; microphoneSupported?: boolean; engine?: string }
type GraphicOverlay = { width: number; height: number; rgbaBase64: string }
type CapturedFile = {
  id: string
  path: string
  bytes: number
  format: string
  durationSeconds: number | null
  requestedMaxSeconds?: number
  audioTrackVerified?: boolean
  displayCount?: number
  elapsedMs?: number
  finalizationMs?: number
  webcamIncluded?: boolean
  webcamFrames?: number
  webcamRepeatedSlots?: number
  plannedAudioSamples48khz?: number
  microphoneIncluded?: boolean
  audioSamplesWritten?: number
  audioPeak?: number
  audioClippedSamples?: number
  overlayIncluded?: boolean
  measuredSamplingFps?: number
  sourceFramesReceived?: number[]
  repeatedSourceSlots?: number[]
  lateOutputFrames?: number
  maxSourceArrivalSkewMs?: number
  profile?: 'baseline' | '720p15' | '1080p30'
  width?: number
  height?: number
  fps?: number
}

/**
 * One explicit bounded native DISPLAY recording, not MooBee's program scene.
 * No synthetic status flags or fake broadcast controls are changed here.
 * The full Record/Go Live outputs remain gated on native A/V/RTMP acceptance.
 */
export default function QuickCapture({ openClips, scenes, sources, otherAudio, streamControl, openSettings }: {
  openClips: () => void; scenes: ReactNode; sources: ReactNode; otherAudio?: ReactNode;
  streamControl: ReactNode; openSettings: () => void
}) {
  const diagnostics = useRef<HTMLDialogElement>(null)
  const desktop = runningInTauri()
  const mac = desktop && /Macintosh|Mac OS X/i.test(navigator.userAgent)
  const supportedDesktop = desktop && (mac || /Windows/i.test(navigator.userAgent))
  const [displays, setDisplays] = useState<Display[]>([])
  const [displayId, setDisplayId] = useState('')
  const [mp4Supported, setMp4Supported] = useState(false)
  const [micSupported, setMicSupported] = useState(false)
  const [mp4AvailabilityReason, setMp4AvailabilityReason] = useState('Native MP4 availability has not been checked')
  const [duration, setDuration] = useState(6)
  const [programAviDuration, setProgramAviDuration] = useState(4)
  const [continuousProfile, setContinuousProfile] = useState<'baseline' | '720p15' | '1080p30'>('baseline')
  const [programWebcam, setProgramWebcam] = useState(false)
  const [programMic, setProgramMic] = useState(false)
  const [micGain, setMicGain] = useState(1)
  const [graphic, setGraphic] = useState<GraphicOverlay | null>(null)
  const [graphicName, setGraphicName] = useState('')
  const [graphicLoading, setGraphicLoading] = useState(false)
  const [systemAudio, setSystemAudio] = useState(false)
  const [microphone, setMicrophone] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [captureMode, setCaptureMode] = useState<'screen' | 'program' | null>(null)
  const [stopRequested, setStopRequested] = useState(false)
  const [error, setError] = useState('')
  const [permissionSettingsError, setPermissionSettingsError] = useState('')
  const [saved, setSaved] = useState<CapturedFile | null>(null)
  const [persistedClips, setPersistedClips] = useState<{ id: string; format: string }[] | null>(null)
  const [inventoryError, setInventoryError] = useState('')

  useEffect(() => {
    if (!desktop) return
    let active = true
    setLoading(true)
    Promise.all([
      request<{ displays: Display[] }>('/capture/displays'),
      mac
        ? request<Availability>('/clips/macos-mp4-availability')
        : Promise.resolve<Availability>({ supported: false, requiredMacos: '14.0', reason: 'Native MP4 recording is available on macOS 14+ only.', microphoneSupported: false })
    ]).then(([list, availability]) => {
      if (!active) return
      setDisplays(list.displays)
      setDisplayId(current =>
        list.displays.some(display => String(display.id) === current)
          ? current : String(list.displays[0]?.id ?? ''))
      setMp4Supported(Boolean(availability.supported))
      setMicSupported(Boolean(availability.microphoneSupported))
      setMp4AvailabilityReason(availability.reason ||
        (availability.supported ? 'Native MP4 is available for device testing' : 'Native MP4 unavailable on this operating system'))
      setError('')
    }).catch(cause => {
      if (active) {
        setMp4AvailabilityReason('Could not check native recorder: ' + (cause as Error).message)
        setError('Could not initialize native capture: ' + (cause as Error).message)
      }
    }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [desktop, mac])

  // Read the installed app's persistent clip directory at every Studio mount.
  // Reopening MooBee can then show saved recordings without relying on a
  // browser session or the previous component's in-memory state.
  useEffect(() => {
    if (!desktop) return
    let mounted = true
    void request<{ clips: { id: string; format: string }[] }>('/clips')
      .then(data => { if (mounted) setPersistedClips(data.clips) })
      .catch(cause => { if (mounted) setInventoryError('Cannot read saved clips: ' + (cause as Error).message) })
    return () => { mounted = false }
  }, [desktop])

  async function refreshSavedClips() {
    setInventoryError('')
    try {
      const result = await request<{ clips: { id: string; format: string }[] }>('/clips')
      setPersistedClips(result.clips)
    } catch (cause) {
      setInventoryError('Cannot read saved clips: ' + (cause as Error).message)
    }
  }

  const useMp4 = mac && mp4Supported
  const canRecord = supportedDesktop && !!displayId && !loading && !busy
  const durations = useMp4 ? [4, 6, 8, 15, 30, 60, 120] : [4, 6, 8]
  const formatLabel = useMp4 ? 'H.264 MP4 (macOS 14+ experimental)' : 'silent MJPEG AVI (10 fps)'
  async function selectProgramDisplay() {
    setError('')
    try {
      const collection = await request<unknown>('/studio/collection')
      const id = singleProgramDisplayId(collection)
      if (!displays.some(display => display.id === id)) {
        throw new Error('The Program display is not connected. Refresh capture devices or choose a display manually.')
      }
      setDisplayId(String(id))
    } catch (cause) {
      setError('Program display selection: ' + (cause as Error).message)
    }
  }

  async function capture() {
    if (!canRecord || !durations.includes(duration)) return
    setBusy(true)
    setCaptureMode('screen')
    setStopRequested(false)
    setSaved(null)
    setError('')
    setPermissionSettingsError('')
    try {
      const result = await request<CapturedFile>(
        useMp4 ? '/clips/capture-mp4' : '/clips/capture-avi',
        useMp4
          ? { displayId: Number(displayId), seconds: duration, systemAudio, microphone }
          : { displayId: Number(displayId), seconds: duration }
      )
      setSaved(result)
      await refreshSavedClips()
    } catch (cause) {
      setError('Native recording failed: ' + (cause as Error).message)
    } finally {
      setBusy(false)
      setCaptureMode(null)
      setStopRequested(false)
    }
  }

  async function loadProgramGraphic(file?: File) {
    if (!file) return
    setGraphicLoading(true); setGraphic(null); setGraphicName(''); setError('')
    try {
      if (file.type !== 'image/png' || file.size > 2 * 1024 * 1024) throw new Error('Choose a PNG of 2 MB or less')
      const image = await createImageBitmap(file)
      try {
        if (!image.width || !image.height || image.width > 4096 || image.height > 4096) throw new Error('PNG dimensions exceed 4096 pixels')
        const scale = Math.min(1, 480 / image.width, 270 / image.height)
        const width = Math.max(1, Math.round(image.width * scale))
        const height = Math.max(1, Math.round(image.height * scale))
        const canvas = document.createElement('canvas')
        canvas.width = width; canvas.height = height
        const context = canvas.getContext('2d')
        if (!context) throw new Error('Cannot prepare overlay')
        context.drawImage(image, 0, 0, width, height)
        const pixels = context.getImageData(0, 0, width, height).data
        let binary = ''
        for (let i = 0; i < pixels.length; i += 8192) binary += String.fromCharCode(...pixels.subarray(i, i + 8192))
        setGraphic({ width, height, rgbaBase64: btoa(binary) })
        setGraphicName(file.name)
      } finally { image.close() }
    } catch (cause) { setError('Graphic import failed: ' + (cause as Error).message) }
    finally { setGraphicLoading(false) }
  }

  async function captureProgramVideoTest(mode: 'sampled' | 'continuous' | 'mp4' = 'sampled') {
    if (!supportedDesktop || loading || busy || graphicLoading || (mode === 'mp4' && !mac)) return
    setBusy(true)
    setCaptureMode('program')
    setStopRequested(false)
    setSaved(null)
    setError('')
    try {
      const collection = await request<unknown>('/studio/collection')
      const displayIds = programTestDisplayIds(collection)
      if (displayIds.some(id => !displays.some(display => display.id === id))) {
        throw new Error('One of the Program displays is disconnected. Refresh displays and check the scene.')
      }
      const result = await request<CapturedFile>(mode === 'mp4'
        ? '/clips/capture-program-continuous-mp4'
        : mode === 'continuous' ? '/clips/capture-program-continuous-avi' : '/clips/capture-program-test-avi', {
        displayIds, seconds: programAviDuration, layout: programTestLayout(collection),
        ...(mode !== 'sampled' ? { profile: continuousProfile, overlay: graphic } : {}),
        ...(mode === 'mp4' ? { webcam: programWebcam, microphone: programMic, micGain } : {})
      })
      setSaved(result)
      await refreshSavedClips()
    } catch (cause) {
      setError((mode === 'mp4' ? 'Composed H.264 MP4' : mode === 'continuous' ? 'Continuous Program AVI' : 'Sampled Program AVI') + ' test failed: ' + (cause as Error).message)
    } finally {
      setBusy(false)
      setCaptureMode(null)
    }
  }

  async function stopCapture() {
    if (!useMp4 || !busy || captureMode !== 'screen' || stopRequested) return
    try {
      const result = await request<{ stopRequested: boolean }>('/clips/stop-mp4', {})
      if (!result.stopRequested) throw new Error('No active capture was found; retry if recording is still running')
      setStopRequested(true)
    } catch (cause) {
      setError('Could not request a graceful stop: ' + (cause as Error).message)
    }
  }

  async function openScreenRecordingSettings() {
    setPermissionSettingsError('')
    try {
      await request('/macos/open-screen-recording-settings', {})
    } catch (cause) {
      setPermissionSettingsError('Could not open Screen Recording settings: ' + (cause as Error).message)
    }
  }

  async function reveal() {
    if (!saved) return
    setError('')
    try {
      await request('/clips/reveal', { clipId: saved.id })
    } catch (cause) {
      setError('Could not reveal saved clip in Finder: ' + (cause as Error).message)
    }
  }

  async function copyPath() {
    if (!saved) return
    try { await navigator.clipboard.writeText(saved.path) }
    catch { setError('Could not copy automatically. Select the file path below to copy it.') }
  }

  const isMacProgram = mac
  const canProgram = supportedDesktop && !busy && !loading && !graphicLoading && displays.length > 0
  const peak = saved?.microphoneIncluded && typeof saved.audioPeak === 'number' ? saved.audioPeak : null
  const openDiagnostics = () => { if (!diagnostics.current?.open) diagnostics.current?.showModal() }
  return <div className="production-capture">
    <div className="production-docks" aria-label="Production docks">
      <div className="production-dock scenes-dock">{scenes}</div>
      <div className="production-dock sources-dock">
        {sources}
        <div className="dock-extra">
          <div className="dock-subtitle">Recording layers <small>Program tests</small></div>
          <label className="layer-switch"><Video size={15}/><span>Webcam inset <small>{mac ? 'Default camera · Mac MP4' : 'Mac MP4 only'}</small></span><input type="checkbox" aria-label="Webcam inset" checked={programWebcam} disabled={!mac || busy} onChange={e => setProgramWebcam(e.target.checked)}/></label>
          <label className="layer-switch"><ImageIcon size={15}/><span>PNG graphic <small>{graphicName || 'Static overlay · continuous recording'}</small></span><input type="file" aria-label="Import PNG overlay" accept="image/png" disabled={!supportedDesktop || busy || graphicLoading} onChange={e => { const file = e.target.files?.[0]; e.target.value = ''; void loadProgramGraphic(file) }}/></label>
          {graphic && <button className="text-button dock-remove" disabled={busy} onClick={() => { setGraphic(null); setGraphicName('') }}>Remove graphic</button>}
          <p className="dock-note">Webcam and graphic render into recording tests, not the still preview.</p>
        </div>
      </div>
      <section className="production-dock mixer-dock panel" aria-label="Audio Mixer">
        <div className="panel-head"><div className="panel-title"><AudioLines size={17}/>Audio Mixer</div><span className="subdued">Test inputs</span></div>
        <div className="dock-body">
          <label className="layer-switch"><Volume2 size={16}/><span>Mic / Aux <small>Default microphone</small></span><input type="checkbox" aria-label="Enable microphone" checked={programMic} disabled={!mac || busy} onChange={e => setProgramMic(e.target.checked)}/></label>
          <div className="meter-caption"><span>Last recorded peak</span><b>{peak === null ? '—' : `${Math.round(peak*100)}%`}</b></div>
          <div className="production-meter" role="meter" aria-label="Last recorded microphone peak" aria-valuemin={0} aria-valuemax={100} aria-valuenow={peak === null ? 0 : Math.round(Math.min(1,peak)*100)} aria-valuetext={peak === null ? 'No recording measured' : `${Math.round(peak*100)} percent`}>
            <span style={{width: `${peak === null ? 0 : Math.min(1,peak)*100}%`}}/>
          </div>
          <label className="gain-label">Gain <b>{Math.round(micGain*100)}%</b><input type="range" aria-label="Microphone gain" min="0" max="2" step="0.1" value={micGain} disabled={!mac || !programMic || busy} onChange={e => setMicGain(Number(e.target.value))}/></label>
          <button className="secondary-button mute-button" disabled={!mac || !programMic || busy} onClick={() => setMicGain(micGain === 0 ? 1 : 0)}>{micGain === 0 ? 'Unmute mic' : 'Mute mic'}</button>
          <div className="desktop-audio-row"><span>Desktop Audio</span><small>Not connected to Program</small></div>
          <p className="dock-note">Mic AAC is available in Mac MP4 tests. Meter reports the last capture, not a live signal.</p>
          {otherAudio}
        </div>
      </section>
      <section className="production-dock output-dock panel" aria-label="Controls">
        <div className="panel-head"><div className="panel-title"><Settings2 size={17}/>Controls</div><span className="subdued">{busy ? 'Working' : 'Local'}</span></div>
        <div className="dock-body">
          <label className="dock-field">Video profile
            <select aria-label="Continuous Program test profile" value={continuousProfile} disabled={busy} onChange={event => { const profile = event.target.value as 'baseline' | '720p15' | '1080p30'; setContinuousProfile(profile); if (profile === '1080p30' && programAviDuration > 4) setProgramAviDuration(4) }}>
              <option value="baseline">360p · 10 fps</option><option value="720p15">720p · 15 fps</option>{mac && <option value="1080p30">1080p · 30 fps (test)</option>}
            </select>
          </label>
          <label className="dock-field">Recording length
            <select aria-label="Composed Program video test duration" value={programAviDuration} disabled={busy} onChange={event => setProgramAviDuration(Number(event.target.value))}>
              {(continuousProfile === '1080p30' ? [2,4] : [2,4,6,8]).map(seconds => <option key={seconds} value={seconds}>{seconds} seconds</option>)}
            </select>
          </label>
          <button className="primary-button program-record" type="button" disabled={!canProgram} onClick={() => void captureProgramVideoTest(isMacProgram ? 'mp4' : 'continuous')}><span className="record-dot"/>{busy ? 'Capturing / finalizing…' : 'Record Program test'}</button>
          <small className="dock-note">{mac ? 'H.264 MP4 · optional AAC' : supportedDesktop ? 'Silent MJPEG AVI' : 'Requires Mac/Windows app'} · bounded diagnostic</small>
          {streamControl}
          <div className="dock-action-row"><button className="secondary-button" onClick={openClips}><Film size={14}/>Clips</button><button className="secondary-button" onClick={openSettings}><Settings2 size={14}/>Settings</button></div>
          <button className="secondary-button diagnostics-button" onClick={openDiagnostics}><Monitor size={14}/>Capture diagnostics</button>
          {saved && <button className="dock-result" onClick={openDiagnostics}><CheckCircle2 size={15}/>Saved {saved.format.toUpperCase()} · View result</button>}
          {error && <button className="dock-error" onClick={openDiagnostics} title={error}>Capture failed · View details</button>}
        </div>
      </section>
    </div>
    <dialog ref={diagnostics} className="capture-dialog" aria-labelledby="capture-diagnostics-heading">
      <div className="dialog-head"><div><h2 id="capture-diagnostics-heading">Quick screen capture — native test</h2><p>Direct-display diagnostics, comparison captures and detailed results.</p></div><button className="icon-button" aria-label="Close capture diagnostics" onClick={() => diagnostics.current?.close()}><X size={20}/></button></div>
      <div className="quick-capture">
        <p className="help-text">The direct screen test records one display. Program recordings use the selected scene and recording layers from the production docks.</p>
        {supportedDesktop && <div className="clip-actions">
          <button className="secondary-button" disabled={!canProgram} onClick={() => void captureProgramVideoTest('sampled')}>Sampled Program AVI</button>
          <button className="secondary-button" disabled={!canProgram || continuousProfile === '1080p30'} onClick={() => void captureProgramVideoTest('continuous')}>Continuous-source Program AVI</button>
        </div>}
    <div className="quick-capture-options">
      <label>Display
        <select aria-label="Screen to record" value={displayId} disabled={!desktop || loading || busy}
          onChange={event => setDisplayId(event.target.value)}>
          {displays.length ? displays.map(display =>
            <option key={display.id} value={display.id}>{display.name} · {display.width}×{display.height}</option>
          ) : <option value="">No native displays detected</option>}
        </select>
      </label>
      <label>Duration
        <select aria-label="Recording duration" value={duration} disabled={busy}
          onChange={event => setDuration(Number(event.target.value))}>
          {durations.map(seconds => <option key={seconds} value={seconds}>{seconds} seconds</option>)}
        </select>
      </label>
      <button className="secondary-button" type="button" disabled={!supportedDesktop || loading || busy}
        onClick={() => void selectProgramDisplay()} title="Select the one enabled screen source of the current Program scene; this does not composite other scene layers.">
        Use Program display
      </button>
      <span className="quick-capture-format"><Video size={15}/>{formatLabel}</span>
    </div>
    {useMp4 && <div className="quick-capture-options">
      <label className="quick-capture-check"><input type="checkbox" checked={systemAudio} disabled={busy}
        onChange={event => setSystemAudio(event.target.checked)}/> Request system audio</label>
      <label className="quick-capture-check"><input type="checkbox" checked={microphone} disabled={busy || !micSupported}
        onChange={event => setMicrophone(event.target.checked)}/> Request microphone audio {micSupported ? '' : '(macOS 15+ only)'}</label>
      <small className="help-text">Audio availability and synchronization are unverified. Permissions may be required.</small>
    </div>}
    <div className="clip-actions">
      <button className="primary-button" type="button" disabled={!canRecord} onClick={() => void capture()}>
        <Monitor size={16}/>{busy ? 'Recording and finalizing…' : 'Capture screen test'}
      </button>
      {useMp4 && busy && captureMode === 'screen' && <button className="secondary-button" type="button" disabled={stopRequested}
        onClick={() => void stopCapture()}><Square size={15}/>{stopRequested ? 'Stopping and finalizing…' : 'Stop and save MP4'}</button>}
      <button className="secondary-button" type="button" onClick={openClips}><Film size={16}/>Open Clips</button>
    </div>
    {desktop && <div className="clip-inventory-status">
      <span>{persistedClips === null ? 'Saved clip inventory not checked'
        : `${persistedClips.length} saved clip(s) found in MooBee's local storage`}</span>
      {saved && persistedClips && <span>{persistedClips.some(clip => clip.id === saved.id)
        ? 'Latest clip is present in local storage'
        : 'Latest clip is not in the saved clip inventory; refresh or open Clips'}</span>}
      <button className="secondary-button" type="button" disabled={busy} onClick={() => void refreshSavedClips()}>
        Refresh saved clips
      </button>
      {inventoryError && <small role="alert">{inventoryError}</small>}
    </div>}
    {!supportedDesktop && <p className="help-text">Native screen capture requires the Mac/Windows desktop application. Android and browser editions cannot record local desktop screens.</p>}
    {mac && !mp4Supported && desktop && <p className="help-text">
      {mp4AvailabilityReason} · Silent AVI capture remains available as a fallback.
    </p>}
    {error && isScreenCapturePermissionDenied(error) && <div className="capture-permission-help" role="alert">
      <strong>macOS blocked screen recording permission</strong>
      <p>Quit MooBee and enable it in System Settings → Privacy &amp; Security → Screen Recording.
        Then reopen MooBee from Applications and retry without audio. Only macOS can grant access.</p>
      <button type="button" className="secondary-button" onClick={() => void openScreenRecordingSettings()}>
        Open Screen Recording Settings
      </button>
      <small>If it is already enabled, toggle it off and back on, quit MooBee completely and retry.</small>
      {permissionSettingsError && <small role="status">{permissionSettingsError}</small>}
    </div>}
    {error && <p className="clip-error" role="alert">{error}</p>}
    {saved && <div className="clip-result" role="status">
      <CheckCircle2 size={18}/>
      <div>
        <strong>Saved {saved.format.toUpperCase()} locally — playback{saved.format === 'mp4' && (saved.displayCount === undefined || saved.microphoneIncluded) ? ' and audio' : ''} not yet verified</strong>
        <small>
          {saved.durationSeconds !== null && Number.isFinite(saved.durationSeconds)
            ? saved.displayCount !== undefined && saved.format === 'avi'
              ? `${saved.durationSeconds.toFixed(2)}s nominal AVI playback timeline (not independently verified)`
              : `${saved.durationSeconds.toFixed(2)}s recorded (container metadata)`
            : 'Recorded duration not available from metadata'}
          {' · '}
          {saved.requestedMaxSeconds ?? duration}s maximum requested
          {' · '}
          {(saved.bytes / (1024 * 1024)).toFixed(2)} MB
        </small>
        {saved.displayCount !== undefined && <p className="help-text" role="status">
          {saved.displayCount} Program display source(s) composed · {saved.width ?? 640}×{saved.height ?? 360} · nominal {saved.fps ?? 10} fps {saved.format.toUpperCase()} ·
          {typeof saved.measuredSamplingFps === 'number' ? ` ${saved.measuredSamplingFps.toFixed(1)} fps measured sampling` : ' capture performance unknown'} ·
          {saved.elapsedMs !== undefined ? ` ${(saved.elapsedMs / 1000).toFixed(2)}s wall-clock capture` : ' elapsed time unknown'}.
          The nominal output clock can differ from wall-clock capture when frames arrive late.
        </p>}
        {saved.overlayIncluded && <p className="help-text">Static graphic overlay included in output.</p>}
        {saved.microphoneIncluded && <p className="help-text" role="status">AAC requested · {saved.audioSamplesWritten ?? 0} PCM frames supplied at 48 kHz · peak {Math.round((saved.audioPeak ?? 0)*100)}% · {saved.audioClippedSamples ?? 0} clipped samples. Confirm the AAC track and audible audio independently; capture timestamps and drift are not verified.</p>}
        {saved.webcamIncluded && <p className="help-text" role="status">Webcam inset included · {saved.webcamFrames ?? 0} received frames · {saved.webcamRepeatedSlots ?? 0} repeated slots. Verify the actual clip.</p>}
        {saved.sourceFramesReceived && <p className="help-text" role="status">
          Continuous native frame input · received per source: {saved.sourceFramesReceived.join(', ')} ·
          repeated input slots: {saved.repeatedSourceSlots?.join(', ') ?? 'unknown'} ·
          late composite frames: {saved.lateOutputFrames ?? 'unknown'} ·
          max source arrival skew: {saved.maxSourceArrivalSkewMs ?? 'unknown'}ms.
          {saved.finalizationMs !== undefined ? `Container finalization: ${saved.finalizationMs} ms. ` : ''}
          Arrival-time skew is not native capture timestamp synchronization.
        </p>}
        <code>{saved.path}</code>
        <div className="clip-actions">
          {mac && <button type="button" className="secondary-button" onClick={() => void reveal()}>
            <FolderOpen size={15}/>Show in Finder
          </button>}
          <button type="button" className="secondary-button" onClick={() => void copyPath()}>
            <Copy size={15}/>Copy path
          </button>
          <button type="button" className="secondary-button" onClick={openClips}>
            <Film size={15}/>View Clips
          </button>
        </div>
        <p className="help-text">{saved.displayCount !== undefined
          ? 'This video is an experimental Program composite, not live Program output. Open it in QuickTime (MP4) or VLC; check that all screens appear, verify motion, duration and seeking, and compare playback timeline with actual wall-clock capture.'
          : 'A recording file can be finalized without verifying playback or audible audio. Open it in QuickTime or VLC to confirm playback and seeking. For a Stop-and-Save test, compare the actual duration above with when you pressed Stop. Reopen MooBee and check that the clip remains in Clips.'}</p>
      </div>
    </div>}
      </div>
    </dialog>
  </div>
}
