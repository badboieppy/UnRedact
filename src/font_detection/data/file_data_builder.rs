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

pub struct FileDataBuilder<'a> {
    accessor: &'a dyn FileAccessor,
}

impl<'a> FileDataBuilder<'a> {
    pub fn new(accessor: &'a dyn FileAccessor) -> Self {
        Self { accessor }
    }
}

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

fn extract_pdf_occurrences(builder: &FileDataBuilder<'_>, path: &Path) -> Result<FontOccurrences, String> {
    let bytes = builder
        .accessor
        .read(FileReadRequest {
            path: path.to_path_buf(),
        })?
        .bytes;

    let doc = Document::load_mem(&bytes).map_err(|e| e.to_string())?;
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
    let content = doc.get_page_content(page_id).map_err(|e| e.to_string())?;
    let decoded = lopdf::content::Content::decode(&content).map_err(|e| e.to_string())?;
    Ok(occurrences_from_ops(
        page_no.saturating_sub(1),
        &decoded.operations,
        &fonts,
    ))
}

fn extract_pdf_page_fonts(doc: &Document, page_id: lopdf::ObjectId) -> Result<BTreeMap<String, String>, String> {
    let (resources, _) = doc.get_page_resources(page_id).map_err(|e| e.to_string())?;
    let resources = match resources {
        None => return Ok(BTreeMap::new()),
        Some(d) => d,
    };

    let font_obj = resources.get(b"Font").ok();
    let font_obj = match font_obj {
        None => return Ok(BTreeMap::new()),
        Some(o) => o,
    };

    let font_dict = deref_to_dict(doc, font_obj).or_else(|| object_to_dict(font_obj));
    let font_dict = match font_dict {
        None => return Ok(BTreeMap::new()),
        Some(d) => d,
    };

    let map = font_dict
        .iter()
        .map(|(k, v)| {
            let key = String::from_utf8_lossy(k).to_string();
            let name = resolve_pdf_font_name(doc, v).unwrap_or_else(|| key.clone());
            (key, name)
        })
        .collect::<BTreeMap<_, _>>();

    Ok(map)
}

fn resolve_pdf_font_name(doc: &Document, font_obj: &Object) -> Option<String> {
    let dict = deref_to_dict(doc, font_obj)?;
    let base = dict.get(b"BaseFont").ok().and_then(object_to_name_string);
    if let Some(b) = base {
        return Some(normalize_subset_font_name(&b));
    }

    let desc_obj = dict.get(b"FontDescriptor").ok();
    let desc_dict = desc_obj.and_then(|o| deref_to_dict(doc, o));
    let desc_dict = desc_dict?;
    let name = desc_dict.get(b"FontName").ok().and_then(object_to_name_string)?;
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
        return (TextState { in_text: true, ..state }, acc);
    }
    if op_name == "ET" {
        return (TextState { in_text: false, ..state }, acc);
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
        return (TextState { text_matrix: state.text_matrix.next_line(), ..state }, acc);
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
        Some(o) => {
            acc.push(o);
            acc
        }
    };

    (state, out)
}

fn apply_tf(state: TextState, operands: &[Object], fonts: &BTreeMap<String, String>) -> TextState {
    let font_key = operands
        .get(0)
        .and_then(object_to_name_string)
        .unwrap_or_else(|| state.font_key.clone());
    let size = operands.get(1).and_then(object_to_f32).unwrap_or(state.font_size_pt);

    let resolved = fonts.get(&font_key).cloned().unwrap_or_else(|| font_key.clone());
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
    TextState { text_matrix: tm, ..state }
}

fn apply_td(state: TextState, operands: &[Object]) -> TextState {
    let dx = operands.get(0).and_then(object_to_f32).unwrap_or(0.0);
    let dy = operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
    let tm = state.text_matrix.translate(dx, dy);
    TextState { text_matrix: tm, ..state }
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
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        return None;
    }

    let font = font_id_from_name(&state.font_name);
    let bbox = estimate_bbox(state);

    Some(build_occurrence(font, Some(page_index), bbox, Some(trimmed), None))
}

fn occ_from_double_quote(
    page_index: u32,
    state: &TextState,
    operands: &[Object],
) -> Option<crate::font_detection::logic::types::file_types::FontOccurrence> {
    let last = operands.last().cloned().map(|o| vec![o]).unwrap_or_else(Vec::new);
    occ_from_tj(page_index, state, &last)
}

fn estimate_bbox(state: &TextState) -> Rect {
    let x = state.text_matrix.e;
    let y = state.text_matrix.f;
    let h = state.font_size_pt.abs();
    let w = (state.font_size_pt.abs() * 3.0).max(1.0);
    Rect::new(x, y - h, x + w, y)
}

fn text_from_tj_operands(operands: &[Object]) -> Option<String> {
    let first = operands.first()?;
    match first {
        Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
        Object::Array(a) => Some(
            a.iter()
                .filter_map(|o| match o {
                    Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        ),
        _ => None,
    }
}

fn object_to_f32(o: &Object) -> Option<f32> {
    match o {
        Object::Real(r) => Some(*r as f32),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn object_to_name_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    }
}

fn object_to_dict(o: &Object) -> Option<&lopdf::Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn deref_to_dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a lopdf::Dictionary> {
    let d = match o {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        _ => o,
    };
    object_to_dict(d)
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
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_operands(operands: &[Object]) -> Option<Self> {
        let nums = operands.iter().map(object_to_f32).collect::<Vec<_>>();
        let a = nums.get(0).copied().flatten()?;
        let b = nums.get(1).copied().flatten()?;
        let c = nums.get(2).copied().flatten()?;
        let d = nums.get(3).copied().flatten()?;
        let e = nums.get(4).copied().flatten()?;
        let f = nums.get(5).copied().flatten()?;
        Some(Self { a, b, c, d, e, f })
    }

    fn translate(&self, dx: f32, dy: f32) -> Self {
        Self {
            e: self.e + dx,
            f: self.f + dy,
            ..*self
        }
    }

    fn next_line(&self) -> Self {
        self.translate(0.0, -self.d.abs().max(1.0) * 1.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_detection::dependency::file_accessor::{FileAccessor, FileReadRequest, FileReadResponse};
    use crate::font_detection::logic::types::file_types::{FontOccurrences, FontsFound, InputFileKind, TextSourceKind};
    use lopdf::{Document, Object};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn minimal_pdf_bytes() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let mut buf = Vec::<u8>::new();
        doc.save_to(&mut buf).unwrap();
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
                err: Some(message.to_string()),
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
                None => Err("not found".to_string()),
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
            DataBuildConfig { include_details: true },
        )
        .unwrap();

        assert_eq!(report.kind, InputFileKind::Unknown);
        assert_eq!(report.text_source, TextSourceKind::Unknown);
        assert_eq!(report.occurrences.unwrap().items, vec![]);
    }

    #[test]
    fn extract_pdf_occurrences_propagates_accessor_error() {
        let accessor = FakeAccessor::fail("io");
        let builder = FileDataBuilder::new(&accessor);
        let err = extract_pdf_occurrences(&builder, Path::new("x.pdf")).unwrap_err();
        assert_eq!(err, "io".to_string());
    }

    #[test]
    fn extract_pdf_occurrences_rejects_invalid_pdf_bytes() {
        let mut files = BTreeMap::new();
        files.insert("x.pdf".to_string(), b"not a pdf".to_vec());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);
        let err = extract_pdf_occurrences(&builder, Path::new("x.pdf")).unwrap_err();
        assert_eq!(err.is_empty(), false);
    }

    #[test]
    fn text_from_tj_operands_handles_string_array_and_other() {
        let s = Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(text_from_tj_operands(&[s]), Some("Hi".to_string()));

        let a = Object::Array(vec![
            Object::String(b"A".to_vec(), lopdf::StringFormat::Literal),
            Object::Integer(-120),
            Object::String(b"B".to_vec(), lopdf::StringFormat::Literal),
        ]);
        assert_eq!(text_from_tj_operands(&[a]), Some("AB".to_string()));

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
            Some("F1".to_string())
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
        let m = Matrix::from_operands(&ops).unwrap();
        assert_eq!(m.a, 1.0);
        assert_eq!(m.e, 10.0);
        assert_eq!(m.f, 20.0);

        let ops2 = vec![Object::Integer(1)];
        assert_eq!(Matrix::from_operands(&ops2), None);
    }

    #[test]
    fn apply_tf_uses_font_map_and_normalizes_name() {
        let mut fonts = BTreeMap::new();
        fonts.insert("F1".to_string(), "ABCDEE+Calibri".to_string());

        let state = TextState::default();
        let next = apply_tf(
            state,
            &[Object::Name(b"F1".to_vec()), Object::Real(12.0)],
            &fonts,
        );

        assert_eq!(next.font_key, "F1".to_string());
        assert_eq!(next.font_name, "Calibri".to_string());
        assert_eq!(next.font_size_pt, 12.0);
    }

    #[test]
    fn estimate_bbox_uses_font_size_and_matrix() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_string(),
            font_name: "F1".to_string(),
            font_size_pt: 10.0,
            text_matrix: Matrix {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 7.0,
                f: 9.0,
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
                vec![Object::String(b"Hello".to_vec(), lopdf::StringFormat::Literal)],
            ),
            lopdf::content::Operation::new("BT", vec![]),
            lopdf::content::Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Integer(11)]),
            lopdf::content::Operation::new(
                "Tj",
                vec![Object::String(b"Hello".to_vec(), lopdf::StringFormat::Literal)],
            ),
            lopdf::content::Operation::new("ET", vec![]),
        ];

        let occs = occurrences_from_ops(0, &ops, &fonts);
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].location.page_index, Some(0));
        assert_eq!(occs[0].font.family, "F1".to_string());
    }

    #[test]
    fn build_file_font_report_image_with_details_returns_empty_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.png"),
            DataBuildConfig { include_details: true },
        )
        .unwrap();

        assert_eq!(report.kind, InputFileKind::Image);
        assert_eq!(report.text_source, TextSourceKind::Ocr);
        assert_eq!(report.occurrences.unwrap().items, vec![]);
    }

    #[test]
    fn extracted_occurrences_aggregate_as_expected() {
        let occs = FontOccurrences {
            items: vec![
                build_occurrence(
                    font_id_from_name("Arial"),
                    Some(0),
                    Rect::new(0.0, 0.0, 1.0, 1.0),
                    Some("a".to_string()),
                    None,
                ),
                build_occurrence(
                    font_id_from_name("Arial"),
                    Some(0),
                    Rect::new(1.0, 0.0, 2.0, 1.0),
                    Some("b".to_string()),
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
        assert_eq!(map.get(&font_id_from_name("Calibri-Bold")).copied(), Some(1));
    }

    #[test]
    fn build_file_font_report_sets_text_source_based_on_kind() {
        let mut files = BTreeMap::new();
        files.insert("a.pdf".to_string(), minimal_pdf_bytes());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);

        let pdf = build_file_font_report(&builder, Path::new("a.pdf"), DataBuildConfig { include_details: false }).unwrap();
        let img = build_file_font_report(&builder, Path::new("a.jpg"), DataBuildConfig { include_details: false }).unwrap();
        let unk = build_file_font_report(&builder, Path::new("a.bin"), DataBuildConfig { include_details: false }).unwrap();

        assert_eq!(pdf.text_source, TextSourceKind::EmbeddedText);
        assert_eq!(img.text_source, TextSourceKind::Ocr);
        assert_eq!(unk.text_source, TextSourceKind::Unknown);
    }

    #[test]
    fn file_data_builder_method_delegates() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let r1 = build_file_font_report(&builder, Path::new("x.bin"), DataBuildConfig { include_details: true }).unwrap();
        let r2 = build_file_font_report(&builder, Path::new("x.bin"), DataBuildConfig { include_details: true }).unwrap();

        assert_eq!(r1, r2);
    }

    #[test]
    fn extract_occurrences_branches_cover_kinds() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let a = extract_occurrences(&builder, Path::new("x.png"), InputFileKind::Image).unwrap();
        let b = extract_occurrences(&builder, Path::new("x.bin"), InputFileKind::Unknown).unwrap();

        assert_eq!(a.items, vec![]);
        assert_eq!(b.items, vec![]);
    }

    #[test]
    fn occ_from_double_quote_uses_last_operand() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_string(),
            font_name: "F1".to_string(),
            font_size_pt: 12.0,
            text_matrix: Matrix::identity(),
        };

        let operands = vec![
            Object::Integer(1),
            Object::Integer(2),
            Object::String(b"X".to_vec(), lopdf::StringFormat::Literal),
        ];

        let out = occ_from_double_quote(0, &state, &operands).unwrap();
        assert_eq!(out.text, Some("X".to_string()));
    }

    #[test]
    fn occ_from_tj_requires_in_text_and_nonempty_text() {
        let state = TextState::default();
        let s = Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(occ_from_tj(0, &state, &[s.clone()]), None);

        let state2 = TextState {
            in_text: true,
            font_key: "F1".to_string(),
            font_name: "F1".to_string(),
            font_size_pt: 12.0,
            text_matrix: Matrix::identity(),
        };

        let empty = Object::String(b"   ".to_vec(), lopdf::StringFormat::Literal);
        assert_eq!(occ_from_tj(0, &state2, &[empty]), None);

        let out = occ_from_tj(2, &state2, &[s]).unwrap();
        assert_eq!(out.location.page_index, Some(2));
        assert_eq!(out.text, Some("Hi".to_string()));
    }

    #[test]
    fn apply_tm_and_apply_td_change_matrix() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_string(),
            font_name: "F1".to_string(),
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
        assert_eq!(s2.text_matrix.e, 10.0);
        assert_eq!(s2.text_matrix.f, 20.0);

        let td_ops = vec![Object::Integer(5), Object::Integer(-3)];
        let s3 = apply_td(s2, &td_ops);
        assert_eq!(s3.text_matrix.e, 15.0);
        assert_eq!(s3.text_matrix.f, 17.0);
    }

    #[test]
    fn file_kind_branch_does_not_depend_on_accessor() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let image = build_file_font_report(&builder, Path::new("x.png"), DataBuildConfig { include_details: true }).unwrap();
        assert_eq!(image.kind, InputFileKind::Image);
        assert_eq!(image.occurrences.unwrap().items.len(), 0);
    }

    #[test]
    fn build_file_font_report_pdf_with_details_reads_file() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let err = build_file_font_report(
            &builder,
            Path::new("x.pdf"),
            DataBuildConfig { include_details: true },
        )
        .unwrap_err();

        assert_eq!(err, "not found".to_string());
    }

    #[test]
    fn extract_pdf_occurrences_uses_path_key() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let err = extract_pdf_occurrences(&builder, Path::new("dir/y.pdf")).unwrap_err();
        assert_eq!(err, "not found".to_string());
    }

    #[test]
    fn kind_and_source_in_report_match_extension_case_insensitively() {
        let mut files = BTreeMap::new();
        files.insert("X.PdF".to_string(), minimal_pdf_bytes());

        let accessor = FakeAccessor::ok(files);
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("X.PdF"),
            DataBuildConfig { include_details: false },
        )
        .unwrap();

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
        assert_eq!(object_to_dict(&Object::Null).is_none(), true);
    }

    #[test]
    fn matrix_next_line_moves_down() {
        let m = Matrix::identity();
        let out = m.next_line();
        assert_eq!(out.f < m.f, true);
    }

    #[test]
    fn estimate_bbox_width_minimum() {
        let state = TextState {
            in_text: true,
            font_key: "F1".to_string(),
            font_name: "F1".to_string(),
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

        let err = extract_occurrences(&builder, Path::new("missing.pdf"), InputFileKind::Pdf).unwrap_err();
        assert_eq!(err, "not found".to_string());
    }

    #[test]
    fn build_file_font_report_does_not_error_for_unknown_without_details() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(&builder, Path::new("x.bin"), DataBuildConfig { include_details: false }).unwrap();
        assert_eq!(report.occurrences, Some(FontOccurrences { items: vec![] }));
        assert_eq!(report.kind, InputFileKind::Unknown);
        assert_eq!(report.text_source, TextSourceKind::Unknown);
    }

    #[test]
    fn build_file_font_report_image_without_details_has_no_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(&builder, Path::new("x.png"), DataBuildConfig { include_details: false }).unwrap();
        assert_eq!(report.occurrences, Some(FontOccurrences { items: vec![] }));
        assert_eq!(report.kind, InputFileKind::Image);
        assert_eq!(report.text_source, TextSourceKind::Ocr);
    }

    #[test]
    fn extract_occurrences_uses_kind_argument() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let a = extract_occurrences(&builder, Path::new("x.pdf"), InputFileKind::Unknown).unwrap();
        assert_eq!(a.items, vec![]);
    }

    #[test]
    fn file_report_path_is_lossy_string() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(&builder, Path::new("x.bin"), DataBuildConfig { include_details: false }).unwrap();
        assert_eq!(report.path, "x.bin".to_string());
    }

    #[test]
    fn occurrences_items_are_public_and_aggregatable() {
        let report = FileFontReport {
            path: "x".to_string(),
            kind: InputFileKind::Unknown,
            text_source: TextSourceKind::Unknown,
            fonts: FontsFound {
                distinct: vec![],
                counts: vec![],
            },
            occurrences: Some(FontOccurrences { items: vec![] }),
        };

        let occs = report.occurrences.unwrap();
        let counts = crate::font_detection::logic::types::file_types::aggregate_counts(&occs.items);
        assert_eq!(counts, vec![]);
    }

    #[test]
    fn normalize_subset_font_name_in_data_resolver() {
        let out = normalize_subset_font_name("ABCDEF+Arial");
        assert_eq!(out, "Arial".to_string());
    }

    #[test]
    fn read_request_path_is_owned() {
        let req = FileReadRequest { path: PathBuf::from("x.pdf") };
        assert_eq!(req.path, PathBuf::from("x.pdf"));
    }
}
