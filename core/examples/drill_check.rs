//! `cargo run --example drill_check -- <model.bin> <16k-mono.wav> "<target phrase>"`
//! Runs a recording through whisper + drill scoring without a mic.
use murmur_core::{drill, transcribe};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, model, wav, phrase] = args.as_slice() else {
        anyhow::bail!("usage: drill_check <model> <wav> <phrase>")
    };
    let samples: Vec<f32> = hound::WavReader::open(wav)?
        .samples::<i16>()
        .map(|s| s.map(|s| s as f32 / i16::MAX as f32))
        .collect::<Result<_, _>>()?;
    let t = transcribe::Transcriber::load(model.as_ref(), "es".into())?;
    let r = drill::score(phrase, &t.transcribe(&samples)?);
    println!("heard: {:?}  score: {}", r.transcript, r.score);
    for w in &r.words {
        println!("  {:<12} {:?} {}", w.word, w.status, w.heard.as_deref().unwrap_or(""));
    }
    Ok(())
}
