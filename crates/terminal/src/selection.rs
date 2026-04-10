/// Text selection in the terminal
#[derive(Debug, Clone, PartialEq)]
pub struct Selection {
    pub start: (usize, usize), // (row, col) in total buffer coordinates
    pub end: (usize, usize),
    pub mode: SelectionMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectionMode {
    Character,
    Word,
    Line,
    Block,
}

impl Selection {
    pub fn new(row: usize, col: usize) -> Self {
        Self {
            start: (row, col),
            end: (row, col),
            mode: SelectionMode::Character,
        }
    }

    pub fn extend(&mut self, row: usize, col: usize) {
        self.end = (row, col);
    }

    /// Get normalized (start <= end) selection
    pub fn normalized(&self) -> ((usize, usize), (usize, usize)) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }

    pub fn contains_row(&self, row: usize) -> bool {
        let (start, end) = self.normalized();
        row >= start.0 && row <= end.0
    }

    pub fn is_cell_selected(&self, row: usize, col: usize) -> bool {
        let (start, end) = self.normalized();
        match self.mode {
            SelectionMode::Character => {
                if row < start.0 || row > end.0 {
                    false
                } else if row == start.0 && row == end.0 {
                    col >= start.1 && col <= end.1
                } else if row == start.0 {
                    col >= start.1
                } else if row == end.0 {
                    col <= end.1
                } else {
                    true
                }
            }
            SelectionMode::Line => row >= start.0 && row <= end.0,
            SelectionMode::Block => {
                let min_col = start.1.min(end.1);
                let max_col = start.1.max(end.1);
                row >= start.0 && row <= end.0 && col >= min_col && col <= max_col
            }
            SelectionMode::Word => {
                // Simplified word selection
                if row < start.0 || row > end.0 { false }
                else if start.0 == end.0 { col >= start.1 && col <= end.1 }
                else if row == start.0 { col >= start.1 }
                else if row == end.0 { col <= end.1 }
                else { true }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
