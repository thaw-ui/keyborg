use wasm_bindgen::prelude::*;

pub const KEYBORG_FOCUSIN: &'static str = "keyborg:focusin";

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Object)]
    pub type KeyborgFocusInEventDetails;

    #[wasm_bindgen(method, getter, js_name = relatedTarget)]
    pub fn related_target(this: &KeyborgFocusInEventDetails) -> web_sys::HtmlElement;

    #[wasm_bindgen(method, getter, js_name = isFocusedProgrammatically)]
    pub fn is_focused_programmatically(this: &KeyborgFocusInEventDetails) -> bool;

    #[wasm_bindgen(method, getter, js_name = originalEvent)]
    pub fn original_event(this: &KeyborgFocusInEventDetails) -> web_sys::FocusEvent;
}
