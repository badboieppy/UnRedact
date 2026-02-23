use lopdf::{Document, ObjectId};
use std::collections::BTreeMap;

use crate::types::redaction_types::{
    PdfRenderer, RedactionFinderConfig, RedactionOccurrence, UnderlyingTextHit,
};

use crate::dependency::pdf_redaction::{
    extract_annotation_redactions, extract_page_drawn_redactions, extract_page_text_runs,
    extract_raster_page_redactions, page_render_box_from_page, DetailPolicy, DrawScanOptions,
};

pub trait RedactionDataRetriever {
    fn page_indices(&self) -> Vec<u32>;
    fn annotation_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn drawn_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn raster_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String>;
}

pub struct PdfFileRetriever<'renderer> {
    doc: Document,
    page_map: BTreeMap<u32, ObjectId>,
    renderer: Option<&'renderer dyn PdfRenderer>,
}

impl<'renderer> PdfFileRetriever<'renderer> {
    pub fn new_from_bytes(
        bytes: &[u8],
        renderer: Option<&'renderer dyn PdfRenderer>,
    ) -> Result<Self, String> {
        let doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
        Self::new(doc, renderer)
    }

    pub fn new(
        doc: Document,
        renderer: Option<&'renderer dyn PdfRenderer>,
    ) -> Result<Self, String> {
        let pages = doc.get_pages();
        let mut page_map = BTreeMap::new();
        for (page_number, object_id) in pages {
            let index = page_number.saturating_sub(1);
            page_map.insert(index, object_id);
        }

        Ok(Self {
            doc,
            page_map,
            renderer,
        })
    }

    fn page_id(&self, page_index: u32) -> Result<ObjectId, String> {
        self.page_map
            .get(&page_index)
            .copied()
            .ok_or_else(|| format!("Page index {} not found", page_index))
    }
}

impl<'renderer> RedactionDataRetriever for PdfFileRetriever<'renderer> {
    fn page_indices(&self) -> Vec<u32> {
        self.page_map.keys().copied().collect()
    }

    fn annotation_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        let pid = self.page_id(page_index)?;
        let detail = DetailPolicy::new(config.include_details);
        extract_annotation_redactions(&self.doc, pid, page_index, detail)
    }

    fn drawn_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        let pid = self.page_id(page_index)?;
        let opts = DrawScanOptions::page_level(config.include_details, config.include_full_page_rects);
        extract_page_drawn_redactions(&self.doc, pid, page_index, opts)
    }

    fn raster_redactions(
        &self,
        page_index: u32,
        config: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        if !config.enable_image_analysis {
            return Ok(Vec::new());
        }
        let renderer = match self.renderer {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        let pid = self.page_id(page_index)?;
        let page_box =
            page_render_box_from_page(&self.doc, pid).unwrap_or(crate::types::redaction_types::Rect::new(
                0.0, 0.0, 612.0, 792.0,
            ));
        extract_raster_page_redactions(renderer, page_index, page_box, &config)
    }

    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String> {
        let pid = self.page_id(page_index)?;
        extract_page_text_runs(&self.doc, pid, page_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::redaction_types::{PdfRenderer, RenderedPage};
    use lopdf::{Dictionary, Object, Stream};

    struct FakeRenderer {
        page_count: usize,
        page: RenderedPage,
    }

    impl PdfRenderer for FakeRenderer {
        fn page_count(&self) -> usize {
            self.page_count
        }
        fn render_page_to_rgba(&self, _page_index: usize, dpi: f32) -> Result<RenderedPage, String> {
            let mut p = self.page.clone();
            p.dpi = dpi;
            Ok(p)
        }
    }

    fn synthetic_rendered_page(w: u32, h: u32, color: u32) -> RenderedPage {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        let r = ((color >> 16_u32) & 0xFF_u32) as u8;
        let g = ((color >> 8_u32) & 0xFF_u32) as u8;
        let b = (color & 0xFF_u32) as u8;
        for _ in 0..(w * h) {
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(255);
        }
        RenderedPage {
            width_px: w,
            height_px: h,
            dpi: 72.0,
            pixels: data,
        }
    }

    #[test]
    fn retriever_provides_underlying_text_hits_for_real_pdf() {
        // Create a minimal PDF with some text
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Font".into()),
            ("Subtype", "Type1".into()),
            ("BaseFont", "Helvetica".into()),
        ]));
        let resources_id = doc.add_object(Dictionary::from_iter(vec![(
            "Font",
            Dictionary::from_iter(vec![("F1", font_id.into())]).into(),
        )]));
        let content = Stream::new(
            Dictionary::new(),
            b"BT /F1 24 Tf 100 100 Td (Hello World) Tj ET".to_vec(),
        );
        let content_id = doc.add_object(content);
        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            ("Resources", resources_id.into()),
            (
                "MediaBox",
                vec![0_i32.into(), 0_i32.into(), 595_i32.into(), 842_i32.into()].into(),
            ),
            ("Contents", content_id.into()),
        ]));
        let pages = Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1_i32.into()),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id.into()),
        ]));
        doc.trailer.set("Root", catalog_id);

        let retriever = PdfFileRetriever::new(doc, None)
            .expect("retriever construction should succeed for synthetic one-page PDF");
        let hits = retriever
            .underlying_text_hits(0)
            .expect("text extraction should succeed for synthetic one-page PDF");
        assert!(!hits.is_empty());
        assert_eq!(hits[0].text, "Hello World");
    }

    #[test]
    fn retriever_raster_redactions_are_deterministic_for_same_input() {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            (
                "MediaBox",
                vec![0_i32.into(), 0_i32.into(), 100_i32.into(), 100_i32.into()].into(),
            ),
        ]));
        let pages = Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1_i32.into()),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id.into()),
        ]));
        doc.trailer.set("Root", catalog_id);

        let fake_renderer = FakeRenderer {
            page_count: 1,
            page: synthetic_rendered_page(100, 100, 0x000000), // black
        };

        let retriever = PdfFileRetriever::new(doc, Some(&fake_renderer))
            .expect("retriever construction should succeed for synthetic raster PDF");
        let config = RedactionFinderConfig {
            include_details: true,
            mode: crate::types::redaction_types::RedactionMode::All,
            include_full_page_rects: false,
            enable_image_analysis: true,
            raster_dpi: 72.0,
        };

        let result1 = retriever
            .raster_redactions(0, config)
            .expect("first raster pass should succeed for synthetic raster PDF");
        let result2 = retriever
            .raster_redactions(0, config)
            .expect("second raster pass should succeed for synthetic raster PDF");

        assert_eq!(result1.len(), result2.len());
        if !result1.is_empty() {
            assert_eq!(result1[0].score, result2[0].score);
        }
    }

    #[test]
    fn retriever_annotation_and_drawn_redactions_are_deterministic() {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Page".into()),
            ("Parent", pages_id.into()),
            (
                "MediaBox",
                vec![0_i32.into(), 0_i32.into(), 100_i32.into(), 100_i32.into()].into(),
            ),
        ]));
        let pages = Dictionary::from_iter(vec![
            ("Type", "Pages".into()),
            ("Kids", vec![page_id.into()].into()),
            ("Count", 1_i32.into()),
        ]);
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(Dictionary::from_iter(vec![
            ("Type", "Catalog".into()),
            ("Pages", pages_id.into()),
        ]));
        doc.trailer.set("Root", catalog_id);

        let retriever = PdfFileRetriever::new(doc, None)
            .expect("retriever construction should succeed for synthetic one-page PDF");
        let config = RedactionFinderConfig {
            include_details: true,
            mode: crate::types::redaction_types::RedactionMode::All,
            include_full_page_rects: false,
            enable_image_analysis: false,
            raster_dpi: 72.0,
        };

        let ann1 = retriever
            .annotation_redactions(0, config)
            .expect("first annotation scan should succeed for synthetic one-page PDF");
        let ann2 = retriever
            .annotation_redactions(0, config)
            .expect("second annotation scan should succeed for synthetic one-page PDF");
        assert_eq!(ann1.len(), ann2.len());

        let drn1 = retriever
            .drawn_redactions(0, config)
            .expect("first drawn-path scan should succeed for synthetic one-page PDF");
        let drn2 = retriever
            .drawn_redactions(0, config)
            .expect("second drawn-path scan should succeed for synthetic one-page PDF");
        assert_eq!(drn1.len(), drn2.len());
    }
}
