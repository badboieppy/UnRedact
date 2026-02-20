use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VisualizerConfig {
    pub color: [f32; 3],
    pub text_color: [f32; 3],
    pub border_width: f32,
}

impl Default for VisualizerConfig {
    #[inline]
    fn default() -> Self {
        Self {
            color: [1.0, 0.0, 0.0],
            text_color: [0.0, 0.4, 1.0],
            border_width: 1.0,
        }
    }
}
