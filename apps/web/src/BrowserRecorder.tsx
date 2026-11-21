import { useEffect, useRef, useState } from 'react'
import { Camera, CheckCircle2, Download, Mic, MonitorPlay, Square, Video } from 'lucide-react'
import './BrowserRecorder.css'

type SavedRecording = { name: string; url: string; mimeType: string; bytes: number; seconds: number }
type ActiveSession = {
  recorder: MediaRecorder
  streams: MediaStream[]
  encodedStream: MediaStream
  audioContext: AudioContext | null
  animationFrame: number
  screenVideo: HTMLVideoElement
  cameraVideo: HTMLVideoElement | null
}

const MAX_RECORDING_BYTES = 64 * 1024 * 1024
const VIDEO_WIDTH = 1280
const VIDEO_HEIGHT = 720
const VIDEO_FPS = 30
const MIME_TYPES = [
  'video/webm;codecs=vp9,opus',
  'video/webm;codecs=vp8,opus',
  'video/webm;codecs=vp9',
  'video/webm;codecs=vp8',
  'video/webm',
  'video/mp4'
]

function supportedMedia() {
  return typeof navigator !== 'undefined' &&
    typeof navigator.mediaDevices?.getDisplayMedia === 'function' &&
    typeof MediaRecorder !== 'undefined' &&
    typeof HTMLCanvasElement !== 'undefined' &&
    typeof HTMLCanvasElement.prototype.captureStream === 'function'
}

function attachMutedVideo(stream: MediaStream) {
  const video = document.createElement('video')
  video.muted = true
  video.playsInline = true
  video.srcObject = stream
  // Playback is needed to make the current MediaStream frame available to canvas.
  // Do not await this promise: some browsers wait until the first frame arrives.
  void video.play().catch(() => {})
  return video
}

function drawContained(ctx: CanvasRenderingContext2D, video: HTMLVideoElement) {
  if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth || !video.videoHeight) return
  const factor = Math.min(VIDEO_WIDTH / video.videoWidth, VIDEO_HEIGHT / video.videoHeight)
  const width = video.videoWidth * factor
  const height = video.videoHeight * factor
  ctx.drawImage(video, (VIDEO_WIDTH - width) / 2, (VIDEO_HEIGHT - height) / 2, width, height)
}

function drawCamera(ctx: CanvasRenderingContext2D, video: HTMLVideoElement, position: 'left' | 'right') {
  if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA || !video.videoWidth || !video.videoHeight) return
  const width = 300, height = 170
  const x = position === 'left' ? 20 : VIDEO_WIDTH - width - 20
  const y = VIDEO_HEIGHT - height - 20
  ctx.fillStyle = '#0d0c13'
  ctx.fillRect(x - 4, y - 4, width + 8, height + 8)
  // Crop a portrait webcam into a 16:9 picture-in-picture region.
  const crop = Math.min(video.videoWidth / width, video.videoHeight / height)
  const sourceWidth = width * crop, sourceHeight = height * crop
  ctx.drawImage(video, (video.videoWidth - sourceWidth) / 2, (video.videoHeight - sourceHeight) / 2,
    sourceWidth, sourceHeight, x, y, width, height)
  ctx.strokeStyle = '#bb9cff'
  ctx.lineWidth = 3
  ctx.strokeRect(x, y, width, height)
}

async function mixAudio(streams: MediaStream[]): Promise<{
  context: AudioContext | null
  tracks: MediaStreamTrack[]
}> {
  const withAudio = streams.filter(stream => stream.getAudioTracks().length > 0)
  if (!withAudio.length) return { context: null, tracks: [] }
  if (typeof AudioContext === 'undefined') throw new Error('This browser cannot mix microphone or shared-screen audio.')
  const context = new AudioContext()
  try {
    const output = context.createMediaStreamDestination()
    for (const stream of withAudio) {
      const audioOnly = new MediaStream(stream.getAudioTracks())
      const source = context.createMediaStreamSource(audioOnly)
      const gain = context.createGain()
      gain.gain.value = 1
      source.connect(gain)
      gain.connect(output)
    }
    if (context.state === 'suspended') await context.resume()
    return { context, tracks: output.stream.getAudioTracks() }
  } catch (error) {
    await context.close().catch(() => {})
    throw error
  }
}

export default function BrowserRecorder() {
  const [supported] = useState(supportedMedia)
  const [duration, setDuration] = useState(10)
  const [cameraEnabled, setCameraEnabled] = useState(false)
  const [micEnabled, setMicEnabled] = useState(false)
  const [sharedAudio, setSharedAudio] = useState(false)
  const [cameraPosition, setCameraPosition] = useState<'left' | 'right'>('right')
  const [isRecording, setIsRecording] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [clip, setClip] = useState<SavedRecording | null>(null)
  const [elapsed, setElapsed] = useState(0)

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const sessionRef = useRef<ActiveSession | null>(null)
  const timerRef = useRef<number | null>(null)
  const clockRef = useRef<number | null>(null)
  const urlRef = useRef<string | null>(null)
  const mountedRef = useRef(true)
  const startingRef = useRef(false)
  const [preparing, setPreparing] = useState(false)

  function clearTimers() {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current)
    if (clockRef.current !== null) window.clearInterval(clockRef.current)
    timerRef.current = null
    clockRef.current = null
  }

  function cleanup(session: ActiveSession) {
    window.cancelAnimationFrame(session.animationFrame)
    session.streams.forEach(stream => stream.getTracks().forEach(track => track.stop()))
    session.encodedStream.getTracks().forEach(track => track.stop())
    session.screenVideo.srcObject = null
    if (session.cameraVideo) session.cameraVideo.srcObject = null
    if (session.audioContext) void session.audioContext.close().catch(() => {})
    if (sessionRef.current === session) sessionRef.current = null
  }

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      clearTimers()
      const session = sessionRef.current
      if (session) {
        session.recorder.onstop = null
        if (session.recorder.state !== 'inactive') session.recorder.stop()
        cleanup(session)
      }
      if (urlRef.current) URL.revokeObjectURL(urlRef.current)
    }
  }, [])

  function stopRecording() {
    clearTimers()
    const session = sessionRef.current
    if (session && session.recorder.state !== 'inactive') session.recorder.stop()
  }

  async function startRecording() {
    if (!supported || sessionRef.current || startingRef.current || isRecording || !canvasRef.current) return
    startingRef.current = true
    setPreparing(true)
    setError('')
    setNotice('')
    setElapsed(0)
    const streams: MediaStream[] = []
    let context: AudioContext | null = null
    let encodedStream: MediaStream | null = null
    let screenVideo: HTMLVideoElement | null = null
    let cameraVideo: HTMLVideoElement | null = null

    try {
      // Must be requested directly from a click, before any other awaited permission request.
      const screen = await navigator.mediaDevices.getDisplayMedia({
        video: { frameRate: { ideal: VIDEO_FPS, max: VIDEO_FPS } },
        audio: sharedAudio
      })
      streams.push(screen)
      if (!mountedRef.current) throw new Error('The recording workspace was closed.')
      if (!screen.getVideoTracks().length) throw new Error('Screen sharing returned no video track.')

      if (cameraEnabled || micEnabled) {
        if (!navigator.mediaDevices.getUserMedia) throw new Error('Camera and microphone capture are unavailable.')
        const devices = await navigator.mediaDevices.getUserMedia({
          video: cameraEnabled ? { width: { ideal: 640 }, height: { ideal: 360 } } : false,
          audio: micEnabled
        })
        streams.push(devices)
        if (!mountedRef.current) throw new Error('The recording workspace was closed.')
        if (cameraEnabled && !devices.getVideoTracks().length) throw new Error('No webcam track was received.')
        if (micEnabled && !devices.getAudioTracks().length) throw new Error('No microphone track was received.')
        if (cameraEnabled) cameraVideo = attachMutedVideo(devices)
      }

      screenVideo = attachMutedVideo(screen)
      const canvas = canvasRef.current
      if (!canvas) throw new Error('The recording canvas is unavailable.')
      canvas.width = VIDEO_WIDTH
      canvas.height = VIDEO_HEIGHT
      const ctx = canvas.getContext('2d', { alpha: false })
      if (!ctx) throw new Error('Canvas composition is unavailable in this browser.')

      // Browser-native compositor: screen + webcam PiP, rendered to a canvas video track.
      ctx.fillStyle = '#15121f'
      ctx.fillRect(0, 0, VIDEO_WIDTH, VIDEO_HEIGHT)
      const canvasStream = canvas.captureStream(VIDEO_FPS)
      const audio = await mixAudio(streams)
      context = audio.context
      if (!mountedRef.current) throw new Error('The recording workspace was closed.')
      encodedStream = new MediaStream([...canvasStream.getVideoTracks(), ...audio.tracks])

      const mime = MIME_TYPES.find(type => MediaRecorder.isTypeSupported(type))
      const recorder = new MediaRecorder(encodedStream, {
        ...(mime ? { mimeType: mime } : {}),
        videoBitsPerSecond: 3_000_000,
        ...(audio.tracks.length ? { audioBitsPerSecond: 128_000 } : {})
      })

      const session: ActiveSession = {
        recorder, streams, encodedStream, audioContext: context, animationFrame: 0,
        screenVideo, cameraVideo
      }
      sessionRef.current = session
      let lastDraw = 0
      const paint = (now: number) => {
        if (now - lastDraw >= 1000 / VIDEO_FPS - 1) {
          lastDraw = now
          ctx.fillStyle = '#15121f'
          ctx.fillRect(0, 0, VIDEO_WIDTH, VIDEO_HEIGHT)
          drawContained(ctx, screenVideo!)
          if (cameraVideo) drawCamera(ctx, cameraVideo, cameraPosition)
        }
        session.animationFrame = window.requestAnimationFrame(paint)
      }
      session.animationFrame = window.requestAnimationFrame(paint)

      const chunks: Blob[] = []
      let total = 0
      let failed = false
      const started = performance.now()
      const limit = duration
      recorder.ondataavailable = event => {
        if (!event.data.size) return
        total += event.data.size
        if (total > MAX_RECORDING_BYTES) {
          failed = true
          if (mountedRef.current) setError('Recording exceeded the 64 MB memory limit. Choose a shorter clip.')
          stopRecording()
        } else chunks.push(event.data)
      }
      recorder.onerror = () => {
        failed = true
        if (mountedRef.current) setError('The browser video encoder failed.')
        stopRecording()
      }
      recorder.onstop = () => {
        clearTimers()
        cleanup(session)
        if (!mountedRef.current) return
        setIsRecording(false)
        setElapsed(0)
        if (failed) return
        if (!chunks.length) {
          setError('The browser did not produce any video data.')
          return
        }
        const mimeType = recorder.mimeType || chunks[0].type || 'video/webm'
        const blob = new Blob(chunks, { type: mimeType })
        const url = URL.createObjectURL(blob)
        if (urlRef.current) URL.revokeObjectURL(urlRef.current)
        urlRef.current = url
        setClip({
          name: 'moobee-composed-' + new Date().toISOString().replace(/[:.]/g, '-') +
            (mimeType.includes('mp4') ? '.mp4' : '.webm'),
          url, mimeType, bytes: blob.size,
          seconds: Math.min(limit, Math.max(1, Math.round((performance.now() - started) / 1000)))
        })
      }
      screen.getVideoTracks()[0].addEventListener('ended', stopRecording, { once: true })
      recorder.start(1000)
      startingRef.current = false
      setPreparing(false)
      setIsRecording(true)
      if (sharedAudio && !screen.getAudioTracks().length) {
        setNotice('The browser provided no shared-screen audio. The video will use microphone audio if enabled.')
      }
      timerRef.current = window.setTimeout(stopRecording, duration * 1000)
      clockRef.current = window.setInterval(() => setElapsed(Math.min(limit,
        Math.floor((performance.now() - started) / 1000))), 250)
    } catch (cause) {
      clearTimers()
      const session = sessionRef.current
      if (session) cleanup(session)
      else {
        streams.forEach(stream => stream.getTracks().forEach(track => track.stop()))
        encodedStream?.getTracks().forEach(track => track.stop())
        if (context) void context.close().catch(() => {})
        if (screenVideo) screenVideo.srcObject = null
        if (cameraVideo) cameraVideo.srcObject = null
      }
      startingRef.current = false
      if (!mountedRef.current) return
      setPreparing(false)
      setIsRecording(false)
      const err = cause as Error
      setError(err.name === 'NotAllowedError' ? 'Screen, camera, or microphone permission was denied.'
        : 'Could not start composed recording: ' + err.message)
    }
  }

  return <section className="browser-recorder">
    <div className="browser-recorder-head">
      <MonitorPlay size={20}/>
      <div><h2>Browser screen recording</h2>
        <p>Combine a shared screen, optional webcam picture-in-picture, and optional microphone into one local video file.</p>
      </div>
    </div>
    <div className="browser-recorder-form">
      <div className="browser-recorder-options">
        <label>Maximum duration
          <select value={duration} disabled={!supported || isRecording || preparing} onChange={e => setDuration(Number(e.target.value))}>
            {[5, 10, 30].map(seconds => <option key={seconds} value={seconds}>{seconds} seconds</option>)}
          </select>
        </label>
        <label>Camera position
          <select value={cameraPosition} disabled={!supported || isRecording || preparing || !cameraEnabled}
            onChange={e => setCameraPosition(e.target.value as 'left' | 'right')}>
            <option value="right">Bottom right</option><option value="left">Bottom left</option>
          </select>
        </label>
      </div>
      <div className="browser-recorder-toggles">
        <label><input type="checkbox" checked={cameraEnabled} disabled={!supported || isRecording}
          onChange={e => setCameraEnabled(e.target.checked)}/><Camera size={16}/> Webcam picture-in-picture</label>
        <label><input type="checkbox" checked={micEnabled} disabled={!supported || isRecording}
          onChange={e => setMicEnabled(e.target.checked)}/><Mic size={16}/> Microphone audio</label>
        <label><input type="checkbox" checked={sharedAudio} disabled={!supported || isRecording}
          onChange={e => setSharedAudio(e.target.checked)}/><Video size={16}/> Shared-tab/system audio (if available)</label>
      </div>
      <div className="browser-composition">
        <canvas ref={canvasRef} width={VIDEO_WIDTH} height={VIDEO_HEIGHT}
          aria-label="Live composited screen and camera preview"/>
        {!isRecording && <div className="browser-composition-empty">
          <MonitorPlay size={30}/><strong>Composited preview</strong>
          <span>A live 1280×720 preview appears after you approve screen sharing.</span>
        </div>}
        {isRecording && <span className="browser-composition-live">REC · {elapsed}s</span>}
      </div>
      <div className="browser-recorder-actions">
        {!isRecording ? <button type="button" onClick={() => void startRecording()} disabled={!supported || preparing}>
          <Video size={17}/>{preparing ? 'Requesting permissions…' : 'Record shared screen'}</button>
          : <button type="button" className="stop" onClick={stopRecording}><Square size={16}/>Stop · {elapsed}s</button>}
        <small>{supported ? 'Your browser asks permission to share your screen and any selected devices.'
          : 'This browser does not support screen sharing, canvas capture, and MediaRecorder together. Try a compatible desktop browser over HTTPS.'}</small>
      </div>
      {notice && <p className="browser-recorder-notice" role="status">{notice}</p>}
      {error && <p className="browser-recorder-error" role="alert">{error}</p>}
      {clip && <div className="browser-recorder-result">
        <div className="browser-recorder-saved"><CheckCircle2 size={18}/><div>
          <strong>Local recording ready</strong>
          <small>{clip.seconds}s · {(clip.bytes / (1024 * 1024)).toFixed(2)} MB · {clip.mimeType}</small>
        </div></div>
        <video src={clip.url} playsInline controls aria-label="Preview browser screen recording"/>
        <a href={clip.url} download={clip.name} className="browser-recorder-download"><Download size={17}/>Save recording</a>
      </div>}
    </div>
    <p className="browser-recorder-note">Experimental local 720p canvas composition at up to 30 fps, dependent on your computer and browser. Webcam/microphone permissions are optional. Files stay on this device. This does not render your saved MooBee scenes, provide a replay buffer, or stream live to any platform.</p>
  </section>
}
