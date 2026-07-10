/// Converts a JavaScript `GPUBufferDescriptor` into `BufferDescriptor`.
pub(super) fn convert_buffer_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<BufferDescriptor, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let size_value = required_member::<E>(cx, value, "size")?;
    let usage_value = required_member::<E>(cx, value, "usage")?;
    let mapped_at_creation_value = E::get_property(cx, value, "mappedAtCreation")?;
    let label_value = E::get_property(cx, value, "label")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    Ok(BufferDescriptor {
        // R8: `[EnforceRange]` GPUSize64 is checked at the 64-bit boundary.
        size: enforce_u64::<E>(cx, size_value, "size")?,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        usage: u64::from(enforce_u32::<E>(cx, usage_value, "usage")?),
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        mapped_at_creation: if E::is_undefined(cx, mapped_at_creation_value) {
            false
        } else {
            E::to_bool(cx, mapped_at_creation_value)
        },
        label: label.to_owned(),
    })
}

/// Converts a JavaScript `GPUSamplerDescriptor` into `WGPUSamplerDescriptor`.
pub(super) fn convert_sampler_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUSamplerDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    let address_mode_u_value = E::get_property(cx, value, "addressModeU")?;
    let address_mode_v_value = E::get_property(cx, value, "addressModeV")?;
    let address_mode_w_value = E::get_property(cx, value, "addressModeW")?;
    let mag_filter_value = E::get_property(cx, value, "magFilter")?;
    let min_filter_value = E::get_property(cx, value, "minFilter")?;
    let mipmap_filter_value = E::get_property(cx, value, "mipmapFilter")?;
    let lod_min_clamp_value = E::get_property(cx, value, "lodMinClamp")?;
    let lod_max_clamp_value = E::get_property(cx, value, "lodMaxClamp")?;
    let compare_value = E::get_property(cx, value, "compare")?;
    let max_anisotropy_value = E::get_property(cx, value, "maxAnisotropy")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let address_mode_u = if E::is_undefined(cx, address_mode_u_value) {
        WGPUAddressMode_WGPUAddressMode_ClampToEdge
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, address_mode_u_value, &enum_arena)? {
            "clamp-to-edge" => WGPUAddressMode_WGPUAddressMode_ClampToEdge,
            "repeat" => WGPUAddressMode_WGPUAddressMode_Repeat,
            "mirror-repeat" => WGPUAddressMode_WGPUAddressMode_MirrorRepeat,
            _ => return Err(E::type_error(cx, "GPUAddressMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let address_mode_v = if E::is_undefined(cx, address_mode_v_value) {
        WGPUAddressMode_WGPUAddressMode_ClampToEdge
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, address_mode_v_value, &enum_arena)? {
            "clamp-to-edge" => WGPUAddressMode_WGPUAddressMode_ClampToEdge,
            "repeat" => WGPUAddressMode_WGPUAddressMode_Repeat,
            "mirror-repeat" => WGPUAddressMode_WGPUAddressMode_MirrorRepeat,
            _ => return Err(E::type_error(cx, "GPUAddressMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let address_mode_w = if E::is_undefined(cx, address_mode_w_value) {
        WGPUAddressMode_WGPUAddressMode_ClampToEdge
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, address_mode_w_value, &enum_arena)? {
            "clamp-to-edge" => WGPUAddressMode_WGPUAddressMode_ClampToEdge,
            "repeat" => WGPUAddressMode_WGPUAddressMode_Repeat,
            "mirror-repeat" => WGPUAddressMode_WGPUAddressMode_MirrorRepeat,
            _ => return Err(E::type_error(cx, "GPUAddressMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let mag_filter = if E::is_undefined(cx, mag_filter_value) {
        WGPUFilterMode_WGPUFilterMode_Nearest
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, mag_filter_value, &enum_arena)? {
            "nearest" => WGPUFilterMode_WGPUFilterMode_Nearest,
            "linear" => WGPUFilterMode_WGPUFilterMode_Linear,
            _ => return Err(E::type_error(cx, "GPUFilterMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let min_filter = if E::is_undefined(cx, min_filter_value) {
        WGPUFilterMode_WGPUFilterMode_Nearest
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, min_filter_value, &enum_arena)? {
            "nearest" => WGPUFilterMode_WGPUFilterMode_Nearest,
            "linear" => WGPUFilterMode_WGPUFilterMode_Linear,
            _ => return Err(E::type_error(cx, "GPUFilterMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let mipmap_filter = if E::is_undefined(cx, mipmap_filter_value) {
        WGPUMipmapFilterMode_WGPUMipmapFilterMode_Nearest
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, mipmap_filter_value, &enum_arena)? {
            "nearest" => WGPUMipmapFilterMode_WGPUMipmapFilterMode_Nearest,
            "linear" => WGPUMipmapFilterMode_WGPUMipmapFilterMode_Linear,
            _ => return Err(E::type_error(cx, "GPUMipmapFilterMode")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let compare = if E::is_undefined(cx, compare_value) {
        WGPUCompareFunction_WGPUCompareFunction_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, compare_value, &enum_arena)? {
            "never" => WGPUCompareFunction_WGPUCompareFunction_Never,
            "less" => WGPUCompareFunction_WGPUCompareFunction_Less,
            "equal" => WGPUCompareFunction_WGPUCompareFunction_Equal,
            "less-equal" => WGPUCompareFunction_WGPUCompareFunction_LessEqual,
            "greater" => WGPUCompareFunction_WGPUCompareFunction_Greater,
            "not-equal" => WGPUCompareFunction_WGPUCompareFunction_NotEqual,
            "greater-equal" => WGPUCompareFunction_WGPUCompareFunction_GreaterEqual,
            "always" => WGPUCompareFunction_WGPUCompareFunction_Always,
            _ => return Err(E::type_error(cx, "GPUCompareFunction")),
        }
    };
    Ok(WGPUSamplerDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        addressModeU: address_mode_u,
        addressModeV: address_mode_v,
        addressModeW: address_mode_w,
        magFilter: mag_filter,
        minFilter: min_filter,
        mipmapFilter: mipmap_filter,
        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.
        lodMinClamp: if E::is_undefined(cx, lod_min_clamp_value) {
            0_f32
        } else {
            restricted_f32::<E>(cx, lod_min_clamp_value, "lodMinClamp")?
        },
        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.
        lodMaxClamp: if E::is_undefined(cx, lod_max_clamp_value) {
            32_f32
        } else {
            restricted_f32::<E>(cx, lod_max_clamp_value, "lodMaxClamp")?
        },
        compare,
        // WebIDL `[Clamp]`: NaN becomes +0, the value is clamped to the
        // unsigned-short range, then rounded to the nearest integer (ties to even).
        maxAnisotropy: if E::is_undefined(cx, max_anisotropy_value) {
            1
        } else {
            clamp_u16::<E>(cx, max_anisotropy_value)?
        },
    })
}

/// Converts a JavaScript `GPUBindGroupEntry` into `WGPUBindGroupEntry`.
pub(super) fn convert_bind_group_entry<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupEntry, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let binding_value = required_member::<E>(cx, value, "binding")?;
    let resource_value = required_member::<E>(cx, value, "resource")?;
    // B8: flattened handle conversion extracts only the native handle.
    let buffer_value = E::get_property(cx, resource_value, "buffer")?;
    let buffer = if E::is_undefined(cx, buffer_value) {
        return Err(E::type_error(cx, "resource must be a GPUBufferBinding"));
    } else {
        buffer_handle::<E>(cx, buffer_value)?
    };
    let offset_value = E::get_property(cx, resource_value, "offset")?;
    // R8: flattened `[EnforceRange]` members keep their WebIDL width.
    let offset = if E::is_undefined(cx, offset_value) {
        0
    } else {
        enforce_u64::<E>(cx, offset_value, "offset")?
    };
    let size_value = E::get_property(cx, resource_value, "size")?;
    // R8: flattened `[EnforceRange]` members keep their WebIDL width.
    let size = if E::is_undefined(cx, size_value) {
        WGPU_WHOLE_SIZE as u64
    } else {
        enforce_u64::<E>(cx, size_value, "size")?
    };
    Ok(WGPUBindGroupEntry {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUIndex32 is checked at the 32-bit boundary.
        binding: enforce_u32::<E>(cx, binding_value, "binding")?,
        buffer,
        offset,
        size,
        sampler: ptr::null_mut(),
        textureView: ptr::null_mut(),
    })
}

/// Converts a JavaScript `GPUBindGroupDescriptor` into `ConvertedBindGroupDescriptor`.
pub(super) fn convert_bind_group_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedBindGroupDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let layout_value = required_member::<E>(cx, value, "layout")?;
    let entries_value = required_member::<E>(cx, value, "entries")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let layout = bind_group_layout_handle::<E>(cx, layout_value)?;
    let entries = {
        let converted = convert_sequence::<E, _>(cx, entries_value, "entries", |item| {
            convert_bind_group_entry::<E>(cx, item)
        })?;
        arena.alloc_slice(converted)
    };
    let buffers = entries
        .iter()
        .filter_map(|item| (!item.buffer.is_null()).then_some(item.buffer))
        .collect();
    let native = WGPUBindGroupDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        layout,
        entryCount: entries.len(),
        entries: if entries.is_empty() {
            ptr::null()
        } else {
            entries.as_ptr()
        },
    };
    Ok(ConvertedBindGroupDescriptor {
        native,
        layout,
        buffers,
    })
}

/// Converts a JavaScript `GPUPipelineLayoutDescriptor` into `WGPUPipelineLayoutDescriptor`.
pub(super) fn convert_pipeline_layout_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUPipelineLayoutDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let bind_group_layouts_value = required_member::<E>(cx, value, "bindGroupLayouts")?;
    let immediate_size_value = E::get_property(cx, value, "immediateSize")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let bind_group_layouts = {
        // B8: conversion extracts handles only; create paths own retention.
        let converted = convert_sequence::<E, _>(cx, bind_group_layouts_value, "bindGroupLayouts", |item| {
            bind_group_layout_handle::<E>(cx, item)
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUPipelineLayoutDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        bindGroupLayoutCount: bind_group_layouts.len(),
        bindGroupLayouts: if bind_group_layouts.is_empty() {
            ptr::null()
        } else {
            bind_group_layouts.as_ptr()
        },
        // R8: `[EnforceRange]` GPUSize32 is checked at the 32-bit boundary.
        immediateSize: if E::is_undefined(cx, immediate_size_value) {
            0
        } else {
            enforce_u32::<E>(cx, immediate_size_value, "immediateSize")?
        },
    })
}

/// Converts a JavaScript `GPUShaderModuleDescriptor` into `WGPUShaderModuleDescriptor`.
pub(super) fn convert_shader_module_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUShaderModuleDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let code_value = required_member::<E>(cx, value, "code")?;
    let compilation_hints_value = E::get_property(cx, value, "compilationHints")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, compilation_hints_value) {
        return Err(E::type_error(cx, "compilationHints are not supported yet"));
    }
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let code = E::to_str(cx, code_value, arena)?;
    // B3: WGSL is represented by an arena-owned chained struct with sType set.
    let code_source = arena.alloc_slice(vec![WGPUShaderSourceWGSL {
        chain: WGPUChainedStruct {
            next: ptr::null_mut(),
            sType: WGPUSType_WGPUSType_ShaderSourceWGSL,
        },
        code: WGPUStringView::from_bytes(code.as_bytes()),
    }]).as_ptr();
    // SAFETY: the arena allocation contains one initialized chained source.
    let code_chain = unsafe { ptr::addr_of!((*code_source).chain) }.cast_mut();
    Ok(WGPUShaderModuleDescriptor {
        nextInChain: code_chain,
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

/// Converts a JavaScript `GPUProgrammableStage` into `WGPUComputeState`.
pub(super) fn convert_programmable_stage<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUComputeState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let module_value = required_member::<E>(cx, value, "module")?;
    let entry_point_value = E::get_property(cx, value, "entryPoint")?;
    let constants_value = E::get_property(cx, value, "constants")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, constants_value) {
        return Err(E::type_error(cx, "constants are not supported yet"));
    }
    let module = shader_module_handle::<E>(cx, module_value)?;
    // B4: optional non-nullable strings preserve absence; present null is stringified.
    let entry_point = if E::is_undefined(cx, entry_point_value) {
        None
    } else {
        Some(E::to_str(cx, entry_point_value, arena)?)
    };
    Ok(WGPUComputeState {
        nextInChain: ptr::null_mut(),
        module,
        entryPoint: entry_point.map_or_else(
            || WGPUStringView { data: ptr::null(), length: wgpu_strlen() },
            |value| WGPUStringView::from_bytes(value.as_bytes()),
        ),
        // Policy skip: recorded deferral: pipeline constants are outside the block 01-03 surface.
        constantCount: 0,
        constants: ptr::null(),
    })
}

/// Converts a JavaScript `GPUComputePipelineDescriptor` into `ConvertedComputePipelineDescriptor`.
pub(super) fn convert_compute_pipeline_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedComputePipelineDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let layout_value = required_member::<E>(cx, value, "layout")?;
    let compute_value = required_member::<E>(cx, value, "compute")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    // Policy: the handle-or-enum union preserves explicit handles and auto layout.
    let layout = if E::is_null(cx, layout_value) {
        return Err(E::type_error(cx, "layout"));
    } else if let Ok(handle) = pipeline_layout_handle::<E>(cx, layout_value) {
        handle
    } else {
        let union_arena = Arena::new();
        match E::to_str(cx, layout_value, &union_arena)? {
            "auto" => ptr::null_mut(),
            _ => return Err(E::type_error(cx, "(GPUPipelineLayout or GPUAutoLayoutMode)")),
        }
    };
    let compute = convert_programmable_stage::<E>(cx, compute_value, arena)?;
    let module = compute.module;
    let native = WGPUComputePipelineDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        layout,
        compute,
    };
    Ok(ConvertedComputePipelineDescriptor {
        native,
        module,
        layout,
    })
}

/// Converts a JavaScript `GPUCommandEncoderDescriptor` into `WGPUCommandEncoderDescriptor`.
pub(super) fn convert_command_encoder_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUCommandEncoderDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    Ok(WGPUCommandEncoderDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

/// Converts a JavaScript `GPUCommandBufferDescriptor` into `WGPUCommandBufferDescriptor`.
pub(super) fn convert_command_buffer_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUCommandBufferDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    Ok(WGPUCommandBufferDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

/// Converts a JavaScript `GPUComputePassDescriptor` into `WGPUComputePassDescriptor`.
pub(super) fn convert_compute_pass_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUComputePassDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
    let timestamp_writes_value = E::get_property(cx, value, "timestampWrites")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, timestamp_writes_value) {
        return Err(E::type_error(cx, "timestampWrites are not supported yet"));
    }
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    Ok(WGPUComputePassDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        // Policy skip: out of scope until query sets.
        timestampWrites: ptr::null(),
    })
}

/// Converts a JavaScript `GPUBufferBindingLayout` into `WGPUBufferBindingLayout`.
pub(super) fn convert_buffer_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBufferBindingLayout, E::Error> {
    let type_value = E::get_property(cx, value, "type")?;
    let has_dynamic_offset_value = E::get_property(cx, value, "hasDynamicOffset")?;
    let min_binding_size_value = E::get_property(cx, value, "minBindingSize")?;
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

/// Converts a JavaScript `GPUBindGroupLayoutEntry` into `WGPUBindGroupLayoutEntry`.
pub(super) fn convert_bind_group_layout_entry<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupLayoutEntry, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let binding_value = required_member::<E>(cx, value, "binding")?;
    let visibility_value = required_member::<E>(cx, value, "visibility")?;
    let buffer_value = E::get_property(cx, value, "buffer")?;
    let sampler_value = E::get_property(cx, value, "sampler")?;
    // G7 carve-out: fail early instead of silently emitting a wrong layout.
    if !E::is_undefined(cx, sampler_value) {
        return Err(E::type_error(cx, "sampler bindings are not supported yet"));
    }
    let texture_value = E::get_property(cx, value, "texture")?;
    // G7 carve-out: fail early instead of silently emitting a wrong layout.
    if !E::is_undefined(cx, texture_value) {
        return Err(E::type_error(cx, "texture bindings are not supported yet"));
    }
    let storage_texture_value = E::get_property(cx, value, "storageTexture")?;
    // G7 carve-out: fail early instead of silently emitting a wrong layout.
    if !E::is_undefined(cx, storage_texture_value) {
        return Err(E::type_error(cx, "storageTexture bindings are not supported yet"));
    }
    let external_texture_value = E::get_property(cx, value, "externalTexture")?;
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
    Ok(WGPUBindGroupLayoutEntry {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUIndex32 is checked at the 32-bit boundary.
        binding: enforce_u32::<E>(cx, binding_value, "binding")?,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        visibility: u64::from(enforce_u32::<E>(cx, visibility_value, "visibility")?),
        buffer,
        // SAFETY: policy permits only a joined `default: zero` C member here.
        sampler: unsafe { std::mem::zeroed() },
        // SAFETY: policy permits only a joined `default: zero` C member here.
        texture: unsafe { std::mem::zeroed() },
        // SAFETY: policy permits only a joined `default: zero` C member here.
        storageTexture: unsafe { std::mem::zeroed() },
        bindingArraySize: 0,
    })
}

/// Converts a JavaScript `GPUBindGroupLayoutDescriptor` into `WGPUBindGroupLayoutDescriptor`.
pub(super) fn convert_bind_group_layout_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUBindGroupLayoutDescriptor, E::Error> {
    let label_value = E::get_property(cx, value, "label")?;
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
