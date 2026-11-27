import {useEffect,useRef,useState} from 'react'
import {request,runningInTauri} from './api'
type Snapshot={sessionId:string;mode:'rehearsal'|'native';phase:'starting'|'streaming'|'stopping'|'stopped'|'failed';elapsedMs:number;mediaFrames:number;sentPackets:number;sentPayloadBytes:number;pendingPackets:number;pendingBytes:number;lateFrames:number;bitrateKbps:number;error?:string|null}
const terminal=(s:Snapshot|null)=>s?.phase==='stopped'||s?.phase==='failed'
export default function LiveLifecycle(){
  const [id,setId]=useState<string|null>(null)
  const [snapshot,setSnapshot]=useState<Snapshot|null>(null)
  const [error,setError]=useState('')
  const [pending,setPending]=useState(false)
  const current=useRef<string|null>(null)
  const busy=useRef(false)
  const supported=runningInTauri()
  useEffect(()=>{
    if(!id||terminal(snapshot))return
    let active=true,inflight=false
    const poll=async()=>{
      if(inflight)return
      inflight=true
      try{
        const s=await request<Snapshot|null>('/studio/live-rehearsal/status',{sessionId:id})
        if(active&&s&&s.sessionId===id&&s.mode==='rehearsal'){setSnapshot(s);setError('')}
      }catch{if(active)setError('Could not refresh rehearsal status')}
      finally{inflight=false}
    }
    void poll()
    const timer=setInterval(()=>void poll(),250)
    return()=>{active=false;clearInterval(timer)}
  },[id,snapshot?.phase==='stopped'||snapshot?.phase==='failed'])
  useEffect(()=>()=>{const sessionId=current.current;if(sessionId)void request('/studio/live-rehearsal/stop',{sessionId}).catch(()=>{})},[])
  async function start(){
    if(busy.current||!supported)return
    busy.current=true;setPending(true);setError('')
    const sessionId=crypto.randomUUID();current.current=sessionId
    try{await request('/studio/live-rehearsal/start',{sessionId,seconds:15});setSnapshot(null);setId(sessionId)}
    catch{current.current=null;setError('Could not start offline rehearsal')}
    finally{busy.current=false;setPending(false)}
  }
  async function stop(){
    if(!id||busy.current)return
    busy.current=true;setPending(true)
    try{await request('/studio/live-rehearsal/stop',{sessionId:id})}
    catch{setError('Could not stop offline rehearsal')}
    finally{busy.current=false;setPending(false)}
  }
  if(!supported)return null
  const active=!!id&&!terminal(snapshot)
  return <div className="dock-note" aria-label="Offline stream lifecycle rehearsal">
    <b>Streaming lifecycle rehearsal</b>
    <p>Offline controls only — no screen, audio, media packets, credentials or network connection. Not a live broadcast.</p>
    <button className="secondary-button" disabled={pending||active} onClick={()=>void start()}>Rehearse Go Live</button>
    <button className="secondary-button" disabled={pending||!active||snapshot?.phase==='stopping'} onClick={()=>void stop()}>Stop rehearsal</button>
    {id&&<div role="status">Rehearsal: {snapshot?.phase==='streaming'?'running (offline)':snapshot?.phase??'starting'} · {((snapshot?.elapsedMs??0)/1000).toFixed(1)}s · {snapshot?.sentPackets??0} network packets</div>}
    {error&&<small className="preview-error" role="alert">{error}</small>}
  </div>
}
