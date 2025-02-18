use gloo_events::{EventListener, EventListenerOptions};
use std::sync::{OnceLock, RwLock};
use web_sys::{wasm_bindgen::UnwrapThrowExt, Window};

use crate::focus_event::KEYBORG_FOCUSIN;

static LAST_ID: OnceLock<RwLock<usize>> = OnceLock::new();

struct KeyborgCore {
    id: String,

    keyborg_focusin_listener: EventListener,
}

impl KeyborgCore {
    pub fn new(win: Window) -> Self {
        let last_id = LAST_ID.get_or_init(Default::default);
        let id = *last_id.read().unwrap_throw() + 1;
        *last_id.write().unwrap_throw() = id;

        let doc = win.document().unwrap_throw();

        let options = EventListenerOptions::run_in_capture_phase();
        let keyborg_focusin_listener =
            EventListener::new_with_options(&doc, KEYBORG_FOCUSIN, options, move |_| {});

        Self {
            id: format!("c{id}"),
            keyborg_focusin_listener,
        }
    }
}

pub struct Keyborg {
    id: String,
}

impl Keyborg {
    pub fn new() -> Self {
        let last_id = LAST_ID.get_or_init(Default::default);
        let id = *last_id.read().unwrap_throw() + 1;
        *last_id.write().unwrap_throw() = id;

        Self {
            id: format!("k{id}"),
        }
    }
}
