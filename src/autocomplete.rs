use std::collections::BTreeSet;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Completion {
    pub name: String,
    pub description: String,
    pub kind: CompletionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompletionKind {
    Command,
    Flag,
    Option,
    Subcommand,
}

use std::sync::Mutex;

static HELP_CACHE: std::sync::LazyLock<Mutex<std::collections::HashMap<String, Vec<Completion>>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

static PATH_COMMANDS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
    let mut cmds = BTreeSet::new();
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.starts_with('.') { cmds.insert(name); }
                }
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        for hist in &[".zsh_history", ".bash_history"] {
            if let Ok(content) = std::fs::read_to_string(home.join(hist)) {
                for line in content.lines().rev().take(300) {
                    let cmd_part = if line.contains(';') {
                        line.splitn(2, ';').nth(1).unwrap_or(line)
                    } else { line };
                    if let Some(first) = cmd_part.trim().split_whitespace().next() {
                        if first.len() >= 2 && first.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                            cmds.insert(first.to_string());
                        }
                    }
                }
            }
        }
    }
    cmds.into_iter().collect()
});

/// Fetch and parse `command --help` output
fn fetch_help(command: &str) -> Vec<Completion> {
    let mut cache = HELP_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(command) {
        return cached.clone();
    }

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() { return vec![]; }

    let mut args: Vec<&str> = parts[1..].to_vec();
    args.push("--help");

    let output = Command::new(parts[0])
        .args(&args)
        .output();

    let text = match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if stdout.len() > stderr.len() { stdout } else { stderr }
        }
        Err(_) => { cache.insert(command.to_string(), vec![]); return vec![]; }
    };

    let mut completions = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Flags: -h, --help
        if trimmed.starts_with('-') {
            let parts: Vec<&str> = trimmed.splitn(2, "  ").collect();
            let flag_part = parts[0].trim();
            let desc = parts.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
            let flags: Vec<&str> = flag_part.split(',').map(|s| s.trim().split_whitespace().next().unwrap_or("")).collect();
            let best = flags.iter().find(|f| f.starts_with("--")).or(flags.first()).unwrap_or(&"");
            if !best.is_empty() && best.starts_with('-') {
                let kind = if flag_part.contains('<') || flag_part.contains('=') {
                    CompletionKind::Option
                } else {
                    CompletionKind::Flag
                };
                completions.push(Completion { name: best.to_string(), description: desc, kind });
            }
        }
        // Subcommands
        else if !trimmed.is_empty() && trimmed.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            let parts: Vec<&str> = trimmed.splitn(2, "  ").collect();
            if parts.len() >= 2 {
                let name = parts[0].trim();
                let desc = parts[1].trim();
                if !name.contains(' ') && name.len() >= 2 && name.len() <= 30 && !desc.is_empty() {
                    completions.push(Completion { name: name.to_string(), description: desc.to_string(), kind: CompletionKind::Subcommand });
                }
            }
        }
    }

    cache.insert(command.to_string(), completions.clone());
    completions
}

pub struct AutocompleteState {
    pub suggestions: Vec<Completion>,
    pub selected: usize,
    pub visible: bool,
    all_completions: Vec<Completion>,
}

impl AutocompleteState {
    pub fn new() -> Self {
        Self { suggestions: vec![], selected: 0, visible: false, all_completions: vec![] }
    }

    /// Update suggestions based on current input text
    pub fn update(&mut self, input: &str) {
        let trimmed = input.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        if parts.is_empty() || trimmed.is_empty() {
            self.visible = false;
            return;
        }

        if parts.len() == 1 && !input.ends_with(' ') {
            // Base command — filter from PATH
            let partial = parts[0].to_lowercase();
            if partial.len() < 2 { self.visible = false; return; }
            self.suggestions = PATH_COMMANDS.iter()
                .filter(|c| c.to_lowercase().starts_with(&partial) && c.to_lowercase() != partial)
                .take(10)
                .map(|c| Completion { name: c.clone(), description: String::new(), kind: CompletionKind::Command })
                .collect();
        } else {
            // After command — fetch help and filter
            if self.all_completions.is_empty() {
                let mut cmd = parts[0].to_string();
                if parts.len() >= 2 && !parts[1].starts_with('-') {
                    cmd = format!("{} {}", parts[0], parts[1]);
                }
                self.all_completions = fetch_help(&cmd);
                if self.all_completions.is_empty() && cmd.contains(' ') {
                    self.all_completions = fetch_help(parts[0]);
                }
            }

            let last_word = parts.last().unwrap_or(&"");
            if last_word.starts_with('-') {
                self.suggestions = self.all_completions.iter()
                    .filter(|c| (c.kind == CompletionKind::Flag || c.kind == CompletionKind::Option) &&
                        c.name.starts_with(last_word) && &c.name != last_word)
                    .take(10).cloned().collect();
            } else {
                self.suggestions = self.all_completions.iter()
                    .filter(|c| c.name.to_lowercase().starts_with(&last_word.to_lowercase()) &&
                        c.name.to_lowercase() != last_word.to_lowercase())
                    .take(10).cloned().collect();
            }
        }

        self.visible = !self.suggestions.is_empty();
        self.selected = 0;
    }

    /// Accept current suggestion, returns text to insert
    pub fn accept(&mut self, input: &str) -> Option<String> {
        if !self.visible || self.suggestions.is_empty() { return None; }
        let s = &self.suggestions[self.selected];
        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        let last_word = parts.last().unwrap_or(&"");
        let insert = format!("{} ", &s.name[last_word.len()..]);

        if s.kind == CompletionKind::Command { self.all_completions.clear(); }
        self.visible = false;
        Some(insert)
    }

    pub fn move_selection(&mut self, delta: i32) {
        if !self.visible || self.suggestions.is_empty() { return; }
        let len = self.suggestions.len() as i32;
        self.selected = ((self.selected as i32 + delta).rem_euclid(len)) as usize;
    }

    pub fn dismiss(&mut self) {
        self.visible = false;
    }

    pub fn reset(&mut self) {
        self.visible = false;
        self.all_completions.clear();
        self.suggestions.clear();
    }
}
