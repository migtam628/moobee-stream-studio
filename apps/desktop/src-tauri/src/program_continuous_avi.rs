//! Continuous native frame-source experiment (XCap receiver -> composite ->
//! bounded silent MJPEG AVI or macOS H.264 MP4). Not a realtime A/V recorder.
//! Screen frames have arrival timestamps only: they are NOT native PTS.
//! Real Mac/Windows hardware validation is required before any beta claim.
use serde::Serialize;
use std::{fs::{self, OpenOptions}, path::PathBuf};
use tauri::Manager;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::{sync::{atomic::Ordering, mpsc::{Receiver, TryRecvError}}, time::{Duration, Instant}};

const FPS: u32 = 10;
const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const MAX_TOTAL_SOURCE_PIXELS: u64 = 36_000_000;
const MAX_STALE_MS: u64 = 1_500;
const MAX_DRAIN_PER_SOURCE_PER_TICK: usize = 64;
const MAX_MP4_SECONDS: u32 = 60;
const MAX_SEGMENTED_SECONDS: u32 = 300;

// Longer tests are sequences of separately finalized 60-second clips.
fn segment_plan(seconds: u32) -> Result<Vec<u32>, String> {
    if (2..=MAX_MP4_SECONDS).contains(&seconds) { return Ok(vec![seconds]); }
    if seconds <= MAX_MP4_SECONDS || seconds > MAX_SEGMENTED_SECONDS || seconds % MAX_MP4_SECONDS != 0 {
        return Err("Choose 2–60 seconds or a whole-minute segmented test up to 5 minutes".into());
    }
    Ok(vec![MAX_MP4_SECONDS; (seconds / MAX_MP4_SECONDS) as usize])
}

// Only scene geometry on the already-open ordered source set may change.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum TransitionKind { Cut, Fade, Slide, Wipe }
impl TransitionKind {
    fn parse(kind:&str,duration_ms:u32)->Result<Self,String>{
        if !(150..=1000).contains(&duration_ms) { return Err("Transition duration must be 150–1000 ms".into()); }
        match kind {"cut"=>Ok(Self::Cut),"fade"=>Ok(Self::Fade),"slide"=>Ok(Self::Slide),"wipe"=>Ok(Self::Wipe),_=>Err("Unsupported Program transition".into())}
    }
    fn frames(self,fps:u32,duration_ms:u32)->u32 {
        if self==Self::Cut {1} else {(fps*duration_ms).div_ceil(1000).max(1)}
    }
}
#[derive(Clone)]
struct SceneGeometry {
    layout:crate::scene_compositor::SceneLayout,
    transforms:Option<Vec<crate::source_transform::SourceTransform>>,
}
struct SceneTake { target:SceneGeometry, kind:TransitionKind, duration_ms:u32 }

#[cfg(any(target_os="macos",target_os="windows"))]
fn compose_geometry(frames:&[image::RgbaImage],width:u32,height:u32,geometry:&SceneGeometry)->Result<image::RgbaImage,String>{
    if let Some(items)=geometry.transforms.as_deref(){crate::source_transform::composite(frames,width,height,items)}
    else{crate::scene_compositor::composite_layout(frames,width,height,geometry.layout)}
}

#[cfg(any(target_os="macos",target_os="windows"))]
fn transition_pixels(outgoing:&image::RgbaImage,incoming:&mut image::RgbaImage,kind:TransitionKind,step:u32,total:u32){
    if kind==TransitionKind::Cut||step>=total{return}
    let (width,height)=incoming.dimensions();
    let edge=width*step/total;
    for y in 0..height { for x in 0..width {
        match kind {
            TransitionKind::Fade=>{
                let a=outgoing.get_pixel(x,y).0;
                let b=incoming.get_pixel_mut(x,y);
                for channel in 0..4 { b.0[channel]=((u32::from(a[channel])*(total-step)+u32::from(b.0[channel])*step+total/2)/total) as u8; }
            }
            TransitionKind::Wipe if x>=edge=>incoming.put_pixel(x,y,*outgoing.get_pixel(x,y)),
            TransitionKind::Slide if x<width-edge=>incoming.put_pixel(x,y,*outgoing.get_pixel(x+edge,y)),
            _=>{}
        }
    }}
}

// Control and telemetry are scoped to the active Program MP4 session.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRecordingStats {
    phase: &'static str,
    recorded_frames: u32,
    fps: u32,
    elapsed_ms: u64,
    paused_ms: u64,
    file_bytes: u64,
    late_frames: u32,
    repeated_source_slots: u64,
    segment_index: u32,
    segment_count: u32,
    completed_segments: u32,
    transition_busy: bool,
    transitions_completed: u32,
    audio_peak: f32,
    audio_missing_source_frames: u64,
}
struct ProgramSession {
    id: String,
    stop: bool,
    pause: bool,
    stats: ProgramRecordingStats,
    switchable_sources: Option<(Vec<usize>,Option<String>)>,
    pending_take: Option<SceneTake>,
}
static PROGRAM_STOP: std::sync::Mutex<Option<ProgramSession>> = std::sync::Mutex::new(None);
struct ProgramStopGuard;
impl ProgramStopGuard {
    fn begin(id: String) -> Self {
        *PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()) = Some(ProgramSession {
            id, stop: false, pause: false, switchable_sources:None, pending_take:None, stats: ProgramRecordingStats {
                phase: "starting", recorded_frames: 0, fps: 0, elapsed_ms: 0,
                paused_ms: 0, file_bytes: 0, late_frames: 0, repeated_source_slots: 0,
                segment_index: 1, segment_count: 1, completed_segments: 0,
                transition_busy:false, transitions_completed:0,
                audio_peak:0., audio_missing_source_frames:0,
            },
        });
        Self
    }
    fn requested(&self) -> bool {
        PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.stop)
    }
    fn paused(&self) -> bool {
        PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|s| s.pause && !s.stop)
    }
    fn update(&self, mut stats: ProgramRecordingStats) {
        if let Some(s) = PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            stats.segment_index = s.stats.segment_index;
            stats.segment_count = s.stats.segment_count;
            stats.completed_segments = s.stats.completed_segments;
            stats.transition_busy = s.stats.transition_busy;
            stats.transitions_completed = s.stats.transitions_completed;
            s.stats = stats;
        }
    }
    fn allow_takes(&self,ids:Vec<usize>,video:Option<String>){
        if let Some(s)=PROGRAM_STOP.lock().unwrap_or_else(|e|e.into_inner()).as_mut(){s.switchable_sources=Some((ids,video));}
    }
    fn next_take(&self)->Option<SceneTake>{
        PROGRAM_STOP.lock().unwrap_or_else(|e|e.into_inner()).as_mut().and_then(|s|s.pending_take.take())
    }
    fn complete_take(&self){
        if let Some(s)=PROGRAM_STOP.lock().unwrap_or_else(|e|e.into_inner()).as_mut(){
            s.stats.transition_busy=false;
            s.stats.transitions_completed=s.stats.transitions_completed.saturating_add(1);
        }
    }
    fn begin_segment(&self, index: u32, count: u32) {
        if let Some(s) = PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            s.pause = false;
            s.stats = ProgramRecordingStats {
                phase: "starting", recorded_frames: 0, fps: 0, elapsed_ms: 0,
                paused_ms: 0, file_bytes: 0, late_frames: 0, repeated_source_slots: 0,
                segment_index: index, segment_count: count, completed_segments: index - 1,
                transition_busy:false, transitions_completed:0,
                audio_peak:0., audio_missing_source_frames:0,
            };
        }
    }
    fn finalizing(&self) {
        if let Some(s) = PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { s.stats.phase = "finalizing"; }
    }
}
impl Drop for ProgramStopGuard {
    fn drop(&mut self) { *PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()) = None; }
}
#[tauri::command]
pub fn stop_continuous_program_mp4(session_id: String) -> bool {
    let mut state = PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = state.as_mut() {
        if s.id == session_id && (s.stats.phase != "finalizing" || s.stats.segment_index < s.stats.segment_count) { s.stop = true; return true; }
    }
    false
}
#[tauri::command]
pub fn pause_continuous_program_mp4(session_id: String, paused: bool) -> bool {
    let mut state = PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = state.as_mut() {
        if s.id == session_id && !s.stop && matches!(s.stats.phase, "recording" | "paused") {
            s.pause = paused; return true;
        }
    }
    false
}
#[tauri::command]
pub fn queue_program_scene_take(session_id:String,display_ids:Vec<usize>,video_device_id:Option<String>,
    layout:Option<String>,transforms:Option<Vec<crate::source_transform::SourceTransform>>,
    kind:String,duration_ms:u32)->Result<(),String>{
    let kind=TransitionKind::parse(&kind,duration_ms)?;
    let layout=crate::scene_compositor::SceneLayout::parse(layout.as_deref())?;
    crate::source_transform::validate_list(transforms.as_deref(),display_ids.len()+usize::from(video_device_id.is_some()))?;
    let mut state=PROGRAM_STOP.lock().unwrap_or_else(|e|e.into_inner());
    let s=state.as_mut().filter(|s|s.id==session_id).ok_or("No matching Program session")?;
    if s.stop||s.pause||s.stats.phase!="recording"||s.stats.transition_busy {
        return Err("Program recorder is not ready for another Take".into());
    }
    if s.switchable_sources.as_ref()!=Some(&(display_ids,video_device_id)){
        return Err("Take requires the same ordered capture inputs and a single MP4 segment without webcam inset".into());
    }
    s.pending_take=Some(SceneTake{target:SceneGeometry{layout,transforms},kind,duration_ms});
    s.stats.transition_busy=true;
    Ok(())
}

#[tauri::command]
pub fn continuous_program_stats(session_id: String) -> Option<ProgramRecordingStats> {
    PROGRAM_STOP.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        .filter(|s| s.id == session_id).map(|s| s.stats.clone())
}

#[tauri::command]
pub fn recover_finalized_program_recordings(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let report = crate::program_mp4_storage::recover_finalized(app)?;
        serde_json::to_value(report).map_err(|e| e.to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { let _ = app; Err("Finalized Program MP4 recovery requires Mac or Windows".into()) }
}

#[derive(Copy, Clone)]
struct EncodingProfile {
    label: &'static str,
    width: u32,
    height: u32,
    fps: u32,
    bitrate_kbps: u32,
}

fn encoding_profile(value: &str) -> Result<EncodingProfile, String> {
    match value {
        "baseline" => Ok(EncodingProfile {
            label: "baseline", width: WIDTH, height: HEIGHT, fps: FPS, bitrate_kbps: 3000,
        }),
        "720p15" => Ok(EncodingProfile {
            label: "720p15", width: 1280, height: 720, fps: 15, bitrate_kbps: 3000,
        }),
        "1080p30" => Ok(EncodingProfile {
            label: "1080p30", width: 1920, height: 1080, fps: 30, bitrate_kbps: 6000,
        }),
        _ => Err("Unsupported Program test profile".into()),
    }
}


#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingSegment {
    id: String,
    path: String,
    bytes: u64,
    duration_seconds: f64,
    frame_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SegmentedProgramReport {
    requested_seconds: u32,
    completed_seconds: f64,
    total_bytes: u64,
    stopped_early: bool,
    warning: Option<String>,
    segments: Vec<RecordingSegment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinuousProgramReport {
    id: String,
    path: String,
    format: &'static str,
    bytes: u64,
    duration_seconds: f64,
    requested_max_seconds: u32,
    stopped_early: bool,
    target_bitrate_kbps: Option<u32>,
    paused_ms: u64,
    frame_count: u32,
    width: u32,
    height: u32,
    fps: u32,
    profile: &'static str,
    display_count: usize,
    video_input_count: usize,
    webcam_included: bool,
    webcam_frames: u32,
    webcam_repeated_slots: u32,
    planned_audio_samples_48khz: u64,
    microphone_included: bool,
    audio_input_count: usize,
    audio_samples_written: u64,
    audio_missing_source_frames: u64,
    audio_peak: f32,
    audio_clipped_samples: usize,
    overlay_included: bool,
    elapsed_ms: u64,
    finalization_ms: u64,
    audio_track_verified: bool,
    measured_sampling_fps: f64,
    source_frames_received: Vec<u32>,
    repeated_source_slots: Vec<u32>,
    late_output_frames: u32,
    max_source_arrival_skew_ms: u64,
    transitions_completed: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    segmented_session: Option<SegmentedProgramReport>,
}

fn validate_request(display_ids: &[usize], seconds: u32, selected_profile: &str) -> Result<EncodingProfile, String> {
    crate::scene_compositor::validate_ids(display_ids)?;
    if !(2..=8).contains(&seconds) {
        return Err("Continuous Program video tests support 2–8 seconds".into());
    }
    let profile = encoding_profile(selected_profile)?;
    if seconds.saturating_mul(profile.fps) > 120 {
        return Err("Requested Program test exceeds the 120-frame safety budget".into());
    }
    Ok(profile)
}

fn validate_mp4_request(display_ids: &[usize], seconds: u32, selected_profile: &str) -> Result<EncodingProfile, String> {
    crate::scene_compositor::validate_ids(display_ids)?;
    segment_plan(seconds)?;
    encoding_profile(selected_profile)
}

fn validate_bitrate(value: Option<u32>, default: u32) -> Result<u32, String> {
    match value.unwrap_or(default) {
        value @ (1500 | 3000 | 6000) => Ok(value),
        _ => Err("Choose a supported MP4 target bitrate (1500, 3000 or 6000 kbps)".into()),
    }
}

fn validate_frame_geometry(width: u32, height: u32, cumulative: u64) -> Result<u64, String> {
    if width == 0 || height == 0 {
        return Err("A native continuous source delivered an empty frame".into());
    }
    let pixels = u64::from(width).checked_mul(u64::from(height))
        .ok_or("Native frame dimensions overflow")?;
    let total = cumulative.checked_add(pixels).ok_or("Source frame size overflow")?;
    if pixels > MAX_TOTAL_SOURCE_PIXELS || total > MAX_TOTAL_SOURCE_PIXELS {
        return Err("Program display sources exceed the bounded 36-million-pixel capture budget".into());
    }
    Ok(total)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct RecorderGuard(Vec<xcap::VideoRecorder>);

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Drop for RecorderGuard {
    fn drop(&mut self) {
        for recorder in &self.0 {
            let _ = recorder.stop();
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn frame_image(frame: xcap::Frame) -> Result<image::RgbaImage, String> {
    validate_frame_geometry(frame.width, frame.height, 0)?;
    image::RgbaImage::from_raw(frame.width, frame.height, frame.raw)
        .ok_or_else(|| "Invalid native continuous RGBA frame buffer".to_owned())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn receive_first_frame(receiver: &Receiver<xcap::Frame>) -> Result<image::RgbaImage, String> {
    let frame = receiver.recv_timeout(Duration::from_secs(3))
        .map_err(|_| "No native video frames arrived within 3 seconds. Check screen permission, display and capture backend.".to_owned())?;
    frame_image(frame)
}

#[derive(Clone, Copy)]
enum ProgramFormat { Avi, Mp4, Flv, Live }

impl ProgramFormat {
    fn extension(self) -> &'static str {
        match self { Self::Avi => "avi", Self::Mp4 => "mp4", Self::Flv => "flv", Self::Live => "rtmps" }
    }
}

// Both formats consume the exact same composed Program canvas. Only the
// encoder differs. No raw-frame queue or external transcoder is introduced.
#[cfg(any(target_os = "windows", target_os = "macos"))]
enum FrameSink {
    Avi(crate::avi::AviWriter<std::fs::File>),
    #[cfg(target_os = "macos")]
    Mp4(crate::program_mp4_sink::ProgramMp4Sink),
    #[cfg(target_os = "windows")]
    Mp4(crate::windows_mp4_sink::WindowsMp4Sink),
    #[cfg(target_os = "macos")]
    Flv(crate::live_flv::NativeSink),
    #[cfg(target_os = "macos")]
    Live(crate::live_program_sink::Sink),
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl FrameSink {
    fn open(path: &std::path::Path, format: ProgramFormat, profile: EncodingProfile, microphone: bool, live_connector:Option<Box<dyn FnOnce()->Result<crate::live_publish_worker::Worker,String>>>) -> Result<Self, String> {
        match format {
            ProgramFormat::Live => {
                #[cfg(target_os = "macos")]
                { Ok(Self::Live(crate::live_program_sink::Sink::open(profile.width,profile.height,profile.fps,profile.bitrate_kbps,microphone,live_connector.ok_or("Missing authorized publisher connector")?)?)) }
                #[cfg(target_os = "windows")]
                { let _=live_connector; Err("Native live encoding requires macOS".into()) }
            }
            ProgramFormat::Flv => {
                #[cfg(target_os = "macos")]
                { Ok(Self::Flv(crate::live_flv::NativeSink::open(path,profile.width,profile.height,profile.fps,profile.bitrate_kbps,microphone)?)) }
                #[cfg(target_os = "windows")]
                { Err("Native streaming packet diagnostics require macOS".into()) }
            }
            ProgramFormat::Avi => {
                let file = OpenOptions::new().write(true).create_new(true).open(path)
                    .map_err(|e| format!("Unable to create private AVI: {e}"))?;
                Ok(Self::Avi(crate::avi::AviWriter::new(file, profile.width, profile.height, profile.fps)?))
            }
            ProgramFormat::Mp4 => {
                #[cfg(target_os = "macos")]
                { Ok(Self::Mp4(crate::program_mp4_sink::ProgramMp4Sink::new_with_settings(path, profile.width, profile.height, profile.fps, microphone, profile.bitrate_kbps)?)) }
                #[cfg(target_os = "windows")]
                { Ok(Self::Mp4(crate::windows_mp4_sink::WindowsMp4Sink::new_with_audio(path,profile.width,profile.height,profile.fps,profile.bitrate_kbps,microphone)?)) }
            }
        }
    }

    fn append(&mut self, canvas: image::RgbaImage) -> Result<(), String> {
        match self {
            Self::Avi(writer) => {
                use image::{codecs::jpeg::JpegEncoder, DynamicImage};
                let rgb = DynamicImage::ImageRgba8(canvas).to_rgb8();
                let mut jpeg = Vec::new();
                JpegEncoder::new_with_quality(&mut jpeg, 72)
                    .encode_image(&DynamicImage::ImageRgb8(rgb))
                    .map_err(|e| format!("Program frame JPEG encoding failed: {e}"))?;
                writer.write_frame(&jpeg)
            }
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Self::Mp4(writer) => writer.append(canvas.as_raw()),
            #[cfg(target_os = "macos")]
            Self::Flv(writer) => writer.append(canvas),
            #[cfg(target_os = "macos")]
            Self::Live(writer) => writer.append(canvas),
        }
    }

    fn append_audio(&mut self, pcm: &[f32]) -> Result<(),String> {
        match self {
            Self::Mp4(writer) => writer.append_audio(pcm),
            #[cfg(target_os = "macos")]
            Self::Flv(writer) => writer.append_audio(pcm),
            #[cfg(target_os = "macos")]
            Self::Live(writer) => writer.append_audio(pcm),
            Self::Avi(_) => Err("AVI does not support the composed AAC diagnostic".into()),
        }
    }
    fn sent_bytes(&self)->u64{
        match self{
            #[cfg(target_os="macos")]
            Self::Live(w)=>w.sent_bytes(),
            _=>0,
        }
    }
    fn finish(&mut self) -> Result<(), (String, bool)> {
        match self {
            Self::Avi(writer) => writer.finish().map(|_| ()).map_err(|e| (e, false)),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Self::Mp4(writer) => writer.finish(),
            #[cfg(target_os = "macos")]
            Self::Flv(writer) => writer.finish().map_err(|e|(e,false)),
            #[cfg(target_os = "macos")]
            Self::Live(writer) => writer.finish().map_err(|e|(e,false)),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn capture_to_dir(
    directory: PathBuf, ids: Vec<usize>, seconds: u32, profile: EncodingProfile, format: ProgramFormat, layout: crate::scene_compositor::SceneLayout, webcam: bool,
    microphone: bool, mic_gain: f32, overlay: Option<crate::program_overlay::OverlayInput>, video_device_id: Option<String>, audio_inputs: Option<Vec<crate::program_audio::AudioInput>>, transforms: Option<Vec<crate::source_transform::SourceTransform>>, stop_guard: Option<&ProgramStopGuard>,
    live_output:Option<LiveOutput>,
) -> Result<ContinuousProgramReport, String> {
    let live=matches!(format,ProgramFormat::Live);
    if live!=live_output.is_some(){return Err("Live output configuration mismatch".into())}
    let live_stop=live_output.as_ref().map(|o|o.stop.clone());
    let live_connector=live_output.map(|o|o.connect);
    let clock = crate::media_clock::MediaClock::new(profile.fps)?;
    let graphic = overlay.map(|input| input.prepare(profile.width, profile.height)).transpose()?;
    #[cfg(target_os = "macos")]
    if microphone || audio_inputs.as_ref().is_some_and(|inputs| inputs.iter().any(|i| i.kind == "device")) { crate::program_microphone::ProgramMicrophone::authorize()?; }
    // Explicit opt-in; permission is resolved before starting the display receivers.
    #[cfg(target_os = "macos")]
    let mut camera = if webcam {
        let mut camera = crate::program_camera::ProgramCamera::start()?;
        let frame = camera.first_frame()?;
        Some((camera, frame, Instant::now()))
    } else { None };
    let mut webcam_frames = if webcam { 1u32 } else { 0 };
    let mut webcam_repeated_slots = 0u32;
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    let mut primary_camera = video_device_id.as_deref().map(|id| crate::program_camera::ProgramCamera::start_selected(Some(id))).transpose()?;
    let source_count = ids.len() + usize::from(video_device_id.is_some());
    let available = if ids.is_empty() { Vec::new() } else { xcap::Monitor::all()
        .map_err(|error| format!("Unable to enumerate Program displays: {error}"))? };
    let mut recorders = Vec::with_capacity(ids.len());
    let mut receivers = Vec::with_capacity(ids.len());
    for id in &ids {
        let monitor = available.get(*id)
            .ok_or_else(|| "One Program display is disconnected; refresh displays".to_owned())?;
        let (recorder, receiver) = monitor.video_recorder()
            .map_err(|error| format!("Continuous display source could not start: {error}"))?;
        recorders.push(recorder);
        receivers.push(receiver);
    }
    let guard = RecorderGuard(recorders);
    for recorder in &guard.0 {
        recorder.start().map_err(|error| {
            format!("Failed to start a native video source. Check capture consent: {error}")
        })?;
    }

    // Keep the latest complete frame from each source, not an accumulating
    // recording in memory. Output is clocked independently at nominal 10fps.
    // The receiver channels are owned by the underlying XCap API; our per-tick
    // draining is bounded, and device-side backlog must be hardware-tested.
    let mut latest = Vec::with_capacity(ids.len());
    let mut last_received = Vec::with_capacity(ids.len());
    let mut total_pixels = 0u64;
    for receiver in &receivers {
        let frame = receive_first_frame(receiver)?;
        total_pixels = validate_frame_geometry(frame.width(), frame.height(), total_pixels)?;
        latest.push(frame);
        last_received.push(Instant::now());
    }
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    if let Some(source) = primary_camera.as_mut() {
        let frame = source.first_frame()?;
        total_pixels = validate_frame_geometry(frame.width(), frame.height(), total_pixels)?;
        latest.push(frame);
        last_received.push(Instant::now());
    }
    if !live{fs::create_dir_all(&directory)
        .map_err(|error| format!("Unable to create private clips directory: {error}"))?;}
    let id = uuid::Uuid::new_v4().to_string();
    let extension = format.extension();
    let final_path = directory.join(format!("clip-{id}.{extension}"));
    let temporary = directory.join(format!(".clip-{id}.part.{extension}"));
    let audio_input_count = audio_inputs.as_ref().map_or(usize::from(microphone), |inputs| inputs.len());
    if live_stop.as_ref().is_some_and(|s|s.load(Ordering::Acquire)){return Err("Live Program cancelled before connection".into())}
    let mut writer = match FrameSink::open(&temporary, format, profile, audio_input_count > 0,live_connector) {
        Ok(writer) => writer,
        Err(error) => { if !live{let _ = fs::remove_file(&temporary);} return Err(error); }
    };

    #[cfg(target_os = "macos")]
    let mut mic = if microphone && audio_inputs.is_none() {
        match crate::program_microphone::ProgramMicrophone::start() {
            Ok(source) => Some(source),
            Err(error) => { drop(writer); if !live{let _ = fs::remove_file(&temporary);} return Err(error); }
        }
    } else { None };
    #[cfg(target_os = "macos")]
    let mixer = if let Some(ref inputs) = audio_inputs {
        if inputs.is_empty() { None } else {
            match crate::program_audio::Mixer::open(inputs) {
                Ok(mixer) => Some(mixer),
                Err(error) => { drop(writer); if !live{let _=fs::remove_file(&temporary);} return Err(error); }
            }
        }
    } else { None };
    #[cfg(target_os = "windows")]
    let windows_mixer = {
        let inputs=audio_inputs.clone().unwrap_or_else(||if microphone {vec![crate::program_audio::AudioInput{kind:"device".into(),device_id:None,gain:mic_gain,muted:false}]}else{vec![]});
        if inputs.is_empty(){None}else{
            match crate::windows_audio::Mixer::open(inputs){
                Ok(mixer)=>Some(mixer),
                Err(error)=>{drop(writer);if !live{let _=fs::remove_file(&temporary);}return Err(error);}
            }
        }
    };
    let mut audio_missing_source_frames=0u64;
    let mut audio_samples_written = 0u64;
    let mut audio_peak = 0.0f32;
    let mut block_audio_peak = 0.0f32;
    let mut audio_clipped_samples = 0usize;
    let started = Instant::now();
    let total_frames = profile.fps * seconds;
    let mut written_frames = 0u32;
    let mut wall_tick = 0u32;
    let mut paused_ticks = 0u32;
    let mut file_bytes = 0u64;
    let mut received = vec![1u32; source_count];
    let mut repeats = vec![0u32; source_count];
    let mut late_frames = 0u32;
    let mut max_skew_ms = 0u64;
    let mut geometry=SceneGeometry{layout,transforms};
    let mut transition:Option<(SceneGeometry,TransitionKind,u32,u32)>=None;
    let mut transitions_completed=0u32;
    let recording = (|| -> Result<(), String> {
        while written_frames < total_frames {
            if live_stop.as_ref().is_some_and(|s|s.load(Ordering::Acquire)){break;}
            if live&&started.elapsed()>=Duration::from_secs(u64::from(seconds)+2){return Err("Live Program exceeded its wall-clock budget".into())}
            // Finish at a complete A/V tick. Limit pauses to 30 nominal seconds
            // and bound the total session wall time even if inputs run slowly.
            if written_frames >= 2 && stop_guard.is_some_and(|guard|
                guard.requested() || paused_ticks >= profile.fps * 30 ||
                started.elapsed() >= Duration::from_secs(u64::from(seconds) + 30)) { break; }
            let paused = written_frames >= 2 && stop_guard.is_some_and(|guard| guard.paused());
            let tick = written_frames;
            let target = started + clock.time(wall_tick);
            wall_tick += 1;
            std::thread::sleep(target.saturating_duration_since(Instant::now()));
            for (index, receiver) in receivers.iter().enumerate() {
                let mut was_updated = false;
                for _ in 0..MAX_DRAIN_PER_SOURCE_PER_TICK {
                    match receiver.try_recv() {
                        Ok(frame) => {
                            let image = frame_image(frame)?;
                            let next_pixels = u64::from(image.width()) * u64::from(image.height());
                            let previous_pixels = u64::from(latest[index].width()) *
                                u64::from(latest[index].height());
                            // Reject hotplug/resolution changes that exceed memory budget.
                            if total_pixels - previous_pixels + next_pixels > MAX_TOTAL_SOURCE_PIXELS {
                                return Err("Program displays exceeded the native capture memory budget".into());
                            }
                            total_pixels = total_pixels - previous_pixels + next_pixels;
                            latest[index] = image;
                            last_received[index] = Instant::now();
                            received[index] = received[index].saturating_add(1);
                            was_updated = true;
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) =>
                            return Err("One continuous Program display source disconnected".into()),
                    }
                }
                if !paused && !was_updated { repeats[index] = repeats[index].saturating_add(1); }
            }
            #[cfg(any(target_os = "macos",target_os = "windows"))]
            if let Some(source) = primary_camera.as_mut() {
                if let Some(frame) = source.poll()? {
                    validate_frame_geometry(frame.width(), frame.height(), 0)?;
                    latest[0] = frame;
                    last_received[0] = Instant::now();
                    received[0] = received[0].saturating_add(1);
                } else if !paused { repeats[0] = repeats[0].saturating_add(1); }
            }
            let now = Instant::now();
            let most_recent = *last_received.iter().max().unwrap();
            let oldest = *last_received.iter().min().unwrap();
            max_skew_ms = max_skew_ms.max(most_recent.duration_since(oldest).as_millis() as u64);
            if last_received.iter().any(|last| now.duration_since(*last).as_millis() as u64 > MAX_STALE_MS) {
                return Err("Program video input stalled for over 1.5 seconds; incomplete file discarded".into());
            }
            #[cfg(target_os = "macos")]
            if let Some((camera, frame, last)) = camera.as_mut() {
                if let Some(new_frame) = camera.poll()? {
                    *frame = new_frame;
                    *last = Instant::now();
                    webcam_frames = webcam_frames.saturating_add(1);
                } else if !paused {
                    webcam_repeated_slots = webcam_repeated_slots.saturating_add(1);
                }
                if last.elapsed() > Duration::from_millis(MAX_STALE_MS) {
                    return Err("Webcam stopped delivering frames; incomplete recording discarded".into());
                }
                validate_frame_geometry(frame.width(), frame.height(), total_pixels)?;
            }
            block_audio_peak=0.;
            if paused {
                // Drain input queues while paused; no video or PCM is submitted
                // to the encoder. Resume continues the contiguous media clock.
                #[cfg(target_os = "macos")]
                {
                    let range = clock.audio_range(tick, 48_000)?;
                    let frames = (range.end - range.start) as usize;
                    if let Some(ref mut source) = mic { let _ = source.read(frames, mic_gain)?; }
                    if let (Some(mixer), Some(inputs)) = (&mixer, &audio_inputs) { let _ = mixer.read(frames, inputs)?; }
                }
                #[cfg(target_os = "windows")]
                if let Some(mixer)=&windows_mixer{mixer.discard()?;}
                paused_ticks += 1;
            } else {
            if let Some(take)=stop_guard.and_then(|g|g.next_take()){
                let old=std::mem::replace(&mut geometry,take.target);
                transition=Some((old,take.kind,take.kind.frames(profile.fps,take.duration_ms),0));
            }
            #[cfg(target_os = "macos")]
            let mut composed = if let Some((_, frame, _)) = camera.as_ref() {
                // Borrow all display/camera images; no full-resolution source clone.
                crate::scene_compositor::composite_with_camera(&latest, frame, profile.width, profile.height)?
            } else {
                compose_geometry(&latest,profile.width,profile.height,&geometry)?
            };
            #[cfg(not(target_os = "macos"))]
            let mut composed = compose_geometry(&latest,profile.width,profile.height,&geometry)?;
            let mut finished_take=false;
            if let Some((old,kind,total,step))=transition.as_mut(){
                *step+=1;
                if *step<*total {
                    let outgoing=compose_geometry(&latest,profile.width,profile.height,old)?;
                    transition_pixels(&outgoing,&mut composed,*kind,*step,*total);
                }
                finished_take=*step>=*total;
            }
            if let Some(ref overlay) = graphic { crate::program_overlay::apply(&mut composed, overlay); }
            writer.append(composed)?;
            #[cfg(target_os = "macos")]
            if let Some(ref mut source) = mic {
                let range = clock.audio_range(tick, 48_000)?;
                let mixed = source.read((range.end-range.start) as usize, mic_gain)?;
                writer.append_audio(&mixed.samples)?;
                audio_samples_written = range.end;
                block_audio_peak=mixed.peak;
                audio_peak = audio_peak.max(mixed.peak);
                audio_clipped_samples = audio_clipped_samples.saturating_add(mixed.clipped);
            }
            #[cfg(target_os = "macos")]
            if let (Some(mixer), Some(inputs)) = (&mixer, &audio_inputs) {
                let range = clock.audio_range(tick,48_000)?;
                let (mixed,_) = mixer.read((range.end-range.start) as usize,inputs)?;
                writer.append_audio(&mixed.samples)?;
                audio_samples_written = range.end;
                block_audio_peak=mixed.peak;
                audio_peak = audio_peak.max(mixed.peak);
                audio_clipped_samples = audio_clipped_samples.saturating_add(mixed.clipped);
            }
            #[cfg(target_os = "windows")]
            if let Some(mixer)=&windows_mixer{
                let range=clock.audio_range(tick,48000)?;
                let(mixed,missing)=mixer.read((range.end-range.start) as usize)?;
                writer.append_audio(&mixed.samples)?;
                audio_samples_written=range.end;
                audio_missing_source_frames=audio_missing_source_frames.saturating_add(missing);
                block_audio_peak=mixed.peak;
                audio_peak=audio_peak.max(mixed.peak);
                audio_clipped_samples=audio_clipped_samples.saturating_add(mixed.clipped);
            }
            written_frames += 1;
            if finished_take {
                transition=None;
                transitions_completed+=1;
                if let Some(g)=stop_guard {g.complete_take();}
            }
            if Instant::now() > started + clock.time(wall_tick) {
                late_frames = late_frames.saturating_add(1);
            }
            }
            if let Some(guard) = stop_guard {
                if wall_tick == 1 || wall_tick % (profile.fps / 2).max(1) == 0 {
                    file_bytes = fs::metadata(&temporary).map(|m| m.len()).unwrap_or(file_bytes);
                    if file_bytes > 64 * 1024 * 1024 { return Err("Program MP4 exceeded the 64 MiB safety budget".into()); }
                }
                guard.update(ProgramRecordingStats {
                    phase: if paused { "paused" } else { "recording" },
                    recorded_frames: written_frames, fps: profile.fps,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    paused_ms: clock.time(paused_ticks).as_millis() as u64,
                    file_bytes, late_frames,
                    segment_index: 1, segment_count: 1, completed_segments: 0,
                    transition_busy:false, transitions_completed:0,
                audio_peak:block_audio_peak, audio_missing_source_frames,
                    repeated_source_slots: repeats.iter().map(|n| u64::from(*n)).sum::<u64>() + u64::from(webcam_repeated_slots),
                });
            }
        }
        Ok(())
    })();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let paused_ms = clock.time(paused_ticks).as_millis() as u64;
    if let Some(guard) = stop_guard { guard.finalizing(); }
    // Stop native input before waiting on the final container writer.
    drop(guard);
    drop(receivers);
    #[cfg(target_os = "macos")]
    drop(camera);
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    drop(primary_camera);
    #[cfg(target_os = "macos")]
    drop(mic);
    #[cfg(target_os = "macos")]
    drop(mixer);
    #[cfg(target_os = "windows")]
    drop(windows_mixer);
    if let Err(error) = recording {
        drop(writer);
        if !live{let _ = fs::remove_file(&temporary);}
        return Err(error);
    }
    let finalizing = Instant::now();
    let result = writer.finish();
    let sent_bytes=writer.sent_bytes();
    drop(writer);
    if let Err((error, still_writing)) = result {
        // A timed-out AVFoundation writer may still own the private file.
        if !still_writing { if !live{let _ = fs::remove_file(&temporary);} }
        return Err(error);
    }
    let inspected = (|| -> Result<(u64, f64), String> {
        match format {
            ProgramFormat::Avi => {
                let file = OpenOptions::new().read(true).write(true).open(&temporary)
                    .map_err(|e| format!("Cannot reopen completed AVI: {e}"))?;
                let bytes = file.metadata().map_err(|e| e.to_string())?.len();
                if bytes == 0 || bytes > 64 * 1024 * 1024 { return Err("AVI file outside safety budget".into()); }
                file.sync_all().map_err(|e| e.to_string())?;
                Ok((bytes, f64::from(seconds)))
            }
            ProgramFormat::Mp4 => {
                crate::program_mp4_storage::validate_finished(&temporary)
            }
            ProgramFormat::Live => Ok((sent_bytes,f64::from(written_frames)/f64::from(profile.fps))),
            ProgramFormat::Flv => {
                let bytes=fs::metadata(&temporary).map_err(|e|e.to_string())?.len();
                if !(14..=64*1024*1024).contains(&bytes){return Err("FLV outside file budget".into())}
                Ok((bytes,f64::from(written_frames)/f64::from(profile.fps)))
            }
        }
    })();
    let (bytes, actual_duration) = match inspected {
        Ok(value) => value,
        Err(error) => { if !live{let _ = fs::remove_file(&temporary);} return Err(error); }
    };
    let finalization_ms = finalizing.elapsed().as_millis() as u64;
    match format {
        ProgramFormat::Live => {},
        ProgramFormat::Flv => {
            if let Err(e)=crate::program_mp4_storage::publish_ready(&temporary,&final_path){if !live{let _=fs::remove_file(&temporary);}return Err(e);}
        }
        ProgramFormat::Avi => {
            if let Err(error) = fs::rename(&temporary, &final_path) {
                if !live{let _ = fs::remove_file(&temporary);}
                return Err(format!("Unable to publish completed Program video: {error}"));
            }
        }
        ProgramFormat::Mp4 => {
            {
                // The encoder is closed and the container has passed validation.
                // A distinct ready name permits explicit recovery after a failed
                // or interrupted publication, without treating raw .part as valid.
                let ready = directory.join(format!(".clip-{id}.ready.mp4"));
                fs::rename(&temporary, &ready)
                    .map_err(|e| format!("Finalized MP4 retained as a private partial file; could not stage recovery: {e}"))?;
                crate::program_mp4_storage::publish_ready(&ready, &final_path)?;
            }

        }
    }
    Ok(ContinuousProgramReport {
        id, path: if live{String::new()}else{final_path.to_string_lossy().into_owned()}, format: extension,
        bytes, duration_seconds: actual_duration, requested_max_seconds: seconds,
        stopped_early: written_frames < total_frames, paused_ms,
        target_bitrate_kbps: matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv|ProgramFormat::Live).then_some(profile.bitrate_kbps),
        frame_count: written_frames, width: profile.width, height: profile.height, fps: profile.fps,
        profile: profile.label,
        display_count: ids.len(), video_input_count: usize::from(video_device_id.is_some()), elapsed_ms, finalization_ms, audio_track_verified: false,
        webcam_included: webcam, webcam_frames, webcam_repeated_slots,
        planned_audio_samples_48khz: clock.audio_range(written_frames - 1, 48_000)?.end,
        microphone_included: microphone, audio_input_count, audio_samples_written, audio_missing_source_frames, audio_peak, audio_clipped_samples,
        overlay_included: graphic.is_some(),
        measured_sampling_fps: if elapsed_ms <= paused_ms { 0.0 }
            else { f64::from(written_frames) * 1000.0 / (elapsed_ms - paused_ms) as f64 },
        source_frames_received: received,
        repeated_source_slots: repeats,
        late_output_frames: late_frames,
        max_source_arrival_skew_ms: max_skew_ms,
        transitions_completed,
        segmented_session: None,
    })
}

// The connector is injected by an explicitly authorized native caller.
// Neither the browser nor Tauri command dispatcher can invoke this path yet.
#[cfg(any(target_os="macos",target_os="windows"))]
struct LiveOutput{
    connect:Box<dyn FnOnce()->Result<crate::live_publish_worker::Worker,String>>,
    stop:std::sync::Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(target_os="macos")]
pub(crate) fn capture_authorized_live_program(
    display_ids:Vec<usize>,video_device_id:Option<String>,seconds:u32,profile:&str,
    bitrate_kbps:Option<u32>,layout:Option<&str>,
    overlay:Option<crate::program_overlay::OverlayInput>,audio_inputs:Vec<crate::program_audio::AudioInput>,
    transforms:Option<Vec<crate::source_transform::SourceTransform>>,
    stop:std::sync::Arc<std::sync::atomic::AtomicBool>,
    connect:impl FnOnce()->Result<crate::live_publish_worker::Worker,String>+'static,
)->Result<ContinuousProgramReport,String>{
    if !(2..=60).contains(&seconds){return Err("Live integration test supports 2–60 seconds".into())}
    let mut p=encoding_profile(profile)?;
    p.bitrate_kbps=validate_bitrate(bitrate_kbps,p.bitrate_kbps)?;
    if let Some(id)=video_device_id.as_ref(){
        if !display_ids.is_empty()||id.is_empty()||id.len()>1024||id.chars().any(char::is_control){
            return Err("Select one video device or up to four displays".into())
        }
    }else{crate::scene_compositor::validate_ids(&display_ids)?;}
    crate::program_audio::validate(&audio_inputs)?;
    crate::source_transform::validate_list(transforms.as_deref(),display_ids.len()+usize::from(video_device_id.is_some()))?;
    let layout=crate::scene_compositor::SceneLayout::parse(layout)?;
    if stop.load(Ordering::Acquire){return Err("Live Program cancelled before capture".into())}
    use crate::clip::{CaptureLock,CAPTURE_BUSY};
    CAPTURE_BUSY.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire)
        .map_err(|_|"Another native capture is active")?;
    let _guard=CaptureLock;
    capture_to_dir(PathBuf::new(),display_ids,seconds,p,ProgramFormat::Live,layout,false,false,1.,overlay,
        video_device_id,Some(audio_inputs),transforms,None,Some(LiveOutput{connect:Box::new(connect),stop}))
}

#[tauri::command]
pub async fn capture_continuous_program_avi(
    app: tauri::AppHandle, display_ids: Vec<usize>, seconds: u32, profile: String, layout: Option<String>,
    overlay: Option<crate::program_overlay::OverlayInput>, transforms: Option<Vec<crate::source_transform::SourceTransform>>,
) -> Result<ContinuousProgramReport, String> {
    capture_continuous_program(app, display_ids, seconds, profile, ProgramFormat::Avi, layout, false, false, 1.0, overlay, None, None, transforms, None, None).await
}

#[tauri::command]
pub async fn capture_continuous_program_mp4(
    app: tauri::AppHandle, display_ids: Vec<usize>, seconds: u32, profile: String, layout: Option<String>, webcam: Option<bool>, microphone: Option<bool>, mic_gain: Option<f32>,
    overlay: Option<crate::program_overlay::OverlayInput>, video_device_id: Option<String>, audio_inputs: Option<Vec<crate::program_audio::AudioInput>>, transforms: Option<Vec<crate::source_transform::SourceTransform>>, session_id: Option<String>, bitrate_kbps: Option<u32>,
) -> Result<ContinuousProgramReport, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    { capture_continuous_program(app, display_ids, seconds, profile, ProgramFormat::Mp4, layout, webcam.unwrap_or(false), microphone.unwrap_or(false), mic_gain.unwrap_or(1.0), overlay, video_device_id, audio_inputs, transforms, session_id, bitrate_kbps).await }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    { let _ = (app, display_ids, seconds, profile, layout, webcam, microphone, mic_gain, overlay, video_device_id, audio_inputs, transforms, session_id, bitrate_kbps); Err("Composed H.264 MP4 requires Mac or Windows".into()) }
}

#[tauri::command]
pub async fn capture_program_live_packet_test(
    app:tauri::AppHandle,display_ids:Vec<usize>,seconds:u32,profile:String,layout:Option<String>,
    overlay:Option<crate::program_overlay::OverlayInput>,video_device_id:Option<String>,
    audio_inputs:Option<Vec<crate::program_audio::AudioInput>>,transforms:Option<Vec<crate::source_transform::SourceTransform>>,bitrate_kbps:Option<u32>,
)->Result<ContinuousProgramReport,String>{
    if !cfg!(target_os="macos")||!(2..=8).contains(&seconds){return Err("Offline streaming packet test requires macOS and 2–8 seconds".into())}
    capture_continuous_program(app,display_ids,seconds,profile,ProgramFormat::Flv,layout,false,false,1.,overlay,video_device_id,audio_inputs,transforms,None,bitrate_kbps).await
}

async fn capture_continuous_program(
    app: tauri::AppHandle, display_ids: Vec<usize>, seconds: u32, profile: String, format: ProgramFormat, layout: Option<String>, webcam: bool, microphone: bool, mic_gain: f32,
    overlay: Option<crate::program_overlay::OverlayInput>, video_device_id: Option<String>, audio_inputs: Option<Vec<crate::program_audio::AudioInput>>, transforms: Option<Vec<crate::source_transform::SourceTransform>>, session_id: Option<String>, bitrate_kbps: Option<u32>,
) -> Result<ContinuousProgramReport, String> {
    if session_id.as_ref().is_some_and(|id| uuid::Uuid::parse_str(id).is_err()) { return Err("Invalid Program recording session ID".into()); }
    if cfg!(target_os = "windows") && matches!(format, ProgramFormat::Mp4) && (webcam || seconds > MAX_MP4_SECONDS) { return Err("Windows MP4 supports one video device or displays up to 60 seconds; separate webcam inset is Mac-only".into()); }
    let validate_duration = if matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv) { validate_mp4_request } else { validate_request };
    if let Some(ref inputs) = audio_inputs {
        crate::program_audio::validate(inputs)?;
        if !cfg!(any(target_os = "macos",target_os = "windows")) || !matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv) { return Err("Audio inputs require native H.264 output".into()); }
        #[cfg(target_os = "windows")]
        crate::windows_audio::validate(inputs)?;
    }
    crate::source_transform::validate_list(transforms.as_deref(), display_ids.len()+usize::from(video_device_id.is_some()))?;
    if transforms.is_some() && webcam { return Err("Disable the separate webcam inset for manual scene transforms".into()); }
    let mut selected_profile = if let Some(ref id) = video_device_id {
        if !cfg!(any(target_os = "macos",target_os = "windows")) || !matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv) || !display_ids.is_empty() || webcam || id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err("Video device recording requires native MP4, one selected video input only, and webcam inset off".into());
        }
        validate_duration(&[0], seconds, &profile)?
    } else { validate_duration(&display_ids, seconds, &profile)? };
    if bitrate_kbps.is_some() && !matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv) { return Err("Bitrate presets require MP4".into()); }
    selected_profile.bitrate_kbps = validate_bitrate(bitrate_kbps, selected_profile.bitrate_kbps)?;
    if selected_profile.label == "1080p30" && (!cfg!(any(target_os = "macos",target_os = "windows")) || !matches!(format, ProgramFormat::Mp4|ProgramFormat::Flv)) {
        return Err("1080p30 diagnostic requires a native H.264 MP4 encoder".into());
    }
    if webcam && (!cfg!(target_os = "macos") || display_ids.len() > 3) {
        return Err("Webcam PiP requires macOS and at most three displays".into());
    }
    if !mic_gain.is_finite() || !(0.0..=2.0).contains(&mic_gain) { return Err("Invalid microphone gain".into()); }
    let layout = crate::scene_compositor::SceneLayout::parse(layout.as_deref())?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use crate::clip::{CaptureLock, CAPTURE_BUSY};
        CAPTURE_BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "Another native recording or capture diagnostic is already running".to_owned())?;
        let directory = match app.path().app_data_dir() {
            Ok(path) => path.join("test-clips"),
            Err(error) => {
                CAPTURE_BUSY.store(false, Ordering::Release);
                return Err(format!("Unable to locate MooBee clips directory: {error}"));
            }
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = CaptureLock;
            let stop_guard = if matches!(format, ProgramFormat::Mp4) { Some(ProgramStopGuard::begin(session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()))) } else { None };
            if seconds<=MAX_MP4_SECONDS && !webcam {
                if let Some(g)=stop_guard.as_ref(){g.allow_takes(display_ids.clone(),video_device_id.clone());}
            }
            if !matches!(format, ProgramFormat::Mp4) || seconds <= MAX_MP4_SECONDS {
                return capture_to_dir(directory, display_ids, seconds, selected_profile, format, layout, webcam, microphone, mic_gain, overlay, video_device_id, audio_inputs, transforms, stop_guard.as_ref(),None);
            }
            let plan = segment_plan(seconds)?;
            let mut segments = Vec::with_capacity(plan.len());
            let mut last = None;
            let mut warning = None;
            let mut stopped_early = false;
            for (index, &length) in plan.iter().enumerate() {
                if let Some(guard) = stop_guard.as_ref() {
                    if guard.requested() { stopped_early = true; break; }
                    guard.begin_segment(index as u32 + 1, plan.len() as u32);
                    if guard.requested() { stopped_early = true; break; }
                }
                // Restart sources at boundaries; this intentionally does not
                // promise a seamless continuous recording or gapless audio.
                match capture_to_dir(directory.clone(), display_ids.clone(), length, selected_profile, format,
                    layout, webcam, microphone, mic_gain, overlay.clone(), video_device_id.clone(),
                    audio_inputs.clone(), transforms.clone(), stop_guard.as_ref(),None) {
                    Ok(report) => {
                        segments.push(RecordingSegment {
                            id: report.id.clone(), path: report.path.clone(), bytes: report.bytes,
                            duration_seconds: report.duration_seconds, frame_count: report.frame_count,
                        });
                        stopped_early = report.stopped_early;
                        last = Some(report);
                        if stopped_early { break; }
                    }
                    Err(error) => {
                        if last.is_none() { return Err(error); }
                        warning = Some(format!("Segment {} failed: {error}. Previously finalized clips were retained.", index + 1));
                        stopped_early = true;
                        break;
                    }
                }
            }
            let mut report = last.ok_or_else(|| "Recording stopped before any segment was finalized".to_owned())?;
            report.segmented_session = Some(SegmentedProgramReport {
                requested_seconds: seconds,
                completed_seconds: segments.iter().map(|s| s.duration_seconds).sum(),
                total_bytes: segments.iter().map(|s| s.bytes).sum(),
                stopped_early: stopped_early || segments.len() < plan.len(),
                warning, segments,
            });
            Ok(report)
        }).await.map_err(|error| format!("Continuous Program capture worker stopped: {error}"))?
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (app, display_ids, selected_profile, format, layout, webcam, microphone, mic_gain, overlay, video_device_id, audio_inputs, transforms, session_id, bitrate_kbps);
        Err("Continuous Program video capture requires a Mac or Windows desktop build".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_request, validate_frame_geometry, MAX_TOTAL_SOURCE_PIXELS};
    #[test]
    fn mp4_duration_and_stop_are_bounded_and_session_scoped() {
        use super::{validate_mp4_request, ProgramStopGuard, stop_continuous_program_mp4};
        assert!(validate_mp4_request(&[0], 30, "1080p30").is_ok());
        assert!(validate_mp4_request(&[0], 60, "1080p30").is_ok());
        assert!(validate_mp4_request(&[0], 61, "1080p30").is_err());
        assert!(validate_mp4_request(&[0], 300, "1080p30").is_ok());
        assert!(validate_mp4_request(&[0], 360, "1080p30").is_err());
        use super::validate_bitrate;
        assert_eq!(validate_bitrate(None, 6000).unwrap(), 6000);
        assert_eq!(validate_bitrate(Some(1500), 6000).unwrap(), 1500);
        assert!(validate_bitrate(Some(8000), 6000).is_err());
        assert!(validate_mp4_request(&[0], 1, "baseline").is_err());
        let guard = ProgramStopGuard::begin("current".into());
        assert!(!stop_continuous_program_mp4("old".into()));
        assert!(!guard.requested());
        use super::{continuous_program_stats, pause_continuous_program_mp4};
        assert!(continuous_program_stats("old".into()).is_none());
        assert!(!pause_continuous_program_mp4("current".into(), true));
        let mut stats = continuous_program_stats("current".into()).unwrap();
        stats.phase = "recording";
        guard.update(stats);
        assert!(!pause_continuous_program_mp4("old".into(), true));
        assert!(pause_continuous_program_mp4("current".into(), true));
        assert!(guard.paused());
        assert!(pause_continuous_program_mp4("current".into(), false));
        assert!(!guard.paused());
        use super::queue_program_scene_take;
        guard.allow_takes(vec![0,1],None);
        let take=|id:&str,ids:Vec<usize>|queue_program_scene_take(id.into(),ids,None,Some("pip".into()),None,"fade".into(),300);
        assert!(take("old",vec![0,1]).is_err());
        assert!(take("current",vec![1,0]).is_err());
        assert!(take("current",vec![0,2]).is_err());
        assert!(take("current",vec![0,1]).is_ok());
        assert!(take("current",vec![0,1]).is_err());
        let queued=guard.next_take().unwrap();
        assert_eq!(queued.kind,super::TransitionKind::Fade);
        assert_eq!(queued.target.layout,crate::scene_compositor::SceneLayout::PictureInPicture);
        assert!(continuous_program_stats("current".into()).unwrap().transition_busy);
        guard.complete_take();
        let stats=continuous_program_stats("current".into()).unwrap();
        assert!(!stats.transition_busy);
        assert_eq!(stats.transitions_completed,1);
        assert!(guard.next_take().is_none());
        assert!(stop_continuous_program_mp4("current".into()));
        assert!(guard.requested());
        assert!(!pause_continuous_program_mp4("current".into(), true));
        guard.finalizing();
        assert!(!stop_continuous_program_mp4("current".into()));
        guard.begin_segment(2, 3);
        let next = continuous_program_stats("current".into()).unwrap();
        assert_eq!((next.segment_index, next.segment_count, next.completed_segments), (2,3,1));
        assert_eq!(next.recorded_frames, 0);
        assert!(guard.requested()); // rolling over never clears a stop request
        guard.finalizing();
        assert!(stop_continuous_program_mp4("current".into()));
        drop(guard);
        assert!(!stop_continuous_program_mp4("current".into()));
    }
    #[test]
    fn transition_timing_is_bounded(){
        assert_eq!(super::TransitionKind::parse("fade",300).unwrap().frames(30,300),9);
        assert_eq!(super::TransitionKind::Cut.frames(30,300),1);
        assert!(super::TransitionKind::parse("unknown",300).is_err());
        assert!(super::TransitionKind::parse("wipe",2000).is_err());
    }
    #[cfg(any(target_os="macos",target_os="windows"))]
    #[test]
    fn transition_pixels_are_encoded_canvas_pixels(){
        use image::{Rgba,RgbaImage};
        use super::{TransitionKind,transition_pixels};
        let old=RgbaImage::from_pixel(16,16,Rgba([200,0,0,255]));
        let target=RgbaImage::from_pixel(16,16,Rgba([0,0,200,255]));
        let mut next=target.clone();
        transition_pixels(&old,&mut next,TransitionKind::Fade,1,2);
        assert_eq!(next.get_pixel(0,0).0,[100,0,100,255]);
        for kind in [TransitionKind::Slide,TransitionKind::Wipe]{
            let mut next=target.clone();transition_pixels(&old,&mut next,kind,1,2);
            assert_ne!(next.get_pixel(0,0),next.get_pixel(15,0));
            transition_pixels(&old,&mut next,kind,2,2);
            // At the endpoint the caller supplies a fresh incoming canvas.
            let mut end=target.clone();transition_pixels(&old,&mut end,kind,2,2);
            assert_eq!(end,target);
        }
    }
    #[test]
    fn segment_plan_is_bounded_and_never_creates_a_short_tail() {
        use super::segment_plan;
        assert_eq!(segment_plan(2).unwrap(), vec![2]);
        assert_eq!(segment_plan(60).unwrap(), vec![60]);
        assert_eq!(segment_plan(120).unwrap(), vec![60,60]);
        assert_eq!(segment_plan(300).unwrap(), vec![60;5]);
        for seconds in [0,1,61,119,301,360,u32::MAX] {
            assert!(segment_plan(seconds).is_err());
        }
    }
    #[test]
    fn validates_display_count_uniqueness_and_duration() {
        assert!(validate_request(&[0], 2, "baseline").is_ok());
        assert!(validate_request(&[0, 1, 2, 3], 8, "baseline").is_ok());
        assert!(validate_request(&[], 4, "baseline").is_err());
        assert!(validate_request(&[1, 1], 4, "baseline").is_err());
        assert!(validate_request(&[0, 1, 2, 3, 4], 4, "baseline").is_err());
        assert!(validate_request(&[0], 9, "baseline").is_err());
        let high = validate_request(&[0, 1], 8, "720p15").unwrap();
        assert_eq!((high.width, high.height, high.fps), (1280, 720, 15));
        assert_eq!(high.label, "720p15");
        assert_eq!(8 * high.fps, 120);
        assert!(validate_request(&[0], 3, "1080p60").is_err());
        let full_hd = validate_request(&[0], 4, "1080p30").unwrap();
        assert_eq!((full_hd.width, full_hd.height, full_hd.fps),(1920,1080,30));
        assert!(validate_request(&[0], 6, "1080p30").is_err());
        assert_eq!(validate_request(&[0], 4, "baseline").unwrap().fps, 10);
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn high_detail_profile_composes_and_encodes_real_720p_jpeg_frames() {
        use image::{codecs::jpeg::JpegEncoder, DynamicImage, Rgba, RgbaImage};
        let profile = validate_request(&[0, 1], 4, "720p15").unwrap();
        let red = RgbaImage::from_pixel(64, 64, Rgba([240, 10, 10, 255]));
        let blue = RgbaImage::from_pixel(64, 64, Rgba([10, 10, 240, 255]));
        let composed = crate::scene_compositor::composite_grid(
            &[red, blue], profile.width, profile.height
        ).unwrap();
        assert_eq!(composed.dimensions(), (1280, 720));
        assert!(composed.get_pixel(320, 360)[0] > 200);
        assert!(composed.get_pixel(960, 360)[2] > 200);
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 72)
            .encode_image(&DynamicImage::ImageRgb8(
                DynamicImage::ImageRgba8(composed).to_rgb8()
            )).unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1280, 720));
        let avi = crate::avi::encode_silent_mjpeg_avi(
            &vec![jpeg; 4], profile.width, profile.height, profile.fps
        ).unwrap();
        assert_eq!(&avi[0..4], b"RIFF");
        assert_eq!(&avi[8..12], b"AVI ");
        assert!(avi.windows(4).any(|window| window == b"idx1"));
    }

    #[test]
    fn frames_have_bounded_pixel_budget() {
        assert_eq!(validate_frame_geometry(1920, 1080, 0).unwrap(), 2_073_600);
        assert!(validate_frame_geometry(0, 1080, 0).is_err());
        assert!(validate_frame_geometry(1920, 1080, MAX_TOTAL_SOURCE_PIXELS).is_err());
        assert!(validate_frame_geometry(u32::MAX, u32::MAX, 0).is_err());
    }
}
