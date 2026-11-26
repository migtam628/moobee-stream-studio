mod clip;
mod capture_probe;
mod avi;
mod native_avi;
mod macos_mp4;
mod mp4_validation;
mod scene_compositor;
mod program_test_avi;
mod program_continuous_avi;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod program_mp4_storage;
#[cfg(target_os = "windows")]
mod windows_mp4_sink;
#[cfg(target_os = "windows")]
mod windows_audio;
#[cfg(target_os = "macos")]
mod program_mp4_sink;
#[cfg(any(target_os = "macos",target_os = "windows"))]
#[cfg_attr(target_os = "windows",path="windows_camera.rs")]
mod program_camera;
#[cfg(any(target_os = "macos",target_os = "windows"))]
mod video_preview;
#[cfg(target_os = "macos")]
mod program_microphone;
mod program_overlay;
mod media_clock;
mod audio_mix;
mod program_audio;
mod source_transform;
mod stream_mux;
mod rtmp_receive;
mod rtmp_session;
mod live_media_queue;
mod live_packet_pacer;
mod live_session;
#[cfg(target_os = "macos")]
mod macos_avc;
#[cfg(target_os = "macos")]
mod macos_aac;

use axum::{
    extract::State,
    response::{sse::{Event, KeepAlive, Sse}, Html},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

const OVERLAY_ADDRESS: &str = "127.0.0.1:8788";

#[derive(Clone)]
struct OverlayState {
    sender: broadcast::Sender<AlertEvent>,
}

#[derive(Clone, Serialize)]
struct AlertEvent {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
    message: String,
    time: u64,
}

#[derive(Deserialize)]
struct AlertInput {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    message: String,
}

fn clean(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() && *character != '<' && *character != '>')
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

#[tauri::command]
fn emit_test_alert(state: tauri::State<'_, OverlayState>, alert: AlertInput) -> Result<AlertEvent, String> {
    if !["follow", "subscription", "tip", "raid"].contains(&alert.kind.as_str()) {
        return Err("Unsupported alert type".into());
    }
    let kind = alert.kind;
    let name = clean(&alert.name, 48);
    let message = clean(&alert.message, 120);
    if name.is_empty() {
        return Err("A display name is required".into());
    }
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let event = AlertEvent {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        name,
        message,
        time,
    };
    // Sending is best-effort: the local overlay may not be open.
    let _ = state.sender.send(event.clone());
    Ok(event)
}

async fn overlay_page() -> Html<&'static str> {
    Html(include_str!("../resources/overlay.html"))
}

async fn overlay_events(
    State(state): State<OverlayState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.sender.subscribe();
    let stream = BroadcastStream::new(receiver).filter_map(|result| {
        match result {
            Ok(alert) => serde_json::to_string(&alert)
                .ok()
                .map(|payload| Ok(Event::default().event("alert").data(payload))),
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("connected"))
}

async fn overlay_health() -> &'static str {
    "ok"
}

async fn serve_overlays(state: OverlayState) {
    let router = Router::new()
        .route("/overlay/alerts", get(overlay_page))
        .route("/api/events", get(overlay_events))
        .route("/health", get(overlay_health))
        .with_state(state);
    match tokio::net::TcpListener::bind(OVERLAY_ADDRESS).await {
        Ok(listener) => {
            if let Err(error) = axum::serve(listener, router).await {
                eprintln!("MooBee Live overlay service stopped: {error}");
            }
        }
        Err(error) => eprintln!("MooBee Live could not bind overlay port {OVERLAY_ADDRESS}: {error}"),
    }
}

// Screenshots are the first vertical slice of MooBee's own media engine.
// Continuous GPU composition, synchronized audio, encoding and RTMP remain separate milestones.
#[derive(Serialize)]
struct DisplayInfo {
    id: usize,
    name: String,
    width: u32,
    height: u32,
}

#[tauri::command]
async fn list_audio_inputs() -> Result<serde_json::Value,String> {
    #[cfg(target_os="macos")]
    { tauri::async_runtime::spawn_blocking(program_audio::list).await.map_err(|e|e.to_string())? }
    #[cfg(target_os="windows")]
    { tauri::async_runtime::spawn_blocking(windows_audio::list).await.map_err(|e|e.to_string())? }
    #[cfg(not(any(target_os="macos",target_os="windows")))]
    { Err("Native audio input discovery requires macOS or Windows".into()) }
}
#[tauri::command]
async fn program_audio_levels(inputs: Vec<program_audio::AudioInput>) -> Result<Option<program_audio::AudioLevels>,String> {
    #[cfg(target_os="macos")]
    { tauri::async_runtime::spawn_blocking(move||program_audio::levels(inputs)).await.map_err(|e|e.to_string())? }
    #[cfg(target_os="windows")]
    { tauri::async_runtime::spawn_blocking(move||windows_audio::levels(inputs)).await.map_err(|e|e.to_string())? }
    #[cfg(not(any(target_os="macos",target_os="windows")))]
    { let _=inputs; Err("Native audio meters require macOS or Windows".into()) }
}
#[tauri::command]
async fn stop_program_audio_levels() -> Result<(),String> {
    #[cfg(target_os="macos")]
    { tauri::async_runtime::spawn_blocking(program_audio::stop).await.map_err(|e|e.to_string())? }
    #[cfg(target_os="windows")]
    { tauri::async_runtime::spawn_blocking(windows_audio::stop).await.map_err(|e|e.to_string())? }
    #[cfg(not(any(target_os="macos",target_os="windows")))]
    { Ok(()) }
}
#[tauri::command]
async fn list_video_inputs() -> Result<serde_json::Value, String> {
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    { serde_json::to_value(program_camera::list_video_inputs().await?).map_err(|e| e.to_string()) }
    #[cfg(not(any(target_os = "macos",target_os = "windows")))]
    { Err("Native video input discovery requires Mac or Windows".into()) }
}

#[tauri::command]
async fn live_video_preview_frame(device_ids: Vec<String>, layout: Option<String>, previous_key: Option<String>, transforms: Option<Vec<source_transform::SourceTransform>>) -> Result<serde_json::Value,String> {
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    { serde_json::to_value(video_preview::frame(device_ids,layout,previous_key,transforms).await?).map_err(|e|e.to_string()) }
    #[cfg(not(any(target_os = "macos",target_os = "windows")))]
    { let _=(device_ids,layout,previous_key,transforms); Err("Live video input preview requires Mac or Windows".into()) }
}

#[tauri::command]
async fn stop_video_previews() -> Result<(),String> {
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    { video_preview::stop_all().await }
    #[cfg(not(any(target_os = "macos",target_os = "windows")))]
    { Ok(()) }
}

#[tauri::command]
async fn test_video_input(device_id: String) -> Result<String, String> {
    #[cfg(any(target_os = "macos",target_os = "windows"))]
    { program_camera::test_video_input(device_id).await }
    #[cfg(not(any(target_os = "macos",target_os = "windows")))]
    { let _ = device_id; Err("Native video input testing requires Mac or Windows".into()) }
}

#[tauri::command]
fn list_displays() -> Result<Vec<DisplayInfo>, String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
        monitors
            .into_iter()
            .enumerate()
            .map(|(id, monitor)| Ok(DisplayInfo {
                id,
                name: monitor.friendly_name().map_err(|error| error.to_string())?,
                width: monitor.width().map_err(|error| error.to_string())?,
                height: monitor.height().map_err(|error| error.to_string())?,
            }))
            .collect()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

#[tauri::command]
fn capture_display(display_id: usize) -> Result<String, String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use base64::Engine as _;
        let monitor = xcap::Monitor::all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .nth(display_id)
            .ok_or_else(|| "Display not found; refresh available displays".to_string())?;
        let screenshot = monitor.capture_image().map_err(|error| error.to_string())?;
        let image = image::DynamicImage::ImageRgba8(screenshot);
        let preview = image.resize(960, 540, image::imageops::FilterType::Triangle);
        let mut cursor = std::io::Cursor::new(Vec::new());
        preview.write_to(&mut cursor, image::ImageFormat::Png).map_err(|error| error.to_string())?;
        Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(cursor.into_inner())))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = display_id;
        Err("Native display capture is not implemented on this platform yet".into())
    }
}

/// Read-only runtime diagnostics. A successful port probe is not proof of a
/// playable stream or encoder; never report future media capabilities as ready.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeServiceReport {
    displays_detected: usize,
    display_enumeration_error: Option<String>,
    overlay_health_responding: bool,
    native_mp4_supported: bool,
    native_mp4_os_version: Option<String>,
    native_mp4_reason: String,
    native_recording_verified: bool,
    audio_mixer_available: bool,
    scene_compositor_available: bool,
    rtmp_output_available: bool,
}

async fn probe_overlay_health() -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let probe = async {
        let mut stream = tokio::net::TcpStream::connect(OVERLAY_ADDRESS).await?;
        stream.write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1:8788\r\nConnection: close\r\n\r\n").await?;
        let mut response = Vec::new();
        stream.take(2048).read_to_end(&mut response).await?;
        Ok::<_, std::io::Error>(response.starts_with(b"HTTP/1.1 200 OK")
            && response.ends_with(b"ok"))
    };
    tokio::time::timeout(Duration::from_secs(2), probe)
        .await.ok().and_then(Result::ok).unwrap_or(false)
}

#[tauri::command]
async fn native_service_diagnostics() -> NativeServiceReport {
    let (displays_detected, display_enumeration_error) = match list_displays() {
        Ok(monitors) => (monitors.len(), None),
        Err(message) => (0, Some(message)),
    };
    let native_mp4 = macos_mp4::macos_mp4_availability();
    NativeServiceReport {
        displays_detected,
        display_enumeration_error,
        overlay_health_responding: probe_overlay_health().await,
        native_mp4_supported: native_mp4.supported,
        native_mp4_os_version: native_mp4.os_version,
        native_mp4_reason: native_mp4.reason,
        // Not ready until completed media and real device tests prove otherwise.
        native_recording_verified: false,
        audio_mixer_available: false,
        scene_compositor_available: false,
        rtmp_output_available: false,
    }
}

/// Open the exact Apple Screen Recording privacy settings panel.
/// This cannot grant permission; only the local user can enable MooBee.
#[tauri::command]
fn open_screen_recording_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .status()
            .map_err(|error| format!("Unable to open Screen Recording settings: {error}"))?;
        if !status.success() {
            return Err("macOS could not open Screen Recording settings. Open System Settings > Privacy & Security > Screen Recording manually.".into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Screen Recording privacy settings are only available on macOS".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (sender, _) = broadcast::channel(100);
    let state = OverlayState { sender };
    let runner_state = state.clone();
    tauri::Builder::default()
        .manage(state)
        .setup(move |_app| {
            tauri::async_runtime::spawn(serve_overlays(runner_state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![emit_test_alert, list_displays, list_video_inputs, list_audio_inputs, program_audio_levels, stop_program_audio_levels, test_video_input, live_video_preview_frame, stop_video_previews, capture_display, scene_compositor::capture_scene_snapshot, clip::capture_test_clip, clip::list_test_clips, clip::read_test_clip, clip::reveal_test_clip, capture_probe::probe_native_capture, native_avi::capture_silent_avi, program_test_avi::capture_program_test_avi, program_continuous_avi::capture_continuous_program_avi, program_continuous_avi::capture_continuous_program_mp4, program_continuous_avi::stop_continuous_program_mp4, program_continuous_avi::pause_continuous_program_mp4, program_continuous_avi::continuous_program_stats, program_continuous_avi::queue_program_scene_take, program_continuous_avi::recover_finalized_program_recordings, macos_mp4::capture_macos_mp4, macos_mp4::stop_macos_mp4, macos_mp4::macos_mp4_availability, native_service_diagnostics, open_screen_recording_settings])
        .run(tauri::generate_context!())
        .expect("failed to run MooBee Live desktop");
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn strips_html_brackets_and_control_characters() {
        assert_eq!(clean("<Mig>\n rocks", 48), "Mig rocks");
    }

    #[test]
    fn enforces_character_limits() {
        assert_eq!(clean("verylong", 4), "very");
    }
}
