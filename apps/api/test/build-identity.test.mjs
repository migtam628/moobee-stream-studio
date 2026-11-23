import test from 'node:test'
import assert from 'node:assert/strict'
import { formatBuildIdentity } from '../../../apps/web/src/buildLabel.ts'

test('release builds show immutable release tag and commit for hardware acceptance', () => {
  assert.equal(formatBuildIdentity('0.3.9', 'release/v0.3.9-alpha.7', 'a12345678abcdef'),
    'v0.3.9-alpha.7 · a1234567')
})

test('local and pull request builds do not masquerade as official releases', () => {
  assert.equal(formatBuildIdentity('0.3.9', 'dev', ''), 'v0.3.9 dev · local')
  assert.equal(formatBuildIdentity('0.3.9', '16/merge', 'a1234567'),
    'v0.3.9 dev · a1234567')
  assert.equal(formatBuildIdentity('0.3.9', 'release/anything-else', 'wrong'), 'v0.3.9 dev · local')
})
