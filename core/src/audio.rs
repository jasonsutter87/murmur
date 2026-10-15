//! Microphone capture. Records at the device's native format, then hands
//! back 16 kHz mono f32 — the shape whisper expects.

use anyhow::{bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use std::sync::{Arc, Mutex};

pub const WHISPER_RATE: u32 = 16_000;

pub struct Recording {
    stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: u16,
}

pub fn start() -> Result<Recording> {
    let device = cpal::default_host()
        .default_input_device()
        .context("no microphone found")?;
    let supported = device.default_input_config()?;
    let rate = supported.sample_rate();
    let channels = supported.channels();
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let buf = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
    let on_err = |e| eprintln!("audio stream error: {e}");

    let stream = match format {
        SampleFormat::F32 => {
            let buf = buf.clone();
            device.build_input_stream(
                config.clone(),
                move |data: &[f32], _| buf.lock().unwrap().extend_from_slice(data),
                on_err,
                None,
            )?
        }
        SampleFormat::I16 => {
            let buf = buf.clone();
            device.build_input_stream(
                config.clone(),
                move |data: &[i16], _| {
                    buf.lock()
                        .unwrap()
                        .extend(data.iter().map(|&s| s as f32 / i16::MAX as f32))
                },
                on_err,
                None,
            )?
        }
        other => bail!("unsupported mic sample format: {other:?}"),
    };
    stream.play()?;
    Ok(Recording { stream, buf, rate, channels })
}

impl Recording {
    /// Stop recording and return 16 kHz mono samples.
    pub fn finish(self) -> Vec<f32> {
        drop(self.stream);
        let raw = std::mem::take(&mut *self.buf.lock().unwrap());
        let mono = downmix(&raw, self.channels as usize);
        resample(&mono, self.rate, WHISPER_RATE)
    }
}

fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Linear-interpolation resampler. Plenty for speech headed to whisper.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = *input.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// Play mono `samples` at `rate` Hz on the default output device. Blocks until
/// done, or until `stop` is set (checked every 20 ms).
pub fn play(samples: &[f32], rate: u32, stop: &std::sync::atomic::AtomicBool) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let device = cpal::default_host()
        .default_output_device()
        .context("no speaker found")?;
    let supported = device.default_output_config()?;
    let out_rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let audio = Arc::new(resample(samples, rate, out_rate));
    let pos = Arc::new(AtomicUsize::new(0));
    let on_err = |e| eprintln!("playback error: {e}");

    // Fill each output frame from the shared buffer; silence once it runs out.
    fn fill<T: Copy>(out: &mut [T], channels: usize, audio: &[f32], pos: &AtomicUsize, conv: impl Fn(f32) -> T) {
        let start = pos.load(Ordering::Relaxed);
        let frames = out.len() / channels;
        for (i, frame) in out.chunks_mut(channels).enumerate() {
            let s = audio.get(start + i).copied().unwrap_or(0.0);
            frame.fill(conv(s));
        }
        pos.store(start + frames, Ordering::Relaxed);
    }

    let stream = match format {
        SampleFormat::F32 => {
            let (audio, pos) = (audio.clone(), pos.clone());
            device.build_output_stream(
                config,
                move |out: &mut [f32], _| fill(out, channels, &audio, &pos, |s| s),
                on_err,
                None,
            )?
        }
        SampleFormat::I16 => {
            let (audio, pos) = (audio.clone(), pos.clone());
            device.build_output_stream(
                config,
                move |out: &mut [i16], _| {
                    fill(out, channels, &audio, &pos, |s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                },
                on_err,
                None,
            )?
        }
        other => bail!("unsupported speaker sample format: {other:?}"),
    };
    stream.play()?;
    // Wait for the buffer to drain, plus a little tail so the last frames are heard.
    let tail = (out_rate / 10) as usize;
    while pos.load(Ordering::Relaxed) < audio.len() + tail {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok(())
}
