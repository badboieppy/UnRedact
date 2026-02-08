use std::collections::BTreeSet;
use std::path::Path;

use crate::font_detection::logic::types::file_types::FontDetectionReport;
use crate::unredact_orchestrator::dependency::FileStore;

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
        fonts: &FontDetectionReport,
        max_dictionary: usize,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let mut diagnostics = Vec::new();
        let dictionary = if let Some(path) = dictionary_path {
            let bytes = self.file_store.read(path)?;
            let text = String::from_utf8_lossy(&bytes);
            let mut tokens = Vec::new();
            for line in text.lines() {
                tokens.extend(split_into_words(line));
            }
            diagnostics.push("dictionary_source=file".to_owned());
            normalize_dictionary(tokens, max_dictionary)
        } else {
            diagnostics.push("dictionary_source=default_names+fonts".to_owned());
            let mut tokens = default_names_tokens();
            for input in &fonts.inputs {
                if let Some(occurrences) = &input.occurrences {
                    for occ in &occurrences.items {
                        if let Some(text) = &occ.text {
                            tokens.extend(split_into_words(text));
                        }
                    }
                }
            }
            normalize_dictionary(tokens, max_dictionary)
        };
        diagnostics.push(format!("dictionary_size={}", dictionary.len()));
        Ok((dictionary, diagnostics))
    }
}

impl Default for DictionaryData {
    #[inline]
    fn default() -> Self {
        Self::new()
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
fn split_into_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '-' {
            buf.push(ch);
        } else if !buf.is_empty() {
            out.push(buf.clone());
            buf.clear();
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

#[inline]
fn default_names_tokens() -> Vec<String> {
    let raw = include_str!("../../../assets/names.txt");
    raw.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_owned())
        .collect::<Vec<_>>()
}
