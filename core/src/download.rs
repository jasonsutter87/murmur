//! Model download: writes to `<dest>.part`, renames when complete.
//!
//! Uses ureq with bundled webpki roots rather than reqwest: reqwest 0.13 always
//! verifies through the OS (rustls-platform-verifier), which panics on Android
//! unless initialized from Java first.

use anyhow::Result;
use std::io::{Read, Write};
use std::path::Path;

/// Download `url` to `dest`, calling `progress(done_bytes, total_bytes)` as it goes
/// (`total` is 0 when the server does not say).
pub fn fetch(url: &str, dest: &Path, mut progress: impl FnMut(u64, u64)) -> Result<()> {
    let resp = ureq::get(url).call()?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .unwrap_or(0);
    let mut body = resp.into_body().into_with_config().limit(u64::MAX).reader();
    let partial = dest.with_extension("part");
    let mut file = std::io::BufWriter::new(std::fs::File::create(&partial)?);
    let mut buf = vec![0u8; 1 << 16];
    let (mut done, mut last) = (0u64, 0u64);
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if done - last >= 1 << 20 {
            progress(done, total);
            last = done;
        }
    }
    file.flush()?;
    drop(file);
    if total != 0 && done != total {
        anyhow::bail!("download incomplete: {done} of {total} bytes");
    }
    std::fs::rename(&partial, dest)?;
    progress(done, total);
    Ok(())
}
