use unredact_core::service::unredact_web_entry::{run, UnredactWebOutputs, UnredactWebRequest};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_unredact_web(request: JsValue) -> Result<JsValue, JsValue> {
    let decoded_request: UnredactWebRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|error| JsValue::from_str(&format!("invalid web request payload: {error}")))?;
    let outputs: UnredactWebOutputs = run(decoded_request)
        .map_err(|error| JsValue::from_str(&format!("web run failed: {error}")))?;
    serde_wasm_bindgen::to_value(&outputs).map_err(|error| {
        JsValue::from_str(&format!("failed to encode web response payload: {error}"))
    })
}
