import { useEffect, useState } from 'react'
import { CheckCircle2, Copy, Film, FolderOpen, Monitor, Square, Video } from 'lucide-react'
import { request, runningInTauri } from './api'
import { isScreenCapturePermissionDenied } from './capturePermission'

type Display = { id: number; name: string; width: number; height: number }
type Availability = { supported: boolean; requiredMacos: string; osVersion?: string | null; reason?: string; microphoneSupported?: boolean; engine?: string }
type CapturedFile = {
  id: string
  path: string
  bytes: number
  format: string
  durationSeconds: number
  audioTrackVerified?: boolean
}

/**
 * One explicit bounded native DISPLAY recording, not MooBee's program scene.
 * No synthetic status flags or fake broadcast controls are changed here.
 * The full Record/Go Live outputs remain gated on native A/V/RTMP acceptance.
 */
export default function QuickCapture({ openClips }: { openClips: () => void }) {
  const desktop = runningInTauri()
  const mac = desktop && /Macintosh|Mac OS X/i.test(navigator.userAgent)
  const supportedDesktop = desktop && (mac || /Windows/i.test(navigator.userAgent))
  const [displays, setDisplays] = useState<Display[]>([])
  const [displayId, setDisplayId] = useState('')
  const [mp4Supported, setMp4Supported] = useState(false)
  const [micSupported, setMicSupported] = useState(false)
  const [mp4AvailabilityReason, setMp4AvailabilityReason] = useState('Native MP4 availability has not been checked')
  const [duration, setDuration] = useState(6)
  const [systemAudio, setSystemAudio] = useState(false)
  const [microphone, setMicrophone] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [stopRequested, setStopRequested] = useState(false)
  const [error, setError] = useState('')
  const [permissionSettingsError, setPermissionSettingsError] = useState('')
  const [saved, setSaved] = useState<CapturedFile | null>(null)

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

  const useMp4 = mac && mp4Supported
  const canRecord = supportedDesktop && !!displayId && !loading && !busy
  const durations = useMp4 ? [4, 6, 8, 15, 30, 60, 120] : [4, 6, 8]
  const formatLabel = useMp4 ? 'H.264 MP4 (macOS 14+ experimental)' : 'silent MJPEG AVI (10 fps)'
  async function capture() {
    if (!canRecord || !durations.includes(duration)) return
    setBusy(true)
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
    } catch (cause) {
      setError('Native recording failed: ' + (cause as Error).message)
    } finally {
      setBusy(false)
      setStopRequested(false)
    }
  }

  async function stopCapture() {
    if (!useMp4 || !busy || stopRequested) return
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

  return <div className="quick-capture">
    <p className="help-text">
      Save a <strong>bounded test recording of one display</strong> using MooBee's native capture. On supported Macs, request up to 120 seconds and stop early with finalization.
      This is not the composited Program feed and does not include saved scene overlays or webcam layers.
    </p>
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
      {useMp4 && busy && <button className="secondary-button" type="button" disabled={stopRequested}
        onClick={() => void stopCapture()}><Square size={15}/>{stopRequested ? 'Stopping and finalizing…' : 'Stop and save MP4'}</button>}
      <button className="secondary-button" type="button" onClick={openClips}><Film size={16}/>Open Clips</button>
    </div>
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
        <strong>Saved {saved.format.toUpperCase()} locally — playback{useMp4 ? ' and audio' : ''} not yet verified</strong>
        <small>{saved.durationSeconds}s requested · {(saved.bytes / (1024 * 1024)).toFixed(2)} MB</small>
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
        <p className="help-text">Open the saved file in QuickTime or VLC to confirm image, duration and sound.</p>
      </div>
    </div>}
  </div>
}
