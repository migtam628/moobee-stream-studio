import {useEffect,useState} from 'react'
import {request,runningInTauri} from './api'
import {broadcastIndicator,type BroadcastIndicator} from './nativeBroadcastStatus.mjs'
import type {StudioStatus} from './types'

function formatElapsed(seconds:number){
  const s=Math.floor(seconds)
  return [Math.floor(s/3600),Math.floor(s/60)%60,s%60].map(v=>String(v).padStart(2,'0')).join(':')
}
export default function NativeBroadcastFooter({status}:{status:StudioStatus}){
  const native=runningInTauri()
  const [indicator,setIndicator]=useState<BroadcastIndicator|null>(()=>native?broadcastIndicator(null,'loading'):null)
  useEffect(()=>{
    if(!native)return
    let alive=true,inflight=false
    async function poll(){
      if(inflight)return
      inflight=true
      try{
        const snapshot=await request<unknown>('/studio/live-health',{})
        if(alive)setIndicator(broadcastIndicator(snapshot,'available'))
      }catch{if(alive)setIndicator(broadcastIndicator(null,'unavailable'))}
      finally{inflight=false}
    }
    void poll()
    const timer=setInterval(()=>void poll(),500)
    return()=>{alive=false;clearInterval(timer)}
  },[native])
  return <>
    <div className={`health ${indicator?'':status.connected?'healthy':''}`} aria-label="Broadcast status" role="status"><span className="status-dot"/>{indicator?.label??(status.connected?(status.streaming?'Live Broadcast':'Scene Editor Ready · No Live Output'):'Local engine offline')}</div>
    <div className="footer-data">
      <span>Encoder <b>{indicator?.active?'Native test':indicator?.uncertain?'—':'In development'}</b></span>
      <span>Bitrate <b>{indicator?(indicator.kbps===null?'—':indicator.kbps+' kbps sender'):(status.bitrateKbps===null?'—':status.bitrateKbps+' kbps')}</b></span>
      <span>Uptime <b>{indicator?(indicator.seconds===null?'—':formatElapsed(indicator.seconds)):formatElapsed(status.streamSeconds)}</b></span>
      {indicator?<span>Late frames <b>{indicator.late??'—'}</b></span>:<span>Dropped <b>{status.droppedFrames??'—'}</b></span>}
      {indicator?.outputs!=null&&<span>Outputs <b>{indicator.outputs}</b></span>}
    </div>
  </>
}
