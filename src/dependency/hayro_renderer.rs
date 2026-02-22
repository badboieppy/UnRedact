use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, RenderSettings};

use crate::types::redaction_types::{PdfRenderer, RenderedPage};

pub struct HayroRenderer {
    pdf: Pdf,
    page_count: usize,
}

impl HayroRenderer {
    #[inline]
    pub fn new_from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let data: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(bytes.to_vec());
        let pdf = Pdf::new(data).map_err(|e| format!("hayro_pdf_load_failed:{e:?}"))?;
        let page_count = pdf.pages().len();
        if page_count == 0 {
            return Err("pdf contains zero pages".to_owned());
        }
        Ok(Self { pdf, page_count })
    }
}

impl PdfRenderer for HayroRenderer {
    #[inline]
    fn page_count(&self) -> usize {
        self.page_count
    }

    #[inline]
    fn render_page_to_rgba(
        &self,
        page_index: usize,
        target_dpi: f32,
    ) -> Result<RenderedPage, String> {
        if !target_dpi.is_finite() || target_dpi <= 0.0 {
            return Err(format!("invalid_dpi:{target_dpi}"));
        }
        if page_index >= self.page_count {
            return Err(format!(
                "page_out_of_bounds:index={} page_count={}",
                page_index, self.page_count
            ));
        }

        let page = self
            .pdf
            .pages()
            .get(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;

        // PDF user space uses 72 units per inch, so scale proportionally to DPI.
        let scale = target_dpi / 72.0;
        let render_settings = RenderSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
            ..RenderSettings::default()
        };
        let interpreter_settings = InterpreterSettings::default();

        let pixmap = render(page, &interpreter_settings, &render_settings);
        let width_px = pixmap.width() as u32;
        let height_px = pixmap.height() as u32;
        let pixels = pixmap.data_as_u8_slice().to_vec();

        Ok(RenderedPage {
            width_px,
            height_px,
            dpi: target_dpi,
            pixels,
        })
    }
}
