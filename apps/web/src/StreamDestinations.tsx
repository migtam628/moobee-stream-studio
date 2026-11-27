import {useCallback,useEffect,useRef,useState,type FormEvent} from 'react'
import {isStaticPreview,request,runningInTauri} from './api'
type Destination={id:string;name:string;serverUrl:string|null;credentialAvailable:boolean}
export default function StreamDestinations(){
  const supported=runningInTauri()&&!isStaticPreview&&/Macintosh|Mac OS X|Windows/i.test(navigator.userAgent)
  const [rows,setRows]=useState<Destination[]>([])
  const [busy,setBusy]=useState(false)
  const [error,setError]=useState('')
  const [loaded,setLoaded]=useState(false)
  const [removeId,setRemoveId]=useState<string|null>(null)
  const lock=useRef(false)
  const keyInput=useRef<HTMLInputElement>(null)
  const refresh=useCallback(async()=>{
    if(!supported||lock.current)return
    lock.current=true;setBusy(true);setError('')
    try{setRows(await request<Destination[]>('/studio/destinations'));setLoaded(true)}
    catch{setError('Could not read the OS credential store. Unlock it and try again.')}
    finally{lock.current=false;setBusy(false)}
  },[supported])
  useEffect(()=>{void refresh()},[refresh])
  async function add(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    if(!supported||lock.current)return
    const form=event.currentTarget
    const data=new FormData(form)
    const streamKey=String(data.get('streamKey')??'')
    if(keyInput.current)keyInput.current.value=''
    lock.current=true;setBusy(true);setError('')
    try{
      const saved=await request<Destination>('/studio/destinations/add',{name:String(data.get('name')??'').trim(),serverUrl:String(data.get('serverUrl')??'').trim(),streamKey})
      setRows(previous=>[...previous,saved]);form.reset()
    }catch(cause){setError((cause as Error).message||'Could not save destination')}
    finally{lock.current=false;setBusy(false)}
  }
  async function remove(id:string){
    if(!supported||lock.current||removeId!==id)return
    lock.current=true;setBusy(true);setError('')
    try{await request('/studio/destinations/remove',{id});setRows(previous=>previous.filter(row=>row.id!==id));setRemoveId(null)}
    catch(cause){setError((cause as Error).message||'Could not remove destination')}
    finally{lock.current=false;setBusy(false)}
  }
  return <section className="panel" aria-label="Secure stream destinations">
    <div className="panel-heading"><h2>Secure stream destinations</h2></div>
    <div style={{padding:'16px'}}>
      <p className="dock-note">Save up to eight RTMPS destinations in the desktop OS credential store. Saving is not a connection, authorization to broadcast, or account link. Actual Go Live remains disabled.</p>
      {!supported?<p>Destination storage requires the macOS or Windows desktop application.</p>:<>
        <button className="secondary-button" disabled={busy} onClick={()=>void refresh()}>Refresh destinations</button>
        {error&&<p role="alert" className="preview-error">{error}</p>}
        {loaded&&rows.length===0&&<p>No saved destinations.</p>}
        {rows.map(row=><div key={row.id} className="dock-note" style={{marginBlock:'12px',overflowWrap:'anywhere'}}>
          <strong>{row.name}</strong> · {row.credentialAvailable?'Credential stored':'Credential missing'}
          {row.serverUrl&&<p>{row.serverUrl}</p>}
          {removeId===row.id?<div><p>Delete this destination and its saved credential?</p><button className="secondary-button" disabled={busy} onClick={()=>void remove(row.id)}>Confirm removal</button><button className="secondary-button" disabled={busy} onClick={()=>setRemoveId(null)}>Cancel</button></div>:
            <button className="secondary-button" disabled={busy} onClick={()=>setRemoveId(row.id)}>Remove {row.name}</button>}
        </div>)}
        <form onSubmit={event=>void add(event)} style={{display:'grid',gap:'10px',maxWidth:'540px'}} autoComplete="off">
          <label>Destination name <input name="name" required maxLength={80} placeholder="My YouTube channel" disabled={busy}/></label>
          <label>RTMPS server URL <input name="serverUrl" required maxLength={768} placeholder="rtmps://ingest.example.com/live" disabled={busy}/></label>
          <label>Stream key <input ref={keyInput} name="streamKey" type="password" required maxLength={768} autoComplete="new-password" spellCheck={false} disabled={busy}/></label>
          <button className="secondary-button" type="submit" disabled={busy||!loaded||rows.length>=8}>Save destination</button>
        </form>
        <p className="dock-note">Keys are never returned by the destination list. To rotate a key, save a new destination and remove the old one. Enter the key only in its dedicated field.</p>
      </>}
    </div>
  </section>
}
