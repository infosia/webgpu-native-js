/// Converts a JavaScript `GPUBindGroupEntry` into `WGPUBindGroupEntry`.
pub(super) fn convert_bind_group_entry<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    created_texture_views: &mut CreatedTextureViewCapture,
) -> Result<WGPUBindGroupEntry, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let binding_value = required_member::<E>(cx, value, "binding")?;
    let resource_value = required_member::<E>(cx, value, "resource")?;
    // C2/R24: wrapper-union arms are selected by generated ClassSpec identity.
    let sampler_resource = E::payload(cx, resource_value, GPU_SAMPLER_CLASS)
        .and_then(|payload| payload.downcast_ref::<SamplerPayload>())
        .map(|_| sampler_handle::<E>(cx, resource_value))
        .transpose()?;
    // C2/R24: wrapper-union arms are selected by generated ClassSpec identity.
    let texture_view_resource = E::payload(cx, resource_value, GPU_TEXTURE_VIEW_CLASS)
        .and_then(|payload| payload.downcast_ref::<TextureViewPayload>())
        .map(|_| texture_view_handle::<E>(cx, resource_value))
        .transpose()?;
    // B-4b: direct union arms are selected by generated ClassSpec identity.
    let buffer_direct_resource = E::payload(cx, resource_value, GPU_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<BufferPayload<E>>())
        .map(|_| ())
        .map(|_| buffer_handle::<E>(cx, resource_value))
        .transpose()?;
    // B-4b: direct union arms are selected by generated ClassSpec identity.
    let texture_direct_resource = E::payload(cx, resource_value, GPU_TEXTURE_CLASS)
        .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        .map(|payload| payload.texture);
    let texture_view_created_resource = if let Some(source) = texture_direct_resource {
        let created = unsafe { (E::environment(cx).gpu().texture_create_view)(source, ptr::null()) };
        if created.is_null() {
            return Err(E::operation_error(cx, "wgpuTextureCreateView returned null"));
        }
        created_texture_views.push(created);
        Some(created)
    } else {
        None
    };
    // B8: flattened handle conversion extracts only the native handle.
    let buffer = if let Some(direct) = buffer_direct_resource {
        direct
    } else if sampler_resource.is_some() || texture_view_resource.is_some() || buffer_direct_resource.is_some() || texture_direct_resource.is_some() {
        ptr::null_mut()
    } else {
        let buffer_value = E::get_property(cx, resource_value, "buffer")?;
        if E::is_undefined(cx, buffer_value) {
            return Err(E::type_error(cx, "resource must be a GPUBindingResource"));
        }
        buffer_handle::<E>(cx, buffer_value)?
    };
    // R8: flattened `[EnforceRange]` members keep their WebIDL width.
    let offset = if sampler_resource.is_some() || texture_view_resource.is_some() || buffer_direct_resource.is_some() || texture_direct_resource.is_some() {
        0
    } else {
        let offset_value = E::get_property(cx, resource_value, "offset")?;
        if E::is_undefined(cx, offset_value) {
            0
        } else {
            enforce_u64::<E>(cx, offset_value, "offset")?
        }
    };
    // R8: flattened `[EnforceRange]` members keep their WebIDL width.
    let size = if sampler_resource.is_some() || texture_view_resource.is_some() || buffer_direct_resource.is_some() || texture_direct_resource.is_some() {
        WGPU_WHOLE_SIZE as u64
    } else {
        let size_value = E::get_property(cx, resource_value, "size")?;
        if E::is_undefined(cx, size_value) {
            WGPU_WHOLE_SIZE as u64
        } else {
            enforce_u64::<E>(cx, size_value, "size")?
        }
    };
    Ok(WGPUBindGroupEntry {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUIndex32 is checked at the 32-bit boundary.
        binding: enforce_u32::<E>(cx, binding_value, "binding")?,
        buffer,
        offset,
        size,
        sampler: sampler_resource.unwrap_or(ptr::null_mut()),
        textureView: texture_view_resource.or(texture_view_created_resource).unwrap_or(ptr::null_mut()),
    })
}

/// Converts a JavaScript `GPUPipelineLayoutDescriptor` into `WGPUPipelineLayoutDescriptor`.
pub(super) fn convert_pipeline_layout_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUPipelineLayoutDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let bind_group_layouts_value = required_member::<E>(cx, value, "bindGroupLayouts")?;
    let immediate_size_value = dictionary_member::<E>(cx, value, "immediateSize")?;
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let bind_group_layouts = {
        // B8: conversion extracts handles only; create paths own retention.
        let converted = convert_sequence::<E, _>(cx, bind_group_layouts_value, "bindGroupLayouts", |item| {
            if E::is_null(cx, item) || E::is_undefined(cx, item) {
                Ok(ptr::null_mut())
            } else {
                bind_group_layout_handle::<E>(cx, item)
            }
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
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let code_value = required_member::<E>(cx, value, "code")?;
    let compilation_hints_value = dictionary_member::<E>(cx, value, "compilationHints")?;
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
    let entry_point_value = dictionary_member::<E>(cx, value, "entryPoint")?;
    let constants_value = dictionary_member::<E>(cx, value, "constants")?;
    let module = shader_module_handle::<E>(cx, module_value)?;
    // B4: optional non-nullable strings preserve absence; present null is stringified.
    let entry_point = if E::is_undefined(cx, entry_point_value) {
        None
    } else {
        Some(E::to_str(cx, entry_point_value, arena)?)
    };
    let constants = if E::is_undefined(cx, constants_value) {
        &[][..]
    } else {
        let names = E::own_property_names(cx, constants_value)?;
        let mut converted = Vec::with_capacity(names.len());
        for key in names {
            let item = E::get_property(cx, constants_value, &key)?;
            let value = restricted_f64::<E>(cx, item, "constants")?;
            let key = arena.alloc_str(&key);
            converted.push(WGPUConstantEntry {
                nextInChain: ptr::null_mut(),
                key: WGPUStringView::from_bytes(key.as_bytes()),
                value,
            });
        }
        arena.alloc_slice(converted)
    };
    Ok(WGPUComputeState {
        nextInChain: ptr::null_mut(),
        module,
        entryPoint: entry_point.map_or_else(
            || WGPUStringView { data: ptr::null(), length: wgpu_strlen() },
            |value| WGPUStringView::from_bytes(value.as_bytes()),
        ),
        constantCount: constants.len(),
        constants: if constants.is_empty() {
            ptr::null()
        } else {
            constants.as_ptr()
        },
    })
}
