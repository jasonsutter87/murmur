//! murmur — hold ⌥Space, talk, let go. Clean text is pasted where your cursor is.
//!
//! Pipeline: hotkey → mic → whisper.cpp (local) → optional Claude cleanup → paste.

mod cleanup;
mod insert;

use murmur_core::{audio, transcribe};

use anyhow::Result;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::TrayIconBuilder;

const IDLE: &str = "🎙";
const RECORDING: &str = "🔴";
const THINKING: &str = "⏳";
/// Taps shorter than this are treated as accidental.
const MIN_SAMPLES: usize = audio::WHISPER_RATE as usize * 3 / 10;

enum AppEvent {
    Done(Option<String>),
}

fn chime(name: &str) {
    let _ = std::process::Command::new("afplay")
        .arg(format!("/System/Library/Sounds/{name}.aiff"))
        .spawn();
}

/// Launched as an .app there is no terminal; send output to ~/Library/Logs/murmur.log.
fn redirect_output_if_detached() {
    use std::io::IsTerminal;
    use std::os::fd::AsRawFd;
    if std::io::stdout().is_terminal() {
        return;
    }
    let Some(logs) = dirs::home_dir().map(|h| h.join("Library/Logs")) else { return };
    let _ = std::fs::create_dir_all(&logs);
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("murmur.log"))
    {
        unsafe {
            libc::dup2(file.as_raw_fd(), libc::STDOUT_FILENO);
            libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO);
        }
    }
}

fn main() -> Result<()> {
    if std::env::var_os("MURMUR_NO_LOGFILE").is_none() {
        redirect_output_if_detached();
    }
    let model_name = std::env::var("MURMUR_WHISPER_MODEL").unwrap_or_else(|_| "small".into());
    let language = std::env::var("MURMUR_LANGUAGE").unwrap_or_else(|_| "auto".into());
    let models_dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("no data dir"))?
        .join("murmur")
        .join("models");
    let model_path = transcribe::ensure_model(&models_dir, &model_name)?;
    let transcriber = Arc::new(transcribe::Transcriber::load(&model_path, language)?);
    let cleaner = Arc::new(cleanup::Cleaner::from_env());
    match cleaner.as_ref() {
        Some(c) => println!("Cleanup: {}", c.model()),
        None => println!("Cleanup: off (set ANTHROPIC_API_KEY to enable)"),
    }

    // Debug mode: `murmur --file speech.wav` runs the pipeline on a 16 kHz mono WAV.
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, path] = args.as_slice() {
        if flag == "--file" {
            let samples: Vec<f32> = hound::WavReader::open(path)?
                .samples::<i16>()
                .map(|s| s.map(|s| s as f32 / i16::MAX as f32))
                .collect::<Result<_, _>>()?;
            process(&transcriber, cleaner.as_ref().as_ref(), &samples);
            return Ok(());
        }
    }

    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();

    let hotkeys = GlobalHotKeyManager::new()?;
    let talk = HotKey::new(Some(Modifiers::ALT), Code::Space);
    hotkeys.register(talk)?;

    let quit = MenuItem::new("Quit murmur", true, None);
    let menu = Menu::new();
    menu.append(&quit)?;
    let mut menu = Some(menu);
    let mut tray = None;

    let mut recording: Option<audio::Recording> = None;
    let mut busy = false;
    println!("Ready. Hold ⌥Space to dictate.");

    event_loop.run(move |event, _, control_flow| {
        let _ = &hotkeys; // keep the hotkey registered for the app's lifetime
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(15));

        match event {
            // macOS requires the tray to be created after the loop starts.
            Event::NewEvents(StartCause::Init) => {
                tray = TrayIconBuilder::new()
                    .with_title(IDLE)
                    .with_tooltip("murmur — hold ⌥Space to dictate")
                    .with_menu(Box::new(menu.take().unwrap()))
                    .build()
                    .ok();
            }
            Event::UserEvent(AppEvent::Done(text)) => {
                busy = false;
                if let Some(text) = text {
                    if let Err(e) = insert::paste(&text) {
                        eprintln!("paste failed: {e} (grant Accessibility access?)");
                    }
                }
                if let Some(t) = &tray {
                    t.set_title(Some(IDLE));
                }
            }
            _ => {}
        }

        while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
            if ev.id != talk.id() {
                continue;
            }
            match ev.state {
                HotKeyState::Pressed if recording.is_none() && !busy => match audio::start() {
                    Ok(r) => {
                        recording = Some(r);
                        chime("Tink");
                        if let Some(t) = &tray {
                            t.set_title(Some(RECORDING));
                        }
                    }
                    Err(e) => eprintln!("mic error: {e} (grant Microphone access?)"),
                },
                HotKeyState::Released => {
                    let Some(r) = recording.take() else { continue };
                    let samples = r.finish();
                    if samples.len() < MIN_SAMPLES {
                        if let Some(t) = &tray {
                            t.set_title(Some(IDLE));
                        }
                        continue;
                    }
                    chime("Pop");
                    busy = true;
                    if let Some(t) = &tray {
                        t.set_title(Some(THINKING));
                    }
                    let (transcriber, cleaner, proxy) =
                        (transcriber.clone(), cleaner.clone(), proxy.clone());
                    std::thread::spawn(move || {
                        let text = process(&transcriber, cleaner.as_ref().as_ref(), &samples);
                        let _ = proxy.send_event(AppEvent::Done(text));
                    });
                }
                _ => {}
            }
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == *quit.id() {
                *control_flow = ControlFlow::Exit;
            }
        }
    });
}

fn process(
    transcriber: &transcribe::Transcriber,
    cleaner: Option<&cleanup::Cleaner>,
    samples: &[f32],
) -> Option<String> {
    let started = Instant::now();
    let raw = match transcriber.transcribe(samples) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => return None,
        Err(e) => {
            eprintln!("transcription failed: {e}");
            return None;
        }
    };
    let stt_ms = started.elapsed().as_millis();

    let text = match cleaner {
        Some(c) => c.clean(&raw).unwrap_or_else(|e| {
            eprintln!("cleanup skipped: {e}");
            raw.clone()
        }),
        None => raw.clone(),
    };
    println!(
        "[{stt_ms}ms stt, {}ms total] {raw}\n  → {text}",
        started.elapsed().as_millis()
    );
    Some(text)
}
