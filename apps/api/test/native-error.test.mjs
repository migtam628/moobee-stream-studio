import test from 'node:test'
import assert from 'node:assert/strict'
import { nativeErrorMessage } from '../../../apps/web/src/nativeError.ts'

test('preserves Tauri Rust Result::Err string instead of displaying undefined', () => {
  assert.equal(nativeErrorMessage('Native macOS recording failed: Screen recording permission denied'),
    'Native macOS recording failed: Screen recording permission denied')
  assert.equal(nativeErrorMessage(new Error('Encoder failed to start')), 'Encoder failed to start')
})

test('handles serialized error payloads and unexpected rejection values', () => {
  assert.equal(nativeErrorMessage({ message: 'Invalid SCStream display ID' }), 'Invalid SCStream display ID')
  assert.equal(nativeErrorMessage({ error: 'AVAssetWriter could not finalize' }), 'AVAssetWriter could not finalize')
  assert.equal(nativeErrorMessage({ reason: 'No screen access' }), 'No screen access')
  assert.equal(nativeErrorMessage({ code: 42 }), 'Native command failed (code: 42)')
  assert.match(nativeErrorMessage(undefined), /without details/)
  assert.match(nativeErrorMessage(null), /without details/)
  assert.match(nativeErrorMessage({}), /without details/)
  assert.notEqual(nativeErrorMessage('Some native error'), 'undefined')
})

test('does not allow long native diagnostics to monopolize the error panel', () => {
  assert.equal(nativeErrorMessage('a'.repeat(2000)).length, 1500)
})
