//! Shared native MP4 validation and finalized-file publication.
use std::{fs::OpenOptions,path::Path};

pub(crate) fn validate_finished(path: &Path) -> Result<(u64, f64), String> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)
        .map_err(|e| format!("Cannot open finalized MP4: {e}"))?;
    let bytes = file.metadata().map_err(|e| e.to_string())?.len();
    if !(4096..=64 * 1024 * 1024).contains(&bytes) { return Err("Composed MP4 is empty or exceeds 64 MiB".into()); }
    crate::mp4_validation::validate_mp4_boxes(&mut file, bytes)?;
    let duration = crate::mp4_validation::movie_duration_seconds(&mut file, bytes)?
        .filter(|v| v.is_finite() && *v > 0.0).ok_or("Missing valid MP4 duration")?;
    file.sync_all().map_err(|e| e.to_string())?;
    Ok((bytes, duration))
}

// A ready file exists only after the encoder has finished and the MP4 has
// passed the structural guard. Publish without replacing an existing clip.
pub(crate) fn publish_ready(ready: &Path, destination: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(ready).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Recording recovery requires a regular ready file".into());
    }
    std::fs::hard_link(ready, destination)
        .map_err(|e| format!("Could not publish finalized recording; ready copy retained: {e}"))?;
    // A failure to unlink the ready name must not turn a published clip into
    // an error. The destination exists and the next recovery skips duplicates.
    let _ = std::fs::remove_file(ready);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    recovered: u32,
    skipped: u32,
    failed: u32,
}

fn recover_ready_files(directory: &Path) -> Result<RecoveryReport, String> {
    let mut report = RecoveryReport { recovered: 0, skipped: 0, failed: 0 };
    if !directory.exists() { return Ok(report); }
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(id) = name.strip_prefix(".clip-").and_then(|s| s.strip_suffix(".ready.mp4")) else { continue };
        if !uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) { continue; }
        let path = entry.path();
        let destination = directory.join(format!("clip-{id}.mp4"));
        if std::fs::symlink_metadata(&destination).is_ok() {
            report.skipped += 1;
            continue;
        }
        if !entry.file_type().is_ok_and(|t| t.is_file() && !t.is_symlink()) ||
            validate_finished(&path).is_err() ||
            publish_ready(&path, &destination).is_err() {
            report.failed += 1;
        } else { report.recovered += 1; }
    }
    Ok(report)
}

pub(crate) fn recover_finalized(app: tauri::AppHandle) -> Result<RecoveryReport, String> {
    use tauri::Manager;
    use std::sync::atomic::Ordering;
    use crate::clip::{CaptureLock, CAPTURE_BUSY};
    CAPTURE_BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "Stop the current native capture before recovery".to_owned())?;
    let _guard = CaptureLock;
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?.join("test-clips");
    recover_ready_files(&directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_publish_never_replaces_existing_destination() {
        let dir = std::env::temp_dir().join(format!("moobee-ready-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let ready = dir.join(".ready.mp4");
        let destination = dir.join("clip.mp4");
        std::fs::write(&ready, b"new").unwrap();
        std::fs::write(&destination, b"old").unwrap();
        assert!(publish_ready(&ready, &destination).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"old");
        assert!(ready.exists());
        std::fs::remove_file(&destination).unwrap();
        publish_ready(&ready, &destination).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"new");
        assert!(!ready.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn recovery_ignores_partial_and_rejects_invalid_ready_files() {
        let dir = std::env::temp_dir().join(format!("moobee-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let id = uuid::Uuid::new_v4();
        let part = dir.join(format!(".clip-{id}.part.mp4"));
        let ready = dir.join(format!(".clip-{id}.ready.mp4"));
        std::fs::write(&part, b"incomplete").unwrap();
        std::fs::write(&ready, b"invalid").unwrap();
        let result = recover_ready_files(&dir).unwrap();
        assert_eq!((result.recovered, result.skipped, result.failed), (0, 0, 1));
        assert!(part.exists() && ready.exists());
        assert!(!dir.join(format!("clip-{id}.mp4")).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
