// Explicit numeric/enum allowlist. Never serialize raw native snapshots.
export const HISTORY_LIMIT = 240
export const SAMPLE_INTERVAL_MS = 10000
const phases = new Set(['starting','streaming','reconnecting','stopping','stopped','failed'])
const healthStates = new Set(['unavailable','warmingUp','normal','backpressure','stalled','stale','ended'])
const destinationPhases = new Set(['starting','connecting','sending','reconnecting','stopping','stopped','failed','complete','draining','waiting','awaitingKey','closed'])
const integer = (v, signed = false) => Number.isSafeInteger(v) && (signed || v >= 0) ? v : null
const pick = (obj, keys, signed = []) => Object.fromEntries(keys.map(key => [key, integer(obj?.[key], signed.includes(key))]))
export function sanitizeLiveSample(value) {
  if (!value || value.mode !== 'native' || !phases.has(value.phase) || integer(value.elapsedMs) === null) return null
  return {
    mode:'native', phase:value.phase, elapsedMs:value.elapsedMs,
    ...pick(value,['sentPackets','sentPayloadBytes','mediaFrames','reconnectAttempt']),
    health:{health:healthStates.has(value.health?.health)?value.health.health:'unavailable',
      ...pick(value.health,['payloadKbps','outputFpsTenths','queueBytes','queuePackets','lateFramesInWindow','sampleAgeMs'])},
    resources:pick(value.resources,['samples','sampleElapsedMs','residentBytes','baselineResidentBytes','peakResidentBytes','cpuTenths','peakCpuTenths']),
    timing:pick(value.timing,['videoPackets','audioPackets','videoEndMs','audioEndMs','encodedVideoLeadMs','maxAbsEncodedGapMs','pendingAudioFrames','pcmFramesSupplied','windowsMissingSourceFrames','maxEncodeMs'],['encodedVideoLeadMs']),
    bitrateAdvice:{...pick(value.bitrateAdvice,['initialKbps','currentKbps','suggestedKbps','pressureMs']),
      fallbackEnabled:typeof value.bitrateAdvice?.fallbackEnabled==='boolean'?value.bitrateAdvice.fallbackEnabled:null},
    destinations:Array.isArray(value.destinations)?value.destinations.slice(0,3).map(d=>({
      phase:destinationPhases.has(d?.phase)?d.phase:'unknown',
      ...pick(d,['sentPackets','sentPayloadBytes','queuedPackets','queuedBytes','admittedPackets','admittedBytes','reconnectAttempt']),
      interrupted:typeof d?.interrupted==='boolean'?d.interrupted:null
    })):[]
  }
}
export function appendLiveSample(history, snapshot) {
  const next = sanitizeLiveSample(snapshot)
  if (!next) return history
  const last = history.at(-1)
  if (last && next.elapsedMs < last.elapsedMs) return history
  if (last && next.elapsedMs-last.elapsedMs < SAMPLE_INTERVAL_MS && next.phase===last.phase &&
      next.reconnectAttempt===last.reconnectAttempt && next.bitrateAdvice.currentKbps===last.bitrateAdvice.currentKbps &&
      next.health.health===last.health.health && JSON.stringify(next.destinations.map(d=>[d.phase,d.reconnectAttempt,d.interrupted]))===JSON.stringify(last.destinations.map(d=>[d.phase,d.reconnectAttempt,d.interrupted]))) return history
  return [...history.slice(1-HISTORY_LIMIT),next]
}
export function readLiveHistory(raw) {
  if (typeof raw !== 'string' || raw.length>1024*1024) return []
  try {
    const data=JSON.parse(raw)
    if (data?.schemaVersion!==1 || !Array.isArray(data.samples)) return []
    return data.samples.slice(-HISTORY_LIMIT).map(sanitizeLiveSample).filter(Boolean)
  } catch { return [] }
}
export function liveHistoryReport(samples) {
  return JSON.stringify({schemaVersion:1,kind:'native-sender-diagnostics',receiverVerified:false,
    note:'Local sampled sender observations only; counters may reset at reconnect; not hardware or receiver acceptance.',
    samples:samples.slice(-HISTORY_LIMIT).map(sanitizeLiveSample).filter(Boolean)},null,2)
}
