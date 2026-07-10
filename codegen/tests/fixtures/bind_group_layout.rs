/// Converts a JavaScript `GPUBufferBindingLayout` into `WGPUBufferBindingLayout`.
pub(super) fn convert_buffer_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBufferBindingLayout, E::Error> {
    let type_value = dictionary_member::<E>(cx, value, "type")?;
    let has_dynamic_offset_value = dictionary_member::<E>(cx, value, "hasDynamicOffset")?;
    let min_binding_size_value = dictionary_member::<E>(cx, value, "minBindingSize")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let type_ = if E::is_undefined(cx, type_value) {
        WGPUBufferBindingType_WGPUBufferBindingType_Uniform
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, type_value, &enum_arena)? {
            "uniform" => WGPUBufferBindingType_WGPUBufferBindingType_Uniform,
            "storage" => WGPUBufferBindingType_WGPUBufferBindingType_Storage,
            "read-only-storage" => WGPUBufferBindingType_WGPUBufferBindingType_ReadOnlyStorage,
            _ => return Err(E::type_error(cx, "GPUBufferBindingType")),
        }
    };
    Ok(WGPUBufferBindingLayout {
        nextInChain: ptr::null_mut(),
        type_,
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        hasDynamicOffset: if E::is_undefined(cx, has_dynamic_offset_value) {
            0
        } else {
            u32::from(E::to_bool(cx, has_dynamic_offset_value))
        },
        // R8: `[EnforceRange]` GPUSize64 is checked at the 64-bit boundary.
        minBindingSize: if E::is_undefined(cx, min_binding_size_value) {
            0
        } else {
            enforce_u64::<E>(cx, min_binding_size_value, "minBindingSize")?
        },
    })
}

/// Converts a JavaScript `GPUSamplerBindingLayout` into `WGPUSamplerBindingLayout`.
pub(super) fn convert_sampler_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUSamplerBindingLayout, E::Error> {
    let type_value = dictionary_member::<E>(cx, value, "type")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let type_ = if E::is_undefined(cx, type_value) {
        WGPUSamplerBindingType_WGPUSamplerBindingType_Filtering
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, type_value, &enum_arena)? {
            "filtering" => WGPUSamplerBindingType_WGPUSamplerBindingType_Filtering,
            "non-filtering" => WGPUSamplerBindingType_WGPUSamplerBindingType_NonFiltering,
            "comparison" => WGPUSamplerBindingType_WGPUSamplerBindingType_Comparison,
            _ => return Err(E::type_error(cx, "GPUSamplerBindingType")),
        }
    };
    Ok(WGPUSamplerBindingLayout {
        nextInChain: ptr::null_mut(),
        type_,
    })
}

/// Converts a JavaScript `GPUTextureBindingLayout` into `WGPUTextureBindingLayout`.
pub(super) fn convert_texture_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTextureBindingLayout, E::Error> {
    let sample_type_value = dictionary_member::<E>(cx, value, "sampleType")?;
    let view_dimension_value = dictionary_member::<E>(cx, value, "viewDimension")?;
    let multisampled_value = dictionary_member::<E>(cx, value, "multisampled")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let sample_type = if E::is_undefined(cx, sample_type_value) {
        WGPUTextureSampleType_WGPUTextureSampleType_Float
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, sample_type_value, &enum_arena)? {
            "float" => WGPUTextureSampleType_WGPUTextureSampleType_Float,
            "unfilterable-float" => WGPUTextureSampleType_WGPUTextureSampleType_UnfilterableFloat,
            "depth" => WGPUTextureSampleType_WGPUTextureSampleType_Depth,
            "sint" => WGPUTextureSampleType_WGPUTextureSampleType_Sint,
            "uint" => WGPUTextureSampleType_WGPUTextureSampleType_Uint,
            _ => return Err(E::type_error(cx, "GPUTextureSampleType")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let view_dimension = if E::is_undefined(cx, view_dimension_value) {
        WGPUTextureViewDimension_WGPUTextureViewDimension_2d
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, view_dimension_value, &enum_arena)? {
            "1d" => WGPUTextureViewDimension_WGPUTextureViewDimension_1d,
            "2d" => WGPUTextureViewDimension_WGPUTextureViewDimension_2d,
            "2d-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_2dArray,
            "cube" => WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
            "cube-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
            "3d" => WGPUTextureViewDimension_WGPUTextureViewDimension_3d,
            _ => return Err(E::type_error(cx, "GPUTextureViewDimension")),
        }
    };
    Ok(WGPUTextureBindingLayout {
        nextInChain: ptr::null_mut(),
        sampleType: sample_type,
        viewDimension: view_dimension,
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        multisampled: if E::is_undefined(cx, multisampled_value) {
            0
        } else {
            u32::from(E::to_bool(cx, multisampled_value))
        },
    })
}

/// Converts a JavaScript `GPUStorageTextureBindingLayout` into `WGPUStorageTextureBindingLayout`.
pub(super) fn convert_storage_texture_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUStorageTextureBindingLayout, E::Error> {
    let access_value = dictionary_member::<E>(cx, value, "access")?;
    // DR-M3: required dictionary members reject undefined.
    let format_value = required_member::<E>(cx, value, "format")?;
    let view_dimension_value = dictionary_member::<E>(cx, value, "viewDimension")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let access = if E::is_undefined(cx, access_value) {
        WGPUStorageTextureAccess_WGPUStorageTextureAccess_WriteOnly
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, access_value, &enum_arena)? {
            "write-only" => WGPUStorageTextureAccess_WGPUStorageTextureAccess_WriteOnly,
            "read-only" => WGPUStorageTextureAccess_WGPUStorageTextureAccess_ReadOnly,
            "read-write" => WGPUStorageTextureAccess_WGPUStorageTextureAccess_ReadWrite,
            _ => return Err(E::type_error(cx, "GPUStorageTextureAccess")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_Rgba8Unorm,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let view_dimension = if E::is_undefined(cx, view_dimension_value) {
        WGPUTextureViewDimension_WGPUTextureViewDimension_2d
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, view_dimension_value, &enum_arena)? {
            "1d" => WGPUTextureViewDimension_WGPUTextureViewDimension_1d,
            "2d" => WGPUTextureViewDimension_WGPUTextureViewDimension_2d,
            "2d-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_2dArray,
            "cube" => WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
            "cube-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
            "3d" => WGPUTextureViewDimension_WGPUTextureViewDimension_3d,
            _ => return Err(E::type_error(cx, "GPUTextureViewDimension")),
        }
    };
    Ok(WGPUStorageTextureBindingLayout {
        nextInChain: ptr::null_mut(),
        access,
        format,
        viewDimension: view_dimension,
    })
}

/// Converts a JavaScript `GPUBindGroupLayoutEntry` into `WGPUBindGroupLayoutEntry`.
pub(super) fn convert_bind_group_layout_entry<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupLayoutEntry, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let binding_value = required_member::<E>(cx, value, "binding")?;
    let visibility_value = required_member::<E>(cx, value, "visibility")?;
    let buffer_value = dictionary_member::<E>(cx, value, "buffer")?;
    let sampler_value = dictionary_member::<E>(cx, value, "sampler")?;
    let texture_value = dictionary_member::<E>(cx, value, "texture")?;
    let storage_texture_value = dictionary_member::<E>(cx, value, "storageTexture")?;
    let external_texture_value = dictionary_member::<E>(cx, value, "externalTexture")?;
    // G7 carve-out: fail early instead of silently emitting a wrong layout.
    if !E::is_undefined(cx, external_texture_value) {
        return Err(E::type_error(cx, "externalTexture bindings are not supported yet"));
    }
    // G11: an absent nested dictionary preserves the C zero/default sentinel.
    let buffer = if E::is_undefined(cx, buffer_value) {
        // SAFETY: the joined C-ABI member declares `default: zero`.
        unsafe { std::mem::zeroed() }
    } else {
        convert_buffer_binding_layout::<E>(cx, buffer_value)?
    };
    // G11: an absent nested dictionary preserves the C zero/default sentinel.
    let sampler = if E::is_undefined(cx, sampler_value) {
        // SAFETY: the joined C-ABI member declares `default: zero`.
        unsafe { std::mem::zeroed() }
    } else {
        convert_sampler_binding_layout::<E>(cx, sampler_value)?
    };
    // G11: an absent nested dictionary preserves the C zero/default sentinel.
    let texture = if E::is_undefined(cx, texture_value) {
        // SAFETY: the joined C-ABI member declares `default: zero`.
        unsafe { std::mem::zeroed() }
    } else {
        convert_texture_binding_layout::<E>(cx, texture_value)?
    };
    // G11: an absent nested dictionary preserves the C zero/default sentinel.
    let storage_texture = if E::is_undefined(cx, storage_texture_value) {
        // SAFETY: the joined C-ABI member declares `default: zero`.
        unsafe { std::mem::zeroed() }
    } else {
        convert_storage_texture_binding_layout::<E>(cx, storage_texture_value)?
    };
    Ok(WGPUBindGroupLayoutEntry {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUIndex32 is checked at the 32-bit boundary.
        binding: enforce_u32::<E>(cx, binding_value, "binding")?,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        visibility: u64::from(enforce_u32::<E>(cx, visibility_value, "visibility")?),
        buffer,
        sampler,
        texture,
        storageTexture: storage_texture,
        bindingArraySize: 0,
    })
}

/// Converts a JavaScript `GPUBindGroupLayoutDescriptor` into `WGPUBindGroupLayoutDescriptor`.
pub(super) fn convert_bind_group_layout_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUBindGroupLayoutDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let entries_value = required_member::<E>(cx, value, "entries")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let entries = {
        let converted = convert_sequence::<E, _>(cx, entries_value, "entries", |item| {
            convert_bind_group_layout_entry::<E>(cx, item)
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUBindGroupLayoutDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        entryCount: entries.len(),
        entries: if entries.is_empty() {
            ptr::null()
        } else {
            entries.as_ptr()
        },
    })
}
