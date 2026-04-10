use stealthterm_utils::CommandHistory;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;

/// Manages inline history-based completion suggestions
pub struct CompletionEngine {
    history: CommandHistory,
    matcher: SkimMatcherV2,
}

impl CompletionEngine {
    pub fn new(history: CommandHistory) -> Self {
        Self {
            history,
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn add_history(&mut self, cmd: &str) {
        self.history.push(cmd);
    }

    /// Returns the portion of the suggestion AFTER the current input
    pub fn inline_suggestion(&self, current_input: &str) -> Option<String> {
        let suggestion = self.history.suggest(current_input)?;
        Some(suggestion[current_input.len()..].to_string())
    }

    /// Prefix-match search; returns commands that start with the query, most recent first
    pub fn prefix_search(&self, query: &str, limit: usize) -> Vec<String> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<String> = Vec::new();
        // Iterate in reverse (most recent first), deduplicate
        for cmd in self.history.entries().iter().rev() {
            if cmd.starts_with(query) && cmd != query && !results.contains(cmd) {
                results.push(cmd.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }
        results
    }

    /// Fuzzy-match search; returns the best-matching command list
    pub fn fuzzy_search(&self, query: &str, limit: usize) -> Vec<String> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(i64, String)> = self.history
            .entries()
            .iter()
            .filter_map(|cmd| {
                self.matcher.fuzzy_match(cmd, query)
                    .map(|score| (score, cmd.clone()))
            })
            .collect();

        matches.sort_by(|a, b| b.0.cmp(&a.0));
        matches.into_iter()
            .take(limit)
            .map(|(_, cmd)| cmd)
            .collect()
    }

    pub fn history(&self) -> &CommandHistory {
        &self.history
    }

    pub fn history_mut(&mut self) -> &mut CommandHistory {
        &mut self.history
    }
}
