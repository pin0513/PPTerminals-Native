use eframe::egui;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::autocomplete::AutocompleteState;

pub struct TerminalTab {
    pub hotkey: String,
    pub title: String,
    pub cwd: String,
    writer: Option<Box<dyn Write + Send>>,
    screen_buffer: Arc<Mutex<Vec<Vec<Cell>>>>,
    cursor: Arc<Mutex<(u16, u16)>>,
    cols: u16,
    rows: u16,
    input_buf: String,
    parser: Arc<Mutex<vt100::Parser>>,
    pub autocomplete: AutocompleteState,
}

#[derive(Clone)]
struct Cell {
    ch: String,
    fg: egui::Color32,
    bg: egui::Color32,
    bold: bool,
    wide: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: " ".to_string(),
            fg: egui::Color32::from_rgb(230, 230, 230),
            bg: egui::Color32::TRANSPARENT,
            bold: false,
            wide: false,
        }
    }
}

fn vt_color_to_egui(c: vt100::Color, is_fg: bool) -> egui::Color32 {
    match c {
        vt100::Color::Default => {
            if is_fg { egui::Color32::from_rgb(230, 230, 230) }
            else { egui::Color32::TRANSPARENT }
        }
        vt100::Color::Idx(i) => match i {
            0 => egui::Color32::from_rgb(10, 14, 20),
            1 => egui::Color32::from_rgb(248, 81, 73),
            2 => egui::Color32::from_rgb(63, 185, 80),
            3 => egui::Color32::from_rgb(210, 153, 34),
            4 => egui::Color32::from_rgb(88, 166, 255),
            5 => egui::Color32::from_rgb(188, 140, 255),
            6 => egui::Color32::from_rgb(57, 197, 207),
            7 => egui::Color32::from_rgb(230, 230, 230),
            8 => egui::Color32::from_rgb(72, 79, 88),
            9 => egui::Color32::from_rgb(255, 123, 114),
            10 => egui::Color32::from_rgb(86, 211, 100),
            11 => egui::Color32::from_rgb(227, 179, 65),
            12 => egui::Color32::from_rgb(121, 192, 255),
            13 => egui::Color32::from_rgb(210, 168, 255),
            14 => egui::Color32::from_rgb(86, 212, 221),
            15 => egui::Color32::from_rgb(255, 255, 255),
            _ => egui::Color32::from_rgb(200, 200, 200),
        }
        vt100::Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
    }
}

impl TerminalTab {
    pub fn new(hotkey: &str) -> Self {
        let cols: u16 = 80;
        let rows: u16 = 24;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let screen_buffer = Arc::new(Mutex::new(Vec::new()));
        let cursor = Arc::new(Mutex::new((0u16, 0u16)));

        let cwd = dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());

        // Create PTY
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .expect("Failed to open PTY");

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        cmd.cwd(&cwd);

        let _child = pair.slave.spawn_command(cmd).expect("Failed to spawn shell");
        drop(pair.slave);

        let writer = pair.master.take_writer().expect("Failed to take writer");
        let mut reader = pair.master.try_clone_reader().expect("Failed to clone reader");

        // Reader thread — feed PTY output to vt100 parser
        let parser_clone = parser.clone();
        let buffer_clone = screen_buffer.clone();
        let cursor_clone = cursor.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let mut p = parser_clone.lock().unwrap();
                        p.process(&buf[..n]);

                        // Update screen buffer from parser
                        let screen = p.screen();
                        let (sr, sc) = screen.size();
                        let mut rows_out = Vec::with_capacity(sr as usize);
                        for r in 0..sr {
                            let mut row = Vec::with_capacity(sc as usize);
                            for c in 0..sc {
                                if let Some(cell) = screen.cell(r, c) {
                                    if cell.is_wide_continuation() {
                                        continue; // skip wide char continuation
                                    }
                                    row.push(Cell {
                                        ch: {
                                            let s = cell.contents();
                                            if s.is_empty() { " ".to_string() } else { s.to_string() }
                                        },
                                        fg: vt_color_to_egui(cell.fgcolor(), true),
                                        bg: vt_color_to_egui(cell.bgcolor(), false),
                                        bold: cell.bold(),
                                        wide: cell.is_wide(),
                                    });
                                } else {
                                    row.push(Cell::default());
                                }
                            }
                            rows_out.push(row);
                        }
                        *buffer_clone.lock().unwrap() = rows_out;
                        let cpos = screen.cursor_position();
                        *cursor_clone.lock().unwrap() = (cpos.0, cpos.1);
                    }
                    Err(_) => break,
                }
            }
        });

        let title = cwd.split('/').last().unwrap_or("~").to_string();

        Self {
            hotkey: hotkey.to_string(),
            title,
            cwd,
            writer: Some(writer),
            screen_buffer,
            cursor,
            cols,
            rows,
            input_buf: String::new(),
            parser,
            autocomplete: AutocompleteState::new(),
        }
    }

    pub fn close(&mut self) {
        self.writer = None;
    }

    pub fn launch_claude(&mut self) {
        self.title = "Claude".to_string();
        // Delay to let PTY init
        self.write_pty(b"claude --dangerously-skip-permissions\n");
    }

    /// Read current input line from vt100 screen (strip prompt)
    fn read_current_input(&self) -> String {
        let p = self.parser.lock().unwrap();
        let screen = p.screen();
        let (cr, _cc) = screen.cursor_position();
        let cols = screen.size().1;
        let mut line = String::new();
        for c in 0..cols {
            if let Some(cell) = screen.cell(cr, c) {
                line.push_str(cell.contents());
            }
        }
        let line = line.trim_end().to_string();
        // Strip prompt: find $, %, ❯, ➜ with shell context before it
        if let Some(pos) = line.rfind(|c: char| "❯➜$%".contains(c)) {
            let before = &line[..pos];
            if before.contains('@') || before.contains('~') || before.contains('/') {
                return line[pos+1..].trim_start().to_string();
            }
        }
        String::new()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // ─── Keyboard input: use ctx.input() for reliable key detection ───
        // Text input (regular characters)
        let text_input: Vec<String> = ctx.input(|i| {
            i.events.iter().filter_map(|e| {
                if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }
            }).collect()
        });
        for text in &text_input {
            self.write_pty(text.as_bytes());
            let input = self.read_current_input();
            self.autocomplete.update(&input);
        }

        // Special keys
        let mods = ctx.input(|i| i.modifiers);

        // ─── macOS Cmd shortcuts ───
        // Cmd+A → select all (copy all visible terminal text to clipboard)
        if ctx.input(|i| i.key_pressed(egui::Key::A) && i.modifiers.command) {
            let all_text = self.get_visible_text();
            ctx.copy_text(all_text);
        }
        // Cmd+C → copy current line (or all if nothing specific)
        if ctx.input(|i| i.key_pressed(egui::Key::C) && i.modifiers.command) {
            let line = self.get_current_line();
            ctx.copy_text(line);
        }
        // Cmd+V → paste clipboard into PTY
        if ctx.input(|i| i.key_pressed(egui::Key::V) && i.modifiers.command) {
            if let Some(text) = ui.ctx().input(|i| i.events.iter().find_map(|e| {
                if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }
            })) {
                self.write_pty(text.as_bytes());
            }
        }
        // Cmd+Left → Home
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) && i.modifiers.command) {
            self.write_pty(b"\x01");
        }
        // Cmd+Right → End
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) && i.modifiers.command) {
            self.write_pty(b"\x05");
        }
        // Cmd+Backspace → clear line
        if ctx.input(|i| i.key_pressed(egui::Key::Backspace) && i.modifiers.command) {
            self.write_pty(b"\x15");
        }

        // Autocomplete interception
        if self.autocomplete.visible {
            if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                let input = self.read_current_input();
                if let Some(insert) = self.autocomplete.accept(&input) {
                    self.write_pty(insert.as_bytes());
                }
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.autocomplete.move_selection(-1);
            } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.autocomplete.move_selection(1);
            } else if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.autocomplete.dismiss();
            } else if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.write_pty(b"\r");
                self.autocomplete.reset();
            }
        } else {
            // Normal key handling
            if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                let data = if mods.shift { b"\n".as_slice() } else { b"\r".as_slice() };
                self.write_pty(data);
                self.autocomplete.reset();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Backspace) && !i.modifiers.command) {
                if mods.alt { self.write_pty(b"\x17"); }
                else { self.write_pty(&[0x7f]); }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Tab)) {
                self.write_pty(b"\t");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.write_pty(b"\x1b");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                self.write_pty(b"\x1b[A");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                self.write_pty(b"\x1b[B");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) && !i.modifiers.command) {
                if mods.alt { self.write_pty(b"\x1bf"); }
                else { self.write_pty(b"\x1b[C"); }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) && !i.modifiers.command) {
                if mods.alt { self.write_pty(b"\x1bb"); }
                else { self.write_pty(b"\x1b[D"); }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Home)) { self.write_pty(b"\x01"); }
            if ctx.input(|i| i.key_pressed(egui::Key::End)) { self.write_pty(b"\x05"); }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete)) { self.write_pty(b"\x1b[3~"); }

            // Ctrl+letter (C=3, D=4, etc.)
            if mods.ctrl {
                let ctrl_keys = [
                    (egui::Key::A, 1u8), (egui::Key::B, 2), (egui::Key::C, 3), (egui::Key::D, 4),
                    (egui::Key::E, 5), (egui::Key::F, 6), (egui::Key::G, 7), (egui::Key::H, 8),
                    (egui::Key::K, 11), (egui::Key::L, 12), (egui::Key::N, 14),
                    (egui::Key::P, 16), (egui::Key::R, 18), (egui::Key::U, 21),
                    (egui::Key::W, 23), (egui::Key::Z, 26),
                ];
                for (key, code) in &ctrl_keys {
                    if ctx.input(|i| i.key_pressed(*key)) {
                        self.write_pty(&[*code]);
                    }
                }
            }
        }

        // Render terminal grid using painter (zero spacing, monospace)
        let buffer = self.screen_buffer.lock().unwrap().clone();
        let (cursor_r, cursor_c) = *self.cursor.lock().unwrap();
        let font = egui::FontId::monospace(14.0);
        let char_w = 8.4_f32;
        let char_h = 18.0_f32;

        let (response, painter) = ui.allocate_painter(
            egui::Vec2::new(ui.available_width(), buffer.len() as f32 * char_h),
            egui::Sense::click_and_drag(),
        );

        // Request keyboard focus so we receive key events
        let term_id = response.id;
        if response.clicked() || !ctx.memory(|m| m.has_focus(term_id)) {
            ctx.memory_mut(|m| m.request_focus(term_id));
        }
        let origin = response.rect.min;

        // Background
        painter.rect_filled(response.rect, 0.0, egui::Color32::from_rgb(10, 14, 20));

        for (r, row) in buffer.iter().enumerate() {
            let mut col = 0usize;
            for cell in row.iter() {
                let x = origin.x + col as f32 * char_w;
                let y = origin.y + r as f32 * char_h;
                let is_cursor = r as u16 == cursor_r && col as u16 == cursor_c;

                // Background
                let bg = if is_cursor {
                    egui::Color32::from_rgb(88, 166, 255)
                } else if cell.bg != egui::Color32::TRANSPARENT {
                    cell.bg
                } else {
                    egui::Color32::TRANSPARENT
                };
                if bg != egui::Color32::TRANSPARENT {
                    let cell_w = if cell.wide { char_w * 2.0 } else { char_w };
                    painter.rect_filled(
                        egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(cell_w, char_h)),
                        0.0, bg,
                    );
                }

                // Text
                let fg = if is_cursor { egui::Color32::from_rgb(10, 14, 20) } else { cell.fg };
                if cell.ch != " " {
                    let f = if cell.bold {
                        egui::FontId::new(14.0, egui::FontFamily::Monospace)
                    } else {
                        font.clone()
                    };
                    painter.text(
                        egui::Pos2::new(x, y),
                        egui::Align2::LEFT_TOP,
                        &cell.ch,
                        f,
                        fg,
                    );
                }

                // Advance column
                col += if cell.wide { 2 } else { 1 };
            }
        }

        // ─── Drop zone: accept dragged paths from Explorer ───
        let drop_resp = ui.interact(response.rect, ui.id().with("drop"), egui::Sense::hover());
        if drop_resp.hovered() {
            // Check if something is being dragged
            if let Some(payload) = egui::DragAndDrop::payload::<String>(ui.ctx()) {
                // Show drop indicator
                painter.rect_stroke(
                    response.rect,
                    4.0,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(88, 166, 255)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    response.rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop to insert path",
                    egui::FontId::monospace(14.0),
                    egui::Color32::from_rgb(88, 166, 255),
                );
            }
        }
        // Handle drop
        if drop_resp.hovered() {
            if let Some(payload) = egui::DragAndDrop::take_payload::<String>(ui.ctx()) {
                let path = (*payload).clone();
                let escaped = if path.contains(' ') { format!("\"{}\" ", path) } else { format!("{} ", path) };
                self.write_pty(escaped.as_bytes());
            }
        }

        // ─── Autocomplete popup ───
        if self.autocomplete.visible && !self.autocomplete.suggestions.is_empty() {
            let ac_x = origin.x + cursor_c as f32 * char_w;
            let ac_y = origin.y + (cursor_r as f32 + 1.0) * char_h;

            let popup_w = 300.0_f32;
            let item_h = 22.0_f32;
            let popup_h = self.autocomplete.suggestions.len() as f32 * item_h + 24.0;

            // Background
            painter.rect_filled(
                egui::Rect::from_min_size(egui::Pos2::new(ac_x, ac_y), egui::Vec2::new(popup_w, popup_h)),
                4.0, egui::Color32::from_rgb(28, 33, 40),
            );
            painter.rect_stroke(
                egui::Rect::from_min_size(egui::Pos2::new(ac_x, ac_y), egui::Vec2::new(popup_w, popup_h)),
                4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 54, 61)), egui::StrokeKind::Outside,
            );

            for (i, s) in self.autocomplete.suggestions.iter().enumerate() {
                let iy = ac_y + 2.0 + i as f32 * item_h;
                let selected = i == self.autocomplete.selected;

                if selected {
                    painter.rect_filled(
                        egui::Rect::from_min_size(egui::Pos2::new(ac_x + 2.0, iy), egui::Vec2::new(popup_w - 4.0, item_h)),
                        2.0, egui::Color32::from_rgb(38, 79, 120),
                    );
                }

                let kind_icon = match s.kind {
                    crate::autocomplete::CompletionKind::Command => "▸",
                    crate::autocomplete::CompletionKind::Flag => "⚑",
                    crate::autocomplete::CompletionKind::Option => "◆",
                    crate::autocomplete::CompletionKind::Subcommand => "▸",
                };
                let kind_color = match s.kind {
                    crate::autocomplete::CompletionKind::Command => egui::Color32::from_rgb(63, 185, 80),
                    crate::autocomplete::CompletionKind::Flag => egui::Color32::from_rgb(88, 166, 255),
                    crate::autocomplete::CompletionKind::Option => egui::Color32::from_rgb(210, 153, 34),
                    crate::autocomplete::CompletionKind::Subcommand => egui::Color32::from_rgb(63, 185, 80),
                };

                painter.text(egui::Pos2::new(ac_x + 8.0, iy + 3.0), egui::Align2::LEFT_TOP, kind_icon,
                    egui::FontId::monospace(10.0), kind_color);
                painter.text(egui::Pos2::new(ac_x + 22.0, iy + 3.0), egui::Align2::LEFT_TOP, &s.name,
                    egui::FontId::monospace(12.0), egui::Color32::from_rgb(230, 237, 243));
                if !s.description.is_empty() {
                    let desc: String = s.description.chars().take(30).collect();
                    painter.text(egui::Pos2::new(ac_x + 140.0, iy + 4.0), egui::Align2::LEFT_TOP, &desc,
                        egui::FontId::monospace(10.0), egui::Color32::from_rgb(72, 79, 88));
                }
            }

            // Hint
            painter.text(
                egui::Pos2::new(ac_x + 8.0, ac_y + popup_h - 18.0),
                egui::Align2::LEFT_TOP,
                "Tab accept  ↑↓ navigate  Esc dismiss",
                egui::FontId::monospace(9.0),
                egui::Color32::from_rgb(72, 79, 88),
            );
        }
    }

    fn handle_key(&mut self, key: egui::Key, mods: &egui::Modifiers) {
        let data: Vec<u8> = match key {
            egui::Key::Enter => vec![b'\r'],
            egui::Key::Backspace => vec![0x7f],
            egui::Key::Tab => vec![b'\t'],
            egui::Key::Escape => vec![0x1b],
            egui::Key::ArrowUp => b"\x1b[A".to_vec(),
            egui::Key::ArrowDown => b"\x1b[B".to_vec(),
            egui::Key::ArrowRight => {
                if mods.command { vec![0x05] } // Cmd+Right = End
                else if mods.alt { b"\x1bf".to_vec() } // Alt+Right = word
                else { b"\x1b[C".to_vec() }
            }
            egui::Key::ArrowLeft => {
                if mods.command { vec![0x01] } // Cmd+Left = Home
                else if mods.alt { b"\x1bb".to_vec() } // Alt+Left = word
                else { b"\x1b[D".to_vec() }
            }
            egui::Key::Home => vec![0x01],
            egui::Key::End => vec![0x05],
            egui::Key::Delete => b"\x1b[3~".to_vec(),
            egui::Key::PageUp => b"\x1b[5~".to_vec(),
            egui::Key::PageDown => b"\x1b[6~".to_vec(),
            // Ctrl+C, Ctrl+D, etc.
            k if mods.ctrl => {
                if let Some(ch) = key_to_char(k) {
                    vec![ch as u8 - b'a' + 1]
                } else { vec![] }
            }
            _ => vec![],
        };

        if !data.is_empty() {
            self.write_pty(&data);
        }
    }

    pub fn write_pty_public(&mut self, data: &[u8]) {
        self.write_pty(data);
    }

    /// Get all visible terminal text (for Cmd+A)
    fn get_visible_text(&self) -> String {
        let p = self.parser.lock().unwrap();
        let screen = p.screen();
        let (rows, cols) = screen.size();
        let mut text = String::new();
        for r in 0..rows {
            let mut line = String::new();
            for c in 0..cols {
                if let Some(cell) = screen.cell(r, c) {
                    let s = cell.contents();
                    if cell.is_wide_continuation() { continue; }
                    line.push_str(if s.is_empty() { " " } else { s });
                }
            }
            text.push_str(line.trim_end());
            text.push('\n');
        }
        text.trim_end().to_string()
    }

    /// Get current line text (for Cmd+C)
    fn get_current_line(&self) -> String {
        let p = self.parser.lock().unwrap();
        let screen = p.screen();
        let (cr, _) = screen.cursor_position();
        let cols = screen.size().1;
        let mut line = String::new();
        for c in 0..cols {
            if let Some(cell) = screen.cell(cr, c) {
                let s = cell.contents();
                if cell.is_wide_continuation() { continue; }
                line.push_str(if s.is_empty() { " " } else { s });
            }
        }
        line.trim_end().to_string()
    }

    fn write_pty(&mut self, data: &[u8]) {
        if let Some(ref mut writer) = self.writer {
            let _ = writer.write_all(data);
            let _ = writer.flush();
        }
    }
}

fn key_to_char(key: egui::Key) -> Option<char> {
    match key {
        egui::Key::A => Some('a'), egui::Key::B => Some('b'), egui::Key::C => Some('c'),
        egui::Key::D => Some('d'), egui::Key::E => Some('e'), egui::Key::F => Some('f'),
        egui::Key::G => Some('g'), egui::Key::H => Some('h'), egui::Key::I => Some('i'),
        egui::Key::J => Some('j'), egui::Key::K => Some('k'), egui::Key::L => Some('l'),
        egui::Key::M => Some('m'), egui::Key::N => Some('n'), egui::Key::O => Some('o'),
        egui::Key::P => Some('p'), egui::Key::Q => Some('q'), egui::Key::R => Some('r'),
        egui::Key::S => Some('s'), egui::Key::T => Some('t'), egui::Key::U => Some('u'),
        egui::Key::V => Some('v'), egui::Key::W => Some('w'), egui::Key::X => Some('x'),
        egui::Key::Y => Some('y'), egui::Key::Z => Some('z'),
        _ => None,
    }
}
