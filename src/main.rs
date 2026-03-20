mod terminal;
mod farm;

use eframe::egui;
use terminal::TerminalTab;
use farm::AgentFarm;

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
    show_farm: bool,
}

impl App {
    fn new() -> Self {
        let mut tabs = Vec::new();
        tabs.push(TerminalTab::new("A"));
        Self { tabs, active_tab: 0, farm: AgentFarm::new(), show_farm: false }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // Menu bar
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Terminal").clicked() {
                        let h = (b'A' + self.tabs.len() as u8) as char;
                        self.tabs.push(TerminalTab::new(&h.to_string()));
                        self.active_tab = self.tabs.len() - 1;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("🐔 Agent Farm").clicked() {
                        self.show_farm = !self.show_farm;
                        ui.close_menu();
                    }
                });
            });
        });

        // Tab bar
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, tab) in self.tabs.iter().enumerate() {
                    if ui.selectable_label(i == self.active_tab, format!("{} {}", tab.hotkey, tab.title)).clicked() {
                        self.active_tab = i;
                    }
                }
                if ui.button("+").clicked() {
                    let h = (b'A' + self.tabs.len() as u8) as char;
                    self.tabs.push(TerminalTab::new(&h.to_string()));
                    self.active_tab = self.tabs.len() - 1;
                }
                if ui.button("🐔").clicked() { self.show_farm = !self.show_farm; }
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(tab) = self.tabs.get(self.active_tab) {
                    ui.label(&tab.cwd);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} tabs", self.tabs.len()));
                });
            });
        });

        // Terminal
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.ui(ui, ctx);
            }
        });

        // Farm window
        if self.show_farm {
            egui::Window::new("🐔 Agent Farm")
                .default_size([400.0, 260.0])
                .resizable(true)
                .show(ctx, |ui| { self.farm.ui(ui); });
        }
    }
}
