# murmur

Hold **⌥Space**, talk, let go. Clean text is pasted wherever your cursor is, in
Terminal, Word, Slack, the browser, anything.

Clean-room dictation app (Wispr Flow-style), built from public techniques only.

```
hotkey ─▶ mic (cpal) ─▶ whisper.cpp, local + Metal ─▶ Claude cleanup (optional) ─▶ ⌘V into focused app
```

## Run

    cargo run --release

- First run downloads the whisper model (~150 MB) to `~/Library/Application Support/murmur/models`.
- macOS will ask for **Microphone** access. Also add your terminal app under
  System Settings → Privacy & Security → **Accessibility** so murmur can paste.
- A menu-bar icon shows state: 🎙 idle · 🔴 recording · ⏳ processing.

## Config (env vars)

| Var | Default | |
|---|---|---|
| `ANTHROPIC_API_KEY` | unset | Enables the cleanup pass (filler removal, self-corrections, punctuation) |
| `MURMUR_CLAUDE_MODEL` | `claude-opus-5-5` | e.g. `claude-haiku-5-5` for lower latency/cost |
| `MURMUR_WHISPER_MODEL` | `small` | Multilingual. `base` is faster, `medium` / `large-v3-turbo` more accurate. `*.en` models are English-only |
| `MURMUR_LANGUAGE` | `auto` | Detect per clip, or pin one (`es`, `en`, …) for short-clip accuracy |

Debug: `murmur --file clip.wav` runs the pipeline on a 16 kHz mono WAV.

## Roadmap

- Per-app modes (terminal: no cleanup/literal; email: formal) via frontmost-app detection
- Personal dictionary, history, configurable hotkey
- iOS keyboard extension + Android IME sharing a Rust core
