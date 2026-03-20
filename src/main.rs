mod terminal;
mod farm;
mod explorer;
mod autocomplete;
mod quick_open;
mod session;

use eframe::egui;
use terminal::TerminalTab;
use farm::AgentFarm;
use explorer::FileExplorer;
use quick_open::QuickOpen;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("PPTerminals Native"),
        ..Default::default()
    };

    eframe::run_native(
        "PPTerminals Native",
        options,
        Box::new(|cc| {
            // Load CJK font for Chinese/Japanese/Korean support
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "system_cjk".to_owned(),
                std::sync::Arc::new(egui::FontData::from_static(include_bytes!("/System/Library/Fonts/Hiragino Sans GB.ttc"))),
            );
            // Add CJK as fallback for both proportional and monospace
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap()
                .push("system_cjk".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap()
                .push("system_cjk".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            cc.egui_ctx.set_style(style);
            Ok(Box::new(App::new()))
        }),
    )
}

struct App {
    tabs: Vec<TerminalTab>,
    active_tab: usize,
    farm: AgentFarm,
    explorer: FileExplorer,
    show_farm: bool,
    show_explorer: bool,
    // Close confirmation
    close_confirm_tab: Option<usize>,
    // Tab switcher (F1)
    show_switcher: bool,
    // Per-tab completion tracking (true when session has exited)
    tab_completed: Vec<bool>,
    // Quick Open (Cmd+P)
    quick_open: QuickOpen,
}

impl App {
    fn new() -> Self {
        Self {
            tabs: vec![TerminalTab::new("A")],
            active_tab: 0,
            farm: AgentFarm::new(),
            explorer: FileExplorer::new(),
            show_farm: false,
            show_explorer: true,
            close_confirm_tab: None,
            show_switcher: false,
            tab_completed: vec![false],
            quick_open: QuickOpen::new(),
        }
    }

    fn new_tab(&mut self) {
        let hotkey = (b'A' + self.tabs.len() as u8) as char;
        self.tabs.push(TerminalTab::new(&hotkey.to_string()));
        self.tab_completed.push(false);
        self.active_tab = self.tabs.len() - 1;
    }

    fn request_close_tab(&mut self, idx: usize) {
        self.close_confirm_tab = Some(idx);
    }

    fn confirm_close_tab(&mut self) {
        if let Some(idx) = self.close_confirm_tab.take() {
            if idx < self.tabs.len() {
                self.tabs[idx].close();
                self.tabs.remove(idx);
                if idx < self.tab_completed.len() {
                    self.tab_completed.remove(idx);
                }
                if self.tabs.is_empty() {
                    self.active_tab = 0;
                } else if self.active_tab >= self.tabs.len() {
                    self.active_tab = self.tabs.len() - 1;
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // ─── Keyboard shortcuts ───
        ctx.input(|i| {
            if i.key_pressed(egui::Key::T) && i.modifiers.command {
                // Handled below after mutable borrow
            }
        });
        let new_tab = ctx.input(|i| i.key_pressed(egui::Key::T) && i.modifiers.command);
        let close_tab = ctx.input(|i| i.key_pressed(egui::Key::W) && i.modifiers.command);
        let toggle_explorer = ctx.input(|i| {
            (i.key_pressed(egui::Key::B) && i.modifiers.command) ||
            (i.key_pressed(egui::Key::Backslash) && i.modifiers.command)
        });
        let toggle_switcher = ctx.input(|i| i.key_pressed(egui::Key::F1));
        let next_tab = ctx.input(|i| i.key_pressed(egui::Key::Tab) && i.modifiers.command && !i.modifiers.shift);
        let prev_tab = ctx.input(|i| i.key_pressed(egui::Key::Tab) && i.modifiers.command && i.modifiers.shift);
        // Cmd+P → Quick Open
        let open_quick_open = ctx.input(|i| i.key_pressed(egui::Key::P) && i.modifiers.command && !i.modifiers.shift);

        if new_tab { self.new_tab(); }
        if close_tab && !self.tabs.is_empty() { self.request_close_tab(self.active_tab); }
        if toggle_explorer { self.show_explorer = !self.show_explorer; }
        if toggle_switcher { self.show_switcher = !self.show_switcher; }
        if open_quick_open {
            let cwd = self.tabs.get(self.active_tab)
                .map(|t| t.cwd.clone())
                .unwrap_or_else(|| ".".to_string());
            self.quick_open.open(&cwd);
        }
        if next_tab && self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + 1) % self.tabs.len();
        }
        if prev_tab && self.tabs.len() > 1 {
            self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
        }

        // Option+A-Z (alt modifier) → switch to tab by hotkey index
        // Option+A = first tab, Option+B = second tab, etc.
        let alpha_keys = [
            egui::Key::A, egui::Key::B, egui::Key::C, egui::Key::D,
            egui::Key::E, egui::Key::F, egui::Key::G, egui::Key::H,
            egui::Key::I, egui::Key::J, egui::Key::K, egui::Key::L,
            egui::Key::M, egui::Key::N, egui::Key::O, egui::Key::P,
            egui::Key::Q, egui::Key::R, egui::Key::S, egui::Key::T,
            egui::Key::U, egui::Key::V, egui::Key::W, egui::Key::X,
            egui::Key::Y, egui::Key::Z,
        ];
        for (key_idx, &key) in alpha_keys.iter().enumerate() {
            // alt = Option key on macOS; no command/shift to avoid conflicts
            let pressed = ctx.input(|i| {
                i.key_pressed(key) && i.modifiers.alt && !i.modifiers.command && !i.modifiers.shift
            });
            if pressed && key_idx < self.tabs.len() {
                self.active_tab = key_idx;
            }
        }

        // ─── Menu bar ───
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Terminal  ⌘T").clicked() { self.new_tab(); ui.close_menu(); }
                    if ui.button("Close Tab  ⌘W").clicked() {
                        if !self.tabs.is_empty() { self.request_close_tab(self.active_tab); }
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    ui.label("Copy  ⌘C");
                    ui.label("Paste  ⌘V");
                });
                ui.menu_button("View", |ui| {
                    let explorer_label = if self.show_explorer { "Hide Explorer  ⌘\\" } else { "Show Explorer  ⌘\\" };
                    if ui.button(explorer_label).clicked() { self.show_explorer = !self.show_explorer; ui.close_menu(); }
                    if ui.button("🐔 Agent Farm").clicked() { self.show_farm = !self.show_farm; ui.close_menu(); }
                    if ui.button("Tab Switcher  F1").clicked() { self.show_switcher = true; ui.close_menu(); }
                    ui.separator();
                    if ui.button("Quick Open  ⌘P").clicked() {
                        let cwd = self.tabs.get(self.active_tab)
                            .map(|t| t.cwd.clone())
                            .unwrap_or_else(|| ".".to_string());
                        self.quick_open.open(&cwd);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Tab", |ui| {
                    if ui.button("Next Tab  ⌘Tab").clicked() {
                        if self.tabs.len() > 1 { self.active_tab = (self.active_tab + 1) % self.tabs.len(); }
                        ui.close_menu();
                    }
                    ui.separator();
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let label = format!("{}  {} {}", tab.hotkey, tab.title, if i == self.active_tab { "◀" } else { "" });
                        if ui.button(label).clicked() { self.active_tab = i; ui.close_menu(); }
                    }
                });
            });
        });

        // ─── Tab bar with styled tabs ───
        egui::TopBottomPanel::top("tabs").min_height(32.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let tab_count = self.tabs.len();
                let active = self.active_tab;
                let mut clicked_tab: Option<usize> = None;
                let mut close_tab: Option<usize> = None;

                for i in 0..tab_count {
                    let is_active = i == active;
                    let hotkey = self.tabs[i].hotkey.clone();
                    let title = self.tabs[i].title.clone();
                    let bg = if is_active { egui::Color32::from_rgb(10, 14, 20) } else { egui::Color32::TRANSPARENT };
                    let fg = if is_active { egui::Color32::from_rgb(230, 230, 230) } else { egui::Color32::from_rgb(139, 148, 158) };

                    let frame = egui::Frame::NONE
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(10, 4))
                        .rounding(egui::Rounding { nw: 6, ne: 6, sw: 0, se: 0 });

                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&hotkey).small().color(
                                if is_active { egui::Color32::from_rgb(88, 166, 255) } else { egui::Color32::from_rgb(72, 79, 88) }
                            ).monospace());
                            if ui.selectable_label(false, egui::RichText::new(&title).color(fg)).clicked() {
                                clicked_tab = Some(i);
                            }
                            if ui.small_button("×").clicked() {
                                close_tab = Some(i);
                            }
                        });
                    });
                }

                if let Some(i) = clicked_tab { self.active_tab = i; }
                if let Some(i) = close_tab { self.request_close_tab(i); }

                // New tab button
                if ui.button(egui::RichText::new("+").size(16.0)).clicked() { self.new_tab(); }
                // Claude quick-launch
                if ui.button(egui::RichText::new("🤖").size(14.0)).on_hover_text("New Claude Session").clicked() {
                    let hotkey = (b'A' + self.tabs.len() as u8) as char;
                    let mut tab = TerminalTab::new(&hotkey.to_string());
                    tab.launch_claude();
                    self.tabs.push(tab);
                    self.active_tab = self.tabs.len() - 1;
                }
                // Farm button
                if ui.button("🐔").clicked() { self.show_farm = !self.show_farm; }
            });
        });

        // ─── Status bar ───
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Explorer toggle
                let explorer_icon = if self.show_explorer { "📁" } else { "📁" };
                if ui.small_button(explorer_icon).clicked() { self.show_explorer = !self.show_explorer; }
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    ui.label(egui::RichText::new(&tab.cwd).small().color(egui::Color32::from_rgb(139, 148, 158)));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("{} tabs", self.tabs.len())).small().color(egui::Color32::from_rgb(72, 79, 88)));
                });
            });
        });

        // ─── Explorer sidebar ───
        if self.show_explorer {
            egui::SidePanel::left("explorer").default_width(240.0).min_width(180.0).show(ctx, |ui| {
                self.explorer.ui(ui);
            });
        }

        // ─── Explorer → Terminal path paste ───
        if let Some(path) = self.explorer.pending_path.take() {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.write_pty_public(path.as_bytes());
            }
        }

        // ─── Quick Open → Terminal path paste ───
        if let Some(path) = self.quick_open.ui(ctx) {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.write_pty_public(path.as_bytes());
            }
        }

        // ─── Terminal ───
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.ui(ui, ctx);
            } else {
                ui.centered_and_justified(|ui| {
                    if ui.button("New Terminal").clicked() { self.new_tab(); }
                });
            }
        });

        // ─── Farm window ───
        if self.show_farm {
            egui::Window::new("🐔 Agent Farm")
                .default_size([400.0, 260.0])
                .resizable(true)
                .collapsible(true)
                .show(ctx, |ui| { self.farm.ui(ui); });
        }

        // ─── Tab Switcher (F1) ───
        if self.show_switcher {
            egui::Window::new("Switch Tab")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Press letter to switch, Esc to close");
                    ui.separator();
                    for (i, tab) in self.tabs.iter().enumerate() {
                        let label = format!("[{}] {} {}", tab.hotkey, tab.title,
                            if i == self.active_tab { "◀" } else { "" });
                        if ui.button(&label).clicked() {
                            self.active_tab = i;
                            self.show_switcher = false;
                        }
                    }
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.show_switcher = false;
                    }
                    // Letter press to switch
                    for (i, tab) in self.tabs.iter().enumerate() {
                        if let Some(ch) = tab.hotkey.chars().next() {
                            let key_idx = (ch as u8).wrapping_sub(b'A');
                            let keys = [egui::Key::A,egui::Key::B,egui::Key::C,egui::Key::D,egui::Key::E,
                                egui::Key::F,egui::Key::G,egui::Key::H,egui::Key::I,egui::Key::J,
                                egui::Key::K,egui::Key::L,egui::Key::M,egui::Key::N,egui::Key::O,
                                egui::Key::P,egui::Key::Q,egui::Key::R,egui::Key::S,egui::Key::T,
                                egui::Key::U,egui::Key::V,egui::Key::W,egui::Key::X,egui::Key::Y,egui::Key::Z];
                            if (key_idx as usize) < keys.len() {
                                if ctx.input(|inp| inp.key_pressed(keys[key_idx as usize]) && !inp.modifiers.command) {
                                    self.active_tab = i;
                                    self.show_switcher = false;
                                }
                            }
                        }
                    }
                });
        }

        // ─── Close confirmation dialog ───
        if self.close_confirm_tab.is_some() {
            egui::Window::new("Close Terminal?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    let idx = self.close_confirm_tab.unwrap();
                    let title = self.tabs.get(idx).map(|t| t.title.clone()).unwrap_or_default();
                    ui.label(format!("Close \"{}\"? This session will be terminated.", title));
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() { self.close_confirm_tab = None; }
                        if ui.button(egui::RichText::new("Close").color(egui::Color32::from_rgb(248, 81, 73))).clicked() {
                            self.confirm_close_tab();
                        }
                    });
                    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { self.close_confirm_tab = None; }
                    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) { self.confirm_close_tab(); }
                });
        }
    }
}
