use js_sys::Reflect;
use wasm_bindgen::JsValue;

pub fn get_attribute<T>(
    object: &JsValue,
    field_name: &str,
    mapper: impl Fn(&JsValue) -> Option<T>,
) -> Result<Option<T>, JsValue> {
    Reflect::get(object, &JsValue::from_str(field_name)).map(|x| mapper(&x))
}

/// Performs x * p in a convoluted way that reduces errors.
pub fn rational_mult(x: usize, p: f64) -> usize {
    if x == 0 {
        return 0;
    }

    // find the optimal power of 2 that will make x * p no greater than
    // usize::MAX so it does not overflow. it's basically just:
    // log(MAX) - log(x) - max(0, log(p))
    let power_2 = (usize::MAX.ilog2() - x.ilog2() - (p.log2().max(0.) as u32)) as i32;

    // x * p = x * (p * q) / q
    // calculate q, which is the power of 2
    let q = 2f64.powi(power_2);

    // calculate (p * q)
    let p_uint = (p * q) as usize;

    x * p_uint / (q as usize)
}
