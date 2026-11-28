//! All destination fields, including URL and key, are stored together in the
//! OS credential vault. The local index contains UUID filenames only.
use serde::{Deserialize,Serialize};
use std::{fs,io::ErrorKind,path::{Path,PathBuf},sync::Mutex};
use tauri::Manager;
const SERVICE:&str="com.moobee.streamstudio.destination.v1";
const MAX_DESTINATIONS:usize=8;
const MAX_RECORD:usize=2048;
static LOCK:Mutex<()>=Mutex::new(());
static ACTIVE:Mutex<Vec<String>>=Mutex::new(Vec::new());
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Record{version:u8,name:String,server_url:String,stream_key:String,#[serde(default)] revision:Option<String>}
impl Drop for Record{fn drop(&mut self){use zeroize::Zeroize;self.stream_key.zeroize();}}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct View{id:String,name:String,server_url:Option<String>,credential_available:bool,revision:Option<String>}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub(crate) struct Preview{id:String,name:String,server_url:String,revision:Option<String>}
trait Vault{
    fn put(&self,id:&str,bytes:&[u8])->Result<(),String>;
    fn get(&self,id:&str)->Result<Option<Vec<u8>>,String>;
    fn remove(&self,id:&str)->Result<(),String>;
}
struct OsVault;
fn entry(id:&str)->Result<keyring::Entry,String>{
    keyring::Entry::new(SERVICE,id).map_err(|_|"OS credential store unavailable".into())
}
impl Vault for OsVault{
    fn put(&self,id:&str,bytes:&[u8])->Result<(),String>{
        entry(id)?.set_secret(bytes).map_err(|_|"Could not save destination to OS credential store".into())
    }
    fn get(&self,id:&str)->Result<Option<Vec<u8>>,String>{
        match entry(id)?.get_secret(){
            Ok(v)=>Ok(Some(v)),Err(keyring::Error::NoEntry)=>Ok(None),
            Err(_)=>Err("Could not read OS credential store".into()),
        }
    }
    fn remove(&self,id:&str)->Result<(),String>{
        match entry(id)?.delete_credential(){
            Ok(())|Err(keyring::Error::NoEntry)=>Ok(()),
            Err(_)=>Err("Could not delete OS credential".into()),
        }
    }
}
fn valid_id(id:&str)->bool{uuid::Uuid::parse_str(id).is_ok_and(|v|v.to_string()==id)}
fn validate(name:&str,url:&str,key:&str)->Result<(),String>{
    if name.trim()!=name||name.is_empty()||name.len()>80||name.chars().any(char::is_control){
        return Err("Enter a destination label of 1–80 bytes".into())
    }
    if url.len()>768||key.is_empty()||key.len()>768||key.trim()!=key||key.chars().any(char::is_control){
        return Err("Invalid destination URL or stream key".into())
    }
    // Shared validation; no DNS, handshake or network.
    crate::rtmp_session::tls::validate_destination(url,key).map_err(|_|"Invalid secure RTMPS destination or stream key".into())
}
fn index(app:&tauri::AppHandle)->Result<PathBuf,String>{
    Ok(app.path().app_data_dir().map_err(|_|"Application storage unavailable")?.join("destinations-v1"))
}
fn ids(dir:&Path)->Result<Vec<String>,String>{
    match fs::symlink_metadata(dir){
        Ok(m) if m.is_dir()&&!m.file_type().is_symlink()=>{},
        Ok(_)=>return Err("Destination index is not a regular directory".into()),
        Err(e) if e.kind()==ErrorKind::NotFound=>return Ok(vec![]),
        Err(_)=>return Err("Destination index unavailable".into()),
    }
    let mut result=Vec::new();
    for item in fs::read_dir(dir).map_err(|_|"Destination index unavailable")?{
        let item=item.map_err(|_|"Destination index unavailable")?;
        let name=item.file_name().to_string_lossy().into_owned();
        if !valid_id(&name){continue}
        let ty=item.file_type().map_err(|_|"Destination index unavailable")?;
        if !ty.is_file()||ty.is_symlink(){return Err("Invalid destination index entry".into())}
        result.push(name);
        if result.len()>MAX_DESTINATIONS{return Err("Destination index exceeds capacity".into())}
    }
    result.sort();Ok(result)
}
fn load(vault:&impl Vault,id:&str)->Result<Option<Record>,String>{
    let Some(bytes)=vault.get(id)?else{return Ok(None)};
    let bytes=zeroize::Zeroizing::new(bytes);
    if bytes.len()>MAX_RECORD{return Err("Invalid stored destination".into())}
    let r:Record=serde_json::from_slice(&bytes).map_err(|_|"Invalid stored destination")?;
    if r.version!=1||r.revision.as_ref().is_some_and(|id|!valid_id(id)){return Err("Unsupported destination version".into())}
    validate(&r.name,&r.server_url,&r.stream_key)?;
    Ok(Some(r))
}
fn views(dir:&Path,vault:&impl Vault)->Result<Vec<View>,String>{
    ids(dir)?.into_iter().map(|id|{
        let r=load(vault,&id)?;
        Ok(match r{
            Some(r)=>View{id,name:r.name.clone(),server_url:Some(r.server_url.clone()),credential_available:true,revision:r.revision.clone()},
            None=>View{id,name:"Missing OS credential".into(),server_url:None,credential_available:false,revision:None},
        })
    }).collect()
}
fn create(dir:&Path,vault:&impl Vault,name:String,server_url:String,stream_key:String)->Result<View,String>{
    let record=Record{version:1,name,server_url,stream_key,revision:Some(uuid::Uuid::new_v4().to_string())};
    validate(&record.name,&record.server_url,&record.stream_key)?;
    if ids(dir)?.len()>=MAX_DESTINATIONS{return Err("At most eight saved destinations are supported".into())}
    let bytes=zeroize::Zeroizing::new(serde_json::to_vec(&record).map_err(|_|"Could not encode destination")?);
    if bytes.len()>MAX_RECORD{return Err("Destination record exceeds credential store budget".into())}
    fs::create_dir_all(dir).map_err(|_|"Could not create destination index")?;
    let id=uuid::Uuid::new_v4().to_string();
    let marker=dir.join(&id);
    let file=fs::OpenOptions::new().create_new(true).write(true).open(&marker).map_err(|_|"Could not create destination index entry")?;
    if file.sync_all().is_err(){drop(file);let _=fs::remove_file(&marker);return Err("Could not sync destination index".into())}
    drop(file);
    if let Err(e)=vault.put(&id,&bytes){
        // A failed store call may have partially succeeded. Keep the index
        // if rollback fails so the user can still delete the vault record.
        if vault.remove(&id).is_ok(){let _=fs::remove_file(&marker);}
        return Err(e)
    }
    Ok(View{id,name:record.name.clone(),server_url:Some(record.server_url.clone()),credential_available:true,revision:record.revision.clone()})
}
// Replace one complete vault record under the process-local store lock.
// No plaintext persistence and no changes to the UUID marker.
fn edit(dir:&Path,vault:&impl Vault,id:&str,expected_revision:Option<&str>,name:String,server_url:String,new_key:Option<String>)->Result<View,String>{
    let new_key=new_key.map(zeroize::Zeroizing::new);
    if !valid_id(id)||!ids(dir)?.iter().any(|v|v==id){return Err("Unknown destination".into())}
    if ACTIVE.lock().map_err(|_|"Destination lease state unavailable")?.iter().any(|v|v==id){return Err("Stop the active publishing session before editing its destination".into())}
    let mut record=load(vault,id)?.ok_or("Destination credential missing; remove and recreate it")?;
    if record.revision.as_deref()!=expected_revision{return Err("Destination changed; refresh and review again".into())}
    // A different endpoint must not silently inherit the old endpoint's key.
    if record.server_url!=server_url&&new_key.is_none(){return Err("A new stream key is required when changing the server URL".into())}
    if let Some(key)=new_key{
        use zeroize::Zeroize;
        record.stream_key.zeroize();
        record.stream_key=key.to_string();
    }
    validate(&name,&server_url,&record.stream_key)?;
    record.name=name;record.server_url=server_url;
    record.revision=Some(uuid::Uuid::new_v4().to_string());
    let bytes=zeroize::Zeroizing::new(serde_json::to_vec(&record).map_err(|_|"Could not encode destination")?);
    if bytes.len()>MAX_RECORD{return Err("Destination record exceeds credential store budget".into())}
    vault.put(id,&bytes)?;
    Ok(View{id:id.to_owned(),name:record.name.clone(),server_url:Some(record.server_url.clone()),credential_available:true,revision:record.revision.clone()})
}
fn remove(dir:&Path,vault:&impl Vault,id:&str)->Result<(),String>{
    if !valid_id(id)||!ids(dir)?.iter().any(|v|v==id){return Err("Unknown destination".into())}
    if ACTIVE.lock().map_err(|_|"Destination lease state unavailable")?.iter().any(|v|v==id){return Err("Stop the active publishing session before removing its destination".into())}
    vault.remove(id)?;
    fs::remove_file(dir.join(id)).map_err(|_|"Credential deleted; could not remove destination index".into())
}
/// Native-only credential lease. No Debug/Serialize implementation. A
/// process-local lease prevents removal or duplicate use while an authorized
/// publisher is active. The exact destination preview must be reconfirmed.
pub(crate) struct Lease{id:String,record:Record}
impl Lease{
    pub(crate) fn credentials(&self)->(&str,&str){(&self.record.server_url,&self.record.stream_key)}
}
impl Drop for Lease{
    fn drop(&mut self){if let Ok(mut active)=ACTIVE.lock(){active.retain(|v|v!=&self.id);}}
}
fn lease(dir:&Path,vault:&impl Vault,id:&str,expected_name:&str,expected_url:&str,expected_revision:Option<&str>,explicitly_authorized:bool)->Result<Lease,String>{
    if !explicitly_authorized{return Err("Explicit destination authorization required".into())}
    if !valid_id(id)||!ids(dir)?.iter().any(|v|v==id){return Err("Unknown destination".into())}
    let record=load(vault,id)?.ok_or("Destination credential missing")?;
    if record.name!=expected_name||record.server_url!=expected_url||record.revision.as_deref()!=expected_revision{return Err("Destination changed; refresh and reconfirm".into())}
    let mut active=ACTIVE.lock().map_err(|_|"Destination lease state unavailable")?;
    if active.iter().any(|v|v==id){return Err("Destination is already in use".into())}
    active.push(id.to_owned());
    Ok(Lease{id:id.to_owned(),record})
}
// Validate only selected saved records. Never return secret material or
// claim remote credentials work. Repeat the route check on actual leases.
fn distinct_routes<'a>(records:impl IntoIterator<Item=(&'a str,&'a str)>)->Result<(),String>{
    let mut seen=Vec::<(&str,&str)>::new();
    for (url,key) in records{
        for (other_url,other_key) in &seen{
            if key==*other_key&&crate::rtmp_session::tls::same_publish_endpoint(url,other_url)?{
                return Err("Choose distinct publishing routes; two selected destinations refer to the same endpoint and key".into())
            }
        }
        seen.push((url,key));
    }
    Ok(())
}
fn preflight(dir:&Path,vault:&impl Vault,selected:&[Preview])->Result<(),String>{
    if !(1..=3).contains(&selected.len()){return Err("Choose one to three destinations".into())}
    let available=ids(dir)?;
    let mut seen=std::collections::HashSet::new();
    let mut records=Vec::new();
    let active=ACTIVE.lock().map_err(|_|"Destination lease state unavailable")?;
    for preview in selected{
        if !valid_id(&preview.id)||!seen.insert(&preview.id)||!available.contains(&preview.id){return Err("Invalid or duplicate destination selection".into())}
        if active.contains(&preview.id){return Err("A selected destination is already in use".into())}
        let record=load(vault,&preview.id)?.ok_or("Destination credential missing")?;
        if record.name!=preview.name||record.server_url!=preview.server_url||record.revision!=preview.revision{return Err("Destination changed; refresh and review again".into())}
        records.push(record);
    }
    distinct_routes(records.iter().map(|r|(r.server_url.as_str(),r.stream_key.as_str())))
}
pub(crate) fn validate_distinct_leases(leases:&[(String,std::sync::Arc<Lease>)])->Result<(),String>{
    distinct_routes(leases.iter().map(|(_,lease)|lease.credentials()))
}
pub(crate) async fn preflight_stream_destinations(app:tauri::AppHandle,selected:Vec<Preview>)->Result<(),String>{
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
        preflight(&index(&app)?,&OsVault,&selected)
    }).await.map_err(|_|"Destination store worker stopped")?
}
pub(crate) fn authorized_destination(app:&tauri::AppHandle,id:&str,expected_name:&str,expected_url:&str,expected_revision:Option<&str>,explicitly_authorized:bool)->Result<Lease,String>{
    let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
    lease(&index(app)?,&OsVault,id,expected_name,expected_url,expected_revision,explicitly_authorized)
}
pub(crate) async fn list_stream_destinations(app:tauri::AppHandle)->Result<Vec<View>,String>{
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
        views(&index(&app)?,&OsVault)
    }).await.map_err(|_|"Destination store worker stopped")?
}
pub(crate) async fn add_stream_destination(app:tauri::AppHandle,name:String,server_url:String,stream_key:String)->Result<View,String>{
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
        create(&index(&app)?,&OsVault,name,server_url,stream_key)
    }).await.map_err(|_|"Destination store worker stopped")?
}
pub(crate) async fn edit_stream_destination(app:tauri::AppHandle,id:String,expected_revision:Option<String>,name:String,server_url:String,new_key:Option<String>)->Result<View,String>{
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
        edit(&index(&app)?,&OsVault,&id,expected_revision.as_deref(),name,server_url,new_key)
    }).await.map_err(|_|"Destination store worker stopped")?
}
pub(crate) async fn remove_stream_destination(app:tauri::AppHandle,id:String)->Result<(),String>{
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=LOCK.lock().map_err(|_|"Destination store unavailable")?;
        remove(&index(&app)?,&OsVault,&id)
    }).await.map_err(|_|"Destination store worker stopped")?
}
#[cfg(test)]
mod tests{
    use super::*;
    use std::{cell::RefCell,collections::HashMap};
    #[derive(Default)]
    struct Memory(RefCell<HashMap<String,Vec<u8>>>);
    impl Vault for Memory{
        fn put(&self,id:&str,b:&[u8])->Result<(),String>{self.0.borrow_mut().insert(id.into(),b.to_vec());Ok(())}
        fn get(&self,id:&str)->Result<Option<Vec<u8>>,String>{Ok(self.0.borrow().get(id).cloned())}
        fn remove(&self,id:&str)->Result<(),String>{self.0.borrow_mut().remove(id);Ok(())}
    }
    struct Temp(PathBuf);
    impl Drop for Temp{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    #[test]fn vault_only_secret_and_uuid_index(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        let v=create(&dir.0,&vault,"Test".into(),"rtmps://example.invalid/live".into(),"fixture-secret".into()).unwrap();
        assert_eq!(fs::read(dir.0.join(&v.id)).unwrap().len(),0);
        let json=serde_json::to_string(&views(&dir.0,&vault).unwrap()).unwrap();
        assert!(!json.contains("fixture-secret"));assert!(!json.contains("streamKey"));
        assert_eq!(load(&vault,&v.id).unwrap().unwrap().stream_key,"fixture-secret");
        assert!(remove(&dir.0,&vault,"../outside").is_err());
        vault.remove(&v.id).unwrap();assert!(!views(&dir.0,&vault).unwrap()[0].credential_available);
        remove(&dir.0,&vault,&v.id).unwrap();assert!(views(&dir.0,&vault).unwrap().is_empty());
    }
    #[test]
    #[ignore="requires unlocked native OS credential store; creates and removes a random fixture credential"]
    fn native_vault_round_trip(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=OsVault;
        let v=create(&dir.0,&vault,"MooBee QA fixture".into(),"rtmps://example.invalid/live".into(),"fixture-not-a-real-key".into()).unwrap();
        let read=load(&vault,&v.id);
        let removed=remove(&dir.0,&vault,&v.id);
        assert!(removed.is_ok());
        assert_eq!(read.unwrap().unwrap().stream_key,"fixture-not-a-real-key");
        assert!(vault.get(&v.id).unwrap().is_none());
    }
    #[test]fn explicit_preview_confirmation_and_lease_block_removal(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        let url="rtmps://example.invalid/live";
        let v=create(&dir.0,&vault,"Test".into(),url.into(),"fixture-secret".into()).unwrap();
        assert!(lease(&dir.0,&vault,&v.id,"Test",url,v.revision.as_deref(),false).is_err());
        assert!(lease(&dir.0,&vault,&v.id,"Other",url,v.revision.as_deref(),true).is_err());
        assert!(lease(&dir.0,&vault,&v.id,"Test","rtmps://other.invalid/live",v.revision.as_deref(),true).is_err());
        let l=lease(&dir.0,&vault,&v.id,"Test",url,v.revision.as_deref(),true).unwrap();
        assert_eq!(l.credentials(),(url,"fixture-secret"));
        assert!(lease(&dir.0,&vault,&v.id,"Test",url,v.revision.as_deref(),true).is_err());
        assert!(remove(&dir.0,&vault,&v.id).is_err());
        let owner=std::sync::Arc::new(l);let transport=owner.clone();
        drop(owner);assert!(remove(&dir.0,&vault,&v.id).is_err());
        drop(transport);remove(&dir.0,&vault,&v.id).unwrap();
    }
    #[test]fn edits_rotate_revision_and_block_stale_preview_and_active_lease(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        let url="rtmps://example.invalid/live";
        let v=create(&dir.0,&vault,"Test".into(),url.into(),"fixture-old".into()).unwrap();
        let l=lease(&dir.0,&vault,&v.id,"Test",url,v.revision.as_deref(),true).unwrap();
        assert!(edit(&dir.0,&vault,&v.id,v.revision.as_deref(),"Test".into(),url.into(),Some("fixture-new".into())).is_err());
        drop(l);
        assert!(edit(&dir.0,&vault,&v.id,v.revision.as_deref(),"Test".into(),"rtmps://other.invalid/live".into(),None).is_err());
        let updated=edit(&dir.0,&vault,&v.id,v.revision.as_deref(),"Test".into(),url.into(),Some("fixture-new".into())).unwrap();
        assert_eq!(updated.id,v.id);assert_ne!(updated.revision,v.revision);
        assert!(lease(&dir.0,&vault,&v.id,"Test",url,v.revision.as_deref(),true).is_err());
        assert!(edit(&dir.0,&vault,&v.id,v.revision.as_deref(),"Stale".into(),url.into(),None).is_err());
        let renamed=edit(&dir.0,&vault,&v.id,updated.revision.as_deref(),"Renamed".into(),url.into(),None).unwrap();
        let l=lease(&dir.0,&vault,&v.id,"Renamed",url,renamed.revision.as_deref(),true).unwrap();
        assert_eq!(l.credentials(),(url,"fixture-new"));
        assert!(!serde_json::to_string(&renamed).unwrap().contains("fixture-new"));
        assert_eq!(fs::read(dir.0.join(&v.id)).unwrap().len(),0);
    }
    #[test]fn legacy_record_without_revision_can_be_updated_once(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        let url="rtmps://example.invalid/live";
        let v=create(&dir.0,&vault,"Test".into(),url.into(),"fixture-old".into()).unwrap();
        let mut record=load(&vault,&v.id).unwrap().unwrap();record.revision=None;
        vault.put(&v.id,&serde_json::to_vec(&record).unwrap()).unwrap();
        let updated=edit(&dir.0,&vault,&v.id,None,"Test".into(),url.into(),None).unwrap();
        assert!(updated.revision.is_some());
        assert!(lease(&dir.0,&vault,&v.id,"Test",url,None,true).is_err());
    }
    #[test]fn local_preflight_rejects_duplicate_route_and_stale_selection(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        let a=create(&dir.0,&vault,"One".into(),"rtmps://example.invalid/live".into(),"fixture-secret".into()).unwrap();
        let b=create(&dir.0,&vault,"Two".into(),"rtmps://EXAMPLE.invalid:443/live/".into(),"fixture-secret".into()).unwrap();
        let preview=|v:&View|Preview{id:v.id.clone(),name:v.name.clone(),server_url:v.server_url.clone().unwrap(),revision:v.revision.clone()};
        assert!(preflight(&dir.0,&vault,&[preview(&a)]).is_ok());
        let err=preflight(&dir.0,&vault,&[preview(&a),preview(&b)]).unwrap_err();
        assert!(err.contains("distinct publishing routes"));assert!(!err.contains("fixture-secret"));
        assert!(preflight(&dir.0,&vault,&[preview(&a),preview(&a)]).is_err());
        let active_lease=lease(&dir.0,&vault,&a.id,&a.name,a.server_url.as_deref().unwrap(),a.revision.as_deref(),true).unwrap();
        assert!(preflight(&dir.0,&vault,&[preview(&a)]).is_err());
        drop(active_lease);
        let updated=edit(&dir.0,&vault,&b.id,b.revision.as_deref(),b.name.clone(),b.server_url.clone().unwrap(),Some("other-fixture-key".into())).unwrap();
        assert!(preflight(&dir.0,&vault,&[preview(&a),preview(&b)]).is_err());
        assert!(preflight(&dir.0,&vault,&[preview(&a),preview(&updated)]).is_ok());
        let a_lease=lease(&dir.0,&vault,&a.id,&a.name,a.server_url.as_deref().unwrap(),a.revision.as_deref(),true).unwrap();
        let b_lease=lease(&dir.0,&vault,&updated.id,&updated.name,updated.server_url.as_deref().unwrap(),updated.revision.as_deref(),true).unwrap();
        assert!(validate_distinct_leases(&[(a.id.clone(),std::sync::Arc::new(a_lease)),(updated.id.clone(),std::sync::Arc::new(b_lease))]).is_ok());
    }
    #[test]fn rejects_invalid_config_without_writing(){
        let dir=Temp(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));let vault=Memory::default();
        for url in ["rtmp://example.invalid/live","rtmps://user:secret@example.invalid/live","rtmps://example.invalid/live?key=secret"]{
            assert!(create(&dir.0,&vault,"Test".into(),url.into(),"fixture-secret".into()).is_err());
        }
        assert!(!dir.0.exists());assert!(vault.0.borrow().is_empty());
    }
}
