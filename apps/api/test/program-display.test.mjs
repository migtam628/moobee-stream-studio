import test from 'node:test'
import assert from 'node:assert/strict'
import { singleProgramDisplayId, programTestDisplayIds } from '../../../apps/web/src/programDisplay.ts'

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


test('composed Program AVI selects active Program displays, never Preview', () => {
  const two = structuredClone(state)
  two.scenes[0].sources.push({ id: 9, kind: 'display', deviceId: '1', enabled: true })
  assert.deepEqual(programTestDisplayIds(two), [0, 1])
  assert.deepEqual(programTestDisplayIds(state), [0])
})

test('composed Program AVI rejects missing, duplicate and unsupported sources', () => {
  const empty = structuredClone(state)
  empty.scenes[0].sources = []
  assert.throws(() => programTestDisplayIds(empty), /one to four/)
  const duplicate = structuredClone(state)
  duplicate.scenes[0].sources.push({ id: 9, kind: 'display', deviceId: '0', enabled: true })
  assert.throws(() => programTestDisplayIds(duplicate), /only once/)
  const unsupported = structuredClone(state)
  unsupported.scenes[0].sources.push({ id: 10, kind: 'camera', deviceId: '1', enabled: true })
  assert.throws(() => programTestDisplayIds(unsupported), /displays only/)
  const invalid = structuredClone(state)
  invalid.scenes[0].sources[0].deviceId = '../../secret'
  assert.throws(() => programTestDisplayIds(invalid), /Invalid Program/)
  const tooMany = structuredClone(state)
  tooMany.scenes[0].sources = [0, 1, 2, 3, 4].map((i) =>
    ({ id: i + 10, kind: 'display', deviceId: String(i), enabled: true }))
  assert.throws(() => programTestDisplayIds(tooMany), /one to four/)
})
