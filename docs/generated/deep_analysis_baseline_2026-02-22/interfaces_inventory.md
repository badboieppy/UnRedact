# Interfaces Inventory

## Rust Public Exports

| File | Line | Kind | Symbol | Signature |
|---|---:|---|---|---|
| src/bin/pdf_to_png.rs | 132 | fn | default_output_dir | pub fn default_output_dir(input: &Path) -> PathBuf { |
| src/bin/pdf_to_png.rs | 139 | fn | default_single_page_path | pub fn default_single_page_path(input: &Path, output_dir: &Path, page: usize) -> PathBuf { |
| src/bin/pdf_to_png.rs | 148 | fn | default_all_pages_path | pub fn default_all_pages_path(input: &Path, output_dir: &Path, page: usize) -> PathBuf { |
| src/data/default_name_dictionary.rs | 4 | const | DEFAULT_NAME_DICTIONARY | pub const DEFAULT_NAME_DICTIONARY: &[&str] = &[ |
| src/data/dictionary_data.rs | 6 | struct | DictionaryInputs | pub struct DictionaryInputs { |
| src/data/dictionary_data.rs | 12 | struct | DictionaryData | pub struct DictionaryData; |
| src/data/dictionary_data.rs | 16 | fn | new | pub fn new() -> Self { |
| src/data/dictionary_data.rs | 21 | fn | load_dictionary_from_bytes | pub fn load_dictionary_from_bytes( |
| src/data/dictionary_data.rs | 37 | fn | load_dictionary_from_bytes | pub fn load_dictionary_from_bytes( |
| src/data/fonts_data.rs | 11 | struct | FontRunInputs | pub struct FontRunInputs { |
| src/data/fonts_data.rs | 16 | struct | FontsData | pub struct FontsData; |
| src/data/fonts_data.rs | 20 | fn | new | pub fn new() -> Self { |
| src/data/fonts_data.rs | 25 | fn | detect_fonts_from_bytes | pub fn detect_fonts_from_bytes( |
| src/data/fonts_data.rs | 42 | fn | load_font_runs_from_bytes | pub fn load_font_runs_from_bytes( |
| src/data/fonts_data.rs | 60 | fn | finalize_file_font_report | pub(super) fn finalize_file_font_report( |
| src/data/guess_validation_data.rs | 9 | struct | GuessReportInputs | pub struct GuessReportInputs { |
| src/data/guess_validation_data.rs | 15 | trait | ReportDataSource | pub trait ReportDataSource { |
| src/data/guess_validation_data.rs | 24 | struct | GuessValidationData | pub struct GuessValidationData { |
| src/data/guess_validation_data.rs | 30 | fn | new | pub fn new() -> Self { |
| src/data/guess_validation_data.rs | 37 | fn | write_guesses | pub fn write_guesses(&self, path: &Path, report: &GuessReport) -> Result<(), String> { |
| src/data/guess_validation_data.rs | 52 | fn | load_reports | pub fn load_reports( |
| src/data/local_file_workflow_data.rs | 6 | struct | LocalFileWorkflowData | pub struct LocalFileWorkflowData { |
| src/data/local_file_workflow_data.rs | 12 | fn | new | pub fn new() -> Self { |
| src/data/local_file_workflow_data.rs | 19 | fn | read_bytes | pub fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, String> { |
| src/data/local_file_workflow_data.rs | 24 | fn | write_bytes_exact | pub fn write_bytes_exact(&self, path: &Path, bytes: &[u8]) -> Result<(), String> { |
| src/data/local_file_workflow_data.rs | 29 | fn | create_dir_all | pub fn create_dir_all(&self, path: &Path) -> Result<(), String> { |
| src/data/local_file_workflow_data.rs | 34 | fn | read_dir_paths | pub fn read_dir_paths(&self, path: &Path) -> Result<Vec<PathBuf>, String> { |
| src/data/local_file_workflow_data.rs | 39 | fn | exists | pub fn exists(&self, path: &Path) -> Result<bool, String> { |
| src/data/local_file_workflow_data.rs | 44 | fn | is_dir | pub fn is_dir(&self, path: &Path) -> Result<bool, String> { |
| src/data/mod.rs | 1 | mod | default_name_dictionary | pub mod default_name_dictionary; |
| src/data/mod.rs | 2 | mod | dictionary_data | pub mod dictionary_data; |
| src/data/mod.rs | 3 | mod | fonts_data | pub mod fonts_data; |
| src/data/mod.rs | 5 | mod | guess_validation_data | pub mod guess_validation_data; |
| src/data/mod.rs | 7 | mod | local_file_workflow_data | pub mod local_file_workflow_data; |
| src/data/mod.rs | 8 | mod | redactions_data | pub mod redactions_data; |
| src/data/mod.rs | 10 | mod | result_data_publisher | pub mod result_data_publisher; |
| src/data/mod.rs | 11 | mod | visualization_data | pub mod visualization_data; |
| src/data/mod.rs | 13 | use | default_name_dictionary::DEFAULT_NAME_DICTIONARY | pub use default_name_dictionary::DEFAULT_NAME_DICTIONARY; |
| src/data/mod.rs | 14 | use | dictionary_data::{DictionaryData, DictionaryInputs} | pub use dictionary_data::{DictionaryData, DictionaryInputs}; |
| src/data/mod.rs | 15 | use | fonts_data::{FontRunInputs, FontsData} | pub use fonts_data::{FontRunInputs, FontsData}; |
| src/data/mod.rs | 17 | use | guess_validation_data::ReportDataSource | pub use guess_validation_data::ReportDataSource; |
| src/data/mod.rs | 19 | use | guess_validation_data::{GuessReportInputs, GuessValidationData} | pub use guess_validation_data::{GuessReportInputs, GuessValidationData}; |
| src/data/mod.rs | 21 | use | local_file_workflow_data::LocalFileWorkflowData | pub use local_file_workflow_data::LocalFileWorkflowData; |
| src/data/mod.rs | 26 | use | visualization_data::{VisualizationData, VisualizationInputs} | pub use visualization_data::{VisualizationData, VisualizationInputs}; |
| src/data/redactions_data.rs | 9 | struct | RedactionsData | pub struct RedactionsData; |
| src/data/redactions_data.rs | 13 | fn | new | pub fn new() -> Self { |
| src/data/redactions_data.rs | 18 | fn | build_renderer | pub fn build_renderer(&self, bytes: &[u8]) -> Result<HayroRenderer, String> { |
| src/data/redactions_data.rs | 30 | trait | RedactionDataRetriever | pub trait RedactionDataRetriever { |
| src/data/redactions_data.rs | 51 | struct | PdfFileRetriever | pub struct PdfFileRetriever<'renderer> { |
| src/data/redactions_data.rs | 57 | fn | new_from_bytes | pub fn new_from_bytes( |
| src/data/result_data_publisher.rs | 6 | struct | ResultPublishPaths | pub struct ResultPublishPaths<'a> { |
| src/data/result_data_publisher.rs | 14 | struct | ResultPublishPayload | pub struct ResultPublishPayload<'a> { |
| src/data/result_data_publisher.rs | 22 | struct | ResultPublishRequest | pub struct ResultPublishRequest<'a> { |
| src/data/result_data_publisher.rs | 28 | struct | ResultDataPublisher | pub struct ResultDataPublisher { |
| src/data/result_data_publisher.rs | 34 | fn | new | pub fn new() -> Self { |
| src/data/result_data_publisher.rs | 41 | fn | publish | pub fn publish(&self, req: ResultPublishRequest<'_>) -> Result<(), String> { |
| src/data/result_data_publisher.rs | 58 | fn | publish_bytes | pub fn publish_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), String> { |
| src/data/visualization_data.rs | 18 | struct | VisualizationInputs | pub struct VisualizationInputs { |
| src/data/visualization_data.rs | 25 | struct | VisualizationData | pub struct VisualizationData; |
| src/data/visualization_data.rs | 36 | fn | new | pub fn new() -> Self { |
| src/data/visualization_data.rs | 41 | fn | load_inputs_from_bytes | pub fn load_inputs_from_bytes( |
| src/data/visualization_data.rs | 62 | fn | render_visualized_pdf_from_bytes | pub fn render_visualized_pdf_from_bytes( |
| src/dependency/file_store.rs | 7 | struct | FileReadRequest | pub struct FileReadRequest { |
| src/dependency/file_store.rs | 12 | struct | FileReadResponse | pub struct FileReadResponse { |
| src/dependency/file_store.rs | 16 | trait | FileAccessor | pub trait FileAccessor { |
| src/dependency/file_store.rs | 21 | struct | FileStore | pub struct FileStore; |
| src/dependency/file_store.rs | 25 | fn | read | pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> { |
| src/dependency/file_store.rs | 30 | fn | write_exact | pub fn write_exact(&self, path: &Path, bytes: &[u8]) -> Result<(), String> { |
| src/dependency/file_store.rs | 36 | fn | write | pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> { |
| src/dependency/file_store.rs | 46 | fn | create_dir_all | pub fn create_dir_all(&self, path: &Path) -> Result<(), String> { |
| src/dependency/file_store.rs | 52 | fn | read_dir | pub fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, String> { |
| src/dependency/file_store.rs | 69 | fn | exists | pub fn exists(&self, path: &Path) -> Result<bool, String> { |
| src/dependency/file_store.rs | 75 | fn | is_dir | pub fn is_dir(&self, path: &Path) -> Result<bool, String> { |
| src/dependency/file_store.rs | 94 | fn | validate_read_request | pub fn validate_read_request(req: &FileReadRequest) -> Result<(), String> { |
| src/dependency/hayro_renderer.rs | 11 | struct | HayroRenderer | pub struct HayroRenderer { |
| src/dependency/hayro_renderer.rs | 18 | fn | new | pub fn new<PathLike>(path: PathLike) -> Result<Self, String> |
| src/dependency/hayro_renderer.rs | 27 | fn | new_from_bytes | pub fn new_from_bytes(bytes: &[u8]) -> Result<Self, String> { |
| src/dependency/hayro_renderer.rs | 38 | fn | is_available | pub fn is_available() -> bool { |
| src/dependency/mod.rs | 1 | mod | file_store | pub mod file_store; |
| src/dependency/mod.rs | 2 | mod | hayro_renderer | pub mod hayro_renderer; |
| src/dependency/mod.rs | 3 | mod | pdf_annotator | pub mod pdf_annotator; |
| src/dependency/mod.rs | 4 | mod | pdf_font_occurrence_accessor | pub mod pdf_font_occurrence_accessor; |
| src/dependency/mod.rs | 5 | mod | pdf_font_run_accessor | pub mod pdf_font_run_accessor; |
| src/dependency/mod.rs | 6 | mod | pdf_redaction_accessor | pub mod pdf_redaction_accessor; |
| src/dependency/pdf_annotator.rs | 7 | struct | PdfAnnotator | pub struct PdfAnnotator; |
| src/dependency/pdf_annotator.rs | 11 | fn | annotate | pub fn annotate( |
| src/dependency/pdf_font_occurrence_accessor.rs | 11 | struct | DataBuildConfig | pub struct DataBuildConfig { |
| src/dependency/pdf_font_occurrence_accessor.rs | 15 | struct | FileDataBuilder | pub struct FileDataBuilder<'accessor> { |
| src/dependency/pdf_font_occurrence_accessor.rs | 21 | fn | new | pub fn new(accessor: &'accessor dyn FileAccessor) -> Self { |
| src/dependency/pdf_font_occurrence_accessor.rs | 27 | fn | build_file_font_report | pub fn build_file_font_report( |
| src/dependency/pdf_font_occurrence_accessor.rs | 49 | fn | build_file_font_report_from_bytes | pub fn build_file_font_report_from_bytes( |
| src/dependency/pdf_font_run_accessor.rs | 74 | fn | build_font_run_report | pub fn build_font_run_report(path: &Path, bytes: &[u8]) -> Result<FontRunReport, String> { |
| src/dependency/pdf_font_run_accessor.rs | 79 | fn | build_font_run_report_from_input_name | pub fn build_font_run_report_from_input_name( |
| src/dependency/pdf_redaction_accessor.rs | 10 | trait | RedactionDataRetriever | pub trait RedactionDataRetriever { |
| src/dependency/pdf_redaction_accessor.rs | 31 | struct | PdfFileRetriever | pub struct PdfFileRetriever<'renderer> { |
| src/dependency/pdf_redaction_accessor.rs | 39 | fn | new_from_bytes | pub fn new_from_bytes( |
| src/dependency/pdf_redaction_accessor.rs | 48 | fn | new | pub fn new(doc: Document, renderer: Option<&'renderer dyn PdfRenderer>) -> Self { |
| src/lib.rs | 22 | mod | data | pub mod data; |
| src/lib.rs | 23 | mod | dependency | pub mod dependency; |
| src/lib.rs | 24 | mod | logic | pub mod logic; |
| src/lib.rs | 25 | mod | service | pub mod service; |
| src/lib.rs | 26 | mod | types | pub mod types; |
| src/logic/dictionary_list_convertion_component.rs | 4 | enum | DictionaryListInput | pub enum DictionaryListInput { |
| src/logic/dictionary_list_convertion_component.rs | 10 | struct | DictionaryListRequest | pub struct DictionaryListRequest { |
| src/logic/dictionary_list_convertion_component.rs | 15 | struct | DictionaryListOutputs | pub struct DictionaryListOutputs { |
| src/logic/dictionary_list_convertion_component.rs | 21 | fn | run_dictionary_list_convertion_component | pub fn run_dictionary_list_convertion_component( |
| src/logic/file_byte_convertion_component.rs | 4 | struct | EncodedPipelineOutputs | pub struct EncodedPipelineOutputs { |
| src/logic/file_byte_convertion_component.rs | 12 | fn | encode_outputs | pub fn encode_outputs(outputs: &BytesPipelineOutputs) -> Result<EncodedPipelineOutputs, String> { |
| src/logic/local_file_workflow_component.rs | 16 | struct | OutputFilePaths | pub struct OutputFilePaths { |
| src/logic/local_file_workflow_component.rs | 24 | fn | read_input_pdf_bytes | pub fn read_input_pdf_bytes(input: &Path) -> Result<Vec<u8>, String> { |
| src/logic/local_file_workflow_component.rs | 30 | fn | build_output_file_paths | pub fn build_output_file_paths(input: &Path, output_dir: &Path) -> Result<OutputFilePaths, String> { |
| src/logic/local_file_workflow_component.rs | 44 | fn | write_encoded_outputs | pub fn write_encoded_outputs( |
| src/logic/local_file_workflow_component.rs | 66 | fn | read_dictionary_input | pub fn read_dictionary_input( |
| src/logic/local_file_workflow_component.rs | 79 | fn | validate_batch_input_directory | pub fn validate_batch_input_directory(input_dir: &Path) -> Result<(), String> { |
| src/logic/local_file_workflow_component.rs | 98 | fn | discover_pdf_inputs | pub fn discover_pdf_inputs(input_dir: &Path) -> Result<Vec<PathBuf>, String> { |
| src/logic/local_file_workflow_component.rs | 127 | fn | ensure_batch_output_dir_for_input | pub fn ensure_batch_output_dir_for_input( |
| src/logic/local_file_workflow_component.rs | 158 | fn | write_batch_manifest | pub fn write_batch_manifest(output_dir: &Path, payload: &[u8]) -> Result<PathBuf, String> { |
| src/logic/mod.rs | 1 | mod | dictionary_list_convertion_component | pub mod dictionary_list_convertion_component; |
| src/logic/mod.rs | 2 | mod | file_byte_convertion_component | pub mod file_byte_convertion_component; |
| src/logic/mod.rs | 4 | mod | local_file_workflow_component | pub mod local_file_workflow_component; |
| src/logic/mod.rs | 5 | mod | redaction_guessing_component | pub mod redaction_guessing_component; |
| src/logic/mod.rs | 6 | mod | time | pub mod time; |
| src/logic/mod.rs | 7 | mod | types | pub mod types; |
| src/logic/mod.rs | 8 | mod | visualization_render_component | pub mod visualization_render_component; |
| src/logic/mod.rs | 13 | use | file_byte_convertion_component::{encode_outputs, EncodedPipelineOutputs} | pub use file_byte_convertion_component::{encode_outputs, EncodedPipelineOutputs}; |
| src/logic/mod.rs | 24 | use | types::{BytesPipelineOutputs, BytesPipelineRequest, PipelineConfig, VisualizationPayload} | pub use types::{BytesPipelineOutputs, BytesPipelineRequest, PipelineConfig, VisualizationPayload}; |
| src/logic/redaction_guessing_component.rs | 10 | fn | run_redaction_guessing_component | pub fn run_redaction_guessing_component( |
| src/logic/redaction_guessing_component.rs | 110 | struct | RunGuessFromBytesRequest | pub struct RunGuessFromBytesRequest<'a> { |
| src/logic/redaction_guessing_component.rs | 120 | fn | run_from_bytes | pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<GuessReport, String> { |
| src/logic/redaction_guessing_component.rs | 3853 | fn | run_redaction_scan_from_bytes | pub fn run_redaction_scan_from_bytes( |
| src/logic/redaction_guessing_component.rs | 3863 | fn | run_redaction_scan | pub fn run_redaction_scan( |
| src/logic/redaction_guessing_component.rs | 3916 | fn | build_report_from_input_name | pub fn build_report_from_input_name( |
| src/logic/redaction_guessing_component.rs | 4591 | use | guess_impl::{run_from_bytes as run_guess_from_bytes, RunGuessFromBytesRequest} | pub use guess_impl::{run_from_bytes as run_guess_from_bytes, RunGuessFromBytesRequest}; |
| src/logic/redaction_guessing_component.rs | 4639 | struct | VisualGuessScoreConfig | pub struct VisualGuessScoreConfig { |
| src/logic/redaction_guessing_component.rs | 4676 | fn | apply_visual_scores_from_bytes | pub fn apply_visual_scores_from_bytes( |
| src/logic/time.rs | 3 | struct | Instant | pub struct Instant { |
| src/logic/time.rs | 10 | fn | now | pub fn now() -> Self { |
| src/logic/time.rs | 17 | fn | elapsed | pub fn elapsed(&self) -> std::time::Duration { |
| src/logic/time.rs | 24 | use | std::time::Instant | pub use std::time::Instant; |
| src/logic/types/mod.rs | 7 | struct | PipelineConfig | pub struct PipelineConfig { |
| src/logic/types/mod.rs | 31 | struct | BytesPipelineRequest | pub struct BytesPipelineRequest { |
| src/logic/types/mod.rs | 40 | struct | VisualizationPayload | pub struct VisualizationPayload { |
| src/logic/types/mod.rs | 46 | struct | BytesPipelineOutputs | pub struct BytesPipelineOutputs { |
| src/logic/visualization_render_component.rs | 7 | struct | VisualizationRenderRequest | pub struct VisualizationRenderRequest<'a> { |
| src/logic/visualization_render_component.rs | 15 | fn | run_visualization_render_component | pub fn run_visualization_render_component( |
| src/service/mod.rs | 2 | mod | unredact_cli_entry | pub mod unredact_cli_entry; |
| src/service/mod.rs | 4 | mod | unredact_web_bindings | pub mod unredact_web_bindings; |
| src/service/mod.rs | 6 | mod | unredact_web_entry | pub mod unredact_web_entry; |
| src/service/unredact_cli_entry.rs | 18 | struct | UnredactServiceConfig | pub struct UnredactServiceConfig { |
| src/service/unredact_cli_entry.rs | 42 | struct | UnredactServiceRequest | pub struct UnredactServiceRequest { |
| src/service/unredact_cli_entry.rs | 50 | struct | UnredactServiceOutputs | pub struct UnredactServiceOutputs { |
| src/service/unredact_cli_entry.rs | 58 | enum | BatchFileStatus | pub enum BatchFileStatus { |
| src/service/unredact_cli_entry.rs | 64 | struct | UnredactBatchRequest | pub struct UnredactBatchRequest { |
| src/service/unredact_cli_entry.rs | 72 | struct | UnredactBatchFileResult | pub struct UnredactBatchFileResult { |
| src/service/unredact_cli_entry.rs | 84 | struct | UnredactBatchOutputs | pub struct UnredactBatchOutputs { |
| src/service/unredact_cli_entry.rs | 93 | fn | run_from_paths | pub fn run_from_paths( |
| src/service/unredact_cli_entry.rs | 108 | fn | run_batch_from_paths | pub fn run_batch_from_paths( |
| src/service/unredact_cli_entry.rs | 123 | fn | run | pub fn run(req: UnredactServiceRequest) -> Result<UnredactServiceOutputs, String> { |
| src/service/unredact_cli_entry.rs | 173 | fn | run_batch | pub fn run_batch(req: UnredactBatchRequest) -> Result<UnredactBatchOutputs, String> { |
| src/service/unredact_web_bindings.rs | 7 | fn | run_unredact_web | pub fn run_unredact_web(request: JsValue) -> Result<JsValue, JsValue> { |
| src/service/unredact_web_entry.rs | 13 | struct | UnredactWebConfig | pub struct UnredactWebConfig { |
| src/service/unredact_web_entry.rs | 37 | struct | UnredactWebRequest | pub struct UnredactWebRequest { |
| src/service/unredact_web_entry.rs | 45 | struct | UnredactWebOutputs | pub struct UnredactWebOutputs { |
| src/service/unredact_web_entry.rs | 53 | fn | run | pub fn run(req: UnredactWebRequest) -> Result<UnredactWebOutputs, String> { |
| src/types/file_types.rs | 7 | enum | OutputFormat | pub enum OutputFormat { |
| src/types/file_types.rs | 12 | struct | FontProcessInput | pub struct FontProcessInput { |
| src/types/file_types.rs | 20 | struct | EncodedOutput | pub struct EncodedOutput { |
| src/types/file_types.rs | 28 | enum | InputFileKind | pub enum InputFileKind { |
| src/types/file_types.rs | 36 | enum | TextSourceKind | pub enum TextSourceKind { |
| src/types/file_types.rs | 43 | struct | FontDetectionReport | pub struct FontDetectionReport { |
| src/types/file_types.rs | 48 | struct | FontRunReport | pub struct FontRunReport { |
| src/types/file_types.rs | 55 | struct | FontTextRun | pub struct FontTextRun { |
| src/types/file_types.rs | 77 | struct | FontAsset | pub struct FontAsset { |
| src/types/file_types.rs | 85 | struct | FileFontReport | pub struct FileFontReport { |
| src/types/file_types.rs | 94 | struct | FontsFound | pub struct FontsFound { |
| src/types/file_types.rs | 100 | struct | FontCount | pub struct FontCount { |
| src/types/file_types.rs | 106 | struct | FontId | pub struct FontId { |
| src/types/file_types.rs | 112 | struct | FontOccurrences | pub struct FontOccurrences { |
| src/types/file_types.rs | 117 | struct | FontOccurrence | pub struct FontOccurrence { |
| src/types/file_types.rs | 125 | struct | DocumentLocation | pub struct DocumentLocation { |
| src/types/file_types.rs | 131 | struct | Region | pub struct Region { |
| src/types/file_types.rs | 136 | struct | Rect | pub struct Rect { |
| src/types/file_types.rs | 145 | fn | new | pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self { |
| src/types/file_types.rs | 155 | fn | distinct_fonts_from_counts | pub fn distinct_fonts_from_counts(counts: &[FontCount]) -> Vec<FontId> { |
| src/types/file_types.rs | 165 | fn | aggregate_counts | pub fn aggregate_counts(occurrences: &[FontOccurrence]) -> Vec<FontCount> { |
| src/types/guess_types.rs | 6 | struct | GuessConfig | pub struct GuessConfig { |
| src/types/guess_types.rs | 24 | struct | GuessReport | pub struct GuessReport { |
| src/types/guess_types.rs | 32 | struct | RedactionGuess | pub struct RedactionGuess { |
| src/types/guess_types.rs | 51 | struct | GuessCandidate | pub struct GuessCandidate { |
| src/types/guess_types.rs | 61 | struct | GuessContext | pub struct GuessContext { |
| src/types/mod.rs | 1 | mod | file_types | pub mod file_types; |
| src/types/mod.rs | 2 | mod | guess_types | pub mod guess_types; |
| src/types/mod.rs | 3 | mod | redaction_types | pub mod redaction_types; |
| src/types/mod.rs | 4 | mod | text_overlay | pub mod text_overlay; |
| src/types/mod.rs | 5 | mod | visualizer_config | pub mod visualizer_config; |
| src/types/mod.rs | 7 | use | guess_types::{GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess} | pub use guess_types::{GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess}; |
| src/types/mod.rs | 12 | use | text_overlay::TextOverlay | pub use text_overlay::TextOverlay; |
| src/types/mod.rs | 13 | use | visualizer_config::VisualizerConfig | pub use visualizer_config::VisualizerConfig; |
| src/types/redaction_types.rs | 5 | struct | Rect | pub struct Rect { |
| src/types/redaction_types.rs | 14 | fn | new | pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self { |
| src/types/redaction_types.rs | 26 | fn | width | pub fn width(&self) -> f32 { |
| src/types/redaction_types.rs | 31 | fn | height | pub fn height(&self) -> f32 { |
| src/types/redaction_types.rs | 36 | fn | area | pub fn area(&self) -> f32 { |
| src/types/redaction_types.rs | 42 | struct | UnderlyingTextHit | pub struct UnderlyingTextHit { |
| src/types/redaction_types.rs | 49 | struct | RedactionOccurrence | pub struct RedactionOccurrence { |
| src/types/redaction_types.rs | 60 | enum | RedactionKind | pub enum RedactionKind { |
| src/types/redaction_types.rs | 69 | struct | RedactionReport | pub struct RedactionReport { |
| src/types/redaction_types.rs | 78 | enum | RedactionMode | pub enum RedactionMode { |
| src/types/redaction_types.rs | 85 | struct | RedactionFinderConfig | pub struct RedactionFinderConfig { |
| src/types/redaction_types.rs | 107 | struct | RedactionFinderOutput | pub struct RedactionFinderOutput { |
| src/types/redaction_types.rs | 113 | struct | RenderedPage | pub struct RenderedPage { |
| src/types/redaction_types.rs | 120 | trait | PdfRenderer | pub trait PdfRenderer { |
| src/types/text_overlay.rs | 4 | struct | TextOverlay | pub struct TextOverlay { |
| src/types/visualizer_config.rs | 4 | struct | VisualizerConfig | pub struct VisualizerConfig { |

## Rust Cross-File Imports (use crate::... / use unredact::...)

| Consumer File | Line | Source Crate | Imported Path | Statement |
|---|---:|---|---|---|
| src/bin/guess_accuracy_benchmark.rs | 6 | unredact | data::DEFAULT_NAME_DICTIONARY | use unredact::data::DEFAULT_NAME_DICTIONARY; |
| src/bin/guess_accuracy_benchmark.rs | 7 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| src/bin/guess_accuracy_benchmark.rs | 8 | unredact | types::guess_types::{GuessConfig, GuessReport, RedactionGuess} | use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess}; |
| src/bin/guess_accuracy_benchmark.rs | 9 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| src/bin/pdf_to_png.rs | 4 | unredact | dependency::hayro_renderer::HayroRenderer | use unredact::dependency::hayro_renderer::HayroRenderer; |
| src/bin/pdf_to_png.rs | 5 | unredact | types::redaction_types::PdfRenderer | use unredact::types::redaction_types::PdfRenderer; |
| src/bin/visual_score_impact_benchmark.rs | 7 | unredact | data::redactions_data::{PdfFileRetriever, RedactionDataRetriever} | use unredact::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever}; |
| src/bin/visual_score_impact_benchmark.rs | 8 | unredact | data::DictionaryData | use unredact::data::DictionaryData; |
| src/bin/visual_score_impact_benchmark.rs | 9 | unredact | logic::{run_guess_from_bytes, RunGuessFromBytesRequest} | use unredact::logic::{run_guess_from_bytes, RunGuessFromBytesRequest}; |
| src/bin/visual_score_impact_benchmark.rs | 10 | unredact | types::guess_types::{GuessConfig, RedactionGuess} | use unredact::types::guess_types::{GuessConfig, RedactionGuess}; |
| src/data/fonts_data.rs | 4 | crate | dependency::pdf_font_run_accessor::build_font_run_report_from_input_name | use crate::dependency::pdf_font_run_accessor::build_font_run_report_from_input_name; |
| src/data/guess_validation_data.rs | 3 | crate | dependency::file_store::FileStore | use crate::dependency::file_store::FileStore; |
| src/data/guess_validation_data.rs | 4 | crate | types::file_types::FontDetectionReport | use crate::types::file_types::FontDetectionReport; |
| src/data/guess_validation_data.rs | 5 | crate | types::guess_types::GuessReport | use crate::types::guess_types::GuessReport; |
| src/data/guess_validation_data.rs | 6 | crate | types::redaction_types::RedactionReport | use crate::types::redaction_types::RedactionReport; |
| src/data/local_file_workflow_data.rs | 3 | crate | dependency::file_store::FileStore | use crate::dependency::file_store::FileStore; |
| src/data/redactions_data.rs | 1 | crate | dependency::hayro_renderer::HayroRenderer | use crate::dependency::hayro_renderer::HayroRenderer; |
| src/data/redactions_data.rs | 2 | crate | dependency::pdf_redaction_accessor::PdfFileRetriever as DependencyPdfFileRetriever | use crate::dependency::pdf_redaction_accessor::PdfFileRetriever as DependencyPdfFileRetriever; |
| src/data/redactions_data.rs | 3 | crate | dependency::pdf_redaction_accessor::RedactionDataRetriever as DependencyRedactionDataRetriever | use crate::dependency::pdf_redaction_accessor::RedactionDataRetriever as DependencyRedactionDataRetriever; |
| src/data/result_data_publisher.rs | 3 | crate | dependency::file_store::FileStore | use crate::dependency::file_store::FileStore; |
| src/data/visualization_data.rs | 5 | crate | dependency::pdf_annotator::PdfAnnotator | use crate::dependency::pdf_annotator::PdfAnnotator; |
| src/data/visualization_data.rs | 6 | crate | types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect} | use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect}; |
| src/data/visualization_data.rs | 7 | crate | types::guess_types::{GuessReport, RedactionGuess} | use crate::types::guess_types::{GuessReport, RedactionGuess}; |
| src/data/visualization_data.rs | 8 | crate | types::redaction_types::{Rect, RedactionKind, RedactionReport} | use crate::types::redaction_types::{Rect, RedactionKind, RedactionReport}; |
| src/data/visualization_data.rs | 9 | crate | types::text_overlay::TextOverlay | use crate::types::text_overlay::TextOverlay; |
| src/data/visualization_data.rs | 10 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/data/visualization_data.rs | 785 | crate | types::file_types::FontRunReport | use crate::types::file_types::FontRunReport; |
| src/data/visualization_data.rs | 786 | crate | types::guess_types::{GuessCandidate, GuessContext, GuessReport, RedactionGuess} | use crate::types::guess_types::{GuessCandidate, GuessContext, GuessReport, RedactionGuess}; |
| src/data/visualization_data.rs | 790 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/dependency/hayro_renderer.rs | 9 | crate | types::redaction_types::{PdfRenderer, RenderedPage} | use crate::types::redaction_types::{PdfRenderer, RenderedPage}; |
| src/dependency/pdf_annotator.rs | 3 | crate | types::redaction_types::Rect | use crate::types::redaction_types::Rect; |
| src/dependency/pdf_annotator.rs | 4 | crate | types::text_overlay::TextOverlay | use crate::types::text_overlay::TextOverlay; |
| src/dependency/pdf_font_occurrence_accessor.rs | 1 | crate | dependency::file_store::{FileAccessor, FileReadRequest} | use crate::dependency::file_store::{FileAccessor, FileReadRequest}; |
| src/dependency/pdf_font_occurrence_accessor.rs | 670 | crate | dependency::file_store::{FileAccessor, FileReadRequest, FileReadResponse} | use crate::dependency::file_store::{FileAccessor, FileReadRequest, FileReadResponse}; |
| src/dependency/pdf_font_occurrence_accessor.rs | 671 | crate | types::file_types::{InputFileKind, TextSourceKind} | use crate::types::file_types::{InputFileKind, TextSourceKind}; |
| src/dependency/pdf_font_run_accessor.rs | 1 | crate | types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect} | use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect}; |
| src/logic/dictionary_list_convertion_component.rs | 1 | crate | data::DictionaryData | use crate::data::DictionaryData; |
| src/logic/file_byte_convertion_component.rs | 1 | crate | logic::types::BytesPipelineOutputs | use crate::logic::types::BytesPipelineOutputs; |
| src/logic/redaction_guessing_component.rs | 1 | crate | data::fonts_data::FontsData | use crate::data::fonts_data::FontsData; |
| src/logic/redaction_guessing_component.rs | 2 | crate | data::redactions_data::RedactionsData | use crate::data::redactions_data::RedactionsData; |
| src/logic/redaction_guessing_component.rs | 3 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/logic/redaction_guessing_component.rs | 4 | crate | logic::types::{BytesPipelineOutputs, BytesPipelineRequest, VisualizationPayload} | use crate::logic::types::{BytesPipelineOutputs, BytesPipelineRequest, VisualizationPayload}; |
| src/logic/redaction_guessing_component.rs | 5 | crate | types::redaction_types::{RedactionFinderConfig, RedactionMode} | use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode}; |
| src/logic/redaction_guessing_component.rs | 100 | crate | dependency::pdf_font_run_accessor::build_font_run_report_from_input_name | use crate::dependency::pdf_font_run_accessor::build_font_run_report_from_input_name; |
| src/logic/redaction_guessing_component.rs | 101 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/logic/redaction_guessing_component.rs | 102 | crate | types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect} | use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect}; |
| src/logic/redaction_guessing_component.rs | 3695 | crate | types::guess_types::GuessConfig | use crate::types::guess_types::GuessConfig; |
| src/logic/redaction_guessing_component.rs | 3696 | crate | types::redaction_types::{RedactionFinderConfig, RedactionMode} | use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode}; |
| src/logic/redaction_guessing_component.rs | 3833 | crate | data::redactions_data::{PdfFileRetriever, RedactionDataRetriever} | use crate::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever}; |
| src/logic/redaction_guessing_component.rs | 3834 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/logic/redaction_guessing_component.rs | 4351 | crate | data::redactions_data::RedactionDataRetriever | use crate::data::redactions_data::RedactionDataRetriever; |
| src/logic/redaction_guessing_component.rs | 4602 | crate | data::visualization_data::{VisualizationData, VisualizationInputs} | use crate::data::visualization_data::{VisualizationData, VisualizationInputs}; |
| src/logic/redaction_guessing_component.rs | 4603 | crate | dependency::hayro_renderer::HayroRenderer | use crate::dependency::hayro_renderer::HayroRenderer; |
| src/logic/redaction_guessing_component.rs | 4604 | crate | dependency::pdf_annotator::PdfAnnotator | use crate::dependency::pdf_annotator::PdfAnnotator; |
| src/logic/redaction_guessing_component.rs | 4605 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/logic/redaction_guessing_component.rs | 4606 | crate | types::file_types::FontRunReport | use crate::types::file_types::FontRunReport; |
| src/logic/redaction_guessing_component.rs | 4607 | crate | types::guess_types::{GuessReport, RedactionGuess} | use crate::types::guess_types::{GuessReport, RedactionGuess}; |
| src/logic/redaction_guessing_component.rs | 4608 | crate | types::redaction_types::{PdfRenderer as _, Rect, RedactionReport, RenderedPage} | use crate::types::redaction_types::{PdfRenderer as _, Rect, RedactionReport, RenderedPage}; |
| src/logic/redaction_guessing_component.rs | 4609 | crate | types::text_overlay::TextOverlay | use crate::types::text_overlay::TextOverlay; |
| src/logic/redaction_guessing_component.rs | 5879 | crate | types::guess_types::GuessConfig | use crate::types::guess_types::GuessConfig; |
| src/logic/redaction_guessing_component.rs | 5880 | crate | types::redaction_types::{RedactionFinderConfig, RedactionMode} | use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode}; |
| src/logic/types/mod.rs | 1 | crate | types::file_types::{FontDetectionReport, FontRunReport} | use crate::types::file_types::{FontDetectionReport, FontRunReport}; |
| src/logic/types/mod.rs | 2 | crate | types::guess_types::{GuessConfig, GuessReport} | use crate::types::guess_types::{GuessConfig, GuessReport}; |
| src/logic/types/mod.rs | 3 | crate | types::redaction_types::RedactionReport | use crate::types::redaction_types::RedactionReport; |
| src/logic/types/mod.rs | 4 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/logic/visualization_render_component.rs | 1 | crate | data::visualization_data::VisualizationData | use crate::data::visualization_data::VisualizationData; |
| src/logic/visualization_render_component.rs | 2 | crate | logic::types::VisualizationPayload | use crate::logic::types::VisualizationPayload; |
| src/logic/visualization_render_component.rs | 3 | crate | types::guess_types::GuessReport | use crate::types::guess_types::GuessReport; |
| src/logic/visualization_render_component.rs | 4 | crate | types::redaction_types::RedactionReport | use crate::types::redaction_types::RedactionReport; |
| src/logic/visualization_render_component.rs | 5 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/main.rs | 7 | unredact | types::guess_types::GuessConfig | use unredact::types::guess_types::GuessConfig; |
| src/main.rs | 8 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| src/service/unredact_cli_entry.rs | 5 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/service/unredact_cli_entry.rs | 14 | crate | types::guess_types::GuessConfig | use crate::types::guess_types::GuessConfig; |
| src/service/unredact_cli_entry.rs | 15 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/service/unredact_web_entry.rs | 3 | crate | logic::time::Instant | use crate::logic::time::Instant; |
| src/service/unredact_web_entry.rs | 9 | crate | types::guess_types::GuessConfig | use crate::types::guess_types::GuessConfig; |
| src/service/unredact_web_entry.rs | 10 | crate | types::visualizer_config::VisualizerConfig | use crate::types::visualizer_config::VisualizerConfig; |
| src/types/guess_types.rs | 3 | crate | types::redaction_types::Rect | use crate::types::redaction_types::Rect; |
| src/types/text_overlay.rs | 1 | crate | types::redaction_types::Rect | use crate::types::redaction_types::Rect; |
| tests/dictionary_entry_format_behavior.rs | 4 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| tests/dictionary_entry_format_behavior.rs | 5 | unredact | types::guess_types::{GuessConfig, GuessReport, RedactionGuess} | use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess}; |
| tests/dictionary_entry_format_behavior.rs | 6 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/efta00038617_guessing.rs | 4 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| tests/efta00038617_guessing.rs | 5 | unredact | types::guess_types::{GuessConfig, GuessReport, RedactionGuess} | use unredact::types::guess_types::{GuessConfig, GuessReport, RedactionGuess}; |
| tests/efta00038617_guessing.rs | 6 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/efta00101126_guessing.rs | 3 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| tests/efta00101126_guessing.rs | 4 | unredact | types::guess_types::{GuessConfig, GuessReport} | use unredact::types::guess_types::{GuessConfig, GuessReport}; |
| tests/efta00101126_guessing.rs | 5 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/generalization_smoke.rs | 3 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| tests/generalization_smoke.rs | 4 | unredact | types::guess_types::{GuessConfig, GuessReport} | use unredact::types::guess_types::{GuessConfig, GuessReport}; |
| tests/generalization_smoke.rs | 5 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/raster_api.rs | 3 | unredact | service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig} | use unredact::service::unredact_cli_entry::{run_from_paths, UnredactServiceConfig}; |
| tests/raster_api.rs | 4 | unredact | types::guess_types::GuessConfig | use unredact::types::guess_types::GuessConfig; |
| tests/raster_api.rs | 5 | unredact | types::redaction_types::{RedactionKind, RedactionReport} | use unredact::types::redaction_types::{RedactionKind, RedactionReport}; |
| tests/raster_api.rs | 6 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/web_entry.rs | 3 | unredact | service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest} | use unredact::service::unredact_web_entry::{run, UnredactWebConfig, UnredactWebRequest}; |
| tests/web_entry.rs | 4 | unredact | types::guess_types::{GuessConfig, GuessReport} | use unredact::types::guess_types::{GuessConfig, GuessReport}; |
| tests/web_entry.rs | 5 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |
| tests/web_entry_dto_boundary.rs | 4 | unredact | types::guess_types::GuessConfig | use unredact::types::guess_types::GuessConfig; |
| tests/web_entry_dto_boundary.rs | 5 | unredact | types::visualizer_config::VisualizerConfig | use unredact::types::visualizer_config::VisualizerConfig; |

## Web JS Function Interfaces

| File | Line | Function | Signature |
|---|---:|---|---|
| web/app.js | 40 | setStatus | function setStatus(message) { |
| web/app.js | 44 | setPdfPreviewState | function setPdfPreviewState(message) { |
| web/app.js | 48 | setBenchmarkSummary | function setBenchmarkSummary(message) { |
| web/app.js | 52 | clearDownloads | function clearDownloads() { |
| web/app.js | 56 | setDownloads | function setDownloads(items) { |
| web/app.js | 67 | successfulBatchResults | function successfulBatchResults() { |
| web/app.js | 71 | updateBatchZipButtonState | function updateBatchZipButtonState() { |
| web/app.js | 82 | asUint8Array | function asUint8Array(value) { |
| web/app.js | 98 | normalizeNumber | function normalizeNumber(value, fallback = 0) { |
| web/app.js | 103 | formatBytes | function formatBytes(value) { |
| web/app.js | 117 | formatMs | function formatMs(value) { |
| web/app.js | 124 | safeZipPath | function safeZipPath(path) { |
| web/app.js | 133 | makeUniqueZipPath | function makeUniqueZipPath(path, usedLowerPaths) { |
| web/app.js | 150 | writeU16 | function writeU16(view, offset, value) { |
| web/app.js | 154 | writeU32 | function writeU32(view, offset, value) { |
| web/app.js | 170 | crc32Bytes | function crc32Bytes(bytes) { |
| web/app.js | 183 | dosTimestamp | function dosTimestamp(date) { |
| web/app.js | 198 | buildZipLocalHeader | function buildZipLocalHeader(nameBytes, crc32, size, dosDate, dosTime) { |
| web/app.js | 216 | buildZipCentralHeader | function buildZipCentralHeader( |
| web/app.js | 247 | buildZipEndRecord | function buildZipEndRecord(entryCount, centralSize, centralOffset) { |
| web/app.js | 317 | fileDisplayLabel | function fileDisplayLabel(file) { |
| web/app.js | 327 | collectPdfFiles | function collectPdfFiles() { |
| web/app.js | 358 | topGuessText | function topGuessText(row) { |
| web/app.js | 368 | summarizeGuessReport | function summarizeGuessReport(report) { |
| web/app.js | 383 | summarizeGuessReportCompact | function summarizeGuessReportCompact(report) { |
| web/app.js | 391 | clearGuessVisualization | function clearGuessVisualization(message) { |
| web/app.js | 397 | formatMaybeNumber | function formatMaybeNumber(value, digits = 2) { |
| web/app.js | 402 | valueOrDash | function valueOrDash(value) { |
| web/app.js | 410 | exactMatchesText | function exactMatchesText(row) { |
| web/app.js | 417 | topCandidate | function topCandidate(row) { |
| web/app.js | 424 | anchorFontDisplay | function anchorFontDisplay(context) { |
| web/app.js | 439 | guessBboxLabel | function guessBboxLabel(bbox) { |
| web/app.js | 449 | buildFoundRedactionRows | function buildFoundRedactionRows(report) { |
| web/app.js | 479 | appendTextCell | function appendTextCell(row, text, className = "") { |
| web/app.js | 488 | appendGuessCell | function appendGuessCell(row, guessText) { |
| web/app.js | 492 | buildFoundRedactionsTable | function buildFoundRedactionsTable(rows) { |
| web/app.js | 559 | renderGuessVisualization | function renderGuessVisualization(report) { |
| web/app.js | 571 | downloadAnchorFromUrl | function downloadAnchorFromUrl(fileName, url) { |
| web/app.js | 580 | triggerBlobDownload | function triggerBlobDownload(fileName, blob) { |
| web/app.js | 592 | parseJsonBytes | function parseJsonBytes(bytes, label) { |
| web/app.js | 601 | parseJsonText | function parseJsonText(text, label) { |
| web/app.js | 620 | outputBaseName | function outputBaseName(label, id) { |
| web/app.js | 629 | requestAsPromise | function requestAsPromise(request) { |
| web/app.js | 637 | txAsPromise | function txAsPromise(tx) { |
| web/app.js | 647 | openResultsDb | function openResultsDb() { |
| web/app.js | 693 | zipFolderNameForResult | function zipFolderNameForResult(result, sequenceIndex) { |
| web/app.js | 760 | batchZipFileName | function batchZipFileName() { |
| web/app.js | 816 | revokeSelectedOutputUrls | function revokeSelectedOutputUrls() { |
| web/app.js | 835 | clearPdfPreview | function clearPdfPreview() { |
| web/app.js | 840 | renderPdfPreview | function renderPdfPreview(url, label) { |
| web/app.js | 853 | setDownloadsForSelected | function setDownloadsForSelected(result, urls) { |
| web/app.js | 877 | overallBatchSummary | function overallBatchSummary() { |
| web/app.js | 895 | renderBatchResults | function renderBatchResults() { |
| web/app.js | 957 | createHeapSample | function createHeapSample(label, metrics) { |
| web/app.js | 983 | renderBenchmarkProgress | function renderBenchmarkProgress(metrics, activeLabel = null) { |
| web/app.js | 1031 | clearUiVisuals | function clearUiVisuals() { |
| web/app.js | 1060 | setRunningUi | function setRunningUi(busy) { |
| web/app.js | 1066 | buildRequestConfig | function buildRequestConfig() { |
| web/app.js | 1126 | createSuccessResultMeta | function createSuccessResultMeta( |
| web/app.js | 1147 | createErrorResultMeta | function createErrorResultMeta(file, id, error, elapsedMs) { |
| web/app.js | 1234 | delayTick | function delayTick() { |
| web/e2e/web_ui_batch_benchmark.spec.mjs | 28 | resolveWebPath | function resolveWebPath(urlPath) { |
| web/pkg/unredact.js | 23 | __wbg_get_imports | function __wbg_get_imports() { |
| web/pkg/unredact.js | 212 | addHeapObject | function addHeapObject(obj) { |
| web/pkg/unredact.js | 221 | debugString | function debugString(val) { |
| web/pkg/unredact.js | 286 | dropObject | function dropObject(idx) { |
| web/pkg/unredact.js | 292 | getArrayU8FromWasm0 | function getArrayU8FromWasm0(ptr, len) { |
| web/pkg/unredact.js | 298 | getDataViewMemory0 | function getDataViewMemory0() { |
| web/pkg/unredact.js | 305 | getStringFromWasm0 | function getStringFromWasm0(ptr, len) { |
| web/pkg/unredact.js | 311 | getUint8ArrayMemory0 | function getUint8ArrayMemory0() { |
| web/pkg/unredact.js | 318 | getObject | function getObject(idx) { return heap[idx]; } |
| web/pkg/unredact.js | 320 | handleError | function handleError(f, args) { |
| web/pkg/unredact.js | 333 | isLikeNone | function isLikeNone(x) { |
| web/pkg/unredact.js | 337 | passStringToWasm0 | function passStringToWasm0(arg, malloc, realloc) { |
| web/pkg/unredact.js | 374 | takeObject | function takeObject(idx) { |
| web/pkg/unredact.js | 384 | decodeText | function decodeText(ptr, len) { |
| web/pkg/unredact.js | 410 | __wbg_finalize_init | function __wbg_finalize_init(instance, module) { |
| web/pkg/unredact.js | 445 | expectedResponseType | function expectedResponseType(type) { |
| web/pkg/unredact.js | 453 | initSync | function initSync(module) { |

## Web HTML IDs (UI Interface Surface)

| File | Line | ID | Snippet |
|---|---:|---|---|
| web/index.html | 21 | pdfFile | id="pdfFile" |
| web/index.html | 31 | pdfDirectory | id="pdfDirectory" |
| web/index.html | 43 | dictionaryFile | id="dictionaryFile" |
| web/index.html | 52 | enableImageAnalysis | id="enableImageAnalysis" |
| web/index.html | 60 | shouldVisuallyScore | id="shouldVisuallyScore" |
| web/index.html | 67 | visualizeOutput | ><input id="visualizeOutput" type="checkbox" checked /> |
| web/index.html | 73 | runButton | <button id="runButton" disabled>Run Analysis</button> |
| web/index.html | 74 | clearResultsButton | <button id="clearResultsButton" type="button"> |
| web/index.html | 82 | status | <pre id="status">Loading WebAssembly module...</pre> |
| web/index.html | 86 | performancePanel | <details id="performancePanel"> |
| web/index.html | 88 | benchmarkSummary | <pre id="benchmarkSummary">No run yet.</pre> |
| web/index.html | 93 | batchResultsPanel | <details id="batchResultsPanel" open> |
| web/index.html | 97 | downloadBatchZipButton | id="downloadBatchZipButton" |
| web/index.html | 104 | batchResults | <div id="batchResults" class="batch-results empty-state"> |
| web/index.html | 112 | summary | <pre id="summary">No run yet.</pre> |
| web/index.html | 118 | guessVisualization | id="guessVisualization" |
| web/index.html | 127 | pdfPreviewState | <div id="pdfPreviewState" class="empty-state"> |
| web/index.html | 132 | pdfPreview | id="pdfPreview" |
| web/index.html | 140 | downloads | <div id="downloads" class="downloads">No outputs yet.</div> |

