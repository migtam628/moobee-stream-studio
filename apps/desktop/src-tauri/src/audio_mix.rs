//! First-party PCM mixing primitive, independent of capture/encoding devices.
//! Inputs must already be aligned and resampled to the same stereo timeline.
//! This module is not a connected microphone or system-audio capture engine.
pub(crate) struct Input<'a> {
    pub samples: &'a [f32],
    pub gain: f32,
    pub muted: bool,
}
#[derive(Debug)]
pub(crate) struct MixResult { pub samples: Vec<f32>, pub peak: f32, pub clipped: usize }
pub(crate) fn mix_stereo(frames: usize, inputs: &[Input<'_>]) -> Result<MixResult, String> {
    if frames == 0 || frames > 48_000 || inputs.len() > 16 {
        return Err("Audio mixer block outside safety limits".into());
    }
    let size = frames * 2;
    for input in inputs {
        if input.samples.len() != size || !input.gain.is_finite() || !(0.0..=2.0).contains(&input.gain)
            || input.samples.iter().any(|x| !x.is_finite()) {
            return Err("Invalid or unaligned PCM input".into());
        }
    }
    let mut samples = vec![0f32; size];
    for input in inputs.iter().filter(|input| !input.muted) {
        for (out, sample) in samples.iter_mut().zip(input.samples) {
            *out += sample.clamp(-1.0,1.0) * input.gain;
        }
    }
    let mut peak = 0f32;
    let mut clipped = 0usize;
    for sample in &mut samples {
        if sample.abs() > 1.0 { clipped += 1; }
        *sample = sample.clamp(-1.0,1.0);
        peak = peak.max(sample.abs());
    }
    Ok(MixResult { samples, peak, clipped })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixes_gain_mute_and_clipping_without_nan() {
        let a = [0.4,-0.4,0.8,-0.8];
        let b = [0.4,0.4,0.8,-0.8];
        let inputs = [Input { samples: &a, gain: 1.0, muted: false }, Input { samples: &b, gain: 1.0, muted: false }];
        let result = mix_stereo(2, &inputs).unwrap();
        assert_eq!(result.samples, vec![0.8,0.0,1.0,-1.0]);
        assert_eq!(result.peak, 1.0);
        assert_eq!(result.clipped, 2);
        assert_eq!(mix_stereo(2, &[Input { samples: &a, gain: 2.0, muted: true }]).unwrap().peak, 0.0);
        assert!(mix_stereo(1, &[Input { samples: &[f32::NAN,0.0], gain: 1.0, muted: false }]).is_err());
        assert!(mix_stereo(1, &[Input { samples: &a, gain: 1.0, muted: false }]).is_err());
    }
}
