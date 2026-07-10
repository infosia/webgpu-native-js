/// Converts a JavaScript `GPUSamplerDescriptor` into `WGPUSamplerDescriptor`.
pub(super) fn convert_sampler_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUSamplerDescriptor, E::Error> {
    let max_anisotropy_value = dictionary_member::<E>(cx, value, "maxAnisotropy")?;
    Ok(WGPUSamplerDescriptor {
        nextInChain: ptr::null_mut(),
        // WebIDL `[Clamp]`: NaN becomes +0, the value is clamped to the
        // unsigned-short range, then rounded to the nearest integer (ties to even).
        maxAnisotropy: if E::is_undefined(cx, max_anisotropy_value) {
            1
        } else {
            clamp_u16::<E>(cx, max_anisotropy_value)?
        },
    })
}
