use js_sys::Object;
use wasm_bindgen::prelude::wasm_bindgen;

// WeakSet
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Object, typescript_type = "WeakRef<object>")]
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub type WeakRef;

    /// The `WeakRef()`` constructor creates WeakRef objects.
    ///
    /// [MDN documentation](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/WeakRef)
    #[wasm_bindgen(constructor)]
    pub fn new(target: Object) -> WeakRef;

    /// The `deref()` method of WeakRef instances returns this `WeakRef`'s target value,
    ///  or `undefined` if the target value has been garbage-collected.
    ///
    /// [MDN documentation](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/WeakRef/deref)
    #[wasm_bindgen(method)]
    pub fn deref(this: &WeakRef) -> Option<Object>;

}
