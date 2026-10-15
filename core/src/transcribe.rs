//! Local speech-to-text via whisper.cpp (Metal-accelerated on Apple Silicon).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Transcriber {
    ctx: WhisperContext,
    /// Whisper language code ("es", "en", …) or "auto" to detect per clip.
    language: String,
}

/// Returns the path to the ggml model in `dir`, downloading it on first run.
pub fn ensure_model(dir: &Path, name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("ggml-{name}.bin"));
    if path.exists() {
        return Ok(path);
    }

    let url = format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin");
    println!("Downloading whisper model {name} (one time)…\n  {url}");
    crate::download::fetch(&url, &path, |_, _| {})?;
    println!("Saved to {}", path.display());
    Ok(path)
}

impl Transcriber {
    pub fn load(model: &Path, language: String) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(
            model.to_str().context("model path is not utf-8")?,
            WhisperContextParameters::default(),
        )
        .context("failed to load whisper model")?;
        Ok(Self { ctx, language })
    }

    pub fn transcribe(&self, audio: &[f32]) -> Result<String> {
        let mut state = self.ctx.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&self.language));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        state.full(params, audio)?;

        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_string());
        }
        Ok(text.trim().to_string())
    }
}
