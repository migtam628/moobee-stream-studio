import test from 'node:test'
import assert from 'node:assert/strict'
import { isScreenCapturePermissionDenied } from '../../../apps/web/src/capturePermission.ts'

test('ScreenCaptureKit -3801 is recognized as a user-granted privacy permission issue', () => {
  const actual = 'Native recording failed: Native macOS recording failed: com.apple.ScreenCaptureKit.SCStreamErrorDomain:-3801 — The user declined TCCs for application, window, display capture'
  assert.equal(isScreenCapturePermissionDenied(actual), true)
  assert.equal(isScreenCapturePermissionDenied('SCStreamErrorUserDeclined'), true)
  assert.equal(isScreenCapturePermissionDenied('User declined TCCs'), true)
})

test('MP4 codec, capture timeout and unrelated ScreenCaptureKit errors are not misdiagnosed as TCC', () => {
  assert.equal(isScreenCapturePermissionDenied('AVAssetWriter: -11800 — H.264 encoder failed'), false)
  assert.equal(isScreenCapturePermissionDenied('SCStreamErrorDomain:-3805 — internal stream error'), false)
  assert.equal(isScreenCapturePermissionDenied('Native MP4 capture timed out'), false)
  assert.equal(isScreenCapturePermissionDenied(''), false)
})
