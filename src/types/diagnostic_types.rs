use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiagnosticValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    pub layer: String,
    pub stage: String,
    pub code: String,
    pub level: DiagnosticLevel,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub row_id: Option<String>,
    #[serde(default)]
    pub redaction_id: Option<String>,
    #[serde(default)]
    pub page_index: Option<u32>,
    #[serde(default)]
    pub bbox: Option<Rect>,
    #[serde(default)]
    pub metrics: BTreeMap<String, DiagnosticValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DiagnosticReport {
    #[serde(default)]
    pub items: Vec<DiagnosticRecord>,
}

impl DiagnosticRecord {
    #[inline]
    pub fn info(layer: &str, stage: &str, code: &str) -> Self {
        Self {
            layer: layer.to_owned(),
            stage: stage.to_owned(),
            code: code.to_owned(),
            level: DiagnosticLevel::Info,
            message: None,
            row_id: None,
            redaction_id: None,
            page_index: None,
            bbox: None,
            metrics: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn warning(layer: &str, stage: &str, code: &str, message: &str) -> Self {
        Self {
            layer: layer.to_owned(),
            stage: stage.to_owned(),
            code: code.to_owned(),
            level: DiagnosticLevel::Warning,
            message: Some(message.to_owned()),
            row_id: None,
            redaction_id: None,
            page_index: None,
            bbox: None,
            metrics: BTreeMap::new(),
        }
    }

    #[inline]
    pub fn error(layer: &str, stage: &str, code: &str, message: &str) -> Self {
        Self {
            layer: layer.to_owned(),
            stage: stage.to_owned(),
            code: code.to_owned(),
            level: DiagnosticLevel::Error,
            message: Some(message.to_owned()),
            row_id: None,
            redaction_id: None,
            page_index: None,
            bbox: None,
            metrics: BTreeMap::new(),
        }
    }
}
