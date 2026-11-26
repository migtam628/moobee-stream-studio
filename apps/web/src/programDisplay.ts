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
    kind: string; deviceId: string; enabled: boolean; transform?:SourceTransform
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
    throw new Error('For this native video-input test, enable only one capture card or camera in Program; hide other sources.')
  }
  return video[0].deviceId
}

export type SourceFilters = {brightness:number;contrast:number;saturation:number;opacity:number;greenKey:boolean}
export const defaultFilters = ():SourceFilters => ({brightness:0,contrast:100,saturation:100,opacity:100,greenKey:false})
export function validateFilters(value:unknown):SourceFilters {
  if(!value || typeof value !== 'object' || Array.isArray(value))throw new Error('Invalid source filters')
  const f=value as SourceFilters
  if(!Number.isInteger(f.brightness)||f.brightness < -100||f.brightness > 100||
    !Number.isInteger(f.contrast)||f.contrast<0||f.contrast>200||
    !Number.isInteger(f.saturation)||f.saturation<0||f.saturation>200||
    !Number.isInteger(f.opacity)||f.opacity<0||f.opacity>100||typeof f.greenKey!=='boolean')throw new Error('Invalid source filter bounds')
  return {brightness:f.brightness,contrast:f.contrast,saturation:f.saturation,opacity:f.opacity,greenKey:f.greenKey}
}
export type SourceTransform = { x:number; y:number; width:number; height:number; cropLeft:number; cropTop:number; cropRight:number; cropBottom:number; rotation:number; fit:boolean; filters?:SourceFilters }
export const fullTransform = ():SourceTransform => ({x:0,y:0,width:10000,height:10000,cropLeft:0,cropTop:0,cropRight:0,cropBottom:0,rotation:0,fit:true})
export function validateTransform(value:unknown):SourceTransform {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid source transform')
  const t=value as SourceTransform
  const values=[t.x,t.y,t.width,t.height,t.cropLeft,t.cropTop,t.cropRight,t.cropBottom,t.rotation]
  if(values.some(v=>!Number.isInteger(v)||v<0)||t.width<250||t.height<250||t.x+t.width>10000||t.y+t.height>10000||
    t.cropLeft+t.cropRight>9000||t.cropTop+t.cropBottom>9000||![0,90,180,270].includes(t.rotation)||typeof t.fit!=='boolean') throw new Error('Invalid source transform bounds')
  return {x:t.x,y:t.y,width:t.width,height:t.height,cropLeft:t.cropLeft,cropTop:t.cropTop,cropRight:t.cropRight,cropBottom:t.cropBottom,rotation:t.rotation,fit:t.fit,...(t.filters===undefined?{}:{filters:validateFilters(t.filters)})}
}
export function defaultTransform(index:number,count:number,layout:SceneLayout):SourceTransform {
  const t=fullTransform()
  if(count===1)return t
  if(layout==='pip'){
    if(index===0)return t
    return {...t,x:6526,y:10000-250-index*2250-(index-1)*250,width:3333,height:2250}
  }
  const cols=count===1?1:2,rows=count<=2?1:2
  return {...t,x:index%cols*5000,y:Math.floor(index/cols)*10000/rows,width:10000/cols,height:10000/rows}
}
export function sceneTransforms(sources:{enabled:boolean;transform?:SourceTransform}[],layout:SceneLayout):SourceTransform[]|undefined{
  const active=sources.filter(s=>s.enabled)
  if(!active.some(s=>s.transform))return undefined
  if(active.length>4)throw new Error('Manual transforms support up to four enabled sources')
  return active.map((s,i)=>s.transform?validateTransform(s.transform):defaultTransform(i,active.length,layout))
}
export function programTransforms(collection:unknown):SourceTransform[]|undefined{
  const data=collection as MinimalSceneCollection
  const scene=data.scenes.find(s=>s.name===data.currentScene)
  if(!scene)throw new Error('Program scene not found')
  return sceneTransforms(scene.sources,validateSceneLayout(scene.layout))
}
