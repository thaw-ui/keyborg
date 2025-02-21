use js_sys::Reflect;
use std::{
    cell::RefCell,
    sync::{Arc, LazyLock, RwLock},
};
use wasm_bindgen::prelude::*;
use web_sys::{CustomEvent, CustomEventInit, Element, FocusEvent, HtmlElement, Window};

pub const KEYBORG_FOCUSIN: &'static str = "keyborg:focusin";
pub const KEYBORG_FOCUSOUT: &'static str = "keyborg:focusout";

fn can_override_native_focus(win: &Window) -> bool {
    let html_element = win.get("HTMLElemnt").unwrap_throw();
    let prototype = Reflect::get(&html_element, &JsValue::from_str("prototype")).unwrap_throw();
    let js_focus = JsValue::from_str("focus");
    let orig_focus = Reflect::get(&prototype, &js_focus).unwrap_throw();

    let is_custom_focus_called = Arc::new(RefCell::new(false));

    let focus_closure: Closure<dyn FnMut()> = Closure::new({
        let is_custom_focus_called = is_custom_focus_called.clone();
        move || {
            *is_custom_focus_called.borrow_mut() = true;
            ()
        }
    });
    Reflect::set(&prototype, &js_focus, &focus_closure.into_js_value()).unwrap_throw();

    let btn = win
        .document()
        .unwrap_throw()
        .create_element("button")
        .unwrap_throw()
        .dyn_into::<HtmlElement>()
        .unwrap_throw();
    let _ = btn.focus();

    Reflect::set(&prototype, &js_focus, &orig_focus).unwrap_throw();

    let rt = *is_custom_focus_called.borrow();
    rt
}

static CAN_OVERRIDE_NATIVE_FOCUS: LazyLock<RwLock<bool>> =
    LazyLock::new(move || RwLock::new(false));

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

/// Overrides the native `focus` and setups the keyborg focus event
pub fn setup_focus_event(win: &Window) {
    let kwin = win;
    // const kwin = win as WindowWithKeyborgFocusEvent;

    if !*CAN_OVERRIDE_NATIVE_FOCUS.read().unwrap_throw() {
        *CAN_OVERRIDE_NATIVE_FOCUS.write().unwrap_throw() = can_override_native_focus(kwin);
    }

    let html_element = kwin.get("HTMLElemnt").unwrap_throw();
    let prototype = Reflect::get(&html_element, &JsValue::from_str("prototype")).unwrap_throw();
    let js_focus = JsValue::from_str("focus");
    let orig_focus = Reflect::get(&prototype, &js_focus).unwrap_throw();
    if Reflect::has(&orig_focus, &JsValue::from_str("__keyborgNativeFocus")).unwrap_throw() {
        // Already set up.
        return;
    }

    let focus = {
        let kwin = win.clone();

        |this: HtmlElement| {
            //   const keyborgNativeFocusEvent = (kwin as WindowWithKeyborgFocusEvent)
            //     .__keyborgData;

            //   if (keyborgNativeFocusEvent) {
            //     keyborgNativeFocusEvent.lastFocusedProgrammatically = new WeakRefInstance(
            //       this,
            //     );
            //   }
            let orig_focus = orig_focus.dyn_ref::<js_sys::Function>().unwrap_throw();
            // js_sys::Function::apply(orig_focus, &this, js_sys::arguments()).unwrap_throw()
        }
    };

    // kwin.HTMLElement.prototype.focus = focus;

    // const shadowTargets: Set<WeakRefInstance<ShadowRoot>> = new Set();

    let focus_out_handler = |e: FocusEvent| {
        let Some(target) = e.target() else {
            return;
        };

        let target = target.dyn_ref::<HtmlElement>().unwrap_throw();

        let init = CustomEventInit::new();
        init.set_cancelable(true);
        init.set_bubbles(true);
        // Allows the event to bubble past an open shadow root
        init.set_composed(true);
        let detail = js_sys::Object::new();
        let _ = Reflect::set(&detail, &JsValue::from_str("originalEvent"), &e);
        init.set_detail(&detail);
        let event = CustomEvent::new_with_event_init_dict(KEYBORG_FOCUSOUT, &init).unwrap_throw();

        let _ = target.dispatch_event(&event);
    };

    let on_focus_in = |
    target: &Element,
  //   relatedTarget?: HTMLElement,
  //   originalEvent?: FocusEvent,
  | {
      let shadow_root = target.shadow_root();
      if let Some(shadow_root) = shadow_root {
          /*
           * https://bugs.chromium.org/p/chromium/issues/detail?id=1512028
           * focusin events don't bubble up through an open shadow root once focus is inside
           * once focus moves into a shadow root - we drop the same focusin handler there
           * keyborg's custom event will still bubble up since it is composed
           * event handlers should be cleaned up once focus leaves the shadow root.
           *
           * When a focusin event is dispatched from a shadow root, its target is the shadow root parent.
           * Each shadow root encounter requires a new capture listener.
           * Why capture? - we want to follow the focus event in order or descending nested shadow roots
           * When there are no more shadow root targets - dispatch the keyborg:focusin event
           *
           * 1. no focus event
           * > document - capture listener ✅
           *   > shadow root 1
           *     > shadow root 2
           *       > shadow root 3
           *         > focused element
           *
           * 2. focus event received by document listener
           * > document - capture listener ✅ (focus event here)
           *   > shadow root 1 - capture listener ✅
           *     > shadow root 2
           *       > shadow root 3
           *         > focused element
   
           * 3. focus event received by root l1 listener
           * > document - capture listener ✅
           *   > shadow root 1 - capture listener ✅ (focus event here)
           *     > shadow root 2 - capture listener ✅
           *       > shadow root 3
           *         > focused element
           *
           * 4. focus event received by root l2 listener
           * > document - capture listener ✅
           *   > shadow root 1 - capture listener ✅
           *     > shadow root 2 - capture listener ✅ (focus event here)
           *       > shadow root 3 - capture listener ✅
           *         > focused element
           *
           * 5. focus event received by root l3 listener, no more shadow root targets
           * > document - capture listener ✅
           *   > shadow root 1 - capture listener ✅
           *     > shadow root 2 - capture listener ✅
           *       > shadow root 3 - capture listener ✅ (focus event here)
           *         > focused element ✅ (no shadow root - dispatch keyborg event)
           */
  
          // for (const shadowRootWeakRef of shadowTargets) {
          //   if (shadowRootWeakRef.deref() === shadowRoot) {
          //     return;
          //   }
          // }
  
      //     shadowRoot.addEventListener("focusin", focusInHandler, true);
      //     shadowRoot.addEventListener("focusout", focusOutHandler, true);
  
      //     shadowTargets.add(new WeakRefInstance(shadowRoot));
  
          return;
        }

        let init = CustomEventInit::new();
        init.set_cancelable(true);
        init.set_bubbles(true);
        // Allows the event to bubble past an open shadow root
        init.set_composed(true);
        let details = js_sys::Object::new();
        //   const details: KeyborgFocusInEventDetails = {
        //     relatedTarget,
        //     originalEvent,
        //   };
        // let _ = Reflect::set(&detail, &JsValue::from_str("relatedTarget"), &e);
        // let _ = Reflect::set(&detail, &JsValue::from_str("originalEvent"), &e);
        init.set_detail(&details);
        let event = CustomEvent::new_with_event_init_dict(KEYBORG_FOCUSIN, &init).unwrap_throw();

        // Tabster (and other users) can still use the legacy details field - keeping for backwards compat
        // event.details = details;

        //   if (_canOverrideNativeFocus || data.lastFocusedProgrammatically) {
        //     details.isFocusedProgrammatically =
        //       target === data.lastFocusedProgrammatically?.deref();

        //     data.lastFocusedProgrammatically = undefined;
        //   }

        let _ = target.dispatch_event(&event);
    };

    let focus_in_handler = |e: FocusEvent| {
        //   const target = e.target as HTMLElement;

        //   if (!target) {
        //     return;
        //   }

        //   let node: Node | null | undefined = e.composedPath()[0] as
        //     | Node
        //     | null
        //     | undefined;

        //   const currentShadows: Set<ShadowRoot> = new Set();

        //   while (node) {
        //     if (node.nodeType === Node.DOCUMENT_FRAGMENT_NODE) {
        //       currentShadows.add(node as ShadowRoot);
        //       node = (node as ShadowRoot).host;
        //     } else {
        //       node = node.parentNode;
        //     }
        //   }

        //   for (const shadowRootWeakRef of shadowTargets) {
        //     const shadowRoot = shadowRootWeakRef.deref();

        //     if (!shadowRoot || !currentShadows.has(shadowRoot)) {
        //       shadowTargets.delete(shadowRootWeakRef);

        //       if (shadowRoot) {
        //         shadowRoot.removeEventListener("focusin", focusInHandler, true);
        //         shadowRoot.removeEventListener("focusout", focusOutHandler, true);
        //       }
        //     }
        //   }

        //   onFocusIn(target, (e.relatedTarget as HTMLElement | null) || undefined);
    };

    // const data: KeyborgFocusEventData = (kwin.__keyborgData = {
    //   focusInHandler,
    //   focusOutHandler,
    //   shadowTargets,
    // });

    // kwin.document.addEventListener(
    //   "focusin",
    //   kwin.__keyborgData.focusInHandler,
    //   true,
    // );

    // kwin.document.addEventListener(
    //   "focusout",
    //   kwin.__keyborgData.focusOutHandler,
    //   true,
    // );

    // function focus(this: HTMLElement) {
    //   const keyborgNativeFocusEvent = (kwin as WindowWithKeyborgFocusEvent)
    //     .__keyborgData;

    //   if (keyborgNativeFocusEvent) {
    //     keyborgNativeFocusEvent.lastFocusedProgrammatically = new WeakRefInstance(
    //       this,
    //     );
    //   }

    //   // eslint-disable-next-line prefer-rest-params
    //   return origFocus.apply(this, arguments);
    // }

    let mut active_element = kwin.document().unwrap_throw().active_element();

    // If keyborg is created with the focus inside shadow root, we need
    // to go through the shadows up to make sure all relevant shadows
    // have focus handlers attached.
    loop {
        let Some(el) = &active_element else {
            break;
        };
        let Some(shadow_root) = &el.shadow_root() else {
            break;
        };

        on_focus_in(el);
        active_element = shadow_root.active_element();
    }

    // (focus as KeyborgFocus).__keyborgNativeFocus = origFocus;
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
