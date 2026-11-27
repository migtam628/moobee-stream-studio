import {useEffect,useState} from 'react'
import {request,runningInTauri} from './api'

type Health='unavailable'|'warmingUp'|'normal'|'backpressure'|'stalled'|'stale'|'ended'
type Destination={id:string;phase:string;sentPackets:number;sentPayloadBytes:number;queuedPackets:number;queuedBytes:number;admittedPackets:number;admittedBytes:number;reconnectAttempt?:number;interrupted?:boolean}
type Resources={samples:number;sampleElapsedMs:number;residentBytes:number|null;baselineResidentBytes:number|null;peakResidentBytes:number|null;cpuTenths:number|null;peakCpuTenths:number|null}
type Timing={videoPackets:number;audioPackets:number;videoEndMs:number|null;audioEndMs:number|null;encodedVideoLeadMs:number|null;maxAbsEncodedGapMs:number|null;pendingAudioFrames:number|null;pcmFramesSupplied:number;windowsMissingSourceFrames:number|null;maxEncodeMs:number}
type BitrateAdvice={initialKbps:number|null;currentKbps:number|null;fallbackEnabled:boolean;suggestedKbps:number|null;pressureMs:number}
type Snapshot={bitrateAdvice?:BitrateAdvice;resources?:Resources;timing?:Timing|null;destinations?:Destination[];mode:'native'|'rehearsal';phase:string;elapsedMs:number;sentPackets:number;sentPayloadBytes:number;mediaFrames:number;reconnectAttempt?:number;health:{health:Health;payloadKbps:number|null;outputFpsTenths:number|null;queueBytes:number;queuePackets:number;lateFramesInWindow:number|null;sampleAgeMs:number|null}}
const labels:Record<Health,string>={unavailable:'Unavailable',warmingUp:'Collecting samples',normal:'Sender normal',backpressure:'Queue pressure',stalled:'Sender stalled',stale:'Stale sample',ended:'Ended'}
export default function LiveHealth(){
  const native=runningInTauri()
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null)
  const [error,setError]=useState(false)
  useEffect(()=>{
    if(!native)return
    let active=true,inflight=false
    const poll=async()=>{
      if(inflight)return
      inflight=true
      try{
        const result=await request<Snapshot|null>('/studio/live-health',{})
        if(active){setSnapshot(result);setError(false)}
      }catch{if(active){setSnapshot(null);setError(true)}}
      finally{inflight=false}
    }
    void poll()
    const timer=setInterval(()=>void poll(),500)
    return()=>{active=false;clearInterval(timer)}
  },[native])
  const h=snapshot?.mode==='native'?snapshot.health:null
  const advice=snapshot?.mode==='native'?snapshot.bitrateAdvice:null
  const r=snapshot?.mode==='native'?snapshot.resources:null
  const t=snapshot?.mode==='native'?snapshot.timing:null
  const mib=(bytes:number|null|undefined)=>bytes==null?'—':(bytes/1048576).toFixed(1)+' MiB'
  const percent=(tenths:number|null|undefined)=>tenths==null?'—':(tenths/10).toFixed(1)+'%'
  const ms=(value:number|null|undefined)=>value==null?'—':value+' ms'
  const resourceMetrics=[
    ['Process CPU',percent(r?.cpuTenths)],['Peak sampled process CPU',percent(r?.peakCpuTenths)],
    ['Resident memory',mib(r?.residentBytes)],['Baseline resident memory',mib(r?.baselineResidentBytes)],
    ['Peak sampled resident memory',mib(r?.peakResidentBytes)],
    ['Resource samples',r?.samples?String(r.samples):'—'],
    ['Resource sample age',r?.samples?ms(Math.max(0,(snapshot?.elapsedMs??0)-r.sampleElapsedMs)):'—'],
  ]
  const timingMetrics=[
    ['Encoded video packets',t?String(t.videoPackets):'—'],['Encoded AAC packets',t?String(t.audioPackets):'—'],
    ['Encoded video lead',ms(t?.encodedVideoLeadMs)],['Maximum absolute encoded A/V gap',ms(t?.maxAbsEncodedGapMs)],
    ['PCM frames supplied',t?String(t.pcmFramesSupplied):'—'],
    ['Pending encoder audio frames',t?.pendingAudioFrames==null?'—':String(t.pendingAudioFrames)],
    ['Windows missing source frames',t?.windowsMissingSourceFrames==null?'—':String(t.windowsMissingSourceFrames)],
    ['Slowest measured encode tick',ms(t?.maxEncodeMs)]
  ]
  const metrics=[
    ['Rolling payload bitrate',h?.payloadKbps==null?'—':h.payloadKbps+' kbps'],
    ['Submitted video frames / sec',h?.outputFpsTenths==null?'—':(h.outputFpsTenths/10).toFixed(1)],
    ['Queued packets',h==null?'—':String(h.queuePackets)],
    ['Queued payload',h==null?'—':(h.queueBytes/1048576).toFixed(2)+' MiB'],
    ['Late frames in sample window',h?.lateFramesInWindow==null?'—':String(h.lateFramesInWindow)],
    ['Sample age',h?.sampleAgeMs==null?'—':h.sampleAgeMs+' ms'],
    ['Epoch sent payload',h==null?'—':((snapshot?.sentPayloadBytes??0)/1048576).toFixed(2)+' MiB'],
    ['Epoch sent packets',h==null?'—':String(snapshot?.sentPackets??0)],
    ['Reconnect attempt',h==null?'—':String(snapshot?.reconnectAttempt??0)]
  ]
  return <section className="panel" aria-label="Native streaming health">
    <div className="panel-heading"><h2>Native streaming health</h2></div>
    <div style={{padding:'16px'}}>
      <p className="dock-note">Sender-side measurements only. These are not viewer reception, network packet loss, or decoder verification.</p>
      <p role="status">{!native?'Desktop application required':error?'Health status unavailable':snapshot?.mode==='rehearsal'?'Offline rehearsal — no network measurements':snapshot?.phase==='reconnecting'?'Waiting to reconnect — previous epoch ended':h?labels[h.health]:'No native publishing session'}</p>
      <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(180px,1fr))',gap:'12px'}}>
        {metrics.map(([label,value])=><div className="dock-note" key={label}><span>{label}</span><br/><strong>{value}</strong></div>)}
      </div>
      <h3>Bitrate recovery</h3>
      <p className="dock-note">Initial target: {advice?.initialKbps??'—'} kbps · Current target: {advice?.currentKbps??'—'} kbps · Retry fallback: {advice==null?'—':advice.fallbackEnabled?'Enabled':'Disabled'}</p>
      <p className="dock-note">{advice?.suggestedKbps!=null&&snapshot?.phase==='streaming'? `Sustained sender queue pressure: consider ${advice.suggestedKbps} kbps for a future session.`:'No active bitrate recommendation.'} A recommendation does not change a running encoder or measure network capacity.</p>
      <h3>Process resources</h3>
      <p className="dock-note">Native process only, sampled about once per second. CPU uses 100% per logical core; browser/WebView helper processes, GPU usage and unsampled peaks are not included. Ended sessions show their last sample.</p>
      <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(180px,1fr))',gap:'12px'}}>{resourceMetrics.map(([label,value])=><div className="dock-note" key={label}><span>{label}</span><br/><strong>{value}</strong></div>)}</div>
      <h3>Encoded A/V timing</h3>
      <p className="dock-note">Approximate packet-end difference; positive means video leads audio. This is not hardware clock drift or receiver lip-sync. Timing counters reset on whole-encoder restart.</p>
      <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(180px,1fr))',gap:'12px'}}>{timingMetrics.map(([label,value])=><div className="dock-note" key={label}><span>{label}</span><br/><strong>{value}</strong></div>)}</div>
      {snapshot?.mode==='native'&&!!snapshot.destinations?.length&&<div aria-label="Per-destination publishing health" style={{marginBlock:'16px'}}><h3>Destination output</h3>{snapshot.destinations.map((d,i)=><div className="dock-note" key={d.id} style={{marginBlock:'8px'}}><strong>{d.id==='single'?'Single destination':`Destination ${i+1} · ${d.id.slice(0,8)}`}</strong> · {d.phase} · Retry {d.reconnectAttempt??0}{d.interrupted?' · Delivery interrupted':''}<br/>{d.sentPackets} of {d.admittedPackets} admitted packets sent · {(d.sentPayloadBytes/1048576).toFixed(2)} MiB sent · {d.queuedPackets} queued</div>)}<p className="dock-note">A failed destination does not automatically stop the remaining workers. Aggregate bitrate is the sum of native sender payloads, not individual stream bitrate.</p></div>}
      <p className="dock-note">Actual Go Live and native recovery remain gated pending secure destination authorization and receiving-side acceptance. Counters reset for each new publishing epoch.</p>
    </div>
  </section>
}
