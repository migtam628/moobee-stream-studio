//! Small dependency-free ISO BMFF structural guard for finalized test MP4s.
//! This does NOT prove codec support, decodability, video duration, or audio sync.
//! Hardware acceptance and independent media probing remain mandatory.

use std::io::{Read, Seek, SeekFrom};

/// Reject truncated/incomplete top-level MP4 boxes before publishing a clip.
/// Scan box headers only; do not buffer media payloads into application memory.
pub(crate) fn validate_mp4_boxes<R: Read + Seek>(
    reader: &mut R,
    length: u64,
) -> Result<(), String> {
    if length < 16 {
        return Err("MP4 container is too short".into());
    }

    let mut offset = 0u64;
    let mut seen_ftyp = false;
    let mut seen_moov = false;
    let mut seen_mdat = false;

    while offset < length {
        let remaining = length - offset;
        if remaining < 8 {
            return Err("MP4 has a truncated trailing box header".into());
        }
        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|error| format!("Could not seek MP4 box: {error}"))?;
        let mut header = [0u8; 8];
        reader
            .read_exact(&mut header)
            .map_err(|error| format!("Could not read MP4 box header: {error}"))?;
        let short_size = u32::from_be_bytes(header[0..4].try_into().unwrap());
        let kind = &header[4..8];
        let (box_size, header_size) = if short_size == 1 {
            if remaining < 16 {
                return Err("MP4 has a truncated extended box header".into());
            }
            let mut extended = [0u8; 8];
            reader
                .read_exact(&mut extended)
                .map_err(|error| format!("Could not read extended MP4 box size: {error}"))?;
            (u64::from_be_bytes(extended), 16u64)
        } else if short_size == 0 {
            // ISO BMFF: size 0 is permitted only for a box extending to EOF.
            (remaining, 8u64)
        } else {
            (u64::from(short_size), 8u64)
        };
        if box_size < header_size || box_size > remaining {
            return Err("MP4 box size exceeds available bytes or is invalid".into());
        }

        if offset == 0 {
            if kind != b"ftyp" || box_size < 16 {
                return Err("MP4 must begin with a valid ftyp box".into());
            }
            seen_ftyp = true;
        }
        if kind == b"moov" && box_size > header_size {
            seen_moov = true;
        }
        if kind == b"mdat" && box_size > header_size {
            seen_mdat = true;
        }
        offset += box_size;
    }

    if !seen_ftyp || !seen_moov || !seen_mdat {
        return Err("MP4 must contain ftyp, moov and non-empty mdat boxes".into());
    }
    Ok(())
}


/// Read the real container movie duration from the finalized movie header
/// (moov/mvhd). A requested capture maximum is not the duration of an early
/// Stop-and-Save. No FFmpeg dependency or large media-buffer allocation.
/// Return None when a structurally valid MP4 lacks usable movie timing.
pub(crate) fn movie_duration_seconds<R: Read + Seek>(
    reader: &mut R,
    length: u64,
) -> Result<Option<f64>, String> {
    fn read_box<R: Read + Seek>(
        reader: &mut R, pos: u64, end: u64,
    ) -> Result<(u64, u64, [u8; 4]), String> {
        if end.saturating_sub(pos) < 8 {
            return Err("Truncated ISO BMFF box header".into());
        }
        reader.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
        let mut header = [0u8; 8];
        reader.read_exact(&mut header).map_err(|e| e.to_string())?;
        let declared = u32::from_be_bytes(header[0..4].try_into().unwrap());
        let (size, header_size) = if declared == 1 {
            if end - pos < 16 { return Err("Truncated extended ISO box".into()); }
            let mut extended = [0; 8];
            reader.read_exact(&mut extended).map_err(|e| e.to_string())?;
            (u64::from_be_bytes(extended), 16)
        } else if declared == 0 { (end - pos, 8) }
        else { (u64::from(declared), 8) };
        if size < header_size || size > end - pos {
            return Err("Invalid or overlong ISO BMFF box".into());
        }
        Ok((size, header_size, header[4..8].try_into().unwrap()))
    }
    let mut cursor = 0u64;
    while cursor < length {
        let (box_len, header_len, kind) = read_box(reader, cursor, length)?;
        if kind == *b"moov" {
            let mut child = cursor + header_len;
            let box_end = cursor + box_len;
            while child < box_end {
                let (size, h, child_kind) = read_box(reader, child, box_end)?;
                if child_kind == *b"mvhd" {
                    let payload_len = size - h;
                    if payload_len < 20 { return Ok(None); }
                    reader.seek(SeekFrom::Start(child + h)).map_err(|e| e.to_string())?;
                    let mut version = [0u8; 4];
                    reader.read_exact(&mut version).map_err(|e| e.to_string())?;
                    let (timescale, duration) = match version[0] {
                        0 if payload_len >= 20 => {
                            let mut fields = [0u8; 16];
                            reader.read_exact(&mut fields).map_err(|e| e.to_string())?;
                            (u32::from_be_bytes(fields[8..12].try_into().unwrap()),
                             u32::from_be_bytes(fields[12..16].try_into().unwrap()) as u64)
                        }
                        1 if payload_len >= 32 => {
                            let mut fields = [0u8; 28];
                            reader.read_exact(&mut fields).map_err(|e| e.to_string())?;
                            (u32::from_be_bytes(fields[16..20].try_into().unwrap()),
                             u64::from_be_bytes(fields[20..28].try_into().unwrap()))
                        }
                        _ => return Ok(None),
                    };
                    if timescale == 0 || duration == 0 || duration == u64::MAX ||
                       duration == u32::MAX as u64 {
                        return Ok(None);
                    }
                    let seconds = duration as f64 / f64::from(timescale);
                    return Ok((0.01..=3600.0).contains(&seconds).then_some(seconds));
                }
                child += size;
            }
        }
        cursor += box_len;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{validate_mp4_boxes, movie_duration_seconds};
    use std::io::Cursor;

    fn atom(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let size = u32::try_from(data.len() + 8).unwrap();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(data);
        bytes
    }

    fn sample(include_moov: bool) -> Vec<u8> {
        let mut bytes = atom(b"ftyp", b"isom\0\0\0\0isom");
        if include_moov {
            bytes.extend(atom(b"moov", &[1u8; 8]));
        }
        bytes.extend(atom(b"mdat", &[2u8; 4096]));
        bytes
    }

    fn check(bytes: &[u8]) -> Result<(), String> {
        validate_mp4_boxes(&mut Cursor::new(bytes), bytes.len() as u64)
    }

    #[test]
    fn accepts_structurally_complete_container() {
        assert!(check(&sample(true)).is_ok());
    }

    #[test]
    fn rejects_missing_moov() {
        assert!(check(&sample(false)).is_err());
    }

    #[test]
    fn rejects_truncated_media_payload() {
        let mut bytes = sample(true);
        bytes.pop();
        assert!(check(&bytes).is_err());
    }

    #[test]
    fn rejects_false_ftyp_and_trailing_garbage() {
        let mut bytes = sample(true);
        bytes[4..8].copy_from_slice(b"free");
        assert!(check(&bytes).is_err());

        let mut bytes = sample(true);
        bytes.extend_from_slice(&[0, 0, 0]);
        assert!(check(&bytes).is_err());
    }

    #[test]
    fn rejects_overstated_or_too_small_box_sizes() {
        let mut bytes = sample(true);
        let mdat_offset = bytes.windows(4).position(|part| part == b"mdat").unwrap() - 4;
        bytes[mdat_offset..mdat_offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(check(&bytes).is_err());

        let mut bytes = sample(true);
        bytes[0..4].copy_from_slice(&4u32.to_be_bytes());
        assert!(check(&bytes).is_err());
    }

    #[test]
    fn accepts_eof_sized_mdat_and_extended_box_size() {
        let mut bytes = sample(true);
        let mdat_offset = bytes.windows(4).position(|part| part == b"mdat").unwrap() - 4;
        bytes[mdat_offset..mdat_offset + 4].copy_from_slice(&0u32.to_be_bytes());
        assert!(check(&bytes).is_ok());

        let mut bytes = sample(true);
        let moov_offset = bytes.windows(4).position(|part| part == b"moov").unwrap() - 4;
        bytes[moov_offset..moov_offset + 4].copy_from_slice(&1u32.to_be_bytes());
        bytes.splice(moov_offset + 8..moov_offset + 8, 24u64.to_be_bytes());
        assert!(check(&bytes).is_ok());
    }
    #[test]
    fn movie_duration_uses_actual_mvhd_v0_and_v1_not_requested_limit() {
        let mut v0 = vec![0u8; 20];
        v0[12..16].copy_from_slice(&600u32.to_be_bytes());
        v0[16..20].copy_from_slice(&9_600u32.to_be_bytes()); // 16 sec
        let mut data = atom(b"ftyp", b"isom\\0\\0\\0\\0isom");
        data.extend(atom(b"moov", &atom(b"mvhd", &v0)));
        data.extend(atom(b"mdat", &[3u8; 32]));
        let measured = movie_duration_seconds(&mut Cursor::new(&data), data.len() as u64).unwrap();
        assert_eq!(measured, Some(16.0));

        let mut v1 = vec![0u8; 32];
        v1[0] = 1;
        v1[20..24].copy_from_slice(&1000u32.to_be_bytes());
        v1[24..32].copy_from_slice(&15_250u64.to_be_bytes());
        let mut data = atom(b"ftyp", b"isom\\0\\0\\0\\0isom");
        data.extend(atom(b"moov", &atom(b"mvhd", &v1)));
        data.extend(atom(b"mdat", &[3u8; 32]));
        assert_eq!(movie_duration_seconds(&mut Cursor::new(&data), data.len() as u64).unwrap(), Some(15.25));
    }

    #[test]
    fn movie_duration_rejects_unavailable_or_invalid_timing_without_guessing() {
        let data = sample(true);
        assert_eq!(movie_duration_seconds(&mut Cursor::new(&data), data.len() as u64).unwrap(), None);
        let mut v0 = vec![0u8; 20];
        v0[16..20].copy_from_slice(&900u32.to_be_bytes()); // timescale is zero
        let mut data = atom(b"ftyp", b"isom\\0\\0\\0\\0isom");
        data.extend(atom(b"moov", &atom(b"mvhd", &v0)));
        data.extend(atom(b"mdat", &[3u8; 32]));
        assert_eq!(movie_duration_seconds(&mut Cursor::new(&data), data.len() as u64).unwrap(), None);
    }

}
