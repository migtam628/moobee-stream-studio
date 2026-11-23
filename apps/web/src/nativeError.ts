/**
 * Tauri command rejection values are not guaranteed to be Error instances.
 * In particular, a Rust Result<T, String> can reject invoke() with a plain
 * string. Never assume an unknown caught value has a .message property.
 */
export function nativeErrorMessage(value: unknown): string {
  if (typeof value === 'string') {
    return value.trim().slice(0, 1500) || 'Native command failed without an explanation'
  }
  if (value instanceof Error) {
    return value.message.trim().slice(0, 1500) || value.name || 'Native command failed'
  }
  if (value && typeof value === 'object') {
    const item = value as Record<string, unknown>
    for (const key of ['message', 'error', 'reason', 'details', 'description']) {
      if (typeof item[key] === 'string' && item[key].trim()) {
        return item[key].trim().slice(0, 1500)
      }
    }
    if (typeof item.code === 'string' || typeof item.code === 'number') {
      return `Native command failed (code: ${String(item.code).slice(0, 100)})`
    }
  }
  return 'Native command failed without details. Check Settings → Runtime Service Diagnostics.'
}
