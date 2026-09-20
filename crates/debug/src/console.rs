//! In-app log console: bounded line history fed by the live fly-to
//! event feed (v0.3.2 — the fake transition-event seed data is deleted;
//! pilot actions push lines through [`LogConsole::push`]).

/// Bounded console history (oldest lines age out).
pub const CONSOLE_CAPACITY: usize = 200;

/// Log console state: the fly-to event feed plus free lines.
pub struct LogConsole {
    lines: Vec<String>,
}

impl LogConsole {
    pub fn new() -> Self {
        LogConsole { lines: Vec::new() }
    }

    /// Push one line, aging out the oldest past capacity.
    pub fn push(&mut self, line: String) {
        if self.lines.len() >= CONSOLE_CAPACITY {
            self.lines.remove(0);
        }
        self.lines.push(line);
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

impl Default for LogConsole {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_ages_out_past_capacity() {
        let mut console = LogConsole::new();
        assert!(console.is_empty());
        for i in 0..CONSOLE_CAPACITY + 10 {
            console.push(format!("line {i}"));
        }
        assert_eq!(console.len(), CONSOLE_CAPACITY);
        assert!(console.lines()[0].starts_with("line 10"));
        assert!(!console.is_empty());
    }
}
