use wasm_bindgen::prelude::*;
use web_sys::Window;

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

/**
 * Removes keyborg event listeners and custom focus override
 * @param win The window that stores keyborg focus events
 */
pub fn dispose_focus_event(win: Window) {
    // const kwin = win as WindowWithKeyborgFocusEvent;
    // const proto = kwin.HTMLElement.prototype;
    // const origFocus = (proto.focus as KeyborgFocus).__keyborgNativeFocus;
    // const keyborgNativeFocusEvent = kwin.__keyborgData;

    // if (keyborgNativeFocusEvent) {
    //   kwin.document.removeEventListener(
    //     "focusin",
    //     keyborgNativeFocusEvent.focusInHandler,
    //     true,
    //   );

    //   kwin.document.removeEventListener(
    //     "focusout",
    //     keyborgNativeFocusEvent.focusOutHandler,
    //     true,
    //   );

    //   for (const shadowRootWeakRef of keyborgNativeFocusEvent.shadowTargets) {
    //     const shadowRoot = shadowRootWeakRef.deref();

    //     if (shadowRoot) {
    //       shadowRoot.removeEventListener(
    //         "focusin",
    //         keyborgNativeFocusEvent.focusInHandler,
    //         true,
    //       );
    //       shadowRoot.removeEventListener(
    //         "focusout",
    //         keyborgNativeFocusEvent.focusOutHandler,
    //         true,
    //       );
    //     }
    //   }

    //   keyborgNativeFocusEvent.shadowTargets.clear();

    //   delete kwin.__keyborgData;
    // }

    // if (origFocus) {
    //   proto.focus = origFocus;
    // }
}
