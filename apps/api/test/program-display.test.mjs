import test from 'node:test'
import assert from 'node:assert/strict'
import { singleProgramDisplayId } from '../../../apps/web/src/programDisplay.ts'

const state = {
  studioMode: true,
  currentScene: 'Gameplay',
  previewScene: 'Starting Soon',
  scenes: [
    { name: 'Gameplay', sources: [{ id: 8, kind: 'display', deviceId: '0', enabled: true }] },
    { name: 'Starting Soon', sources: [{ id: 12, kind: 'display', deviceId: '2', enabled: true }] }
  ]
}

test('direct recorder selects current Program, not Studio Mode Preview', () => {
  assert.equal(singleProgramDisplayId(state), 0)
})

test('rejects scenes with zero or multiple active sources rather than silently dropping overlays', () => {
  const empty = { ...state, scenes: [{ name: 'Gameplay', sources: [] }] }
  assert.throws(() => singleProgramDisplayId(empty), /exactly one/)
  const multiple = { ...state, scenes: [{ name: 'Gameplay', sources: [
    { kind: 'display', deviceId: '0', enabled: true },
    { kind: 'display', deviceId: '1', enabled: true }
  ] }] }
  assert.throws(() => singleProgramDisplayId(multiple), /scene compositor/)
})

test('rejects unavailable or invalid Program scenes', () => {
  assert.throws(() => singleProgramDisplayId(null), /No current/)
  assert.throws(() => singleProgramDisplayId({ ...state, currentScene: 'Missing' }), /could not be found/)
  const invalid = { ...state, scenes: [{ name: 'Gameplay', sources: [
    { kind: 'display', deviceId: '../../etc/passwd', enabled: true }
  ] }] }
  assert.throws(() => singleProgramDisplayId(invalid), /identifier is invalid/)
})
