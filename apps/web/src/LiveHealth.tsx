import {useEffect,useState} from 'react'
import {request,runningInTauri} from './api'

type Health='unavailable'|'warmingUp'|'normal'|'backpressure'|'stalled'|'stale'|'ended'
type Snapshot={mode:'native'|'rehearsal';phase:string;elapsedMs:number;sentPackets:number;sentPayloadBytes:number;mediaFrames:number;health:{health:Health;payloadKbps:number|null;outputFpsTenths:number|null;queueBytes:number;queuePackets:number;lateFramesInWindow:number|null;sampleAgeMs:number|null}}
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
  const metrics=[
    ['Rolling payload bitrate',h?.payloadKbps==null?'—':h.payloadKbps+' kbps'],
    ['Submitted video frames / sec',h?.outputFpsTenths==null?'—':(h.outputFpsTenths/10).toFixed(1)],
    ['Queued packets',h==null?'—':String(h.queuePackets)],
    ['Queued payload',h==null?'—':(h.queueBytes/1048576).toFixed(2)+' MiB'],
    ['Late frames in sample window',h?.lateFramesInWindow==null?'—':String(h.lateFramesInWindow)],
    ['Sample age',h?.sampleAgeMs==null?'—':h.sampleAgeMs+' ms'],
    ['Total sent payload',h==null?'—':((snapshot?.sentPayloadBytes??0)/1048576).toFixed(2)+' MiB'],
    ['Total sent packets',h==null?'—':String(snapshot?.sentPackets??0)]
  ]
  return <section className="panel" aria-label="Native streaming health">
    <div className="panel-heading"><h2>Native streaming health</h2></div>
    <div style={{padding:'16px'}}>
      <p className="dock-note">Sender-side measurements only. These are not viewer reception, network packet loss, or decoder verification.</p>
      <p role="status">{!native?'Desktop application required':error?'Health status unavailable':snapshot?.mode==='rehearsal'?'Offline rehearsal — no network measurements':h?labels[h.health]:'No native publishing session'}</p>
      <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(180px,1fr))',gap:'12px'}}>
        {metrics.map(([label,value])=><div className="dock-note" key={label}><span>{label}</span><br/><strong>{value}</strong></div>)}
      </div>
      <p className="dock-note">Actual Go Live and automatic reconnection remain gated pending destination authorization and receiving-side acceptance.</p>
    </div>
  </section>
}
