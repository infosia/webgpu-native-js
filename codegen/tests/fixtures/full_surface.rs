/// Function-pointer dispatch for the WebGPU C ABI calls used by this slice.
#[derive(Clone, Copy)]
pub struct GpuDispatch {
    /// `wgpuInstanceProcessEvents`.
    pub instance_process_events: unsafe fn(WGPUInstance),
    /// `wgpuInstanceRequestAdapter`.
    pub instance_request_adapter: unsafe fn(WGPUInstance, *const WGPURequestAdapterOptions, WGPURequestAdapterCallbackInfo) -> WGPUFuture,
    /// `wgpuAdapterRequestDevice`.
    pub adapter_request_device: unsafe fn(WGPUAdapter, *const WGPUDeviceDescriptor, WGPURequestDeviceCallbackInfo) -> WGPUFuture,
    /// `wgpuAdapterRelease`.
    pub adapter_release: unsafe fn(WGPUAdapter),
    /// `wgpuBufferGetConstMappedRange`.
    pub buffer_get_const_mapped_range: unsafe fn(WGPUBuffer, usize, usize) -> *const ::std::ffi::c_void,
    /// `wgpuDeviceAddRef`.
    pub device_add_ref: unsafe fn(WGPUDevice),
    /// `wgpuDeviceRelease`.
    pub device_release: unsafe fn(WGPUDevice),
    /// `wgpuDeviceCreateBuffer`.
    pub device_create_buffer: unsafe fn(WGPUDevice, *const WGPUBufferDescriptor) -> WGPUBuffer,
    /// `wgpuDeviceCreateSampler`.
    pub device_create_sampler: unsafe fn(WGPUDevice, *const WGPUSamplerDescriptor) -> WGPUSampler,
    /// `wgpuDeviceCreateShaderModule`.
    pub device_create_shader_module: unsafe fn(WGPUDevice, *const WGPUShaderModuleDescriptor) -> WGPUShaderModule,
    /// `wgpuDeviceCreateBindGroupLayout`.
    pub device_create_bind_group_layout: unsafe fn(WGPUDevice, *const WGPUBindGroupLayoutDescriptor) -> WGPUBindGroupLayout,
    /// `wgpuDeviceCreatePipelineLayout`.
    pub device_create_pipeline_layout: unsafe fn(WGPUDevice, *const WGPUPipelineLayoutDescriptor) -> WGPUPipelineLayout,
    /// `wgpuDeviceCreateBindGroup`.
    pub device_create_bind_group: unsafe fn(WGPUDevice, *const WGPUBindGroupDescriptor) -> WGPUBindGroup,
    /// `wgpuDeviceCreateComputePipeline`.
    pub device_create_compute_pipeline: unsafe fn(WGPUDevice, *const WGPUComputePipelineDescriptor) -> WGPUComputePipeline,
    /// `wgpuDeviceCreateCommandEncoder`.
    pub device_create_command_encoder: unsafe fn(WGPUDevice, *const WGPUCommandEncoderDescriptor) -> WGPUCommandEncoder,
    /// `wgpuDeviceGetQueue`.
    pub device_get_queue: unsafe fn(WGPUDevice) -> WGPUQueue,
    /// `wgpuBufferAddRef`.
    pub buffer_add_ref: unsafe fn(WGPUBuffer),
    /// `wgpuBufferRelease`.
    pub buffer_release: unsafe fn(WGPUBuffer),
    /// `wgpuBufferMapAsync`.
    pub buffer_map_async: unsafe fn(WGPUBuffer, WGPUMapMode, usize, usize, WGPUBufferMapCallbackInfo) -> WGPUFuture,
    /// `wgpuBufferGetMappedRange`.
    pub buffer_get_mapped_range: unsafe fn(WGPUBuffer, usize, usize) -> *mut ::std::ffi::c_void,
    /// `wgpuBufferUnmap`.
    pub buffer_unmap: unsafe fn(WGPUBuffer),
    /// `wgpuBufferDestroy`.
    pub buffer_destroy: unsafe fn(WGPUBuffer),
    /// `wgpuBufferSetLabel`.
    pub buffer_set_label: unsafe fn(WGPUBuffer, WGPUStringView),
    /// `wgpuQueueAddRef`.
    pub queue_add_ref: unsafe fn(WGPUQueue),
    /// `wgpuQueueRelease`.
    pub queue_release: unsafe fn(WGPUQueue),
    /// `wgpuQueueWriteBuffer`.
    pub queue_write_buffer: unsafe fn(WGPUQueue, WGPUBuffer, u64, *const ::std::ffi::c_void, usize),
    /// `wgpuQueueSubmit`.
    pub queue_submit: unsafe fn(WGPUQueue, usize, *const WGPUCommandBuffer),
    /// `wgpuQueueOnSubmittedWorkDone`.
    pub queue_on_submitted_work_done: unsafe fn(WGPUQueue, WGPUQueueWorkDoneCallbackInfo) -> WGPUFuture,
    /// `wgpuShaderModuleAddRef`.
    pub shader_module_add_ref: unsafe fn(WGPUShaderModule),
    /// `wgpuShaderModuleRelease`.
    pub shader_module_release: unsafe fn(WGPUShaderModule),
    /// `wgpuSamplerAddRef`.
    pub sampler_add_ref: unsafe fn(WGPUSampler),
    /// `wgpuSamplerRelease`.
    pub sampler_release: unsafe fn(WGPUSampler),
    /// `wgpuSamplerSetLabel`.
    pub sampler_set_label: unsafe fn(WGPUSampler, WGPUStringView),
    /// `wgpuBindGroupLayoutAddRef`.
    pub bind_group_layout_add_ref: unsafe fn(WGPUBindGroupLayout),
    /// `wgpuBindGroupLayoutRelease`.
    pub bind_group_layout_release: unsafe fn(WGPUBindGroupLayout),
    /// `wgpuPipelineLayoutAddRef`.
    pub pipeline_layout_add_ref: unsafe fn(WGPUPipelineLayout),
    /// `wgpuPipelineLayoutRelease`.
    pub pipeline_layout_release: unsafe fn(WGPUPipelineLayout),
    /// `wgpuBindGroupAddRef`.
    pub bind_group_add_ref: unsafe fn(WGPUBindGroup),
    /// `wgpuBindGroupRelease`.
    pub bind_group_release: unsafe fn(WGPUBindGroup),
    /// `wgpuComputePipelineAddRef`.
    pub compute_pipeline_add_ref: unsafe fn(WGPUComputePipeline),
    /// `wgpuComputePipelineRelease`.
    pub compute_pipeline_release: unsafe fn(WGPUComputePipeline),
    /// `wgpuCommandEncoderRelease`.
    pub command_encoder_release: unsafe fn(WGPUCommandEncoder),
    /// `wgpuCommandEncoderBeginComputePass`.
    pub command_encoder_begin_compute_pass: unsafe fn(WGPUCommandEncoder, *const WGPUComputePassDescriptor) -> WGPUComputePassEncoder,
    /// `wgpuCommandEncoderCopyBufferToBuffer`.
    pub command_encoder_copy_buffer_to_buffer: unsafe fn(WGPUCommandEncoder, WGPUBuffer, u64, WGPUBuffer, u64, u64),
    /// `wgpuCommandEncoderFinish`.
    pub command_encoder_finish: unsafe fn(WGPUCommandEncoder, *const WGPUCommandBufferDescriptor) -> WGPUCommandBuffer,
    /// `wgpuComputePassEncoderRelease`.
    pub compute_pass_encoder_release: unsafe fn(WGPUComputePassEncoder),
    /// `wgpuComputePassEncoderSetPipeline`.
    pub compute_pass_encoder_set_pipeline: unsafe fn(WGPUComputePassEncoder, WGPUComputePipeline),
    /// `wgpuComputePassEncoderSetBindGroup`.
    pub compute_pass_encoder_set_bind_group: unsafe fn(WGPUComputePassEncoder, u32, WGPUBindGroup, usize, *const u32),
    /// `wgpuComputePassEncoderDispatchWorkgroups`.
    pub compute_pass_encoder_dispatch_workgroups: unsafe fn(WGPUComputePassEncoder, u32, u32, u32),
    /// `wgpuComputePassEncoderEnd`.
    pub compute_pass_encoder_end: unsafe fn(WGPUComputePassEncoder),
    /// `wgpuCommandBufferRelease`.
    pub command_buffer_release: unsafe fn(WGPUCommandBuffer),
}

/// Invokes a caller-supplied macro with every dispatch `(field, symbol, signature)` triple.
#[macro_export]
macro_rules! for_each_gpu_dispatch_entry {
    ($macro:ident $(, $context:ident)?) => {
        $macro! {
            $($context;)?
            (instance_process_events, wgpuInstanceProcessEvents, unsafe fn(instance: $crate::WGPUInstance)),
            (instance_request_adapter, wgpuInstanceRequestAdapter, unsafe fn(instance: $crate::WGPUInstance, options: *const $crate::WGPURequestAdapterOptions, callback_info: $crate::WGPURequestAdapterCallbackInfo) -> $crate::WGPUFuture),
            (adapter_request_device, wgpuAdapterRequestDevice, unsafe fn(adapter: $crate::WGPUAdapter, descriptor: *const $crate::WGPUDeviceDescriptor, callback_info: $crate::WGPURequestDeviceCallbackInfo) -> $crate::WGPUFuture),
            (adapter_release, wgpuAdapterRelease, unsafe fn(adapter: $crate::WGPUAdapter)),
            (buffer_get_const_mapped_range, wgpuBufferGetConstMappedRange, unsafe fn(buffer: $crate::WGPUBuffer, offset: usize, size: usize) -> *const ::std::ffi::c_void),
            (device_add_ref, wgpuDeviceAddRef, unsafe fn(device: $crate::WGPUDevice)),
            (device_release, wgpuDeviceRelease, unsafe fn(device: $crate::WGPUDevice)),
            (device_create_buffer, wgpuDeviceCreateBuffer, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUBufferDescriptor) -> $crate::WGPUBuffer),
            (device_create_sampler, wgpuDeviceCreateSampler, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUSamplerDescriptor) -> $crate::WGPUSampler),
            (device_create_shader_module, wgpuDeviceCreateShaderModule, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUShaderModuleDescriptor) -> $crate::WGPUShaderModule),
            (device_create_bind_group_layout, wgpuDeviceCreateBindGroupLayout, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUBindGroupLayoutDescriptor) -> $crate::WGPUBindGroupLayout),
            (device_create_pipeline_layout, wgpuDeviceCreatePipelineLayout, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUPipelineLayoutDescriptor) -> $crate::WGPUPipelineLayout),
            (device_create_bind_group, wgpuDeviceCreateBindGroup, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUBindGroupDescriptor) -> $crate::WGPUBindGroup),
            (device_create_compute_pipeline, wgpuDeviceCreateComputePipeline, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUComputePipelineDescriptor) -> $crate::WGPUComputePipeline),
            (device_create_command_encoder, wgpuDeviceCreateCommandEncoder, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUCommandEncoderDescriptor) -> $crate::WGPUCommandEncoder),
            (device_get_queue, wgpuDeviceGetQueue, unsafe fn(device: $crate::WGPUDevice) -> $crate::WGPUQueue),
            (buffer_add_ref, wgpuBufferAddRef, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_release, wgpuBufferRelease, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_map_async, wgpuBufferMapAsync, unsafe fn(buffer: $crate::WGPUBuffer, mode: $crate::WGPUMapMode, offset: usize, size: usize, callback_info: $crate::WGPUBufferMapCallbackInfo) -> $crate::WGPUFuture),
            (buffer_get_mapped_range, wgpuBufferGetMappedRange, unsafe fn(buffer: $crate::WGPUBuffer, offset: usize, size: usize) -> *mut ::std::ffi::c_void),
            (buffer_unmap, wgpuBufferUnmap, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_destroy, wgpuBufferDestroy, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_set_label, wgpuBufferSetLabel, unsafe fn(buffer: $crate::WGPUBuffer, label: $crate::WGPUStringView)),
            (queue_add_ref, wgpuQueueAddRef, unsafe fn(queue: $crate::WGPUQueue)),
            (queue_release, wgpuQueueRelease, unsafe fn(queue: $crate::WGPUQueue)),
            (queue_write_buffer, wgpuQueueWriteBuffer, unsafe fn(queue: $crate::WGPUQueue, buffer: $crate::WGPUBuffer, buffer_offset: u64, data: *const ::std::ffi::c_void, size: usize)),
            (queue_submit, wgpuQueueSubmit, unsafe fn(queue: $crate::WGPUQueue, commands_count: usize, commands: *const $crate::WGPUCommandBuffer)),
            (queue_on_submitted_work_done, wgpuQueueOnSubmittedWorkDone, unsafe fn(queue: $crate::WGPUQueue, callback_info: $crate::WGPUQueueWorkDoneCallbackInfo) -> $crate::WGPUFuture),
            (shader_module_add_ref, wgpuShaderModuleAddRef, unsafe fn(shader_module: $crate::WGPUShaderModule)),
            (shader_module_release, wgpuShaderModuleRelease, unsafe fn(shader_module: $crate::WGPUShaderModule)),
            (sampler_add_ref, wgpuSamplerAddRef, unsafe fn(sampler: $crate::WGPUSampler)),
            (sampler_release, wgpuSamplerRelease, unsafe fn(sampler: $crate::WGPUSampler)),
            (sampler_set_label, wgpuSamplerSetLabel, unsafe fn(sampler: $crate::WGPUSampler, label: $crate::WGPUStringView)),
            (bind_group_layout_add_ref, wgpuBindGroupLayoutAddRef, unsafe fn(bind_group_layout: $crate::WGPUBindGroupLayout)),
            (bind_group_layout_release, wgpuBindGroupLayoutRelease, unsafe fn(bind_group_layout: $crate::WGPUBindGroupLayout)),
            (pipeline_layout_add_ref, wgpuPipelineLayoutAddRef, unsafe fn(pipeline_layout: $crate::WGPUPipelineLayout)),
            (pipeline_layout_release, wgpuPipelineLayoutRelease, unsafe fn(pipeline_layout: $crate::WGPUPipelineLayout)),
            (bind_group_add_ref, wgpuBindGroupAddRef, unsafe fn(bind_group: $crate::WGPUBindGroup)),
            (bind_group_release, wgpuBindGroupRelease, unsafe fn(bind_group: $crate::WGPUBindGroup)),
            (compute_pipeline_add_ref, wgpuComputePipelineAddRef, unsafe fn(compute_pipeline: $crate::WGPUComputePipeline)),
            (compute_pipeline_release, wgpuComputePipelineRelease, unsafe fn(compute_pipeline: $crate::WGPUComputePipeline)),
            (command_encoder_release, wgpuCommandEncoderRelease, unsafe fn(command_encoder: $crate::WGPUCommandEncoder)),
            (command_encoder_begin_compute_pass, wgpuCommandEncoderBeginComputePass, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, descriptor: *const $crate::WGPUComputePassDescriptor) -> $crate::WGPUComputePassEncoder),
            (command_encoder_copy_buffer_to_buffer, wgpuCommandEncoderCopyBufferToBuffer, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, source: $crate::WGPUBuffer, source_offset: u64, destination: $crate::WGPUBuffer, destination_offset: u64, size: u64)),
            (command_encoder_finish, wgpuCommandEncoderFinish, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, descriptor: *const $crate::WGPUCommandBufferDescriptor) -> $crate::WGPUCommandBuffer),
            (compute_pass_encoder_release, wgpuComputePassEncoderRelease, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder)),
            (compute_pass_encoder_set_pipeline, wgpuComputePassEncoderSetPipeline, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, pipeline: $crate::WGPUComputePipeline)),
            (compute_pass_encoder_set_bind_group, wgpuComputePassEncoderSetBindGroup, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, group_index: u32, group: $crate::WGPUBindGroup, dynamic_offsets_count: usize, dynamic_offsets: *const u32)),
            (compute_pass_encoder_dispatch_workgroups, wgpuComputePassEncoderDispatchWorkgroups, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, workgroup_count_x: u32, workgroup_count_y: u32, workgroup_count_z: u32)),
            (compute_pass_encoder_end, wgpuComputePassEncoderEnd, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder)),
            (command_buffer_release, wgpuCommandBufferRelease, unsafe fn(command_buffer: $crate::WGPUCommandBuffer)),
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __gpu_dispatch_from_ffi {
    ($ffi:ident; $(($field:ident, $symbol:ident, unsafe fn($($argument:ident: $argument_type:ty),*) $(-> $result:ty)?),)*) => {{
        $(unsafe fn $field($($argument: $argument_type),*) $(-> $result)? {
            unsafe { $ffi::$symbol($($argument),*) }
        })*
        $crate::GpuDispatch { $($field),* }
    }};
}

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
