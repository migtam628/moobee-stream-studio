/**
 * Resolve the selected Program scene's one enabled display source. In Studio
 * Mode the preview scene is intentionally different: recording must never
 * accidentally capture the Preview source as though it were Program output.
 *
 * This is only a source-selection convenience for the direct-display test
 * recorder, NOT a scene compositor. Multiple sources must not be silently
 * reduced to one.
 */
export type SceneLayout = 'grid' | 'pip'

export function validateSceneLayout(value: unknown): SceneLayout {
  if (value === undefined || value === 'grid') return 'grid'
  if (value === 'pip') return 'pip'
  throw new Error('Unsupported native scene layout')
}

export type MinimalSceneCollection = {
  currentScene: string
  previewScene?: string
  scenes: { name: string; layout?: SceneLayout; sources: {
    kind: string; deviceId: string; enabled: boolean
  }[] }[]
}

export function singleProgramDisplayId(collection: unknown): number {
  if (!collection || typeof collection !== 'object') {
    throw new Error('No current Program scene is available')
  }
  const data = collection as Partial<MinimalSceneCollection>
  if (typeof data.currentScene !== 'string' || !Array.isArray(data.scenes)) {
    throw new Error('Invalid Program scene state')
  }
  const program = data.scenes.find(scene => scene?.name === data.currentScene)
  if (!program || !Array.isArray(program.sources)) {
    throw new Error('The current Program scene could not be found')
  }
  const active = program.sources.filter(source => source?.enabled)
  if (active.length !== 1 || active[0].kind !== 'display') {
    throw new Error('The direct-display recorder requires exactly one enabled display source in the current Program scene. Multiple sources need the future scene compositor.')
  }
  if (typeof active[0].deviceId !== 'string' || !/^\d{1,4}$/.test(active[0].deviceId)) {
    throw new Error('The Program display identifier is invalid')
  }
  const id = Number(active[0].deviceId)
  if (!Number.isSafeInteger(id)) throw new Error('Invalid Program display selection')
  return id
}

/** Resolve up to four enabled display sources from the selected Program scene.
 * This is a bounded silent AVI diagnostic; it does not record webcam, alerts,
 * microphone or synchronized audio. Never capture the Studio Preview scene.
 */
export function programTestDisplayIds(collection: unknown): number[] {
  if (!collection || typeof collection !== 'object') {
    throw new Error('No current Program scene is available')
  }
  const state = collection as Partial<MinimalSceneCollection>
  if (typeof state.currentScene !== 'string' || !Array.isArray(state.scenes)) {
    throw new Error('Invalid Program scene state')
  }
  const program = state.scenes.find(scene => scene?.name === state.currentScene)
  if (!program || !Array.isArray(program.sources)) {
    throw new Error('The selected Program scene could not be found')
  }
  const enabled = program.sources.filter(source => source?.enabled)
  if (enabled.length < 1 || enabled.length > 4) {
    throw new Error('Composited AVI test requires one to four enabled display sources in Program')
  }
  if (enabled.some(source => source.kind !== 'display')) {
    throw new Error('Composited AVI test supports displays only; camera, overlays and audio are not recorded')
  }
  const ids = enabled.map(source => {
    if (typeof source.deviceId !== 'string' || !/^(0|[1-9]\d{0,1})$/.test(source.deviceId)) {
      throw new Error('Invalid Program display identifier')
    }
    const id = Number(source.deviceId)
    if (!Number.isSafeInteger(id) || id > 24) {
      throw new Error('Invalid Program display identifier')
    }
    return id
  })
  if (new Set(ids).size !== ids.length) {
    throw new Error('A display may appear only once in the composed AVI test')
  }
  return ids
}

/** The layout belongs to Program, never the independently edited Preview. */
export function programTestLayout(collection: unknown): SceneLayout {
  programTestDisplayIds(collection)
  const state = collection as MinimalSceneCollection
  return validateSceneLayout(state.scenes.find(scene => scene.name === state.currentScene)?.layout)
}

/** Stable native device IDs are not numeric monitor-list indices. */
export function validVideoInputId(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 1024 && !/[\x00-\x1f\x7f-\x9f]/.test(value)
}

export function programVideoInput(collection: unknown): string | undefined {
  const state = collection as MinimalSceneCollection | null
  const scene = state?.scenes?.find(scene => scene.name === state.currentScene)
  const enabled = scene?.sources?.filter(source => source.enabled) ?? []
  const video = enabled.filter(source => source.kind === 'video')
  if (!video.length) return undefined
  if (enabled.length !== 1 || video.length !== 1 || !validVideoInputId(video[0].deviceId)) {
    throw new Error('For this Mac video-input test, enable only one capture card or camera in Program; hide other sources.')
  }
  return video[0].deviceId
}
