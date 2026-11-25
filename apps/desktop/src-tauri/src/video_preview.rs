//! Bounded local moving preview, independent of recording and network output.
//! One persistent worker per physical input, shared between Preview and Program.
use crate::{clip::{CaptureLock, CAPTURE_BUSY}, program_camera::{ProgramCamera, valid_device_id}, scene_compositor::{SceneLayout, composite_layout}};
use std::{collections::HashMap, sync::{Arc, Weak, Mutex, OnceLock, atomic::{AtomicBool, Ordering}}, thread::{self, JoinHandle}, time::{Duration, Instant}};
use image::RgbaImage;
const LEASE: Duration = Duration::from_secs(5);
const STALE: Duration = Duration::from_secs(3);
const MAX_DEVICES: usize = 2;

struct Frame { image: Arc<RgbaImage>, received: Instant, number: u64 }
struct Shared {
    last_use: Mutex<Instant>,
    stop: AtomicBool,
    frame: Mutex<Option<Frame>>,
    error: Mutex<Option<String>>,
}
struct Worker { state: Arc<Shared>, handle: JoinHandle<()>, generation: String }
#[derive(Default)]
struct Registry { workers: HashMap<String, Worker>, guard: Weak<CaptureLock> }
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
fn registry() -> &'static Mutex<Registry> { REGISTRY.get_or_init(|| Mutex::new(Registry::default())) }

fn worker_loop(id: String, state: Arc<Shared>, _capture_guard: Arc<CaptureLock>) {
    let result = (|| -> Result<(),String> {
        if state.stop.load(Ordering::Acquire) { return Ok(()); }
        let mut camera = ProgramCamera::start_selected(Some(&id))?;
        let mut last = Instant::now();
        let mut number = 0u64;
        while !state.stop.load(Ordering::Acquire) && state.last_use.lock().unwrap().elapsed() < LEASE {
            if let Some(image) = camera.poll()? {
                number = number.saturating_add(1);
                last = Instant::now();
                *state.frame.lock().unwrap() = Some(Frame{image:Arc::new(image),received:last,number});
            } else if last.elapsed() >= STALE {
                return Err("No fresh video frames; check HDMI, USB, and camera permissions. Pause and resume preview to retry.".into());
            }
            thread::sleep(Duration::from_millis(66));
        }
        Ok(())
    })();
    *state.frame.lock().unwrap() = None;
    if let Err(error) = result { *state.error.lock().unwrap() = Some(error); }
}
fn prune(reg: &mut Registry) {
    reg.workers.retain(|_,worker| {
        // Failed workers stay visible until the consumer stops renewing its lease.
        !(worker.handle.is_finished() && worker.state.last_use.lock().unwrap().elapsed() >= LEASE)
    });
}
fn ensure_worker(reg: &mut Registry, id: &str) -> Result<(),String> {
    if reg.workers.contains_key(id) { return Ok(()); }
    if reg.workers.len() >= MAX_DEVICES { return Err("At most two video devices can be previewed. Pause preview or wait for unused sources to release.".into()); }
    let guard = if let Some(guard) = reg.guard.upgrade() { guard } else {
        CAPTURE_BUSY.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire)
            .map_err(|_| "Another native capture is running; preview will resume after it ends".to_owned())?;
        let guard = Arc::new(CaptureLock);
        reg.guard = Arc::downgrade(&guard);
        guard
    };
    let state = Arc::new(Shared {last_use: Mutex::new(Instant::now()),stop:AtomicBool::new(false),frame:Mutex::new(None),error:Mutex::new(None)});
    let worker_state = state.clone();
    let device = id.to_owned();
    let handle = thread::Builder::new().name("moobee-video-preview".into())
        .spawn(move || worker_loop(device,worker_state,guard)).map_err(|e|e.to_string())?;
    reg.workers.insert(id.to_owned(),Worker { state, handle, generation: uuid::Uuid::new_v4().to_string() });
    Ok(())
}
fn validate_ids(ids: &[String]) -> Result<(),String> {
    if ids.is_empty() || ids.len() > MAX_DEVICES || ids.iter().any(|id|!valid_device_id(id)) ||
        ids.iter().enumerate().any(|(i,id)| ids[..i].contains(id)) {
        return Err("Select one or two distinct native video inputs".into());
    }
    Ok(())
}
#[derive(serde::Serialize)]
#[serde(rename_all="camelCase")]
pub(crate) struct PreviewFrame {
    image: Option<String>,
    frame_key: Option<String>,
    live_video: bool,
    waiting: bool,
}
fn preview(ids: Vec<String>, layout: SceneLayout, previous_key: Option<String>) -> Result<PreviewFrame,String> {
    use base64::Engine as _;
    use image::{DynamicImage, codecs::jpeg::JpegEncoder};
    validate_ids(&ids)?;
    let mut frames = Vec::new();
    let mut keys = Vec::new();
    {
        let mut reg = registry().lock().map_err(|_|"Preview registry unavailable")?;
        prune(&mut reg);
        // Renew all consumers before reading so two monitors can share a source.
        for id in &ids {
            ensure_worker(&mut reg,id)?;
            *reg.workers[id].state.last_use.lock().unwrap() = Instant::now();
        }
        for id in &ids {
            let worker = &reg.workers[id];
            if let Some(error) = worker.state.error.lock().unwrap().clone() { return Err(error); }
            let frame = worker.state.frame.lock().unwrap();
            match frame.as_ref().filter(|f| f.received.elapsed() < STALE) {
                Some(frame) => {
                    frames.push(frame.image.clone());
                    keys.push(format!("{}:{}",worker.generation,frame.number));
                },
                None => {
                    if worker.handle.is_finished() { return Err("Video input preview ended. Pause and resume to reconnect.".into()); }
                    return Ok(PreviewFrame{image:None,frame_key:None,live_video:true,waiting:true});
                }
            }
        }
    }
    let key = format!("{layout:?}:{}",keys.join(","));
    if previous_key.as_deref()==Some(key.as_str()) {
        return Ok(PreviewFrame{image:None,frame_key:Some(key),live_video:true,waiting:false});
    }
    // Bounded CPU composition at 640x360. No frame queue or disk writes.
    let images: Vec<RgbaImage> = frames.iter().map(|frame|(**frame).clone()).collect();
    let canvas = composite_layout(&images,640,360,layout)?;
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes,70).encode_image(&DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(canvas).to_rgb8())).map_err(|e|e.to_string())?;
    Ok(PreviewFrame{image:Some(format!("data:image/jpeg;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))),frame_key:Some(key),live_video:true,waiting:false})
}
fn stop() -> Result<(),String> {
    let mut reg = registry().lock().map_err(|_|"Preview registry unavailable")?;
    for worker in reg.workers.values() { worker.state.stop.store(true,Ordering::Release); }
    for (_,worker) in reg.workers.drain() { let _ = worker.handle.join(); }
    reg.guard = Weak::new();
    Ok(())
}
pub(crate) async fn frame(ids: Vec<String>, layout: Option<String>, previous_key: Option<String>) -> Result<PreviewFrame,String> {
    let layout = SceneLayout::parse(layout.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || preview(ids,layout,previous_key)).await.map_err(|e|e.to_string())?
}
pub(crate) async fn stop_all() -> Result<(),String> {
    tauri::async_runtime::spawn_blocking(stop).await.map_err(|e|e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_and_unbounded_devices() {
        assert!(validate_ids(&["card".into(),"camera".into()]).is_ok());
        assert!(validate_ids(&["card".into(),"card".into()]).is_err());
        assert!(validate_ids(&["a".into(),"b".into(),"c".into()]).is_err());
        assert!(validate_ids(&[]).is_err());
        assert!(validate_ids(&["bad\0id".into()]).is_err());
    }
}
