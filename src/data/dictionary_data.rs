use std::collections::BTreeSet;

use super::default_name_dictionary::DEFAULT_NAME_DICTIONARY;

#[derive(Debug, Clone)]
pub struct DictionaryInputs {
    pub dictionary: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct DictionaryData;

impl DictionaryData {
    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn load_dictionary_from_bytes(
        &self,
        dictionary_bytes: Option<&[u8]>,
    ) -> Result<DictionaryInputs, String> {
        load_dictionary_from_bytes(dictionary_bytes)
    }
}

impl Default for DictionaryData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
pub fn load_dictionary_from_bytes(
    dictionary_bytes: Option<&[u8]>,
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
        normalize_dictionary(entries)
    } else {
        diagnostics.push("dictionary_source=default_names".to_owned());
        build_default_name_dictionary()
    };
    diagnostics.push(format!("dictionary_size={}", dictionary.len()));
    Ok(DictionaryInputs {
        dictionary,
        diagnostics,
    })
}

#[inline]
fn normalize_dictionary(words: Vec<String>) -> Vec<String> {
    let mut set = BTreeSet::<String>::new();
    for word in words {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            continue;
        }
        set.insert(trimmed.to_owned());
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
fn build_default_name_dictionary() -> Vec<String> {
    let mut entries = Vec::<String>::new();
    for value in DEFAULT_NAME_DICTIONARY {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        entries.extend(case_variants(trimmed));
    }
    normalize_dictionary(entries)
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
    use super::DictionaryData;

    #[test]
    fn load_dictionary_from_bytes_dedupes_and_adds_case_variants() {
        let data = DictionaryData::new();
        let loaded = data
            .load_dictionary_from_bytes(Some(b"b\na\nb\n"))
            .expect("expected dictionary bytes to parse");
        assert_eq!(loaded.dictionary, vec!["A", "B", "a", "b"]);
        assert!(loaded
            .diagnostics
            .iter()
            .any(|value| value == "dictionary_source=file"));
    }

    #[test]
    fn missing_dictionary_bytes_falls_back_to_default_names() {
        let data = DictionaryData::new();
        let loaded = data.load_dictionary_from_bytes(None);
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
