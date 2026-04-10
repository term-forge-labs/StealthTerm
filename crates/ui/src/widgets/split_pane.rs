use egui::{Ui, Vec2};

#[derive(Debug, Clone, PartialEq)]
pub enum SplitDirection {
    /// Horizontal split (left/right)
    Horizontal,
    /// Vertical split (top/bottom)
    Vertical,
}

pub struct SplitPane {
    pub direction: SplitDirection,
    /// 0.0..1.0 ratio of first pane
    pub ratio: f32,
    pub dragging: bool,
}

impl SplitPane {
    pub fn new(direction: SplitDirection) -> Self {
        Self {
            direction,
            ratio: 0.5,
            dragging: false,
        }
    }

    pub fn show<A, B>(
        &mut self,
        ui: &mut Ui,
        first: impl FnOnce(&mut Ui) -> A,
        second: impl FnOnce(&mut Ui) -> B,
    ) -> (A, B) {
        let available = ui.available_size();
        let divider_width = 4.0;
        let divider_color = ui.visuals().panel_fill;

        match self.direction {
            SplitDirection::Horizontal => {
                let first_width = available.x * self.ratio - divider_width / 2.0;
                let second_width = available.x * (1.0 - self.ratio) - divider_width / 2.0;

                let mut a_opt: Option<A> = None;
                let mut b_opt: Option<B> = None;

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    let first_result = ui.allocate_ui(Vec2::new(first_width, available.y), |ui| first(ui));
                    a_opt = Some(first_result.inner);

                    // Divider
                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(divider_width, available.y),
                        egui::Sense::drag(),
                    );
                    if response.dragged() {
                        let delta = response.drag_delta().x / available.x;
                        self.ratio = (self.ratio + delta).clamp(0.1, 0.9);
                    }
                    ui.painter().rect_filled(rect, 0.0, divider_color);

                    let second_result = ui.allocate_ui(Vec2::new(second_width, available.y), |ui| second(ui));
                    b_opt = Some(second_result.inner);
                });

                (a_opt.unwrap(), b_opt.unwrap())
            }
            SplitDirection::Vertical => {
                let first_height = available.y * self.ratio - divider_width / 2.0;
                let second_height = available.y * (1.0 - self.ratio) - divider_width / 2.0;

                let mut a_opt: Option<A> = None;
                let mut b_opt: Option<B> = None;

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;

                    let first_result = ui.allocate_ui(Vec2::new(available.x, first_height), |ui| first(ui));
                    a_opt = Some(first_result.inner);

                    let (rect, response) = ui.allocate_exact_size(
                        Vec2::new(available.x, divider_width),
                        egui::Sense::drag(),
                    );
                    if response.dragged() {
                        let delta = response.drag_delta().y / available.y;
                        self.ratio = (self.ratio + delta).clamp(0.1, 0.9);
                    }
                    ui.painter().rect_filled(rect, 0.0, divider_color);

                    let second_result = ui.allocate_ui(Vec2::new(available.x, second_height), |ui| second(ui));
                    b_opt = Some(second_result.inner);
                });

                (a_opt.unwrap(), b_opt.unwrap())
            }
        }
    }
}
