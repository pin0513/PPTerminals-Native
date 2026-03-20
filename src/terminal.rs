use eframe::egui;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

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
}

#[derive(Clone)]
struct Cell {
    ch: String,
    fg: egui::Color32,
    bg: egui::Color32,
    bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: " ".to_string(),
            fg: egui::Color32::from_rgb(230, 230, 230),
            bg: egui::Color32::TRANSPARENT,
            bold: false,
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
        }
    }

    pub fn close(&mut self) {
        self.writer = None;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // Handle keyboard input
        let events = ctx.input(|i| i.events.clone());
        for event in &events {
            match event {
                egui::Event::Text(text) => {
                    self.write_pty(text.as_bytes());
                }
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    self.handle_key(*key, modifiers);
                }
                _ => {}
            }
        }

        // Render terminal grid
        let buffer = self.screen_buffer.lock().unwrap().clone();
        let (cursor_r, cursor_c) = *self.cursor.lock().unwrap();
        let font = egui::FontId::monospace(14.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (r, row) in buffer.iter().enumerate() {
                ui.horizontal(|ui| {
                    for (c, cell) in row.iter().enumerate() {
                        let is_cursor = r as u16 == cursor_r && c as u16 == cursor_c;
                        let text_color = if is_cursor { egui::Color32::from_rgb(10, 14, 20) } else { cell.fg };
                        let bg_color = if is_cursor { egui::Color32::from_rgb(88, 166, 255) } else { cell.bg };

                        let text = egui::RichText::new(&cell.ch)
                            .font(font.clone())
                            .color(text_color)
                            .background_color(bg_color);

                        if cell.bold {
                            ui.label(text.strong());
                        } else {
                            ui.label(text);
                        }
                    }
                });
            }
        });
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
