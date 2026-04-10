use std::collections::HashSet;

/// A command block: the command line + its output range
#[derive(Debug, Clone)]
pub struct CommandBlock {
    /// Absolute row number of the command line (the line where the user pressed Enter)
    pub prompt_line: usize,
    /// Output start row (prompt_line + 1)
    pub output_start: usize,
    /// Output end row (inclusive)
    pub output_end: usize,
}

/// Fold render info passed to TerminalView
#[derive(Debug, Clone)]
pub struct FoldRenderInfo {
    pub blocks: Vec<CommandBlock>,
    pub hidden_rows: HashSet<usize>,
    pub collapsed: HashSet<usize>,
}

/// Manages the fold state of command output
pub struct CommandFoldManager {
    /// Set of collapsed prompt_lines
    collapsed: HashSet<usize>,
}

impl CommandFoldManager {
    pub fn new() -> Self {
        Self {
            collapsed: HashSet::new(),
        }
    }

    /// Build command block list from emulator's command_line_rows.
    /// cursor_abs_row: absolute row of the cursor, used to bound the last command's output range.
    pub fn build_blocks(command_line_rows: &[usize], cursor_abs_row: usize) -> Vec<CommandBlock> {
        let mut blocks = Vec::new();
        for (i, &prompt) in command_line_rows.iter().enumerate() {
            let output_start = prompt + 1;
            let output_end = if i + 1 < command_line_rows.len() {
                // Row before the next command
                command_line_rows[i + 1].saturating_sub(1)
            } else {
                // Last command: up to the row before the cursor
                cursor_abs_row.saturating_sub(1)
            };
            // Only create a block if there is at least one output line
            if output_start <= output_end {
                blocks.push(CommandBlock {
                    prompt_line: prompt,
                    output_start,
                    output_end,
                });
            }
        }
        blocks
    }

    /// Toggle the fold state of a command block
    pub fn toggle(&mut self, prompt_line: usize) {
        if !self.collapsed.remove(&prompt_line) {
            self.collapsed.insert(prompt_line);
        }
    }

    /// Whether the block is collapsed
    pub fn is_collapsed(&self, prompt_line: usize) -> bool {
        self.collapsed.contains(&prompt_line)
    }

    /// Collapse the specified command
    pub fn collapse(&mut self, prompt_line: usize) {
        self.collapsed.insert(prompt_line);
    }

    /// Expand the specified command
    pub fn expand(&mut self, prompt_line: usize) {
        self.collapsed.remove(&prompt_line);
    }

    /// Collapse all commands
    pub fn collapse_all(&mut self, blocks: &[CommandBlock]) {
        for block in blocks {
            self.collapsed.insert(block.prompt_line);
        }
    }

    /// Expand all commands
    pub fn expand_all(&mut self) {
        self.collapsed.clear();
    }

    /// Return a reference to the collapsed set
    pub fn collapsed_set(&self) -> &HashSet<usize> {
        &self.collapsed
    }

    /// Compute all hidden row numbers (output rows that are collapsed)
    pub fn hidden_rows(&self, blocks: &[CommandBlock]) -> HashSet<usize> {
        let mut hidden = HashSet::new();
        for block in blocks {
            if self.collapsed.contains(&block.prompt_line) {
                for row in block.output_start..=block.output_end {
                    hidden.insert(row);
                }
            }
        }
        hidden
    }

    /// Build the complete FoldRenderInfo
    pub fn build_render_info(&self, command_line_rows: &[usize], cursor_abs_row: usize) -> FoldRenderInfo {
        let blocks = Self::build_blocks(command_line_rows, cursor_abs_row);
        let hidden_rows = self.hidden_rows(&blocks);
        let collapsed = self.collapsed.clone();
        FoldRenderInfo {
            blocks,
            hidden_rows,
            collapsed,
        }
    }
}

impl Default for CommandFoldManager {
    fn default() -> Self {
        Self::new()
    }
}
