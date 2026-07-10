/// Converts a JavaScript `GPUExtent3DDict` into `WGPUExtent3D`.
#[allow(dead_code)] // T1 emits union arms even before every typedef has an API consumer.
pub(super) fn convert_extent3d_dict<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUExtent3D, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let width_value = required_member::<E>(cx, value, "width")?;
    let height_value = dictionary_member::<E>(cx, value, "height")?;
    let depth_or_array_layers_value = dictionary_member::<E>(cx, value, "depthOrArrayLayers")?;
    Ok(WGPUExtent3D {
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        width: enforce_u32::<E>(cx, width_value, "width")?,
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        height: if E::is_undefined(cx, height_value) {
            1
        } else {
            enforce_u32::<E>(cx, height_value, "height")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        depthOrArrayLayers: if E::is_undefined(cx, depth_or_array_layers_value) {
            1
        } else {
            enforce_u32::<E>(cx, depth_or_array_layers_value, "depthOrArrayLayers")?
        },
    })
}

/// Converts a JavaScript `GPUOrigin3DDict` into `WGPUOrigin3D`.
#[allow(dead_code)] // T1 emits union arms even before every typedef has an API consumer.
pub(super) fn convert_origin3d_dict<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUOrigin3D, E::Error> {
    let x_value = dictionary_member::<E>(cx, value, "x")?;
    let y_value = dictionary_member::<E>(cx, value, "y")?;
    let z_value = dictionary_member::<E>(cx, value, "z")?;
    Ok(WGPUOrigin3D {
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        x: if E::is_undefined(cx, x_value) {
            0
        } else {
            enforce_u32::<E>(cx, x_value, "x")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        y: if E::is_undefined(cx, y_value) {
            0
        } else {
            enforce_u32::<E>(cx, y_value, "y")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        z: if E::is_undefined(cx, z_value) {
            0
        } else {
            enforce_u32::<E>(cx, z_value, "z")?
        },
    })
}

/// Converts the dictionary-or-sequence `GPUExtent3D` typedef into `WGPUExtent3D`.
#[allow(dead_code)] // T1 policy selects both typedefs; some land before their API consumer.
pub(super) fn convert_gpu_extent3d<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUExtent3D, E::Error> {
    // T1: an iterable selects the sequence arm; otherwise dictionary conversion applies.
    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {
        return convert_extent3d_dict::<E>(cx, value);
    };
    let values = convert_sequence_from_method::<E, _>(cx, value, iterator_method, "GPUExtent3D", |item| {
        enforce_u32::<E>(cx, item, "coordinate")
    })?;
    if values.is_empty() || values.len() > 3 {
        return Err(E::type_error(cx, "GPUExtent3D sequence length must be 1..=3"));
    }
    Ok(WGPUExtent3D {
        width: values[0],
        height: values.get(1).copied().unwrap_or(1),
        depthOrArrayLayers: values.get(2).copied().unwrap_or(1),
    })
}

/// Converts the dictionary-or-sequence `GPUOrigin3D` typedef into `WGPUOrigin3D`.
#[allow(dead_code)] // T1 policy selects both typedefs; some land before their API consumer.
pub(super) fn convert_gpu_origin3d<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUOrigin3D, E::Error> {
    // T1: an iterable selects the sequence arm; otherwise dictionary conversion applies.
    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {
        return convert_origin3d_dict::<E>(cx, value);
    };
    let values = convert_sequence_from_method::<E, _>(cx, value, iterator_method, "GPUOrigin3D", |item| {
        enforce_u32::<E>(cx, item, "coordinate")
    })?;
    if values.is_empty() || values.len() > 3 {
        return Err(E::type_error(cx, "GPUOrigin3D sequence length must be 1..=3"));
    }
    Ok(WGPUOrigin3D {
        x: values[0],
        y: values.get(1).copied().unwrap_or(0),
        z: values.get(2).copied().unwrap_or(0),
    })
}
