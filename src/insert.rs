//! Put text into whatever app has focus: stash the clipboard, paste, restore.
//! Must run on the main thread (macOS keyboard APIs are main-thread only).

use anyhow::Result;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// macOS virtual keycode for the "V" key (kVK_ANSI_V). Using the raw code
/// skips a keyboard-layout lookup that is not thread-safe.
const KVK_ANSI_V: u16 = 9;

pub fn paste(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text)?;

    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Meta, Direction::Press)?;
    enigo.raw(KVK_ANSI_V, Direction::Click)?;
    enigo.key(Key::Meta, Direction::Release)?;

    // Give the target app a moment to read the clipboard before restoring it.
    if let Some(previous) = previous {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            if let Ok(mut cb) = Clipboard::new() {
                let _ = cb.set_text(previous);
            }
        });
    }
    Ok(())
}
