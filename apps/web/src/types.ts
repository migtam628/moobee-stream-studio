export type StudioStatus = {
  connected: boolean
  address: string
  engine: string
  captureAvailable: boolean
  recordAvailable: boolean
  broadcastAvailable: boolean
  currentScene: string
  previewScene: string
  studioMode: boolean
  scenes: string[]
  sources: { id: number; name: string; enabled: boolean; kind: string; deviceId?: string }[]
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
export const offline: StudioStatus = {
  connected: false, address: 'local', engine: 'moobee-native-alpha',
  captureAvailable: false, recordAvailable: false, broadcastAvailable: false,
  currentScene: '', previewScene: '', studioMode: false,
  scenes: [], sources: [], inputs: [], streaming: false, recording: false,
  streamSeconds: 0, bitrateKbps: null, droppedFrames: null, fps: null, lastError: ''
}
