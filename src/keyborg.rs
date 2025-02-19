use gloo_events::{EventListener, EventListenerOptions};
use js_sys::Reflect;
use send_wrapper::SendWrapper;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};
use wasm_bindgen::{prelude::Closure, JsValue};
use web_sys::{
    wasm_bindgen::{JsCast, UnwrapThrowExt},
    Event, Window,
};

use crate::focus_event::KEYBORG_FOCUSIN;

static LAST_ID: OnceLock<RwLock<usize>> = OnceLock::new();
static KEYBORG_MAP: OnceLock<RwLock<Option<KeyborgMap>>> = OnceLock::new();

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

struct KeyborgCore {
    id: String,
    win: SendWrapper<Window>,

    is_mouse_or_touch_used_timer: Arc<RwLock<Option<i32>>>,
    is_navigating_with_keyboard: Arc<RwLock<IsNavigatingWithKeyboard>>,

    listener_list: Vec<SendWrapper<EventListener>>,
}

impl KeyborgCore {
    pub fn new(win: Window) -> Self {
        let last_id = LAST_ID.get_or_init(Default::default);
        let id = *last_id.read().unwrap_throw() + 1;
        *last_id.write().unwrap_throw() = id;

        let is_mouse_or_touch_used_timer = Arc::new(RwLock::new(None::<i32>));
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

        let on_key_down = {
            let is_navigating_with_keyboard = is_navigating_with_keyboard.clone();

            move |event: &Event| {
                if is_navigating_with_keyboard.read().unwrap_throw().get() {
                    //   if (this._shouldDismissKeyboardNavigation(e)) {
                    //     this._scheduleDismiss();
                    //   }
                } else {
                    //   if (this._shouldTriggerKeyboardNavigation(e)) {
                    //     this.isNavigatingWithKeyboard = true;
                    //   }
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
            is_navigating_with_keyboard,
            listener_list,
        }
    }
}

impl Drop for KeyborgCore {
    fn drop(&mut self) {
        let Self {
            win,
            is_mouse_or_touch_used_timer,
            ..
        } = self;

        if let Some(timer) = is_mouse_or_touch_used_timer.read().unwrap_throw().clone() {
            win.clear_timeout_with_handle(timer);
        }

        //   if (this._dismissTimer) {
        //     win.clearTimeout(this._dismissTimer);
        //     this._dismissTimer = undefined;
        //   }

        //   disposeFocusEvent(win);
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

    pub fn create(win: Window) -> Arc<RwLock<Self>> {
        let keyborg = Arc::new(RwLock::new(Self::new()));
        let id = { keyborg.read().unwrap_throw().id.clone() };

        let init = {
            let keyborg = keyborg.clone();
            let id = id.clone();
            move || {
                let core = Arc::new(KeyborgCore::new(win));
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

    pub fn dispose(self) {
        let Self { id, core, .. } = self;
        let current = KEYBORG_MAP.get().unwrap_throw();

        let (is_remove, is_empty) = if let Some(current) = current.write().unwrap_throw().as_mut() {
            current.refs.remove(&id);

            (current.refs.remove(&id).is_some(), current.refs.is_empty())
        } else {
            Default::default()
        };

        if is_remove && is_empty {
            drop(core);
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
        // return !!this._core?.isNavigatingWithKeyboard;
        todo!()
    }

    /// callback - Called when the keyboard navigation state changes
    pub fn subscribe(&mut self, callback: KeyborgCallback) {
        self.cb.push(callback);
    }
}
