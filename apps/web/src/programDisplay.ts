/**
 * Resolve the selected Program scene's one enabled display source. In Studio
 * Mode the preview scene is intentionally different: recording must never
 * accidentally capture the Preview source as though it were Program output.
 *
 * This is only a source-selection convenience for the direct-display test
 * recorder, NOT a scene compositor. Multiple sources must not be silently
 * reduced to one.
 */
export type MinimalSceneCollection = {
  currentScene: string
  previewScene?: string
  scenes: { name: string; sources: {
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
