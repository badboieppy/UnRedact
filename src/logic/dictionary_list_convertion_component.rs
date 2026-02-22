use crate::data::dictionary_data::DictionaryData;

#[derive(Debug, Clone, PartialEq)]
pub enum DictionaryListInput {
    FileBytes(Vec<u8>),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryListRequest {
    pub dictionary_input: DictionaryListInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryListOutputs {
    pub dictionary_entries: Vec<String>,
    pub dictionary_diagnostics: Vec<String>,
}

#[inline]
pub fn run_dictionary_list_convertion_component(
    req: DictionaryListRequest,
) -> Result<DictionaryListOutputs, String> {
    let dictionary_data = DictionaryData::new();
    let (dictionary_entries, dictionary_diagnostics) = match req.dictionary_input {
        DictionaryListInput::FileBytes(bytes) => {
            let inputs = dictionary_data.load_dictionary_from_bytes(Some(bytes.as_slice()))?;
            (inputs.dictionary, inputs.diagnostics)
        }
        DictionaryListInput::Missing => {
            let inputs = dictionary_data.load_dictionary_from_bytes(None)?;
            (inputs.dictionary, inputs.diagnostics)
        }
    };
    Ok(DictionaryListOutputs {
        dictionary_entries,
        dictionary_diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::run_dictionary_list_convertion_component;
    use super::{DictionaryListInput, DictionaryListRequest};

    #[test]
    fn bytes_dictionary_is_converted_to_entries() {
        let req = DictionaryListRequest {
            dictionary_input: DictionaryListInput::FileBytes(
                b"SARAH KELLEN\nNADIA MARCINKOVA\n".to_vec(),
            ),
        };
        let out = run_dictionary_list_convertion_component(req)
            .expect("dictionary conversion should succeed");
        assert!(out
            .dictionary_entries
            .iter()
            .any(|value| value == "SARAH KELLEN"));
        assert!(out
            .dictionary_diagnostics
            .iter()
            .any(|line| line == "dictionary_source=file"));
    }

    #[test]
    fn missing_dictionary_bytes_uses_default_fallback() {
        let req = DictionaryListRequest {
            dictionary_input: DictionaryListInput::Missing,
        };
        let out = run_dictionary_list_convertion_component(req)
            .expect("fallback dictionary conversion should succeed");
        assert!(!out.dictionary_entries.is_empty());
        assert!(
            out.dictionary_entries.iter().any(|value| value == "Allen"),
            "expected built-in fallback dictionary entries"
        );
        assert!(out
            .dictionary_diagnostics
            .iter()
            .any(|line| line == "dictionary_source=default_names"));
    }
}
