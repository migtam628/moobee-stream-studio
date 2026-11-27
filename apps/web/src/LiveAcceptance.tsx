import {useEffect,useRef,useState} from 'react'
import {createPortal} from 'react-dom'
import {request,runningInTauri} from './api'
type Destination={id:string;name:string;serverUrl:string|null;credentialAvailable:boolean}
type Prepared={scene:string;sources:string[];audio:string[];config:Record<string,unknown>}
type Snapshot={sessionId:string;mode:string;phase:string;mediaFrames:number;sentPackets:number;elapsedMs:number;error?:string|null}
const confirmation='START REAL TEST STREAM'
export default function LiveAcceptance({active,disabled,prepare,onBusy}:{active:boolean;disabled:boolean;prepare:(seconds:number)=>Promise<Prepared>;onBusy:(busy:boolean)=>void}){
  const [available,setAvailable]=useState(false)
  const [destinations,setDestinations]=useState<Destination[]>([])
  const [selected,setSelected]=useState<string[]>([])
  const [duration,setDuration]=useState(4)
  const [fallback,setFallback]=useState(false)
  const [adaptive,setAdaptive]=useState(false)
  const [review,setReview]=useState<{prepared:Prepared;targets:Destination[];fallback:boolean;adaptive:boolean}|null>(null)
  const [phrase,setPhrase]=useState('')
  const [session,setSession]=useState<string|null>(null)
  const [status,setStatus]=useState<Snapshot|null>(null)
  const [error,setError]=useState('')
  const [loading,setLoading]=useState(false)
  const current=useRef<string|null>(null)
  const busy=useRef(false)
  const reviewGeneration=useRef(0)
  const [stopping,setStopping]=useState(false)
  useEffect(()=>{
    if(!runningInTauri())return
    let alive=true
    void request<boolean>('/studio/live-acceptance/available').then(v=>{if(alive)setAvailable(v===true)}).catch(()=>{})
    return()=>{alive=false}
  },[])
  async function refresh(){
    if(!available||busy.current)return
    setLoading(true);setError('');setReview(null);setSelected([])
    try{setDestinations(await request<Destination[]>('/studio/destinations'))}
    catch{setError('Could not load saved destinations')}
    finally{setLoading(false)}
  }
  useEffect(()=>{
    if(!session)return
    let alive=true,inflight=false
    const poll=async()=>{
      if(inflight)return
      inflight=true
      try{
        const s=await request<Snapshot|null>('/studio/live-rehearsal/status',{sessionId:session})
        if(alive&&s?.sessionId===session&&s.mode==='native')setStatus(s)
      }catch{/* Start/stop remain available even if status polling fails. */}
      finally{inflight=false}
    }
    void poll()
    const timer=setInterval(()=>void poll(),250)
    return()=>{alive=false;clearInterval(timer)}
  },[session])
  useEffect(()=>{
    if(!active){
      // Navigation discards unconfirmed intent, not an authorized native run.
      reviewGeneration.current++
      setReview(null);setPhrase('')
    }
  },[active])
  async function reviewSelection(){
    if(disabled||busy.current||!active||selected.length===0||selected.length>3)return
    const generation=++reviewGeneration.current
    setLoading(true);setError('');setPhrase('')
    try{
      const targets=destinations.filter(d=>selected.includes(d.id)&&d.credentialAvailable&&d.serverUrl)
      if(targets.length!==selected.length)throw new Error('Refresh and reselect available destinations')
      const prepared=await prepare(duration)
      if(generation===reviewGeneration.current)setReview({prepared,targets,fallback:targets.length===1&&fallback,adaptive:targets.length===1&&adaptive})
    }catch(e){setError((e as Error).message);setReview(null)}
    finally{setLoading(false)}
  }
  async function start(){
    if(!review||phrase!==confirmation||busy.current||disabled||!active)return
    const snapshot=review
    const sessionId=crypto.randomUUID()
    busy.current=true;current.current=sessionId;onBusy(true)
    setSession(sessionId);setStatus(null);setError('');setPhrase('');setReview(null);setStopping(false)
    try{
      await request('/studio/live-acceptance/start',{
        ...snapshot.prepared.config,sessionId,confirmation,fallbackOnReconnect:snapshot.fallback,adaptiveBitrate:snapshot.adaptive,
        destinations:snapshot.targets.map(d=>({id:d.id,name:d.name,serverUrl:d.serverUrl}))
      })
    }catch{setError('Native test stream did not complete successfully. Review sender health and the saved destination.')}
    finally{
      current.current=null;busy.current=false;onBusy(false);setStopping(false)
      // Fetch final state without claiming a successful receiver-side result.
      try{const s=await request<Snapshot|null>('/studio/live-rehearsal/status',{sessionId});if(!current.current&&s?.sessionId===sessionId)setStatus(s)}catch{}
    }
  }
  async function stop(){
    const sessionId=current.current
    if(!sessionId||stopping)return
    setStopping(true);setError('')
    try{if(!await request<boolean>('/studio/live-acceptance/stop',{sessionId})){if(current.current===sessionId)setError('Stop not acknowledged yet; retry if the test is still starting.');setStopping(false)}}
    catch{if(current.current===sessionId)setError('Stop request failed; retry.');setStopping(false)}
  }
  if(!available)return null
  const dock=document.getElementById('native-live-session-dock')
  const controls=<>
    {busy.current&&<button className="secondary-button" disabled={stopping} onClick={()=>void stop()}>{stopping?'Stop requested — draining':'Stop real test stream'}</button>}
    {session&&<p role="status">Native test: {status?.phase??(busy.current?'starting':'status unavailable')} · {status?.mediaFrames??0} submitted frames · {status?.sentPackets??0} sent packets. Receiver verification required.</p>}
    {error&&<p role="alert" className="preview-error">{error}</p>}
  </>
  return <section className="dock-note" aria-label="Real native streaming acceptance">
    <strong>Developer-only real stream test</strong>
    <p>Actual capture and network transmission. Use a private test destination. This is not production Go Live.</p>
    <button className="secondary-button" disabled={disabled||loading||busy.current} onClick={()=>void refresh()}>Load test destinations</button>
    {destinations.map(d=><label key={d.id} className="layer-switch"><span>{d.name}<small>{d.serverUrl??'Missing credential'}</small></span><input type="checkbox" checked={selected.includes(d.id)} disabled={disabled||loading||busy.current||!d.credentialAvailable||(!selected.includes(d.id)&&selected.length>=3)} onChange={e=>{setReview(null);setSelected(old=>e.target.checked?[...old,d.id]:old.filter(id=>id!==d.id))}}/></label>)}
    <label>Test stream duration <select aria-label="Test stream duration" value={duration} disabled={disabled||loading||busy.current} onChange={e=>{setDuration(Number(e.target.value));setReview(null);setPhrase('')}}>{[4,15,30,60,300,900,1800,3600,7200].map(s=><option key={s} value={s}>{s>=60?s/60+' min':s+' sec'}</option>)}</select></label>
    <button className="secondary-button" disabled={disabled||loading||busy.current||selected.length===0} onClick={()=>void reviewSelection()}>Review real stream test</button>
    {selected.length===1&&<label className="layer-switch"><span>Lower bitrate after reconnect <small>Optional; only a new single-destination encoder epoch</small></span><input aria-label="Lower bitrate after reconnect" type="checkbox" checked={fallback} disabled={disabled||loading||busy.current} onChange={e=>{setFallback(e.target.checked);setReview(null);setPhrase('')}}/></label>}
    {selected.length===1&&<label className="layer-switch"><span>Adaptive bitrate during stream <small>Experimental; reduce only at GOP boundaries if encoder supports it</small></span><input aria-label="Adaptive bitrate during stream" type="checkbox" checked={adaptive} disabled={disabled||loading||busy.current} onChange={e=>{setAdaptive(e.target.checked);setReview(null);setPhrase('')}}/></label>}
    {review&&<div>
      <b>Confirm the exact capture and destinations</b>
      <p>Scene: {review.prepared.scene} · {String(review.prepared.config.seconds)}s · {String(review.prepared.config.profile)}</p>
      <p>Sources: {review.prepared.sources.join(', ')}</p>
      <p>Audio: {review.prepared.audio.join(', ')||'None'}</p>
      <p>Bitrate fallback: {review.fallback?'Enabled — lower preset after a classified transport retry':'Disabled'}</p>
      <p>In-place bitrate reduction: {review.adaptive?'Enabled — experimental single-output downshift':'Disabled'}</p>
      {review.targets.map(d=><p key={d.id}>{d.name} — {d.serverUrl}</p>)}
      <label>Type {confirmation}<input aria-label="Real stream confirmation" value={phrase} onChange={e=>setPhrase(e.target.value)} autoComplete="off"/></label>
      <button className="secondary-button" disabled={disabled||phrase!==confirmation} onClick={()=>void start()}>Start real test stream</button>
      <button className="secondary-button" onClick={()=>{setReview(null);setPhrase('')}}>Cancel review</button>
    </div>}
    {active&&controls}
    {!active&&dock&&session&&createPortal(<section className="dock-note" aria-label="Active native stream controls"><strong>Native stream test</strong><p>Workspace navigation does not end an authorized test. Use Stop to end it.</p>{controls}</section>,dock)}
  </section>
}
