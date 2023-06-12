use js_sys::Reflect;
use wasm_bindgen::JsValue;

pub fn get_attribute<T>(
    object: &JsValue,
    field_name: &str,
    mapper: impl Fn(&JsValue) -> Option<T>,
) -> Result<Option<T>, JsValue> {
    Reflect::get(object, &JsValue::from_str(field_name)).map(|x| mapper(&x))
}
