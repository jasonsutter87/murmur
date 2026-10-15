//! `cargo run --example dl -- <url> <dest>`: exercise the model downloader.
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    murmur_core::download::fetch(&a[1], a[2].as_ref(), |d, t| eprint!("\r{d}/{t}"))?;
    eprintln!();
    Ok(())
}
