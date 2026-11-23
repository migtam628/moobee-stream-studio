/**
 * Every alpha package uses a core semver (e.g. 0.3.9), so display its actual
 * CI release branch and commit to keep device acceptance reports reproducible.
 * These strings are baked into web/Tauri assets at build time, never fetched
 * from the network or derived from the user's machine.
 */
import { version as appVersion } from '../../../package.json'
import { formatBuildIdentity } from './buildLabel'

declare const __MOOBEE_BUILD_SHA__: string
declare const __MOOBEE_RELEASE_REF__: string

export const buildIdentity = formatBuildIdentity(
  appVersion,
  typeof __MOOBEE_RELEASE_REF__ === 'string' ? __MOOBEE_RELEASE_REF__ : 'dev',
  typeof __MOOBEE_BUILD_SHA__ === 'string' ? __MOOBEE_BUILD_SHA__ : 'local'
)
