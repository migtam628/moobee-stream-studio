import { useEffect, useRef, useState, type ReactNode } from 'react'
import { CheckCircle2, Copy, Film, FolderOpen, Monitor, Square, Video, AudioLines, Settings2, X, Volume2, Image as ImageIcon } from 'lucide-react'
import { request, runningInTauri } from './api'
import LiveAcceptance from './LiveAcceptance'
import { isScreenCapturePermissionDenied } from './capturePermission'
import { singleProgramDisplayId, programTestDisplayIds, programTestLayout, programTransforms, programVideoInput, type MinimalSceneCollection, validateSceneLayout } from './programDisplay'

type AudioDevice = { id:string; name:string; isDefault:boolean }
type AudioInput = { kind:'device'|'desktop'; deviceId:string|null; gain:number; muted:boolean }
type AudioLevels = { inputPeaks:number[]; mixedPeak:number; clippedSamples:number }
type Display = { id: number; name: string; width: number; height: number }
type Availability = { supported: boolean; requiredMacos: string; osVersion?: string | null; reason?: string; microphoneSupported?: boolean; engine?: string }
type OverlayPlacement = { xPercent: number; yPercent: number; widthPercent: number }
type GraphicOverlay = { width: number; height: number; rgbaBase64: string }
type ProgramStats = { phase:'starting'|'recording'|'paused'|'finalizing'; recordedFrames:number; fps:number; elapsedMs:number; pausedMs:number; fileBytes:number; lateFrames:number; repeatedSourceSlots:number; transitionBusy?:boolean; transitionsCompleted?:number; audioPeak?:number; audioMissingSourceFrames?:number; segmentIndex?:number; segmentCount?:number; completedSegments?:number }
type CapturedFile = {
  id: string
  path: string
  bytes: number
  format: string
  durationSeconds: number | null
  requestedMaxSeconds?: number
  stoppedEarly?: boolean
  pausedMs?: number
  targetBitrateKbps?: number
  transitionsCompleted?: number
  segmentedSession?: {requestedSeconds:number; completedSeconds:number; totalBytes:number; stoppedEarly:boolean; warning?:string|null; segments:{id:string;path:string;bytes:number;durationSeconds:number;frameCount:number}[]}
  audioTrackVerified?: boolean
  displayCount?: number
  videoInputCount?: number
  elapsedMs?: number
  finalizationMs?: number
  webcamIncluded?: boolean
  webcamFrames?: number
  webcamRepeatedSlots?: number
  plannedAudioSamples48khz?: number
  microphoneIncluded?: boolean
  audioInputCount?: number
  audioSamplesWritten?: number
  audioMissingSourceFrames?: number
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
export default function QuickCapture({ openClips, scenes, sources, otherAudio, streamControl, openSettings, active }: {
  openClips: () => void; scenes: ReactNode; sources: ReactNode; otherAudio?: ReactNode;
  streamControl: ReactNode; openSettings: () => void; active: boolean
}) {
  const diagnostics = useRef<HTMLDialogElement>(null)
  useEffect(() => { if (!active && diagnostics.current?.open) diagnostics.current.close() }, [active])
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
  const [programBitrate, setProgramBitrate] = useState<'auto'|'1500'|'3000'|'6000'>('auto')
  const [continuousProfile, setContinuousProfile] = useState<'baseline' | '720p15' | '1080p30'>('baseline')
  const [programWebcam, setProgramWebcam] = useState(false)
  const [programMic, setProgramMic] = useState(false)
  const [micGain, setMicGain] = useState(1)
  const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([])
  const [micDevice, setMicDevice] = useState('')
  const [cardDevice, setCardDevice] = useState('')
  const [cardAudio, setCardAudio] = useState(false)
  const [cardGain, setCardGain] = useState(1)
  const [desktopAudio, setDesktopAudio] = useState(false)
  const [desktopGain, setDesktopGain] = useState(1)
  const [liveMeter, setLiveMeter] = useState(false)
  const [audioLevels, setAudioLevels] = useState<AudioLevels|null>(null)
  const [audioError, setAudioError] = useState('')
  const [scanningAudio, setScanningAudio] = useState(false)
  async function refreshAudio() {
    if (!supportedDesktop) return
    setScanningAudio(true); setAudioError('')
    try {
      const result = await request<{devices:AudioDevice[]}>('/capture/audio-inputs')
      setAudioDevices(result.devices)
      if (mac) {
        setMicDevice(current => result.devices.some(d=>d.id===current) ? current : result.devices.find(d=>d.isDefault)?.id ?? result.devices[0]?.id ?? '')
        setCardDevice(current => result.devices.some(d=>d.id===current) ? current : '')
      } // Windows: preserve explicit selections; never silently substitute a disconnected endpoint.
    } catch (cause) { setAudioError(String(cause)) }
    finally { setScanningAudio(false) }
  }
  useEffect(()=>{if(supportedDesktop) void refreshAudio()},[supportedDesktop])
  const audioRows: { name:string; input:AudioInput }[] = []
  if(programMic) audioRows.push({name:'Mic / Aux',input:{kind:'device',deviceId:micDevice||null,gain:micGain,muted:micGain===0}})
  if(cardAudio && cardDevice) audioRows.push({name:'HDMI / USB Audio',input:{kind:'device',deviceId:cardDevice,gain:cardGain,muted:cardGain===0}})
  if(desktopAudio) audioRows.push({name:'Desktop Audio',input:{kind:'desktop',deviceId:null,gain:desktopGain,muted:desktopGain===0}})
  const audioConfig = JSON.stringify(audioRows.map(r=>r.input))
  const audioConfigRef = useRef(audioConfig)
  audioConfigRef.current = audioConfig
  const audioIdentity = JSON.stringify(audioRows.map(r=>[r.input.kind,r.input.deviceId]))
  const audioSelectionError = !mac && supportedDesktop && ((programMic && micDevice && !audioDevices.some(d=>d.id===micDevice)) || (cardAudio && cardDevice && !audioDevices.some(d=>d.id===cardDevice))) ? 'Selected audio device unavailable. Refresh or choose another device.' : cardAudio && !cardDevice ? 'Select the capture card audio input.' : cardAudio && programMic && (!micDevice || cardDevice===micDevice) ? 'Select different named microphone and HDMI audio devices.' : ''

  const [graphic, setGraphic] = useState<GraphicOverlay | null>(null)
  const [graphicName, setGraphicName] = useState('')
  const [graphicPreview, setGraphicPreview] = useState('')
  const [placement, setPlacement] = useState<OverlayPlacement>({ xPercent: 0, yPercent: 0, widthPercent: 25 })
  const [graphicLoading, setGraphicLoading] = useState(false)
  const [systemAudio, setSystemAudio] = useState(false)
  const [microphone, setMicrophone] = useState(false)
  const [loading, setLoading] = useState(false)
  const [busy, setBusy] = useState(false)
  useEffect(()=>{
    setAudioLevels(null)
    if(!supportedDesktop || !active || !liveMeter || audioSelectionError || busy) return
    let cancelled=false
    let timer:ReturnType<typeof setTimeout>|undefined
    async function update(){
      if(cancelled)return
      if(document.hidden){timer=setTimeout(update,250);return}
      try{
        const response=await request<{levels:AudioLevels|null}>('/capture/audio-levels',{inputs:JSON.parse(audioConfigRef.current)})
        if(!cancelled){setAudioLevels(response.levels);setAudioError('')}
      }catch(cause){if(!cancelled){setAudioLevels(null);setAudioError(String(cause))}}
      if(!cancelled)timer=setTimeout(update,250)
    }
    void update()
    const visibility=()=>{if(document.hidden){setAudioLevels(null);void request('/capture/stop-audio-levels',{}).catch(()=>{})}}
    document.addEventListener('visibilitychange',visibility)
    return()=>{cancelled=true;if(timer!==undefined)clearTimeout(timer);document.removeEventListener('visibilitychange',visibility);void request('/capture/stop-audio-levels',{}).catch(()=>{})}
  },[supportedDesktop,active,liveMeter,audioIdentity,audioSelectionError,busy])
  const [captureMode, setCaptureMode] = useState<'screen' | 'program' | null>(null)
  const [stopRequested, setStopRequested] = useState(false)
  const [programSession, setProgramSession] = useState<string | null>(null)
  const programSessionRef = useRef<string | null>(null)
  const [programStopError, setProgramStopError] = useState('')
  const [programStats, setProgramStats] = useState<ProgramStats | null>(null)
  const [pausePending, setPausePending] = useState(false)
  useEffect(() => {
    setProgramStats(null)
    if (!programSession) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout>
    async function poll() {
      try {
        const stats = await request<ProgramStats | null>('/clips/program-mp4-stats', {sessionId:programSession})
        if (!cancelled && stats) setProgramStats(stats)
      } catch {
        if (!cancelled) setProgramStats(null)
      } finally {
        if (!cancelled) timer = setTimeout(poll, 250)
      }
    }
    void poll()
    return () => { cancelled = true; clearTimeout(timer) }
  }, [programSession])
  const [error, setError] = useState('')
  const [permissionSettingsError, setPermissionSettingsError] = useState('')
  const [saved, setSaved] = useState<CapturedFile | null>(null)
  const [persistedClips, setPersistedClips] = useState<{ id: string; format: string }[] | null>(null)
  const [inventoryError, setInventoryError] = useState('')
  const [recoveryBusy, setRecoveryBusy] = useState(false)
  const [recoveryMessage, setRecoveryMessage] = useState('')

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
    setGraphicLoading(true); setGraphic(null); setGraphicName(''); setGraphicPreview(''); setError('')
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
        setGraphicPreview(canvas.toDataURL('image/png'))
      } finally { image.close() }
    } catch (cause) { setError('Graphic import failed: ' + (cause as Error).message) }
    finally { setGraphicLoading(false) }
  }

  async function prepareLiveAcceptance(){
    if(!supportedDesktop||busy||loading||graphicLoading||audioSelectionError||programWebcam||programAviDuration>60)throw new Error('Select a valid 2–60 second native Program capture with webcam inset off.')
    const collection=await request<unknown>('/studio/collection')
    const state=collection as MinimalSceneCollection
    const videoDeviceId=programVideoInput(collection)
    const displayIds=videoDeviceId?[]:programTestDisplayIds(collection)
    if(displayIds.some(id=>!displays.some(d=>d.id===id)))throw new Error('Refresh disconnected displays before testing.')
    const scene=state.scenes.find(s=>s.name===state.currentScene)
    const layout=videoDeviceId?validateSceneLayout(scene?.layout):programTestLayout(collection)
    return {scene:state.currentScene,sources:videoDeviceId?[ 'Selected capture device: '+videoDeviceId ]:displayIds.map(id=>displays.find(d=>d.id===id)?.name??String(id)),
      audio:audioRows.map(row=>row.name+' · '+(row.input.muted?'muted':Math.round(row.input.gain*100)+'% gain')),
      config:{displayIds,videoDeviceId:videoDeviceId??null,seconds:programAviDuration,profile:continuousProfile,layout,transforms:programTransforms(collection)??null,
        overlay:graphic?{...graphic,placement}:null,audioInputs:audioRows.map(row=>row.input),bitrateKbps:programBitrate==='auto'?null:Number(programBitrate)}}
  }

  async function captureProgramVideoTest(mode: 'sampled' | 'continuous' | 'mp4' | 'flv' = 'sampled') {
    if (!supportedDesktop || loading || busy || graphicLoading || (mode === 'mp4' && !supportedDesktop)) return
    if (mode === 'flv' && (!supportedDesktop || programWebcam || programAviDuration>8)) return
    if (mode !== 'mp4' && mode !== 'flv' && (programAviDuration > 8 || continuousProfile === '1080p30')) return
    const sessionId = mode === 'mp4' ? crypto.randomUUID() : null
    programSessionRef.current = sessionId
    setProgramSession(sessionId)
    setProgramStopError('')
    setPausePending(false)
    setBusy(true)
    setCaptureMode('program')
    setStopRequested(false)
    setSaved(null)
    setError('')
    try {
      const collection = await request<unknown>('/studio/collection')
      const videoDeviceId = programVideoInput(collection)
      const transforms=programTransforms(collection)
      if(transforms && mode==='mp4' && programWebcam)throw new Error('Disable the separate webcam inset to use manual source geometry.')
      if (videoDeviceId && ((mode !== 'mp4' && mode !== 'flv') || !supportedDesktop || programWebcam)) throw new Error('Capture card recording requires native MP4; turn off the separate webcam inset.')
      const displayIds = videoDeviceId ? [] : programTestDisplayIds(collection)
      const state = collection as MinimalSceneCollection
      const layout = videoDeviceId ? validateSceneLayout(state.scenes.find(scene => scene.name === state.currentScene)?.layout) : programTestLayout(collection)
      if (displayIds.some(id => !displays.some(display => display.id === id))) {
        throw new Error('One of the Program displays is disconnected. Refresh displays and check the scene.')
      }
      const result = await request<CapturedFile>(mode === 'mp4'
        ? '/clips/capture-program-continuous-mp4'
        : mode === 'flv' ? '/clips/capture-program-live-packets' : mode === 'continuous' ? '/clips/capture-program-continuous-avi' : '/clips/capture-program-test-avi', {
        displayIds, seconds: programAviDuration, layout, transforms,
        ...(videoDeviceId ? { videoDeviceId } : {}),
        ...(mode !== 'sampled' ? { profile: continuousProfile, overlay: graphic ? { ...graphic, placement } : null } : {}),
        ...(mode === 'mp4' ? { sessionId, ...(programBitrate === 'auto' ? {} : {bitrateKbps:Number(programBitrate)}), webcam: programWebcam, microphone: programMic, micGain, audioInputs: audioRows.map(row=>row.input) } : mode === 'flv' ? { ...(programBitrate === 'auto' ? {} : {bitrateKbps:Number(programBitrate)}),audioInputs:audioRows.map(row=>row.input) } : {})
      })
      setSaved(result)
      await refreshSavedClips()
    } catch (cause) {
      setError((mode === 'mp4' ? 'Composed H.264 MP4' : mode === 'flv' ? 'Offline streaming packet FLV' : mode === 'continuous' ? 'Continuous Program AVI' : 'Sampled Program AVI') + ' test failed: ' + (cause as Error).message)
    } finally {
      programSessionRef.current = null
      setProgramSession(null)
      setProgramStopError('')
      setBusy(false)
      setCaptureMode(null)
    }
  }

  async function toggleProgramPause() {
    const sessionId = programSessionRef.current
    if (!sessionId || !programStats || pausePending || stopRequested || !['recording','paused'].includes(programStats.phase)) return
    setPausePending(true)
    setProgramStopError('')
    try {
      const paused = programStats.phase !== 'paused'
      const result = await request<{accepted:boolean}>('/clips/pause-program-mp4', {sessionId, paused})
      if (!result.accepted) throw new Error('Recording is not available for pause or resume.')
      // Refresh from the worker; do not synthesize a recording phase.
      const stats = await request<ProgramStats | null>('/clips/program-mp4-stats', {sessionId})
      if (programSessionRef.current === sessionId && stats) setProgramStats(stats)
    } catch (cause) {
      if (programSessionRef.current === sessionId) setProgramStopError((cause as Error).message)
    } finally {
      if (programSessionRef.current === sessionId) setPausePending(false)
    }
  }

  async function stopProgramCapture() {
    const sessionId = programSessionRef.current
    if (!sessionId || stopRequested) return
    setStopRequested(true)
    setProgramStopError('')
    try {
      const result = await request<{stopRequested:boolean}>('/clips/stop-program-mp4', { sessionId })
      if (!result.stopRequested) throw new Error('Recorder is starting or has finished. Retry if it is still recording.')
    } catch (cause) {
      if (programSessionRef.current === sessionId) {
        setStopRequested(false)
        setProgramStopError((cause as Error).message)
      }
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

  async function recoverFinalizedRecordings() {
    if (!supportedDesktop || busy || recoveryBusy) return
    setRecoveryBusy(true)
    setRecoveryMessage('')
    try {
      const result = await request<{recovered:number;skipped:number;failed:number}>('/clips/recover-finalized-program', {})
      setRecoveryMessage(`Recovered ${result.recovered} finalized recording(s); ${result.skipped} already present; ${result.failed} could not be recovered.`)
      await refreshSavedClips()
    } catch (cause) {
      setRecoveryMessage('Recovery failed: ' + (cause as Error).message)
    } finally { setRecoveryBusy(false) }
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

  const isNativeProgramMp4 = supportedDesktop
  const canProgram = supportedDesktop && !recoveryBusy && !busy && !loading && !graphicLoading && !audioSelectionError && (displays.length > 0 || mac)
  const overlayWidth = graphic ? Math.min(placement.widthPercent, 50 * 9 / 16 * graphic.width / graphic.height) : 0
  const overlayHeight = graphic ? overlayWidth * 16 / 9 * graphic.height / graphic.width : 0
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
          {graphic && <div className="graphic-placement-editor">
            <div className="graphic-stage" aria-label="Graphic placement reference canvas" onPointerDown={event => {
              if (busy) return
              event.currentTarget.setPointerCapture(event.pointerId)
              const rect = event.currentTarget.getBoundingClientRect()
              const x = Math.max(0, Math.min(100, Math.round(((event.clientX-rect.left)/rect.width*100-1.40625-overlayWidth/2)/(100-2*1.40625-overlayWidth)*100)))
              const y = Math.max(0, Math.min(100, Math.round(((event.clientY-rect.top)/rect.height*100-2.5-overlayHeight/2)/(95-overlayHeight)*100)))
              setPlacement(p => ({...p,xPercent:x,yPercent:y}))
            }} onPointerMove={event => {
              if (busy || !event.currentTarget.hasPointerCapture(event.pointerId)) return
              const rect = event.currentTarget.getBoundingClientRect()
              const x = Math.max(0, Math.min(100, Math.round(((event.clientX-rect.left)/rect.width*100-1.40625-overlayWidth/2)/(100-2*1.40625-overlayWidth)*100)))
              const y = Math.max(0, Math.min(100, Math.round(((event.clientY-rect.top)/rect.height*100-2.5-overlayHeight/2)/(95-overlayHeight)*100)))
              setPlacement(p => ({...p,xPercent:x,yPercent:y}))
            }}>
              <span>16:9 output reference</span>
              <img draggable={false} alt="Imported graphic placement" src={graphicPreview} style={{ width: `${overlayWidth}%`,height: `${overlayHeight}%`,left:`${1.40625+(100-2*1.40625-overlayWidth)*placement.xPercent/100}%`,top:`${2.5+(95-overlayHeight)*placement.yPercent/100}%` }}/>
            </div>
            <label className="gain-label">Horizontal <b>{placement.xPercent}%</b><input type="range" aria-label="Graphic horizontal position" min="0" max="100" value={placement.xPercent} disabled={busy} onChange={e => setPlacement(p => ({...p,xPercent:Number(e.target.value)}))}/></label>
            <label className="gain-label">Vertical <b>{placement.yPercent}%</b><input type="range" aria-label="Graphic vertical position" min="0" max="100" value={placement.yPercent} disabled={busy} onChange={e => setPlacement(p => ({...p,yPercent:Number(e.target.value)}))}/></label>
            <label className="gain-label">Size <b>{placement.widthPercent}%</b><input type="range" aria-label="Graphic width" min="10" max="50" value={placement.widthPercent} disabled={busy} onChange={e => setPlacement(p => ({...p,widthPercent:Number(e.target.value)}))}/></label>
            <button className="text-button dock-remove" disabled={busy} onClick={() => { setGraphic(null); setGraphicName(''); setGraphicPreview('') }}>Remove graphic</button>
          </div>}
          <p className="dock-note">Webcam and graphic render into recording tests, not the still preview.</p>
        </div>
      </div>
      <section className="production-dock mixer-dock panel" aria-label="Audio Mixer">
        <div className="panel-head"><div className="panel-title"><AudioLines size={17}/>Audio Mixer</div><span className="subdued">Test inputs</span></div>
        <div className="dock-body">
          <label className="layer-switch"><Volume2 size={16}/><span>Mic / Aux <small>Select native audio device</small></span><input type="checkbox" aria-label="Enable microphone" checked={programMic} disabled={!supportedDesktop || busy} onChange={e => setProgramMic(e.target.checked)}/></label>
          {supportedDesktop && <label className="dock-field">Microphone device<select aria-label="Microphone device" value={micDevice} disabled={busy || scanningAudio} onChange={e=>setMicDevice(e.target.value)}><option value="">System default</option>{micDevice&&!audioDevices.some(d=>d.id===micDevice)&&<option value={micDevice}>Selected device unavailable</option>}{audioDevices.map(d=><option key={d.id} value={d.id}>{d.name}{d.isDefault?' (default)':''}</option>)}</select></label>}
          <div className="meter-caption"><span>Last recorded peak</span><b>{peak === null ? '—' : `${Math.round(peak*100)}%`}</b></div>
          <div className="production-meter" role="meter" aria-label="Last recorded microphone peak" aria-valuemin={0} aria-valuemax={100} aria-valuenow={peak === null ? 0 : Math.round(Math.min(1,peak)*100)} aria-valuetext={peak === null ? 'No recording measured' : `${Math.round(peak*100)} percent`}>
            <span style={{width: `${peak === null ? 0 : Math.min(1,peak)*100}%`}}/>
          </div>
          <label className="gain-label">Gain <b>{Math.round(micGain*100)}%</b><input type="range" aria-label="Microphone gain" min="0" max="2" step="0.1" value={micGain} disabled={!supportedDesktop || !programMic || busy} onChange={e => setMicGain(Number(e.target.value))}/></label>
          <button className="secondary-button mute-button" disabled={!supportedDesktop || !programMic || busy} onClick={() => setMicGain(micGain === 0 ? 1 : 0)}>{micGain === 0 ? 'Unmute mic' : 'Mute mic'}</button>
          <label className="layer-switch"><Volume2 size={16}/><span>HDMI / USB Audio<small>Choose capture card audio endpoint</small></span><input type="checkbox" aria-label="Enable HDMI audio" checked={cardAudio} disabled={!supportedDesktop||busy} onChange={e=>setCardAudio(e.target.checked)}/></label>
          {cardAudio && <><label className="dock-field">Capture card audio<select aria-label="Capture card audio device" value={cardDevice} disabled={busy||scanningAudio} onChange={e=>setCardDevice(e.target.value)}><option value="">Select device</option>{cardDevice&&!audioDevices.some(d=>d.id===cardDevice)&&<option value={cardDevice}>Selected device unavailable</option>}{audioDevices.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select></label><label className="gain-label">HDMI gain <b>{Math.round(cardGain*100)}%</b><input aria-label="HDMI gain" type="range" min="0" max="2" step="0.1" value={cardGain} disabled={busy} onChange={e=>setCardGain(Number(e.target.value))}/></label><button className="secondary-button" disabled={busy} onClick={()=>setCardGain(cardGain===0?1:0)}>{cardGain===0?'Unmute HDMI':'Mute HDMI'}</button></>}
          <label className="layer-switch"><Volume2 size={16}/><span>Desktop Audio<small>{mac?'ScreenCaptureKit · Mac':'WASAPI · default playback'}</small></span><input aria-label="Enable desktop audio" type="checkbox" checked={desktopAudio} disabled={!supportedDesktop||busy} onChange={e=>setDesktopAudio(e.target.checked)}/></label>
          {desktopAudio && <><label className="gain-label">Desktop gain <b>{Math.round(desktopGain*100)}%</b><input aria-label="Desktop gain" type="range" min="0" max="2" step="0.1" value={desktopGain} disabled={busy} onChange={e=>setDesktopGain(Number(e.target.value))}/></label><button className="secondary-button" disabled={busy} onClick={()=>setDesktopGain(desktopGain===0?1:0)}>{desktopGain===0?'Unmute desktop':'Mute desktop'}</button></>}
          <button className="secondary-button" disabled={!supportedDesktop||busy||scanningAudio} onClick={()=>void refreshAudio()}>Refresh audio devices</button>
          <label className="layer-switch"><AudioLines size={16}/><span>Live audio meters<small>Explicit local capture · stops before recording</small></span><input aria-label="Live audio meters" type="checkbox" checked={liveMeter} disabled={!supportedDesktop||busy} onChange={e=>setLiveMeter(e.target.checked)}/></label>
          {liveMeter && [...audioRows.map((row,i)=>({name:row.name,peak:audioLevels?.inputPeaks[i]})),{name:'Program mix',peak:audioLevels?.mixedPeak}].map(row=><div key={row.name}><div className="meter-caption"><span>{row.name}</span><b>{row.peak===undefined?'—':Math.round(row.peak*100)+'%'}</b></div><div className="production-meter" role="meter" aria-label={row.name+' live peak'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round((row.peak??0)*100)} aria-valuetext={row.peak===undefined?'Waiting for samples':Math.round(row.peak*100)+' percent'}><span style={{width:Math.min(100,(row.peak??0)*100)+'%'}}/></div></div>)}
          {audioLevels && <small className="dock-note">Mixed clipping: {audioLevels.clippedSamples} samples / measured block</small>}
          {(audioSelectionError||audioError) && <small className="preview-error" role="alert">{audioSelectionError||audioError}</small>}
          <p className="dock-note">{mac?'Named inputs mix into Mac MP4 AAC tests. Live meters measure local PCM, not playback.':'Windows mixes named capture endpoints and default playback loopback. Live meters measure local PCM, not playback.'} Device-clock sync remains unverified.</p>
          {otherAudio}
        </div>
      </section>
      <section className="production-dock output-dock panel" aria-label="Controls">
        <div className="panel-head"><div className="panel-title"><Settings2 size={17}/>Controls</div><span className="subdued">{busy ? 'Working' : 'Local'}</span></div>
        <div className="dock-body">
          <label className="dock-field">Video profile
            <select aria-label="Continuous Program test profile" value={continuousProfile} disabled={busy} onChange={event => { const profile = event.target.value as 'baseline' | '720p15' | '1080p30'; setContinuousProfile(profile) }}>
              <option value="baseline">360p · 10 fps</option><option value="720p15">720p · 15 fps</option>{supportedDesktop && <option value="1080p30">1080p · 30 fps (test)</option>}
            </select>
          </label>
          {supportedDesktop && <label className="dock-field">Target video bitrate
            <select aria-label="Program MP4 target bitrate" value={programBitrate} disabled={busy} onChange={e=>setProgramBitrate(e.target.value as typeof programBitrate)}>
              <option value="auto">Automatic by profile</option><option value="1500">1.5 Mbps</option><option value="3000">3 Mbps</option><option value="6000">6 Mbps</option>
            </select>
          </label>}
          <label className="dock-field">Recording length
            <select aria-label="Composed Program video test duration" value={programAviDuration} disabled={busy} onChange={event => setProgramAviDuration(Number(event.target.value))}>
              {(mac ? [2,4,6,8,15,30,45,60,120,180,240,300] : supportedDesktop ? [2,4,6,8,15,30,45,60] : [2,4,6,8]).map(seconds => <option key={seconds} value={seconds}>{seconds > 60 ? `${seconds/60} min · segmented` : `${seconds} seconds`}</option>)}
            </select>
          </label>
          {mac && programAviDuration > 60 && <small className="dock-note">Separate 60-second MP4 clips. Sources restart between clips; gaps are expected. Completed clips remain saved if a later segment fails.</small>}
          <button className="primary-button program-record" type="button" disabled={!canProgram} onClick={() => void captureProgramVideoTest(isNativeProgramMp4 ? 'mp4' : 'continuous')}><span className="record-dot"/>{busy ? 'Capturing / finalizing…' : 'Record Program test'}</button>
          {programSession && busy && <button className="secondary-button" type="button" disabled={stopRequested} onClick={() => void stopProgramCapture()}><Square size={15}/>{stopRequested ? 'Stopping and finalizing…' : 'Stop and save Program MP4'}</button>}
          {programSession && busy && <>
            <button className="secondary-button" type="button" disabled={pausePending || stopRequested || !programStats || !['recording','paused'].includes(programStats.phase)} onClick={() => void toggleProgramPause()}>{programStats?.phase === 'paused' ? 'Resume Program recording' : 'Pause Program recording'}</button>
            <div aria-label="Program recording statistics" role="status" className="dock-note">
              {programStats ? <>
                <b>{programStats.phase} · {programStats.fps > 0 ? (programStats.recordedFrames/programStats.fps).toFixed(1) : '0.0'}s media</b>
                {(programStats.segmentCount??1)>1 && <div>Segment {programStats.segmentIndex} / {programStats.segmentCount} · {programStats.completedSegments} previously finalized</div>}
                {typeof programStats.transitionsCompleted==='number' && <div>{programStats.transitionBusy?'Native Take pending / encoding':'Native Takes completed: '+programStats.transitionsCompleted}</div>}
                {audioRows.length>0&&typeof programStats.audioPeak==='number'&&<><div className="meter-caption"><span>Recorded mix peak</span><b>{Math.round(programStats.audioPeak*100)}%</b></div><div className="production-meter" role="meter" aria-label="Recording mix peak" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(programStats.audioPeak*100)}><span style={{width:Math.min(100,programStats.audioPeak*100)+'%'}}/></div>{!mac&&<div>{programStats.audioMissingSourceFrames??0} source frames padded with silence</div>}</>}
                <div>{programStats.recordedFrames} submitted frames · {(programStats.elapsedMs/1000).toFixed(1)}s wall time</div>
                <div>{(programStats.fileBytes/1048576).toFixed(2)} MiB written · {programStats.recordedFrames > 0 ? (programStats.fileBytes*8*programStats.fps/programStats.recordedFrames/1000).toFixed(0) : '—'} kbps file average (buffered)</div>
                <div>{programStats.lateFrames} late slots · {programStats.repeatedSourceSlots} repeated source slots</div>
                <div>{(programStats.pausedMs/1000).toFixed(1)}s paused (nominal)</div>
              </> : 'Waiting for native recording statistics…'}
            </div>
          </>}
          {programStopError && <small className="preview-error" role="status">{programStopError}</small>}
          <small className="dock-note">{mac ? 'H.264 MP4 · optional AAC' : supportedDesktop ? 'Windows H.264 MP4 · optional AAC'  : 'Requires Mac/Windows app'} · bounded diagnostic</small>
          {streamControl}
          <LiveAcceptance active={active} disabled={!canProgram||programWebcam||programAviDuration>60} prepare={prepareLiveAcceptance} onBusy={setBusy}/>
          <div className="dock-action-row"><button className="secondary-button" onClick={openClips}><Film size={14}/>Clips</button><button className="secondary-button" onClick={openSettings}><Settings2 size={14}/>Settings</button></div>
          <button className="secondary-button diagnostics-button" onClick={openDiagnostics}><Monitor size={14}/>Capture diagnostics</button>
          {saved?.segmentedSession?.warning && <small className="preview-error" role="status">{saved.segmentedSession.warning}</small>}
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
          <button className="secondary-button" disabled={!canProgram || programAviDuration > 8 || continuousProfile === '1080p30'} onClick={() => void captureProgramVideoTest('sampled')}>Sampled Program AVI</button>
          <button className="secondary-button" disabled={!canProgram || continuousProfile === '1080p30' || programAviDuration > 8} onClick={() => void captureProgramVideoTest('continuous')}>Continuous-source Program AVI</button>
          {supportedDesktop && <button className="secondary-button" disabled={!canProgram || programWebcam || programAviDuration>8} onClick={()=>void captureProgramVideoTest('flv')}>Offline streaming packet FLV test</button>}
          {supportedDesktop && <small className="dock-note">FLV: local 2–8 second live-encoder diagnostic. No network connection; separate webcam inset must be off.</small>}
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
      {supportedDesktop && <button className="secondary-button" type="button" disabled={busy || recoveryBusy} onClick={() => void recoverFinalizedRecordings()}>{recoveryBusy ? 'Checking finalized files…' : 'Recover finalized Program recordings'}</button>}
      {recoveryMessage && <small role="status">{recoveryMessage}</small>}
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
        <strong>Saved {saved.format.toUpperCase()} locally — playback{saved.format === 'mp4' && (saved.displayCount === undefined || saved.microphoneIncluded || (saved.audioInputCount??0)>0) ? ' and audio' : ''} not yet verified</strong>
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
        {saved.segmentedSession && <div className="help-text">
          <strong>Segmented session: {saved.segmentedSession.segments.length} saved clip(s)</strong>
          <p>{saved.segmentedSession.completedSeconds.toFixed(2)}s total media / {saved.segmentedSession.requestedSeconds}s requested · {(saved.segmentedSession.totalBytes/1048576).toFixed(2)} MiB{saved.segmentedSession.stoppedEarly ? ' · ended early' : ''}. Separate files; gaps between clips are possible. Details below refer to the last clip.</p>
          {saved.segmentedSession.warning && <p role="alert">{saved.segmentedSession.warning}</p>}
          {saved.segmentedSession.segments.map((segment,i)=><p key={segment.id}>Segment {i+1} · {segment.durationSeconds.toFixed(2)}s · {(segment.bytes/1048576).toFixed(2)} MiB<br/><code>{segment.path}</code></p>)}
        </div>}
        {saved.displayCount !== undefined && <p className="help-text" role="status">
          {saved.displayCount} display(s) · {saved.videoInputCount ?? 0} video device(s) · {saved.width ?? 640}×{saved.height ?? 360} · nominal {saved.fps ?? 10} fps {saved.format.toUpperCase()} ·
          {typeof saved.measuredSamplingFps === 'number' ? ` ${saved.measuredSamplingFps.toFixed(1)} fps measured sampling` : ' capture performance unknown'} ·
          {saved.elapsedMs !== undefined ? ` ${(saved.elapsedMs / 1000).toFixed(2)}s wall-clock capture` : ' elapsed time unknown'}.
          The nominal output clock can differ from wall-clock capture when frames arrive late.
        </p>}
        {typeof saved.transitionsCompleted==='number' && <p className="help-text">Native scene transitions completed: {saved.transitionsCompleted}. Independent playback verification pending.</p>}
        {typeof saved.targetBitrateKbps === 'number' && <p className="help-text">Requested video bitrate: {saved.targetBitrateKbps} kbps (encoder target, not guaranteed actual bitrate).</p>}
        {typeof saved.pausedMs === 'number' && saved.pausedMs > 0 && <p className="help-text">{(saved.pausedMs/1000).toFixed(2)}s paused input omitted from the nominal recording timeline.</p>}
        {saved.stoppedEarly && <p className="help-text" role="status">Program recording stopped early and finalized.</p>}
        {saved.overlayIncluded && <p className="help-text">Static graphic overlay included in output.</p>}
        {(saved.microphoneIncluded || (saved.audioInputCount??0)>0) && <p className="help-text" role="status">AAC requested · {saved.audioSamplesWritten ?? 0} PCM frames supplied at 48 kHz · peak {Math.round((saved.audioPeak ?? 0)*100)}% · {saved.audioClippedSamples ?? 0} clipped samples. {typeof saved.audioMissingSourceFrames==='number'&&saved.audioMissingSourceFrames>0 ? ` Windows source shortfall: ${saved.audioMissingSourceFrames} source frames filled with silence.` : ''} Confirm the AAC track and audible audio independently; capture timestamps and drift are not verified.</p>}
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
