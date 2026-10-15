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
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
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
