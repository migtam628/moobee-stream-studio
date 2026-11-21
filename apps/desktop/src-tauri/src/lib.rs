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
    // Sending is best-effort: the overlay may not yet be open in OBS.
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
        .invoke_handler(tauri::generate_handler![emit_test_alert])
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
