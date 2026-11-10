//! JS error conversion helpers shared by the renderer modules.

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub(super) fn error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

pub(super) fn js_message(e: &JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}
