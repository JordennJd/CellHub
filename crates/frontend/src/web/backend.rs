//! Graphics backend selection before the canvas context is created.

use super::error::error;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Backend {
    WebGpu,
    WebGl,
}

impl Backend {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::WebGpu => "webgpu",
            Self::WebGl => "webgl",
        }
    }

    pub(super) async fn select(requested: &str) -> Result<Self, JsValue> {
        match requested {
            "webgpu" => {
                if !wgpu::util::is_browser_webgpu_supported().await {
                    return Err(error(
                        "WebGPU недоступен. Используйте браузер с WebGPU через localhost или HTTPS.",
                    ));
                }
                Ok(Self::WebGpu)
            }
            "webgl" => Ok(Self::WebGl),
            _ if wgpu::util::is_browser_webgpu_supported().await => Ok(Self::WebGpu),
            _ => Ok(Self::WebGl),
        }
    }
}
