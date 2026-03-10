#![deny(clippy::all)]
#![warn(clippy::perf)]
#![warn(clippy::complexity)]
#![warn(clippy::style)]
#![warn(clippy::suspicious)]
#![warn(clippy::correctness)]
#![deny(clippy::self_named_module_files)]
#![deny(clippy::unseparated_literal_suffix)]
#![deny(clippy::default_numeric_fallback)]
#![deny(clippy::if_then_some_else_none)]
#![deny(clippy::integer_division)]
#![deny(clippy::integer_division_remainder_used)]
#![deny(clippy::let_underscore_untyped)]
#![deny(clippy::missing_inline_in_public_items)]
#![deny(clippy::pub_without_shorthand)]
#![deny(clippy::str_to_string)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::assertions_on_result_states)]
#![deny(clippy::let_underscore_must_use)]
#![deny(clippy::unused_trait_names)]

#[cfg(all(target_family = "wasm", feature = "web-entry"))]
use wasm_bindgen::prelude::*;

pub mod benchmarks;
mod data;
mod dependency;
mod logic;
pub mod service;
pub mod types;

#[cfg(all(target_family = "wasm", feature = "web-entry"))]
#[allow(clippy::missing_inline_in_public_items)]
#[wasm_bindgen]
pub fn run_unredact_web(request: JsValue) -> Result<JsValue, JsValue> {
    let decoded_request: crate::service::unredact_web_entry::UnredactWebRequest =
        serde_wasm_bindgen::from_value(request)
            .map_err(|error| JsValue::from_str(&format!("invalid web request payload: {error}")))?;
    let outputs: crate::service::unredact_web_entry::UnredactWebOutputs =
        crate::service::unredact_web_entry::run(decoded_request)
            .map_err(|error| JsValue::from_str(&format!("web run failed: {error}")))?;
    serde_wasm_bindgen::to_value(&outputs).map_err(|error| {
        JsValue::from_str(&format!("failed to encode web response payload: {error}"))
    })
}
