---
name: PPTerminals Native
description: 純 Rust native terminal app（egui + vt100 + portable-pty）開發指南。涵蓋 terminal rendering、Agent Farm、session tracking、keyboard handling。當修改 PPTerminals-Native 的任何 Rust 代碼時使用。
---

# PPTerminals Native Development Guide

純 Rust terminal app，零 webview。GPU 渲染 via egui，terminal emulation via vt100。

## Architecture

```
egui (GPU UI) → vt100::Parser (ANSI emulation) → portable-pty (PTY)
```

## Module Map

| File | Purpose |
|------|---------|
| `main.rs` | App struct, menu bar, tab bar, keyboard shortcuts, layout |
| `terminal.rs` | PTY session, vt100 screen rendering, key input, autocomplete, selection |
| `farm.rs` | Agent Farm pixel art (hens, chicks, ascension, gravestones, heaven) |
| `session.rs` | Claude session detection, sub-agent/token tracking from PTY output |
| `explorer.rs` | File tree sidebar, drag-and-drop, navigation |
| `autocomplete.rs` | PATH command + --help flag completion |
| `quick_open.rs` | Cmd+P fuzzy file search |

## Terminal Rendering Pattern

vt100 handles all ANSI parsing. We render the screen grid with `egui::Painter`:

```rust
// Each cell painted at exact pixel position
let x = origin.x + col as f32 * CHAR_W;
let y = origin.y + row as f32 * CHAR_H;
painter.text(Pos2::new(x, y), Align2::LEFT_TOP, &cell.ch, font, fg_color);
```

Key considerations:
- `cell.is_wide()` → CJK/emoji take 2 columns, skip continuation cells
- `cell.is_wide_continuation()` → skip in reader thread
- Cursor: thin 2px blinking bar, not block
- Background: per-cell `rect_filled` for colored backgrounds

## Keyboard Handling

egui doesn't have traditional focus. Use `ctx.input(|i| i.key_pressed(Key::Enter))` for reliable detection, NOT `Event::Key` matching.

```rust
// Reliable pattern:
if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
    self.write_pty(b"\r");
}
// Text input via Event::Text:
let texts: Vec<String> = ctx.input(|i| {
    i.events.iter().filter_map(|e| {
        if let Event::Text(t) = e { Some(t.clone()) } else { None }
    }).collect()
});
```

macOS shortcuts:
- `Cmd+C` → copy (ctx.copy_text)
- `Cmd+V` → paste (Event::Paste)
- `Cmd+Left/Right` → Ctrl+A/E (Home/End)
- `Ctrl+C` → send \x03 to PTY

## Agent Farm API

```rust
let hen_idx = farm.add_hen("A", "$0.00", session_hash);
farm.add_chick(hen_idx, "A1", "$0.00");
farm.ascend_chick("A1");  // triggers ascension → gravestone → heaven
farm.set_cost("A", "$1.23");
```

## Session Detection

`session.rs` parses stripped ANSI output for:
- `Claude Code v` → session start
- `Running N agents` → sub-agent count
- `↓ N tokens` → token accumulation

## CJK Font

macOS: `include_bytes!("/System/Library/Fonts/Hiragino Sans GB.ttc")` as fallback font.

## Build & Run

```bash
cargo run          # dev
cargo build --release  # production
```
