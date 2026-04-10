use egui::{Response, Sense, Ui, Vec2};

/// Draw an emoji SVG image in the given size area and return a clickable Response.
/// Does not use EmojiLabel to avoid internal interactive widgets stealing click events.
pub fn emoji_button(ui: &mut Ui, emoji: &str, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        if let Some(svg_data) = twemoji_assets::svg::SvgTwemojiAsset::from_emoji(emoji) {
            let source = egui::ImageSource::Bytes {
                uri: format!("{emoji}.svg").into(),
                bytes: egui::load::Bytes::Static(svg_data.as_bytes()),
            };
            let inner = rect.shrink(4.0);
            let img = egui::Image::new(source).fit_to_exact_size(inner.size());
            img.paint_at(ui, inner);
        }
    }
    resp
}
