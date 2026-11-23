#!/usr/bin/env node
// Beta gate inventory audit. This checks recorded evidence metadata only.
// It does NOT independently certify physical hardware tests or signed binaries.
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const REQUIRED_BETA_GATES = [
  'mac-native-mp4',
  'program-video-compositor',
  'camera-and-overlays',
  'audio-mix-and-sync',
  'windows-h264-aac',
  'rtmps-broadcast',
  'secure-credentials-and-api',
  'signed-desktop-and-hardware-acceptance'
]

const STATUSES = new Set(['blocked', 'in-progress', 'needs-hardware-qa', 'verified'])

export function auditBetaReadiness(document) {
  const errors = []
  if (!document || typeof document !== 'object' || document.schemaVersion !== 1) {
    errors.push('Expected schemaVersion 1')
  }
  if (!/^v\d+\.\d+\.\d+-beta\.\d+$/.test(document?.target || '')) {
    errors.push('Target must use vX.Y.Z-beta.N')
  }
  const entries = Array.isArray(document?.gates) ? document.gates : []
  if (!Array.isArray(document?.gates)) errors.push('Missing gates array')
  const seen = new Set()
  const tally = { blocked: 0, 'in-progress': 0, 'needs-hardware-qa': 0, verified: 0 }
  for (const gate of entries) {
    if (!gate || !REQUIRED_BETA_GATES.includes(gate.id)) {
      errors.push('Unknown beta gate ID')
      continue
    }
    if (seen.has(gate.id)) errors.push('Duplicate beta gate: ' + gate.id)
    seen.add(gate.id)
    if (!STATUSES.has(gate.status)) {
      errors.push('Invalid status for ' + gate.id)
      continue
    }
    tally[gate.status] += 1
    if (gate.status === 'verified') {
      if (!Array.isArray(gate.evidence) || gate.evidence.length === 0 ||
          !gate.evidence.every(link => typeof link === 'string' &&
            /^https:\/\/github\.com\/migtam628\/moobee-stream-studio\/(issues|pull|actions\/runs)\/\d+(?:[#?].*)?$/.test(link))) {
        errors.push('Verified gate requires GitHub evidence links: ' + gate.id)
      }
    }
  }
  for (const id of REQUIRED_BETA_GATES) {
    if (!seen.has(id)) errors.push('Missing beta gate: ' + id)
  }
  const ready = errors.length === 0 && tally.verified === REQUIRED_BETA_GATES.length
  return { target: document?.target || null, ready, automatedAuditOnly: true,
    verified: tally.verified, total: REQUIRED_BETA_GATES.length, statuses: tally,
    blocked: entries.filter(g => g && g.status !== 'verified').map(g => g.id),
    errors }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const args = new Set(process.argv.slice(2))
  try {
    const document = JSON.parse(readFileSync(new URL('../docs/BETA_READINESS.json', import.meta.url), 'utf8'))
    const report = auditBetaReadiness(document)
    if (args.has('--json')) console.log(JSON.stringify(report, null, 2))
    else {
      console.log('Beta readiness (evidence metadata, not hardware certification)')
      console.log(report.target + ': ' + report.verified + '/' + report.total + ' verified')
      for (const id of report.blocked) console.log('NOT VERIFIED: ' + id)
      for (const error of report.errors) console.error('INVALID: ' + error)
      console.log(report.ready ? 'Metadata complete; human acceptance still required.' : 'NOT READY for beta publishing.')
    }
    if (report.errors.length || (args.has('--enforce') && !report.ready)) process.exitCode = 1
  } catch (error) {
    console.error('Beta readiness audit failed: ' + (error instanceof Error ? error.message : String(error)))
    process.exitCode = 1
  }
}
