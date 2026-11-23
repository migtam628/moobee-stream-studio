import { useCallback, useEffect, useState } from 'react'
import { Activity, RefreshCcw } from 'lucide-react'
import { request, runningInTauri } from './api'

type NativeReport = {
  displaysDetected: number
  displayEnumerationError: string | null
  overlayHealthResponding: boolean
  nativeMp4Supported: boolean
  nativeMp4OsVersion: string | null
  nativeMp4Reason: string
  nativeRecordingVerified: boolean
  audioMixerAvailable: boolean
  sceneCompositorAvailable: boolean
  rtmpOutputAvailable: boolean
}
type ServiceRow = { name: string; status: string; detail: string; ready: boolean }

export default function ServiceDiagnostics({ sceneConnected }: { sceneConnected: boolean }) {
  const desktop = runningInTauri()
  const [report, setReport] = useState<NativeReport | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')

  const refresh = useCallback(async () => {
    if (!desktop) return
    setLoading(true)
    setError('')
    try {
      const result = await request<NativeReport>('/services/diagnostics')
      setReport(result)
    } catch (cause) {
      setReport(null)
      setError('Native diagnostics unavailable: ' + (cause as Error).message)
    } finally {
      setLoading(false)
    }
  }, [desktop])
  useEffect(() => { void refresh() }, [refresh])

  const rows: ServiceRow[] = [
    {
      name: 'Scene editor',
      status: sceneConnected ? 'Available' : 'Offline',
      detail: sceneConnected ? 'Local scene state and source editing are responding' : 'Open the desktop app or start the loopback API',
      ready: sceneConnected
    },
    {
      name: 'Display devices',
      status: !desktop ? 'Desktop only' : report ? (report.displaysDetected > 0 ? 'Detected' : 'Unavailable') : 'Not checked',
      detail: report?.displayEnumerationError || (report && report.displaysDetected > 0
        ? `${report.displaysDetected} display(s) detected; screen permissions and file recording still need an actual test`
        : 'Device enumeration requires the installed desktop app and operating-system permissions'),
      ready: !!report && report.displaysDetected > 0
    },
    {
      name: 'Local alert overlay',
      status: report?.overlayHealthResponding ? 'Responding' : 'Not verified',
      detail: report?.overlayHealthResponding
        ? 'MooBee loopback overlay /health endpoint returned a response'
        : 'The loopback overlay server has not responded; this does not test visible alerts inside a stream',
      ready: !!report?.overlayHealthResponding
    },
    {
      name: 'macOS H.264 recorder',
      status: report?.nativeMp4Supported ? 'Available to test' : 'Not available',
      detail: report?.nativeMp4Reason ||
        'Run native diagnostics to determine whether this Mac meets the system requirement or whether the recorder bridge has failed',
      ready: false
    },
    {
      name: 'Program scene compositor',
      status: 'Not implemented',
      detail: 'Preview and Program are not rendered into a real mixed video output',
      ready: false
    },
    {
      name: 'Live audio mixer',
      status: 'Not implemented',
      detail: 'No synchronized device mixer, meters or verified audio output yet',
      ready: false
    },
    {
      name: 'RTMP / RTMPS broadcast',
      status: 'Not implemented',
      detail: 'Twitch, YouTube and Kick cannot receive video from MooBee yet',
      ready: false
    },
    {
      name: 'Production signed updates',
      status: 'Not configured',
      detail: 'GitHub prerelease downloads work; secure signed automatic installation requires owner-controlled signing keys',
      ready: false
    }
  ]

  return <div className="service-diagnostics">
    <p>Checks report <strong>runtime availability, not production acceptance</strong>. A compiled recorder or connected destination icon does not mean it can broadcast.</p>
    <div className="service-diagnostics-actions">
      <button className="secondary-button" type="button" disabled={loading || !desktop}
        onClick={() => void refresh()}><RefreshCcw size={16}/>{loading ? 'Checking native services…' : 'Refresh local services'}</button>
      <span>{desktop ? 'Installed desktop runtime' : 'Browser/control-panel edition'}</span>
    </div>
    {error && <p className="clip-error" role="alert">{error}</p>}
    <div className="service-diagnostics-list" aria-label="MooBee engine services" role="list">
      {rows.map(row => <div className="service-diagnostics-row" key={row.name} role="listitem">
        <div className="service-diagnostics-name"><Activity size={15}/><strong>{row.name}</strong></div>
        <span className={row.ready ? 'service-state-ready' : 'service-state-pending'}>{row.status}</span>
        <small>{row.detail}</small>
      </div>)}
    </div>
  </div>
}
