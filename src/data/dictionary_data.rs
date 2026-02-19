use std::collections::BTreeSet;
use std::path::Path;

use super::default_name_dictionary::DEFAULT_NAME_DICTIONARY;
use crate::dependency::file_store::FileStore;

#[derive(Debug, Clone)]
pub struct DictionaryInputs {
    pub dictionary: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub trait DictionaryDataSource {
    fn load_dictionary(
        &self,
        dictionary_path: Option<&Path>,
        max_dictionary: usize,
    ) -> Result<DictionaryInputs, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct DictionaryData {
    file_store: FileStore,
}

impl DictionaryData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn build_dictionary(
        &self,
        dictionary_path: Option<&Path>,
        max_dictionary: usize,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let inputs = self.load_dictionary(dictionary_path, max_dictionary)?;
        Ok((inputs.dictionary, inputs.diagnostics))
    }

    #[inline]
    pub fn read_dictionary_bytes(&self, dictionary_path: &Path) -> Result<Vec<u8>, String> {
        self.file_store.read(dictionary_path)
    }

    #[inline]
    pub fn load_dictionary_from_bytes(
        &self,
        dictionary_bytes: Option<&[u8]>,
        max_dictionary: usize,
    ) -> Result<DictionaryInputs, String> {
        load_dictionary_from_bytes(dictionary_bytes, max_dictionary)
    }
}

impl Default for DictionaryData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub fn load_dictionary(
    file_store: &FileStore,
    dictionary_path: Option<&Path>,
    max_dictionary: usize,
) -> Result<DictionaryInputs, String> {
    let dictionary_bytes = match dictionary_path {
        Some(path) => Some(file_store.read(path)?),
        None => None,
    };
    load_dictionary_from_bytes(dictionary_bytes.as_deref(), max_dictionary)
}

#[inline]
pub fn load_dictionary_from_bytes(
    dictionary_bytes: Option<&[u8]>,
    max_dictionary: usize,
) -> Result<DictionaryInputs, String> {
    let mut diagnostics = Vec::<String>::new();
    let dictionary = if let Some(bytes) = dictionary_bytes {
        let text = String::from_utf8_lossy(bytes);
        let mut entries = Vec::<String>::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            entries.extend(case_variants(trimmed));
        }
        diagnostics.push("dictionary_source=file".to_owned());
        normalize_dictionary(entries, max_dictionary)
    } else {
        diagnostics.push("dictionary_source=default_names".to_owned());
        build_default_name_dictionary(max_dictionary)
    };
    diagnostics.push(format!("dictionary_size={}", dictionary.len()));
    Ok(DictionaryInputs {
        dictionary,
        diagnostics,
    })
}

impl DictionaryDataSource for DictionaryData {
    #[inline]
    fn load_dictionary(
        &self,
        dictionary_path: Option<&Path>,
        max_dictionary: usize,
    ) -> Result<DictionaryInputs, String> {
        load_dictionary(&self.file_store, dictionary_path, max_dictionary)
    }
}

#[inline]
fn normalize_dictionary(words: Vec<String>, max_dictionary: usize) -> Vec<String> {
    let mut set = BTreeSet::<String>::new();
    for word in words {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            continue;
        }
        set.insert(trimmed.to_owned());
        if set.len() >= max_dictionary {
            break;
        }
    }
    set.into_iter().collect::<Vec<_>>()
}

#[inline]
fn case_variants(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![
        trimmed.to_owned(),
        trimmed.to_lowercase(),
        trimmed.to_uppercase(),
        title_case(trimmed),
    ]
}

#[inline]
fn build_default_name_dictionary(max_dictionary: usize) -> Vec<String> {
    let mut entries = Vec::<String>::new();
    for value in DEFAULT_NAME_DICTIONARY {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        entries.extend(case_variants(trimmed));
    }
    normalize_dictionary(entries, max_dictionary)
}

fn title_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut new_word = true;
    for ch in value.chars() {
        if ch.is_alphabetic() {
            if new_word {
                out.extend(ch.to_uppercase());
                new_word = false;
            } else {
                out.extend(ch.to_lowercase());
            }
        } else {
            new_word = ch == ' ' || ch == '-' || ch == '\'';
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_dictionary_dedupes_and_orders() {
        let words = vec!["b".to_owned(), "a".to_owned(), "b".to_owned()];
        let out = normalize_dictionary(words, 10);
        assert_eq!(out, vec!["a".to_owned(), "b".to_owned()]);
    }

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

        let loaded = load_dictionary(&FileStore, Some(&tmp_path), 200);
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
        let loaded = load_dictionary(&FileStore, None, 100);
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
