import { useEffect, useState } from 'react'
import { CheckCircle2, Copy, Film, FolderOpen, Monitor, Video } from 'lucide-react'
import { request, runningInTauri } from './api'

type Display = { id: number; name: string; width: number; height: number }
type Availability = { supported: boolean; requiredMacos: string }
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
  const [displays, setDisplays] = useState<Display[]>([])
  const [displayId, setDisplayId] = useState('')
  const [mp4Supported, setMp4Supported] = useState(false)
  const [duration, setDuration] = useState(6)
  const [systemAudio, setSystemAudio] = useState(false)
  const [microphone, setMicrophone] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [saved, setSaved] = useState<CapturedFile | null>(null)

  useEffect(() => {
    if (!desktop) return
    let active = true
    setLoading(true)
    Promise.all([
      request<{ displays: Display[] }>('/capture/displays'),
      mac
        ? request<Availability>('/clips/macos-mp4-availability')
        : Promise.resolve({ supported: false, requiredMacos: '15.0' })
    ]).then(([list, availability]) => {
      if (!active) return
      setDisplays(list.displays)
      setDisplayId(current =>
        list.displays.some(display => String(display.id) === current)
          ? current : String(list.displays[0]?.id ?? ''))
      setMp4Supported(Boolean(availability.supported))
      setError('')
    }).catch(cause => {
      if (active) setError('Could not enumerate capture devices: ' + (cause as Error).message)
    }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [desktop, mac])

  const useMp4 = mac && mp4Supported
  const canRecord = desktop && !!displayId && !loading && !busy
  const formatLabel = useMp4 ? 'H.264 MP4 (macOS 15+)' : 'silent MJPEG AVI (10 fps)'
  async function capture() {
    if (!canRecord || ![4, 6, 8].includes(duration)) return
    setBusy(true)
    setSaved(null)
    setError('')
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
      Save a <strong>4–8 second test recording of one display</strong> using MooBee's own native capture.
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
          {[4, 6, 8].map(seconds => <option key={seconds} value={seconds}>{seconds} seconds</option>)}
        </select>
      </label>
      <span className="quick-capture-format"><Video size={15}/>{formatLabel}</span>
    </div>
    {useMp4 && <div className="quick-capture-options">
      <label className="quick-capture-check"><input type="checkbox" checked={systemAudio} disabled={busy}
        onChange={event => setSystemAudio(event.target.checked)}/> Request system audio</label>
      <label className="quick-capture-check"><input type="checkbox" checked={microphone} disabled={busy}
        onChange={event => setMicrophone(event.target.checked)}/> Request microphone audio</label>
      <small className="help-text">Audio availability and synchronization are unverified. Permissions may be required.</small>
    </div>}
    <div className="clip-actions">
      <button className="primary-button" type="button" disabled={!canRecord} onClick={() => void capture()}>
        <Monitor size={16}/>{busy ? 'Recording and finalizing…' : 'Capture screen test'}
      </button>
      <button className="secondary-button" type="button" onClick={openClips}><Film size={16}/>Open Clips</button>
    </div>
    {!desktop && <p className="help-text">Install the Mac/Windows desktop application to use native capture.</p>}
    {mac && !mp4Supported && desktop && <p className="help-text">
      Native MP4 requires macOS 15+. Silent AVI capture is available as a fallback.
    </p>}
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
