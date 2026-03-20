use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

/// Maximum depth to recurse into when scanning directories.
const MAX_DEPTH: usize = 5;
/// Maximum number of results to display.
const MAX_RESULTS: usize = 20;

/// Directories to skip during scanning.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".next",
    ".nuxt",
];

/// A single scanned filesystem entry.
#[derive(Clone)]
struct FileEntry {
    /// Display name (file/dir name only).
    name: String,
    /// Full absolute path.
    full_path: PathBuf,
    /// Whether this entry is a directory.
    is_dir: bool,
}

/// Match score used to rank results.  Lower is better.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum MatchScore {
    Exact = 0,
    Prefix = 1,
    Fuzzy = 2,
}

/// A filtered result ready for display.
#[derive(Clone)]
struct SearchResult {
    entry: FileEntry,
    score: MatchScore,
}

/// Shared scan state between the background scanning thread and the UI.
#[derive(Default)]
struct ScanState {
    entries: Vec<FileEntry>,
    done: bool,
}

/// Quick Open floating window – call `ui()` every frame.
///
/// Returns `Some(path_string)` when the user confirms a selection, `None`
/// otherwise.  The caller is responsible for pasting the returned path into
/// the active terminal.
pub struct QuickOpen {
    /// Whether the window is currently visible.
    pub is_open: bool,

    query: String,
    results: Vec<SearchResult>,
    selected: usize,

    /// Background scan state (populated by a worker thread).
    scan_state: Arc<Mutex<ScanState>>,
    /// The working directory from which the last scan was started.
    last_scanned_cwd: Option<PathBuf>,
    /// Set to `true` on the first frame after opening to focus the text field.
    request_focus: bool,
}

impl QuickOpen {
    /// Create a new (closed) Quick Open widget.
    pub fn new() -> Self {
        Self {
            is_open: false,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            scan_state: Arc::new(Mutex::new(ScanState::default())),
            last_scanned_cwd: None,
            request_focus: false,
        }
    }

    /// Open the Quick Open window, triggering a background scan of `cwd`.
    pub fn open(&mut self, cwd: &str) {
        let cwd_path = PathBuf::from(cwd);

        // Only re-scan when the working directory has changed.
        let needs_scan = self
            .last_scanned_cwd
            .as_deref()
            .map(|p| p != cwd_path)
            .unwrap_or(true);

        if needs_scan {
            self.last_scanned_cwd = Some(cwd_path.clone());
            self.start_scan(cwd_path);
        }

        self.is_open = true;
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        self.request_focus = true;
    }

    /// Spawn a background thread to recursively scan `root`.
    fn start_scan(&mut self, root: PathBuf) {
        // Reset existing scan state.
        {
            let mut state = self.scan_state.lock().unwrap();
            state.entries.clear();
            state.done = false;
        }

        let scan_state = Arc::clone(&self.scan_state);

        thread::spawn(move || {
            let mut entries = Vec::new();
            scan_dir(&root, &root, 0, &mut entries);

            let mut state = scan_state.lock().unwrap();
            state.entries = entries;
            state.done = true;
        });
    }

    /// Render the Quick Open window.
    ///
    /// Returns `Some(path)` when the user selects an entry, `None` otherwise.
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<String> {
        if !self.is_open {
            return None;
        }

        // Close on Esc (checked before showing the window so it fires even
        // when nothing inside has keyboard focus).
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.is_open = false;
            return None;
        }

        let mut result: Option<String> = None;

        // Rebuild results whenever the query changes or new scan data arrived.
        let query_snapshot = self.query.clone();
        {
            let state = self.scan_state.lock().unwrap();
            self.results = filter_results(&state.entries, &query_snapshot);
        }
        // Clamp selection index after results changed.
        if self.results.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.results.len() {
            self.selected = self.results.len() - 1;
        }

        // Navigate with arrow keys.
        let down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));

        if down && !self.results.is_empty() {
            self.selected = (self.selected + 1).min(self.results.len() - 1);
        }
        if up && self.selected > 0 {
            self.selected -= 1;
        }
        if enter {
            if let Some(res) = self.results.get(self.selected) {
                let path_str = res.entry.full_path.to_string_lossy().to_string();
                self.is_open = false;
                return Some(path_str);
            }
        }

        egui::Window::new("Quick Open")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -60.0])
            .fixed_size([520.0, 360.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgb(22, 27, 34))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 54, 61)))
                    .rounding(egui::Rounding::same(8)),
            )
            .show(ctx, |ui| {
                ui.style_mut().visuals.extreme_bg_color = egui::Color32::from_rgb(13, 17, 23);

                // ── Search bar ──────────────────────────────────────────────
                let search_frame = egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(13, 17, 23))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .rounding(egui::Rounding::same(6));

                search_frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("🔍")
                                .size(14.0)
                                .color(egui::Color32::from_rgb(139, 148, 158)),
                        );
                        let te = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Search files and directories…")
                            .desired_width(f32::INFINITY)
                            .frame(false)
                            .text_color(egui::Color32::from_rgb(230, 230, 230));

                        let response = ui.add(te);

                        // Focus on first open.
                        if self.request_focus {
                            response.request_focus();
                            self.request_focus = false;
                        }
                    });
                });

                ui.separator();

                // ── Results list ─────────────────────────────────────────────
                let scanning = {
                    let state = self.scan_state.lock().unwrap();
                    !state.done
                };

                if self.results.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        if scanning {
                            ui.label(
                                egui::RichText::new("Scanning…")
                                    .color(egui::Color32::from_rgb(139, 148, 158))
                                    .italics(),
                            );
                        } else if query_snapshot.is_empty() {
                            ui.label(
                                egui::RichText::new("Start typing to search")
                                    .color(egui::Color32::from_rgb(139, 148, 158)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("No results")
                                    .color(egui::Color32::from_rgb(139, 148, 158)),
                            );
                        }
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(280.0)
                        .show(ui, |ui| {
                            for (idx, res) in self.results.iter().enumerate() {
                                let is_selected = idx == self.selected;
                                let row_bg = if is_selected {
                                    egui::Color32::from_rgb(31, 111, 235)
                                } else {
                                    egui::Color32::TRANSPARENT
                                };

                                let row_frame = egui::Frame::NONE
                                    .fill(row_bg)
                                    .inner_margin(egui::Margin::symmetric(8, 3))
                                    .rounding(egui::Rounding::same(4));

                                let row_response = row_frame.show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        // Icon
                                        let icon = if res.entry.is_dir {
                                            "📁"
                                        } else {
                                            file_icon(&res.entry.name)
                                        };
                                        ui.label(egui::RichText::new(icon).size(13.0));

                                        // Name
                                        let name_color = if is_selected {
                                            egui::Color32::WHITE
                                        } else {
                                            egui::Color32::from_rgb(230, 230, 230)
                                        };
                                        ui.label(
                                            egui::RichText::new(&res.entry.name)
                                                .color(name_color)
                                                .monospace(),
                                        );

                                        // Path (dimmed)
                                        let path_str = res
                                            .entry
                                            .full_path
                                            .parent()
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_default();
                                        let path_color = if is_selected {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 160)
                                        } else {
                                            egui::Color32::from_rgb(72, 79, 88)
                                        };
                                        ui.label(
                                            egui::RichText::new(path_str)
                                                .color(path_color)
                                                .small()
                                                .monospace(),
                                        );
                                    });
                                });

                                if row_response.response.clicked() {
                                    let path_str =
                                        res.entry.full_path.to_string_lossy().to_string();
                                    result = Some(path_str);
                                    self.is_open = false;
                                }
                                if row_response.response.hovered() {
                                    self.selected = idx;
                                }
                            }
                        });
                }

                // ── Footer hint ──────────────────────────────────────────────
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("↑↓ navigate   Enter confirm   Esc close")
                            .small()
                            .color(egui::Color32::from_rgb(72, 79, 88)),
                    );
                });
            });

        result
    }
}

// ── Filesystem scanning ──────────────────────────────────────────────────────

/// Recursively scan `dir`, collecting entries up to `MAX_DEPTH` levels deep.
/// `root` is kept for future relative-path display (currently unused here).
fn scan_dir(root: &PathBuf, dir: &PathBuf, depth: usize, out: &mut Vec<FileEntry>) {
    if depth > MAX_DEPTH {
        return;
    }

    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };

    for entry in read.flatten() {
        let path = entry.path();
        let name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        // Skip hidden files/dirs at the scan level.
        if name.starts_with('.') {
            continue;
        }

        let is_dir = path.is_dir();

        if is_dir {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            out.push(FileEntry {
                name: name.clone(),
                full_path: path.clone(),
                is_dir: true,
            });
            // The `root` parameter is forwarded but the recursive call uses
            // `path` as the new `dir`.
            scan_dir(root, &path, depth + 1, out);
        } else {
            out.push(FileEntry {
                name,
                full_path: path,
                is_dir: false,
            });
        }
    }
}

// ── Filtering & scoring ──────────────────────────────────────────────────────

/// Filter and score `entries` against `query`, returning up to `MAX_RESULTS`
/// results sorted by score.
fn filter_results(entries: &[FileEntry], query: &str) -> Vec<SearchResult> {
    if query.is_empty() {
        // Show the first MAX_RESULTS entries (directories first) when there is
        // no query, so the user sees something immediately.
        let mut results: Vec<SearchResult> = entries
            .iter()
            .take(MAX_RESULTS * 4)
            .filter(|e| e.is_dir)
            .take(MAX_RESULTS)
            .map(|e| SearchResult {
                entry: e.clone(),
                score: MatchScore::Fuzzy,
            })
            .collect();
        if results.is_empty() {
            results = entries
                .iter()
                .take(MAX_RESULTS)
                .map(|e| SearchResult {
                    entry: e.clone(),
                    score: MatchScore::Fuzzy,
                })
                .collect();
        }
        return results;
    }

    let q_lower = query.to_lowercase();

    let mut results: Vec<SearchResult> = entries
        .iter()
        .filter_map(|e| {
            let name_lower = e.name.to_lowercase();
            let score = match_score(&name_lower, &q_lower)?;
            Some(SearchResult {
                entry: e.clone(),
                score,
            })
        })
        .collect();

    // Stable sort: score first, then directories before files for equal
    // scores, then alphabetical.
    results.sort_by(|a, b| {
        a.score
            .cmp(&b.score)
            .then_with(|| b.entry.is_dir.cmp(&a.entry.is_dir))
            .then_with(|| a.entry.name.cmp(&b.entry.name))
    });

    results.truncate(MAX_RESULTS);
    results
}

/// Return the `MatchScore` for `name` against `query`, or `None` if the
/// characters cannot be matched in order (fuzzy no-match).
fn match_score(name: &str, query: &str) -> Option<MatchScore> {
    // Exact match.
    if name == query {
        return Some(MatchScore::Exact);
    }
    // Prefix match.
    if name.starts_with(query) {
        return Some(MatchScore::Prefix);
    }
    // Fuzzy match: every character in `query` must appear in `name` in order.
    let mut name_chars = name.chars();
    for qc in query.chars() {
        if !name_chars.any(|nc| nc == qc) {
            return None;
        }
    }
    Some(MatchScore::Fuzzy)
}

// ── File icon mapping ────────────────────────────────────────────────────────

/// Return an emoji icon for a given file name based on its extension.
fn file_icon(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "rs" => "🦀",
        "toml" | "yaml" | "yml" | "json" | "json5" | "jsonc" => "⚙️",
        "ts" | "tsx" => "🔷",
        "js" | "jsx" | "mjs" | "cjs" => "🟨",
        "html" | "htm" => "🌐",
        "css" | "scss" | "sass" | "less" => "🎨",
        "md" | "mdx" | "markdown" => "📝",
        "sh" | "bash" | "zsh" | "fish" => "📜",
        "py" => "🐍",
        "go" => "🐹",
        "java" | "kt" | "kts" => "☕",
        "c" | "cc" | "cpp" | "cxx" | "h" | "hpp" => "⚡",
        "swift" => "🍎",
        "rb" => "💎",
        "php" => "🐘",
        "cs" => "🔵",
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => "🖼",
        "mp4" | "mov" | "avi" | "mkv" | "webm" => "🎬",
        "mp3" | "wav" | "flac" | "ogg" | "aac" => "🎵",
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "📦",
        "pdf" => "📄",
        "txt" | "log" => "📋",
        "lock" => "🔒",
        "env" | "envrc" => "🔐",
        "dockerfile" | "containerfile" => "🐳",
        _ => "📄",
    }
}
