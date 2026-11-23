import test from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { auditBetaReadiness, REQUIRED_BETA_GATES } from '../../../scripts/check-beta-readiness.mjs'

const source = JSON.parse(readFileSync(new URL('../../../docs/BETA_READINESS.json', import.meta.url), 'utf8'))
const clone = () => structuredClone(source)

test('current beta inventory is complete, valid and explicitly not ready', () => {
  const report = auditBetaReadiness(source)
  assert.deepEqual(report.errors, [])
  assert.equal(report.total, REQUIRED_BETA_GATES.length)
  assert.equal(report.ready, false)
  assert.equal(report.verified, 0)
  assert.equal(report.blocked.length, REQUIRED_BETA_GATES.length)
})

test('beta audit rejects missing and duplicated gates, invalid tags and statuses', () => {
  const broken = clone()
  broken.target = 'v0.4.0-alpha.1'
  broken.gates.pop()
  broken.gates.push(structuredClone(broken.gates[0]))
  broken.gates[1].status = 'done'
  const report = auditBetaReadiness(broken)
  assert.equal(report.ready, false)
  assert.match(report.errors.join(' '), /Target must use/)
  assert.match(report.errors.join(' '), /Duplicate beta gate/)
  assert.match(report.errors.join(' '), /Missing beta gate/)
  assert.match(report.errors.join(' '), /Invalid status/)
})

test('a claimed verified gate without real evidence references fails closed', () => {
  const broken = clone()
  broken.gates[0].status = 'verified'
  let report = auditBetaReadiness(broken)
  assert.match(report.errors.join(' '), /requires GitHub evidence links/)
  broken.gates[0].evidence = ['file:///private/recording.mp4']
  report = auditBetaReadiness(broken)
  assert.match(report.errors.join(' '), /requires GitHub evidence links/)
})

test('all gates marked verified requires linked evidence; human validation remains separate', () => {
  const document = clone()
  document.gates.forEach(gate => {
    gate.status = 'verified'
    gate.evidence = ['https://github.com/migtam628/moobee-stream-studio/issues/14']
  })
  const report = auditBetaReadiness(document)
  assert.equal(report.ready, true)
  assert.equal(report.automatedAuditOnly, true)
  assert.equal(report.verified, REQUIRED_BETA_GATES.length)
})
