use gloo_events::{EventListener, EventListenerOptions};
use js_sys::Reflect;
use send_wrapper::SendWrapper;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, RwLock},
};
use wasm_bindgen::{prelude::Closure, JsValue};
use web_sys::{
    wasm_bindgen::{JsCast, UnwrapThrowExt},
    Event, HtmlElement, KeyboardEvent, Window,
};

use crate::focus_event::{dispose_focus_event, KEYBORG_FOCUSIN};

static LAST_ID: OnceLock<RwLock<usize>> = OnceLock::new();
static KEYBORG_MAP: OnceLock<RwLock<Option<KeyborgMap>>> = OnceLock::new();

// When a key from dismiss_keys is pressed and the focus is not moved
// during DISMISS_TIMEOUT time, dismiss the keyboard navigation mode.
const DISMISS_TIMEOUT: i32 = 500;

struct KeyborgMap {
    core: Arc<KeyborgCore>,
    refs: HashMap<String, Arc<RwLock<Keyborg>>>,
}

impl KeyborgMap {
    pub fn new(core: Arc<KeyborgCore>) -> Self {
        Self {
            core,
            refs: Default::default(),
        }
    }
}

struct IsNavigatingWithKeyboard {
    do_not_use: bool,
}

impl IsNavigatingWithKeyboard {
    pub fn new() -> Self {
        Self { do_not_use: false }
    }

    pub fn get(&self) -> bool {
        self.do_not_use
    }

    pub fn set(&mut self, val: bool) {
        if self.do_not_use != val {
            self.do_not_use = val;
            self.update();
        }
    }

    /// Updates all keyborg instances with the keyboard navigation state
    fn update(&self) {
        if let Some(keyborg_map) = KEYBORG_MAP.get() {
            let keyborg_map = keyborg_map.read().unwrap_throw();
            if let Some(keyborg_map) = keyborg_map.as_ref() {
                for keyborg in keyborg_map.refs.values() {
                    let keyborg = keyborg.read().unwrap_throw();
                    keyborg.update(self.do_not_use)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyborgProps {
    // Keys to be used to trigger keyboard navigation mode. By default, any key will trigger
    // it. Could be limited to, for example, just Tab (or Tab and arrow keys).
    trigger_keys: Option<Vec<u32>>,
    // Keys to be used to dismiss keyboard navigation mode using keyboard (in addition to
    // mouse clicks which dismiss it). For example, Esc could be used to dismiss.
    dismiss_keys: Option<Vec<u32>>,
}

struct KeyborgCore {
    id: String,
    win: SendWrapper<Window>,

    is_mouse_or_touch_used_timer: Arc<RwLock<Option<i32>>>,
    dismiss_timer: Arc<RwLock<Option<i32>>>,
    trigger_keys: Arc<Option<HashSet<u32>>>,
    dismiss_keys: Arc<Option<HashSet<u32>>>,
    is_navigating_with_keyboard: Arc<RwLock<IsNavigatingWithKeyboard>>,

    _listener_list: Vec<SendWrapper<EventListener>>,
}

impl KeyborgCore {
    pub fn new(win: Window, props: Option<KeyborgProps>) -> Self {
        let last_id = LAST_ID.get_or_init(Default::default);
        let id = *last_id.read().unwrap_throw() + 1;
        *last_id.write().unwrap_throw() = id;

        let mut dismiss_keys = None::<HashSet<u32>>;
        let mut trigger_keys = None::<HashSet<u32>>;

        if let Some(props) = props {
            if let Some(keys) = props.trigger_keys {
                if !keys.is_empty() {
                    trigger_keys = Some(HashSet::from_iter(keys));
                }
            }

            if let Some(keys) = props.dismiss_keys {
                if !keys.is_empty() {
                    dismiss_keys = Some(HashSet::from_iter(keys));
                }
            }
        }
        let dismiss_keys = Arc::new(dismiss_keys);
        let trigger_keys = Arc::new(trigger_keys);

        let is_mouse_or_touch_used_timer = Arc::new(RwLock::new(None::<i32>));
        let dismiss_timer = Arc::new(RwLock::new(None::<i32>));
        let is_navigating_with_keyboard = Arc::new(RwLock::new(IsNavigatingWithKeyboard::new()));
        let mut listener_list = vec![];

        let doc = win.document().unwrap_throw();

        let on_focus_in = {
            let is_mouse_or_touch_used_timer = is_mouse_or_touch_used_timer.clone();
            let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();
            move |event: &Event| {
                let e = event.dyn_ref::<web_sys::CustomEvent>().unwrap_throw();

                // When the focus is moved not programmatically and without keydown events,
                // it is likely that the focus is moved by screen reader (as it might swallow
                // the events when the screen reader shortcuts are used). The screen reader
                // usage is keyboard navigation.

                if is_mouse_or_touch_used_timer.read().unwrap_throw().is_some() {
                    // There was a mouse or touch event recently.
                    return;
                }

                if is_navigating_with_keyboard.read().unwrap_throw().get() {
                    return;
                }

                // KeyborgFocusInEventDetails
                let details = e.detail();

                let Some(details) = details.dyn_ref::<js_sys::Object>() else {
                    return;
                };

                if !Reflect::has(&details, &JsValue::from("relatedTarget")).unwrap_throw() {
                    return;
                }

                let is_focused_programmatically =
                    Reflect::get(&details, &JsValue::from("isFocusedProgrammatically"))
                        .unwrap_throw();

                if is_focused_programmatically.as_bool().unwrap_or_default()
                    || is_focused_programmatically.is_undefined()
                {
                    // The element is focused programmatically, or the programmatic focus detection
                    // is not working.
                    return;
                }

                is_navigating_with_keyboard.write().unwrap_throw().set(true);
            }
        };
        let options = EventListenerOptions::run_in_capture_phase();
        let listener = EventListener::new_with_options(&doc, KEYBORG_FOCUSIN, options, on_focus_in);
        listener_list.push(SendWrapper::new(listener));

        let on_mouse_or_touch = {
            let is_mouse_or_touch_used_timer = is_mouse_or_touch_used_timer.clone();
            let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();
            let win = win.clone();
            move |_: &Event| {
                if let Some(timer) = is_mouse_or_touch_used_timer.read().unwrap_throw().clone() {
                    win.clear_timeout_with_handle(timer);
                }

                let closure = Closure::once({
                    let is_mouse_or_touch_used_timer = is_mouse_or_touch_used_timer.clone();
                    move || {
                        is_mouse_or_touch_used_timer.write().unwrap_throw().take();
                    }
                });

                let id = win
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        closure.as_ref().unchecked_ref::<js_sys::Function>(),
                        1000,
                    )
                    .unwrap_throw();
                *is_mouse_or_touch_used_timer.write().unwrap_throw() = Some(id);
                // Keeping the indication of mouse or touch usage for some time.

                is_navigating_with_keyboard
                    .write()
                    .unwrap_throw()
                    .set(false);
            }
        };

        let on_mouse_down = {
            let on_mouse_or_touch = on_mouse_or_touch.clone();
            move |event: &Event| {
                let e = event.dyn_ref::<web_sys::MouseEvent>().unwrap_throw();
                if e.buttons() == 0
                    || (e.client_x() == 0
                        && e.client_y() == 0
                        && e.screen_x() == 0
                        && e.screen_y() == 0)
                {
                    // This is most likely an event triggered by the screen reader to perform
                    // an action on an element, do not dismiss the keyboard navigation mode.
                    return;
                }

                on_mouse_or_touch(event);
            }
        };
        let options = EventListenerOptions::run_in_capture_phase();
        let listener = EventListener::new_with_options(&doc, "mousedown", options, on_mouse_down);
        listener_list.push(SendWrapper::new(listener));

        let should_dismiss_keyboard_navigation = {
            let dismiss_keys = dismiss_keys.clone();
            move |e: &KeyboardEvent| {
                dismiss_keys
                    .as_ref()
                    .clone()
                    .is_some_and(|keys| keys.contains(&e.key_code()))
            }
        };

        let schedule_dismiss = {
            let win = win.clone();
            let dismiss_timer = dismiss_timer.clone();
            let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();
            move || {
                if let Some(timer) = dismiss_timer.read().unwrap_throw().clone() {
                    win.clear_timeout_with_handle(timer);
                }

                let was = win.document().unwrap_throw().active_element();

                let closure = Closure::once({
                    let dismiss_timer = dismiss_timer.clone();
                    let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();
                    let win = win.clone();
                    move || {
                        dismiss_timer.write().unwrap_throw().take();
                        let cur = win.document().unwrap_throw().active_element();

                        let Some(was) = was else {
                            return;
                        };

                        let Some(cur) = cur else {
                            return;
                        };

                        if was == cur {
                            // Esc was pressed, currently focused element hasn't changed.
                            // Just dismiss the keyboard navigation mode.
                            is_navigating_with_keyboard
                                .write()
                                .unwrap_throw()
                                .set(false);
                        }
                    }
                });

                let id = win
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        closure.as_ref().unchecked_ref::<js_sys::Function>(),
                        DISMISS_TIMEOUT,
                    )
                    .unwrap_throw();
                *dismiss_timer.write().unwrap_throw() = Some(id);
            }
        };

        // @returns whether the keyboard event should trigger keyboard navigation mode
        let should_trigger_keyboard_navigation = {
            let win = win.clone();
            let trigger_keys = trigger_keys.clone();
            move |e: &KeyboardEvent| {
                // TODO Some rich text fields can allow Tab key for indentation so it doesn't
                // need to be a navigation key. If there is a bug regarding that we should revisit
                if e.key() == "Tab" {
                    return true;
                }

                let active_element = win.document().unwrap_throw().active_element();

                let is_trigger_key = trigger_keys
                    .as_ref()
                    .clone()
                    .map_or(true, |keys| keys.contains(&e.key_code()));

                let is_editable = if let Some(Ok(active_element)) =
                    active_element.map(|el| el.dyn_into::<HtmlElement>())
                {
                    if ["INPUT".to_string(), "TEXTAREA".to_string()]
                        .contains(&active_element.tag_name())
                    {
                        true
                    } else if active_element.is_content_editable() {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                is_trigger_key && !is_editable
            }
        };

        let on_key_down = {
            let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();

            move |event: &Event| {
                let e = event.dyn_ref::<web_sys::KeyboardEvent>().unwrap_throw();

                if is_navigating_with_keyboard.read().unwrap_throw().get() {
                    if should_dismiss_keyboard_navigation(e) {
                        schedule_dismiss();
                    }
                } else {
                    if should_trigger_keyboard_navigation(e) {
                        is_navigating_with_keyboard.write().unwrap_throw().set(true);
                    }
                }
            }
        };
        let options = EventListenerOptions::run_in_capture_phase();
        let listener = EventListener::new_with_options(&win, "keydown", options, on_key_down);
        listener_list.push(SendWrapper::new(listener));

        let options = EventListenerOptions::run_in_capture_phase();
        let listener =
            EventListener::new_with_options(&win, "touchstart", options, on_mouse_or_touch.clone());
        listener_list.push(SendWrapper::new(listener));

        let options = EventListenerOptions::run_in_capture_phase();
        let listener =
            EventListener::new_with_options(&win, "touchend", options, on_mouse_or_touch.clone());
        listener_list.push(SendWrapper::new(listener));

        let options = EventListenerOptions::run_in_capture_phase();
        let listener =
            EventListener::new_with_options(&win, "touchcancel", options, on_mouse_or_touch);
        listener_list.push(SendWrapper::new(listener));

        Self {
            id: format!("c{id}"),
            win: SendWrapper::new(win),
            is_mouse_or_touch_used_timer,
            dismiss_timer,
            trigger_keys,
            dismiss_keys,
            is_navigating_with_keyboard,
            _listener_list: listener_list,
        }
    }
}

impl Drop for KeyborgCore {
    fn drop(&mut self) {
        let Self {
            win,
            is_mouse_or_touch_used_timer,
            dismiss_timer,
            ..
        } = self;

        if let Some(timer) = is_mouse_or_touch_used_timer.read().unwrap_throw().clone() {
            win.clear_timeout_with_handle(timer);
        }

        if let Some(timer) = dismiss_timer.read().unwrap_throw().clone() {
            win.clear_timeout_with_handle(timer);
        }

        dispose_focus_event(win.clone().take());
    }
}

type KeyborgCallback = Box<dyn Fn(bool) + Send + Sync>;

pub struct Keyborg {
    id: String,
    core: Option<Arc<KeyborgCore>>,
    cb: Vec<KeyborgCallback>,
}

impl Keyborg {
    /// Updates all subscribed callbacks with the keyboard navigation state
    fn update(&self, is_navigating_with_keyboard: bool) {
        self.cb
            .iter()
            .for_each(|callback| callback(is_navigating_with_keyboard));
    }

    pub fn create(win: Window, props: Option<KeyborgProps>) -> Arc<RwLock<Self>> {
        let keyborg = Arc::new(RwLock::new(Self::new()));
        let id = { keyborg.read().unwrap_throw().id.clone() };

        let init = {
            let keyborg = keyborg.clone();
            let id = id.clone();
            move || {
                let core = Arc::new(KeyborgCore::new(win, props));
                {
                    let mut keyborg = keyborg.write().unwrap_throw();
                    keyborg.core = Some(core.clone());
                }

                let mut keyborg_map = KeyborgMap::new(core);
                keyborg_map.refs.insert(id, keyborg.clone());

                RwLock::new(Some(keyborg_map))
            }
        };

        let current = KEYBORG_MAP.get_or_init(init.clone());
        if let Some(current) = current.write().unwrap_throw().as_mut() {
            {
                let mut keyborg = keyborg.write().unwrap_throw();
                keyborg.core = Some(current.core.clone());
            }
            current.refs.insert(id, keyborg.clone());
        } else {
            let _ = KEYBORG_MAP.set(init());
        }

        keyborg
    }

    fn new() -> Self {
        let last_id = LAST_ID.get_or_init(Default::default);
        let id = *last_id.read().unwrap_throw() + 1;
        *last_id.write().unwrap_throw() = id;

        Self {
            id: format!("k{id}"),
            core: None,
            cb: vec![],
        }
    }

    pub fn dispose(&self) {
        let Self { id, .. } = self;
        let current = KEYBORG_MAP.get().unwrap_throw();

        let (is_remove, is_empty) = if let Some(current) = current.write().unwrap_throw().as_mut() {
            current.refs.remove(id);

            (current.refs.remove(id).is_some(), current.refs.is_empty())
        } else {
            Default::default()
        };

        if is_remove && is_empty {
            // drop(core);
            *current.write().unwrap_throw() = None;
        }

        if !is_remove && cfg!(debug_assertions) {
            web_sys::console::error_1(&JsValue::from(&format!(
                "Keyborg instance {id} is being disposed incorrectly."
            )));
        }
    }

    /// @returns Whether the user is navigating with keyboard
    pub fn is_navigating_with_keyboard(&self) -> bool {
        if let Some(core) = &self.core {
            core.is_navigating_with_keyboard.read().unwrap_throw().get()
        } else {
            false
        }
    }

    /// callback - Called when the keyboard navigation state changes
    pub fn subscribe(&mut self, callback: impl Fn(bool) + Send + Sync + 'static) {
        self.cb.push(Box::new(callback));
    }

    /// @param callback - Registered with subscribe
    pub fn unsubscribe(callback: KeyborgCallback) {
        // const index = this._cb.indexOf(callback);

        // if (index >= 0) {
        //     this._cb.splice(index, 1);
        // }
    }
}
