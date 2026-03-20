/// Session tracking for Claude Code sessions.
///
/// The `SessionManager` watches raw PTY output lines and extracts:
/// - Whether a Claude Code session is active ("Claude Code" in output)
/// - Sub-agent count ("Running N agents" pattern)
/// - Token usage ("↓ N tokens" from status bar output)
///
/// It provides a list of `SessionInfo` structs suitable for driving
/// the `AgentFarm` UI.

// ─── Public data types ─────────────────────────────────────────────────────────

/// A snapshot of one Claude session's state, used by the UI layer.
#[derive(Clone, Debug)]
pub struct SessionInfo {
    /// Stable display label derived from the tab hotkey (e.g. "A").
    pub label: String,
    /// Number of detected sub-agents (0 = no agents spawned yet).
    pub sub_agent_count: usize,
    /// Cumulative tokens observed (input + output), 0 if unknown.
    pub tokens: u64,
    /// Approximate cost string, e.g. "$0.12".
    pub cost: String,
    /// Whether Claude Code has been detected as running in this session.
    pub is_active: bool,
}

impl SessionInfo {
    /// Format tokens as a dollar-cost string using a rough estimate.
    /// Claude 3.5 Sonnet: ~$3 / 1M input tokens, ~$15 / 1M output tokens.
    /// We use a blended ~$6 / 1M tokens as a conservative heuristic.
    fn format_cost(tokens: u64) -> String {
        let cost = tokens as f64 * 6.0 / 1_000_000.0;
        format!("${:.2}", cost)
    }
}

// ─── Internal session record ──────────────────────────────────────────────────

#[derive(Debug)]
struct SessionRecord {
    label: String,
    is_active: bool,
    sub_agent_count: usize,
    tokens: u64,
}

impl SessionRecord {
    fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            is_active: false,
            sub_agent_count: 0,
            tokens: 0,
        }
    }

    fn to_info(&self) -> SessionInfo {
        SessionInfo {
            label: self.label.clone(),
            sub_agent_count: self.sub_agent_count,
            tokens: self.tokens,
            cost: SessionInfo::format_cost(self.tokens),
            is_active: self.is_active,
        }
    }
}

// ─── SessionManager ────────────────────────────────────────────────────────────

/// Tracks one `SessionRecord` per terminal tab (keyed by tab label).
pub struct SessionManager {
    sessions: Vec<SessionRecord>,
}

impl SessionManager {
    /// Create a new, empty session manager.
    pub fn new() -> Self {
        Self { sessions: Vec::new() }
    }

    /// Ensure a session record exists for the given tab label.
    /// Call this whenever a new terminal tab is opened.
    pub fn register_tab(&mut self, label: &str) {
        if !self.sessions.iter().any(|s| s.label == label) {
            self.sessions.push(SessionRecord::new(label));
        }
    }

    /// Remove the session record for a closed tab.
    pub fn unregister_tab(&mut self, label: &str) {
        self.sessions.retain(|s| s.label != label);
    }

    /// Feed a chunk of raw PTY output for the given tab label.
    ///
    /// The manager scans lines for known Claude Code patterns and updates
    /// the corresponding session record.
    pub fn process_output(&mut self, label: &str, data: &[u8]) {
        let text = String::from_utf8_lossy(data);

        // Ensure session exists (lazy registration)
        if !self.sessions.iter().any(|s| s.label == label) {
            self.sessions.push(SessionRecord::new(label));
        }

        let record = self.sessions
            .iter_mut()
            .find(|s| s.label == label)
            .expect("session must exist after lazy registration");

        for line in text.lines() {
            // Strip ANSI escape sequences for pattern matching
            let clean = strip_ansi(line);

            // Detect Claude Code session start
            if !record.is_active
                && (clean.contains("Claude Code")
                    || clean.contains("claude-code")
                    || clean.contains("╭─") // Claude's TUI border
                    || clean.contains("Human:"))
            {
                record.is_active = true;
            }

            // Detect sub-agent count: "Running N agents" or "N agents running"
            if let Some(n) = extract_agent_count(&clean) {
                record.sub_agent_count = n;
            }

            // Detect token usage from status line: "↓ N tokens" or "N tokens"
            if let Some(t) = extract_token_count(&clean) {
                // Accumulate only upward moves (status line may reset)
                if t > record.tokens {
                    record.tokens = t;
                }
            }
        }
    }

    /// Return a snapshot of all sessions.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.sessions.iter().map(|r| r.to_info()).collect()
    }

    /// Return info for a single tab label, or `None` if not found.
    pub fn get(&self, label: &str) -> Option<SessionInfo> {
        self.sessions.iter().find(|s| s.label == label).map(|r| r.to_info())
    }

    /// Reset a session back to inactive (e.g. when Claude exits).
    pub fn reset(&mut self, label: &str) {
        if let Some(r) = self.sessions.iter_mut().find(|s| s.label == label) {
            r.is_active = false;
            r.sub_agent_count = 0;
            // Keep tokens accumulated for history
        }
    }
}

// ─── Pattern extraction helpers ───────────────────────────────────────────────

/// Extract sub-agent count from patterns like:
/// - "Running 3 agents"
/// - "3 agents running"
/// - "Spawning agent 2 of 5" (returns 5 as total)
fn extract_agent_count(line: &str) -> Option<usize> {
    // "Running N agents"
    if let Some(pos) = find_sequence(line, "Running ") {
        let rest = &line[pos + 8..];
        if let Some(n) = parse_leading_number(rest) {
            if rest[digit_len(rest)..].trim_start().starts_with("agent") {
                return Some(n);
            }
        }
    }
    // "N agents running" or "N agents"
    if let Some(n) = parse_leading_number(line) {
        let after = &line[digit_len(line)..].trim_start();
        if after.starts_with("agent") {
            return Some(n);
        }
    }
    // "Spawning agent N of M" → return M
    if let Some(pos) = find_sequence(line, " of ") {
        let rest = &line[pos + 4..];
        if let Some(n) = parse_leading_number(rest) {
            return Some(n);
        }
    }
    None
}

/// Extract token count from patterns like:
/// - "↓ 12345 tokens"
/// - "12,345 tokens"
/// - "Tokens: 12345"
fn extract_token_count(line: &str) -> Option<u64> {
    // "↓ N tokens" (Claude status bar uses ↓ for input tokens)
    let search_strs: &[&str] = &["↓ ", "↑ ", "Tokens: ", "tokens: "];
    for prefix in search_strs {
        if let Some(pos) = find_sequence(line, prefix) {
            let rest = &line[pos + prefix.len()..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == ',').collect();
            let digits: String = digits.chars().filter(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                if let Ok(n) = digits.parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    // Plain "N tokens" anywhere
    if let Some(pos) = find_sequence(line, " tokens") {
        // Walk back from pos to find the number
        let before = &line[..pos];
        let rev_digits: String = before.chars().rev()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .collect();
        if !rev_digits.is_empty() {
            let digits: String = rev_digits.chars().filter(|c| c.is_ascii_digit()).collect();
            let s: String = digits.chars().rev().collect();
            if let Ok(n) = s.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

// ─── String utilities ─────────────────────────────────────────────────────────

/// Remove ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until end of escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Consume until a letter
                while let Some(&nc) = chars.peek() {
                    chars.next();
                    if nc.is_ascii_alphabetic() { break; }
                }
            } else {
                // Other escape: skip one more char
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Find a literal substring, returning the byte offset.
fn find_sequence(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

/// Parse a leading decimal number from a string slice.
fn parse_leading_number(s: &str) -> Option<usize> {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { None } else { digits.parse().ok() }
}

/// Number of leading ASCII digit chars.
fn digit_len(s: &str) -> usize {
    s.chars().take_while(|c| c.is_ascii_digit()).count()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        let input = "\x1b[32mHello\x1b[0m World";
        assert_eq!(strip_ansi(input), "Hello World");
    }

    #[test]
    fn test_extract_agent_count_running() {
        assert_eq!(extract_agent_count("Running 3 agents"), Some(3));
        assert_eq!(extract_agent_count("Running 1 agent concurrently"), Some(1));
    }

    #[test]
    fn test_extract_agent_count_n_agents() {
        assert_eq!(extract_agent_count("5 agents running"), Some(5));
    }

    #[test]
    fn test_extract_token_count_arrow() {
        assert_eq!(extract_token_count("↓ 12345 tokens used"), Some(12345));
    }

    #[test]
    fn test_extract_token_count_plain() {
        assert_eq!(extract_token_count("Used 4096 tokens"), Some(4096));
    }

    #[test]
    fn test_session_manager_detect_claude() {
        let mut mgr = SessionManager::new();
        mgr.register_tab("A");
        assert!(!mgr.get("A").unwrap().is_active);
        mgr.process_output("A", b"Welcome to Claude Code\n");
        assert!(mgr.get("A").unwrap().is_active);
    }

    #[test]
    fn test_session_manager_token_accumulation() {
        let mut mgr = SessionManager::new();
        mgr.register_tab("B");
        mgr.process_output("B", b"Claude Code started\n");
        mgr.process_output("B", b"\x1b[32m\xE2\x86\x93 500 tokens\x1b[0m\n");
        let info = mgr.get("B").unwrap();
        assert_eq!(info.tokens, 500);
        // A lower value should not overwrite
        mgr.process_output("B", b"\xE2\x86\x93 100 tokens\n");
        assert_eq!(mgr.get("B").unwrap().tokens, 500);
        // A higher value should
        mgr.process_output("B", b"\xE2\x86\x93 1200 tokens\n");
        assert_eq!(mgr.get("B").unwrap().tokens, 1200);
    }

    #[test]
    fn test_format_cost() {
        // 1 000 000 tokens ≈ $6.00
        assert_eq!(SessionInfo::format_cost(1_000_000), "$6.00");
        assert_eq!(SessionInfo::format_cost(0), "$0.00");
    }
}
