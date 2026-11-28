// Never infer receiver success or production live state from sender counters.
const active = new Set(['starting','streaming','reconnecting','stopping'])
const count = v=>Number.isSafeInteger(v)&&v>=0?v:null
export function broadcastIndicator(snapshot, state){
  if(state==='loading')return {label:'Checking native broadcast status',active:false,uncertain:true,seconds:null,kbps:null,late:null,outputs:null}
  if(state==='unavailable')return {label:'Native broadcast status unavailable',active:false,uncertain:true,seconds:null,kbps:null,late:null,outputs:null}
  if(snapshot?.mode!=='native'||!active.has(snapshot.phase))return null
  const labels={starting:'Native test · preparing / authorizing',streaming:'Native test · transmitting',reconnecting:'Native test · reconnecting',stopping:'Native test · stopping / draining'}
  const h=snapshot.health
  const validRate=snapshot.phase==='streaming'&&['normal','backpressure','stalled'].includes(h?.health)
  const rows=Array.isArray(snapshot.destinations)?snapshot.destinations:null
  return {label:labels[snapshot.phase],active:true,uncertain:false,seconds:count(snapshot.elapsedMs)===null?null:Math.floor(snapshot.elapsedMs/1000),kbps:validRate?count(h.payloadKbps):null,late:count(snapshot.lateFrames),outputs:rows?.length?rows.filter(d=>['sending','authorized','reconnecting'].includes(d.phase)).length:null}
}
