use crate::buffer::ScrollbackBuffer;
use crate::cell::{Cell, CellAttributes, Color};
use crate::grid::{Grid, Row};
use crate::selection::Selection;

/// Cursor shape for rendering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CursorShape {
    Block,
    Underline,
    Bar,
}

impl Default for CursorShape {
    fn default() -> Self {
        CursorShape::Block
    }
}

/// Shell prompt state tracked via OSC 133 sequences
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromptState {
    /// Waiting for prompt (initial state, or after command end)
    Idle,
    /// Prompt is being drawn (after OSC 133;A)
    PromptShown,
    /// User is typing a command (after OSC 133;B)
    UserTyping,
    /// Command is executing (after OSC 133;C)
    CommandRunning,
}

/// The core terminal state machine
pub struct TerminalEmulator {
    pub grid: Grid,
    pub scrollback: ScrollbackBuffer,
    pub title: String,
    pub current_fg: Color,
    pub current_bg: Color,
    pub current_attrs: CellAttributes,
    pub cursor_visible: bool,
    pub cursor_shape: CursorShape,
    pub cursor_blink: bool,
    pub auto_wrap: bool,
    pub application_cursor_keys: bool,
    pub bracketed_paste: bool,
    pub selection: Option<Selection>,
    pub scroll_offset: usize,
    /// Alternate screen buffer (saved primary grid when switching to alt)
    alt_grid: Option<Grid>,
    alt_cursor: Option<(usize, usize)>,
    /// Records the absolute row number of the cursor when the user presses Enter (i.e. the command line position)
    pub command_line_rows: Vec<usize>,
    /// Whether an interactive child process is active (REPL, CLI tool with readline)
    pub interactive_child_active: bool,
    /// Set after mark_command_line(), cleared when interactive detection resolves
    command_pending: bool,
    /// Tracks whether DECRST 1 was seen after command_pending was set
    saw_decrst1_after_command: bool,
    /// Cursor absolute row when command was sent (to detect output between transitions)
    command_sent_cursor_abs: usize,
    /// Title saved when entering interactive mode, used to detect exit via title change
    interactive_saved_title: String,
    /// Whether OSC 133 shell integration is available (set on first OSC 133 received)
    pub osc133_available: bool,
    /// Current prompt state (tracked via OSC 133)
    pub prompt_state: PromptState,
    /// Finalized command rows (commands that completed, suitable for fold lines)
    pub finalized_command_rows: Vec<usize>,
    /// Row where the currently running command started (pending finalization)
    pending_command_row: Option<usize>,
    /// Persistent VTE parser — keeps state across process() calls so split escape sequences work
    vte_parser: vte::Parser,
}

impl TerminalEmulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            grid: Grid::new(cols, rows),
            scrollback: ScrollbackBuffer::new(100_000),
            title: String::new(),
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_attrs: CellAttributes::default(),
            cursor_visible: true,
            cursor_shape: CursorShape::Block,
            cursor_blink: true,
            auto_wrap: true,
            application_cursor_keys: false,
            bracketed_paste: false,
            selection: None,
            scroll_offset: 0,
            alt_grid: None,
            alt_cursor: None,
            command_line_rows: Vec::new(),
            interactive_child_active: false,
            command_pending: false,
            saw_decrst1_after_command: false,
            command_sent_cursor_abs: 0,
            interactive_saved_title: String::new(),
            osc133_available: false,
            prompt_state: PromptState::Idle,
            finalized_command_rows: Vec::new(),
            pending_command_row: None,
            vte_parser: vte::Parser::new(),
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let old_rows = self.grid.rows;
        if rows < old_rows {
        // When shrinking rows, push excess rows below the cursor into scrollback
        // Keep the cursor row and above visible
            let keep_rows = rows;
            let cursor_row = self.grid.cursor_row;

            // Number of rows to remove
            let excess = old_rows - keep_rows;
            // Push rows from the top into scrollback (keep cursor within new grid)
            let rows_to_push = if cursor_row >= keep_rows {
                // Cursor exceeds new grid, must push enough rows
                cursor_row - keep_rows + 1 + excess.saturating_sub(cursor_row - keep_rows + 1)
            } else {
                excess
            };

            for _ in 0..rows_to_push {
                if !self.grid.cells.is_empty() {
                    let row = self.grid.cells.remove(0);
                    self.scrollback.push(row);
                    if self.grid.cursor_row > 0 {
                        self.grid.cursor_row -= 1;
                    }
                }
            }
        }
        self.grid.resize(cols, rows);
    }

    pub fn process(&mut self, data: &[u8]) {
        // Take the parser out temporarily to avoid double mutable borrow
        let mut parser = std::mem::replace(&mut self.vte_parser, vte::Parser::new());
        for byte in data {
            parser.advance(self, *byte);
        }
        self.vte_parser = parser;
    }

    pub fn newline(&mut self) {
        if self.grid.cursor_row == self.grid.scroll_bottom {
            // Scroll up, push to scrollback
            let scrolled = self.grid.scroll_up(1);
            self.scrollback.push_many(scrolled);
        } else if self.grid.cursor_row + 1 < self.grid.rows {
            self.grid.cursor_row += 1;
        }
    }

    /// Returns true if the terminal is in alternate screen mode (e.g. vim, less, htop)
    pub fn is_alt_screen(&self) -> bool {
        self.alt_grid.is_some()
    }

    /// Returns true if in any interactive mode (alt-screen OR interactive child OR running command via OSC 133)
    pub fn is_interactive(&self) -> bool {
        let alt = self.alt_grid.is_some();
        let interactive = self.interactive_child_active;
        let osc_running = self.osc133_available && self.prompt_state == PromptState::CommandRunning;
        let result = alt || interactive || osc_running;
        if result {
            tracing::debug!("is_interactive=true: alt_screen={}, interactive_child={}, osc_running={} (osc133_available={}, prompt_state={:?})",
                alt, interactive, osc_running, self.osc133_available, self.prompt_state);
        }
        result
    }

    /// Called when application_cursor_keys (DECSET/DECRST 1) changes to detect interactive child transitions
    pub fn on_application_cursor_keys_changed(&mut self, enabled: bool) {
        if !self.interactive_child_active {
            if self.command_pending && !enabled {
                self.saw_decrst1_after_command = true;
            } else if self.saw_decrst1_after_command && enabled {
                // DECSET 1 after DECRST 1 — shell re-prompting after non-interactive command
                self.command_pending = false;
                self.saw_decrst1_after_command = false;
            }
        }
    }

    /// Switch to alternate screen buffer
    pub fn enter_alt_screen(&mut self) {
        if self.alt_grid.is_none() {
            tracing::info!(">>> ENTER ALT SCREEN: prompt_state={:?}, interactive_child={}, osc133_available={}",
                self.prompt_state, self.interactive_child_active, self.osc133_available);
            self.alt_grid = Some(self.grid.clone());
            self.alt_cursor = Some((self.grid.cursor_row, self.grid.cursor_col));
            self.grid.erase_screen();
            self.grid.cursor_row = 0;
            self.grid.cursor_col = 0;
        }
    }

    /// Switch back to primary screen buffer
    pub fn exit_alt_screen(&mut self) {
        if let Some(saved) = self.alt_grid.take() {
            tracing::info!(">>> EXIT ALT SCREEN: prompt_state={:?}, interactive_child={}, osc133_available={}, pending_command_row={:?}",
                self.prompt_state, self.interactive_child_active, self.osc133_available, self.pending_command_row);
            self.grid = saved;
            if let Some((row, col)) = self.alt_cursor.take() {
                self.grid.cursor_row = row.min(self.grid.rows.saturating_sub(1));
                self.grid.cursor_col = col.min(self.grid.cols.saturating_sub(1));
            }
            // Always finalize any pending command when leaving alt-screen,
            // regardless of prompt_state (it may have been changed by title updates etc.)
            if let Some(row) = self.pending_command_row.take() {
                if self.finalized_command_rows.last() != Some(&row) {
                    self.finalized_command_rows.push(row);
                }
            }
            // Reset prompt state so fold lines and history resume
            if self.prompt_state == PromptState::CommandRunning {
                self.prompt_state = PromptState::Idle;
            }
            self.interactive_child_active = false;
            self.interactive_saved_title.clear();
            self.command_pending = false;
        }
    }

    pub fn apply_sgr(&mut self, params: &[u16]) {
        let mut i = 0;
        if params.is_empty() {
            self.reset_attrs();
            return;
        }
        while i < params.len() {
            match params[i] {
                0 => self.reset_attrs(),
                1 => self.current_attrs.bold = true,
                2 => self.current_attrs.dim = true,
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                5 | 6 => self.current_attrs.blink = true,
                7 => self.current_attrs.reverse = true,
                8 => self.current_attrs.invisible = true,
                9 => self.current_attrs.strikethrough = true,
                22 => { self.current_attrs.bold = false; self.current_attrs.dim = false; }
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline = false,
                25 => self.current_attrs.blink = false,
                27 => self.current_attrs.reverse = false,
                28 => self.current_attrs.invisible = false,
                29 => self.current_attrs.strikethrough = false,
                // Standard foreground colors
                30..=37 => self.current_fg = Color::Indexed((params[i] - 30) as u8),
                38 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.current_fg = Color::Palette(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        let r = params[i + 2] as u8;
                        let g = params[i + 3] as u8;
                        let b = params[i + 4] as u8;
                        self.current_fg = Color::Rgb(r, g, b);
                        tracing::debug!("SGR 38;2 RGB fg: ({},{},{})", r, g, b);
                        i += 4;
                    }
                }
                39 => self.current_fg = Color::Default,
                // Standard background colors
                40..=47 => self.current_bg = Color::Indexed((params[i] - 40) as u8),
                48 => {
                    if i + 2 < params.len() && params[i + 1] == 5 {
                        self.current_bg = Color::Palette(params[i + 2] as u8);
                        i += 2;
                    } else if i + 4 < params.len() && params[i + 1] == 2 {
                        let r = params[i + 2] as u8;
                        let g = params[i + 3] as u8;
                        let b = params[i + 4] as u8;
                        self.current_bg = Color::Rgb(r, g, b);
                        tracing::debug!("SGR 48;2 RGB bg: ({},{},{})", r, g, b);
                        i += 4;
                    }
                }
                49 => self.current_bg = Color::Default,
                // Bright foreground colors
                90..=97 => self.current_fg = Color::Indexed((params[i] - 90 + 8) as u8),
                // Bright background colors
                100..=107 => self.current_bg = Color::Indexed((params[i] - 100 + 8) as u8),
                _ => {}
            }
            i += 1;
        }
    }

    fn reset_attrs(&mut self) {
        self.current_fg = Color::Default;
        self.current_bg = Color::Default;
        self.current_attrs = CellAttributes::default();
    }

    /// Get combined view: scrollback + grid rows
    pub fn total_rows(&self) -> usize {
        self.scrollback.len() + self.grid.rows
    }

    /// Get recent lines as text (for extracting filenames)
    pub fn get_recent_lines(&self, count: usize) -> Vec<String> {
        let total = self.total_rows();
        let start = total.saturating_sub(count);
        (start..total)
            .filter_map(|i| self.get_row(i))
            .map(|row| {
                row.cells.iter()
                    .map(|cell| cell.ch)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// Get a row by absolute index (0 = oldest scrollback)
    pub fn get_row(&self, idx: usize) -> Option<&Row> {
        let sb_len = self.scrollback.len();
        if idx < sb_len {
            self.scrollback.get(idx)
        } else {
            self.grid.cells.get(idx - sb_len)
        }
    }

    /// Get visible rows (respecting scroll offset)
    pub fn visible_rows(&self) -> Vec<&Row> {
        let display_start = self.visible_start();

        (display_start..display_start + self.grid.rows)
            .filter_map(|i| self.get_row(i))
            .collect()
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let max_offset = self.scrollback.len();
        if delta > 0 {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_offset);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        }
    }

    /// Compute the absolute row index where the visible area starts
    pub fn visible_start(&self) -> usize {
        let total = self.total_rows();
        let bottom = total.saturating_sub(self.grid.rows);
        let start = bottom.saturating_sub(self.scroll_offset);
        tracing::debug!("visible_start: total={}, grid.rows={}, bottom={}, scroll_offset={}, start={}",
            total, self.grid.rows, bottom, self.scroll_offset, start);
        start
    }

    /// Read text from the start of the cursor's row up to the cursor position
    pub fn current_input_line(&self) -> String {
        let row = &self.grid.cells[self.grid.cursor_row];
        let end = self.grid.cursor_col.min(row.cells.len());
        row.cells[..end]
            .iter()
            .filter(|c| !c.wide_placeholder)
            .map(|c| c.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    /// Read the cursor row text and strip the shell prompt prefix
    pub fn current_input_after_prompt(&self) -> String {
        let line = self.current_input_line();
        strip_prompt(&line).to_string()
    }

    /// Search upward from the current cursor row to find the previous command's row (shell prompt).
    /// Returns the absolute row number of that row.
    /// Mark the current cursor row as a command line (called when the user presses Enter).
    /// Check if a command is a known interactive program
    fn is_interactive_command(cmd: &str) -> bool {
        // Extract the first word (the program name), handling paths and env prefixes
        let program = cmd.split_whitespace()
            .find(|w| !w.contains('=') && *w != "env" && *w != "sudo" && *w != "nohup")
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");

        matches!(program,
            "python" | "python3" | "python2" | "ipython" | "bpython" |
            "node" | "deno" | "bun" |
            "ruby" | "irb" | "pry" |
            "lua" | "luajit" |
            "ghci" | "stack" |
            "erl" | "iex" |
            "scala" | "amm" |
            "R" | "Rscript" |
            "julia" |
            "php" |
            "perl" |
            "sqlite3" | "mysql" | "psql" | "redis-cli" | "mongo" | "mongosh" |
            "gdb" | "lldb" |
            "ssh" | "telnet" | "nc" | "ncat" |
            "claude" | "aichat" | "chatgpt" |
            "ftp" | "sftp" |
            "bc" | "dc" |
            "ocaml" | "utop" |
            "guile" | "racket" | "sbcl" | "clisp" |
            "clj" | "clojure" | "lein" |
            "sml" | "poly" |
            "nix" | "nix-shell"
        )
    }

    /// Mark the current cursor row as a command line (called when the user presses Enter).
    /// If `cmd` is provided, check if it's a known interactive program (fallback when OSC 133 unavailable).
    pub fn mark_command_line_with_cmd(&mut self, cmd: &str) {
        if self.interactive_child_active {
            return;
        }

        // When OSC 133 is available, it handles command lifecycle — skip heuristics
        if self.osc133_available {
            return;
        }

        // Fallback: use command-name heuristic when OSC 133 is NOT available
        if !cmd.is_empty() && Self::is_interactive_command(cmd) {
            self.interactive_child_active = true;
            self.command_pending = false;
            self.interactive_saved_title = self.title.clone();
            let abs_row = self.scrollback.len() + self.grid.cursor_row;
            if self.command_line_rows.last() != Some(&abs_row) {
                self.command_line_rows.push(abs_row);
            }
            return;
        }

        let abs_row = self.scrollback.len() + self.grid.cursor_row;
        if self.command_line_rows.last() != Some(&abs_row) {
            self.command_line_rows.push(abs_row);
        }
        self.command_pending = true;
        self.saw_decrst1_after_command = false;
        self.command_sent_cursor_abs = abs_row;
    }

    /// Called when the title changes (OSC 0/2). Used to detect interactive child exit.
    pub fn on_title_changed(&mut self) {
        tracing::info!(">>> TITLE CHANGED: '{}', alt_screen={}, prompt_state={:?}, interactive_child={}",
            self.title, self.alt_grid.is_some(), self.prompt_state, self.interactive_child_active);
        let looks_like_shell = self.title.contains(':') || self.title.contains('~')
            || self.title.contains('/');

        // If OSC 133 state is stuck in CommandRunning and title looks like shell,
        // the interactive child has exited and shell is re-prompting
        if self.osc133_available && self.prompt_state == PromptState::CommandRunning && looks_like_shell {
            self.prompt_state = PromptState::Idle;
            // Finalize the pending command block
            if let Some(row) = self.pending_command_row.take() {
                if self.finalized_command_rows.last() != Some(&row) {
                    self.finalized_command_rows.push(row);
                }
            }
        }

        if self.interactive_child_active && !self.interactive_saved_title.is_empty() {
            if self.title != self.interactive_saved_title && looks_like_shell {
                self.interactive_child_active = false;
                self.interactive_saved_title.clear();
            }
        }
    }

    /// Explicitly clear interactive child mode (e.g., when Ctrl+C/Ctrl+D is pressed)
    pub fn clear_interactive_child(&mut self) {
        self.interactive_child_active = false;
        self.interactive_saved_title.clear();
        self.command_pending = false;
        // Finalize any pending command so fold lines can resume
        if let Some(row) = self.pending_command_row.take() {
            if self.finalized_command_rows.last() != Some(&row) {
                self.finalized_command_rows.push(row);
            }
        }
        // Reset prompt state if stuck in CommandRunning
        if self.prompt_state == PromptState::CommandRunning {
            self.prompt_state = PromptState::Idle;
        }
    }

    // --- OSC 133 Shell Integration ---

    /// OSC 133;A — prompt start
    pub fn osc133_prompt_start(&mut self) {
        tracing::info!(">>> OSC 133;A (prompt start): alt_screen={}, prompt_state={:?}, interactive_child={}, osc133_available={}, pending_command_row={:?}",
            self.alt_grid.is_some(), self.prompt_state, self.interactive_child_active, self.osc133_available, self.pending_command_row);
        // If we receive prompt-start while still in alt-screen, the child must have exited
        // without sending DECRST 1049. Force exit alt-screen to recover.
        if self.alt_grid.is_some() {
            self.exit_alt_screen();
        }
        self.osc133_available = true;
        // Receiving prompt-start means any previous command has ended.
        // Finalize pending command if it wasn't already finalized by OSC 133;D.
        if let Some(row) = self.pending_command_row.take() {
            if self.finalized_command_rows.last() != Some(&row) {
                self.finalized_command_rows.push(row);
            }
        }
        self.prompt_state = PromptState::PromptShown;
        // If we were in interactive mode, the child has exited and shell is re-prompting
        if self.interactive_child_active {
            self.interactive_child_active = false;
            self.interactive_saved_title.clear();
            self.command_pending = false;
        }
    }

    /// OSC 133;B — prompt end (user starts typing)
    pub fn osc133_prompt_end(&mut self) {
        tracing::info!(">>> OSC 133;B (prompt end): alt_screen={}, prompt_state={:?}", self.alt_grid.is_some(), self.prompt_state);
        if self.alt_grid.is_some() { return; }
        self.osc133_available = true;
        self.prompt_state = PromptState::UserTyping;
    }

    /// OSC 133;C — command start (user pressed Enter)
    /// Also emits the prompt-end cursor position for the UI to use.
    pub fn osc133_command_start(&mut self) {
        tracing::info!(">>> OSC 133;C (command start): alt_screen={}, prompt_state={:?}", self.alt_grid.is_some(), self.prompt_state);
        if self.alt_grid.is_some() { return; }
        self.osc133_available = true;
        self.prompt_state = PromptState::CommandRunning;
        let abs_row = self.scrollback.len() + self.grid.cursor_row;
        self.pending_command_row = Some(abs_row);
        // Also record in command_line_rows for backward compat
        if self.command_line_rows.last() != Some(&abs_row) {
            self.command_line_rows.push(abs_row);
        }
    }

    /// OSC 133;D — command end (shell re-prompting)
    pub fn osc133_command_end(&mut self, _exit_code: Option<i32>) {
        tracing::info!(">>> OSC 133;D (command end): alt_screen={}, prompt_state={:?}, pending_command_row={:?}, exit_code={:?}",
            self.alt_grid.is_some(), self.prompt_state, self.pending_command_row, _exit_code);
        if self.alt_grid.is_some() { return; }
        self.osc133_available = true;
        self.prompt_state = PromptState::Idle;
        // Finalize the pending command block — it's now foldable
        if let Some(row) = self.pending_command_row.take() {
            if self.finalized_command_rows.last() != Some(&row) {
                self.finalized_command_rows.push(row);
            }
        }
        // Clear interactive state if it was set
        if self.interactive_child_active {
            self.interactive_child_active = false;
            self.interactive_saved_title.clear();
            self.command_pending = false;
        }
    }

    /// Returns the appropriate command rows for fold rendering
    pub fn fold_command_rows(&self) -> &[usize] {
        let rows = if self.osc133_available {
            &self.finalized_command_rows
        } else {
            &self.command_line_rows
        };
        tracing::debug!("fold_command_rows: osc133_available={}, using {} rows (count={})",
            self.osc133_available, if self.osc133_available { "finalized" } else { "command_line" }, rows.len());
        rows
    }

    /// Legacy mark_command_line without command text
    pub fn mark_command_line(&mut self) {
        self.mark_command_line_with_cmd("");
    }

    /// Find the previous command line from the current position
    pub fn find_previous_prompt(&self) -> Option<usize> {
        if self.command_line_rows.is_empty() { return None; }

        // Determine search start point
        let reference_row = if self.scroll_offset > 0 {
            self.visible_start()
        } else {
            // At the bottom, use the cursor's absolute row
            self.scrollback.len() + self.grid.cursor_row
        };

        // Find the maximum value in command_line_rows that is < reference_row
        let mut best: Option<usize> = None;
        for &row in &self.command_line_rows {
            if row < reference_row {
                best = Some(row);
            }
        }
        best
    }

    /// Jump to the specified absolute row (display that row at the top of the visible area)
    pub fn scroll_to_row(&mut self, abs_row: usize) {
        let total = self.total_rows();
        // To show abs_row at the top of the visible area:
        // visible_top = total - scroll_offset - grid.rows = abs_row
        // scroll_offset = total - grid.rows - abs_row
        let max_offset = total.saturating_sub(self.grid.rows);
        self.scroll_offset = max_offset.saturating_sub(abs_row);
    }

    /// Start a new selection at the given absolute (row, col)
    pub fn start_selection(&mut self, row: usize, col: usize, mode: crate::selection::SelectionMode) {
        let mut sel = Selection::new(row, col);
        sel.mode = mode;
        self.selection = Some(sel);
    }

    /// Extend the current selection to the given absolute (row, col)
    pub fn extend_selection(&mut self, row: usize, col: usize) {
        if let Some(sel) = &mut self.selection {
            sel.extend(row, col);
        }
    }

    /// Clear the current selection
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Extract the selected text as a String
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        if sel.is_empty() {
            return None;
        }
        let (start, end) = sel.normalized();
        let mut text = String::new();

        for row_idx in start.0..=end.0 {
            if let Some(row) = self.get_row(row_idx) {
                let col_start = if row_idx == start.0 { start.1 } else { 0 };
                let col_end = if row_idx == end.0 { end.1 + 1 } else { row.cells.len() };
                let col_end = col_end.min(row.cells.len());

                for col in col_start..col_end {
                    if !row.cells[col].wide_placeholder {
                        text.push(row.cells[col].ch);
                    }
                }
                // Trim trailing spaces from each line
                if row_idx < end.0 {
                    let trimmed = text.trim_end_matches(' ');
                    text.truncate(trimmed.len());
                    text.push('\n');
                }
            }
        }

        if text.is_empty() { None } else { Some(text) }
    }

    /// Expand selection at (row, col) to word boundaries
    pub fn word_bounds_at(&self, row: usize, col: usize) -> (usize, usize) {
        if let Some(r) = self.get_row(row) {
            let cells = &r.cells;
            let len = cells.len();
            if col >= len { return (col, col); }

            // If clicked on a wide character placeholder, fall back to the actual character
            let col = if cells[col].wide_placeholder && col > 0 { col - 1 } else { col };

            let ch = cells[col].ch;
            // Non-whitespace characters are treated as selectable word characters
            let is_word_char = |c: &Cell| !c.ch.is_whitespace() || c.wide_placeholder;

            if ch.is_whitespace() && !cells[col].wide_placeholder {
                return (col, col);
            }

            let mut start = col;
            while start > 0 && is_word_char(&cells[start - 1]) {
                start -= 1;
            }
            let mut end = col;
            while end + 1 < len && is_word_char(&cells[end + 1]) {
                end += 1;
            }
            (start, end)
        } else {
            (col, col)
        }
    }
}

/// Strip common shell prompt prefixes to extract the user's actual command input
fn strip_prompt(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return trimmed;
    }

    // Match "user@host:path$ cmd" or "user@host:path# cmd" format
    // Also matches "[user@host path]$ cmd" or "[user@host path]# cmd" format
    // Use rfind to find the last "$ " or "# "
    if let Some(pos) = line.rfind("$ ") {
        let rest = &line[pos + 2..];
        if !rest.is_empty() {
            return rest;
        }
    }
    if let Some(pos) = line.rfind("# ") {
        let rest = &line[pos + 2..];
        if !rest.is_empty() {
            return rest;
        }
    }
    // Match "% " (zsh default prompt)
    if let Some(pos) = line.rfind("% ") {
        let rest = &line[pos + 2..];
        if !rest.is_empty() {
            return rest;
        }
    }
    // Match "> " (e.g. zsh secondary prompt)
    if let Some(rest) = line.strip_prefix("> ") {
        return rest;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interactive_command_detection() {
        assert!(TerminalEmulator::is_interactive_command("python3"));
        assert!(TerminalEmulator::is_interactive_command("python"));
        assert!(TerminalEmulator::is_interactive_command("node"));
        assert!(TerminalEmulator::is_interactive_command("claude"));
        assert!(TerminalEmulator::is_interactive_command("mysql"));
        assert!(TerminalEmulator::is_interactive_command("psql"));
        assert!(TerminalEmulator::is_interactive_command("ssh"));
        assert!(TerminalEmulator::is_interactive_command("irb"));
        assert!(TerminalEmulator::is_interactive_command("sqlite3"));

        // With arguments
        assert!(TerminalEmulator::is_interactive_command("python3 -i script.py"));
        assert!(TerminalEmulator::is_interactive_command("ssh user@host"));
        assert!(TerminalEmulator::is_interactive_command("mysql -u root -p"));

        // With path
        assert!(TerminalEmulator::is_interactive_command("/usr/bin/python3"));

        // With sudo/env prefix
        assert!(TerminalEmulator::is_interactive_command("sudo python3"));
        assert!(TerminalEmulator::is_interactive_command("env python3"));
        assert!(TerminalEmulator::is_interactive_command("sudo ssh user@host"));

        // Non-interactive
        assert!(!TerminalEmulator::is_interactive_command("ls"));
        assert!(!TerminalEmulator::is_interactive_command("ls -la"));
        assert!(!TerminalEmulator::is_interactive_command("cat file.txt"));
        assert!(!TerminalEmulator::is_interactive_command("grep pattern file"));
        assert!(!TerminalEmulator::is_interactive_command("cargo build"));
        assert!(!TerminalEmulator::is_interactive_command(""));
    }

    #[test]
    fn test_interactive_child_via_command_name() {
        let mut emu = TerminalEmulator::new(80, 24);
        assert!(!emu.interactive_child_active);

        // User types "python3" and presses Enter
        emu.mark_command_line_with_cmd("python3");
        assert!(emu.interactive_child_active);
        assert!(emu.is_interactive());

        // mark_command_line should be skipped in interactive mode
        let prev_count = emu.command_line_rows.len();
        emu.mark_command_line_with_cmd("print('hello')");
        assert_eq!(emu.command_line_rows.len(), prev_count);

        // Ctrl+D clears interactive mode
        emu.clear_interactive_child();
        assert!(!emu.interactive_child_active);
        assert!(!emu.is_interactive());
    }

    #[test]
    fn test_interactive_exit_via_title_change() {
        let mut emu = TerminalEmulator::new(80, 24);

        // Set initial shell title
        emu.process(b"\x1b]0;user@host:~/project\x07");
        assert_eq!(emu.title, "user@host:~/project");

        // User starts python
        emu.mark_command_line_with_cmd("python3");
        assert!(emu.interactive_child_active);
        assert_eq!(emu.interactive_saved_title, "user@host:~/project");

        // Python might change title (or not)
        // When python exits, shell re-prompts and sets title back
        emu.process(b"\x1b]0;user@host:~/project\x07");
        // Title is same as saved — no exit detected (could be python setting same title)
        // Actually the title didn't change from saved, so on_title_changed won't trigger
        assert!(emu.interactive_child_active);

        // Shell sets a different title (e.g., different directory)
        emu.process(b"\x1b]0;user@host:~/other\x07");
        // Title changed and looks like shell title → exit detected
        assert!(!emu.interactive_child_active);
    }

    #[test]
    fn test_noninteractive_command_not_detected() {
        let mut emu = TerminalEmulator::new(80, 24);

        emu.mark_command_line_with_cmd("ls -la");
        assert!(!emu.interactive_child_active);
        assert!(emu.command_pending);

        emu.mark_command_line_with_cmd("cargo build");
        assert!(!emu.interactive_child_active);
    }

    #[test]
    fn test_empty_enter_not_interactive() {
        let mut emu = TerminalEmulator::new(80, 24);

        emu.mark_command_line_with_cmd("");
        assert!(!emu.interactive_child_active);
    }

    #[test]
    fn test_alt_screen_is_interactive() {
        let mut emu = TerminalEmulator::new(80, 24);
        assert!(!emu.is_interactive());

        emu.enter_alt_screen();
        assert!(emu.is_interactive());
        assert!(emu.is_alt_screen());

        emu.exit_alt_screen();
        assert!(!emu.is_interactive());
    }

    #[test]
    fn test_osc133_basic_lifecycle() {
        let mut emu = TerminalEmulator::new(80, 24);
        assert!(!emu.osc133_available);
        assert_eq!(emu.prompt_state, PromptState::Idle);

        // Shell sends prompt start
        emu.process(b"\x1b]133;A\x07");
        assert!(emu.osc133_available);
        assert_eq!(emu.prompt_state, PromptState::PromptShown);

        // Shell sends prompt end (user can type)
        emu.process(b"\x1b]133;B\x07");
        assert_eq!(emu.prompt_state, PromptState::UserTyping);
        assert!(!emu.is_interactive()); // not interactive while typing

        // User presses Enter → command start
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);
        assert!(emu.is_interactive()); // running command = interactive
        assert!(emu.pending_command_row.is_some());
        assert!(emu.finalized_command_rows.is_empty()); // not finalized yet

        // Command completes → command end
        emu.process(b"\x1b]133;D;0\x07");
        assert_eq!(emu.prompt_state, PromptState::Idle);
        assert!(!emu.is_interactive()); // back to idle
        assert_eq!(emu.finalized_command_rows.len(), 1); // now finalized
    }

    #[test]
    fn test_osc133_interactive_child_detection() {
        let mut emu = TerminalEmulator::new(80, 24);

        // Shell integration active
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");

        // User runs python → command start, no command end
        emu.process(b"\x1b]133;C\x07");
        assert!(emu.is_interactive());

        // Python is running... no D received
        // Eventually python exits, shell re-prompts with D then A
        emu.process(b"\x1b]133;D;0\x07");
        assert!(!emu.is_interactive());
        assert_eq!(emu.finalized_command_rows.len(), 1);

        // Move cursor down to simulate output
        emu.process(b"\n\n");

        // Shell re-prompts
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");

        // User runs ls → command start then end
        emu.process(b"\x1b]133;C\x07");
        assert!(emu.is_interactive());
        emu.process(b"\x1b]133;D;0\x07");
        assert!(!emu.is_interactive());
        assert_eq!(emu.finalized_command_rows.len(), 2);
    }

    #[test]
    fn test_osc133_skips_command_name_heuristic() {
        let mut emu = TerminalEmulator::new(80, 24);

        // Enable OSC 133
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        assert!(emu.osc133_available);

        // mark_command_line_with_cmd should be a no-op when OSC 133 is available
        emu.mark_command_line_with_cmd("python3");
        assert!(!emu.interactive_child_active); // NOT set via heuristic
    }

    #[test]
    fn test_osc133_fold_rows_vs_command_line_rows() {
        let mut emu = TerminalEmulator::new(80, 24);

        // Without OSC 133, fold_command_rows returns command_line_rows
        emu.command_line_rows.push(0);
        assert_eq!(emu.fold_command_rows(), &[0]);

        // With OSC 133, fold_command_rows returns finalized_command_rows
        emu.osc133_available = true;
        emu.finalized_command_rows.push(5);
        assert_eq!(emu.fold_command_rows(), &[5]);
    }

    #[test]
    fn test_osc133_in_alt_screen() {
        let mut emu = TerminalEmulator::new(80, 24);

        // Enable OSC 133 and start a command
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);

        // Enter alt-screen (e.g., Claude Code starts)
        emu.enter_alt_screen();
        assert!(emu.is_alt_screen());

        // OSC 133;B and D should be ignored in alt-screen
        emu.process(b"\x1b]133;D;0\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning); // unchanged

        // OSC 133;A (prompt start) forces exit from alt-screen — the child must have exited
        emu.process(b"\x1b]133;A\x07");
        assert!(!emu.is_alt_screen());
        assert_eq!(emu.prompt_state, PromptState::PromptShown);
        assert!(!emu.is_interactive());
        assert_eq!(emu.finalized_command_rows.len(), 1); // command was finalized
    }

    #[test]
    fn test_clear_interactive_resets_prompt_state() {
        let mut emu = TerminalEmulator::new(80, 24);

        // OSC 133 command running
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        emu.process(b"\x1b]133;C\x07");
        assert!(emu.is_interactive());

        // Ctrl+C/Ctrl+D calls clear_interactive_child
        emu.clear_interactive_child();
        assert!(!emu.is_interactive());
        assert_eq!(emu.prompt_state, PromptState::Idle);
        // pending command should be finalized
        assert_eq!(emu.finalized_command_rows.len(), 1);
    }

    #[test]
    fn test_alt_screen_app_exit_fold_recovery() {
        // Simulates: user runs "claude" → enters alt-screen → exits → shell re-prompts → user runs "cat"
        let mut emu = TerminalEmulator::new(80, 24);

        // Initial shell prompt
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        assert_eq!(emu.prompt_state, PromptState::UserTyping);

        // User types "ls" and presses Enter
        emu.process(b"ls\r\n");
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);
        assert!(emu.pending_command_row.is_some());

        // ls output
        emu.process(b"file1.txt  file2.txt\r\n");

        // ls completes
        emu.process(b"\x1b]133;D;0\x07");
        assert_eq!(emu.prompt_state, PromptState::Idle);
        assert_eq!(emu.finalized_command_rows.len(), 1);

        // Shell re-prompts
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");

        // User types "claude" and presses Enter
        emu.process(b"claude\r\n");
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);
        assert!(emu.pending_command_row.is_some());
        let claude_row = emu.pending_command_row.unwrap();

        // Claude Code enters alt-screen
        emu.process(b"\x1b[?1049h");
        assert!(emu.is_alt_screen());
        assert!(emu.is_interactive());

        // Claude Code does stuff in alt-screen...
        emu.process(b"Claude Code output...\r\n");

        // Claude Code exits alt-screen
        emu.process(b"\x1b[?1049l");
        assert!(!emu.is_alt_screen());
        // The "claude" command should be finalized
        assert!(emu.finalized_command_rows.contains(&claude_row));
        assert_eq!(emu.finalized_command_rows.len(), 2);

        // Shell sends OSC 133;D (command end for "claude")
        emu.process(b"\x1b]133;D;0\x07");
        // Shell re-prompts
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        assert_eq!(emu.prompt_state, PromptState::UserTyping);
        assert!(!emu.is_interactive());

        // Fold lines should be available
        let fold_rows = emu.fold_command_rows();
        assert_eq!(fold_rows.len(), 2); // "ls" and "claude" commands

        // User types "cat a.txt" and presses Enter
        emu.process(b"cat a.txt\r\n");
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);

        // cat output
        emu.process(b"hello world\r\n");

        // cat completes
        emu.process(b"\x1b]133;D;0\x07");
        assert_eq!(emu.prompt_state, PromptState::Idle);
        assert_eq!(emu.finalized_command_rows.len(), 3); // ls, claude, cat

        // Shell re-prompts
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        assert!(!emu.is_interactive());

        // All three commands should have fold lines
        let fold_rows = emu.fold_command_rows();
        assert_eq!(fold_rows.len(), 3);
    }

    #[test]
    fn test_alt_screen_ctrl_c_exit_fold_recovery() {
        // Simulates: user runs "claude" → enters alt-screen → Ctrl+C → exits → fold recovery
        let mut emu = TerminalEmulator::new(80, 24);

        // Initial shell prompt
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");

        // User types "claude" and presses Enter
        emu.process(b"claude\r\n");
        emu.process(b"\x1b]133;C\x07");
        assert!(emu.is_interactive());

        // Claude Code enters alt-screen
        emu.process(b"\x1b[?1049h");
        assert!(emu.is_alt_screen());

        // User presses Ctrl+C → clear_interactive_child is called
        emu.clear_interactive_child();
        // prompt_state should be Idle, pending command finalized
        assert_eq!(emu.prompt_state, PromptState::Idle);
        assert_eq!(emu.finalized_command_rows.len(), 1);

        // Claude Code exits alt-screen (cleanup after SIGINT)
        emu.process(b"\x1b[?1049l");
        assert!(!emu.is_alt_screen());
        assert!(!emu.is_interactive());

        // Shell re-prompts
        emu.process(b"\x1b]133;D;130\x07"); // exit code 130 = SIGINT
        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        assert_eq!(emu.prompt_state, PromptState::UserTyping);
        assert!(!emu.is_interactive());

        // Fold lines should work for subsequent commands
        emu.process(b"cat a.txt\r\n");
        emu.process(b"\x1b]133;C\x07");
        emu.process(b"hello\r\n");
        emu.process(b"\x1b]133;D;0\x07");
        assert_eq!(emu.finalized_command_rows.len(), 2);
    }

    #[test]
    fn test_osc133_prompt_start_finalizes_pending() {
        // If OSC 133;D is missed, OSC 133;A should still finalize the pending command
        let mut emu = TerminalEmulator::new(80, 24);

        emu.process(b"\x1b]133;A\x07");
        emu.process(b"\x1b]133;B\x07");
        emu.process(b"\x1b]133;C\x07");
        assert!(emu.pending_command_row.is_some());
        assert_eq!(emu.finalized_command_rows.len(), 0);

        // Skip OSC 133;D — go straight to next prompt
        emu.process(b"\x1b]133;A\x07");
        // pending_command_row should be finalized
        assert!(emu.pending_command_row.is_none());
        assert_eq!(emu.finalized_command_rows.len(), 1);
    }

    #[test]
    fn test_split_osc_sequence_across_chunks() {
        // Verify that OSC sequences split across process() calls are handled correctly
        let mut emu = TerminalEmulator::new(80, 24);

        // Send OSC 133;A split across two chunks
        emu.process(b"\x1b]133");
        assert!(!emu.osc133_available); // not yet complete
        emu.process(b";A\x07");
        assert!(emu.osc133_available); // now complete
        assert_eq!(emu.prompt_state, PromptState::PromptShown);

        // Send OSC 133;B split differently
        emu.process(b"\x1b]");
        emu.process(b"133;B\x07");
        assert_eq!(emu.prompt_state, PromptState::UserTyping);

        // Send OSC 133;C in one chunk (control case)
        emu.process(b"\x1b]133;C\x07");
        assert_eq!(emu.prompt_state, PromptState::CommandRunning);

        // Send OSC 133;D split at the semicolon
        emu.process(b"\x1b]133;D;");
        emu.process(b"0\x07");
        assert_eq!(emu.prompt_state, PromptState::Idle);
        assert_eq!(emu.finalized_command_rows.len(), 1);
    }
}
