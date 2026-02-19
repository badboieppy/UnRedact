use crate::dependency::file_store::{FileAccessor, FileReadRequest};
use crate::types::file_types::{
    DocumentLocation, FileFontReport, FontId, FontOccurrence, FontOccurrences, FontsFound,
    InputFileKind, Rect, Region, TextSourceKind,
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

#[inline]
pub fn build_file_font_report_from_bytes(
    input_name: &str,
    bytes: &[u8],
    _config: DataBuildConfig,
) -> Result<FileFontReport, String> {
    let pdf_occurrences = extract_pdf_occurrences_from_bytes(bytes);
    let (kind, text_source, occurrences) = match pdf_occurrences {
        Ok(occurrences) => (
            InputFileKind::Pdf,
            TextSourceKind::EmbeddedText,
            Some(occurrences),
        ),
        Err(_) => {
            let classified = classify_kind_from_path(Path::new(input_name));
            (
                classified,
                default_text_source_for_kind(classified),
                Some(FontOccurrences { items: vec![] }),
            )
        }
    };

    Ok(FileFontReport {
        path: input_name.to_owned(),
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

fn extract_pdf_occurrences_from_bytes(bytes: &[u8]) -> Result<FontOccurrences, String> {
    let doc = Document::load_mem(bytes).map_err(|error| error.to_string())?;
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
    let decoded = lopdf::content::Content::decode(&content).map_err(|error| error.to_string())?;
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
    let desc_dict_opt =
        desc_obj.and_then(|descriptor_object| deref_to_dict(doc, descriptor_object));
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
) -> Option<crate::types::file_types::FontOccurrence> {
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
) -> Option<crate::types::file_types::FontOccurrence> {
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

#[inline]
fn normalize_subset_font_name(raw: &str) -> String {
    let parts = raw.split('+').collect::<Vec<_>>();
    normalize_from_parts(&parts)
}

fn normalize_from_parts(parts: &[&str]) -> String {
    let is_subset = is_subset_prefix(parts);
    if !is_subset {
        return parts.join("+");
    }

    let second = parts.get(1).copied().unwrap_or("");
    if second.is_empty() {
        return parts.join("+");
    }

    second.to_owned()
}

fn is_subset_prefix(parts: &[&str]) -> bool {
    let has_two = parts.len() == 2;
    if !has_two {
        return false;
    }

    let prefix = parts[0];
    let len_ok = prefix.len() == 6;
    if !len_ok {
        return false;
    }

    prefix.chars().all(|c| c.is_ascii_uppercase())
}

#[inline]
fn font_id_from_name(name: &str) -> FontId {
    let (family, variant) = split_font_family_variant(name);
    FontId { family, variant }
}

#[inline]
fn split_font_family_variant(name: &str) -> (String, Option<String>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return ("".to_owned(), None);
    }

    let parts = trimmed.split('-').collect::<Vec<_>>();
    let two = parts.len() == 2;

    if !two {
        return (trimmed.to_owned(), None);
    }

    let family = parts[0].trim().to_owned();
    let variant = parts[1].trim().to_owned();

    let variant_opt = if variant.is_empty() {
        None
    } else {
        Some(variant)
    };
    (family, variant_opt)
}

#[inline]
fn classify_kind_from_path(path: &Path) -> InputFileKind {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let ext_lower = ext.to_ascii_lowercase();

    if ext_lower == "pdf" {
        return InputFileKind::Pdf;
    }

    let is_image = matches!(
        ext_lower.as_str(),
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp"
    );
    if is_image {
        return InputFileKind::Image;
    }

    InputFileKind::Unknown
}

#[inline]
fn default_text_source_for_kind(kind: InputFileKind) -> TextSourceKind {
    match kind {
        InputFileKind::Pdf => TextSourceKind::EmbeddedText,
        InputFileKind::Image => TextSourceKind::ImageRaster,
        InputFileKind::Unknown => TextSourceKind::Unknown,
    }
}

#[inline]
fn build_occurrence(
    font: FontId,
    page_index: Option<u32>,
    bbox: Rect,
    text: Option<String>,
    confidence: Option<f32>,
) -> FontOccurrence {
    FontOccurrence {
        font,
        location: DocumentLocation {
            page_index,
            region: Region { bbox },
        },
        text,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_file_font_report, build_file_font_report_from_bytes, DataBuildConfig, FileDataBuilder,
    };
    use crate::dependency::file_store::{FileAccessor, FileReadRequest, FileReadResponse};
    use crate::types::file_types::{InputFileKind, TextSourceKind};
    use std::collections::BTreeMap;
    use std::path::Path;

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
            if let Some(message) = &self.err {
                return Err(message.clone());
            }
            let key = req.path.to_string_lossy().to_string();
            self.files
                .get(&key)
                .cloned()
                .map(|bytes| FileReadResponse { bytes })
                .ok_or_else(|| "not found".to_owned())
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
        .expect("unknown file report should build");

        assert_eq!(report.kind, InputFileKind::Unknown);
        assert_eq!(report.text_source, TextSourceKind::Unknown);
        assert_eq!(report.path, "x.bin".to_owned());
        assert_eq!(
            report
                .occurrences
                .expect("occurrences should exist")
                .items
                .len(),
            0
        );
    }

    #[test]
    fn build_file_font_report_image_kind_has_no_occurrences() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);

        let report = build_file_font_report(
            &builder,
            Path::new("x.png"),
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("image file report should build");

        assert_eq!(report.kind, InputFileKind::Image);
        assert_eq!(report.text_source, TextSourceKind::ImageRaster);
        assert_eq!(
            report
                .occurrences
                .expect("occurrences should exist")
                .items
                .len(),
            0
        );
    }

    #[test]
    fn build_file_font_report_propagates_read_errors_for_pdf() {
        let accessor = FakeAccessor::fail("io_error");
        let builder = FileDataBuilder::new(&accessor);

        let err = build_file_font_report(
            &builder,
            Path::new("x.pdf"),
            DataBuildConfig {
                include_details: true,
            },
        )
        .expect_err("pdf report should fail when read fails");
        assert_eq!(err, "io_error".to_owned());
    }

    #[test]
    fn build_file_font_report_from_bytes_handles_invalid_pdf_without_crashing() {
        let report = build_file_font_report_from_bytes(
            "bad.pdf",
            b"not a pdf",
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("invalid pdf bytes should still produce a report");
        assert_eq!(report.kind, InputFileKind::Pdf);
        assert_eq!(report.text_source, TextSourceKind::EmbeddedText);
        assert!(report.fonts.distinct.is_empty());
        assert!(report.fonts.counts.is_empty());
    }

    #[test]
    fn build_file_font_report_from_bytes_parses_real_pdf() {
        let input = Path::new("test_data/EFTA00101126.pdf");
        let bytes = std::fs::read(input)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));

        let report = build_file_font_report_from_bytes(
            "EFTA00101126.pdf",
            &bytes,
            DataBuildConfig {
                include_details: false,
            },
        )
        .expect("real pdf bytes should parse");

        assert_eq!(report.kind, InputFileKind::Pdf);
        assert_eq!(report.text_source, TextSourceKind::EmbeddedText);
    }
}
