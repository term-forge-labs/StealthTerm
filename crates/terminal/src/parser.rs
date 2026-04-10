use vte::{Params, Perform};
use crate::cell::Cell;
use crate::emulator::TerminalEmulator;
use unicode_width::UnicodeWidthChar;
use tracing::debug;

impl Perform for TerminalEmulator {
    fn print(&mut self, c: char) {
        let width = c.width().unwrap_or(1);
        let col = self.grid.cursor_col;
        let row = self.grid.cursor_row;

        if col >= self.grid.cols {
            // Soft wrap: cursor was at right margin, move to next line col 0
            self.grid.cells[row].wrapped = true;
            self.grid.cursor_col = 0;
            self.newline();
        }

        let cell = Cell {
            ch: c,
            fg: self.current_fg,
            bg: self.current_bg,
            attrs: self.current_attrs,
            wide: width > 1,
            wide_placeholder: false,
        };

        let col = self.grid.cursor_col;
        let row = self.grid.cursor_row;
        self.grid.set_cell(row, col, cell.clone());

        if width > 1 && col + 1 < self.grid.cols {
            let placeholder = Cell {
                ch: ' ',
                wide_placeholder: true,
                ..cell.clone()
            };
            self.grid.set_cell(row, col + 1, placeholder);
        }

        self.grid.cursor_col += width;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\r' => {
                self.grid.cursor_col = 0;
            }
            b'\n' | b'\x0b' | b'\x0c' => {
                self.newline();
            }
            b'\x08' => {
                // Backspace
                if self.grid.cursor_col > 0 {
                    self.grid.cursor_col -= 1;
                }
            }
            b'\x07' => {
                // Bell - ignore
            }
            b'\t' => {
                // Tab - advance to next 8-column stop
                let col = self.grid.cursor_col;
                let next = ((col / 8) + 1) * 8;
                self.grid.cursor_col = next.min(self.grid.cols - 1);
            }
            _ => {
                debug!("Unhandled execute byte: 0x{:02x}", byte);
            }
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2 {
            match params[0] {
                b"0" | b"2" => {
                    // Set window title
                    if let Ok(title) = std::str::from_utf8(params[1]) {
                        self.title = title.to_string();
                        self.on_title_changed();
                    }
                }
                b"133" => {
                    // FinalTerm / iTerm2 shell integration
                    match params[1] {
                        b"A" => self.osc133_prompt_start(),
                        b"B" => self.osc133_prompt_end(),
                        b"C" => self.osc133_command_start(),
                        b"D" => {
                            let exit_code = params.get(2)
                                .and_then(|s| std::str::from_utf8(s).ok())
                                .and_then(|s| s.parse::<i32>().ok());
                            self.osc133_command_end(exit_code);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let p: Vec<u16> = params.iter()
            .map(|sub| sub.first().copied().unwrap_or(0))
            .collect();

        let p1 = p.first().copied().unwrap_or(0);
        let p2 = p.get(1).copied().unwrap_or(0);

        match (intermediates, action) {
            (b"", 'A') => {
                // CUU - cursor up
                let n = p1.max(1) as usize;
                self.grid.cursor_row = self.grid.cursor_row.saturating_sub(n);
            }
            (b"", 'B') => {
                // CUD - cursor down
                let n = p1.max(1) as usize;
                self.grid.cursor_row = (self.grid.cursor_row + n).min(self.grid.rows - 1);
            }
            (b"", 'C') => {
                // CUF - cursor forward
                let n = p1.max(1) as usize;
                self.grid.cursor_col = (self.grid.cursor_col + n).min(self.grid.cols - 1);
            }
            (b"", 'D') => {
                // CUB - cursor back
                let n = p1.max(1) as usize;
                self.grid.cursor_col = self.grid.cursor_col.saturating_sub(n);
            }
            (b"", 'E') => {
                // CNL - cursor next line
                let n = p1.max(1) as usize;
                self.grid.cursor_row = (self.grid.cursor_row + n).min(self.grid.rows - 1);
                self.grid.cursor_col = 0;
            }
            (b"", 'F') => {
                // CPL - cursor previous line
                let n = p1.max(1) as usize;
                self.grid.cursor_row = self.grid.cursor_row.saturating_sub(n);
                self.grid.cursor_col = 0;
            }
            (b"", 'G') => {
                // CHA - cursor horizontal absolute
                let col = p1.max(1).saturating_sub(1) as usize;
                self.grid.cursor_col = col.min(self.grid.cols - 1);
            }
            (b"", 'H') | (b"", 'f') => {
                // CUP - cursor position
                let row = p1.saturating_sub(1) as usize;
                let col = p2.saturating_sub(1) as usize;
                self.grid.cursor_row = row.min(self.grid.rows - 1);
                self.grid.cursor_col = col.min(self.grid.cols - 1);
            }
            (b"", 'J') => {
                // ED - erase in display
                match p1 {
                    0 => self.grid.erase_screen_down(self.grid.cursor_row, self.grid.cursor_col),
                    1 => self.grid.erase_screen_up(self.grid.cursor_row, self.grid.cursor_col),
                    2 | 3 => self.grid.erase_screen(),
                    _ => {}
                }
            }
            (b"", 'K') => {
                // EL - erase in line
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                match p1 {
                    0 => self.grid.erase_line_right(row, col),
                    1 => self.grid.erase_line_left(row, col),
                    2 => self.grid.erase_line(row),
                    _ => {}
                }
            }
            (b"", 'X') => {
                // ECH - erase characters
                let n = p1.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                self.grid.erase_characters(row, col, n);
            }
            (b"", 'P') => {
                // DCH - delete characters
                let n = p1.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                self.grid.delete_characters(row, col, n);
            }
            (b"", '@') => {
                // ICH - insert characters
                let n = p1.max(1) as usize;
                let row = self.grid.cursor_row;
                let col = self.grid.cursor_col;
                self.grid.insert_characters(row, col, n);
            }
            (b"", 'd') => {
                // VPA - vertical position absolute
                let row = p1.max(1).saturating_sub(1) as usize;
                self.grid.cursor_row = row.min(self.grid.rows - 1);
            }
            (b"", '`') => {
                // HPA - horizontal position absolute
                let col = p1.max(1).saturating_sub(1) as usize;
                self.grid.cursor_col = col.min(self.grid.cols - 1);
            }
            (b"", 'm') => {
                // SGR - select graphic rendition
                self.apply_sgr(&p);
            }
            (b"", 'r') => {
                // DECSTBM - set scroll region
                let top = p1.saturating_sub(1) as usize;
                let bot = if p2 == 0 { self.grid.rows - 1 } else { (p2 - 1) as usize };
                self.grid.scroll_top = top;
                self.grid.scroll_bottom = bot.min(self.grid.rows - 1);
                self.grid.cursor_row = 0;
                self.grid.cursor_col = 0;
            }
            (b"", 's') => {
                self.grid.save_cursor();
            }
            (b"", 'u') => {
                self.grid.restore_cursor();
            }
            (b"", 'S') => {
                // Scroll up
                let n = p1.max(1) as usize;
                let scrolled = self.grid.scroll_up(n);
                self.scrollback.push_many(scrolled);
            }
            (b"", 'T') => {
                // Scroll down
                let n = p1.max(1) as usize;
                self.grid.scroll_down(n);
            }
            (b"", 'L') => {
                // IL - insert lines
                let n = p1.max(1) as usize;
                for _ in 0..n {
                    let row = self.grid.cursor_row;
                    self.grid.cells.insert(row, crate::grid::Row::new(self.grid.cols));
                    let bot = self.grid.scroll_bottom;
                    if bot < self.grid.cells.len() {
                        self.grid.cells.remove(bot + 1);
                    } else if self.grid.cells.len() > self.grid.rows {
                        self.grid.cells.pop();
                    }
                }
            }
            (b"", 'M') => {
                // DL - delete lines
                let n = p1.max(1) as usize;
                for _ in 0..n {
                    let row = self.grid.cursor_row;
                    if row < self.grid.cells.len() {
                        self.grid.cells.remove(row);
                        let bot = self.grid.scroll_bottom.min(self.grid.rows - 1);
                        self.grid.cells.insert(bot, crate::grid::Row::new(self.grid.cols));
                    }
                }
            }
            (b" ", 'q') => {
                // DECSCUSR - set cursor style
                use crate::emulator::CursorShape;
                match p1 {
                    0 | 1 => {
                        self.cursor_shape = CursorShape::Block;
                        self.cursor_blink = true;
                    }
                    2 => {
                        self.cursor_shape = CursorShape::Block;
                        self.cursor_blink = false;
                    }
                    3 => {
                        self.cursor_shape = CursorShape::Underline;
                        self.cursor_blink = true;
                    }
                    4 => {
                        self.cursor_shape = CursorShape::Underline;
                        self.cursor_blink = false;
                    }
                    5 => {
                        self.cursor_shape = CursorShape::Bar;
                        self.cursor_blink = true;
                    }
                    6 => {
                        self.cursor_shape = CursorShape::Bar;
                        self.cursor_blink = false;
                    }
                    _ => {}
                }
            }
            (b"?", 'h') => {
                // DEC private mode set — process ALL parameters
                tracing::info!(">>> DECSET modes: {:?}", p);
                for &mode in &p {
                    match mode {
                        1 => {
                            self.application_cursor_keys = true;
                            self.on_application_cursor_keys_changed(true);
                        }
                        7 => self.auto_wrap = true,
                        25 => self.cursor_visible = true,
                        47 | 1047 | 1049 => self.enter_alt_screen(),
                        2004 => {
                            self.bracketed_paste = true;
                        }
                        _ => {}
                    }
                }
            }
            (b"?", 'l') => {
                // DEC private mode reset — process ALL parameters
                tracing::info!(">>> DECRST modes: {:?}", p);
                for &mode in &p {
                    match mode {
                        1 => {
                            self.application_cursor_keys = false;
                            self.on_application_cursor_keys_changed(false);
                        }
                        7 => self.auto_wrap = false,
                        25 => self.cursor_visible = false,
                        47 | 1047 | 1049 => self.exit_alt_screen(),
                        2004 => {
                            self.bracketed_paste = false;
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                debug!("Unhandled CSI: {:?} {:?} {}", intermediates, p, action);
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            (b"", b'7') => self.grid.save_cursor(),
            (b"", b'8') => self.grid.restore_cursor(),
            (b"", b'D') => {
                // IND - index (move cursor down, scroll if at bottom)
                self.newline();
            }
            (b"", b'M') => {
                // RI - reverse index
                if self.grid.cursor_row == self.grid.scroll_top {
                    self.grid.scroll_down(1);
                } else if self.grid.cursor_row > 0 {
                    self.grid.cursor_row -= 1;
                }
            }
            (b"", b'c') => {
                // RIS - full reset
                tracing::warn!(">>> RIS (full reset) triggered! osc133_available was {}", self.osc133_available);
                let cols = self.grid.cols;
                let rows = self.grid.rows;
                *self = TerminalEmulator::new(cols, rows);
            }
            (b"#", b'8') => {
                // DECALN - fill screen with 'E' for alignment test
                for row in &mut self.grid.cells {
                    for cell in &mut row.cells {
                        cell.ch = 'E';
                    }
                }
            }
            _ => {
                debug!("Unhandled ESC: {:?} 0x{:02x}", intermediates, byte);
            }
        }
    }
}
