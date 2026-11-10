//! One `requestAnimationFrame` at a time, owned by Rust.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Rust owns this signal, so JS only reports "new data/view exists"; it never
/// drives the WebGPU paint call.
#[derive(Default, Clone)]
pub(super) struct RepaintSignal {
    callback: Rc<RefCell<Option<js_sys::Function>>>,
    handle: Rc<Cell<Option<i32>>>,
}

impl RepaintSignal {
    pub(super) fn set_callback(&self, callback: js_sys::Function) {
        *self.callback.borrow_mut() = Some(callback);
    }

    pub(super) fn clear_handle(&self) {
        self.handle.set(None);
    }

    pub(super) fn request(&self) {
        if self.handle.get().is_some() {
            return;
        }
        let Some(window) = web_sys::window() else {
            return;
        };
        let callback = self.callback.borrow();
        if let Some(callback) = callback.as_ref() {
            if let Ok(handle) = window.request_animation_frame(callback) {
                self.handle.set(Some(handle));
            }
        }
    }

    pub(super) fn cancel(&self) {
        if let Some(handle) = self.handle.take() {
            if let Some(window) = web_sys::window() {
                let _ = window.cancel_animation_frame(handle);
            }
        }
        self.callback.borrow_mut().take();
    }
}
