/** ScreenCaptureKit's -3801 is a TCC screen capture denial, not an encoder error. */
export function isScreenCapturePermissionDenied(message: string): boolean {
  return /SCStreamErrorDomain\s*:\s*-3801\b|\buser declined TCCs\b|\bSCStreamErrorUserDeclined\b/i.test(message)
}
