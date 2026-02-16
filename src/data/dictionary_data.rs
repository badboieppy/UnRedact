use std::collections::BTreeSet;
use std::path::Path;

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
    let mut diagnostics = Vec::new();
    let dictionary = if let Some(path) = dictionary_path {
        let bytes = file_store.read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut tokens = Vec::new();
        for line in text.lines() {
            tokens.extend(split_into_words(line));
        }
        diagnostics.push("dictionary_source=file".to_owned());
        normalize_dictionary(tokens, max_dictionary)
    } else {
        diagnostics.push("dictionary_source=default_names".to_owned());
        build_name_dictionary(max_dictionary)
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
fn build_name_dictionary(max_dictionary: usize) -> Vec<String> {
    let mut out = Vec::new();
    for base_name in default_names_tokens() {
        for candidate in name_combinations(&base_name) {
            out.extend(case_variants(&candidate));
        }
        if out.len() >= max_dictionary.saturating_mul(4) {
            break;
        }
    }
    normalize_dictionary(out, max_dictionary)
}

fn name_combinations(value: &str) -> Vec<String> {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        return Vec::new();
    }

    let tokens = cleaned
        .split_whitespace()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut set = BTreeSet::new();
    let full = tokens.join(" ");
    set.insert(full.clone());

    let first = tokens.first().copied().unwrap_or_default().to_owned();
    let last = tokens.last().copied().unwrap_or_default().to_owned();
    set.insert(first.clone());
    set.insert(last.clone());

    if tokens.len() >= 2 {
        set.insert(format!("{first} {last}"));
        set.insert(format!("{last}, {first}"));
    }

    set.into_iter().collect::<Vec<_>>()
}

#[inline]
fn case_variants(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lower = value.to_lowercase();
    let upper = value.to_uppercase();
    let title = title_case(value);
    out.push(lower);
    out.push(upper);
    out.push(title);
    out
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
    let raw = include_str!("../../assets/names.txt");
    raw.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_owned())
        .collect::<Vec<_>>()
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
    fn split_into_words_basic() {
        let out = split_into_words("A-b c_d E");
        assert_eq!(
            out,
            vec![
                "A-b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
                "E".to_owned()
            ]
        );
    }
}
