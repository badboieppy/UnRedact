use std::path::Path;

use lopdf::Document;
use unredact::redaction_finder::dependency::hayro_renderer::HayroRenderer;
use unredact::redaction_finder::service::redaction_finder_entry::{
    find_redactions_in_pdf_bytes_vector_only, find_redactions_in_pdf_bytes_with_renderer,
    find_redactions_in_pdf_path_with_renderer,
};
use unredact::redaction_finder::types::{
    PdfRenderer, RedactionFinderConfig, RedactionKind, RenderedPage,
};

#[derive(Clone)]
struct FakeRenderer {
    page_count: usize,
    page: RenderedPage,
}

impl PdfRenderer for FakeRenderer {
    fn page_count(&self) -> usize {
        self.page_count
    }

    fn render_page_to_rgba(
        &self,
        page_index: usize,
        _target_dpi: f32,
    ) -> Result<RenderedPage, String> {
        if page_index >= self.page_count {
            return Err(format!(
                "page_out_of_bounds:index={} page_count={}",
                page_index, self.page_count
            ));
        }
        Ok(self.page.clone())
    }
}

fn synthetic_rendered_page() -> RenderedPage {
    let width_px = 220u32;
    let height_px = 140u32;
    let mut pixels = vec![230u8; (width_px as usize) * (height_px as usize) * 4];

    for px in pixels.chunks_exact_mut(4) {
        px[3] = 255;
    }

    for y in 44usize..94usize {
        for x in 30usize..190usize {
            let idx = (y * (width_px as usize) + x) * 4;
            pixels[idx] = 4;
            pixels[idx + 1] = 4;
            pixels[idx + 2] = 4;
            pixels[idx + 3] = 255;
        }
    }

    RenderedPage {
        width_px,
        height_px,
        dpi: 200.0,
        pixels,
    }
}

#[test]
fn bytes_api_with_renderer_detects_raster_region_on_real_pdf() {
    let path = Path::new("test_data/EFTA02238592.pdf");
    let bytes = std::fs::read(path).unwrap();
    let page_count = Document::load_mem(&bytes).unwrap().get_pages().len();

    let renderer = FakeRenderer {
        page_count,
        page: synthetic_rendered_page(),
    };
    let cfg = RedactionFinderConfig {
        enable_image_analysis: true,
        ..RedactionFinderConfig::default()
    };

    let output = find_redactions_in_pdf_bytes_with_renderer(&bytes, &renderer, cfg).unwrap();
    assert!(output
        .redactions
        .iter()
        .any(|r| matches!(r.kind, RedactionKind::RasterDarkRegion)));
}

#[test]
fn vector_only_api_parses_real_pdf() {
    let path = Path::new("test_data/EFTA02238592.pdf");
    let bytes = std::fs::read(path).unwrap();
    let cfg = RedactionFinderConfig {
        enable_image_analysis: false,
        ..RedactionFinderConfig::default()
    };

    let output_a = find_redactions_in_pdf_bytes_vector_only(&bytes, cfg).unwrap();
    let output_b = find_redactions_in_pdf_bytes_vector_only(&bytes, cfg).unwrap();

    assert_eq!(output_a.redactions.len(), output_b.redactions.len());
    assert_eq!(output_a.diagnostics, output_b.diagnostics);
}

#[test]
fn hayro_renderer_real_pdf_smoke_if_available() {
    if !HayroRenderer::is_available() {
        return;
    }

    let path = Path::new("test_data/EFTA02238592.pdf");
    let renderer = HayroRenderer::new(path).unwrap();
    let cfg = RedactionFinderConfig {
        enable_image_analysis: true,
        ..RedactionFinderConfig::default()
    };

    let output = find_redactions_in_pdf_path_with_renderer(path, &renderer, cfg).unwrap();
    assert!(!output
        .diagnostics
        .iter()
        .any(|d| d.contains("raster_page_error=")));
}
