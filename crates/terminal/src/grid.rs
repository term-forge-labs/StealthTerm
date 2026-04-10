use crate::cell::Cell;
use serde::{Deserialize, Serialize};

/// A single row in the terminal grid
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Whether this row has a hard newline at the end (vs soft wrap)
    pub wrapped: bool,
}

impl Row {
    pub fn new(cols: usize) -> Self {
        Self {
            cells: vec![Cell::blank(); cols],
            wrapped: false,
        }
    }

    pub fn resize(&mut self, cols: usize) {
        if self.cells.len() < cols {
            self.cells.resize(cols, Cell::blank());
        } else {
            self.cells.truncate(cols);
        }
    }

    pub fn get(&self, col: usize) -> Option<&Cell> {
        self.cells.get(col)
    }

    pub fn get_mut(&mut self, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(col)
    }

    /// Convert row to plain string
    pub fn to_string_lossy(&self) -> String {
        let s: String = self.cells.iter()
            .filter(|c| !c.wide_placeholder)
            .map(|c| c.ch)
            .collect();
        s.trim_end().to_string()
    }
}

/// The active terminal grid (visible area)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<Row>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub scroll_top: usize,
    pub scroll_bottom: usize,
    pub saved_cursor: Option<(usize, usize)>,
}

impl Grid {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: (0..rows).map(|_| Row::new(cols)).collect(),
            cursor_col: 0,
            cursor_row: 0,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            saved_cursor: None,
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols;
        self.rows = rows;
        self.cells.resize_with(rows, || Row::new(cols));
        for row in &mut self.cells {
            row.resize(cols);
        }
        self.cursor_col = self.cursor_col.min(cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(rows.saturating_sub(1));
        self.scroll_bottom = rows.saturating_sub(1);
    }

    pub fn cell_at(&self, row: usize, col: usize) -> Option<&Cell> {
        self.cells.get(row)?.get(col)
    }

    pub fn cell_at_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
        self.cells.get_mut(row)?.get_mut(col)
    }

    pub fn set_cell(&mut self, row: usize, col: usize, cell: Cell) {
        if let Some(r) = self.cells.get_mut(row) {
            if let Some(c) = r.cells.get_mut(col) {
                *c = cell;
            }
        }
    }

    /// Scroll up the scroll region by n lines, adding blank lines at bottom
    pub fn scroll_up(&mut self, n: usize) -> Vec<Row> {
        let top = self.scroll_top;
        let bot = self.scroll_bottom.min(self.rows.saturating_sub(1));
        let mut scrolled = Vec::new();
        for _ in 0..n {
            if top <= bot {
                scrolled.push(self.cells.remove(top));
                self.cells.insert(bot, Row::new(self.cols));
            }
        }
        scrolled
    }

    /// Scroll down the scroll region by n lines
    pub fn scroll_down(&mut self, n: usize) {
        let top = self.scroll_top;
        let bot = self.scroll_bottom.min(self.rows.saturating_sub(1));
        for _ in 0..n {
            if top <= bot {
                self.cells.remove(bot);
                self.cells.insert(top, Row::new(self.cols));
            }
        }
    }

    /// Erase from cursor to end of line
    pub fn erase_line_right(&mut self, row: usize, col: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for c in col..r.cells.len() {
                r.cells[c] = Cell::blank();
            }
        }
    }

    /// Erase from start of line to cursor
    pub fn erase_line_left(&mut self, row: usize, col: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for c in 0..=col.min(r.cells.len().saturating_sub(1)) {
                r.cells[c] = Cell::blank();
            }
        }
    }

    /// Erase entire line
    pub fn erase_line(&mut self, row: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for c in &mut r.cells {
                *c = Cell::blank();
            }
        }
    }

    /// Erase from cursor to end of screen
    pub fn erase_screen_down(&mut self, row: usize, col: usize) {
        self.erase_line_right(row, col);
        for r in (row + 1)..self.rows {
            self.erase_line(r);
        }
    }

    /// Erase from start of screen to cursor
    pub fn erase_screen_up(&mut self, row: usize, col: usize) {
        for r in 0..row {
            self.erase_line(r);
        }
        self.erase_line_left(row, col);
    }

    /// Erase entire screen
    pub fn erase_screen(&mut self) {
        for r in &mut self.cells {
            for c in &mut r.cells {
                *c = Cell::blank();
            }
        }
    }

    /// Erase n characters starting at cursor position
    pub fn erase_characters(&mut self, row: usize, col: usize, n: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            for i in 0..n {
                let c = col + i;
                if c < r.cells.len() {
                    r.cells[c] = Cell::blank();
                }
            }
        }
    }

    /// Delete n characters at cursor, shifting remaining left
    pub fn delete_characters(&mut self, row: usize, col: usize, n: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            let len = r.cells.len();
            let n = n.min(len.saturating_sub(col));
            for _ in 0..n {
                if col < r.cells.len() {
                    r.cells.remove(col);
                    r.cells.push(Cell::blank());
                }
            }
        }
    }

    /// Insert n blank characters at cursor, shifting existing right
    pub fn insert_characters(&mut self, row: usize, col: usize, n: usize) {
        if let Some(r) = self.cells.get_mut(row) {
            let len = r.cells.len();
            let n = n.min(len.saturating_sub(col));
            for _ in 0..n {
                if col < len {
                    r.cells.insert(col, Cell::blank());
                    r.cells.truncate(len);
                }
            }
        }
    }

    pub fn save_cursor(&mut self) {
        self.saved_cursor = Some((self.cursor_row, self.cursor_col));
    }

    pub fn restore_cursor(&mut self) {
        if let Some((row, col)) = self.saved_cursor {
            self.cursor_row = row.min(self.rows.saturating_sub(1));
            self.cursor_col = col.min(self.cols.saturating_sub(1));
        }
    }
}
