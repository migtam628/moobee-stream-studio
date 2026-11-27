export const HISTORY_LIMIT: number
export const SAMPLE_INTERVAL_MS: number
export type LiveSample = {mode:'native';phase:string;elapsedMs:number;reconnectAttempt:number|null;[key:string]:unknown}
export function sanitizeLiveSample(value:unknown):LiveSample|null
export function appendLiveSample(history:LiveSample[],snapshot:unknown):LiveSample[]
export function readLiveHistory(raw:string|null):LiveSample[]
export function liveHistoryReport(samples:LiveSample[]):string
