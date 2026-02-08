use crate::redaction_finder::types::Rect;

#[derive(Debug, Clone, PartialEq)]
pub struct TextOverlay {
    pub page_index: u32,
    pub text: String,
    pub font_key: String,
    pub font_size_pt: f32,
    pub x: f32,
    pub y: f32,
    pub bbox: Rect,
}
