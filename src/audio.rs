//! Extract a video's audio track as a 16 kHz mono 16-bit WAV, in pure Rust (no ffmpeg).
//! Decoding uses symphonia; resampling is a streaming windowed-sinc low-pass resampler.

use std::{f64::consts::PI, fs::File, path::Path};

use symphonia::core::{
    codecs::{audio::AudioDecoderOptions, CodecParameters},
    errors::Error as SymError,
    formats::{probe::Hint, FormatOptions, SeekMode, SeekTo, TrackType},
    io::MediaSourceStream,
    meta::MetadataOptions,
    units::{Time, Timestamp},
};

pub const OUT_RATE: u32 = 16_000;
/// Refuse absurdly long input (4 hours of 16 kHz mono 16-bit is ~460 MB).
const MAX_OUT_SAMPLES: usize = OUT_RATE as usize * 3600 * 4;

/// Streaming sinc resampler. Positions are measured in input samples.
struct Resampler {
    step: f64,
    half: f64,
    cutoff: f64,
    pos: f64,
    base: usize,
    total_in: usize,
    buf: Vec<f32>,
}

impl Resampler {
    fn new(in_rate: u32, out_rate: u32) -> Self {
        let step = f64::from(in_rate) / f64::from(out_rate);
        // Cutoff (cycles per input sample) at 95% of the output Nyquist frequency.
        let cutoff = (0.5 / step.max(1.0)) * 0.95;
        Self {
            step,
            half: 8.0 / cutoff,
            cutoff,
            pos: 0.0,
            base: 0,
            total_in: 0,
            buf: Vec::new(),
        }
    }

    fn kernel(&self, x: f64) -> f64 {
        let t = x / self.half;
        if t.abs() >= 1.0 {
            return 0.0;
        }
        let window = 0.42 + 0.5 * (PI * t).cos() + 0.08 * (2.0 * PI * t).cos(); // Blackman
        let y = 2.0 * self.cutoff * x;
        let sinc = if y.abs() < 1e-9 {
            1.0
        } else {
            (PI * y).sin() / (PI * y)
        };
        2.0 * self.cutoff * sinc * window
    }

    fn emit(&mut self, out: &mut Vec<i16>, limit: Option<f64>) {
        loop {
            let need = (self.pos + self.half).ceil() as usize;
            let have = self.base + self.buf.len();
            let ready = match limit {
                Some(end) => self.pos < end,
                None => need < have,
            };
            if !ready || (limit.is_none() && need >= have) {
                break;
            }
            let lo = ((self.pos - self.half).floor().max(0.0)) as usize;
            let hi = (((self.pos + self.half).ceil()) as usize)
                .min(self.base + self.buf.len().saturating_sub(1));
            let mut acc = 0.0f64;
            for n in lo..=hi {
                if n >= self.base {
                    acc += f64::from(self.buf[n - self.base]) * self.kernel(n as f64 - self.pos);
                }
            }
            out.push((acc.clamp(-1.0, 1.0) * 32767.0).round() as i16);
            self.pos += self.step;
        }
        let keep_from = ((self.pos - self.half).floor().max(0.0)) as usize;
        if keep_from > self.base {
            let drop = (keep_from - self.base).min(self.buf.len());
            self.buf.drain(..drop);
            self.base += drop;
        }
    }

    fn feed(&mut self, samples: &[f32], out: &mut Vec<i16>) {
        self.total_in += samples.len();
        self.buf.extend_from_slice(samples);
        self.emit(out, None);
    }

    fn finish(&mut self, out: &mut Vec<i16>) {
        let end = self.total_in as f64;
        self.buf
            .extend(std::iter::repeat_n(0.0, self.half.ceil() as usize + 2));
        self.emit(out, Some(end));
    }
}

fn wav_bytes(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + samples.len() * 2);
    v.extend(b"RIFF");
    v.extend((36 + data_len).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(16u32.to_le_bytes());
    v.extend(1u16.to_le_bytes()); // PCM
    v.extend(1u16.to_le_bytes()); // mono
    v.extend(OUT_RATE.to_le_bytes());
    v.extend((OUT_RATE * 2).to_le_bytes());
    v.extend(2u16.to_le_bytes());
    v.extend(16u16.to_le_bytes());
    v.extend(b"data");
    v.extend(data_len.to_le_bytes());
    v.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    v
}

/// Seconds of audio decoded before the requested start, so codec warm-up is not heard.
const PREROLL_S: f64 = 1.0;

/// Extract mono 16 kHz WAV audio. With `range`, only that span (in source-file seconds)
/// is returned; the file is seeked, not decoded from the start.
pub fn extract_wav(path: &Path, range: Option<(f64, f64)>) -> Result<Vec<u8>, String> {
    match catching(|| decode(path, range, true)) {
        Err(e) if e == SEEK_FAILED => catching(|| decode(path, range, false)),
        other => other,
    }
}

/// Media files come from outside, and a decoder can have bugs. If it panics, report an error
/// instead of taking the whole app down.
fn catching<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err("This file could not be read. It may be damaged.".into()))
}

const SEEK_FAILED: &str = "seek failed";

fn decode(path: &Path, range: Option<(f64, f64)>, allow_seek: bool) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|_| "Could not open the video file.")?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| "This video format is not supported.")?;
    let track = format
        .first_track(TrackType::Audio)
        .ok_or("This video has no audio track.")?;
    let (track_id, time_base) = (track.id, track.time_base);
    let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
        return Err("This video has no audio track.".into());
    };
    let in_rate = params.sample_rate.ok_or("This video has no audio track.")?;
    // Keep every decoded frame (no gapless trimming), as the 0.5 decoders did by default.
    #[allow(clippy::field_reassign_with_default)]
    let opts = {
        let mut o = AudioDecoderOptions::default();
        o.gapless = false;
        o
    };
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &opts)
        .map_err(|_| "This audio format is not supported.")?;

    let (r_start, r_end) = range.unwrap_or((0.0, f64::INFINITY));
    if allow_seek && r_start > PREROLL_S + 0.5 {
        let target = Time::try_from_secs_f64(r_start - PREROLL_S).ok_or(SEEK_FAILED)?;
        let seek = SeekTo::Time {
            time: target,
            track_id: Some(track_id),
        };
        if format.seek(SeekMode::Accurate, seek).is_err() {
            return Err(SEEK_FAILED.into());
        }
        decoder.reset();
    }
    let ts_to_secs = |ts: Timestamp| match time_base.and_then(|tb| tb.calc_time(ts)) {
        Some(t) => t.as_secs_f64(),
        None => ts.get() as f64 / f64::from(in_rate),
    };

    let mut rs = (in_rate != OUT_RATE).then(|| Resampler::new(in_rate, OUT_RATE));
    let mut out: Vec<i16> = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut first_time: Option<f64> = None;
    let mut frames_seen = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(_) => return Err("The audio could not be read to the end.".into()),
        };
        if packet.track_id != track_id {
            continue;
        }
        let t0 = *first_time.get_or_insert_with(|| ts_to_secs(packet.pts));
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // skip a damaged packet
            Err(_) => return Err("The audio could not be decoded.".into()),
        };
        let channels = decoded.spec().channels().count().max(1);
        interleaved.clear();
        decoded.copy_to_vec_interleaved::<f32>(&mut interleaved);
        mono.clear();
        mono.extend(
            interleaved
                .chunks(channels)
                .map(|f| f.iter().sum::<f32>() / channels as f32),
        );

        // Keep only the frames inside [r_start, r_end).
        let chunk_start = frames_seen;
        frames_seen += mono.len();
        let to_idx = |t: f64| ((t - t0) * f64::from(in_rate)).round().max(0.0) as usize;
        let lo = to_idx(r_start);
        let hi = if r_end.is_finite() {
            to_idx(r_end)
        } else {
            usize::MAX
        };
        if chunk_start >= hi {
            break;
        }
        let a = lo.saturating_sub(chunk_start).min(mono.len());
        let b = hi.saturating_sub(chunk_start).min(mono.len());
        let slice = &mono[a..b];
        match rs.as_mut() {
            Some(r) => r.feed(slice, &mut out),
            None => out.extend(
                slice
                    .iter()
                    .map(|s| (s.clamp(-1.0, 1.0) * 32767.0).round() as i16),
            ),
        }
        if out.len() > MAX_OUT_SAMPLES {
            return Err("This video is too long (over 4 hours).".into());
        }
    }
    if let Some(r) = rs.as_mut() {
        r.finish(&mut out);
    }
    if out.is_empty() {
        return Err("No audio was found in this video.".into());
    }
    Ok(wav_bytes(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path of a short video with speech, set by the developer to run the real-file tests:
    ///   set SCS_TEST_VIDEO=C:\\path\\to\\clip.mov
    /// Without it those tests simply skip.
    fn test_video() -> Option<std::path::PathBuf> {
        std::env::var_os("SCS_TEST_VIDEO")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
    }

    fn tone(rate: u32, hz: f64, secs: f64) -> Vec<f32> {
        (0..(f64::from(rate) * secs) as usize)
            .map(|i| (0.5 * (2.0 * PI * hz * i as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }
    fn rms(v: &[i16]) -> f64 {
        (v.iter()
            .map(|s| (f64::from(*s) / 32768.0).powi(2))
            .sum::<f64>()
            / v.len() as f64)
            .sqrt()
    }
    fn resample(rate: u32, hz: f64) -> Vec<i16> {
        let mut r = Resampler::new(rate, OUT_RATE);
        let mut out = Vec::new();
        for chunk in tone(rate, hz, 1.0).chunks(1000) {
            r.feed(chunk, &mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn keeps_length_and_in_band_tone() {
        for rate in [48_000, 44_100, 22_050] {
            let out = resample(rate, 440.0);
            assert!(
                (out.len() as i64 - 16_000).abs() <= 3,
                "{rate}: got {} samples",
                out.len()
            );
            let mid = &out[2_000..14_000];
            let expected = 0.5 / 2f64.sqrt();
            assert!(
                (rms(mid) - expected).abs() / expected < 0.05,
                "{rate}: rms {}",
                rms(mid)
            );
        }
    }

    #[test]
    fn removes_out_of_band_tone() {
        // 12 kHz is above the 8 kHz output Nyquist and would alias without the low-pass filter.
        let out = resample(48_000, 12_000.0);
        assert!(
            rms(&out[2_000..14_000]) < 0.02,
            "aliased energy {}",
            rms(&out[2_000..14_000])
        );
    }

    #[test]
    fn chunk_size_does_not_change_the_result() {
        let input = tone(48_000, 1_000.0, 0.5);
        let run = |size: usize| {
            let mut r = Resampler::new(48_000, OUT_RATE);
            let mut o = Vec::new();
            input.chunks(size).for_each(|c| r.feed(c, &mut o));
            r.finish(&mut o);
            o
        };
        assert_eq!(run(333), run(4096));
    }

    #[test]
    fn wav_header_is_valid() {
        let w = wav_bytes(&[1, -2, 3]);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), OUT_RATE);
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 6);
        assert_eq!(w.len(), 44 + 6);
    }

    #[test]
    fn missing_and_bad_files_give_clear_errors() {
        assert!(extract_wav(Path::new("Z:/does/not/exist.mov"), None).is_err());
        let dir = std::env::temp_dir().join("scs-test-bad.mov");
        std::fs::write(&dir, b"this is not a video").unwrap();
        assert!(extract_wav(&dir, None).is_err());
        let _ = std::fs::remove_file(dir);
    }

    #[test]
    fn real_test_video_decodes_to_about_9_seconds() {
        let video = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("test-video")
            .join("IMG_3571.MOV");
        if !video.is_file() {
            return; // sample video not present on this machine
        }
        let wav = extract_wav(&video, None).unwrap();
        let samples = (wav.len() - 44) / 2;
        let secs = samples as f64 / f64::from(OUT_RATE);
        assert!((8.5..9.5).contains(&secs), "duration {secs}s");
        let pcm: Vec<i16> = wav[44..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert!(rms(&pcm) > 0.005, "audio looks silent");
    }

    #[test]
    fn range_extraction_matches_the_same_part_of_the_full_audio() {
        let Some(video) = test_video() else { return };
        let pcm = |w: &[u8]| -> Vec<i16> {
            w[44..]
                .chunks(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect()
        };
        let full = pcm(&extract_wav(&video, None).unwrap());
        let part = pcm(&extract_wav(&video, Some((3.0, 6.0))).unwrap());
        let secs = part.len() as f64 / f64::from(OUT_RATE);
        assert!((2.9..3.1).contains(&secs), "range length {secs}s");
        // Compare the middle of the range with the same stretch of the full decode.
        let (a, b) = (3 * 16_000 + 4_000, 3 * 16_000 + 40_000);
        let want = &full[a..b];
        let got = &part[4_000..40_000];
        let err = (want
            .iter()
            .zip(got)
            .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
            .sum::<f64>()
            / want.len() as f64)
            .sqrt();
        let sig =
            (want.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / want.len() as f64).sqrt();
        assert!(
            err < sig * 0.05 + 8.0,
            "range audio differs from full audio: err {err}, signal {sig}"
        );
    }

    #[test]
    fn range_past_the_end_is_an_error_not_a_hang() {
        let Some(video) = test_video() else { return };
        assert!(extract_wav(&video, Some((500.0, 510.0))).is_err());
    }

    #[test]
    #[allow(clippy::panic)] // the panic is the point of this test
    fn a_panic_while_decoding_becomes_an_error() {
        let r: Result<(), String> = catching(|| panic!("decoder bug"));
        assert_eq!(
            r.unwrap_err(),
            "This file could not be read. It may be damaged."
        );
        assert_eq!(catching(|| Ok::<_, String>(7)).unwrap(), 7);
    }
}
