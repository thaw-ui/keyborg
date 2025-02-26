use crate::js::WeakRef;
use gloo_events::{EventListener, EventListenerOptions};
use js_sys::{Reflect, Set};
use send_wrapper::SendWrapper;
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};
use wasm_bindgen::prelude::*;
use web_sys::{
    CustomEvent, CustomEventInit, Element, Event, EventTarget, FocusEvent, HtmlElement, Node,
    ShadowRoot, Window,
};

pub const KEYBORG_FOCUSIN: &'static str = "keyborg:focusin";
pub const KEYBORG_FOCUSOUT: &'static str = "keyborg:focusout";

static EVENT_LISTENER_MAP: LazyLock<RwLock<EventListenerMap>> =
    LazyLock::new(move || Default::default());

static KEYBORG_DATA_LIST: LazyLock<RwLock<KeyborgDataList>> =
    LazyLock::new(move || Default::default());

#[derive(Debug, Default)]
struct EventListenerMap(HashMap<&'static str, Vec<SendWrapper<(EventTarget, EventListener)>>>);

impl EventListenerMap {
    pub fn insert(
        &mut self,
        target: EventTarget,
        event_name: &'static str,
        listener: EventListener,
    ) {
        let event = SendWrapper::new((target, listener));

        if let Some(list) = self.0.get_mut(event_name) {
            list.push(event);
        } else {
            self.0.insert(event_name, vec![event]);
        }
    }

    pub fn remove(&mut self, target: &EventTarget, event_name: &'static str) {
        if let Some(list) = self.0.get_mut(event_name) {
            if let Some(index) = list.iter().position(|v| &v.0 == target) {
                list.remove(index);
            }
        }
    }
}

#[derive(Default)]
struct KeyborgDataList(Vec<(SendWrapper<Window>, KeyborgData)>);

impl KeyborgDataList {
    pub fn push(&mut self, win: Window, data: KeyborgData) {
        self.0.push((SendWrapper::new(win), data));
    }

    pub fn remove(&mut self, win: &Window) {
        if let Some(index) = self.0.iter().position(|v| *v.0 == *win) {
            self.0.remove(index);
        }
    }

    pub fn find(&self, win: &Window) -> Option<&KeyborgData> {
        if let Some((_, data)) = self.0.iter().find(|v| *v.0 == *win) {
            Some(data)
        } else {
            None
        }
    }
}

struct KeyborgData {
    focus_in_handler: Arc<dyn Fn(&Event) + Send + Sync + 'static>,
    focus_out_handler: Arc<dyn Fn(&Event) + Send + Sync + 'static>,
}

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
        let orig_focus = orig_focus.clone();
        move |this: HtmlElement| {
            let keyborg_native_focus_event = kwin.get("__keyborgData");

            if let Some(keyborg_native_focus_event) = keyborg_native_focus_event {
                let _ = Reflect::set(
                    &keyborg_native_focus_event,
                    &JsValue::from_str("lastFocusedProgrammatically"),
                    &WeakRef::new(this.clone().into()),
                );
            }

            let orig_focus = orig_focus.dyn_ref::<js_sys::Function>().unwrap_throw();
            js_sys::Function::apply(orig_focus, &this, &js_sys::Array::new()).unwrap_throw();
        }
    };

    let _ = Reflect::set(
        &kwin,
        &JsValue::from_str("__keyborgNativeFocus"),
        &orig_focus,
    );

    let closure = Closure::wrap(Box::new(focus) as Box<dyn Fn(HtmlElement)>);
    let _ = Reflect::set(
        &kwin,
        &JsValue::from_str("__keyborgHTMLElementFocus"),
        closure.as_ref().unchecked_ref::<js_sys::Function>(),
    );
    let _ = js_sys::eval(
        "HTMLElement.prototype.focus = function focus() { __keyborgHTMLElementFocus(this); }",
    );

    // Set<WeakRefInstance<ShadowRoot>>
    let shadow_targets = js_sys::Set::default();

    let focus_out_handler = |event: &Event| {
        let e = event.dyn_ref::<FocusEvent>().unwrap_throw();

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

    let on_focus_in = {
        let shadow_targets = SendWrapper::new(shadow_targets.clone());
        let kwin = SendWrapper::new(kwin.clone());
        move |target: &Element,
              related_target: Option<HtmlElement>,
              original_event: Option<FocusEvent>| {
            let shadow_root = target.shadow_root();
            if let Some(shadow_root) = shadow_root {
                // https://bugs.chromium.org/p/chromium/issues/detail?id=1512028
                // focusin events don't bubble up through an open shadow root once focus is inside
                // once focus moves into a shadow root - we drop the same focusin handler there
                // keyborg's custom event will still bubble up since it is composed
                // event handlers should be cleaned up once focus leaves the shadow root.
                //
                // When a focusin event is dispatched from a shadow root, its target is the shadow root parent.
                // Each shadow root encounter requires a new capture listener.
                // Why capture? - we want to follow the focus event in order or descending nested shadow roots
                // When there are no more shadow root targets - dispatch the keyborg:focusin event
                //
                // 1. no focus event
                // > document - capture listener ✅
                //   > shadow root 1
                //     > shadow root 2
                //       > shadow root 3
                //         > focused element
                //
                // 2. focus event received by document listener
                // > document - capture listener ✅ (focus event here)
                //   > shadow root 1 - capture listener ✅
                //     > shadow root 2
                //       > shadow root 3
                //         > focused element

                // 3. focus event received by root l1 listener
                // > document - capture listener ✅
                //   > shadow root 1 - capture listener ✅ (focus event here)
                //     > shadow root 2 - capture listener ✅
                //       > shadow root 3
                //         > focused element
                //
                // 4. focus event received by root l2 listener
                // > document - capture listener ✅
                //   > shadow root 1 - capture listener ✅
                //     > shadow root 2 - capture listener ✅ (focus event here)
                //       > shadow root 3 - capture listener ✅
                //         > focused element
                //
                // 5. focus event received by root l3 listener, no more shadow root targets
                // > document - capture listener ✅
                //   > shadow root 1 - capture listener ✅
                //     > shadow root 2 - capture listener ✅
                //       > shadow root 3 - capture listener ✅ (focus event here)
                //         > focused element ✅ (no shadow root - dispatch keyborg event)

                for shadow_root_weak_ref in shadow_targets.values() {
                    let shadow_root_weak_ref = shadow_root_weak_ref
                        .unwrap_throw()
                        .dyn_into::<WeakRef>()
                        .unwrap_throw();
                    if shadow_root_weak_ref.deref() == Some(shadow_root.clone().into()) {
                        return;
                    }
                }

                let mut event_listener_map = EVENT_LISTENER_MAP.write().unwrap_throw();
                let keyborg_data_list = KEYBORG_DATA_LIST.read().unwrap_throw();
                let keyborg_data = keyborg_data_list.find(&kwin).unwrap_throw();

                let options = EventListenerOptions::run_in_capture_phase();
                let focus_in_handler = keyborg_data.focus_in_handler.clone();
                let listener =
                    EventListener::new_with_options(&shadow_root, "focusin", options, move |e| {
                        focus_in_handler(e)
                    });
                event_listener_map.insert(shadow_root.clone().into(), "focusin", listener);
                let options = EventListenerOptions::run_in_capture_phase();
                let focus_out_handler = keyborg_data.focus_out_handler.clone();
                let listener =
                    EventListener::new_with_options(&shadow_root, "focusout", options, move |e| {
                        focus_out_handler(e)
                    });
                event_listener_map.insert(shadow_root.clone().into(), "focusout", listener);

                shadow_targets.add(&WeakRef::new(shadow_root.into()));
                return;
            }

            let init = CustomEventInit::new();
            init.set_cancelable(true);
            init.set_bubbles(true);
            // Allows the event to bubble past an open shadow root
            init.set_composed(true);
            // Tabster (and other users) can still use the legacy details field - keeping for backwards compat
            let details = js_sys::Object::new();
            if let Some(related_target) = related_target {
                Reflect::set(
                    &details,
                    &JsValue::from_str("relatedTarget"),
                    &related_target,
                )
                .unwrap_throw();
            }
            if let Some(original_event) = original_event {
                Reflect::set(
                    &details,
                    &JsValue::from_str("originalEvent"),
                    &original_event,
                )
                .unwrap_throw();
            }
            init.set_detail(&details);
            let event =
                CustomEvent::new_with_event_init_dict(KEYBORG_FOCUSIN, &init).unwrap_throw();

            let data = kwin.get("__keyborgData").unwrap_throw();

            let last_focused_programmatically =
                Reflect::get(&data, &JsValue::from_str("lastFocusedProgrammatically"))
                    .unwrap_throw();
            let last_focused_programmatically = last_focused_programmatically.dyn_into::<WeakRef>();

            if *CAN_OVERRIDE_NATIVE_FOCUS.read().unwrap_throw()
                || last_focused_programmatically.is_ok()
            {
                let is_focused_programmatically =
                    if let Ok(last_focused_programmatically) = last_focused_programmatically {
                        Some(target.clone().into()) == last_focused_programmatically.deref()
                    } else {
                        false
                    };
                Reflect::set(
                    &details,
                    &JsValue::from_str("isFocusedProgrammatically"),
                    &JsValue::from_bool(is_focused_programmatically),
                )
                .unwrap_throw();
                Reflect::set(
                    &data,
                    &JsValue::from_str("lastFocusedProgrammatically"),
                    &JsValue::undefined(),
                )
                .unwrap_throw();
            }

            let _ = target.dispatch_event(&event);
        }
    };

    let focus_in_handler = {
        let on_focus_in = on_focus_in.clone();
        let shadow_targets = SendWrapper::new(shadow_targets.clone());
        move |event: &Event| {
            let e = event.dyn_ref::<FocusEvent>().unwrap_throw();
            let Some(target) = e.target() else {
                return;
            };
            let target = target.dyn_into::<HtmlElement>().unwrap_throw();

            let node = e.composed_path().at(0);
            let mut node = if node.is_null() || node.is_undefined() {
                None
            } else {
                Some(node.dyn_into::<Node>().unwrap_throw())
            };
            // Set<ShadowRoot>
            let current_shadows = Set::default();
            while let Some(node_ref) = node {
                if node_ref.node_type() == Node::DOCUMENT_FRAGMENT_NODE {
                    let node_ref = node_ref.dyn_into::<ShadowRoot>().unwrap_throw();
                    current_shadows.add(&node_ref);
                    node = Some(node_ref.host().into());
                } else {
                    node = node_ref.parent_node();
                }
            }

            for shadow_root_weak_ref in shadow_targets.values() {
                let shadow_root_weak_ref = shadow_root_weak_ref
                    .unwrap_throw()
                    .dyn_into::<WeakRef>()
                    .unwrap_throw();
                let shadow_root = shadow_root_weak_ref.deref();

                if shadow_root.is_none()
                    || !current_shadows.has(&shadow_root.clone().unwrap_throw())
                {
                    shadow_targets.delete(&shadow_root_weak_ref);
                    if let Some(shadow_root) = shadow_root {
                        let shadow_root = shadow_root.dyn_into::<ShadowRoot>().unwrap_throw();
                        let mut event_listener_map = EVENT_LISTENER_MAP.write().unwrap_throw();

                        event_listener_map.remove(&shadow_root, "focusin");
                        event_listener_map.remove(&shadow_root, "focusout");
                    }
                }
            }

            on_focus_in(
                &target,
                e.related_target()
                    .map(|t| t.dyn_into::<HtmlElement>().unwrap_throw()),
                None,
            );
        }
    };

    let keyborg_data = KeyborgData {
        focus_in_handler: Arc::new(focus_in_handler.clone()),
        focus_out_handler: Arc::new(focus_out_handler.clone()),
    };

    KEYBORG_DATA_LIST
        .write()
        .unwrap_throw()
        .push(kwin.clone(), keyborg_data);
    let obj = js_sys::Object::new();
    Reflect::set(&obj, &JsValue::from_str("shadowTargets"), &shadow_targets).unwrap_throw();
    Reflect::set(&kwin, &JsValue::from_str("__keyborgData"), &obj).unwrap_throw();

    let doc = kwin.document().unwrap_throw();
    let mut event_listener_map = EVENT_LISTENER_MAP.write().unwrap_throw();

    let options = EventListenerOptions::run_in_capture_phase();
    let listener = EventListener::new_with_options(&doc, "focusin", options, focus_in_handler);
    event_listener_map.insert(doc.clone().into(), "focusin", listener);

    let options = EventListenerOptions::run_in_capture_phase();
    let listener = EventListener::new_with_options(&doc, "focusout", options, focus_out_handler);
    event_listener_map.insert(doc.clone().into(), "focusout", listener);

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

        on_focus_in(el, None, None);
        active_element = shadow_root.active_element();
    }
}

/// Removes keyborg event listeners and custom focus override
/// @param win The window that stores keyborg focus events
pub fn dispose_focus_event(win: Window) {
    let kwin = win;
    let html_element = kwin.get("HTMLElemnt").unwrap_throw();
    let proto = Reflect::get(&html_element, &JsValue::from_str("prototype")).unwrap_throw();
    let orig_focus = kwin.get("__keyborgNativeFocus");
    let keyborg_data = kwin.get("__keyborgData");

    if let Some(keyborg_data) = keyborg_data {
        let doc = kwin.document().unwrap_throw();
        let mut event_listener_map = EVENT_LISTENER_MAP.write().unwrap_throw();

        event_listener_map.remove(&doc, "focusin");
        event_listener_map.remove(&doc, "focusout");

        let shadow_targets =
            Reflect::get(&keyborg_data, &JsValue::from_str("shadowTargets")).unwrap_throw();
        let shadow_targets = shadow_targets.dyn_into::<js_sys::Set>().unwrap_throw();
        for shadow_root_weak_ref in shadow_targets.values() {
            let shadow_root_weak_ref = shadow_root_weak_ref
                .unwrap_throw()
                .dyn_into::<WeakRef>()
                .unwrap_throw();
            let shadow_root = shadow_root_weak_ref.deref();

            if let Some(shadow_root) = shadow_root {
                let shadow_root = shadow_root.dyn_into::<ShadowRoot>().unwrap_throw();
                event_listener_map.remove(&shadow_root, "focusin");
                event_listener_map.remove(&shadow_root, "focusout");
            }
        }

        shadow_targets.clear();

        KEYBORG_DATA_LIST.write().unwrap_throw().remove(&kwin);
        Reflect::set(
            &kwin,
            &JsValue::from_str("__keyborgData"),
            &JsValue::undefined(),
        )
        .unwrap_throw();
    }

    if let Some(orig_focus) = orig_focus {
        Reflect::set(&proto, &JsValue::from_str("focus"), &orig_focus).unwrap_throw();
    }
}
