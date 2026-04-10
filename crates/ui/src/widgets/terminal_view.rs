use egui::{Color32, FontFamily, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use egui::emath::GuiRounding;
use stealthterm_terminal::emulator::TerminalEmulator;
use stealthterm_terminal::cell::{Cell};
use stealthterm_terminal::command_fold::FoldRenderInfo;
use stealthterm_terminal::renderer::TerminalTheme;
use stealthterm_terminal::selection::SelectionMode;
use std::collections::HashSet;

/// Return value of show_with_size
pub struct TerminalViewResult {
    pub response: Response,
    pub size_changed: bool,
    /// Left-click on a fold icon (returns prompt_line)
    pub fold_left_click: Option<usize>,
    /// Right-click on a fold area (returns prompt_line + mouse position)
    pub fold_right_click: Option<(usize, Pos2)>,
}

/// Renders a TerminalEmulator into an egui panel
pub struct TerminalView {
    pub font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    pub theme: TerminalTheme,
    pub show_line_numbers: bool,
    /// Search match cells for highlighting — set by the panel before rendering
    pub search_match_cells: HashSet<(usize, usize)>,
    /// Ghost text suggestion to display after cursor
    pub suggestion: Option<String>,
    /// Whether the backend is still connected (controls cursor blink)
    pub connected: bool,
    cursor_blink_visible: bool,
    last_blink_toggle: std::time::Instant,
    /// Cached font metrics — invalidated when font_size changes (set to 0.0 to force recalc)
    pub cached_font_size: f32,
    /// X offset from terminal rect origin to content area (line numbers + fold gutter)
    content_x_offset: f32,
}

impl TerminalView {
    pub fn new(font_size: f32) -> Self {
        let font_size = font_size.round();
        Self {
            font_size,
            cell_width: (font_size * 0.6).round(),
            cell_height: (font_size * 1.2).round(),
            theme: TerminalTheme::default(),
            show_line_numbers: false,
            search_match_cells: HashSet::new(),
            suggestion: None,
            connected: true,
            cursor_blink_visible: true,
            last_blink_toggle: std::time::Instant::now(),
            cached_font_size: 0.0,
            content_x_offset: 0.0,
        }
    }

    /// Measure actual glyph metrics from the font and update cell dimensions
    fn update_cell_metrics(&mut self, ctx: &egui::Context) {
        if (self.cached_font_size - self.font_size).abs() < 0.01 {
            return;
        }
        let ppp = ctx.pixels_per_point();
        let font_id = FontId::monospace(self.font_size);
        let (glyph_w, row_h) = ctx.fonts_mut(|fonts| {
            let galley = fonts.layout_no_wrap("M".to_string(), font_id.clone(), Color32::WHITE);
            let w = if !galley.rows.is_empty() && !galley.rows[0].glyphs.is_empty() {
                galley.rows[0].glyphs[0].advance_width
            } else {
                self.font_size * 0.6
            };
            let h = galley.size().y;
            (w, h)
        });
        self.cell_width = (glyph_w * ppp).ceil() / ppp;
        self.cell_height = (row_h * ppp).ceil() / ppp;
        self.cached_font_size = self.font_size;
    }

    pub fn compute_size(&self, cols: usize, rows: usize) -> Vec2 {
        Vec2::new(cols as f32 * self.cell_width, rows as f32 * self.cell_height)
    }

    /// Convenience method: render without fold info
    pub fn show(&mut self, ui: &mut Ui, emulator: &mut TerminalEmulator) -> (Response, bool) {
        let result = self.show_with_size(ui, emulator, None, None);
        (result.response, result.size_changed)
    }

    pub fn show_with_size(
        &mut self,
        ui: &mut Ui,
        emulator: &mut TerminalEmulator,
        size: Option<Vec2>,
        fold_info: Option<&FoldRenderInfo>,
    ) -> TerminalViewResult {
        self.update_cell_metrics(ui.ctx());

        let ppp = ui.ctx().pixels_per_point();

        // Cursor blink (500ms); stop blinking after disconnect
        if self.connected {
            if self.last_blink_toggle.elapsed().as_millis() >= 500 {
                self.cursor_blink_visible = !self.cursor_blink_visible;
                self.last_blink_toggle = std::time::Instant::now();
                ui.ctx().request_repaint();
            }
        } else {
            self.cursor_blink_visible = false;
        }

        let available_size = size.unwrap_or_else(|| ui.max_rect().size());
        let (resp, painter) = ui.allocate_painter(
            available_size,
            Sense::click_and_drag().union(Sense::click()),
        );

        if resp.clicked() {
            resp.request_focus();
        }

        let rect = resp.rect;
        let origin = rect.min.round_to_pixels(ppp);

        let bg_color = Color32::from_rgb(self.theme.bg[0], self.theme.bg[1], self.theme.bg[2]);
        painter.rect_filled(rect, 0.0, bg_color);

        let total_abs_rows = emulator.total_rows();
        let grid_rows = emulator.grid.rows;
        let cursor_row = emulator.grid.cursor_row;
        let cursor_col = emulator.grid.cursor_col;
        // Alt screen (vim/htop/less) manages its own cursor — don't draw ours
        let cursor_visible = self.cursor_blink_visible && emulator.cursor_visible && !emulator.is_alt_screen();
        let sb_len = emulator.scrollback.len();
        // Cursor absolute row
        let cursor_abs = sb_len + cursor_row;

        // Line number column width
        let line_number_width = if self.show_line_numbers {
            let digits = format!("{}", total_abs_rows).len().max(3);
            (digits as f32 * self.cell_width * 0.6 + 8.0).round_to_pixels(ppp)
        } else {
            0.0
        };

        // Fold gutter width
        let fold_gutter_width = if fold_info.is_some() { 16.0_f32.round_to_pixels(ppp) } else { 0.0 };

        self.content_x_offset = line_number_width + fold_gutter_width;
        let content_origin_x = (origin.x + self.content_x_offset).round_to_pixels(ppp);
        let fold_gutter_x = (origin.x + line_number_width).round_to_pixels(ppp);

        let cw = self.cell_width;
        let ch = self.cell_height;
        let max_display_rows = (rect.height() / ch).floor() as usize;

        // --- Build display row list ---
        // display_rows: Vec<abs_row> — absolute row numbers to display top-to-bottom on screen
        let hidden = fold_info.map(|fi| &fi.hidden_rows);

        let display_rows: Vec<usize> = if hidden.is_some() && !hidden.unwrap().is_empty() {
            let hidden_set = hidden.unwrap();
            // With folds: build full list of non-hidden rows, then position window using scroll_offset
            let all_visible: Vec<usize> = (0..total_abs_rows)
                .filter(|r| !hidden_set.contains(r))
                .collect();
            let total_visible = all_visible.len();

            // Convert scroll_offset (based on raw row numbers) to an offset in the visible row list
            // Top row of the raw window
            let orig_bottom = total_abs_rows.saturating_sub(emulator.scroll_offset);
            let orig_top = orig_bottom.saturating_sub(grid_rows);
            // Find the first position in the visible row list that is >= orig_top as the window start
            let win_start = all_visible.partition_point(|&r| r < orig_top);
            let win_end = (win_start + max_display_rows).min(total_visible);
            all_visible[win_start..win_end].to_vec()
        } else {
            // No folds: use original logic
            let bottom_abs = total_abs_rows.saturating_sub(emulator.scroll_offset);
            let top_abs = bottom_abs.saturating_sub(grid_rows);
            (top_abs..bottom_abs).take(max_display_rows).collect()
        };

        // --- Fold click detection variables ---
        let mut fold_left_click: Option<usize> = None;
        let mut fold_right_click: Option<(usize, Pos2)> = None;
        let pointer_pos = ui.ctx().pointer_latest_pos();
        let primary_clicked = ui.ctx().input(|i| i.pointer.primary_clicked());
        let secondary_clicked = ui.ctx().input(|i| i.pointer.secondary_clicked());

        // --- Render all rows ---
        for (display_idx, &abs_row) in display_rows.iter().enumerate() {
            let row = match emulator.get_row(abs_row) {
                Some(r) => r,
                None => continue,
            };
            let y = (origin.y + display_idx as f32 * ch).round_to_pixels(ppp);

            // Line number
            if self.show_line_numbers {
                let line_num = abs_row + 1;
                let num_text = format!("{:>3}", line_num);
                let num_color = Color32::from_rgba_premultiplied(0x60, 0x60, 0x60, 0xff);
                painter.text(
                    Pos2::new((origin.x + 2.0).round_to_pixels(ppp), y),
                    egui::Align2::LEFT_TOP,
                    num_text,
                    FontId::monospace(self.font_size * 0.8),
                    num_color,
                );
            }

            // --- Fold gutter drawing ---
            if let Some(fi) = fold_info {
                self.draw_fold_gutter(
                    &painter, fi, abs_row, fold_gutter_x, y, ch, ppp,
                    pointer_pos, primary_clicked, secondary_clicked,
                    &mut fold_left_click, &mut fold_right_click,
                );
            }

            // --- Render cells ---
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if cell.wide_placeholder { continue; }

                let x = (content_origin_x + col_idx as f32 * cw).round_to_pixels(ppp);

                let is_cursor_cell = abs_row == cursor_abs
                    && col_idx == cursor_col
                    && cursor_visible
                    && emulator.scroll_offset == 0;

                let is_selected = emulator.selection.as_ref().map_or(false, |s| s.is_cell_selected(abs_row, col_idx));
                let is_match = self.search_match_cells.contains(&(abs_row, col_idx));

                let (fg, bg) = self.resolve_colors(cell, is_cursor_cell, is_selected, is_match);

                // Background
                if bg != self.theme.bg {
                    let cell_rect = Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(if cell.wide { cw * 2.0 } else { cw }, ch),
                    );
                    painter.rect_filled(cell_rect, 0.0, Color32::from_rgba_premultiplied(bg[0], bg[1], bg[2], bg[3]));
                }

                // Cursor
                if is_cursor_cell {
                    let cursor_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cw, ch));
                    painter.rect_filled(cursor_rect, 0.0, Color32::from_rgba_premultiplied(
                        self.theme.cursor_color[0],
                        self.theme.cursor_color[1],
                        self.theme.cursor_color[2],
                        200,
                    ));
                }

                // Character
                if cell.ch != ' ' && cell.ch != '\0' {
                    let font = FontId::new(self.font_size, FontFamily::Monospace);
                    painter.text(
                        Pos2::new(x, y),
                        egui::Align2::LEFT_TOP,
                        cell.ch.to_string(),
                        font,
                        Color32::from_rgba_premultiplied(fg[0], fg[1], fg[2], fg[3]),
                    );
                }

                // Underline
                if cell.attrs.underline {
                    let underline_y = (y + ch - 1.0).round_to_pixels(ppp);
                    painter.line_segment(
                        [Pos2::new(x, underline_y), Pos2::new(x + cw, underline_y)],
                        Stroke::new(1.0, Color32::from_rgba_premultiplied(fg[0], fg[1], fg[2], fg[3])),
                    );
                }

                // Strikethrough
                if cell.attrs.strikethrough {
                    let strike_y = (y + ch * 0.5).round_to_pixels(ppp);
                    painter.line_segment(
                        [Pos2::new(x, strike_y), Pos2::new(x + cw, strike_y)],
                        Stroke::new(1.0, Color32::from_rgba_premultiplied(fg[0], fg[1], fg[2], fg[3])),
                    );
                }
            }
        }

        // --- Ghost text suggestion ---
        if let Some(ref suggestion) = self.suggestion {
            if emulator.scroll_offset == 0 && !suggestion.is_empty() {
                if let Some(disp_idx) = display_rows.iter().position(|&r| r == cursor_abs) {
                    let ghost_y = (origin.y + disp_idx as f32 * ch).round_to_pixels(ppp);
                    let ghost_color = Color32::from_rgba_unmultiplied(0x60, 0x60, 0x60, 0xA0);
                    let font = FontId::monospace(self.font_size);

                    for (i, ch_char) in suggestion.chars().enumerate() {
                        let col = cursor_col + i;
                        if col >= emulator.grid.cols { break; }
                        let ghost_x = (content_origin_x + col as f32 * cw).round_to_pixels(ppp);
                        painter.text(
                            Pos2::new(ghost_x, ghost_y),
                            egui::Align2::LEFT_TOP,
                            ch_char.to_string(),
                            font.clone(),
                            ghost_color,
                        );
                    }
                }
            }
        }

        // Terminal size change detection (unaffected by folds, always based on physical size)
        let new_cols = ((rect.width() - line_number_width - fold_gutter_width) / self.cell_width).floor() as usize;
        let new_rows = (rect.height() / self.cell_height).floor() as usize;
        let size_changed = new_cols != emulator.grid.cols || new_rows != emulator.grid.rows;

        TerminalViewResult {
            response: resp,
            size_changed,
            fold_left_click,
            fold_right_click,
        }
    }

    /// Draw the marker for a row in the fold gutter.
    /// Lines run from output_start to output_end (not drawn on the command line itself).
    fn draw_fold_gutter(
        &self,
        painter: &egui::Painter,
        fi: &FoldRenderInfo,
        abs_row: usize,
        gutter_x: f32,
        y: f32,
        ch: f32,
        ppp: f32,
        pointer_pos: Option<Pos2>,
        primary_clicked: bool,
        secondary_clicked: bool,
        fold_left_click: &mut Option<usize>,
        fold_right_click: &mut Option<(usize, Pos2)>,
    ) {
        let gutter_w = 16.0_f32.round_to_pixels(ppp);
        let line_color = Color32::from_rgba_premultiplied(0x58, 0x6e, 0x75, 0x90);
        let icon_color = Color32::from_rgba_premultiplied(0x6c, 0x8e, 0x9c, 0xd0);
        let icon_hover_color = Color32::from_rgba_premultiplied(0x8b, 0xb8, 0xc8, 0xff);
        let icon_bg = Color32::from_rgba_premultiplied(0x30, 0x34, 0x46, 0xe0);

        // Find which block's output range (output_start..=output_end) this row belongs to
        let block = fi.blocks.iter().find(|b| abs_row >= b.output_start && abs_row <= b.output_end);
        let block = match block {
            Some(b) => b,
            None => {
                // When collapsed, the row after prompt_line (output_start) is hidden.
                // Need to show the + icon on prompt_line.
                if let Some(b) = fi.blocks.iter().find(|b| abs_row == b.prompt_line && fi.collapsed.contains(&b.prompt_line)) {
                    // Draw the + icon for collapsed state (on the command line)
                    let center_x = (gutter_x + gutter_w * 0.5).round_to_pixels(ppp);
                    let center_y = (y + ch * 0.5).round_to_pixels(ppp);
                    let icon_size = 9.0_f32.round_to_pixels(ppp);
                    let icon_rect = Rect::from_center_size(
                        Pos2::new(center_x, center_y),
                        Vec2::splat(icon_size),
                    );
                    let is_hovered = pointer_pos.map_or(false, |p| icon_rect.contains(p));
                    let ic = if is_hovered { icon_hover_color } else { icon_color };

                    painter.rect_filled(icon_rect, 2.0, icon_bg);
                    painter.rect_stroke(icon_rect, 2.0, Stroke::new(1.0, ic), StrokeKind::Middle);

                    let h_margin = 2.0_f32.round_to_pixels(ppp);
                    // Horizontal line
                    painter.line_segment(
                        [
                            Pos2::new(icon_rect.min.x + h_margin, center_y),
                            Pos2::new(icon_rect.max.x - h_margin, center_y),
                        ],
                        Stroke::new(1.0, ic),
                    );
                    // Vertical line (+ sign)
                    painter.line_segment(
                        [
                            Pos2::new(center_x, icon_rect.min.y + h_margin),
                            Pos2::new(center_x, icon_rect.max.y - h_margin),
                        ],
                        Stroke::new(1.0, ic),
                    );

                    // Click detection
                    if is_hovered {
                        if primary_clicked {
                            *fold_left_click = Some(b.prompt_line);
                        }
                        if secondary_clicked {
                            if let Some(pos) = pointer_pos {
                                *fold_right_click = Some((b.prompt_line, pos));
                            }
                        }
                    }
                }
                return;
            }
        };

        let center_x = (gutter_x + gutter_w * 0.5).round_to_pixels(ppp);
        let center_y = (y + ch * 0.5).round_to_pixels(ppp);

        // Icon area (for click detection)
        let icon_size = 9.0_f32.round_to_pixels(ppp);
        let icon_rect = Rect::from_center_size(
            Pos2::new(center_x, center_y),
            Vec2::splat(icon_size),
        );

        let is_hovered = pointer_pos.map_or(false, |p| icon_rect.contains(p));

        // Right-click detection (entire gutter row area, all output rows support right-click)
        let row_gutter_rect = Rect::from_min_size(Pos2::new(gutter_x, y), Vec2::new(gutter_w, ch));

        if abs_row == block.output_start {
            // --- First output row: draw ⊟ icon ---
            let ic = if is_hovered { icon_hover_color } else { icon_color };

            painter.rect_filled(icon_rect, 2.0, icon_bg);
            painter.rect_stroke(icon_rect, 2.0, Stroke::new(1.0, ic), StrokeKind::Middle);

            // Horizontal line (minus sign)
            let h_margin = 2.0_f32.round_to_pixels(ppp);
            painter.line_segment(
                [
                    Pos2::new(icon_rect.min.x + h_margin, center_y),
                    Pos2::new(icon_rect.max.x - h_margin, center_y),
                ],
                Stroke::new(1.0, ic),
            );

            // Draw vertical line from icon bottom to row bottom (connecting to output rows below)
            if block.output_start < block.output_end {
                painter.line_segment(
                    [
                        Pos2::new(center_x, icon_rect.max.y),
                        Pos2::new(center_x, y + ch),
                    ],
                    Stroke::new(1.0, line_color),
                );
            }

            // Click detection
            if is_hovered {
                if primary_clicked {
                    *fold_left_click = Some(block.prompt_line);
                }
                if secondary_clicked {
                    if let Some(pos) = pointer_pos {
                        *fold_right_click = Some((block.prompt_line, pos));
                    }
                }
            }
        } else if abs_row == block.output_end {
            // --- Last output row: └ corner ---
            painter.line_segment(
                [
                    Pos2::new(center_x, y),
                    Pos2::new(center_x, center_y),
                ],
                Stroke::new(1.0, line_color),
            );
            painter.line_segment(
                [
                    Pos2::new(center_x, center_y),
                    Pos2::new(gutter_x + gutter_w - 2.0, center_y),
                ],
                Stroke::new(1.0, line_color),
            );

            if let Some(pos) = pointer_pos {
                if row_gutter_rect.contains(pos) && secondary_clicked {
                    *fold_right_click = Some((block.prompt_line, pos));
                }
            }
        } else {
            // --- Middle output rows: vertical line │ ---
            painter.line_segment(
                [
                    Pos2::new(center_x, y),
                    Pos2::new(center_x, y + ch),
                ],
                Stroke::new(1.0, line_color),
            );

            if let Some(pos) = pointer_pos {
                if row_gutter_rect.contains(pos) && secondary_clicked {
                    *fold_right_click = Some((block.prompt_line, pos));
                }
            }
        }
    }

    /// Handle mouse selection (drag, double-click)
    pub fn handle_mouse_selection(&self, ui: &Ui, terminal_rect: egui::Rect, origin: Pos2, emulator: &mut TerminalEmulator) {
        let pointer_pos = ui.ctx().pointer_latest_pos().unwrap_or_default();

        if !terminal_rect.contains(pointer_pos) {
            return;
        }

        let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
        let primary_pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
        let double_clicked = ui.ctx().input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary));

        if double_clicked {
            let (row, col) = self.pixel_to_cell(pointer_pos, origin, emulator);
            let (word_start, word_end) = emulator.word_bounds_at(row, col);
            emulator.start_selection(row, word_start, SelectionMode::Word);
            emulator.extend_selection(row, word_end);
        } else if primary_pressed {
            let (row, col) = self.pixel_to_cell(pointer_pos, origin, emulator);
            emulator.start_selection(row, col, SelectionMode::Character);
        } else if primary_down {
            let (row, col) = self.pixel_to_cell(pointer_pos, origin, emulator);
            // When dragging in Word mode, extend to word boundary
            if emulator.selection.as_ref().map_or(false, |s| s.mode == SelectionMode::Word) {
                let (_, word_end) = emulator.word_bounds_at(row, col);
                emulator.extend_selection(row, word_end);
            } else {
                emulator.extend_selection(row, col);
            }

            // Auto-scroll
            let margin = self.cell_height * 2.0;
            if pointer_pos.y < terminal_rect.min.y + margin {
                emulator.scroll_by(1);
            } else if pointer_pos.y > terminal_rect.max.y - margin {
                emulator.scroll_by(-1);
            }
        }
    }

    /// Convert pixel position to (absolute_row, col)
    fn pixel_to_cell(&self, pos: Pos2, origin: Pos2, emulator: &TerminalEmulator) -> (usize, usize) {
        let row = ((pos.y - origin.y) / self.cell_height).floor() as usize;
        let col = ((pos.x - origin.x - self.content_x_offset) / self.cell_width).floor().max(0.0) as usize;
        let abs_row = emulator.visible_start() + row.min(emulator.grid.rows.saturating_sub(1));
        let col = col.min(emulator.grid.cols.saturating_sub(1));
        (abs_row, col)
    }

    /// Resolve foreground and background colors for a cell
    fn resolve_colors(&self, cell: &Cell, is_cursor: bool, is_selected: bool, is_match: bool) -> ([u8; 4], [u8; 4]) {
        let mut fg = cell.fg.to_rgba(true, &self.theme.colors, self.theme.fg, self.theme.bg);
        let mut bg = cell.bg.to_rgba(false, &self.theme.colors, self.theme.fg, self.theme.bg);

        if cell.attrs.reverse {
            std::mem::swap(&mut fg, &mut bg);
        }

        // No bold→bright color mapping; all text uses normal weight uniformly

        if cell.attrs.dim {
            fg[0] = (fg[0] as f32 * 0.5) as u8;
            fg[1] = (fg[1] as f32 * 0.5) as u8;
            fg[2] = (fg[2] as f32 * 0.5) as u8;
        }

        if is_match {
            bg = self.theme.match_highlight_color;
        }

        if is_selected {
            bg = self.theme.selection_color;
        }

        if is_cursor {
            bg = self.theme.cursor_color;
            bg[3] = 255;
            fg = self.theme.bg;
        }

        (fg, bg)
    }
}
