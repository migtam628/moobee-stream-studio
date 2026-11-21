import { isTauri } from '@tauri-apps/api/core'
import { nativeRequest } from './native'
import type { Activity, Alert, StudioStatus } from './types'

export function runningInTauri(): boolean { return isTauri() }
export const isStaticPreview = import.meta.env.VITE_STATIC_PREVIEW === 'true'

export async function request<T>(path: string, body?: object): Promise<T> {
  if (isStaticPreview) throw new Error('The hosted preview has no device capture. Install MooBee Studio for native capture.')
  if (runningInTauri()) return await nativeRequest(path, body as Record<string, unknown> | undefined) as T
  const response = await fetch(`/api${path}`, {
    method: body ? 'POST' : 'GET',
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined
  })
  const data = await response.json()
  if (!response.ok) throw new Error(data.error || data.lastError || 'Request failed')
  return data as T
}

type EventCallbacks = { status: (data: StudioStatus) => void; activity: (data: Activity) => void; alert: (data: Alert) => void }
export function subscribeEvents(callbacks: EventCallbacks): () => void {
  if (isStaticPreview) return () => {}
  if (runningInTauri()) {
    const handlers = Object.keys(callbacks).map((name) => {
      const listener: EventListener = (event) => {
        const detail = (event as CustomEvent).detail
        if (name === 'status') callbacks.status(detail as StudioStatus)
        else if (name === 'activity') callbacks.activity(detail as Activity)
        else if (name === 'alert') callbacks.alert(detail as Alert)
      }
      window.addEventListener(`moobee:${name}`, listener)
      return { name, listener }
    })
    return () => handlers.forEach(({ name, listener }) => window.removeEventListener(`moobee:${name}`, listener))
  }
  const source = new EventSource('/api/events')
  source.addEventListener('status', (event) => callbacks.status(JSON.parse((event as MessageEvent).data)))
  source.addEventListener('activity', (event) => callbacks.activity(JSON.parse((event as MessageEvent).data)))
  source.addEventListener('alert', (event) => callbacks.alert(JSON.parse((event as MessageEvent).data)))
  return () => source.close()
}
