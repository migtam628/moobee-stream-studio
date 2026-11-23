/** Pure, testable display format. Never infer release status from semver alone. */
export function formatBuildIdentity(version: string, reference: string, sha: string): string {
  const normalized = /^release\/(v\d+\.\d+\.\d+-alpha\.\d+)$/.exec(reference)?.[1]
  const cleanSha = /^[a-f0-9]{7,40}$/i.test(sha) ? sha.slice(0, 8) : 'local'
  return normalized ? `${normalized} · ${cleanSha}` : `v${version} dev · ${cleanSha}`
}
