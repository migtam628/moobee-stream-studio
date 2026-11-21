//! First-party, bounded, video-only RIFF AVI / Motion JPEG muxer.
//! Frames stream directly to a Write + Seek target: O(1) JPEG-frame memory,
//! O(frame count) compact index. No FFmpeg process or OBS dependency.
use std::io::{Cursor, Seek, SeekFrom, Write};

const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VIDEO_DIMENSION: u32 = 2048;
const MAX_FRAMES: usize = 120;

fn u16le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn io_error(error: std::io::Error) -> String {
    format!("AVI file I/O failed: {error}")
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
fn validate_settings(width: u32, height: u32, fps: u32) -> Result<(), String> {
    if width == 0 || height == 0 ||
       width > MAX_VIDEO_DIMENSION || height > MAX_VIDEO_DIMENSION ||
       !(1..=30).contains(&fps) {
        return Err("Unsupported AVI dimensions or frame rate".to_owned());
    }
    Ok(())
}
fn make_header(
    width: u32,
    height: u32,
    fps: u32,
    frame_count: u32,
    max_frame_bytes: u32,
) -> Result<Vec<u8>, String> {
    // AVI Main Header (AVIMAINHEADER, 56 bytes).
    let mut avih = Vec::with_capacity(56);
    for value in [
        1_000_000 / fps,
        max_frame_bytes.saturating_mul(fps),
        0,
        0x10, // AVIF_HASINDEX
        frame_count,
        0,
        1,
        max_frame_bytes,
        width,
        height,
        0, 0, 0, 0,
    ] {
        u32le(&mut avih, value);
    }
    debug_assert_eq!(avih.len(), 56);

    // AVI Stream Header (AVISTREAMHEADER, 56 bytes).
    let mut strh = b"vidsMJPG".to_vec();
    u32le(&mut strh, 0);
    u16le(&mut strh, 0);
    u16le(&mut strh, 0);
    for value in [0, 1, fps, 0, frame_count, max_frame_bytes, u32::MAX, 0] {
        u32le(&mut strh, value);
    }
    for value in [0, 0, width as u16, height as u16] {
        u16le(&mut strh, value);
    }
    debug_assert_eq!(strh.len(), 56);

    // BITMAPINFOHEADER (40 bytes), MJPEG compressed frames.
    let mut strf = Vec::with_capacity(40);
    u32le(&mut strf, 40);
    u32le(&mut strf, width);
    u32le(&mut strf, height);
    u16le(&mut strf, 1);
    u16le(&mut strf, 24);
    strf.extend_from_slice(b"MJPG");
    u32le(&mut strf, max_frame_bytes);
    for _ in 0..4 {
        u32le(&mut strf, 0);
    }
    debug_assert_eq!(strf.len(), 40);

    let mut stream = chunk(b"strh", strh)?;
    stream.extend(chunk(b"strf", strf)?);
    let mut hdrl = chunk(b"avih", avih)?;
    hdrl.extend(list(b"strl", stream)?);
    list(b"hdrl", hdrl)
}

/// Incremental file-safe AVI writer. The caller owns the temporary file and is
/// responsible for sync_all + atomic rename only after finish() succeeds.
/// An unfinished temporary file must never be presented as a complete recording.
pub struct AviWriter<W: Write + Seek> {
    output: W,
    width: u32,
    height: u32,
    fps: u32,
    header_start: u64,
    header_len: usize,
    movi_start: u64,
    frames: Vec<(u32, u32)>,
    largest_frame: u32,
    finished: bool,
}

impl<W: Write + Seek> AviWriter<W> {
    pub fn new(mut output: W, width: u32, height: u32, fps: u32) -> Result<Self, String> {
        validate_settings(width, height, fps)?;
        let initial_header = make_header(width, height, fps, 0, 0)?;
        output.write_all(b"RIFF\0\0\0\0AVI ").map_err(io_error)?;
        let header_start = output.stream_position().map_err(io_error)?;
        output.write_all(&initial_header).map_err(io_error)?;
        let movi_start = output.stream_position().map_err(io_error)?;
        output.write_all(b"LIST\0\0\0\0movi").map_err(io_error)?;

        Ok(Self {
            output, width, height, fps, header_start,
            header_len: initial_header.len(),
            movi_start, frames: Vec::new(),
            largest_frame: 0, finished: false,
        })
    }

    pub fn write_frame(&mut self, jpeg: &[u8]) -> Result<(), String> {
        if self.finished {
            return Err("Cannot append to a finalized AVI".into());
        }
        if self.frames.len() >= MAX_FRAMES {
            return Err("AVI test recording exceeds the frame-count limit".into());
        }
        if jpeg.len() < 4 || !jpeg.starts_with(&[0xff, 0xd8]) ||
           !jpeg.ends_with(&[0xff, 0xd9]) {
            return Err("AVI frame is not a complete JPEG image".into());
        }
        let length = u32::try_from(jpeg.len()).map_err(|_| "JPEG exceeds AVI size limit")?;
        let position = self.output.stream_position().map_err(io_error)?;
        let next_frame = position
            .checked_add(8 + u64::from(length) + u64::from(length % 2))
            .ok_or("AVI output offset overflow")?;
        let total_with_index = next_frame
            .checked_add(8 + ((self.frames.len() as u64 + 1) * 16))
            .ok_or("AVI index size overflow")?;
        if total_with_index > MAX_OUTPUT_BYTES {
            return Err("AVI exceeds the 64 MiB file safety limit".into());
        }

        let offset = position.checked_sub(self.movi_start + 8)
            .ok_or("AVI frame index underflow")?;
        let offset = u32::try_from(offset).map_err(|_| "AVI frame index overflow")?;
        self.output.write_all(b"00dc").map_err(io_error)?;
        self.output.write_all(&length.to_le_bytes()).map_err(io_error)?;
        self.output.write_all(jpeg).map_err(io_error)?;
        if length % 2 != 0 {
            self.output.write_all(&[0]).map_err(io_error)?;
        }
        self.frames.push((offset, length));
        self.largest_frame = self.largest_frame.max(length);
        Ok(())
    }

    /// Patch RIFF/LIST/header lengths and append idx1 for seekable playback.
    /// Returns the complete byte size. No caller should publish before sync_all.
    pub fn finish(&mut self) -> Result<u64, String> {
        if self.finished {
            return Err("AVI file was already finalized".into());
        }
        if self.frames.is_empty() {
            return Err("Cannot finish an AVI without video frames".into());
        }
        let movi_end = self.output.stream_position().map_err(io_error)?;
        let movi_size = movi_end.checked_sub(self.movi_start + 8)
            .ok_or("AVI movi size underflow")?;
        let movi_size = u32::try_from(movi_size).map_err(|_| "AVI movi list too large")?;
        let index_len = u32::try_from(self.frames.len() * 16)
            .map_err(|_| "AVI index too large")?;

        self.output.write_all(b"idx1").map_err(io_error)?;
        self.output.write_all(&index_len.to_le_bytes()).map_err(io_error)?;
        for (offset, length) in &self.frames {
            self.output.write_all(b"00dc").map_err(io_error)?;
            self.output.write_all(&0x10_u32.to_le_bytes()).map_err(io_error)?;
            self.output.write_all(&offset.to_le_bytes()).map_err(io_error)?;
            self.output.write_all(&length.to_le_bytes()).map_err(io_error)?;
        }
        let total = self.output.stream_position().map_err(io_error)?;
        if total > MAX_OUTPUT_BYTES {
            return Err("Final AVI exceeds the 64 MiB safety limit".into());
        }
        let riff_size = u32::try_from(total - 8).map_err(|_| "AVI RIFF size overflow")?;
        let header = make_header(
            self.width, self.height, self.fps, self.frames.len() as u32, self.largest_frame
        )?;
        if header.len() != self.header_len {
            return Err("AVI header size changed during finalization".into());
        }

        self.output.seek(SeekFrom::Start(4)).map_err(io_error)?;
        self.output.write_all(&riff_size.to_le_bytes()).map_err(io_error)?;
        self.output.seek(SeekFrom::Start(self.movi_start + 4)).map_err(io_error)?;
        self.output.write_all(&movi_size.to_le_bytes()).map_err(io_error)?;
        self.output.seek(SeekFrom::Start(self.header_start)).map_err(io_error)?;
        self.output.write_all(&header).map_err(io_error)?;
        self.output.seek(SeekFrom::Start(total)).map_err(io_error)?;
        self.output.flush().map_err(io_error)?;
        self.finished = true;
        Ok(total)
    }
}

/// Compatibility wrapper used by tests/tools. Native recording should use
/// AviWriter<File> directly to avoid buffering an entire video in RAM.
pub fn encode_silent_mjpeg_avi(
    frames: &[Vec<u8>], width: u32, height: u32, fps: u32,
) -> Result<Vec<u8>, String> {
    let mut output = Cursor::new(Vec::new());
    {
        let mut writer = AviWriter::new(&mut output, width, height, fps)?;
        for jpeg in frames {
            writer.write_frame(jpeg)?;
        }
        writer.finish()?;
    }
    Ok(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{encode_silent_mjpeg_avi, AviWriter};
    use std::io::{Cursor, Seek, SeekFrom};

    fn sample_jpeg() -> Vec<u8> {
        vec![0xff, 0xd8, 0x01, 0x02, 0xff, 0xd9]
    }

    #[test]
    fn writes_seekable_indexed_video_only_avi() {
        let bytes = encode_silent_mjpeg_avi(&vec![sample_jpeg(); 40], 640, 360, 10).unwrap();
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
    fn streaming_writer_matches_memory_wrapper() {
        let jpeg = sample_jpeg();
        let frames = vec![jpeg.clone(); 20];
        let expected = encode_silent_mjpeg_avi(&frames, 640, 360, 10).unwrap();
        let mut output = Cursor::new(Vec::new());
        let len = {
            let mut writer = AviWriter::new(&mut output, 640, 360, 10).unwrap();
            for frame in &frames {
                writer.write_frame(frame).unwrap();
            }
            assert!(writer.finish().unwrap() > 0);
            assert!(writer.finish().is_err());
            output.seek(SeekFrom::End(0)).unwrap()
        };
        assert_eq!(len as usize, expected.len());
        assert_eq!(output.into_inner(), expected);
    }

    #[test]
    fn invalid_frame_cannot_be_published() {
        let mut output = Cursor::new(Vec::new());
        let mut writer = AviWriter::new(&mut output, 640, 360, 10).unwrap();
        assert!(writer.finish().is_err());
        assert!(writer.write_frame(&[0, 1, 2]).is_err());
        writer.write_frame(&sample_jpeg()).unwrap();
        assert!(writer.finish().is_ok());
        assert!(writer.write_frame(&sample_jpeg()).is_err());
    }

    #[test]
    fn rejects_invalid_frames_or_timing() {
        assert!(encode_silent_mjpeg_avi(&[], 640, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&[sample_jpeg()], 0, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&[sample_jpeg()], 640, 360, 0).is_err());
        assert!(encode_silent_mjpeg_avi(&[vec![0, 1]], 640, 360, 10).is_err());
        assert!(encode_silent_mjpeg_avi(&vec![sample_jpeg(); 121], 640, 360, 10).is_err());
    }
}
