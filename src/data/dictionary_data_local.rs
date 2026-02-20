use std::path::Path;

use crate::dependency::file_store::FileStore;

use super::dictionary_data::{load_dictionary_from_bytes, DictionaryData, DictionaryInputs};

pub trait DictionaryDataSource {
    fn load_dictionary(&self, dictionary_path: Option<&Path>) -> Result<DictionaryInputs, String>;
}

impl DictionaryData {
    #[inline]
    pub fn build_dictionary(
        &self,
        dictionary_path: Option<&Path>,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let inputs = self.load_dictionary(dictionary_path)?;
        Ok((inputs.dictionary, inputs.diagnostics))
    }

    #[inline]
    pub fn read_dictionary_bytes(&self, dictionary_path: &Path) -> Result<Vec<u8>, String> {
        let file_store = FileStore;
        file_store.read(dictionary_path)
    }

    #[inline]
    pub fn load_dictionary(
        &self,
        dictionary_path: Option<&Path>,
    ) -> Result<DictionaryInputs, String> {
        let file_store = FileStore;
        load_dictionary(&file_store, dictionary_path)
    }
}

#[inline]
pub fn load_dictionary(
    file_store: &FileStore,
    dictionary_path: Option<&Path>,
) -> Result<DictionaryInputs, String> {
    let dictionary_bytes = match dictionary_path {
        Some(path) => Some(file_store.read(path)?),
        None => None,
    };
    load_dictionary_from_bytes(dictionary_bytes.as_deref())
}

impl DictionaryDataSource for DictionaryData {
    #[inline]
    fn load_dictionary(&self, dictionary_path: Option<&Path>) -> Result<DictionaryInputs, String> {
        DictionaryData::load_dictionary(self, dictionary_path)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DictionaryData;

    #[test]
    fn file_dictionary_keeps_full_name_candidates() {
        let tmp_path = std::env::temp_dir().join(format!(
            "unredact_dict_test_{}_{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        ));
        let write_result = std::fs::write(
            &tmp_path,
            "MUCINSKA, ADRIANA\nSarah Kellen\nNADIA MARCINKOVA\n",
        );
        assert!(
            write_result.is_ok(),
            "failed to create temp dictionary file for test: {:?}",
            write_result.err()
        );

        let data = DictionaryData::new();
        let loaded = data.load_dictionary(Some(&tmp_path));
        assert!(
            loaded.is_ok(),
            "expected file dictionary to load in test, got {:?}",
            loaded.err()
        );
        let loaded = loaded.expect("dictionary should load in test");
        let items = loaded.dictionary;

        assert!(items.contains(&"MUCINSKA, ADRIANA".to_owned()));
        assert!(items.contains(&"Sarah Kellen".to_owned()));
        assert!(items.contains(&"NADIA MARCINKOVA".to_owned()));
        assert!(!items.contains(&"SARAH".to_owned()));
        assert!(!items.contains(&"KELLEN".to_owned()));

        let remove_result = std::fs::remove_file(tmp_path);
        drop(remove_result);
    }

    #[test]
    fn missing_dictionary_falls_back_to_default_names() {
        let data = DictionaryData::new();
        let loaded = data.load_dictionary(None::<&Path>);
        assert!(
            loaded.is_ok(),
            "expected fallback dictionary when missing input"
        );
        let loaded = loaded.expect("dictionary should load");
        assert!(!loaded.dictionary.is_empty());
        assert!(
            loaded.dictionary.iter().any(|value| value == "Allen"),
            "expected fallback dictionary to include names from built-in default list"
        );
        assert!(loaded
            .diagnostics
            .iter()
            .any(|line| line == "dictionary_source=default_names"));
    }
}
