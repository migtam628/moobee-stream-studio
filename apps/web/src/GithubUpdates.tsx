import { useState } from 'react'

// GitHub is the distribution source. This component never downloads,
// installs, or trusts an unsigned executable in the app.
// Native automatic installation requires the Tauri signed-updater rollout.
const API = 'https://api.github.com/repos/migtam628/moobee-stream-studio/releases?per_page=10'

type GithubRelease = {
  tag_name: string
  prerelease: boolean
  draft: boolean
  published_at: string | null
}

type ReleaseInfo = {
  tag: string
  kind: 'alpha' | 'beta' | 'rc' | 'stable'
  url: string
}

export default function GithubUpdates({ appVersion }: { appVersion: string }) {
  const [loading, setLoading] = useState(false)
  const [latest, setLatest] = useState<ReleaseInfo | null>(null)
  const [message, setMessage] = useState('')

  async function check() {
    setLoading(true)
    setMessage('')
    setLatest(null)
    try {
      const response = await fetch(API, {
        headers: { Accept: 'application/vnd.github+json' }
      })
      if (!response.ok) throw new Error(`GitHub release check returned HTTP ${response.status}`)
      const data: unknown = await response.json()
      if (!Array.isArray(data)) throw new Error('GitHub returned an invalid release list')
      const release = (data as GithubRelease[]).find(item =>
        item && !item.draft && typeof item.tag_name === 'string' &&
        /^v\d+\.\d+\.\d+(?:-(?:alpha|beta|rc)\.\d+)?$/.test(item.tag_name) &&
        typeof item.published_at === 'string')
      if (!release) {
        setMessage('No published MooBee releases were found.')
        return
      }
      // Construct URLs from validated tags; never follow arbitrary API-provided links.
      const tag = release.tag_name
      const kind = tag.includes('-alpha.') ? 'alpha' : tag.includes('-beta.') ?
        'beta' : tag.includes('-rc.') ? 'rc' : 'stable'
      setLatest({
        tag,
        kind,
        url: `https://github.com/migtam628/moobee-stream-studio/releases/tag/${tag}`
      })
    } catch (error) {
      setMessage(`Unable to check GitHub Releases: ${(error as Error).message}`)
    } finally {
      setLoading(false)
    }
  }

  return <div className="collection-backup">
    <p>Current application code version: <strong>{appVersion}</strong>. Check the latest
      published installer and release notes directly from the official GitHub repository.</p>
    <div className="collection-actions">
      <button type="button" className="secondary-button" disabled={loading}
        onClick={() => void check()}>
        {loading ? 'Checking GitHub…' : 'Check for releases'}
      </button>
      <a className="secondary-button"
        href="https://github.com/migtam628/moobee-stream-studio/releases"
        target="_blank" rel="noopener noreferrer">All releases</a>
    </div>
    {latest && <div className="info-block" role="status">
      <div>
        <strong>Latest published: {latest.tag}</strong>
        <p>{latest.kind === 'stable' ? 'Stable release' : `Experimental ${latest.kind} prerelease`}.
          A published GitHub release is not proof of full recording or streaming capability.</p>
        <a href={latest.url} target="_blank" rel="noopener noreferrer">
          View release notes and installers on GitHub
        </a>
      </div>
    </div>}
    {message && <p className="help-text" role="status">{message}</p>}
    <p className="help-text">GitHub-hosted downloads are available now. Secure automatic
      installation inside the app is intentionally disabled until updater signatures,
      code signing, and supported-device acceptance are configured and verified.</p>
  </div>
}
