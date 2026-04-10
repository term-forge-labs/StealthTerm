use crate::grid::Row;
use std::collections::VecDeque;

/// Scrollback buffer storing lines that have scrolled off the top of the visible grid
pub struct ScrollbackBuffer {
    lines: VecDeque<Row>,
    max_lines: usize,
}

impl ScrollbackBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines,
        }
    }

    pub fn push(&mut self, row: Row) {
        self.lines.push_back(row);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    pub fn push_many(&mut self, rows: impl IntoIterator<Item = Row>) {
        for row in rows {
            self.push(row);
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn get(&self, idx: usize) -> Option<&Row> {
        self.lines.get(idx)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Row> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Get last n lines (most recent)
    pub fn last_n(&self, n: usize) -> impl Iterator<Item = &Row> {
        let start = self.lines.len().saturating_sub(n);
        self.lines.range(start..)
    }

    /// Search for text in scrollback buffer, returns (line_idx, col) pairs
    pub fn search(&self, pattern: &regex::Regex) -> Vec<(usize, usize, usize)> {
        let mut results = Vec::new();
        for (line_idx, row) in self.lines.iter().enumerate() {
            let text = row.to_string_lossy();
            for m in pattern.find_iter(&text) {
                results.push((line_idx, m.start(), m.end()));
            }
        }
        results
    }
}
