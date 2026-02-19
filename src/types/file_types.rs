use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontProcessInput {
    pub inputs: Vec<PathBuf>,
    pub output: Option<PathBuf>,
    pub format: OutputFormat,
    pub include_details: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedOutput {
    pub path: Option<PathBuf>,
    pub format: OutputFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputFileKind {
    Pdf,
    Image,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSourceKind {
    EmbeddedText,
    ImageRaster,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontDetectionReport {
    pub inputs: Vec<FileFontReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontRunReport {
    pub input: String,
    pub runs: Vec<FontTextRun>,
    pub assets: Vec<FontAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontTextRun {
    pub page_index: u32,
    pub text: String,
    pub bbox: Rect,
    pub font_key: String,
    pub font_name: String,
    pub font_size_pt: f32,
    #[serde(default = "default_h_scale_pct")]
    pub h_scale_pct: f32,
    #[serde(default)]
    pub measured_width_pt: Option<f32>,
    #[serde(default)]
    pub measured_width_px: Option<f32>,
    #[serde(default)]
    pub measured_dpi: Option<f32>,
    #[serde(default)]
    pub char_advances_pt: Vec<f32>,
    #[serde(default)]
    pub char_advances_px: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontAsset {
    pub font_key: String,
    pub font_name: String,
    pub units_per_em: u16,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileFontReport {
    pub path: String,
    pub kind: InputFileKind,
    pub text_source: TextSourceKind,
    pub fonts: FontsFound,
    pub occurrences: Option<FontOccurrences>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontsFound {
    pub distinct: Vec<FontId>,
    pub counts: Vec<FontCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontCount {
    pub font: FontId,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FontId {
    pub family: String,
    pub variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontOccurrences {
    pub items: Vec<FontOccurrence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontOccurrence {
    pub font: FontId,
    pub location: DocumentLocation,
    pub text: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentLocation {
    pub page_index: Option<u32>,
    pub region: Region,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub bbox: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    #[inline]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }
}

fn default_h_scale_pct() -> f32 {
    100.0_f32
}

#[inline]
pub fn distinct_fonts_from_counts(counts: &[FontCount]) -> Vec<FontId> {
    counts
        .iter()
        .map(|c| c.font.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
}

#[inline]
pub fn aggregate_counts(occurrences: &[FontOccurrence]) -> Vec<FontCount> {
    let m = occurrences
        .iter()
        .fold(BTreeMap::<FontId, u32>::new(), |mut acc, o| {
            *acc.entry(o.font.clone()).or_insert(0) += 1;
            acc
        });

    m.into_iter()
        .map(|(font, count)| FontCount { font, count })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_serializes_json() {
        let fmt = OutputFormat::Json;
        let s = serde_json::to_string(&fmt).expect("expected value in test");
        assert_eq!(s, "\"json\"");
    }

    #[test]
    fn rect_dimensions() {
        let r = Rect::new(1.0, 2.0, 6.5, 10.0);
        assert_eq!(r.x1 - r.x0, 5.5);
        assert_eq!(r.y1 - r.y0, 8.0);
    }

    #[test]
    fn font_id_ordering_is_deterministic() {
        let a = FontId {
            family: "Arial".to_owned(),
            variant: None,
        };
        let b = FontId {
            family: "Calibri".to_owned(),
            variant: Some("Bold".to_owned()),
        };
        let mut v = vec![b.clone(), a.clone()];
        v.sort();
        assert_eq!(v, vec![a, b]);
    }

    #[test]
    fn distinct_fonts_from_counts_deduplicates_and_sorts() {
        let arial = FontId {
            family: "Arial".to_owned(),
            variant: None,
        };
        let calibri = FontId {
            family: "Calibri".to_owned(),
            variant: None,
        };
        let counts = vec![
            FontCount {
                font: calibri.clone(),
                count: 2,
            },
            FontCount {
                font: arial.clone(),
                count: 1,
            },
            FontCount {
                font: calibri.clone(),
                count: 9,
            },
        ];

        let out = distinct_fonts_from_counts(&counts);
        assert_eq!(out, vec![arial, calibri]);
    }

    fn counts_as_map(counts: &[FontCount]) -> BTreeMap<FontId, u32> {
        counts.iter().map(|c| (c.font.clone(), c.count)).collect()
    }

    #[test]
    fn counts_as_map_builds_expected_map() {
        let arial = FontId {
            family: "Arial".to_owned(),
            variant: None,
        };
        let calibri = FontId {
            family: "Calibri".to_owned(),
            variant: Some("Bold".to_owned()),
        };
        let counts = vec![
            FontCount {
                font: calibri.clone(),
                count: 2,
            },
            FontCount {
                font: arial.clone(),
                count: 1,
            },
        ];

        let m = counts_as_map(&counts);
        assert_eq!(m.get(&arial).copied(), Some(1));
        assert_eq!(m.get(&calibri).copied(), Some(2));
    }

    #[test]
    fn aggregate_counts_counts_occurrences() {
        let arial = FontId {
            family: "Arial".to_owned(),
            variant: None,
        };
        let calibri = FontId {
            family: "Calibri".to_owned(),
            variant: None,
        };

        let occurrences = vec![
            FontOccurrence {
                font: arial.clone(),
                location: DocumentLocation {
                    page_index: Some(0),
                    region: Region {
                        bbox: Rect::new(0.0, 0.0, 1.0, 1.0),
                    },
                },
                text: Some("a".to_owned()),
                confidence: Some(0.9),
            },
            FontOccurrence {
                font: arial.clone(),
                location: DocumentLocation {
                    page_index: Some(0),
                    region: Region {
                        bbox: Rect::new(1.0, 0.0, 2.0, 1.0),
                    },
                },
                text: Some("b".to_owned()),
                confidence: Some(0.8),
            },
            FontOccurrence {
                font: calibri.clone(),
                location: DocumentLocation {
                    page_index: Some(1),
                    region: Region {
                        bbox: Rect::new(0.0, 1.0, 2.0, 2.0),
                    },
                },
                text: None,
                confidence: None,
            },
        ];

        let counts = aggregate_counts(&occurrences);
        let m = counts_as_map(&counts);

        assert_eq!(m.get(&arial).copied(), Some(2));
        assert_eq!(m.get(&calibri).copied(), Some(1));
    }

    #[test]
    fn font_process_input_is_eq() {
        let a = FontProcessInput {
            inputs: vec![PathBuf::from("a.pdf")],
            output: None,
            format: OutputFormat::Json,
            include_details: true,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn encoded_output_is_eq() {
        let a = EncodedOutput {
            path: Some(PathBuf::from("out.json")),
            format: OutputFormat::Json,
            bytes: b"{}\n".to_vec(),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn report_roundtrips_json() {
        let arial = FontId {
            family: "Arial".to_owned(),
            variant: None,
        };

        let report = FontDetectionReport {
            inputs: vec![FileFontReport {
                path: "x.pdf".to_owned(),
                kind: InputFileKind::Pdf,
                text_source: TextSourceKind::EmbeddedText,
                fonts: FontsFound {
                    distinct: vec![arial.clone()],
                    counts: vec![FontCount {
                        font: arial.clone(),
                        count: 1,
                    }],
                },
                occurrences: Some(FontOccurrences {
                    items: vec![FontOccurrence {
                        font: arial,
                        location: DocumentLocation {
                            page_index: Some(0),
                            region: Region {
                                bbox: Rect::new(1.0, 2.0, 3.0, 4.0),
                            },
                        },
                        text: Some("Hi".to_owned()),
                        confidence: None,
                    }],
                }),
            }],
        };

        let s = serde_json::to_string(&report).expect("expected value in test");
        let decoded: FontDetectionReport =
            serde_json::from_str(&s).expect("expected value in test");
        assert_eq!(decoded, report);
    }

    #[test]
    fn font_text_run_defaults_metric_fields_when_missing() {
        let json = r#"{
            "page_index": 0,
            "text": "Sample",
            "bbox": { "x0": 1.0, "y0": 2.0, "x1": 7.0, "y1": 10.0 },
            "font_key": "F1",
            "font_name": "Helvetica",
            "font_size_pt": 12.0,
            "h_scale_pct": 100.0
        }"#;
        let run: FontTextRun = serde_json::from_str(json).expect("expected value in test");
        assert!(run.measured_width_pt.is_none());
        assert!(run.measured_width_px.is_none());
        assert!(run.measured_dpi.is_none());
        assert!(run.char_advances_pt.is_empty());
        assert!(run.char_advances_px.is_empty());
    }

    #[test]
    fn text_source_kind_serializes_image_raster() {
        let value = TextSourceKind::ImageRaster;
        let json = serde_json::to_string(&value).expect("expected value in test");
        assert_eq!(json, "\"image_raster\"");
    }
}
