use eframe::egui;
use std::fs;
use std::path::PathBuf;

struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    is_hidden: bool,
}

struct TreeNode {
    entry: DirEntry,
    expanded: bool,
    children: Option<Vec<TreeNode>>,
}

pub struct FileExplorer {
    root: PathBuf,
    tree: Vec<TreeNode>,
    show_hidden: bool,
    /// Path to paste into terminal (set on click, consumed by App)
    pub pending_path: Option<String>,
}

impl FileExplorer {
    pub fn new() -> Self {
        let root = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let tree = load_dir(&root);
        Self { root, tree, show_hidden: false, pending_path: None }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // Header
        ui.horizontal(|ui| {
            let name = self.root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string());
            ui.strong(egui::RichText::new(name.to_uppercase()).small().color(egui::Color32::from_rgb(139, 148, 158)));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("⊙").on_hover_text("Toggle hidden files").clicked() {
                    self.show_hidden = !self.show_hidden;
                }
                if ui.small_button("↑").on_hover_text("Parent directory").clicked() {
                    if let Some(parent) = self.root.parent() {
                        self.root = parent.to_path_buf();
                        self.tree = load_dir(&self.root);
                    }
                }
            });
        });
        ui.separator();

        // Tree
        egui::ScrollArea::vertical().show(ui, |ui| {
            let show_hidden = self.show_hidden;
            let mut navigate_to: Option<PathBuf> = None;
            let mut paste_path: Option<String> = None;

            for node in &mut self.tree {
                if !show_hidden && node.entry.is_hidden { continue; }
                Self::render_node(ui, node, 0, show_hidden, &mut navigate_to, &mut paste_path);
            }

            if paste_path.is_some() {
                self.pending_path = paste_path;
            }

            if let Some(path) = navigate_to {
                self.root = path;
                self.tree = load_dir(&self.root);
            }
        });

        // Footer
        ui.separator();
        ui.label(egui::RichText::new(self.root.to_string_lossy().to_string()).small().color(egui::Color32::from_rgb(72, 79, 88)));
    }

    fn render_node(ui: &mut egui::Ui, node: &mut TreeNode, depth: usize, show_hidden: bool, navigate_to: &mut Option<PathBuf>, paste_path: &mut Option<String>) {
        let indent = depth as f32 * 16.0 + 4.0;

        ui.horizontal(|ui| {
            ui.add_space(indent);

            let path_str = node.entry.path.to_string_lossy().to_string();
            let item_id = egui::Id::new(&path_str);

            if node.entry.is_dir {
                let icon = if node.expanded { "▾" } else { "▸" };
                if ui.small_button(icon).clicked() {
                    node.expanded = !node.expanded;
                    if node.expanded && node.children.is_none() {
                        node.children = Some(load_dir(&node.entry.path));
                    }
                }
                let color = egui::Color32::from_rgb(230, 230, 230);
                // Draggable folder
                let resp = ui.dnd_drag_source(item_id, path_str.clone(), |ui| {
                    ui.label(egui::RichText::new(format!("📁 {}", node.entry.name)).color(color));
                }).response;
                if resp.clicked() {
                    node.expanded = !node.expanded;
                    if node.expanded && node.children.is_none() {
                        node.children = Some(load_dir(&node.entry.path));
                    }
                }
                resp.on_hover_text(&path_str);
            } else {
                ui.add_space(18.0);
                let color = if node.entry.is_hidden {
                    egui::Color32::from_rgb(72, 79, 88)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                };
                let file_ic = file_icon(&node.entry.name);
                // Draggable file
                let resp = ui.dnd_drag_source(item_id, path_str.clone(), |ui| {
                    ui.label(egui::RichText::new(format!("{} {}", file_ic, node.entry.name)).color(color));
                }).response;
                resp.on_hover_text(&path_str);
            }
        });

        // Render children
        if node.expanded {
            if let Some(ref mut children) = node.children {
                for child in children.iter_mut() {
                    if !show_hidden && child.entry.is_hidden { continue; }
                    Self::render_node(ui, child, depth + 1, show_hidden, navigate_to, paste_path);
                }
            }
        }
    }
}

fn load_dir(path: &PathBuf) -> Vec<TreeNode> {
    let Ok(entries) = fs::read_dir(path) else { return vec![] };
    let mut nodes: Vec<TreeNode> = entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let is_hidden = name.starts_with('.');
            TreeNode {
                entry: DirEntry { name, path: e.path(), is_dir, is_hidden },
                expanded: false,
                children: None,
            }
        })
        .collect();

    // Sort: dirs first, then alphabetical
    nodes.sort_by(|a, b| {
        b.entry.is_dir.cmp(&a.entry.is_dir)
            .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
    });
    nodes
}

fn file_icon(name: &str) -> &'static str {
    if let Some(ext) = name.rsplit('.').next() {
        match ext {
            "rs" => "🦀",
            "ts" | "tsx" => "🟦",
            "js" | "jsx" => "🟨",
            "md" => "📝",
            "json" => "📋",
            "toml" | "yaml" | "yml" => "⚙️",
            "css" => "🎨",
            "html" => "🌐",
            "py" => "🐍",
            "sh" | "bash" | "zsh" => "📜",
            "lock" => "🔒",
            _ => "📄",
        }
    } else {
        "📄"
    }
}
