//! Pure Rust RIFF AVI (Motion JPEG) writer for bounded, silent local video samples.
//! No external executable, OBS API, proprietary format SDK, or network connection.
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_VIDEO_DIMENSION: u32 = 2048;

fn u16le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn chunk(id: &[u8; 4], payload: Vec<u8>) -> Result<Vec<u8>, String> {
    let size = u32::try_from(payload.len()).map_err(|_| "AVI chunk exceeds 4 GiB")?;
    let mut out = Vec::with_capacity(payload.len() + 9);
    out.extend_from_slice(id);
    u32le(&mut out, size);
    out.extend_from_slice(&payload);
    if size % 2 != 0 {
        out.push(0);
    }
    Ok(out)
}
fn list(id: &[u8; 4], body: Vec<u8>) -> Result<Vec<u8>, String> {
    let mut payload = Vec::with_capacity(body.len() + 4);
    payload.extend_from_slice(id);
    payload.extend_from_slice(&body);
    chunk(b"LIST", payload)
}

/// Wrap consecutive JPEG images into a seekable video-only AVI file.
/// Fixed-frame-rate playback; callers must duplicate a previous frame to fill timing gaps.
/// Captured frames must all be scaled/padded to the provided width and height before JPEG encoding.
pub fn encode_silent_mjpeg_avi(
    frames: &[Vec<u8>],
    width: u32,
    height: u32,
    fps: u32,
) -> Result<Vec<u8>, String> {
    if frames.is_empty() || frames.len() > 120 {
        return Err("AVI capture requires 1–120 JPEG frames".into());
    }
    if width == 0 || height == 0 || width > MAX_VIDEO_DIMENSION || height > MAX_VIDEO_DIMENSION ||
       width > i16::MAX as u32 || height > i16::MAX as u32 || !(1..=30).contains(&fps) {
        return Err("Unsupported AVI frame size or frame rate".into());
    }
    let mut bytes = 0usize;
    for jpeg in frames {
        if jpeg.len() < 4 || !jpeg.starts_with(&[0xff, 0xd8]) ||
           !jpeg.ends_with(&[0xff, 0xd9]) {
            return Err("AVI frame is not a complete JPEG image".into());
        }
        bytes = bytes.checked_add(jpeg.len()).ok_or("AVI exceeds size limit")?;
        if bytes > MAX_OUTPUT_BYTES - 64 * 1024 {
            return Err("AVI is larger than the 64 MiB safety limit".into());
        }
    }

    let max_size = u32::try_from(frames.iter().map(Vec::len).max().unwrap_or(0))
        .map_err(|_| "JPEG frame too large")?;
    let count = u32::try_from(frames.len()).map_err(|_| "Too many frames")?;

    // AVIMAINHEADER is exactly 56 bytes.
    let mut avih = Vec::new();
    for value in [
        1_000_000 / fps, max_size.saturating_mul(fps), 0, 0x10,
        count, 0, 1, max_size, width, height, 0, 0, 0, 0
    ] {
        u32le(&mut avih, value);
    }
    debug_assert_eq!(avih.len(), 56);

    // AVISTREAMHEADER is exactly 56 bytes: "vids" + "MJPG".
    let mut strh = b"vidsMJPG".to_vec();
    u32le(&mut strh, 0); // dwFlags
    u16le(&mut strh, 0); // wPriority
    u16le(&mut strh, 0); // wLanguage
    for value in [0, 1, fps, 0, count, max_size, u32::MAX, 0] {
        u32le(&mut strh, value);
    }
    for value in [0, 0, width as u16, height as u16] {
        u16le(&mut strh, value);
    }
    debug_assert_eq!(strh.len(), 56);

    // BITMAPINFOHEADER is exactly 40 bytes.
    let mut strf = Vec::new();
    u32le(&mut strf, 40);
    u32le(&mut strf, width);
    u32le(&mut strf, height);
    u16le(&mut strf, 1);
    u16le(&mut strf, 24);
    strf.extend_from_slice(b"MJPG");
    u32le(&mut strf, max_size);
    for _ in 0..4 {
        u32le(&mut strf, 0);
    }
    debug_assert_eq!(strf.len(), 40);

    let mut strl_body = chunk(b"strh", strh)?;
    strl_body.extend(chunk(b"strf", strf)?);
    let stream_list = list(b"strl", strl_body)?;
    let mut hdrl_body = chunk(b"avih", avih)?;
    hdrl_body.extend(stream_list);
    let header_list = list(b"hdrl", hdrl_body)?;

    let mut movi_body = b"movi".to_vec();
    let mut index = Vec::with_capacity(frames.len() * 16);
    for jpeg in frames {
        let start = u32::try_from(movi_body.len()).map_err(|_| "AVI index offset overflow")?;
        let length = u32::try_from(jpeg.len()).map_err(|_| "AVI frame length overflow")?;
        index.extend_from_slice(b"00dc");
        u32le(&mut index, 0x10); // Every MJPEG frame is independently decodable.
        u32le(&mut index, start); // Relative to movi list data, beginning with 'movi' tag.
        u32le(&mut index, length);
        movi_body.extend(chunk(b"00dc", jpeg.clone())?);
    }
    let movi_list = chunk(b"LIST", movi_body)?;
    let index_chunk = chunk(b"idx1", index)?;

    let mut payload = b"AVI ".to_vec();
    payload.extend(header_list);
    payload.extend(movi_list);
    payload.extend(index_chunk);
    if payload.len() + 8 > MAX_OUTPUT_BYTES {
        return Err("Final AVI exceeds the 64 MiB safety limit".into());
    }
    chunk(b"RIFF", payload)
}

#[cfg(test)]
mod tests {
    use super::encode_silent_mjpeg_avi;

    #[test]
    fn writes_seekable_indexed_video_only_avi() {
        let jpeg = vec![0xff, 0xd8, 0x01, 0x02, 0xff, 0xd9];
        let bytes = encode_silent_mjpeg_avi(&vec![jpeg; 40], 640, 360, 10).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"AVI ");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize, bytes.len() - 8);
        assert!(bytes.windows(4).any(|w| w == b"avih"));
        assert!(bytes.windows(4).any(|w| w == b"vids"));
        assert!(bytes.windows(4).any(|w| w == b"MJPG"));
        assert!(bytes.windows(4).any(|w| w == b"movi"));
        assert!(bytes.windows(4).any(|w| w == b"idx1"));
        assert_eq!(bytes.windows(4).filter(|w| *w == b"00dc").count(), 80);
    }

    #[test]
    fn rejects_invalid_frames_or_timing() {
        let jpeg = vec![0xff, 0xd8, 0xff, 0xd9];
        assert!(encode_silent_mjpeg_avi(&[], 640, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&[jpeg.clone()], 0, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&[jpeg.clone()], 640, 360, 0).is_err());
        assert!(encode_silent_mjpeg_avi(&[vec![0x00, 0x01]], 640, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&vec![jpeg; 121], 640, 360, 10).is_err());
    }
}
