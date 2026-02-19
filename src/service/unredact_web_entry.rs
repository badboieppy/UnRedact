use serde::{Deserialize, Serialize};

use crate::logic::{
    encode_outputs, run_dictionary_list_convertion_component, run_redaction_guessing_component,
    BytesPipelineRequest, DictionaryListInput, DictionaryListRequest, EncodedPipelineOutputs,
    PipelineConfig,
};

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactWebRequest {
    pub input_name: String,
    pub pdf_bytes: Vec<u8>,
    pub dictionary_file_bytes: Option<Vec<u8>>,
    pub cfg: PipelineConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnredactWebOutputs {
    pub redactions_json: Vec<u8>,
    pub fonts_json: Vec<u8>,
    pub guesses_json: Vec<u8>,
    pub visualized_pdf_bytes: Option<Vec<u8>>,
}

#[inline]
pub fn run(req: UnredactWebRequest) -> Result<UnredactWebOutputs, String> {
    let max_dictionary = req.cfg.guess.max_dictionary;
    let dictionary_input = req
        .dictionary_file_bytes
        .map(DictionaryListInput::FileBytes)
        .unwrap_or(DictionaryListInput::Missing);
    let dictionary_outputs = run_dictionary_list_convertion_component(DictionaryListRequest {
        dictionary_input,
        max_dictionary,
    })?;
    let outputs = run_redaction_guessing_component(BytesPipelineRequest {
        input_name: req.input_name,
        pdf_bytes: req.pdf_bytes,
        dictionary_entries: dictionary_outputs.dictionary_entries,
        dictionary_diagnostics: dictionary_outputs.dictionary_diagnostics,
        cfg: req.cfg,
    })?;
    let encoded: EncodedPipelineOutputs = encode_outputs(&outputs)?;
    Ok(UnredactWebOutputs {
        redactions_json: encoded.redactions_json,
        fonts_json: encoded.fonts_json,
        guesses_json: encoded.guesses_json,
        visualized_pdf_bytes: encoded.visualized_pdf_bytes,
    })
}
