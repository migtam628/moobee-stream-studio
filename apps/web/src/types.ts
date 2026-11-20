export type ObsStatus = {
  connected: boolean
  address: string
  currentScene: string
  previewScene: string
  studioMode: boolean
  scenes: string[]
  sources: { id: number; name: string; enabled: boolean; kind: string }[]
  inputs: { name: string; kind: string; muted: boolean; volume: number }[]
  streaming: boolean
  recording: boolean
  streamSeconds: number
  bitrateKbps: number | null
  droppedFrames: number | null
  fps: number | null
  lastError: string
}

export type Activity = { id: string; type: string; message: string; time: number }
export type Alert = { id: string; type: string; name: string; message: string; time: number }

export const disconnected: ObsStatus = {
  connected: false, address: 'ws://127.0.0.1:4455', currentScene: '', previewScene: '', studioMode: false, scenes: [], sources: [], inputs: [],
  streaming: false, recording: false, streamSeconds: 0, bitrateKbps: null,
  droppedFrames: null, fps: null, lastError: ''
}
