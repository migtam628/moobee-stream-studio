import { useEffect, useRef, useState } from 'react'
import { CheckCircle2, Download, MonitorPlay, Square, Video } from 'lucide-react'
import './BrowserRecorder.css'

type SavedRecording = {
  name: string
  url: string
  mimeType: string
  bytes: number
  seconds: number
}

const MAX_RECORDING_BYTES = 64 * 1024 * 1024
const MIME_TYPES = [
  'video/webm;codecs=vp9',
  'video/webm;codecs=vp8',
  'video/webm',
  'video/mp4'
]

function browserSupportsRecording() {
  return typeof window !== 'undefined' && !!navigator.mediaDevices?.getDisplayMedia &&
    typeof MediaRecorder !== 'undefined'
}

/**
 * Local-only browser capture, no backend or streaming service involved.
 * Requires a secure context, explicit user gesture, and browser screen-share permission.
 */
export default function BrowserRecorder() {
  const [supported] = useState(browserSupportsRecording)
  const [duration, setDuration] = useState(10)
  const [isRecording, setIsRecording] = useState(false)
  const [error, setError] = useState('')
  const [clip, setClip] = useState<SavedRecording | null>(null)
  const [elapsed, setElapsed] = useState(0)

  const recorderRef = useRef<MediaRecorder | null>(null)
  const streamRef = useRef<MediaStream | null>(null)
  const stopTimerRef = useRef<number | null>(null)
  const clockRef = useRef<number | null>(null)
  const urlRef = useRef<string | null>(null)

  const clearTimers = () => {
    if (stopTimerRef.current !== null) window.clearTimeout(stopTimerRef.current)
    if (clockRef.current !== null) window.clearInterval(clockRef.current)
    stopTimerRef.current = null
    clockRef.current = null
  }
  const releaseStream = () => {
    streamRef.current?.getTracks().forEach(track => track.stop())
    streamRef.current = null
  }

  useEffect(() => () => {
    clearTimers()
    const recorder = recorderRef.current
    if (recorder && recorder.state !== 'inactive') recorder.stop()
    releaseStream()
    if (urlRef.current) URL.revokeObjectURL(urlRef.current)
  }, [])

  function stopRecording() {
    clearTimers()
    const recorder = recorderRef.current
    if (recorder && recorder.state !== 'inactive') recorder.stop()
    else releaseStream()
  }

  async function startRecording() {
    // getDisplayMedia must be the first awaited operation in this user-click handler.
    if (!supported || isRecording) return
    setError('')
    setElapsed(0)
    let stream: MediaStream
    try {
      stream = await navigator.mediaDevices.getDisplayMedia({
        video: { frameRate: { ideal: 30, max: 30 } },
        audio: false
      })
    } catch (cause) {
      setError((cause as Error).name === 'NotAllowedError'
        ? 'Screen sharing was cancelled or denied.'
        : `Could not start screen sharing: ${(cause as Error).message}`)
      return
    }
    streamRef.current = stream
    try {
      const mime = MIME_TYPES.find(type => MediaRecorder.isTypeSupported(type))
      const recorder = new MediaRecorder(stream, {
        ...(mime ? { mimeType: mime } : {}),
        videoBitsPerSecond: 2_500_000
      })
      const chunks: Blob[] = []
      let total = 0
      let overLimit = false
      const started = performance.now()
      const recordingDuration = duration
      recorderRef.current = recorder

      recorder.ondataavailable = event => {
        if (event.data.size) {
          total += event.data.size
          if (total > MAX_RECORDING_BYTES) {
            overLimit = true
            stopRecording()
          } else {
            chunks.push(event.data)
          }
        }
      }
      recorder.onerror = () => {
        setError('Browser recording failed. Please retry with a different shared window or screen.')
        stopRecording()
      }
      recorder.onstop = () => {
        clearTimers()
        releaseStream()
        recorderRef.current = null
        setIsRecording(false)
        setElapsed(0)
        if (overLimit) {
          setError('The recording exceeded the 64 MB local memory limit. Choose a shorter duration.')
          return
        }
        if (!chunks.length) {
          setError('The browser did not return any video data.')
          return
        }
        const mimeType = recorder.mimeType || chunks[0].type || 'video/webm'
        const blob = new Blob(chunks, { type: mimeType })
        const url = URL.createObjectURL(blob)
        if (urlRef.current) URL.revokeObjectURL(urlRef.current)
        urlRef.current = url
        const extension = mimeType.includes('mp4') ? 'mp4' : 'webm'
        const name = `moobee-browser-capture-${new Date().toISOString().replace(/[:.]/g, '-') }.${extension}`
        setClip({
          name, url, mimeType, bytes: blob.size,
          seconds: Math.min(recordingDuration, Math.max(1, Math.round((performance.now() - started) / 1000)))
        })
      }
      stream.getVideoTracks()[0]?.addEventListener('ended', () => stopRecording(), { once: true })
      recorder.start(1000)
      setIsRecording(true)
      stopTimerRef.current = window.setTimeout(stopRecording, recordingDuration * 1000)
      clockRef.current = window.setInterval(() => {
        setElapsed(Math.min(recordingDuration, Math.floor((performance.now() - started) / 1000)))
      }, 250)
    } catch (cause) {
      setIsRecording(false)
      recorderRef.current = null
      releaseStream()
      setError(`Your browser could not create a video recorder: ${(cause as Error).message}`)
    }
  }

  return <section className="browser-recorder">
    <div className="browser-recorder-head"><MonitorPlay size={19}/><div><h2>Browser screen recording</h2><p>Save a silent WebM or MP4 test recording directly on this device. No upload, account, or streaming server.</p></div></div>
    <div className="browser-recorder-form">
      <label>Maximum duration
        <select value={duration} disabled={!supported || isRecording} onChange={event => setDuration(Number(event.target.value))}>
          {[5, 10, 30].map(seconds => <option key={seconds} value={seconds}>{seconds} seconds</option>)}
        </select>
      </label>
      <div className="browser-recorder-actions">
        {!isRecording ? <button type="button" onClick={() => void startRecording()} disabled={!supported}><Video size={17}/>Record shared screen</button>
          : <button type="button" className="stop" onClick={stopRecording}><Square size={16}/>Stop · {elapsed}s</button>}
        <small>{supported ? 'Your browser asks you what to share first.' : 'Screen recording is not available in this browser or device. Use a supported desktop browser over HTTPS.'}</small>
      </div>
      {error && <p className="browser-recorder-error" role="alert">{error}</p>}
      {clip && <div className="browser-recorder-result">
        <div className="browser-recorder-saved"><CheckCircle2 size={18}/><div><strong>Local recording ready</strong><small>{clip.seconds}s · {(clip.bytes / (1024 * 1024)).toFixed(2)} MB · {clip.mimeType}</small></div></div>
        <video src={clip.url} playsInline controls aria-label="Preview browser screen recording"/>
        <a href={clip.url} download={clip.name} className="browser-recorder-download"><Download size={17}/>Save recording</a>
      </div>}
    </div>
    <p className="browser-recorder-note">This is an experimental browser recording tool. It only captures the window, tab, or screen you approve. It does not mix microphone audio, render MooBee scenes, or stream to Twitch/YouTube.</p>
  </section>
}
