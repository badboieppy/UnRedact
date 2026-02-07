use crate::font_detection::dependency::file_accessor::{FileAccessor, FileReadRequest};
use crate::font_detection::logic::file_font_process::{
    build_occurrence, classify_kind_from_path, default_text_source_for_kind, font_id_from_name,
    normalize_subset_font_name,
};
use crate::font_detection::logic::types::file_types::{
    FileFontReport, FontOccurrence, FontOccurrences, FontsFound, InputFileKind, Rect,
};
use lopdf::{Document, Object};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataBuildConfig {
    pub include_details: bool,
}

pub struct FileDataBuilder<'accessor> {
    accessor: &'accessor dyn FileAccessor,
}

impl<'accessor> FileDataBuilder<'accessor> {
    #[inline]
    pub fn new(accessor: &'accessor dyn FileAccessor) -> Self {
        Self { accessor }
    }
}

#[inline]
pub fn build_file_font_report(
    builder: &FileDataBuilder<'_>,
    path: &Path,
    _config: DataBuildConfig,
) -> Result<FileFontReport, String> {
    let kind = classify_kind_from_path(path);
    let text_source = default_text_source_for_kind(kind);
    let occurrences = Some(extract_occurrences(builder, path, kind)?);

    Ok(FileFontReport {
        path: path.to_string_lossy().to_string(),
        kind,
        text_source,
        fonts: FontsFound {
            distinct: vec![],
            counts: vec![],
        },
        occurrences,
    })
}

fn extract_occurrences(
    builder: &FileDataBuilder<'_>,
    path: &Path,
    kind: InputFileKind,
) -> Result<FontOccurrences, String> {
    match kind {
        InputFileKind::Pdf => extract_pdf_occurrences(builder, path),
        InputFileKind::Image => Ok(FontOccurrences { items: vec![] }),
        InputFileKind::Unknown => Ok(FontOccurrences { items: vec![] }),
    }
}

fn extract_pdf_occurrences(
    builder: &FileDataBuilder<'_>,
    path: &Path,
) -> Result<FontOccurrences, String> {
    let bytes = builder
        .accessor
        .read(FileReadRequest {
            path: path.to_path_buf(),
        })?
        .bytes;

    let doc = Document::load_mem(&bytes).map_err(|error| error.to_string())?;
    let pages = doc.get_pages();

    let items = pages
        .into_iter()
        .map(|(page_no, page_id)| extract_pdf_page_occurrences(&doc, page_no, page_id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    Ok(FontOccurrences { items })
}

fn extract_pdf_page_occurrences(
    doc: &Document,
    page_no: u32,
    page_id: lopdf::ObjectId,
) -> Result<Vec<FontOccurrence>, String> {
    let fonts = extract_pdf_page_fonts(doc, page_id)?;
    let content = doc
        .get_page_content(page_id)
        .map_err(|error| error.to_string())?;
    let decoded =
        lopdf::content::Content::decode(&content).map_err(|error| error.to_string())?;
    Ok(occurrences_from_ops(
        page_no.saturating_sub(1),
        &decoded.operations,
        &fonts,
    ))
}

fn extract_pdf_page_fonts(
    doc: &Document,
    page_id: lopdf::ObjectId,
) -> Result<BTreeMap<String, String>, String> {
    let (page_resources_opt, _unused_pages) = doc
        .get_page_resources(page_id)
        .map_err(|error| error.to_string())?;
    let resources = match page_resources_opt {
        None => return Ok(BTreeMap::new()),
        Some(resources_dict) => resources_dict,
    };

    let font_obj_opt = resources.get(b"Font").ok();
    let font_obj = match font_obj_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_object) => font_object,
    };

    let font_dict_opt = deref_to_dict(doc, font_obj).or_else(|| object_to_dict(font_obj));
    let font_dict = match font_dict_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_dictionary) => font_dictionary,
    };

    let map = font_dict
        .iter()
        .map(|(key_bytes, value_object)| {
            let key = String::from_utf8_lossy(key_bytes).to_string();
            let name = resolve_pdf_font_name(doc, value_object).unwrap_or_else(|| key.clone());
            (key, name)
        })
        .collect::<BTreeMap<_, _>>();

    Ok(map)
}

fn resolve_pdf_font_name(doc: &Document, font_obj: &Object) -> Option<String> {
    let dict = deref_to_dict(doc, font_obj)?;
    let base = dict.get(b"BaseFont").ok().and_then(object_to_name_string);
    if let Some(base_font_name) = base {
        return Some(normalize_subset_font_name(&base_font_name));
    }

    let desc_obj = dict.get(b"FontDescriptor").ok();
    let desc_dict_opt = desc_obj.and_then(|descriptor_object| deref_to_dict(doc, descriptor_object));
    let desc_dict = desc_dict_opt?;
    let name = desc_dict
        .get(b"FontName")
        .ok()
        .and_then(object_to_name_string)?;
    Some(normalize_subset_font_name(&name))
}

fn occurrences_from_ops(
    page_index: u32,
    ops: &[lopdf::content::Operation],
    fonts: &BTreeMap<String, String>,
) -> Vec<FontOccurrence> {
    let init = TextState::default();
    ops.iter()
        .fold((init, Vec::<FontOccurrence>::new()), |(state, acc), op| {
            reduce_op(page_index, state, acc, op, fonts)
        })
        .1
}

fn reduce_op(
    page_index: u32,
    state: TextState,
    mut acc: Vec<FontOccurrence>,
    op: &lopdf::content::Operation,
    fonts: &BTreeMap<String, String>,
) -> (TextState, Vec<FontOccurrence>) {
    let op_name = op.operator.as_str();

    if op_name == "BT" {
        return (
            TextState {
                in_text: true,
                ..state
            },
            acc,
        );
    }
    if op_name == "ET" {
        return (
            TextState {
                in_text: false,
                ..state
            },
            acc,
        );
    }
    if op_name == "Tf" {
        return (apply_tf(state, &op.operands, fonts), acc);
    }
    if op_name == "Tm" {
        return (apply_tm(state, &op.operands), acc);
    }
    if op_name == "Td" {
        return (apply_td(state, &op.operands), acc);
    }
    if op_name == "TD" {
        return (apply_td(state, &op.operands), acc);
    }
    if op_name == "T*" {
        return (
            TextState {
                text_matrix: state.text_matrix.next_line(),
                ..state
            },
            acc,
        );
    }

    let occ = if op_name == "TJ" || op_name == "Tj" || op_name == "'" {
        occ_from_tj(page_index, &state, &op.operands)
    } else if op_name == "\"" {
        occ_from_double_quote(page_index, &state, &op.operands)
    } else {
        None
    };

    let out = match occ {
        None => acc,
        Some(occurrence) => {
            acc.push(occurrence);
            acc
        }
    };

    (state, out)
}

fn apply_tf(state: TextState, operands: &[Object], fonts: &BTreeMap<String, String>) -> TextState {
    let font_key = operands
        .first()
        .and_then(object_to_name_string)
        .unwrap_or_else(|| state.font_key.clone());
    let size = operands
        .get(1)
        .and_then(object_to_f32)
        .unwrap_or(state.font_size_pt);

    let resolved = fonts
        .get(&font_key)
        .cloned()
        .unwrap_or_else(|| font_key.clone());
    let normalized = normalize_subset_font_name(&resolved);

    TextState {
        font_key,
        font_name: normalized,
        font_size_pt: size,
        ..state
    }
}

fn apply_tm(state: TextState, operands: &[Object]) -> TextState {
    let tm = Matrix::from_operands(operands).unwrap_or(state.text_matrix);
    TextState {
        text_matrix: tm,
        ..state
    }
}

fn apply_td(state: TextState, operands: &[Object]) -> TextState {
    let dx = operands.first().and_then(object_to_f32).unwrap_or(0.0);
    let dy = operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
    let tm = state.text_matrix.translate(dx, dy);
    TextState {
        text_matrix: tm,
        ..state
    }
}

fn occ_from_tj(
    page_index: u32,
    state: &TextState,
    operands: &[Object],
) -> Option<crate::font_detection::logic::types::file_types::FontOccurrence> {
    let in_text = state.in_text;
    if !in_text {
        return None;
    }

    let text = text_from_tj_operands(operands)?;
    let trimmed = text.trim().to_owned();
    if trimmed.is_empty() {
        return None;
    }

    let font = font_id_from_name(&state.font_name);
    let bbox = estimate_bbox(state);

    Some(build_occurrence(
        font,
        Some(page_index),
        bbox,
        Some(trimmed),
        None,
    ))
}

fn occ_from_double_quote(
    page_index: u32,
    state: &TextState,
    operands: &[Object],
) -> Option<crate::font_detection::logic::types::file_types::FontOccurrence> {
    let last = operands
        .last()
        .cloned()
        .map(|operand| vec![operand])
        .unwrap_or_else(Vec::new);
    occ_from_tj(page_index, state, &last)
}

fn estimate_bbox(state: &TextState) -> Rect {
    let text_x = state.text_matrix.tx;
    let text_y = state.text_matrix.ty;
    let height = state.font_size_pt.abs();
    let width = (state.font_size_pt.abs() * 3.0).max(1.0);
    Rect::new(text_x, text_y - height, text_x + width, text_y)
}

fn text_from_tj_operands(operands: &[Object]) -> Option<String> {
    let first = operands.first()?;
    match first {
        Object::String(text_bytes, _) => Some(String::from_utf8_lossy(text_bytes).to_string()),
        Object::Array(array_items) => Some(
            array_items
                .iter()
                .filter_map(object_string_to_string)
                .collect::<Vec<_>>()
                .join(""),
        ),
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Integer(_)
        | &Object::Real(_)
        | &Object::Name(_)
        | &Object::Dictionary(_)
        | &Object::Stream(_)
        | &Object::Reference(_) => None,
    }
}

fn object_string_to_string(object: &Object) -> Option<String> {
    match object {
        Object::String(text_bytes, _) => Some(String::from_utf8_lossy(text_bytes).to_string()),
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Integer(_)
        | &Object::Real(_)
        | &Object::Name(_)
        | &Object::Array(_)
        | &Object::Dictionary(_)
        | &Object::Stream(_)
        | &Object::Reference(_) => None,
    }
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        &Object::Real(real_value) => Some(real_value),
        &Object::Integer(integer_value) => integer_value.to_string().parse::<f32>().ok(),
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Name(_)
        | &Object::String(..)
        | &Object::Array(_)
        | &Object::Dictionary(_)
        | &Object::Stream(_)
        | &Object::Reference(_) => None,
    }
}

fn object_to_name_string(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Integer(_)
        | &Object::Real(_)
        | &Object::String(..)
        | &Object::Array(_)
        | &Object::Dictionary(_)
        | &Object::Stream(_)
        | &Object::Reference(_) => None,
    }
}

fn object_to_dict(object: &Object) -> Option<&lopdf::Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Integer(_)
        | &Object::Real(_)
        | &Object::Name(_)
        | &Object::String(..)
        | &Object::Array(_)
        | &Object::Stream(_)
        | &Object::Reference(_) => None,
    }
}

fn deref_to_dict<'doc>(
    doc: &'doc Document,
    object: &'doc Object,
) -> Option<&'doc lopdf::Dictionary> {
    let dereferenced = match object {
        &Object::Reference(object_id) => doc.get_object(object_id).ok()?,
        &Object::Null
        | &Object::Boolean(_)
        | &Object::Integer(_)
        | &Object::Real(_)
        | &Object::Name(_)
        | &Object::String(..)
        | &Object::Array(_)
        | &Object::Dictionary(_)
        | &Object::Stream(_) => object,
    };
    object_to_dict(dereferenced)
}

#[derive(Debug, Clone)]
struct TextState {
    in_text: bool,
    font_key: String,
    font_name: String,
    font_size_pt: f32,
    text_matrix: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            in_text: false,
            font_key: String::new(),
            font_name: String::new(),
            font_size_pt: 0.0,
            text_matrix: Matrix::identity(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    m11: f32,
    m12: f32,
    m21: f32,
    m22: f32,
    tx: f32,
    ty: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m21: 0.0,
            m22: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn from_operands(operands: &[Object]) -> Option<Self> {
        let nums = operands.iter().map(object_to_f32).collect::<Vec<_>>();
        let m11 = nums.first().copied().flatten()?;
        let m12 = nums.get(1).copied().flatten()?;
        let m21 = nums.get(2).copied().flatten()?;
        let m22 = nums.get(3).copied().flatten()?;
        let tx = nums.get(4).copied().flatten()?;
        let ty = nums.get(5).copied().flatten()?;
        Some(Self {
            m11,
            m12,
            m21,
            m22,
            tx,
            ty,
        })
    }

    fn translate(&self, dx: f32, dy: f32) -> Self {
        Self {
            tx: self.tx + dx,
            ty: self.ty + dy,
            ..*self
        }
    }

    fn next_line(&self) -> Self {
        self.translate(0.0, -self.m22.abs().max(1.0) * 1.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_detection::dependency::file_accessor::{
        FileAccessor, FileReadRequest, FileReadResponse,
    };
    use crate::font_detection::logic::types::file_types::{
        FontOccurrences, FontsFound, InputFileKind, TextSourceKind,
    };
    use lopdf::{Document, Object};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn minimal_pdf_bytes() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let mut buf = Vec::<u8>::new();
        doc.save_to(&mut buf).expect("expected value in test");
        buf
    }

    struct FakeAccessor {
        files: BTreeMap<String, Vec<u8>>,
        err: Option<String>,
    }

    impl FakeAccessor {
        fn ok(files: BTreeMap<String, Vec<u8>>) -> Self {
            Self { files, err: None }
        }

        fn fail(message: &str) -> Self {
            Self {
                files: BTreeMap::new(),
                err: Some(message.to_owned()),
            }
        }
    }

    impl FileAccessor for FakeAccessor {
        fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String> {
            let err = self.err.clone();
            if let Some(e) = err {
                return Err(e);
            }

            let key = req.path.to_string_lossy().to_string();
            let bytes = self.files.get(&key).cloned();
            match bytes {
                None => Err("not found".to_owned()),
                Some(b) => Ok(FileReadResponse { bytes: b }),
            }
        }
    }

    #[test]
    fn build_file_font_report_unknown_with_details_returns_empty_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.bin"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect("expected value in test");

        assert_eq!(report.kind, InputFileKind::Unknown);
        assert_eq!(report.text_source, TextSourceKind::Unknown);
        assert_eq!(report.occurrences.expect("expected value in test").items, vec![]);
    }

    #[test]
    fn extract_pdf_occurrences_propagates_accessor_error() {
        let accessor = FakeAccessor::fail("io");
        let builder = FileDataBuilder::new(&accessor);
        let err = extract_pdf_occurrences(&builder, Path::new("x.pdf")).expect_err("expected error in test");
        assert_eq!(err, "io".to_owned());
    }

    #[test]
    fn extract_pdf_occurrences_rejects_invalid_pdf_bytes() {
        let mut files = BTreeMap::new();
        files.insert("x.pdf".to_owned(), b"not a pdf".to_vec());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);
        let err = extract_pdf_occurrences(&builder, Path::new("x.pdf")).expect_err("expected error in test");
        assert!(!err.is_empty());
    }

    #[test]
    fn text_from_tj_operands_handles_string_array_and_other() {
        let s = Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(text_from_tj_operands(&[s]), Some("Hi".to_owned()));

        let a = Object::Array(vec![
            Object::String(b"A".to_vec(), lopdf::StringFormat::Literal),
            Object::Integer(-120),
            Object::String(b"B".to_vec(), lopdf::StringFormat::Literal),
        ]);
        assert_eq!(text_from_tj_operands(&[a]), Some("AB".to_owned()));

        assert_eq!(text_from_tj_operands(&[Object::Null]), None);
        assert_eq!(text_from_tj_operands(&[]), None);
    }

    #[test]
    fn object_to_f32_handles_integer_real_and_other() {
        assert_eq!(object_to_f32(&Object::Integer(3)), Some(3.0));
        assert_eq!(object_to_f32(&Object::Real(2.5)), Some(2.5));
        assert_eq!(object_to_f32(&Object::Null), None);
    }

    #[test]
    fn object_to_name_string_handles_name_and_other() {
        assert_eq!(
            object_to_name_string(&Object::Name(b"F1".to_vec())),
            Some("F1".to_owned())
        );
        assert_eq!(object_to_name_string(&Object::Null), None);
    }

    #[test]
    fn matrix_from_operands_requires_six_numbers() {
        let ops = vec![
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(10),
            Object::Integer(20),
        ];
        let m = Matrix::from_operands(&ops).expect("expected value in test");
        assert_eq!(m.m11, 1.0);
        assert_eq!(m.tx, 10.0);
        assert_eq!(m.ty, 20.0);

        let ops2 = vec![Object::Integer(1)];
        assert_eq!(Matrix::from_operands(&ops2), None);
    }

    #[test]
    fn apply_tf_uses_font_map_and_normalizes_name() {
        let mut fonts = BTreeMap::new();
        fonts.insert("F1".to_owned(), "ABCDEE+Calibri".to_owned());

        let state = TextState::default();
        let next = apply_tf(
            state,
            &[Object::Name(b"F1".to_vec()), Object::Real(12.0)],
            &fonts,
        );

        assert_eq!(next.font_key, "F1".to_owned());
        assert_eq!(next.font_name, "Calibri".to_owned());
        assert_eq!(next.font_size_pt, 12.0);
    }

    #[test]
    fn estimate_bbox_uses_font_size_and_matrix() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_owned(),
            font_name: "F1".to_owned(),
            font_size_pt: 10.0,
            text_matrix: Matrix {
                m11: 1.0,
                m12: 0.0,
                m21: 0.0,
                m22: 1.0,
                tx: 7.0,
                ty: 9.0,
            },
        };
        let rect = estimate_bbox(&state);
        assert_eq!(rect.x0, 7.0);
        assert_eq!(rect.y1, 9.0);
        assert_eq!(rect.y0, -1.0);
        assert_eq!(rect.x1, 37.0);
    }

    #[test]
    fn occurrences_from_ops_collects_only_in_text() {
        let fonts = BTreeMap::new();
        let ops = vec![
            lopdf::content::Operation::new(
                "Tj",
                vec![Object::String(
                    b"Hello".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(11)],
            ),
            lopdf::content::Operation::new(
                "Tj",
                vec![Object::String(
                    b"Hello".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            lopdf::content::Operation::new("ET", vec![]),
        ];

        let occs = occurrences_from_ops(0, &ops, &fonts);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].location.page_index, Some(0));
        assert_eq!(occs[0].font.family, "F1".to_owned());
    }

    #[test]
    fn build_file_font_report_image_with_details_returns_empty_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.png"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect("expected value in test");

        assert_eq!(report.kind, InputFileKind::Image);
        assert_eq!(report.text_source, TextSourceKind::Ocr);
        assert_eq!(report.occurrences.expect("expected value in test").items, vec![]);
    }

    #[test]
    fn extracted_occurrences_aggregate_as_expected() {
        let occs = FontOccurrences {
            items: vec![
                build_occurrence(
                    font_id_from_name("Arial"),
                    Some(0),
                    Rect::new(0.0, 0.0, 1.0, 1.0),
                    Some("a".to_owned()),
                    None,
                ),
                build_occurrence(
                    font_id_from_name("Arial"),
                    Some(0),
                    Rect::new(1.0, 0.0, 2.0, 1.0),
                    Some("b".to_owned()),
                    None,
                ),
                build_occurrence(
                    font_id_from_name("Calibri-Bold"),
                    Some(1),
                    Rect::new(0.0, 1.0, 2.0, 2.0),
                    None,
                    None,
                ),
            ],
        };

        let counts = crate::font_detection::logic::types::file_types::aggregate_counts(&occs.items);
        let map = counts
            .iter()
            .map(|c| (c.font.clone(), c.count))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(map.get(&font_id_from_name("Arial")).copied(), Some(2));
        assert_eq!(
            map.get(&font_id_from_name("Calibri-Bold")).copied(),
            Some(1)
        );
    }

    #[test]
    fn build_file_font_report_sets_text_source_based_on_kind() {
        let mut files = BTreeMap::new();
        files.insert("a.pdf".to_owned(), minimal_pdf_bytes());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);

        let pdf = build_file_font_report(
            &builder,
            Path::new("a.pdf"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");
        let img = build_file_font_report(
            &builder,
            Path::new("a.jpg"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");
        let unk = build_file_font_report(
            &builder,
            Path::new("a.bin"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");

        assert_eq!(pdf.text_source, TextSourceKind::EmbeddedText);
        assert_eq!(img.text_source, TextSourceKind::Ocr);
        assert_eq!(unk.text_source, TextSourceKind::Unknown);
    }

    #[test]
    fn file_data_builder_method_delegates() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let r1 = build_file_font_report(
            &builder,
            Path::new("x.bin"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect("expected value in test");
        let r2 = build_file_font_report(
            &builder,
            Path::new("x.bin"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect("expected value in test");

        assert_eq!(r1, r2);
    }

    #[test]
    fn extract_occurrences_branches_cover_kinds() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let a = extract_occurrences(&builder, Path::new("x.png"), InputFileKind::Image).expect("expected value in test");
        let b = extract_occurrences(&builder, Path::new("x.bin"), InputFileKind::Unknown).expect("expected value in test");

        assert_eq!(a.items, vec![]);
        assert_eq!(b.items, vec![]);
    }

    #[test]
    fn occ_from_double_quote_uses_last_operand() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_owned(),
            font_name: "F1".to_owned(),
            font_size_pt: 12.0,
            text_matrix: Matrix::identity(),
        };

        let operands = vec![
            Object::Integer(1),
            Object::Integer(2),
            Object::String(b"X".to_vec(), lopdf::StringFormat::Literal),
        ];

        let out = occ_from_double_quote(0, &state, &operands).expect("expected value in test");
        assert_eq!(out.text, Some("X".to_owned()));
    }

    #[test]
    fn occ_from_tj_requires_in_text_and_nonempty_text() {
        let state = TextState::default();
        let s = Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(occ_from_tj(0, &state, std::slice::from_ref(&s)), None);

        let state2 = TextState {
            in_text: true,
            font_key: "F1".to_owned(),
            font_name: "F1".to_owned(),
            font_size_pt: 12.0,
            text_matrix: Matrix::identity(),
        };

        let empty = Object::String(b"   ".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(occ_from_tj(0, &state2, &[empty]), None);

        let out = occ_from_tj(2, &state2, &[s]).expect("expected value in test");
        assert_eq!(out.location.page_index, Some(2));
        assert_eq!(out.text, Some("Hi".to_owned()));
    }

    #[test]
    fn apply_tm_and_apply_td_change_matrix() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_owned(),
            font_name: "F1".to_owned(),
            font_size_pt: 10.0,
            text_matrix: Matrix::identity(),
        };

        let tm_ops = vec![
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(10),
            Object::Integer(20),
        ];
        let s2 = apply_tm(state.clone(), &tm_ops);
        assert_eq!(s2.text_matrix.tx, 10.0);
        assert_eq!(s2.text_matrix.ty, 20.0);

        let td_ops = vec![Object::Integer(5), Object::Integer(-3)];
        let s3 = apply_td(s2, &td_ops);
        assert_eq!(s3.text_matrix.tx, 15.0);
        assert_eq!(s3.text_matrix.ty, 17.0);
    }

    #[test]
    fn file_kind_branch_does_not_depend_on_accessor() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let image = build_file_font_report(
            &builder,
            Path::new("x.png"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect("expected value in test");
        assert_eq!(image.kind, InputFileKind::Image);
        assert_eq!(image.occurrences.expect("expected value in test").items.len(), 0);
    }

    #[test]
    fn build_file_font_report_pdf_with_details_reads_file() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let err = build_file_font_report(
            &builder,
            Path::new("x.pdf"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect_err("expected error in test");

        assert_eq!(err, "not found".to_owned());
    }

    #[test]
    fn extract_pdf_occurrences_uses_path_key() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let err = extract_pdf_occurrences(&builder, Path::new("dir/y.pdf")).expect_err("expected error in test");
        assert_eq!(err, "not found".to_owned());
    }

    #[test]
    fn kind_and_source_in_report_match_extension_case_insensitively() {
        let mut files = BTreeMap::new();
        files.insert("X.PdF".to_owned(), minimal_pdf_bytes());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("X.PdF"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");

        assert_eq!(report.kind, InputFileKind::Pdf);
        assert_eq!(report.text_source, TextSourceKind::EmbeddedText);
    }

    #[test]
    fn resolve_pdf_font_name_none_when_not_dict() {
        let doc = Document::with_version("1.5");
        let o = Object::Null;
        let out = resolve_pdf_font_name(&doc, &o);
        assert_eq!(out, None);
    }

    #[test]
    fn object_to_dict_none_when_not_dictionary() {
        assert!(object_to_dict(&Object::Null).is_none());
    }

    #[test]
    fn matrix_next_line_moves_down() {
        let m = Matrix::identity();
        let out = m.next_line();
        assert!(out.ty < m.ty);
    }

    #[test]
    fn estimate_bbox_width_minimum() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_owned(),
            font_name: "F1".to_owned(),
            font_size_pt: 0.0,
            text_matrix: Matrix::identity(),
        };
        let r = estimate_bbox(&state);
        assert_eq!(r.x1 - r.x0, 1.0);
    }

    #[test]
    fn extract_occurrences_pdf_errors_on_missing_file() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let err = extract_occurrences(&builder, Path::new("missing.pdf"), InputFileKind::Pdf)
            .expect_err("expected error in test");
        assert_eq!(err, "not found".to_owned());
    }

    #[test]
    fn build_file_font_report_does_not_error_for_unknown_without_details() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.bin"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");
        assert_eq!(report.occurrences, Some(FontOccurrences { items: vec![] }));
        assert_eq!(report.kind, InputFileKind::Unknown);
        assert_eq!(report.text_source, TextSourceKind::Unknown);
    }

    #[test]
    fn build_file_font_report_image_without_details_has_no_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.png"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");
        assert_eq!(report.occurrences, Some(FontOccurrences { items: vec![] }));
        assert_eq!(report.kind, InputFileKind::Image);
        assert_eq!(report.text_source, TextSourceKind::Ocr);
    }

    #[test]
    fn extract_occurrences_uses_kind_argument() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let a = extract_occurrences(&builder, Path::new("x.pdf"), InputFileKind::Unknown).expect("expected value in test");
        assert_eq!(a.items, vec![]);
    }

    #[test]
    fn file_report_path_is_lossy_string() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.bin"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("expected value in test");
        assert_eq!(report.path, "x.bin".to_owned());
    }

    #[test]
    fn occurrences_items_are_public_and_aggregatable() {
        let report = FileFontReport {
            path: "x".to_owned(),
            kind: InputFileKind::Unknown,
            text_source: TextSourceKind::Unknown,
            fonts: FontsFound {
                distinct: vec![],
                counts: vec![],
            },
            occurrences: Some(FontOccurrences { items: vec![] }),
        };

        let occs = report.occurrences.expect("expected value in test");
        let counts = crate::font_detection::logic::types::file_types::aggregate_counts(&occs.items);
        assert_eq!(counts, vec![]);
    }

    #[test]
    fn normalize_subset_font_name_in_data_resolver() {
        let out = normalize_subset_font_name("ABCDEF+Arial");
        assert_eq!(out, "Arial".to_owned());
    }

    #[test]
    fn read_request_path_is_owned() {
        let req = FileReadRequest {
            path: PathBuf::from("x.pdf"),
        };
        assert_eq!(req.path, PathBuf::from("x.pdf"));
    }
}

