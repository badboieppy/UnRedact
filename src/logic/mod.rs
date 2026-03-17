#[cfg(feature = "local-file-workflow")]
pub(crate) mod local_file_workflow_component;
pub(crate) mod redaction_guessing_component;
#[cfg(feature = "cli-entry")]
pub(crate) mod tooling_component;
pub(crate) mod types;
#[cfg(feature = "cli-entry")]
pub(crate) mod visual_anchor_metrics_component;
#[cfg(feature = "cli-entry")]
pub(crate) mod visual_anchor_research_component;
pub(crate) mod visualization_component;

#[cfg(feature = "local-file-workflow")]
pub(crate) use local_file_workflow_component::{
    build_output_file_paths, build_visual_metric_file_paths, discover_pdf_inputs,
    ensure_batch_output_dir_for_input, read_dictionary_input, read_input_pdf_bytes,
    validate_batch_input_directory, write_batch_manifest, write_encoded_outputs,
    write_output_bytes, write_visual_metric_outputs, OutputFilePaths,
};
pub(crate) use redaction_guessing_component::run_redaction_guessing_component;
pub(crate) use types::{
    encode_outputs, BytesPipelineRequest, EncodedPipelineOutputs, PipelineConfig,
    PipelineExecutionOptions,
};
pub(crate) use visualization_component::{render_visualization, VisualizationRenderRequest};
