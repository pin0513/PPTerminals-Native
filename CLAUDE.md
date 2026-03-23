# PPTerminals Native

Pure Rust terminal app with Agent Farm. Zero webview.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| GUI | egui (GPU-accelerated) |
| Terminal Emulation | vt100 crate |
| PTY | portable-pty |
| CJK Font | Hiragino Sans GB (macOS) |

## Build

```bash
cargo run          # development
cargo build --release  # production
```

## Conventions

- All code in `src/*.rs`, no nested modules
- egui painter API for custom rendering (not ui.label per cell)
- `ctx.input()` for keyboard detection (not Event::Key)
- CJK wide chars: use `cell.is_wide()` from vt100

## Shortcuts

| Key | Action |
|-----|--------|
| ⌘T | New tab |
| ⌘W | Close tab (with confirm) |
| ⌘B / ⌘\\ | Toggle explorer |
| ⌘P | Quick Open |
| ⌥A/B/C | Switch to tab |
| F1 | Tab switcher |
| ⌘C | Copy selection/line |
| ⌘V | Paste |
| ⌘← / ⌘→ | Home / End |
