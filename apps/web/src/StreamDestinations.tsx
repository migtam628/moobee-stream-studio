import {useCallback,useEffect,useRef,useState,type FormEvent} from 'react'
import {isStaticPreview,request,runningInTauri} from './api'
type Destination={id:string;name:string;serverUrl:string|null;credentialAvailable:boolean;revision?:string|null}
export default function StreamDestinations(){
  const supported=runningInTauri()&&!isStaticPreview&&/Macintosh|Mac OS X|Windows/i.test(navigator.userAgent)
  const [rows,setRows]=useState<Destination[]>([])
  const [busy,setBusy]=useState(false)
  const [error,setError]=useState('')
  const [loaded,setLoaded]=useState(false)
  const [removeId,setRemoveId]=useState<string|null>(null)
  const [editing,setEditing]=useState<Destination|null>(null)
  const editKeyInput=useRef<HTMLInputElement>(null)
  const lock=useRef(false)
  const keyInput=useRef<HTMLInputElement>(null)
  const refresh=useCallback(async()=>{
    if(!supported||lock.current)return
    lock.current=true;setBusy(true);setError('');setEditing(null);setRemoveId(null)
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
  async function edit(event:FormEvent<HTMLFormElement>){
    event.preventDefault()
    if(!supported||lock.current||!editing)return
    const data=new FormData(event.currentTarget)
    const newKey=String(data.get('newKey')??'')
    const name=String(data.get('editName')??'').trim()
    const serverUrl=String(data.get('editUrl')??'').trim()
    if(editKeyInput.current)editKeyInput.current.value=''
    if(serverUrl!==editing.serverUrl&&!newKey){setError('Enter a new stream key when changing the server URL.');return}
    const id=editing.id
    lock.current=true;setBusy(true);setError('')
    try{
      const saved=await request<Destination>('/studio/destinations/edit',{id,expectedRevision:editing.revision??null,name,serverUrl,newKey:newKey||null})
      setRows(previous=>previous.map(row=>row.id===id?saved:row));setEditing(null)
    }catch(cause){setError((cause as Error).message||'Could not update destination. Refresh before retrying.')}
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
          {editing?.id===row.id?<form key={row.id} aria-label="Edit saved destination" onSubmit={event=>void edit(event)} style={{display:'grid',gap:'8px',maxWidth:'540px'}} autoComplete="off">
            <label>Edited destination name <input name="editName" defaultValue={editing.name} required maxLength={80} disabled={busy}/></label>
            <label>Edited RTMPS server URL <input name="editUrl" defaultValue={editing.serverUrl??''} required maxLength={768} disabled={busy}/></label>
            <label>Replacement stream key <input ref={editKeyInput} name="newKey" type="password" maxLength={768} autoComplete="new-password" spellCheck={false} disabled={busy}/></label>
            <small>Leave the replacement key blank to retain the current key for the same URL. Changing the URL requires a new key.</small>
            <div><button type="submit" className="secondary-button" disabled={busy}>Save destination changes</button><button type="button" className="secondary-button" disabled={busy} onClick={()=>{setEditing(null);setError('')}}>Cancel edit</button></div>
          </form>:removeId===row.id?<div><p>Delete this destination and its saved credential?</p><button className="secondary-button" disabled={busy} onClick={()=>void remove(row.id)}>Confirm removal</button><button className="secondary-button" disabled={busy} onClick={()=>setRemoveId(null)}>Cancel</button></div>:
            <div>{row.credentialAvailable&&<button className="secondary-button" disabled={busy} onClick={()=>{setEditing(row);setRemoveId(null);setError('')}}>Edit {row.name}</button>}<button className="secondary-button" disabled={busy} onClick={()=>{setEditing(null);setRemoveId(row.id)}}>Remove {row.name}</button></div>}
        </div>)}
        <form onSubmit={event=>void add(event)} style={{display:'grid',gap:'10px',maxWidth:'540px'}} autoComplete="off">
          <label>Destination name <input name="name" required maxLength={80} placeholder="My YouTube channel" disabled={busy}/></label>
          <label>RTMPS server URL <input name="serverUrl" required maxLength={768} placeholder="rtmps://ingest.example.com/live" disabled={busy}/></label>
          <label>Stream key <input ref={keyInput} name="streamKey" type="password" required maxLength={768} autoComplete="new-password" spellCheck={false} disabled={busy}/></label>
          <button className="secondary-button" type="submit" disabled={busy||!loaded||rows.length>=8}>Save destination</button>
        </form>
        <p className="dock-note">Keys are never returned by the destination list. Edit a destination to rotate its key or rename it. Changes require a fresh broadcast review; active publishing leases block edits. Enter the key only in its dedicated field.</p>
      </>}
    </div>
  </section>
}
