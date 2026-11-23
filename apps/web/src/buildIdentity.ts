/**
 * Every alpha package uses a core semver (e.g. 0.3.9), so display its actual
 * CI release branch and commit to keep device acceptance reports reproducible.
 * These strings are baked into web/Tauri assets at build time, never fetched
 * from the network or derived from the user's machine.
 */
import { version as appVersion } from '../../../package.json'

declare const __MOOBEE_BUILD_SHA__: string
declare const __MOOBEE_RELEASE_REF__: string

export function formatBuildIdentity(version: string, reference: string, sha: string): string {
  const normalized = /^release\/(v\d+\.\d+\.\d+-alpha\.\d+)$/.exec(reference)?.[1]
  const cleanSha = /^[a-f0-9]{7,40}$/i.test(sha) ? sha.slice(0, 8) : 'local'
  return normalized ? `${normalized} · ${cleanSha}` : `v${version} dev · ${cleanSha}`
}

export const buildIdentity = formatBuildIdentity(
  appVersion,
  __MOOBEE_RELEASE_REF__,
  __MOOBEE_BUILD_SHA__
)
