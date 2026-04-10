use regex::Regex;

#[derive(Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub results: Vec<SearchResult>,
    pub current_result: usize,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub buffer_row: usize,
    pub col_start: usize,
    pub col_end: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn compile_regex(&self) -> Option<Regex> {
        if self.query.is_empty() {
            return None;
        }
        let pattern = if self.use_regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        let pattern = if self.case_sensitive {
            pattern
        } else {
            format!("(?i){}", pattern)
        };
        Regex::new(&pattern).ok()
    }

    pub fn next_result(&mut self) {
        if !self.results.is_empty() {
            self.current_result = (self.current_result + 1) % self.results.len();
        }
    }

    pub fn prev_result(&mut self) {
        if !self.results.is_empty() {
            self.current_result = self.current_result
                .checked_sub(1)
                .unwrap_or(self.results.len() - 1);
        }
    }

    pub fn current(&self) -> Option<&SearchResult> {
        self.results.get(self.current_result)
    }
}
