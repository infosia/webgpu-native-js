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
    /// `wgpuDeviceCreateTexture`.
    pub device_create_texture: unsafe fn(WGPUDevice, *const WGPUTextureDescriptor) -> WGPUTexture,
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
    /// `wgpuDeviceCreateRenderPipeline`.
    pub device_create_render_pipeline: unsafe fn(WGPUDevice, *const WGPURenderPipelineDescriptor) -> WGPURenderPipeline,
    /// `wgpuDeviceCreateCommandEncoder`.
    pub device_create_command_encoder: unsafe fn(WGPUDevice, *const WGPUCommandEncoderDescriptor) -> WGPUCommandEncoder,
    /// `wgpuDeviceGetQueue`.
    pub device_get_queue: unsafe fn(WGPUDevice) -> WGPUQueue,
    /// `wgpuDevicePushErrorScope`.
    pub device_push_error_scope: unsafe fn(WGPUDevice, WGPUErrorFilter),
    /// `wgpuDevicePopErrorScope`.
    pub device_pop_error_scope: unsafe fn(WGPUDevice, WGPUPopErrorScopeCallbackInfo) -> WGPUFuture,
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
    /// `wgpuTextureAddRef`.
    pub texture_add_ref: unsafe fn(WGPUTexture),
    /// `wgpuTextureRelease`.
    pub texture_release: unsafe fn(WGPUTexture),
    /// `wgpuTextureCreateView`.
    pub texture_create_view: unsafe fn(WGPUTexture, *const WGPUTextureViewDescriptor) -> WGPUTextureView,
    /// `wgpuTextureDestroy`.
    pub texture_destroy: unsafe fn(WGPUTexture),
    /// `wgpuTextureGetWidth`.
    pub texture_get_width: unsafe fn(WGPUTexture) -> u32,
    /// `wgpuTextureGetHeight`.
    pub texture_get_height: unsafe fn(WGPUTexture) -> u32,
    /// `wgpuTextureGetDepthOrArrayLayers`.
    pub texture_get_depth_or_array_layers: unsafe fn(WGPUTexture) -> u32,
    /// `wgpuTextureGetMipLevelCount`.
    pub texture_get_mip_level_count: unsafe fn(WGPUTexture) -> u32,
    /// `wgpuTextureGetSampleCount`.
    pub texture_get_sample_count: unsafe fn(WGPUTexture) -> u32,
    /// `wgpuTextureGetDimension`.
    pub texture_get_dimension: unsafe fn(WGPUTexture) -> WGPUTextureDimension,
    /// `wgpuTextureGetFormat`.
    pub texture_get_format: unsafe fn(WGPUTexture) -> WGPUTextureFormat,
    /// `wgpuTextureGetUsage`.
    pub texture_get_usage: unsafe fn(WGPUTexture) -> WGPUTextureUsage,
    /// `wgpuTextureViewAddRef`.
    pub texture_view_add_ref: unsafe fn(WGPUTextureView),
    /// `wgpuTextureViewRelease`.
    pub texture_view_release: unsafe fn(WGPUTextureView),
    /// `wgpuQueueAddRef`.
    pub queue_add_ref: unsafe fn(WGPUQueue),
    /// `wgpuQueueRelease`.
    pub queue_release: unsafe fn(WGPUQueue),
    /// `wgpuQueueWriteBuffer`.
    pub queue_write_buffer: unsafe fn(WGPUQueue, WGPUBuffer, u64, *const ::std::ffi::c_void, usize),
    /// `wgpuQueueWriteTexture`.
    pub queue_write_texture: unsafe fn(WGPUQueue, *const WGPUTexelCopyTextureInfo, *const ::std::ffi::c_void, usize, *const WGPUTexelCopyBufferLayout, *const WGPUExtent3D),
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
    /// `wgpuRenderPipelineAddRef`.
    pub render_pipeline_add_ref: unsafe fn(WGPURenderPipeline),
    /// `wgpuRenderPipelineRelease`.
    pub render_pipeline_release: unsafe fn(WGPURenderPipeline),
    /// `wgpuCommandEncoderRelease`.
    pub command_encoder_release: unsafe fn(WGPUCommandEncoder),
    /// `wgpuCommandEncoderBeginComputePass`.
    pub command_encoder_begin_compute_pass: unsafe fn(WGPUCommandEncoder, *const WGPUComputePassDescriptor) -> WGPUComputePassEncoder,
    /// `wgpuCommandEncoderBeginRenderPass`.
    pub command_encoder_begin_render_pass: unsafe fn(WGPUCommandEncoder, *const WGPURenderPassDescriptor) -> WGPURenderPassEncoder,
    /// `wgpuCommandEncoderCopyBufferToBuffer`.
    pub command_encoder_copy_buffer_to_buffer: unsafe fn(WGPUCommandEncoder, WGPUBuffer, u64, WGPUBuffer, u64, u64),
    /// `wgpuCommandEncoderCopyBufferToTexture`.
    pub command_encoder_copy_buffer_to_texture: unsafe fn(WGPUCommandEncoder, *const WGPUTexelCopyBufferInfo, *const WGPUTexelCopyTextureInfo, *const WGPUExtent3D),
    /// `wgpuCommandEncoderCopyTextureToBuffer`.
    pub command_encoder_copy_texture_to_buffer: unsafe fn(WGPUCommandEncoder, *const WGPUTexelCopyTextureInfo, *const WGPUTexelCopyBufferInfo, *const WGPUExtent3D),
    /// `wgpuCommandEncoderCopyTextureToTexture`.
    pub command_encoder_copy_texture_to_texture: unsafe fn(WGPUCommandEncoder, *const WGPUTexelCopyTextureInfo, *const WGPUTexelCopyTextureInfo, *const WGPUExtent3D),
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
    /// `wgpuRenderPassEncoderRelease`.
    pub render_pass_encoder_release: unsafe fn(WGPURenderPassEncoder),
    /// `wgpuRenderPassEncoderSetPipeline`.
    pub render_pass_encoder_set_pipeline: unsafe fn(WGPURenderPassEncoder, WGPURenderPipeline),
    /// `wgpuRenderPassEncoderSetVertexBuffer`.
    pub render_pass_encoder_set_vertex_buffer: unsafe fn(WGPURenderPassEncoder, u32, WGPUBuffer, u64, u64),
    /// `wgpuRenderPassEncoderSetIndexBuffer`.
    pub render_pass_encoder_set_index_buffer: unsafe fn(WGPURenderPassEncoder, WGPUBuffer, WGPUIndexFormat, u64, u64),
    /// `wgpuRenderPassEncoderSetBindGroup`.
    pub render_pass_encoder_set_bind_group: unsafe fn(WGPURenderPassEncoder, u32, WGPUBindGroup, usize, *const u32),
    /// `wgpuRenderPassEncoderDraw`.
    pub render_pass_encoder_draw: unsafe fn(WGPURenderPassEncoder, u32, u32, u32, u32),
    /// `wgpuRenderPassEncoderDrawIndexed`.
    pub render_pass_encoder_draw_indexed: unsafe fn(WGPURenderPassEncoder, u32, u32, u32, i32, u32),
    /// `wgpuRenderPassEncoderSetViewport`.
    pub render_pass_encoder_set_viewport: unsafe fn(WGPURenderPassEncoder, f32, f32, f32, f32, f32, f32),
    /// `wgpuRenderPassEncoderSetScissorRect`.
    pub render_pass_encoder_set_scissor_rect: unsafe fn(WGPURenderPassEncoder, u32, u32, u32, u32),
    /// `wgpuRenderPassEncoderEnd`.
    pub render_pass_encoder_end: unsafe fn(WGPURenderPassEncoder),
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
            (device_create_texture, wgpuDeviceCreateTexture, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUTextureDescriptor) -> $crate::WGPUTexture),
            (device_create_sampler, wgpuDeviceCreateSampler, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUSamplerDescriptor) -> $crate::WGPUSampler),
            (device_create_shader_module, wgpuDeviceCreateShaderModule, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUShaderModuleDescriptor) -> $crate::WGPUShaderModule),
            (device_create_bind_group_layout, wgpuDeviceCreateBindGroupLayout, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUBindGroupLayoutDescriptor) -> $crate::WGPUBindGroupLayout),
            (device_create_pipeline_layout, wgpuDeviceCreatePipelineLayout, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUPipelineLayoutDescriptor) -> $crate::WGPUPipelineLayout),
            (device_create_bind_group, wgpuDeviceCreateBindGroup, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUBindGroupDescriptor) -> $crate::WGPUBindGroup),
            (device_create_compute_pipeline, wgpuDeviceCreateComputePipeline, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUComputePipelineDescriptor) -> $crate::WGPUComputePipeline),
            (device_create_render_pipeline, wgpuDeviceCreateRenderPipeline, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPURenderPipelineDescriptor) -> $crate::WGPURenderPipeline),
            (device_create_command_encoder, wgpuDeviceCreateCommandEncoder, unsafe fn(device: $crate::WGPUDevice, descriptor: *const $crate::WGPUCommandEncoderDescriptor) -> $crate::WGPUCommandEncoder),
            (device_get_queue, wgpuDeviceGetQueue, unsafe fn(device: $crate::WGPUDevice) -> $crate::WGPUQueue),
            (device_push_error_scope, wgpuDevicePushErrorScope, unsafe fn(device: $crate::WGPUDevice, filter: $crate::WGPUErrorFilter)),
            (device_pop_error_scope, wgpuDevicePopErrorScope, unsafe fn(device: $crate::WGPUDevice, callback_info: $crate::WGPUPopErrorScopeCallbackInfo) -> $crate::WGPUFuture),
            (buffer_add_ref, wgpuBufferAddRef, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_release, wgpuBufferRelease, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_map_async, wgpuBufferMapAsync, unsafe fn(buffer: $crate::WGPUBuffer, mode: $crate::WGPUMapMode, offset: usize, size: usize, callback_info: $crate::WGPUBufferMapCallbackInfo) -> $crate::WGPUFuture),
            (buffer_get_mapped_range, wgpuBufferGetMappedRange, unsafe fn(buffer: $crate::WGPUBuffer, offset: usize, size: usize) -> *mut ::std::ffi::c_void),
            (buffer_unmap, wgpuBufferUnmap, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_destroy, wgpuBufferDestroy, unsafe fn(buffer: $crate::WGPUBuffer)),
            (buffer_set_label, wgpuBufferSetLabel, unsafe fn(buffer: $crate::WGPUBuffer, label: $crate::WGPUStringView)),
            (texture_add_ref, wgpuTextureAddRef, unsafe fn(texture: $crate::WGPUTexture)),
            (texture_release, wgpuTextureRelease, unsafe fn(texture: $crate::WGPUTexture)),
            (texture_create_view, wgpuTextureCreateView, unsafe fn(texture: $crate::WGPUTexture, descriptor: *const $crate::WGPUTextureViewDescriptor) -> $crate::WGPUTextureView),
            (texture_destroy, wgpuTextureDestroy, unsafe fn(texture: $crate::WGPUTexture)),
            (texture_get_width, wgpuTextureGetWidth, unsafe fn(texture: $crate::WGPUTexture) -> u32),
            (texture_get_height, wgpuTextureGetHeight, unsafe fn(texture: $crate::WGPUTexture) -> u32),
            (texture_get_depth_or_array_layers, wgpuTextureGetDepthOrArrayLayers, unsafe fn(texture: $crate::WGPUTexture) -> u32),
            (texture_get_mip_level_count, wgpuTextureGetMipLevelCount, unsafe fn(texture: $crate::WGPUTexture) -> u32),
            (texture_get_sample_count, wgpuTextureGetSampleCount, unsafe fn(texture: $crate::WGPUTexture) -> u32),
            (texture_get_dimension, wgpuTextureGetDimension, unsafe fn(texture: $crate::WGPUTexture) -> $crate::WGPUTextureDimension),
            (texture_get_format, wgpuTextureGetFormat, unsafe fn(texture: $crate::WGPUTexture) -> $crate::WGPUTextureFormat),
            (texture_get_usage, wgpuTextureGetUsage, unsafe fn(texture: $crate::WGPUTexture) -> $crate::WGPUTextureUsage),
            (texture_view_add_ref, wgpuTextureViewAddRef, unsafe fn(texture_view: $crate::WGPUTextureView)),
            (texture_view_release, wgpuTextureViewRelease, unsafe fn(texture_view: $crate::WGPUTextureView)),
            (queue_add_ref, wgpuQueueAddRef, unsafe fn(queue: $crate::WGPUQueue)),
            (queue_release, wgpuQueueRelease, unsafe fn(queue: $crate::WGPUQueue)),
            (queue_write_buffer, wgpuQueueWriteBuffer, unsafe fn(queue: $crate::WGPUQueue, buffer: $crate::WGPUBuffer, buffer_offset: u64, data: *const ::std::ffi::c_void, size: usize)),
            (queue_write_texture, wgpuQueueWriteTexture, unsafe fn(queue: $crate::WGPUQueue, destination: *const $crate::WGPUTexelCopyTextureInfo, data: *const ::std::ffi::c_void, data_size: usize, data_layout: *const $crate::WGPUTexelCopyBufferLayout, write_size: *const $crate::WGPUExtent3D)),
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
            (render_pipeline_add_ref, wgpuRenderPipelineAddRef, unsafe fn(render_pipeline: $crate::WGPURenderPipeline)),
            (render_pipeline_release, wgpuRenderPipelineRelease, unsafe fn(render_pipeline: $crate::WGPURenderPipeline)),
            (command_encoder_release, wgpuCommandEncoderRelease, unsafe fn(command_encoder: $crate::WGPUCommandEncoder)),
            (command_encoder_begin_compute_pass, wgpuCommandEncoderBeginComputePass, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, descriptor: *const $crate::WGPUComputePassDescriptor) -> $crate::WGPUComputePassEncoder),
            (command_encoder_begin_render_pass, wgpuCommandEncoderBeginRenderPass, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, descriptor: *const $crate::WGPURenderPassDescriptor) -> $crate::WGPURenderPassEncoder),
            (command_encoder_copy_buffer_to_buffer, wgpuCommandEncoderCopyBufferToBuffer, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, source: $crate::WGPUBuffer, source_offset: u64, destination: $crate::WGPUBuffer, destination_offset: u64, size: u64)),
            (command_encoder_copy_buffer_to_texture, wgpuCommandEncoderCopyBufferToTexture, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, source: *const $crate::WGPUTexelCopyBufferInfo, destination: *const $crate::WGPUTexelCopyTextureInfo, copy_size: *const $crate::WGPUExtent3D)),
            (command_encoder_copy_texture_to_buffer, wgpuCommandEncoderCopyTextureToBuffer, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, source: *const $crate::WGPUTexelCopyTextureInfo, destination: *const $crate::WGPUTexelCopyBufferInfo, copy_size: *const $crate::WGPUExtent3D)),
            (command_encoder_copy_texture_to_texture, wgpuCommandEncoderCopyTextureToTexture, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, source: *const $crate::WGPUTexelCopyTextureInfo, destination: *const $crate::WGPUTexelCopyTextureInfo, copy_size: *const $crate::WGPUExtent3D)),
            (command_encoder_finish, wgpuCommandEncoderFinish, unsafe fn(command_encoder: $crate::WGPUCommandEncoder, descriptor: *const $crate::WGPUCommandBufferDescriptor) -> $crate::WGPUCommandBuffer),
            (compute_pass_encoder_release, wgpuComputePassEncoderRelease, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder)),
            (compute_pass_encoder_set_pipeline, wgpuComputePassEncoderSetPipeline, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, pipeline: $crate::WGPUComputePipeline)),
            (compute_pass_encoder_set_bind_group, wgpuComputePassEncoderSetBindGroup, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, group_index: u32, group: $crate::WGPUBindGroup, dynamic_offsets_count: usize, dynamic_offsets: *const u32)),
            (compute_pass_encoder_dispatch_workgroups, wgpuComputePassEncoderDispatchWorkgroups, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder, workgroup_count_x: u32, workgroup_count_y: u32, workgroup_count_z: u32)),
            (compute_pass_encoder_end, wgpuComputePassEncoderEnd, unsafe fn(compute_pass_encoder: $crate::WGPUComputePassEncoder)),
            (render_pass_encoder_release, wgpuRenderPassEncoderRelease, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder)),
            (render_pass_encoder_set_pipeline, wgpuRenderPassEncoderSetPipeline, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, pipeline: $crate::WGPURenderPipeline)),
            (render_pass_encoder_set_vertex_buffer, wgpuRenderPassEncoderSetVertexBuffer, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, slot: u32, buffer: $crate::WGPUBuffer, offset: u64, size: u64)),
            (render_pass_encoder_set_index_buffer, wgpuRenderPassEncoderSetIndexBuffer, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, buffer: $crate::WGPUBuffer, format: $crate::WGPUIndexFormat, offset: u64, size: u64)),
            (render_pass_encoder_set_bind_group, wgpuRenderPassEncoderSetBindGroup, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, group_index: u32, group: $crate::WGPUBindGroup, dynamic_offsets_count: usize, dynamic_offsets: *const u32)),
            (render_pass_encoder_draw, wgpuRenderPassEncoderDraw, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, vertex_count: u32, instance_count: u32, first_vertex: u32, first_instance: u32)),
            (render_pass_encoder_draw_indexed, wgpuRenderPassEncoderDrawIndexed, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, index_count: u32, instance_count: u32, first_index: u32, base_vertex: i32, first_instance: u32)),
            (render_pass_encoder_set_viewport, wgpuRenderPassEncoderSetViewport, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, x: f32, y: f32, width: f32, height: f32, min_depth: f32, max_depth: f32)),
            (render_pass_encoder_set_scissor_rect, wgpuRenderPassEncoderSetScissorRect, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder, x: u32, y: u32, width: u32, height: u32)),
            (render_pass_encoder_end, wgpuRenderPassEncoderEnd, unsafe fn(render_pass_encoder: $crate::WGPURenderPassEncoder)),
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
    let mapped_at_creation_value = dictionary_member::<E>(cx, value, "mappedAtCreation")?;
    let label_value = dictionary_member::<E>(cx, value, "label")?;
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

/// Converts a JavaScript `GPUColorDict` into `WGPUColor`.
#[allow(dead_code)] // T1 emits union arms even before every typedef has an API consumer.
pub(super) fn convert_color_dict<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUColor, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let r_value = required_member::<E>(cx, value, "r")?;
    let g_value = required_member::<E>(cx, value, "g")?;
    let b_value = required_member::<E>(cx, value, "b")?;
    let a_value = required_member::<E>(cx, value, "a")?;
    Ok(WGPUColor {
        // WebIDL restricted `double` rejects non-finite values.
        r: restricted_f64::<E>(cx, r_value, "r")?,
        // WebIDL restricted `double` rejects non-finite values.
        g: restricted_f64::<E>(cx, g_value, "g")?,
        // WebIDL restricted `double` rejects non-finite values.
        b: restricted_f64::<E>(cx, b_value, "b")?,
        // WebIDL restricted `double` rejects non-finite values.
        a: restricted_f64::<E>(cx, a_value, "a")?,
    })
}

/// Converts a JavaScript `GPURenderPassColorAttachment` into `WGPURenderPassColorAttachment`.
pub(super) fn convert_render_pass_color_attachment<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPURenderPassColorAttachment, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let view_value = required_member::<E>(cx, value, "view")?;
    let depth_slice_value = dictionary_member::<E>(cx, value, "depthSlice")?;
    let resolve_target_value = dictionary_member::<E>(cx, value, "resolveTarget")?;
    let clear_value_value = dictionary_member::<E>(cx, value, "clearValue")?;
    let load_op_value = required_member::<E>(cx, value, "loadOp")?;
    let store_op_value = required_member::<E>(cx, value, "storeOp")?;
    let view = texture_view_handle::<E>(cx, view_value)?;
    let resolve_target = if E::is_undefined(cx, resolve_target_value) {
        ptr::null_mut()
    } else {
        texture_view_handle::<E>(cx, resolve_target_value)?
    };
    let clear_value = if E::is_undefined(cx, clear_value_value) {
        // The pinned C initializer uses the all-zero value for an absent numeric union.
        unsafe { std::mem::zeroed() }
    } else {
        convert_gpu_color::<E>(cx, clear_value_value)?
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let load_op = {
        let enum_arena = Arena::new();
        match E::to_str(cx, load_op_value, &enum_arena)? {
            "load" => WGPULoadOp_WGPULoadOp_Load,
            "clear" => WGPULoadOp_WGPULoadOp_Clear,
            _ => return Err(E::type_error(cx, "GPULoadOp")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let store_op = {
        let enum_arena = Arena::new();
        match E::to_str(cx, store_op_value, &enum_arena)? {
            "store" => WGPUStoreOp_WGPUStoreOp_Store,
            "discard" => WGPUStoreOp_WGPUStoreOp_Discard,
            _ => return Err(E::type_error(cx, "GPUStoreOp")),
        }
    };
    Ok(WGPURenderPassColorAttachment {
        nextInChain: ptr::null_mut(),
        view,
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        depthSlice: if E::is_undefined(cx, depth_slice_value) {
            WGPU_DEPTH_SLICE_UNDEFINED
        } else {
            enforce_u32::<E>(cx, depth_slice_value, "depthSlice")?
        },
        resolveTarget: resolve_target,
        clearValue: clear_value,
        loadOp: load_op,
        storeOp: store_op,
    })
}

/// Converts a JavaScript `GPURenderPassDepthStencilAttachment` into `WGPURenderPassDepthStencilAttachment`.
pub(super) fn convert_render_pass_depth_stencil_attachment<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPURenderPassDepthStencilAttachment, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let view_value = required_member::<E>(cx, value, "view")?;
    let depth_clear_value_value = dictionary_member::<E>(cx, value, "depthClearValue")?;
    let depth_load_op_value = dictionary_member::<E>(cx, value, "depthLoadOp")?;
    let depth_store_op_value = dictionary_member::<E>(cx, value, "depthStoreOp")?;
    let depth_read_only_value = dictionary_member::<E>(cx, value, "depthReadOnly")?;
    let stencil_clear_value_value = dictionary_member::<E>(cx, value, "stencilClearValue")?;
    let stencil_load_op_value = dictionary_member::<E>(cx, value, "stencilLoadOp")?;
    let stencil_store_op_value = dictionary_member::<E>(cx, value, "stencilStoreOp")?;
    let stencil_read_only_value = dictionary_member::<E>(cx, value, "stencilReadOnly")?;
    let view = texture_view_handle::<E>(cx, view_value)?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let depth_load_op = if E::is_undefined(cx, depth_load_op_value) {
        WGPULoadOp_WGPULoadOp_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, depth_load_op_value, &enum_arena)? {
            "load" => WGPULoadOp_WGPULoadOp_Load,
            "clear" => WGPULoadOp_WGPULoadOp_Clear,
            _ => return Err(E::type_error(cx, "GPULoadOp")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let depth_store_op = if E::is_undefined(cx, depth_store_op_value) {
        WGPUStoreOp_WGPUStoreOp_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, depth_store_op_value, &enum_arena)? {
            "store" => WGPUStoreOp_WGPUStoreOp_Store,
            "discard" => WGPUStoreOp_WGPUStoreOp_Discard,
            _ => return Err(E::type_error(cx, "GPUStoreOp")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let stencil_load_op = if E::is_undefined(cx, stencil_load_op_value) {
        WGPULoadOp_WGPULoadOp_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, stencil_load_op_value, &enum_arena)? {
            "load" => WGPULoadOp_WGPULoadOp_Load,
            "clear" => WGPULoadOp_WGPULoadOp_Clear,
            _ => return Err(E::type_error(cx, "GPULoadOp")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let stencil_store_op = if E::is_undefined(cx, stencil_store_op_value) {
        WGPUStoreOp_WGPUStoreOp_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, stencil_store_op_value, &enum_arena)? {
            "store" => WGPUStoreOp_WGPUStoreOp_Store,
            "discard" => WGPUStoreOp_WGPUStoreOp_Discard,
            _ => return Err(E::type_error(cx, "GPUStoreOp")),
        }
    };
    Ok(WGPURenderPassDepthStencilAttachment {
        nextInChain: ptr::null_mut(),
        view,
        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.
        depthClearValue: if E::is_undefined(cx, depth_clear_value_value) {
            WGPU_DEPTH_CLEAR_VALUE_UNDEFINED
        } else {
            restricted_f32::<E>(cx, depth_clear_value_value, "depthClearValue")?
        },
        depthLoadOp: depth_load_op,
        depthStoreOp: depth_store_op,
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        depthReadOnly: if E::is_undefined(cx, depth_read_only_value) {
            0
        } else {
            u32::from(E::to_bool(cx, depth_read_only_value))
        },
        // R8: `[EnforceRange]` GPUStencilValue is checked at the 32-bit boundary.
        stencilClearValue: if E::is_undefined(cx, stencil_clear_value_value) {
            0
        } else {
            enforce_u32::<E>(cx, stencil_clear_value_value, "stencilClearValue")?
        },
        stencilLoadOp: stencil_load_op,
        stencilStoreOp: stencil_store_op,
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        stencilReadOnly: if E::is_undefined(cx, stencil_read_only_value) {
            0
        } else {
            u32::from(E::to_bool(cx, stencil_read_only_value))
        },
    })
}

/// Converts a JavaScript `GPURenderPassDescriptor` into `WGPURenderPassDescriptor`.
pub(super) fn convert_render_pass_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPURenderPassDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let color_attachments_value = required_member::<E>(cx, value, "colorAttachments")?;
    let depth_stencil_attachment_value = dictionary_member::<E>(cx, value, "depthStencilAttachment")?;
    let occlusion_query_set_value = dictionary_member::<E>(cx, value, "occlusionQuerySet")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, occlusion_query_set_value) {
        return Err(E::type_error(cx, "occlusionQuerySet are not supported yet"));
    }
    let timestamp_writes_value = dictionary_member::<E>(cx, value, "timestampWrites")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, timestamp_writes_value) {
        return Err(E::type_error(cx, "timestampWrites are not supported yet"));
    }
    let max_draw_count_value = dictionary_member::<E>(cx, value, "maxDrawCount")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, max_draw_count_value) {
        return Err(E::type_error(cx, "maxDrawCount are not supported yet"));
    }
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let color_attachments = {
        let converted = convert_sequence::<E, _>(cx, color_attachments_value, "colorAttachments", |item| {
            // T5: nullable sequence elements are C sentinel-filled struct holes.
            if E::is_null(cx, item) {
                // The pinned webgpu.h INIT macro defines a hole with a null view,
                // undefined depth slice/load/store values, and a zero color.
                Ok(WGPURenderPassColorAttachment {
                    nextInChain: ptr::null_mut(),
                    view: ptr::null_mut(),
                    depthSlice: WGPU_DEPTH_SLICE_UNDEFINED,
                    resolveTarget: ptr::null_mut(),
                    loadOp: WGPULoadOp_WGPULoadOp_Undefined,
                    storeOp: WGPUStoreOp_WGPUStoreOp_Undefined,
                    // SAFETY: WGPU_COLOR_INIT is the all-zero WGPUColor.
                    clearValue: unsafe { std::mem::zeroed() },
                })
            } else {
                convert_render_pass_color_attachment::<E>(cx, item)
            }
        })?;
        arena.alloc_slice(converted)
    };
    // T5: an absent optional dictionary is a null pointer in the pinned C ABI.
    let depth_stencil_attachment = if E::is_undefined(cx, depth_stencil_attachment_value) {
        ptr::null()
    } else {
        let converted = convert_render_pass_depth_stencil_attachment::<E>(cx, depth_stencil_attachment_value)?;
        arena.alloc_slice(vec![converted]).as_ptr()
    };
    Ok(WGPURenderPassDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        colorAttachmentCount: color_attachments.len(),
        colorAttachments: if color_attachments.is_empty() {
            ptr::null()
        } else {
            color_attachments.as_ptr()
        },
        depthStencilAttachment: depth_stencil_attachment,
        // Policy skip: query sets are outside the block 09 render-pass slice.
        occlusionQuerySet: ptr::null_mut(),
        // Policy skip: query sets are outside the block 09 render-pass slice.
        timestampWrites: ptr::null(),
    })
}

/// Converts a JavaScript `GPUTexelCopyBufferLayout` into `WGPUTexelCopyBufferLayout`.
pub(super) fn convert_texel_copy_buffer_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTexelCopyBufferLayout, E::Error> {
    let offset_value = dictionary_member::<E>(cx, value, "offset")?;
    let bytes_per_row_value = dictionary_member::<E>(cx, value, "bytesPerRow")?;
    let rows_per_image_value = dictionary_member::<E>(cx, value, "rowsPerImage")?;
    Ok(WGPUTexelCopyBufferLayout {
        // R8: `[EnforceRange]` GPUSize64 is checked at the 64-bit boundary.
        offset: if E::is_undefined(cx, offset_value) {
            0
        } else {
            enforce_u64::<E>(cx, offset_value, "offset")?
        },
        // R8: `[EnforceRange]` GPUSize32 is checked at the 32-bit boundary.
        bytesPerRow: if E::is_undefined(cx, bytes_per_row_value) {
            WGPU_COPY_STRIDE_UNDEFINED
        } else {
            enforce_u32::<E>(cx, bytes_per_row_value, "bytesPerRow")?
        },
        // R8: `[EnforceRange]` GPUSize32 is checked at the 32-bit boundary.
        rowsPerImage: if E::is_undefined(cx, rows_per_image_value) {
            WGPU_COPY_STRIDE_UNDEFINED
        } else {
            enforce_u32::<E>(cx, rows_per_image_value, "rowsPerImage")?
        },
    })
}

/// Converts a JavaScript `GPUTexelCopyBufferInfo` into `WGPUTexelCopyBufferInfo`.
pub(super) fn convert_texel_copy_buffer_info<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTexelCopyBufferInfo, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let buffer_value = required_member::<E>(cx, value, "buffer")?;
    let layout = convert_texel_copy_buffer_layout::<E>(cx, value)?;
    let buffer = buffer_handle::<E>(cx, buffer_value)?;
    Ok(WGPUTexelCopyBufferInfo {
        buffer,
        layout,
    })
}

/// Converts a JavaScript `GPUTexelCopyTextureInfo` into `WGPUTexelCopyTextureInfo`.
pub(super) fn convert_texel_copy_texture_info<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTexelCopyTextureInfo, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let texture_value = required_member::<E>(cx, value, "texture")?;
    let mip_level_value = dictionary_member::<E>(cx, value, "mipLevel")?;
    let origin_value = dictionary_member::<E>(cx, value, "origin")?;
    let aspect_value = dictionary_member::<E>(cx, value, "aspect")?;
    let texture = texture_handle::<E>(cx, texture_value)?;
    let origin = if E::is_undefined(cx, origin_value) {
        // The pinned C initializer uses the all-zero value for an absent numeric union.
        unsafe { std::mem::zeroed() }
    } else {
        convert_gpu_origin3d::<E>(cx, origin_value)?
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let aspect = if E::is_undefined(cx, aspect_value) {
        WGPUTextureAspect_WGPUTextureAspect_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, aspect_value, &enum_arena)? {
            "all" => WGPUTextureAspect_WGPUTextureAspect_All,
            "stencil-only" => WGPUTextureAspect_WGPUTextureAspect_StencilOnly,
            "depth-only" => WGPUTextureAspect_WGPUTextureAspect_DepthOnly,
            _ => return Err(E::type_error(cx, "GPUTextureAspect")),
        }
    };
    Ok(WGPUTexelCopyTextureInfo {
        texture,
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        mipLevel: if E::is_undefined(cx, mip_level_value) {
            0
        } else {
            enforce_u32::<E>(cx, mip_level_value, "mipLevel")?
        },
        origin,
        aspect,
    })
}

/// Converts a JavaScript `GPUTextureDescriptor` into `WGPUTextureDescriptor`.
pub(super) fn convert_texture_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUTextureDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let size_value = required_member::<E>(cx, value, "size")?;
    let mip_level_count_value = dictionary_member::<E>(cx, value, "mipLevelCount")?;
    let sample_count_value = dictionary_member::<E>(cx, value, "sampleCount")?;
    let dimension_value = dictionary_member::<E>(cx, value, "dimension")?;
    let format_value = required_member::<E>(cx, value, "format")?;
    let usage_value = required_member::<E>(cx, value, "usage")?;
    let view_formats_value = dictionary_member::<E>(cx, value, "viewFormats")?;
    let texture_binding_view_dimension_value = dictionary_member::<E>(cx, value, "textureBindingViewDimension")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, texture_binding_view_dimension_value) {
        return Err(E::type_error(cx, "textureBindingViewDimension are not supported yet"));
    }
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    let size = convert_gpu_extent3d::<E>(cx, size_value)?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let dimension = if E::is_undefined(cx, dimension_value) {
        WGPUTextureDimension_WGPUTextureDimension_2D
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, dimension_value, &enum_arena)? {
            "1d" => WGPUTextureDimension_WGPUTextureDimension_1D,
            "2d" => WGPUTextureDimension_WGPUTextureDimension_2D,
            "3d" => WGPUTextureDimension_WGPUTextureDimension_3D,
            _ => return Err(E::type_error(cx, "GPUTextureDimension")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "r8unorm" => WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
            "r8snorm" => WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
            "r8uint" => WGPUTextureFormat_WGPUTextureFormat_R8Uint,
            "r8sint" => WGPUTextureFormat_WGPUTextureFormat_R8Sint,
            "r16unorm" => WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
            "r16snorm" => WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
            "r16uint" => WGPUTextureFormat_WGPUTextureFormat_R16Uint,
            "r16sint" => WGPUTextureFormat_WGPUTextureFormat_R16Sint,
            "r16float" => WGPUTextureFormat_WGPUTextureFormat_R16Float,
            "rg8unorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
            "rg8snorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
            "rg8uint" => WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
            "rg8sint" => WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
            "r32uint" => WGPUTextureFormat_WGPUTextureFormat_R32Uint,
            "r32sint" => WGPUTextureFormat_WGPUTextureFormat_R32Sint,
            "r32float" => WGPUTextureFormat_WGPUTextureFormat_R32Float,
            "rg16unorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
            "rg16snorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
            "rg16uint" => WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
            "rg16sint" => WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
            "rg16float" => WGPUTextureFormat_WGPUTextureFormat_RG16Float,
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
            "rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb,
            "rgba8snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
            "rgba8uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
            "rgba8sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
            "bgra8unorm" => WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
            "bgra8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
            "rgb9e5ufloat" => WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat,
            "rgb10a2uint" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
            "rgb10a2unorm" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
            "rg11b10ufloat" => WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
            "rg32uint" => WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
            "rg32sint" => WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
            "rg32float" => WGPUTextureFormat_WGPUTextureFormat_RG32Float,
            "rgba16unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
            "rgba16snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
            "rgba16uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
            "rgba16sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
            "rgba16float" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
            "rgba32uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
            "rgba32sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
            "rgba32float" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
            "stencil8" => WGPUTextureFormat_WGPUTextureFormat_Stencil8,
            "depth16unorm" => WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm,
            "depth24plus" => WGPUTextureFormat_WGPUTextureFormat_Depth24Plus,
            "depth24plus-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
            "depth32float" => WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
            "depth32float-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8,
            "bc1-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm,
            "bc1-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb,
            "bc2-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm,
            "bc2-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb,
            "bc3-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm,
            "bc3-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb,
            "bc4-r-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm,
            "bc4-r-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm,
            "bc5-rg-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm,
            "bc5-rg-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm,
            "bc6h-rgb-ufloat" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat,
            "bc6h-rgb-float" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat,
            "bc7-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm,
            "bc7-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb,
            "etc2-rgb8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm,
            "etc2-rgb8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb,
            "etc2-rgb8a1unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm,
            "etc2-rgb8a1unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
            "etc2-rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm,
            "etc2-rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb,
            "eac-r11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm,
            "eac-r11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm,
            "eac-rg11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm,
            "eac-rg11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm,
            "astc-4x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm,
            "astc-4x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb,
            "astc-5x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm,
            "astc-5x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb,
            "astc-5x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm,
            "astc-5x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb,
            "astc-6x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm,
            "astc-6x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb,
            "astc-6x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm,
            "astc-6x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb,
            "astc-8x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm,
            "astc-8x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb,
            "astc-8x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm,
            "astc-8x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb,
            "astc-8x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm,
            "astc-8x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb,
            "astc-10x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm,
            "astc-10x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb,
            "astc-10x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm,
            "astc-10x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb,
            "astc-10x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm,
            "astc-10x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb,
            "astc-10x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm,
            "astc-10x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb,
            "astc-12x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm,
            "astc-12x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb,
            "astc-12x12-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm,
            "astc-12x12-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    let view_formats = if E::is_undefined(cx, view_formats_value) {
        &[][..]
    } else {
        let converted = convert_sequence::<E, _>(cx, view_formats_value, "viewFormats", |item| {
            let enum_arena = Arena::new();
            match E::to_str(cx, item, &enum_arena)? {
                "r8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_R8Unorm),
                "r8snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_R8Snorm),
                "r8uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R8Uint),
                "r8sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R8Sint),
                "r16unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_R16Unorm),
                "r16snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_R16Snorm),
                "r16uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R16Uint),
                "r16sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R16Sint),
                "r16float" => Ok(WGPUTextureFormat_WGPUTextureFormat_R16Float),
                "rg8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG8Unorm),
                "rg8snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG8Snorm),
                "rg8uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG8Uint),
                "rg8sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG8Sint),
                "r32uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R32Uint),
                "r32sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_R32Sint),
                "r32float" => Ok(WGPUTextureFormat_WGPUTextureFormat_R32Float),
                "rg16unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG16Unorm),
                "rg16snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG16Snorm),
                "rg16uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG16Uint),
                "rg16sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG16Sint),
                "rg16float" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG16Float),
                "rgba8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm),
                "rgba8unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb),
                "rgba8snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm),
                "rgba8uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint),
                "rgba8sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint),
                "bgra8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm),
                "bgra8unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb),
                "rgb9e5ufloat" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat),
                "rgb10a2uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint),
                "rgb10a2unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm),
                "rg11b10ufloat" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat),
                "rg32uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG32Uint),
                "rg32sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG32Sint),
                "rg32float" => Ok(WGPUTextureFormat_WGPUTextureFormat_RG32Float),
                "rgba16unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm),
                "rgba16snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm),
                "rgba16uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint),
                "rgba16sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint),
                "rgba16float" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA16Float),
                "rgba32uint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint),
                "rgba32sint" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint),
                "rgba32float" => Ok(WGPUTextureFormat_WGPUTextureFormat_RGBA32Float),
                "stencil8" => Ok(WGPUTextureFormat_WGPUTextureFormat_Stencil8),
                "depth16unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm),
                "depth24plus" => Ok(WGPUTextureFormat_WGPUTextureFormat_Depth24Plus),
                "depth24plus-stencil8" => Ok(WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8),
                "depth32float" => Ok(WGPUTextureFormat_WGPUTextureFormat_Depth32Float),
                "depth32float-stencil8" => Ok(WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8),
                "bc1-rgba-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm),
                "bc1-rgba-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb),
                "bc2-rgba-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm),
                "bc2-rgba-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb),
                "bc3-rgba-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm),
                "bc3-rgba-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb),
                "bc4-r-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm),
                "bc4-r-snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm),
                "bc5-rg-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm),
                "bc5-rg-snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm),
                "bc6h-rgb-ufloat" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat),
                "bc6h-rgb-float" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat),
                "bc7-rgba-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm),
                "bc7-rgba-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb),
                "etc2-rgb8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm),
                "etc2-rgb8unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb),
                "etc2-rgb8a1unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm),
                "etc2-rgb8a1unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb),
                "etc2-rgba8unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm),
                "etc2-rgba8unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb),
                "eac-r11unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm),
                "eac-r11snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm),
                "eac-rg11unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm),
                "eac-rg11snorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm),
                "astc-4x4-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm),
                "astc-4x4-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb),
                "astc-5x4-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm),
                "astc-5x4-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb),
                "astc-5x5-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm),
                "astc-5x5-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb),
                "astc-6x5-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm),
                "astc-6x5-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb),
                "astc-6x6-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm),
                "astc-6x6-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb),
                "astc-8x5-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm),
                "astc-8x5-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb),
                "astc-8x6-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm),
                "astc-8x6-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb),
                "astc-8x8-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm),
                "astc-8x8-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb),
                "astc-10x5-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm),
                "astc-10x5-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb),
                "astc-10x6-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm),
                "astc-10x6-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb),
                "astc-10x8-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm),
                "astc-10x8-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb),
                "astc-10x10-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm),
                "astc-10x10-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb),
                "astc-12x10-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm),
                "astc-12x10-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb),
                "astc-12x12-unorm" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm),
                "astc-12x12-unorm-srgb" => Ok(WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb),
                _ => Err(E::type_error(cx, "GPUTextureFormat")),
            }
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUTextureDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        size,
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        mipLevelCount: if E::is_undefined(cx, mip_level_count_value) {
            1
        } else {
            enforce_u32::<E>(cx, mip_level_count_value, "mipLevelCount")?
        },
        // R8: `[EnforceRange]` GPUSize32 is checked at the 32-bit boundary.
        sampleCount: if E::is_undefined(cx, sample_count_value) {
            1
        } else {
            enforce_u32::<E>(cx, sample_count_value, "sampleCount")?
        },
        dimension,
        format,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        usage: u64::from(enforce_u32::<E>(cx, usage_value, "usage")?),
        viewFormatCount: view_formats.len(),
        viewFormats: if view_formats.is_empty() {
            ptr::null()
        } else {
            view_formats.as_ptr()
        },
    })
}

/// Converts a JavaScript `GPUTextureViewDescriptor` into `WGPUTextureViewDescriptor`.
pub(super) fn convert_texture_view_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUTextureViewDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    let format_value = dictionary_member::<E>(cx, value, "format")?;
    let dimension_value = dictionary_member::<E>(cx, value, "dimension")?;
    let usage_value = dictionary_member::<E>(cx, value, "usage")?;
    let aspect_value = dictionary_member::<E>(cx, value, "aspect")?;
    let base_mip_level_value = dictionary_member::<E>(cx, value, "baseMipLevel")?;
    let mip_level_count_value = dictionary_member::<E>(cx, value, "mipLevelCount")?;
    let base_array_layer_value = dictionary_member::<E>(cx, value, "baseArrayLayer")?;
    let array_layer_count_value = dictionary_member::<E>(cx, value, "arrayLayerCount")?;
    let swizzle_value = dictionary_member::<E>(cx, value, "swizzle")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, swizzle_value) {
        return Err(E::type_error(cx, "swizzle are not supported yet"));
    }
    // B4: non-nullable strings default only for undefined; null is stringified.
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = if E::is_undefined(cx, format_value) {
        WGPUTextureFormat_WGPUTextureFormat_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "r8unorm" => WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
            "r8snorm" => WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
            "r8uint" => WGPUTextureFormat_WGPUTextureFormat_R8Uint,
            "r8sint" => WGPUTextureFormat_WGPUTextureFormat_R8Sint,
            "r16unorm" => WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
            "r16snorm" => WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
            "r16uint" => WGPUTextureFormat_WGPUTextureFormat_R16Uint,
            "r16sint" => WGPUTextureFormat_WGPUTextureFormat_R16Sint,
            "r16float" => WGPUTextureFormat_WGPUTextureFormat_R16Float,
            "rg8unorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
            "rg8snorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
            "rg8uint" => WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
            "rg8sint" => WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
            "r32uint" => WGPUTextureFormat_WGPUTextureFormat_R32Uint,
            "r32sint" => WGPUTextureFormat_WGPUTextureFormat_R32Sint,
            "r32float" => WGPUTextureFormat_WGPUTextureFormat_R32Float,
            "rg16unorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
            "rg16snorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
            "rg16uint" => WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
            "rg16sint" => WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
            "rg16float" => WGPUTextureFormat_WGPUTextureFormat_RG16Float,
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
            "rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb,
            "rgba8snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
            "rgba8uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
            "rgba8sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
            "bgra8unorm" => WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
            "bgra8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
            "rgb9e5ufloat" => WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat,
            "rgb10a2uint" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
            "rgb10a2unorm" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
            "rg11b10ufloat" => WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
            "rg32uint" => WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
            "rg32sint" => WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
            "rg32float" => WGPUTextureFormat_WGPUTextureFormat_RG32Float,
            "rgba16unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
            "rgba16snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
            "rgba16uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
            "rgba16sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
            "rgba16float" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
            "rgba32uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
            "rgba32sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
            "rgba32float" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
            "stencil8" => WGPUTextureFormat_WGPUTextureFormat_Stencil8,
            "depth16unorm" => WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm,
            "depth24plus" => WGPUTextureFormat_WGPUTextureFormat_Depth24Plus,
            "depth24plus-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
            "depth32float" => WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
            "depth32float-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8,
            "bc1-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm,
            "bc1-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb,
            "bc2-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm,
            "bc2-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb,
            "bc3-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm,
            "bc3-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb,
            "bc4-r-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm,
            "bc4-r-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm,
            "bc5-rg-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm,
            "bc5-rg-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm,
            "bc6h-rgb-ufloat" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat,
            "bc6h-rgb-float" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat,
            "bc7-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm,
            "bc7-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb,
            "etc2-rgb8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm,
            "etc2-rgb8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb,
            "etc2-rgb8a1unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm,
            "etc2-rgb8a1unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
            "etc2-rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm,
            "etc2-rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb,
            "eac-r11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm,
            "eac-r11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm,
            "eac-rg11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm,
            "eac-rg11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm,
            "astc-4x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm,
            "astc-4x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb,
            "astc-5x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm,
            "astc-5x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb,
            "astc-5x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm,
            "astc-5x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb,
            "astc-6x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm,
            "astc-6x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb,
            "astc-6x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm,
            "astc-6x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb,
            "astc-8x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm,
            "astc-8x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb,
            "astc-8x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm,
            "astc-8x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb,
            "astc-8x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm,
            "astc-8x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb,
            "astc-10x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm,
            "astc-10x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb,
            "astc-10x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm,
            "astc-10x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb,
            "astc-10x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm,
            "astc-10x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb,
            "astc-10x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm,
            "astc-10x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb,
            "astc-12x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm,
            "astc-12x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb,
            "astc-12x12-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm,
            "astc-12x12-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let dimension = if E::is_undefined(cx, dimension_value) {
        WGPUTextureViewDimension_WGPUTextureViewDimension_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, dimension_value, &enum_arena)? {
            "1d" => WGPUTextureViewDimension_WGPUTextureViewDimension_1D,
            "2d" => WGPUTextureViewDimension_WGPUTextureViewDimension_2D,
            "2d-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray,
            "cube" => WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
            "cube-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
            "3d" => WGPUTextureViewDimension_WGPUTextureViewDimension_3D,
            _ => return Err(E::type_error(cx, "GPUTextureViewDimension")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let aspect = if E::is_undefined(cx, aspect_value) {
        WGPUTextureAspect_WGPUTextureAspect_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, aspect_value, &enum_arena)? {
            "all" => WGPUTextureAspect_WGPUTextureAspect_All,
            "stencil-only" => WGPUTextureAspect_WGPUTextureAspect_StencilOnly,
            "depth-only" => WGPUTextureAspect_WGPUTextureAspect_DepthOnly,
            _ => return Err(E::type_error(cx, "GPUTextureAspect")),
        }
    };
    Ok(WGPUTextureViewDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        format,
        dimension,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        usage: if E::is_undefined(cx, usage_value) {
            0
        } else {
            u64::from(enforce_u32::<E>(cx, usage_value, "usage")?)
        },
        aspect,
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        baseMipLevel: if E::is_undefined(cx, base_mip_level_value) {
            0
        } else {
            enforce_u32::<E>(cx, base_mip_level_value, "baseMipLevel")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        mipLevelCount: if E::is_undefined(cx, mip_level_count_value) {
            WGPU_MIP_LEVEL_COUNT_UNDEFINED
        } else {
            enforce_u32::<E>(cx, mip_level_count_value, "mipLevelCount")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        baseArrayLayer: if E::is_undefined(cx, base_array_layer_value) {
            0
        } else {
            enforce_u32::<E>(cx, base_array_layer_value, "baseArrayLayer")?
        },
        // R8: `[EnforceRange]` GPUIntegerCoordinate is checked at the 32-bit boundary.
        arrayLayerCount: if E::is_undefined(cx, array_layer_count_value) {
            WGPU_ARRAY_LAYER_COUNT_UNDEFINED
        } else {
            enforce_u32::<E>(cx, array_layer_count_value, "arrayLayerCount")?
        },
    })
}

/// Converts a JavaScript `GPUSamplerDescriptor` into `WGPUSamplerDescriptor`.
pub(super) fn convert_sampler_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUSamplerDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    let address_mode_u_value = dictionary_member::<E>(cx, value, "addressModeU")?;
    let address_mode_v_value = dictionary_member::<E>(cx, value, "addressModeV")?;
    let address_mode_w_value = dictionary_member::<E>(cx, value, "addressModeW")?;
    let mag_filter_value = dictionary_member::<E>(cx, value, "magFilter")?;
    let min_filter_value = dictionary_member::<E>(cx, value, "minFilter")?;
    let mipmap_filter_value = dictionary_member::<E>(cx, value, "mipmapFilter")?;
    let lod_min_clamp_value = dictionary_member::<E>(cx, value, "lodMinClamp")?;
    let lod_max_clamp_value = dictionary_member::<E>(cx, value, "lodMaxClamp")?;
    let compare_value = dictionary_member::<E>(cx, value, "compare")?;
    let max_anisotropy_value = dictionary_member::<E>(cx, value, "maxAnisotropy")?;
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
    // C2/R24: wrapper-union arms are selected by generated ClassSpec identity.
    let sampler_resource = E::payload(cx, resource_value, GPU_SAMPLER_CLASS)
        .and_then(|payload| payload.downcast_ref::<SamplerPayload>())
        .map(|payload| payload.sampler);
    // C2/R24: wrapper-union arms are selected by generated ClassSpec identity.
    let texture_view_resource = E::payload(cx, resource_value, GPU_TEXTURE_VIEW_CLASS)
        .and_then(|payload| payload.downcast_ref::<TextureViewPayload>())
        .map(|payload| payload.texture_view);
    // B8: flattened handle conversion extracts only the native handle.
    let buffer = if sampler_resource.is_some() || texture_view_resource.is_some() {
        ptr::null_mut()
    } else {
        let buffer_value = E::get_property(cx, resource_value, "buffer")?;
        if E::is_undefined(cx, buffer_value) {
            return Err(E::type_error(cx, "resource must be a GPUBufferBinding"));
        }
        buffer_handle::<E>(cx, buffer_value)?
    };
    // R8: flattened `[EnforceRange]` members keep their WebIDL width.
    let offset = if sampler_resource.is_some() || texture_view_resource.is_some() {
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
    let size = if sampler_resource.is_some() || texture_view_resource.is_some() {
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
        textureView: texture_view_resource.unwrap_or(ptr::null_mut()),
    })
}

/// Converts a JavaScript `GPUBindGroupDescriptor` into `ConvertedBindGroupDescriptor`.
pub(super) fn convert_bind_group_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedBindGroupDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
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
    let samplers = entries
        .iter()
        .filter_map(|item| (!item.sampler.is_null()).then_some(item.sampler))
        .collect();
    let texture_views = entries
        .iter()
        .filter_map(|item| (!item.textureView.is_null()).then_some(item.textureView))
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
        samplers,
        texture_views,
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
    let label_value = dictionary_member::<E>(cx, value, "label")?;
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

/// Converts a JavaScript `GPUVertexAttribute` into `WGPUVertexAttribute`.
pub(super) fn convert_vertex_attribute<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUVertexAttribute, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let format_value = required_member::<E>(cx, value, "format")?;
    let offset_value = required_member::<E>(cx, value, "offset")?;
    let shader_location_value = required_member::<E>(cx, value, "shaderLocation")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "uint8" => WGPUVertexFormat_WGPUVertexFormat_Uint8,
            "uint8x2" => WGPUVertexFormat_WGPUVertexFormat_Uint8x2,
            "uint8x4" => WGPUVertexFormat_WGPUVertexFormat_Uint8x4,
            "sint8" => WGPUVertexFormat_WGPUVertexFormat_Sint8,
            "sint8x2" => WGPUVertexFormat_WGPUVertexFormat_Sint8x2,
            "sint8x4" => WGPUVertexFormat_WGPUVertexFormat_Sint8x4,
            "unorm8" => WGPUVertexFormat_WGPUVertexFormat_Unorm8,
            "unorm8x2" => WGPUVertexFormat_WGPUVertexFormat_Unorm8x2,
            "unorm8x4" => WGPUVertexFormat_WGPUVertexFormat_Unorm8x4,
            "snorm8" => WGPUVertexFormat_WGPUVertexFormat_Snorm8,
            "snorm8x2" => WGPUVertexFormat_WGPUVertexFormat_Snorm8x2,
            "snorm8x4" => WGPUVertexFormat_WGPUVertexFormat_Snorm8x4,
            "uint16" => WGPUVertexFormat_WGPUVertexFormat_Uint16,
            "uint16x2" => WGPUVertexFormat_WGPUVertexFormat_Uint16x2,
            "uint16x4" => WGPUVertexFormat_WGPUVertexFormat_Uint16x4,
            "sint16" => WGPUVertexFormat_WGPUVertexFormat_Sint16,
            "sint16x2" => WGPUVertexFormat_WGPUVertexFormat_Sint16x2,
            "sint16x4" => WGPUVertexFormat_WGPUVertexFormat_Sint16x4,
            "unorm16" => WGPUVertexFormat_WGPUVertexFormat_Unorm16,
            "unorm16x2" => WGPUVertexFormat_WGPUVertexFormat_Unorm16x2,
            "unorm16x4" => WGPUVertexFormat_WGPUVertexFormat_Unorm16x4,
            "snorm16" => WGPUVertexFormat_WGPUVertexFormat_Snorm16,
            "snorm16x2" => WGPUVertexFormat_WGPUVertexFormat_Snorm16x2,
            "snorm16x4" => WGPUVertexFormat_WGPUVertexFormat_Snorm16x4,
            "float16" => WGPUVertexFormat_WGPUVertexFormat_Float16,
            "float16x2" => WGPUVertexFormat_WGPUVertexFormat_Float16x2,
            "float16x4" => WGPUVertexFormat_WGPUVertexFormat_Float16x4,
            "float32" => WGPUVertexFormat_WGPUVertexFormat_Float32,
            "float32x2" => WGPUVertexFormat_WGPUVertexFormat_Float32x2,
            "float32x3" => WGPUVertexFormat_WGPUVertexFormat_Float32x3,
            "float32x4" => WGPUVertexFormat_WGPUVertexFormat_Float32x4,
            "uint32" => WGPUVertexFormat_WGPUVertexFormat_Uint32,
            "uint32x2" => WGPUVertexFormat_WGPUVertexFormat_Uint32x2,
            "uint32x3" => WGPUVertexFormat_WGPUVertexFormat_Uint32x3,
            "uint32x4" => WGPUVertexFormat_WGPUVertexFormat_Uint32x4,
            "sint32" => WGPUVertexFormat_WGPUVertexFormat_Sint32,
            "sint32x2" => WGPUVertexFormat_WGPUVertexFormat_Sint32x2,
            "sint32x3" => WGPUVertexFormat_WGPUVertexFormat_Sint32x3,
            "sint32x4" => WGPUVertexFormat_WGPUVertexFormat_Sint32x4,
            "unorm10-10-10-2" => WGPUVertexFormat_WGPUVertexFormat_Unorm10_10_10_2,
            "unorm8x4-bgra" => WGPUVertexFormat_WGPUVertexFormat_Unorm8x4BGRA,
            _ => return Err(E::type_error(cx, "GPUVertexFormat")),
        }
    };
    Ok(WGPUVertexAttribute {
        nextInChain: ptr::null_mut(),
        format,
        // R8: `[EnforceRange]` GPUSize64 is checked at the 64-bit boundary.
        offset: enforce_u64::<E>(cx, offset_value, "offset")?,
        // R8: `[EnforceRange]` GPUIndex32 is checked at the 32-bit boundary.
        shaderLocation: enforce_u32::<E>(cx, shader_location_value, "shaderLocation")?,
    })
}

/// Converts a JavaScript `GPUVertexBufferLayout` into `WGPUVertexBufferLayout`.
pub(super) fn convert_vertex_buffer_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUVertexBufferLayout, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let array_stride_value = required_member::<E>(cx, value, "arrayStride")?;
    let step_mode_value = dictionary_member::<E>(cx, value, "stepMode")?;
    let attributes_value = required_member::<E>(cx, value, "attributes")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let step_mode = if E::is_undefined(cx, step_mode_value) {
        WGPUVertexStepMode_WGPUVertexStepMode_Vertex
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, step_mode_value, &enum_arena)? {
            "vertex" => WGPUVertexStepMode_WGPUVertexStepMode_Vertex,
            "instance" => WGPUVertexStepMode_WGPUVertexStepMode_Instance,
            _ => return Err(E::type_error(cx, "GPUVertexStepMode")),
        }
    };
    let attributes = {
        let converted = convert_sequence::<E, _>(cx, attributes_value, "attributes", |item| {
            convert_vertex_attribute::<E>(cx, item)
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUVertexBufferLayout {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUSize64 is checked at the 64-bit boundary.
        arrayStride: enforce_u64::<E>(cx, array_stride_value, "arrayStride")?,
        stepMode: step_mode,
        attributeCount: attributes.len(),
        attributes: if attributes.is_empty() {
            ptr::null()
        } else {
            attributes.as_ptr()
        },
    })
}

/// Converts a JavaScript `GPUVertexState` into `WGPUVertexState`.
pub(super) fn convert_vertex_state<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUVertexState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let module_value = required_member::<E>(cx, value, "module")?;
    let entry_point_value = dictionary_member::<E>(cx, value, "entryPoint")?;
    let constants_value = dictionary_member::<E>(cx, value, "constants")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, constants_value) {
        return Err(E::type_error(cx, "constants are not supported yet"));
    }
    let buffers_value = dictionary_member::<E>(cx, value, "buffers")?;
    let module = shader_module_handle::<E>(cx, module_value)?;
    // B4: optional non-nullable strings preserve absence; present null is stringified.
    let entry_point = if E::is_undefined(cx, entry_point_value) {
        None
    } else {
        Some(E::to_str(cx, entry_point_value, arena)?)
    };
    let buffers = if E::is_undefined(cx, buffers_value) {
        &[][..]
    } else {
        let converted = convert_sequence::<E, _>(cx, buffers_value, "buffers", |item| {
            // T5: nullable sequence elements are C sentinel-filled struct holes.
            if E::is_null(cx, item) {
                // SAFETY: the pinned C ABI defines the all-zero element as the hole sentinel.
                Ok(unsafe { std::mem::zeroed() })
            } else {
                convert_vertex_buffer_layout::<E>(cx, item, arena)
            }
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUVertexState {
        nextInChain: ptr::null_mut(),
        module,
        entryPoint: entry_point.map_or_else(
            || WGPUStringView { data: ptr::null(), length: wgpu_strlen() },
            |value| WGPUStringView::from_bytes(value.as_bytes()),
        ),
        // Policy skip: recorded deferral: pipeline constants are outside the block 09 slice-3 surface.
        constantCount: 0,
        constants: ptr::null(),
        bufferCount: buffers.len(),
        buffers: if buffers.is_empty() {
            ptr::null()
        } else {
            buffers.as_ptr()
        },
    })
}

/// Converts a JavaScript `GPUPrimitiveState` into `WGPUPrimitiveState`.
pub(super) fn convert_primitive_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUPrimitiveState, E::Error> {
    let topology_value = dictionary_member::<E>(cx, value, "topology")?;
    let strip_index_format_value = dictionary_member::<E>(cx, value, "stripIndexFormat")?;
    let front_face_value = dictionary_member::<E>(cx, value, "frontFace")?;
    let cull_mode_value = dictionary_member::<E>(cx, value, "cullMode")?;
    let unclipped_depth_value = dictionary_member::<E>(cx, value, "unclippedDepth")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let topology = if E::is_undefined(cx, topology_value) {
        WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleList
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, topology_value, &enum_arena)? {
            "point-list" => WGPUPrimitiveTopology_WGPUPrimitiveTopology_PointList,
            "line-list" => WGPUPrimitiveTopology_WGPUPrimitiveTopology_LineList,
            "line-strip" => WGPUPrimitiveTopology_WGPUPrimitiveTopology_LineStrip,
            "triangle-list" => WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleList,
            "triangle-strip" => WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleStrip,
            _ => return Err(E::type_error(cx, "GPUPrimitiveTopology")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let strip_index_format = if E::is_undefined(cx, strip_index_format_value) {
        WGPUIndexFormat_WGPUIndexFormat_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, strip_index_format_value, &enum_arena)? {
            "uint16" => WGPUIndexFormat_WGPUIndexFormat_Uint16,
            "uint32" => WGPUIndexFormat_WGPUIndexFormat_Uint32,
            _ => return Err(E::type_error(cx, "GPUIndexFormat")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let front_face = if E::is_undefined(cx, front_face_value) {
        WGPUFrontFace_WGPUFrontFace_CCW
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, front_face_value, &enum_arena)? {
            "ccw" => WGPUFrontFace_WGPUFrontFace_CCW,
            "cw" => WGPUFrontFace_WGPUFrontFace_CW,
            _ => return Err(E::type_error(cx, "GPUFrontFace")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let cull_mode = if E::is_undefined(cx, cull_mode_value) {
        WGPUCullMode_WGPUCullMode_None
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, cull_mode_value, &enum_arena)? {
            "none" => WGPUCullMode_WGPUCullMode_None,
            "front" => WGPUCullMode_WGPUCullMode_Front,
            "back" => WGPUCullMode_WGPUCullMode_Back,
            _ => return Err(E::type_error(cx, "GPUCullMode")),
        }
    };
    Ok(WGPUPrimitiveState {
        nextInChain: ptr::null_mut(),
        topology,
        stripIndexFormat: strip_index_format,
        frontFace: front_face,
        cullMode: cull_mode,
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        unclippedDepth: if E::is_undefined(cx, unclipped_depth_value) {
            0
        } else {
            u32::from(E::to_bool(cx, unclipped_depth_value))
        },
    })
}

/// Converts a JavaScript `GPUStencilFaceState` into `WGPUStencilFaceState`.
pub(super) fn convert_stencil_face_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUStencilFaceState, E::Error> {
    let compare_value = dictionary_member::<E>(cx, value, "compare")?;
    let fail_op_value = dictionary_member::<E>(cx, value, "failOp")?;
    let depth_fail_op_value = dictionary_member::<E>(cx, value, "depthFailOp")?;
    let pass_op_value = dictionary_member::<E>(cx, value, "passOp")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let compare = if E::is_undefined(cx, compare_value) {
        WGPUCompareFunction_WGPUCompareFunction_Always
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
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let fail_op = if E::is_undefined(cx, fail_op_value) {
        WGPUStencilOperation_WGPUStencilOperation_Keep
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, fail_op_value, &enum_arena)? {
            "keep" => WGPUStencilOperation_WGPUStencilOperation_Keep,
            "zero" => WGPUStencilOperation_WGPUStencilOperation_Zero,
            "replace" => WGPUStencilOperation_WGPUStencilOperation_Replace,
            "invert" => WGPUStencilOperation_WGPUStencilOperation_Invert,
            "increment-clamp" => WGPUStencilOperation_WGPUStencilOperation_IncrementClamp,
            "decrement-clamp" => WGPUStencilOperation_WGPUStencilOperation_DecrementClamp,
            "increment-wrap" => WGPUStencilOperation_WGPUStencilOperation_IncrementWrap,
            "decrement-wrap" => WGPUStencilOperation_WGPUStencilOperation_DecrementWrap,
            _ => return Err(E::type_error(cx, "GPUStencilOperation")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let depth_fail_op = if E::is_undefined(cx, depth_fail_op_value) {
        WGPUStencilOperation_WGPUStencilOperation_Keep
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, depth_fail_op_value, &enum_arena)? {
            "keep" => WGPUStencilOperation_WGPUStencilOperation_Keep,
            "zero" => WGPUStencilOperation_WGPUStencilOperation_Zero,
            "replace" => WGPUStencilOperation_WGPUStencilOperation_Replace,
            "invert" => WGPUStencilOperation_WGPUStencilOperation_Invert,
            "increment-clamp" => WGPUStencilOperation_WGPUStencilOperation_IncrementClamp,
            "decrement-clamp" => WGPUStencilOperation_WGPUStencilOperation_DecrementClamp,
            "increment-wrap" => WGPUStencilOperation_WGPUStencilOperation_IncrementWrap,
            "decrement-wrap" => WGPUStencilOperation_WGPUStencilOperation_DecrementWrap,
            _ => return Err(E::type_error(cx, "GPUStencilOperation")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let pass_op = if E::is_undefined(cx, pass_op_value) {
        WGPUStencilOperation_WGPUStencilOperation_Keep
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, pass_op_value, &enum_arena)? {
            "keep" => WGPUStencilOperation_WGPUStencilOperation_Keep,
            "zero" => WGPUStencilOperation_WGPUStencilOperation_Zero,
            "replace" => WGPUStencilOperation_WGPUStencilOperation_Replace,
            "invert" => WGPUStencilOperation_WGPUStencilOperation_Invert,
            "increment-clamp" => WGPUStencilOperation_WGPUStencilOperation_IncrementClamp,
            "decrement-clamp" => WGPUStencilOperation_WGPUStencilOperation_DecrementClamp,
            "increment-wrap" => WGPUStencilOperation_WGPUStencilOperation_IncrementWrap,
            "decrement-wrap" => WGPUStencilOperation_WGPUStencilOperation_DecrementWrap,
            _ => return Err(E::type_error(cx, "GPUStencilOperation")),
        }
    };
    Ok(WGPUStencilFaceState {
        compare,
        failOp: fail_op,
        depthFailOp: depth_fail_op,
        passOp: pass_op,
    })
}

/// Converts a JavaScript `GPUDepthStencilState` into `WGPUDepthStencilState`.
pub(super) fn convert_depth_stencil_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUDepthStencilState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let format_value = required_member::<E>(cx, value, "format")?;
    let depth_write_enabled_value = dictionary_member::<E>(cx, value, "depthWriteEnabled")?;
    let depth_compare_value = dictionary_member::<E>(cx, value, "depthCompare")?;
    let stencil_front_value = dictionary_member::<E>(cx, value, "stencilFront")?;
    let stencil_back_value = dictionary_member::<E>(cx, value, "stencilBack")?;
    let stencil_read_mask_value = dictionary_member::<E>(cx, value, "stencilReadMask")?;
    let stencil_write_mask_value = dictionary_member::<E>(cx, value, "stencilWriteMask")?;
    let depth_bias_value = dictionary_member::<E>(cx, value, "depthBias")?;
    let depth_bias_slope_scale_value = dictionary_member::<E>(cx, value, "depthBiasSlopeScale")?;
    let depth_bias_clamp_value = dictionary_member::<E>(cx, value, "depthBiasClamp")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "r8unorm" => WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
            "r8snorm" => WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
            "r8uint" => WGPUTextureFormat_WGPUTextureFormat_R8Uint,
            "r8sint" => WGPUTextureFormat_WGPUTextureFormat_R8Sint,
            "r16unorm" => WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
            "r16snorm" => WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
            "r16uint" => WGPUTextureFormat_WGPUTextureFormat_R16Uint,
            "r16sint" => WGPUTextureFormat_WGPUTextureFormat_R16Sint,
            "r16float" => WGPUTextureFormat_WGPUTextureFormat_R16Float,
            "rg8unorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
            "rg8snorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
            "rg8uint" => WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
            "rg8sint" => WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
            "r32uint" => WGPUTextureFormat_WGPUTextureFormat_R32Uint,
            "r32sint" => WGPUTextureFormat_WGPUTextureFormat_R32Sint,
            "r32float" => WGPUTextureFormat_WGPUTextureFormat_R32Float,
            "rg16unorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
            "rg16snorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
            "rg16uint" => WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
            "rg16sint" => WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
            "rg16float" => WGPUTextureFormat_WGPUTextureFormat_RG16Float,
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
            "rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb,
            "rgba8snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
            "rgba8uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
            "rgba8sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
            "bgra8unorm" => WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
            "bgra8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
            "rgb9e5ufloat" => WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat,
            "rgb10a2uint" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
            "rgb10a2unorm" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
            "rg11b10ufloat" => WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
            "rg32uint" => WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
            "rg32sint" => WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
            "rg32float" => WGPUTextureFormat_WGPUTextureFormat_RG32Float,
            "rgba16unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
            "rgba16snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
            "rgba16uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
            "rgba16sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
            "rgba16float" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
            "rgba32uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
            "rgba32sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
            "rgba32float" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
            "stencil8" => WGPUTextureFormat_WGPUTextureFormat_Stencil8,
            "depth16unorm" => WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm,
            "depth24plus" => WGPUTextureFormat_WGPUTextureFormat_Depth24Plus,
            "depth24plus-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
            "depth32float" => WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
            "depth32float-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8,
            "bc1-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm,
            "bc1-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb,
            "bc2-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm,
            "bc2-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb,
            "bc3-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm,
            "bc3-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb,
            "bc4-r-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm,
            "bc4-r-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm,
            "bc5-rg-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm,
            "bc5-rg-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm,
            "bc6h-rgb-ufloat" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat,
            "bc6h-rgb-float" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat,
            "bc7-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm,
            "bc7-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb,
            "etc2-rgb8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm,
            "etc2-rgb8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb,
            "etc2-rgb8a1unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm,
            "etc2-rgb8a1unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
            "etc2-rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm,
            "etc2-rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb,
            "eac-r11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm,
            "eac-r11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm,
            "eac-rg11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm,
            "eac-rg11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm,
            "astc-4x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm,
            "astc-4x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb,
            "astc-5x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm,
            "astc-5x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb,
            "astc-5x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm,
            "astc-5x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb,
            "astc-6x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm,
            "astc-6x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb,
            "astc-6x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm,
            "astc-6x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb,
            "astc-8x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm,
            "astc-8x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb,
            "astc-8x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm,
            "astc-8x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb,
            "astc-8x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm,
            "astc-8x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb,
            "astc-10x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm,
            "astc-10x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb,
            "astc-10x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm,
            "astc-10x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb,
            "astc-10x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm,
            "astc-10x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb,
            "astc-10x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm,
            "astc-10x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb,
            "astc-12x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm,
            "astc-12x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb,
            "astc-12x12-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm,
            "astc-12x12-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let depth_compare = if E::is_undefined(cx, depth_compare_value) {
        WGPUCompareFunction_WGPUCompareFunction_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, depth_compare_value, &enum_arena)? {
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
    let stencil_front = convert_stencil_face_state::<E>(cx, stencil_front_value)?;
    let stencil_back = convert_stencil_face_state::<E>(cx, stencil_back_value)?;
    Ok(WGPUDepthStencilState {
        nextInChain: ptr::null_mut(),
        format,
        // T5: an omitted optional boolean maps to WGPUOptionalBool_Undefined.
        depthWriteEnabled: if E::is_undefined(cx, depth_write_enabled_value) {
            WGPUOptionalBool_WGPUOptionalBool_Undefined
        } else if E::to_bool(cx, depth_write_enabled_value) {
            WGPUOptionalBool_WGPUOptionalBool_True
        } else {
            WGPUOptionalBool_WGPUOptionalBool_False
        },
        depthCompare: depth_compare,
        stencilFront: stencil_front,
        stencilBack: stencil_back,
        // R8: `[EnforceRange]` GPUStencilValue is checked at the 32-bit boundary.
        stencilReadMask: if E::is_undefined(cx, stencil_read_mask_value) {
            0xFFFFFFFF
        } else {
            enforce_u32::<E>(cx, stencil_read_mask_value, "stencilReadMask")?
        },
        // R8: `[EnforceRange]` GPUStencilValue is checked at the 32-bit boundary.
        stencilWriteMask: if E::is_undefined(cx, stencil_write_mask_value) {
            0xFFFFFFFF
        } else {
            enforce_u32::<E>(cx, stencil_write_mask_value, "stencilWriteMask")?
        },
        // T5: signed `[EnforceRange]` long is checked at the i32 boundary.
        depthBias: if E::is_undefined(cx, depth_bias_value) {
            0
        } else {
            enforce_i32::<E>(cx, depth_bias_value, "depthBias")?
        },
        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.
        depthBiasSlopeScale: if E::is_undefined(cx, depth_bias_slope_scale_value) {
            0_f32
        } else {
            restricted_f32::<E>(cx, depth_bias_slope_scale_value, "depthBiasSlopeScale")?
        },
        // G11: restricted WebIDL `float` rejects non-finite values before f32 conversion.
        depthBiasClamp: if E::is_undefined(cx, depth_bias_clamp_value) {
            0_f32
        } else {
            restricted_f32::<E>(cx, depth_bias_clamp_value, "depthBiasClamp")?
        },
    })
}

/// Converts a JavaScript `GPUMultisampleState` into `WGPUMultisampleState`.
pub(super) fn convert_multisample_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUMultisampleState, E::Error> {
    let count_value = dictionary_member::<E>(cx, value, "count")?;
    let mask_value = dictionary_member::<E>(cx, value, "mask")?;
    let alpha_to_coverage_enabled_value = dictionary_member::<E>(cx, value, "alphaToCoverageEnabled")?;
    Ok(WGPUMultisampleState {
        nextInChain: ptr::null_mut(),
        // R8: `[EnforceRange]` GPUSize32 is checked at the 32-bit boundary.
        count: if E::is_undefined(cx, count_value) {
            1
        } else {
            enforce_u32::<E>(cx, count_value, "count")?
        },
        // R8: `[EnforceRange]` GPUSampleMask is checked at the 32-bit boundary.
        mask: if E::is_undefined(cx, mask_value) {
            0xFFFFFFFF
        } else {
            enforce_u32::<E>(cx, mask_value, "mask")?
        },
        // R8: an optional boolean defaults to false and otherwise uses `ToBoolean`.
        alphaToCoverageEnabled: if E::is_undefined(cx, alpha_to_coverage_enabled_value) {
            0
        } else {
            u32::from(E::to_bool(cx, alpha_to_coverage_enabled_value))
        },
    })
}

/// Converts a JavaScript `GPUBlendComponent` into `WGPUBlendComponent`.
pub(super) fn convert_blend_component<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBlendComponent, E::Error> {
    let operation_value = dictionary_member::<E>(cx, value, "operation")?;
    let src_factor_value = dictionary_member::<E>(cx, value, "srcFactor")?;
    let dst_factor_value = dictionary_member::<E>(cx, value, "dstFactor")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let operation = if E::is_undefined(cx, operation_value) {
        WGPUBlendOperation_WGPUBlendOperation_Add
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, operation_value, &enum_arena)? {
            "add" => WGPUBlendOperation_WGPUBlendOperation_Add,
            "subtract" => WGPUBlendOperation_WGPUBlendOperation_Subtract,
            "reverse-subtract" => WGPUBlendOperation_WGPUBlendOperation_ReverseSubtract,
            "min" => WGPUBlendOperation_WGPUBlendOperation_Min,
            "max" => WGPUBlendOperation_WGPUBlendOperation_Max,
            _ => return Err(E::type_error(cx, "GPUBlendOperation")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let src_factor = if E::is_undefined(cx, src_factor_value) {
        WGPUBlendFactor_WGPUBlendFactor_One
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, src_factor_value, &enum_arena)? {
            "zero" => WGPUBlendFactor_WGPUBlendFactor_Zero,
            "one" => WGPUBlendFactor_WGPUBlendFactor_One,
            "src" => WGPUBlendFactor_WGPUBlendFactor_Src,
            "one-minus-src" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc,
            "src-alpha" => WGPUBlendFactor_WGPUBlendFactor_SrcAlpha,
            "one-minus-src-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrcAlpha,
            "dst" => WGPUBlendFactor_WGPUBlendFactor_Dst,
            "one-minus-dst" => WGPUBlendFactor_WGPUBlendFactor_OneMinusDst,
            "dst-alpha" => WGPUBlendFactor_WGPUBlendFactor_DstAlpha,
            "one-minus-dst-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusDstAlpha,
            "src-alpha-saturated" => WGPUBlendFactor_WGPUBlendFactor_SrcAlphaSaturated,
            "constant" => WGPUBlendFactor_WGPUBlendFactor_Constant,
            "one-minus-constant" => WGPUBlendFactor_WGPUBlendFactor_OneMinusConstant,
            "src1" => WGPUBlendFactor_WGPUBlendFactor_Src1,
            "one-minus-src1" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc1,
            "src1-alpha" => WGPUBlendFactor_WGPUBlendFactor_Src1Alpha,
            "one-minus-src1-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc1Alpha,
            _ => return Err(E::type_error(cx, "GPUBlendFactor")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let dst_factor = if E::is_undefined(cx, dst_factor_value) {
        WGPUBlendFactor_WGPUBlendFactor_Zero
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, dst_factor_value, &enum_arena)? {
            "zero" => WGPUBlendFactor_WGPUBlendFactor_Zero,
            "one" => WGPUBlendFactor_WGPUBlendFactor_One,
            "src" => WGPUBlendFactor_WGPUBlendFactor_Src,
            "one-minus-src" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc,
            "src-alpha" => WGPUBlendFactor_WGPUBlendFactor_SrcAlpha,
            "one-minus-src-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrcAlpha,
            "dst" => WGPUBlendFactor_WGPUBlendFactor_Dst,
            "one-minus-dst" => WGPUBlendFactor_WGPUBlendFactor_OneMinusDst,
            "dst-alpha" => WGPUBlendFactor_WGPUBlendFactor_DstAlpha,
            "one-minus-dst-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusDstAlpha,
            "src-alpha-saturated" => WGPUBlendFactor_WGPUBlendFactor_SrcAlphaSaturated,
            "constant" => WGPUBlendFactor_WGPUBlendFactor_Constant,
            "one-minus-constant" => WGPUBlendFactor_WGPUBlendFactor_OneMinusConstant,
            "src1" => WGPUBlendFactor_WGPUBlendFactor_Src1,
            "one-minus-src1" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc1,
            "src1-alpha" => WGPUBlendFactor_WGPUBlendFactor_Src1Alpha,
            "one-minus-src1-alpha" => WGPUBlendFactor_WGPUBlendFactor_OneMinusSrc1Alpha,
            _ => return Err(E::type_error(cx, "GPUBlendFactor")),
        }
    };
    Ok(WGPUBlendComponent {
        operation,
        srcFactor: src_factor,
        dstFactor: dst_factor,
    })
}

/// Converts a JavaScript `GPUBlendState` into `WGPUBlendState`.
pub(super) fn convert_blend_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBlendState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let color_value = required_member::<E>(cx, value, "color")?;
    let alpha_value = required_member::<E>(cx, value, "alpha")?;
    let color = convert_blend_component::<E>(cx, color_value)?;
    let alpha = convert_blend_component::<E>(cx, alpha_value)?;
    Ok(WGPUBlendState {
        color,
        alpha,
    })
}

/// Converts a JavaScript `GPUColorTargetState` into `WGPUColorTargetState`.
pub(super) fn convert_color_target_state<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUColorTargetState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let format_value = required_member::<E>(cx, value, "format")?;
    let blend_value = dictionary_member::<E>(cx, value, "blend")?;
    let write_mask_value = dictionary_member::<E>(cx, value, "writeMask")?;
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let format = {
        let enum_arena = Arena::new();
        match E::to_str(cx, format_value, &enum_arena)? {
            "r8unorm" => WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
            "r8snorm" => WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
            "r8uint" => WGPUTextureFormat_WGPUTextureFormat_R8Uint,
            "r8sint" => WGPUTextureFormat_WGPUTextureFormat_R8Sint,
            "r16unorm" => WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
            "r16snorm" => WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
            "r16uint" => WGPUTextureFormat_WGPUTextureFormat_R16Uint,
            "r16sint" => WGPUTextureFormat_WGPUTextureFormat_R16Sint,
            "r16float" => WGPUTextureFormat_WGPUTextureFormat_R16Float,
            "rg8unorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
            "rg8snorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
            "rg8uint" => WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
            "rg8sint" => WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
            "r32uint" => WGPUTextureFormat_WGPUTextureFormat_R32Uint,
            "r32sint" => WGPUTextureFormat_WGPUTextureFormat_R32Sint,
            "r32float" => WGPUTextureFormat_WGPUTextureFormat_R32Float,
            "rg16unorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
            "rg16snorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
            "rg16uint" => WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
            "rg16sint" => WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
            "rg16float" => WGPUTextureFormat_WGPUTextureFormat_RG16Float,
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
            "rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb,
            "rgba8snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
            "rgba8uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
            "rgba8sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
            "bgra8unorm" => WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
            "bgra8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
            "rgb9e5ufloat" => WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat,
            "rgb10a2uint" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
            "rgb10a2unorm" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
            "rg11b10ufloat" => WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
            "rg32uint" => WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
            "rg32sint" => WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
            "rg32float" => WGPUTextureFormat_WGPUTextureFormat_RG32Float,
            "rgba16unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
            "rgba16snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
            "rgba16uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
            "rgba16sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
            "rgba16float" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
            "rgba32uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
            "rgba32sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
            "rgba32float" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
            "stencil8" => WGPUTextureFormat_WGPUTextureFormat_Stencil8,
            "depth16unorm" => WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm,
            "depth24plus" => WGPUTextureFormat_WGPUTextureFormat_Depth24Plus,
            "depth24plus-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
            "depth32float" => WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
            "depth32float-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8,
            "bc1-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm,
            "bc1-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb,
            "bc2-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm,
            "bc2-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb,
            "bc3-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm,
            "bc3-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb,
            "bc4-r-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm,
            "bc4-r-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm,
            "bc5-rg-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm,
            "bc5-rg-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm,
            "bc6h-rgb-ufloat" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat,
            "bc6h-rgb-float" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat,
            "bc7-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm,
            "bc7-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb,
            "etc2-rgb8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm,
            "etc2-rgb8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb,
            "etc2-rgb8a1unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm,
            "etc2-rgb8a1unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
            "etc2-rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm,
            "etc2-rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb,
            "eac-r11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm,
            "eac-r11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm,
            "eac-rg11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm,
            "eac-rg11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm,
            "astc-4x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm,
            "astc-4x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb,
            "astc-5x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm,
            "astc-5x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb,
            "astc-5x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm,
            "astc-5x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb,
            "astc-6x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm,
            "astc-6x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb,
            "astc-6x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm,
            "astc-6x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb,
            "astc-8x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm,
            "astc-8x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb,
            "astc-8x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm,
            "astc-8x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb,
            "astc-8x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm,
            "astc-8x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb,
            "astc-10x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm,
            "astc-10x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb,
            "astc-10x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm,
            "astc-10x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb,
            "astc-10x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm,
            "astc-10x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb,
            "astc-10x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm,
            "astc-10x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb,
            "astc-12x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm,
            "astc-12x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb,
            "astc-12x12-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm,
            "astc-12x12-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    // T5: an absent optional dictionary is a null pointer in the pinned C ABI.
    let blend = if E::is_undefined(cx, blend_value) {
        ptr::null()
    } else {
        let converted = convert_blend_state::<E>(cx, blend_value)?;
        arena.alloc_slice(vec![converted]).as_ptr()
    };
    Ok(WGPUColorTargetState {
        nextInChain: ptr::null_mut(),
        format,
        blend,
        // R8/B7: the 32-bit WebIDL value is checked before C-ABI widening.
        writeMask: if E::is_undefined(cx, write_mask_value) {
            0xF
        } else {
            u64::from(enforce_u32::<E>(cx, write_mask_value, "writeMask")?)
        },
    })
}

/// Converts a JavaScript `GPUFragmentState` into `WGPUFragmentState`.
pub(super) fn convert_fragment_state<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUFragmentState, E::Error> {
    // DR-M3: required dictionary members reject undefined.
    let module_value = required_member::<E>(cx, value, "module")?;
    let entry_point_value = dictionary_member::<E>(cx, value, "entryPoint")?;
    let constants_value = dictionary_member::<E>(cx, value, "constants")?;
    // Policy skip: reject present unsupported API instead of ignoring it.
    if !E::is_undefined(cx, constants_value) {
        return Err(E::type_error(cx, "constants are not supported yet"));
    }
    let targets_value = required_member::<E>(cx, value, "targets")?;
    let module = shader_module_handle::<E>(cx, module_value)?;
    // B4: optional non-nullable strings preserve absence; present null is stringified.
    let entry_point = if E::is_undefined(cx, entry_point_value) {
        None
    } else {
        Some(E::to_str(cx, entry_point_value, arena)?)
    };
    let targets = {
        let converted = convert_sequence::<E, _>(cx, targets_value, "targets", |item| {
            // T5: nullable sequence elements are C sentinel-filled struct holes.
            if E::is_null(cx, item) {
                // SAFETY: the pinned C ABI defines the all-zero element as the hole sentinel.
                Ok(unsafe { std::mem::zeroed() })
            } else {
                convert_color_target_state::<E>(cx, item, arena)
            }
        })?;
        arena.alloc_slice(converted)
    };
    Ok(WGPUFragmentState {
        nextInChain: ptr::null_mut(),
        module,
        entryPoint: entry_point.map_or_else(
            || WGPUStringView { data: ptr::null(), length: wgpu_strlen() },
            |value| WGPUStringView::from_bytes(value.as_bytes()),
        ),
        // Policy skip: recorded deferral: pipeline constants are outside the block 09 slice-3 surface.
        constantCount: 0,
        constants: ptr::null(),
        targetCount: targets.len(),
        targets: if targets.is_empty() {
            ptr::null()
        } else {
            targets.as_ptr()
        },
    })
}

/// Converts a JavaScript `GPURenderPipelineDescriptor` into `ConvertedRenderPipelineDescriptor`.
pub(super) fn convert_render_pipeline_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedRenderPipelineDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    // DR-M3: required dictionary members reject undefined.
    let layout_value = required_member::<E>(cx, value, "layout")?;
    let vertex_value = required_member::<E>(cx, value, "vertex")?;
    let primitive_value = dictionary_member::<E>(cx, value, "primitive")?;
    let depth_stencil_value = dictionary_member::<E>(cx, value, "depthStencil")?;
    let multisample_value = dictionary_member::<E>(cx, value, "multisample")?;
    let fragment_value = dictionary_member::<E>(cx, value, "fragment")?;
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
    let vertex = convert_vertex_state::<E>(cx, vertex_value, arena)?;
    let primitive = convert_primitive_state::<E>(cx, primitive_value)?;
    // T5: an absent optional dictionary is a null pointer in the pinned C ABI.
    let depth_stencil = if E::is_undefined(cx, depth_stencil_value) {
        ptr::null()
    } else {
        let converted = convert_depth_stencil_state::<E>(cx, depth_stencil_value)?;
        arena.alloc_slice(vec![converted]).as_ptr()
    };
    let multisample = convert_multisample_state::<E>(cx, multisample_value)?;
    // T5: an absent optional dictionary is a null pointer in the pinned C ABI.
    let fragment = if E::is_undefined(cx, fragment_value) {
        ptr::null()
    } else {
        let converted = convert_fragment_state::<E>(cx, fragment_value, arena)?;
        arena.alloc_slice(vec![converted]).as_ptr()
    };
    let vertex_module = vertex.module;
    let fragment_module = if fragment.is_null() {
        ptr::null_mut()
    } else {
        // SAFETY: the arena-owned optional nested descriptor remains live through the native call.
        unsafe { (*fragment).module }
    };
    let native = WGPURenderPipelineDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        layout,
        vertex,
        primitive,
        depthStencil: depth_stencil,
        multisample,
        fragment,
    };
    Ok(ConvertedRenderPipelineDescriptor {
        native,
        vertex_module,
        fragment_module,
        layout,
    })
}

/// Converts a JavaScript `GPUCommandEncoderDescriptor` into `WGPUCommandEncoderDescriptor`.
pub(super) fn convert_command_encoder_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUCommandEncoderDescriptor, E::Error> {
    let label_value = dictionary_member::<E>(cx, value, "label")?;
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
    let label_value = dictionary_member::<E>(cx, value, "label")?;
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
    let label_value = dictionary_member::<E>(cx, value, "label")?;
    let timestamp_writes_value = dictionary_member::<E>(cx, value, "timestampWrites")?;
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
        WGPUTextureViewDimension_WGPUTextureViewDimension_2D
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, view_dimension_value, &enum_arena)? {
            "1d" => WGPUTextureViewDimension_WGPUTextureViewDimension_1D,
            "2d" => WGPUTextureViewDimension_WGPUTextureViewDimension_2D,
            "2d-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray,
            "cube" => WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
            "cube-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
            "3d" => WGPUTextureViewDimension_WGPUTextureViewDimension_3D,
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
            "r8unorm" => WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
            "r8snorm" => WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
            "r8uint" => WGPUTextureFormat_WGPUTextureFormat_R8Uint,
            "r8sint" => WGPUTextureFormat_WGPUTextureFormat_R8Sint,
            "r16unorm" => WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
            "r16snorm" => WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
            "r16uint" => WGPUTextureFormat_WGPUTextureFormat_R16Uint,
            "r16sint" => WGPUTextureFormat_WGPUTextureFormat_R16Sint,
            "r16float" => WGPUTextureFormat_WGPUTextureFormat_R16Float,
            "rg8unorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
            "rg8snorm" => WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
            "rg8uint" => WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
            "rg8sint" => WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
            "r32uint" => WGPUTextureFormat_WGPUTextureFormat_R32Uint,
            "r32sint" => WGPUTextureFormat_WGPUTextureFormat_R32Sint,
            "r32float" => WGPUTextureFormat_WGPUTextureFormat_R32Float,
            "rg16unorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
            "rg16snorm" => WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
            "rg16uint" => WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
            "rg16sint" => WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
            "rg16float" => WGPUTextureFormat_WGPUTextureFormat_RG16Float,
            "rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
            "rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb,
            "rgba8snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
            "rgba8uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
            "rgba8sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
            "bgra8unorm" => WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
            "bgra8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
            "rgb9e5ufloat" => WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat,
            "rgb10a2uint" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
            "rgb10a2unorm" => WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
            "rg11b10ufloat" => WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
            "rg32uint" => WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
            "rg32sint" => WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
            "rg32float" => WGPUTextureFormat_WGPUTextureFormat_RG32Float,
            "rgba16unorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
            "rgba16snorm" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
            "rgba16uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
            "rgba16sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
            "rgba16float" => WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
            "rgba32uint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
            "rgba32sint" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
            "rgba32float" => WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
            "stencil8" => WGPUTextureFormat_WGPUTextureFormat_Stencil8,
            "depth16unorm" => WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm,
            "depth24plus" => WGPUTextureFormat_WGPUTextureFormat_Depth24Plus,
            "depth24plus-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
            "depth32float" => WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
            "depth32float-stencil8" => WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8,
            "bc1-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm,
            "bc1-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb,
            "bc2-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm,
            "bc2-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb,
            "bc3-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm,
            "bc3-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb,
            "bc4-r-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm,
            "bc4-r-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm,
            "bc5-rg-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm,
            "bc5-rg-snorm" => WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm,
            "bc6h-rgb-ufloat" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat,
            "bc6h-rgb-float" => WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat,
            "bc7-rgba-unorm" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm,
            "bc7-rgba-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb,
            "etc2-rgb8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm,
            "etc2-rgb8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb,
            "etc2-rgb8a1unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm,
            "etc2-rgb8a1unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
            "etc2-rgba8unorm" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm,
            "etc2-rgba8unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb,
            "eac-r11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm,
            "eac-r11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm,
            "eac-rg11unorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm,
            "eac-rg11snorm" => WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm,
            "astc-4x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm,
            "astc-4x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb,
            "astc-5x4-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm,
            "astc-5x4-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb,
            "astc-5x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm,
            "astc-5x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb,
            "astc-6x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm,
            "astc-6x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb,
            "astc-6x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm,
            "astc-6x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb,
            "astc-8x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm,
            "astc-8x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb,
            "astc-8x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm,
            "astc-8x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb,
            "astc-8x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm,
            "astc-8x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb,
            "astc-10x5-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm,
            "astc-10x5-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb,
            "astc-10x6-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm,
            "astc-10x6-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb,
            "astc-10x8-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm,
            "astc-10x8-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb,
            "astc-10x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm,
            "astc-10x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb,
            "astc-12x10-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm,
            "astc-12x10-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb,
            "astc-12x12-unorm" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm,
            "astc-12x12-unorm-srgb" => WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb,
            _ => return Err(E::type_error(cx, "GPUTextureFormat")),
        }
    };
    // B6: string enums are joined to C values; absence uses the IDL default or C sentinel.
    let view_dimension = if E::is_undefined(cx, view_dimension_value) {
        WGPUTextureViewDimension_WGPUTextureViewDimension_2D
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, view_dimension_value, &enum_arena)? {
            "1d" => WGPUTextureViewDimension_WGPUTextureViewDimension_1D,
            "2d" => WGPUTextureViewDimension_WGPUTextureViewDimension_2D,
            "2d-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray,
            "cube" => WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
            "cube-array" => WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
            "3d" => WGPUTextureViewDimension_WGPUTextureViewDimension_3D,
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

/// Converts the dictionary-or-sequence `GPUExtent3D` typedef into `WGPUExtent3D`.
#[allow(dead_code)] // T1 policy selects both typedefs; some land before their API consumer.
pub(super) fn convert_gpu_extent3d<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUExtent3D, E::Error> {
    // T1: only an object can select the sequence or dictionary union arm.
    if !E::is_object(cx, value) { return Err(E::type_error(cx, "GPUExtent3D must be an object")); }
    // T1: an iterable object selects the sequence arm; otherwise dictionary conversion applies.
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
    // T1: only an object can select the sequence or dictionary union arm.
    if !E::is_object(cx, value) { return Err(E::type_error(cx, "GPUOrigin3D must be an object")); }
    // T1: an iterable object selects the sequence arm; otherwise dictionary conversion applies.
    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {
        return convert_origin3d_dict::<E>(cx, value);
    };
    let values = convert_sequence_from_method::<E, _>(cx, value, iterator_method, "GPUOrigin3D", |item| {
        enforce_u32::<E>(cx, item, "coordinate")
    })?;
    if values.len() > 3 {
        return Err(E::type_error(cx, "GPUOrigin3D sequence length must be 0..=3"));
    }
    Ok(WGPUOrigin3D {
        x: values.first().copied().unwrap_or(0),
        y: values.get(1).copied().unwrap_or(0),
        z: values.get(2).copied().unwrap_or(0),
    })
}

/// Converts the dictionary-or-sequence `GPUColor` typedef into `WGPUColor`.
#[allow(dead_code)] // T1 policy selects both typedefs; some land before their API consumer.
pub(super) fn convert_gpu_color<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUColor, E::Error> {
    // T1: only an object can select the sequence or dictionary union arm.
    if !E::is_object(cx, value) { return Err(E::type_error(cx, "GPUColor must be an object")); }
    // T1: an iterable object selects the sequence arm; otherwise dictionary conversion applies.
    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {
        return convert_color_dict::<E>(cx, value);
    };
    let values = convert_sequence_from_method::<E, _>(cx, value, iterator_method, "GPUColor", |item| {
        restricted_f64::<E>(cx, item, "color channel")
    })?;
    if values.len() < 4 || values.len() > 4 {
        return Err(E::type_error(cx, "GPUColor sequence length must be 4..=4"));
    }
    Ok(WGPUColor {
        r: values[0],
        g: values[1],
        b: values[2],
        a: values[3],
    })
}

pub(super) fn convert_gpu_error_filter<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUErrorFilter, E::Error> {
    // B6: generated WebIDL string-enum conversion rejects unknown values.
    let arena = Arena::new();
    match E::to_str(cx, value, &arena)? {
        "validation" => Ok(WGPUErrorFilter_WGPUErrorFilter_Validation),
        "out-of-memory" => Ok(WGPUErrorFilter_WGPUErrorFilter_OutOfMemory),
        "internal" => Ok(WGPUErrorFilter_WGPUErrorFilter_Internal),
        _ => Err(E::type_error(cx, "GPUErrorFilter")),
    }
}

pub(super) fn convert_gpu_index_format<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<WGPUIndexFormat, E::Error> {
    // B6: generated WebIDL string-enum conversion rejects unknown values.
    let arena = Arena::new();
    match E::to_str(cx, value, &arena)? {
        "uint16" => Ok(WGPUIndexFormat_WGPUIndexFormat_Uint16),
        "uint32" => Ok(WGPUIndexFormat_WGPUIndexFormat_Uint32),
        _ => Err(E::type_error(cx, "GPUIndexFormat")),
    }
}

/// Payload stored by a `GPUShaderModule` wrapper.
pub struct ShaderModulePayload {
    pub(super) module: WGPUShaderModule,
}

// SAFETY: `ShaderModulePayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for ShaderModulePayload {}

/// Payload stored by a `GPUSampler` wrapper.
pub struct SamplerPayload {
    pub(super) sampler: WGPUSampler,
    pub(super) label: Mutex<String>,
}

// SAFETY: `SamplerPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for SamplerPayload {}

/// Payload stored by a `GPUTexture` wrapper.
pub struct TexturePayload {
    pub(super) texture: WGPUTexture,
    pub(super) destroyed: AtomicBool,
}

// SAFETY: `TexturePayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for TexturePayload {}

/// Payload stored by a `GPUTextureView` wrapper.
pub struct TextureViewPayload {
    pub(super) texture_view: WGPUTextureView,
    pub(super) texture: WGPUTexture,
}

// SAFETY: `TextureViewPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for TextureViewPayload {}

/// Payload stored by a `GPUBindGroupLayout` wrapper.
pub struct BindGroupLayoutPayload {
    pub(super) layout: WGPUBindGroupLayout,
}

// SAFETY: `BindGroupLayoutPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for BindGroupLayoutPayload {}

/// Payload stored by a `GPUPipelineLayout` wrapper.
pub struct PipelineLayoutPayload {
    pub(super) layout: WGPUPipelineLayout,
}

// SAFETY: `PipelineLayoutPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for PipelineLayoutPayload {}

/// Payload stored by a `GPUBindGroup` wrapper.
pub struct BindGroupPayload {
    pub(super) bind_group: WGPUBindGroup,
    pub(super) layout: WGPUBindGroupLayout,
    pub(super) buffers: Vec<WGPUBuffer>,
    pub(super) samplers: Vec<WGPUSampler>,
    pub(super) texture_views: Vec<WGPUTextureView>,
}

// SAFETY: `BindGroupPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for BindGroupPayload {}

/// Payload stored by a `GPUComputePipeline` wrapper.
pub struct ComputePipelinePayload {
    pub(super) pipeline: WGPUComputePipeline,
    pub(super) module: WGPUShaderModule,
    pub(super) layout: WGPUPipelineLayout,
}

// SAFETY: `ComputePipelinePayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for ComputePipelinePayload {}

/// Payload stored by a `GPURenderPipeline` wrapper.
pub struct RenderPipelinePayload {
    pub(super) render_pipeline: WGPURenderPipeline,
    pub(super) vertex_module: WGPUShaderModule,
    pub(super) fragment_module: WGPUShaderModule,
    pub(super) layout: WGPUPipelineLayout,
}

// SAFETY: `RenderPipelinePayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for RenderPipelinePayload {}

/// Payload stored by a `GPUCommandEncoder` wrapper.
pub struct CommandEncoderPayload {
    pub(super) state: Arc<Mutex<CommandEncoderState>>,
}

// SAFETY: `CommandEncoderPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for CommandEncoderPayload {}

/// One release request enqueued by finalizers and drained by the host tick.
pub enum ReleaseRequest {
    /// Release an adapter.
    Adapter { /// Adapter handle.
        adapter: WGPUAdapter, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release an adopted device.
    Device { /// Device handle.
        device: WGPUDevice, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a buffer and its parent device reference.
    BufferWithDeviceRef { /// Buffer handle.
        buffer: WGPUBuffer, /// Parent device handle.
        device: WGPUDevice, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a standalone buffer reference.
    Buffer { /// Buffer handle.
        buffer: WGPUBuffer, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a queue.
    Queue { /// Queue handle.
        queue: WGPUQueue, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a `GPUShaderModule` and its retained descriptor handles.
    ShaderModule {
        /// Created native handle.
        module: WGPUShaderModule,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUSampler` and its retained descriptor handles.
    Sampler {
        /// Created native handle.
        sampler: WGPUSampler,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUTexture` and its retained descriptor handles.
    Texture {
        /// Created native handle.
        texture: WGPUTexture,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUTextureView` and its retained descriptor handles.
    TextureView {
        /// Created native handle.
        texture_view: WGPUTextureView,
        /// Retained descriptor handle or handles.
        texture: WGPUTexture,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUBindGroupLayout` and its retained descriptor handles.
    BindGroupLayout {
        /// Created native handle.
        layout: WGPUBindGroupLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUPipelineLayout` and its retained descriptor handles.
    PipelineLayout {
        /// Created native handle.
        layout: WGPUPipelineLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUBindGroup` and its retained descriptor handles.
    BindGroup {
        /// Created native handle.
        bind_group: WGPUBindGroup,
        /// Retained descriptor handle or handles.
        layout: WGPUBindGroupLayout,
        /// Retained descriptor handle or handles.
        buffers: Vec<WGPUBuffer>,
        /// Retained descriptor handle or handles.
        samplers: Vec<WGPUSampler>,
        /// Retained descriptor handle or handles.
        texture_views: Vec<WGPUTextureView>,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUComputePipeline` and its retained descriptor handles.
    ComputePipeline {
        /// Created native handle.
        pipeline: WGPUComputePipeline,
        /// Retained descriptor handle or handles.
        module: WGPUShaderModule,
        /// Retained descriptor handle or handles.
        layout: WGPUPipelineLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPURenderPipeline` and its retained descriptor handles.
    RenderPipeline {
        /// Created native handle.
        render_pipeline: WGPURenderPipeline,
        /// Retained descriptor handle or handles.
        vertex_module: WGPUShaderModule,
        /// Retained descriptor handle or handles.
        fragment_module: WGPUShaderModule,
        /// Retained descriptor handle or handles.
        layout: WGPUPipelineLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a `GPUCommandEncoder` and its retained descriptor handles.
    CommandEncoder {
        /// Created native handle.
        encoder: WGPUCommandEncoder,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a command buffer.
    CommandBuffer { /// Command-buffer handle.
        command_buffer: WGPUCommandBuffer, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a compute-pass encoder.
    ComputePassEncoder { /// Pass handle.
        pass: WGPUComputePassEncoder, /// Dispatch table.
        gpu: GpuDispatch },
    /// Release a render-pass encoder.
    RenderPassEncoder { /// Pass handle.
        pass: WGPURenderPassEncoder, /// Dispatch table.
        gpu: GpuDispatch },
}

// SAFETY: finalizers only move WGPU handle values into this queue; native
// handles are dereferenced only by `run()` on the creating `tick()` thread.
unsafe impl Send for ReleaseRequest {}

impl ReleaseRequest {
    pub(super) fn run(self) {
        match self {
            Self::Adapter { adapter, gpu } => unsafe { (gpu.adapter_release)(adapter) },
            Self::Device { device, gpu } => unsafe { (gpu.device_release)(device) },
            Self::BufferWithDeviceRef { buffer, device, gpu } => unsafe { (gpu.buffer_release)(buffer); (gpu.device_release)(device); },
            Self::Buffer { buffer, gpu } => unsafe { (gpu.buffer_release)(buffer) },
            Self::Queue { queue, gpu } => unsafe { (gpu.queue_release)(queue) },
            Self::ShaderModule { module, gpu } => unsafe {
                (gpu.shader_module_release)(module);
            },
            Self::Sampler { sampler, gpu } => unsafe {
                (gpu.sampler_release)(sampler);
            },
            Self::Texture { texture, gpu } => unsafe {
                (gpu.texture_release)(texture);
            },
            Self::TextureView { texture_view, texture, gpu } => unsafe {
                (gpu.texture_view_release)(texture_view);
                (gpu.texture_release)(texture);
            },
            Self::BindGroupLayout { layout, gpu } => unsafe {
                (gpu.bind_group_layout_release)(layout);
            },
            Self::PipelineLayout { layout, gpu } => unsafe {
                (gpu.pipeline_layout_release)(layout);
            },
            Self::BindGroup { bind_group, layout, buffers, samplers, texture_views, gpu } => unsafe {
                (gpu.bind_group_release)(bind_group);
                (gpu.bind_group_layout_release)(layout);
                for handle in buffers { (gpu.buffer_release)(handle); }
                for handle in samplers { (gpu.sampler_release)(handle); }
                for handle in texture_views { (gpu.texture_view_release)(handle); }
            },
            Self::ComputePipeline { pipeline, module, layout, gpu } => unsafe {
                (gpu.compute_pipeline_release)(pipeline);
                (gpu.shader_module_release)(module);
                if !layout.is_null() { (gpu.pipeline_layout_release)(layout); }
            },
            Self::RenderPipeline { render_pipeline, vertex_module, fragment_module, layout, gpu } => unsafe {
                (gpu.render_pipeline_release)(render_pipeline);
                (gpu.shader_module_release)(vertex_module);
                if !fragment_module.is_null() { (gpu.shader_module_release)(fragment_module); }
                if !layout.is_null() { (gpu.pipeline_layout_release)(layout); }
            },
            Self::CommandEncoder { encoder, gpu } => unsafe {
                (gpu.command_encoder_release)(encoder);
            },
            Self::CommandBuffer { command_buffer, gpu } => unsafe { (gpu.command_buffer_release)(command_buffer) },
            Self::ComputePassEncoder { pass, gpu } => unsafe { (gpu.compute_pass_encoder_release)(pass) },
            Self::RenderPassEncoder { pass, gpu } => unsafe { (gpu.render_pass_encoder_release)(pass) },
        }
    }
}

/// Implements `GPUDevice.createShaderModule`.
pub fn device_create_shader_module<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUShaderModuleDescriptor"))?;
    let native = convert_shader_module_descriptor::<E>(cx, descriptor, &arena)?;
    let module = unsafe { (E::environment(cx).gpu().device_create_shader_module)(device, ptr::from_ref(&native)) };
    if module.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateShaderModule returned null"));
    }
    if let Err(error) = E::register_class(cx, shader_module_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().shader_module_release)(module);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_SHADER_MODULE_CLASS, Box::new(ShaderModulePayload {
        module,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().shader_module_release)(module);
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUShaderModule` payload by enqueuing its release.
pub fn finalize_shader_module(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<ShaderModulePayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::ShaderModule {
        module: payload.module,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createSampler`.
pub fn device_create_sampler<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().unwrap_or_else(|| E::undefined(cx));
    let native = convert_sampler_descriptor::<E>(cx, descriptor, &arena)?;
    let label = unsafe { string_view_to_owned(native.label) };
    let sampler = unsafe { (E::environment(cx).gpu().device_create_sampler)(device, ptr::from_ref(&native)) };
    if sampler.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateSampler returned null"));
    }
    if let Err(error) = E::register_class(cx, sampler_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().sampler_release)(sampler);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_SAMPLER_CLASS, Box::new(SamplerPayload {
        sampler,
        label: Mutex::new(label),
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().sampler_release)(sampler);
            }
            Err(error)
        }
    }
}

/// Implements the `GPUSampler.label` getter.
pub fn sampler_label_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_SAMPLER_CLASS).and_then(|payload| payload.downcast_ref::<SamplerPayload>()).ok_or_else(|| E::type_error(cx, "GPUSampler.label called on an incompatible object"))?;
    let label = payload.label.lock().map_err(|_| E::operation_error(cx, "GPUSampler label is poisoned"))?;
    E::string(cx, &label)
}

/// Implements the `GPUSampler.label` setter.
pub fn sampler_label_set<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value, value: E::Value) -> Result<(), E::Error> {
    let arena = Arena::new();
    let new_label = E::to_str(cx, value, &arena)?;
    let payload = E::payload(cx, this, GPU_SAMPLER_CLASS).and_then(|payload| payload.downcast_ref::<SamplerPayload>()).ok_or_else(|| E::type_error(cx, "GPUSampler.label called on an incompatible object"))?;
    let mut label = payload.label.lock().map_err(|_| E::operation_error(cx, "GPUSampler label is poisoned"))?;
    unsafe { (E::environment(cx).gpu().sampler_set_label)(payload.sampler, WGPUStringView::from_bytes(new_label.as_bytes())); }
    new_label.clone_into(&mut label);
    Ok(())
}

/// Finalizes a `GPUSampler` payload by enqueuing its release.
pub fn finalize_sampler(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<SamplerPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::Sampler {
        sampler: payload.sampler,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createTexture`.
pub fn device_create_texture<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUTextureDescriptor"))?;
    let native = convert_texture_descriptor::<E>(cx, descriptor, &arena)?;
    let texture = unsafe { (E::environment(cx).gpu().device_create_texture)(device, ptr::from_ref(&native)) };
    if texture.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateTexture returned null"));
    }
    if let Err(error) = E::register_class(cx, texture_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().texture_release)(texture);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_TEXTURE_CLASS, Box::new(TexturePayload {
        texture,
        destroyed: AtomicBool::new(false),
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().texture_release)(texture);
            }
            Err(error)
        }
    }
}

/// Implements the readonly `GPUTexture.depthOrArrayLayers` getter through `wgpuTextureGetDepthOrArrayLayers`.
pub fn texture_depth_or_array_layers_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.depthOrArrayLayers called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_depth_or_array_layers)(payload.texture) };
    E::number(cx, native as f64)
}

/// Implements the readonly `GPUTexture.dimension` getter through `wgpuTextureGetDimension`.
pub fn texture_dimension_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.dimension called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_dimension)(payload.texture) };
    match native {
        value if value == WGPUTextureDimension_WGPUTextureDimension_1D => E::string(cx, "1d"),
        value if value == WGPUTextureDimension_WGPUTextureDimension_2D => E::string(cx, "2d"),
        value if value == WGPUTextureDimension_WGPUTextureDimension_3D => E::string(cx, "3d"),
        _ => Err(E::operation_error(cx, "wgpuTextureGetDimension returned an unknown GPUTextureDimension")),
    }
}

/// Implements the readonly `GPUTexture.format` getter through `wgpuTextureGetFormat`.
pub fn texture_format_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.format called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_format)(payload.texture) };
    match native {
        value if value == WGPUTextureFormat_WGPUTextureFormat_R8Unorm => E::string(cx, "r8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R8Snorm => E::string(cx, "r8snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R8Uint => E::string(cx, "r8uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R8Sint => E::string(cx, "r8sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R16Unorm => E::string(cx, "r16unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R16Snorm => E::string(cx, "r16snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R16Uint => E::string(cx, "r16uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R16Sint => E::string(cx, "r16sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R16Float => E::string(cx, "r16float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG8Unorm => E::string(cx, "rg8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG8Snorm => E::string(cx, "rg8snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG8Uint => E::string(cx, "rg8uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG8Sint => E::string(cx, "rg8sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R32Uint => E::string(cx, "r32uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R32Sint => E::string(cx, "r32sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_R32Float => E::string(cx, "r32float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG16Unorm => E::string(cx, "rg16unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG16Snorm => E::string(cx, "rg16snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG16Uint => E::string(cx, "rg16uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG16Sint => E::string(cx, "rg16sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG16Float => E::string(cx, "rg16float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm => E::string(cx, "rgba8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb => E::string(cx, "rgba8unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm => E::string(cx, "rgba8snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint => E::string(cx, "rgba8uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint => E::string(cx, "rgba8sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm => E::string(cx, "bgra8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb => E::string(cx, "bgra8unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGB9E5Ufloat => E::string(cx, "rgb9e5ufloat"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint => E::string(cx, "rgb10a2uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm => E::string(cx, "rgb10a2unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat => E::string(cx, "rg11b10ufloat"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG32Uint => E::string(cx, "rg32uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG32Sint => E::string(cx, "rg32sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RG32Float => E::string(cx, "rg32float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm => E::string(cx, "rgba16unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm => E::string(cx, "rgba16snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint => E::string(cx, "rgba16uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint => E::string(cx, "rgba16sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA16Float => E::string(cx, "rgba16float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint => E::string(cx, "rgba32uint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint => E::string(cx, "rgba32sint"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_RGBA32Float => E::string(cx, "rgba32float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Stencil8 => E::string(cx, "stencil8"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Depth16Unorm => E::string(cx, "depth16unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Depth24Plus => E::string(cx, "depth24plus"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8 => E::string(cx, "depth24plus-stencil8"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Depth32Float => E::string(cx, "depth32float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_Depth32FloatStencil8 => E::string(cx, "depth32float-stencil8"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnorm => E::string(cx, "bc1-rgba-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC1RGBAUnormSrgb => E::string(cx, "bc1-rgba-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnorm => E::string(cx, "bc2-rgba-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC2RGBAUnormSrgb => E::string(cx, "bc2-rgba-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnorm => E::string(cx, "bc3-rgba-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC3RGBAUnormSrgb => E::string(cx, "bc3-rgba-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC4RUnorm => E::string(cx, "bc4-r-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC4RSnorm => E::string(cx, "bc4-r-snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC5RGUnorm => E::string(cx, "bc5-rg-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC5RGSnorm => E::string(cx, "bc5-rg-snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC6HRGBUfloat => E::string(cx, "bc6h-rgb-ufloat"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC6HRGBFloat => E::string(cx, "bc6h-rgb-float"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnorm => E::string(cx, "bc7-rgba-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_BC7RGBAUnormSrgb => E::string(cx, "bc7-rgba-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8Unorm => E::string(cx, "etc2-rgb8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8UnormSrgb => E::string(cx, "etc2-rgb8unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1Unorm => E::string(cx, "etc2-rgb8a1unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGB8A1UnormSrgb => E::string(cx, "etc2-rgb8a1unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8Unorm => E::string(cx, "etc2-rgba8unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ETC2RGBA8UnormSrgb => E::string(cx, "etc2-rgba8unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_EACR11Unorm => E::string(cx, "eac-r11unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_EACR11Snorm => E::string(cx, "eac-r11snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_EACRG11Unorm => E::string(cx, "eac-rg11unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_EACRG11Snorm => E::string(cx, "eac-rg11snorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC4x4Unorm => E::string(cx, "astc-4x4-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC4x4UnormSrgb => E::string(cx, "astc-4x4-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC5x4Unorm => E::string(cx, "astc-5x4-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC5x4UnormSrgb => E::string(cx, "astc-5x4-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC5x5Unorm => E::string(cx, "astc-5x5-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC5x5UnormSrgb => E::string(cx, "astc-5x5-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC6x5Unorm => E::string(cx, "astc-6x5-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC6x5UnormSrgb => E::string(cx, "astc-6x5-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC6x6Unorm => E::string(cx, "astc-6x6-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC6x6UnormSrgb => E::string(cx, "astc-6x6-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x5Unorm => E::string(cx, "astc-8x5-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x5UnormSrgb => E::string(cx, "astc-8x5-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x6Unorm => E::string(cx, "astc-8x6-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x6UnormSrgb => E::string(cx, "astc-8x6-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x8Unorm => E::string(cx, "astc-8x8-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC8x8UnormSrgb => E::string(cx, "astc-8x8-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x5Unorm => E::string(cx, "astc-10x5-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x5UnormSrgb => E::string(cx, "astc-10x5-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x6Unorm => E::string(cx, "astc-10x6-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x6UnormSrgb => E::string(cx, "astc-10x6-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x8Unorm => E::string(cx, "astc-10x8-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x8UnormSrgb => E::string(cx, "astc-10x8-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x10Unorm => E::string(cx, "astc-10x10-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC10x10UnormSrgb => E::string(cx, "astc-10x10-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC12x10Unorm => E::string(cx, "astc-12x10-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC12x10UnormSrgb => E::string(cx, "astc-12x10-unorm-srgb"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC12x12Unorm => E::string(cx, "astc-12x12-unorm"),
        value if value == WGPUTextureFormat_WGPUTextureFormat_ASTC12x12UnormSrgb => E::string(cx, "astc-12x12-unorm-srgb"),
        _ => Err(E::operation_error(cx, "wgpuTextureGetFormat returned an unknown GPUTextureFormat")),
    }
}

/// Implements the readonly `GPUTexture.height` getter through `wgpuTextureGetHeight`.
pub fn texture_height_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.height called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_height)(payload.texture) };
    E::number(cx, native as f64)
}

/// Implements the readonly `GPUTexture.mipLevelCount` getter through `wgpuTextureGetMipLevelCount`.
pub fn texture_mip_level_count_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.mipLevelCount called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_mip_level_count)(payload.texture) };
    E::number(cx, native as f64)
}

/// Implements the readonly `GPUTexture.sampleCount` getter through `wgpuTextureGetSampleCount`.
pub fn texture_sample_count_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.sampleCount called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_sample_count)(payload.texture) };
    E::number(cx, native as f64)
}

/// Implements the readonly `GPUTexture.usage` getter through `wgpuTextureGetUsage`.
pub fn texture_usage_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.usage called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_usage)(payload.texture) };
    E::number(cx, native as f64)
}

/// Implements the readonly `GPUTexture.width` getter through `wgpuTextureGetWidth`.
pub fn texture_width_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS).and_then(|payload| payload.downcast_ref::<TexturePayload>()).ok_or_else(|| E::type_error(cx, "GPUTexture.width called on an incompatible object"))?;
    let native = unsafe { (E::environment(cx).gpu().texture_get_width)(payload.texture) };
    E::number(cx, native as f64)
}

/// Finalizes a `GPUTexture` payload by enqueuing its release.
pub fn finalize_texture(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<TexturePayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::Texture {
        texture: payload.texture,
        gpu: env.gpu(),
    });
}

/// Implements `GPUTexture.createView`.
pub fn texture_create_view<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let texture = texture_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().unwrap_or_else(|| E::undefined(cx));
    let native = convert_texture_view_descriptor::<E>(cx, descriptor, &arena)?;
    let texture_view = unsafe { (E::environment(cx).gpu().texture_create_view)(texture, ptr::from_ref(&native)) };
    if texture_view.is_null() {
        return Err(E::operation_error(cx, "wgpuTextureCreateView returned null"));
    }
    let gpu = E::environment(cx).gpu();
    unsafe {
        (gpu.texture_add_ref)(texture);
    }
    if let Err(error) = E::register_class(cx, texture_view_class::<E>()) {
        unsafe {
            (gpu.texture_view_release)(texture_view);
            (gpu.texture_release)(texture);
        }
        return Err(error);
    }
    let retained_texture = texture;
    match E::new_instance(cx, GPU_TEXTURE_VIEW_CLASS, Box::new(TextureViewPayload {
        texture_view,
        texture,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.texture_view_release)(texture_view);
                (gpu.texture_release)(retained_texture);
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUTextureView` payload by enqueuing its release.
pub fn finalize_texture_view(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<TextureViewPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::TextureView {
        texture_view: payload.texture_view,
        texture: payload.texture,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createBindGroupLayout`.
pub fn device_create_bind_group_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUBindGroupLayoutDescriptor"))?;
    let native = convert_bind_group_layout_descriptor::<E>(cx, descriptor, &arena)?;
    let layout = unsafe { (E::environment(cx).gpu().device_create_bind_group_layout)(device, ptr::from_ref(&native)) };
    if layout.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateBindGroupLayout returned null"));
    }
    if let Err(error) = E::register_class(cx, bind_group_layout_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().bind_group_layout_release)(layout);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_BIND_GROUP_LAYOUT_CLASS, Box::new(BindGroupLayoutPayload {
        layout,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().bind_group_layout_release)(layout);
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUBindGroupLayout` payload by enqueuing its release.
pub fn finalize_bind_group_layout(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BindGroupLayoutPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::BindGroupLayout {
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createPipelineLayout`.
pub fn device_create_pipeline_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUPipelineLayoutDescriptor"))?;
    let native = convert_pipeline_layout_descriptor::<E>(cx, descriptor, &arena)?;
    let layout = unsafe { (E::environment(cx).gpu().device_create_pipeline_layout)(device, ptr::from_ref(&native)) };
    if layout.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreatePipelineLayout returned null"));
    }
    if let Err(error) = E::register_class(cx, pipeline_layout_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().pipeline_layout_release)(layout);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_PIPELINE_LAYOUT_CLASS, Box::new(PipelineLayoutPayload {
        layout,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().pipeline_layout_release)(layout);
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUPipelineLayout` payload by enqueuing its release.
pub fn finalize_pipeline_layout(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<PipelineLayoutPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::PipelineLayout {
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createBindGroup`.
pub fn device_create_bind_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUBindGroupDescriptor"))?;
    let converted = convert_bind_group_descriptor::<E>(cx, descriptor, &arena)?;
    let bind_group = unsafe { (E::environment(cx).gpu().device_create_bind_group)(device, ptr::from_ref(&converted.native)) };
    if bind_group.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateBindGroup returned null"));
    }
    let gpu = E::environment(cx).gpu();
    unsafe {
        (gpu.bind_group_layout_add_ref)(converted.layout);
        for handle in &converted.buffers { (gpu.buffer_add_ref)(*handle); }
        for handle in &converted.samplers { (gpu.sampler_add_ref)(*handle); }
        for handle in &converted.texture_views { (gpu.texture_view_add_ref)(*handle); }
    }
    if let Err(error) = E::register_class(cx, bind_group_class::<E>()) {
        unsafe {
            (gpu.bind_group_release)(bind_group);
            (gpu.bind_group_layout_release)(converted.layout);
            for handle in &converted.buffers { (gpu.buffer_release)(*handle); }
            for handle in &converted.samplers { (gpu.sampler_release)(*handle); }
            for handle in &converted.texture_views { (gpu.texture_view_release)(*handle); }
        }
        return Err(error);
    }
    let retained_layout = converted.layout;
    let retained_buffers = converted.buffers.clone();
    let retained_samplers = converted.samplers.clone();
    let retained_texture_views = converted.texture_views.clone();
    match E::new_instance(cx, GPU_BIND_GROUP_CLASS, Box::new(BindGroupPayload {
        bind_group,
        layout: converted.layout,
        buffers: converted.buffers,
        samplers: converted.samplers,
        texture_views: converted.texture_views,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.bind_group_release)(bind_group);
                (gpu.bind_group_layout_release)(retained_layout);
                for handle in &retained_buffers { (gpu.buffer_release)(*handle); }
                for handle in &retained_samplers { (gpu.sampler_release)(*handle); }
                for handle in &retained_texture_views { (gpu.texture_view_release)(*handle); }
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUBindGroup` payload by enqueuing its release.
pub fn finalize_bind_group(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BindGroupPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::BindGroup {
        bind_group: payload.bind_group,
        layout: payload.layout,
        buffers: payload.buffers,
        samplers: payload.samplers,
        texture_views: payload.texture_views,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createComputePipeline`.
pub fn device_create_compute_pipeline<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPUComputePipelineDescriptor"))?;
    let converted = convert_compute_pipeline_descriptor::<E>(cx, descriptor, &arena)?;
    let pipeline = unsafe { (E::environment(cx).gpu().device_create_compute_pipeline)(device, ptr::from_ref(&converted.native)) };
    if pipeline.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateComputePipeline returned null"));
    }
    let gpu = E::environment(cx).gpu();
    unsafe {
        (gpu.shader_module_add_ref)(converted.module);
        if !converted.layout.is_null() { (gpu.pipeline_layout_add_ref)(converted.layout); }
    }
    if let Err(error) = E::register_class(cx, compute_pipeline_class::<E>()) {
        unsafe {
            (gpu.compute_pipeline_release)(pipeline);
            (gpu.shader_module_release)(converted.module);
            if !converted.layout.is_null() { (gpu.pipeline_layout_release)(converted.layout); }
        }
        return Err(error);
    }
    let retained_module = converted.module;
    let retained_layout = converted.layout;
    match E::new_instance(cx, GPU_COMPUTE_PIPELINE_CLASS, Box::new(ComputePipelinePayload {
        pipeline,
        module: converted.module,
        layout: converted.layout,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.compute_pipeline_release)(pipeline);
                (gpu.shader_module_release)(retained_module);
                if !retained_layout.is_null() { (gpu.pipeline_layout_release)(retained_layout); }
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUComputePipeline` payload by enqueuing its release.
pub fn finalize_compute_pipeline(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<ComputePipelinePayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::ComputePipeline {
        pipeline: payload.pipeline,
        module: payload.module,
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createRenderPipeline`.
pub fn device_create_render_pipeline<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().ok_or_else(|| E::type_error(cx, "GPURenderPipelineDescriptor"))?;
    let converted = convert_render_pipeline_descriptor::<E>(cx, descriptor, &arena)?;
    let render_pipeline = unsafe { (E::environment(cx).gpu().device_create_render_pipeline)(device, ptr::from_ref(&converted.native)) };
    if render_pipeline.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateRenderPipeline returned null"));
    }
    let gpu = E::environment(cx).gpu();
    unsafe {
        (gpu.shader_module_add_ref)(converted.vertex_module);
        if !converted.fragment_module.is_null() { (gpu.shader_module_add_ref)(converted.fragment_module); }
        if !converted.layout.is_null() { (gpu.pipeline_layout_add_ref)(converted.layout); }
    }
    if let Err(error) = E::register_class(cx, render_pipeline_class::<E>()) {
        unsafe {
            (gpu.render_pipeline_release)(render_pipeline);
            (gpu.shader_module_release)(converted.vertex_module);
            if !converted.fragment_module.is_null() { (gpu.shader_module_release)(converted.fragment_module); }
            if !converted.layout.is_null() { (gpu.pipeline_layout_release)(converted.layout); }
        }
        return Err(error);
    }
    let retained_vertex_module = converted.vertex_module;
    let retained_fragment_module = converted.fragment_module;
    let retained_layout = converted.layout;
    match E::new_instance(cx, GPU_RENDER_PIPELINE_CLASS, Box::new(RenderPipelinePayload {
        render_pipeline,
        vertex_module: converted.vertex_module,
        fragment_module: converted.fragment_module,
        layout: converted.layout,
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.render_pipeline_release)(render_pipeline);
                (gpu.shader_module_release)(retained_vertex_module);
                if !retained_fragment_module.is_null() { (gpu.shader_module_release)(retained_fragment_module); }
                if !retained_layout.is_null() { (gpu.pipeline_layout_release)(retained_layout); }
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPURenderPipeline` payload by enqueuing its release.
pub fn finalize_render_pipeline(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<RenderPipelinePayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::RenderPipeline {
        render_pipeline: payload.render_pipeline,
        vertex_module: payload.vertex_module,
        fragment_module: payload.fragment_module,
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Implements `GPUDevice.createCommandEncoder`.
pub fn device_create_command_encoder<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => Some(convert_command_encoder_descriptor::<E>(cx, value, &arena)?),
        _ => None,
    };
    let encoder = unsafe { (E::environment(cx).gpu().device_create_command_encoder)(device, native.as_ref().map_or(ptr::null(), ptr::from_ref)) };
    if encoder.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateCommandEncoder returned null"));
    }
    if let Err(error) = E::register_class(cx, command_encoder_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().command_encoder_release)(encoder);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_COMMAND_ENCODER_CLASS, Box::new(CommandEncoderPayload {
        state: Arc::new(Mutex::new(CommandEncoderState {
            encoder,
            ended: false,
        })),
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().command_encoder_release)(encoder);
            }
            Err(error)
        }
    }
}

/// Finalizes a `GPUCommandEncoder` payload by enqueuing its release.
pub fn finalize_command_encoder(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<CommandEncoderPayload>() else { return; };
    let Ok(state) = payload.state.lock() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::CommandEncoder { encoder: state.encoder, gpu: env.gpu() });
}

pub(super) fn gpu_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_CLASS, || ClassSpec {
        name: "GPU",
        id: GPU_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "requestAdapter", length: 0, call: gpu_request_adapter::<E> },
        ])),
        finalizer: |_payload, _env| {},
    })
}

pub(super) fn adapter_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_ADAPTER_CLASS, || ClassSpec {
        name: "GPUAdapter",
        id: GPU_ADAPTER_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "requestDevice", length: 0, call: adapter_request_device::<E> },
        ])),
        finalizer: finalize_adapter,
    })
}

pub(super) fn device_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_DEVICE_CLASS, || ClassSpec {
        name: "GPUDevice",
        id: GPU_DEVICE_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec { name: "queue", get: Some(device_queue_get::<E>), set: None },
            PropertySpec { name: "lost", get: Some(device_lost_get::<E>), set: None },
            PropertySpec { name: "onuncapturederror", get: Some(device_on_uncaptured_error_get::<E>), set: Some(device_on_uncaptured_error_set::<E>) },
        ])),
        methods: Box::leak(Box::new([
            MethodSpec { name: "createBuffer", length: 1, call: device_create_buffer::<E> },
            MethodSpec { name: "pushErrorScope", length: 1, call: device_push_error_scope::<E> },
            MethodSpec { name: "popErrorScope", length: 0, call: device_pop_error_scope::<E> },
            MethodSpec { name: "createShaderModule", length: 1, call: device_create_shader_module::<E> },
            MethodSpec { name: "createSampler", length: 0, call: device_create_sampler::<E> },
            MethodSpec { name: "createTexture", length: 1, call: device_create_texture::<E> },
            MethodSpec { name: "createBindGroupLayout", length: 1, call: device_create_bind_group_layout::<E> },
            MethodSpec { name: "createPipelineLayout", length: 1, call: device_create_pipeline_layout::<E> },
            MethodSpec { name: "createBindGroup", length: 1, call: device_create_bind_group::<E> },
            MethodSpec { name: "createComputePipeline", length: 1, call: device_create_compute_pipeline::<E> },
            MethodSpec { name: "createRenderPipeline", length: 1, call: device_create_render_pipeline::<E> },
            MethodSpec { name: "createCommandEncoder", length: 0, call: device_create_command_encoder::<E> },
        ])),
        finalizer: finalize_device::<E>,
    })
}

pub(super) fn buffer_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BUFFER_CLASS, || ClassSpec {
        name: "GPUBuffer",
        id: GPU_BUFFER_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec { name: "label", get: Some(buffer_label_get::<E>), set: Some(buffer_label_set::<E>) },
            PropertySpec { name: "size", get: Some(buffer_size_get::<E>), set: None },
            PropertySpec { name: "usage", get: Some(buffer_usage_get::<E>), set: None },
        ])),
        methods: Box::leak(Box::new([
            MethodSpec { name: "destroy", length: 0, call: buffer_destroy::<E> },
            MethodSpec { name: "mapAsync", length: 1, call: buffer_map_async::<E> },
            MethodSpec { name: "getMappedRange", length: 0, call: buffer_get_mapped_range::<E> },
            MethodSpec { name: "unmap", length: 0, call: buffer_unmap::<E> },
        ])),
        finalizer: finalize_buffer::<E>,
    })
}

pub(super) fn texture_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_TEXTURE_CLASS, || ClassSpec {
        name: "GPUTexture",
        id: GPU_TEXTURE_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec { name: "width", get: Some(texture_width_get::<E>), set: None },
            PropertySpec { name: "height", get: Some(texture_height_get::<E>), set: None },
            PropertySpec { name: "depthOrArrayLayers", get: Some(texture_depth_or_array_layers_get::<E>), set: None },
            PropertySpec { name: "mipLevelCount", get: Some(texture_mip_level_count_get::<E>), set: None },
            PropertySpec { name: "sampleCount", get: Some(texture_sample_count_get::<E>), set: None },
            PropertySpec { name: "dimension", get: Some(texture_dimension_get::<E>), set: None },
            PropertySpec { name: "format", get: Some(texture_format_get::<E>), set: None },
            PropertySpec { name: "usage", get: Some(texture_usage_get::<E>), set: None },
        ])),
        methods: Box::leak(Box::new([
            MethodSpec { name: "destroy", length: 0, call: texture_destroy::<E> },
            MethodSpec { name: "createView", length: 0, call: texture_create_view::<E> },
        ])),
        finalizer: finalize_texture,
    })
}

pub(super) fn texture_view_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_TEXTURE_VIEW_CLASS, || ClassSpec {
        name: "GPUTextureView",
        id: GPU_TEXTURE_VIEW_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_texture_view,
    })
}

pub(super) fn queue_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_QUEUE_CLASS, || ClassSpec {
        name: "GPUQueue",
        id: GPU_QUEUE_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "writeBuffer", length: 3, call: queue_write_buffer::<E> },
            MethodSpec { name: "writeTexture", length: 4, call: queue_write_texture::<E> },
            MethodSpec { name: "submit", length: 1, call: queue_submit::<E> },
            MethodSpec { name: "onSubmittedWorkDone", length: 0, call: queue_on_submitted_work_done::<E> },
        ])),
        finalizer: finalize_queue,
    })
}

pub(super) fn shader_module_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_SHADER_MODULE_CLASS, || ClassSpec {
        name: "GPUShaderModule",
        id: GPU_SHADER_MODULE_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_shader_module,
    })
}

pub(super) fn sampler_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_SAMPLER_CLASS, || ClassSpec {
        name: "GPUSampler",
        id: GPU_SAMPLER_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec { name: "label", get: Some(sampler_label_get::<E>), set: Some(sampler_label_set::<E>) },
        ])),
        methods: &[],
        finalizer: finalize_sampler,
    })
}

pub(super) fn bind_group_layout_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BIND_GROUP_LAYOUT_CLASS, || ClassSpec {
        name: "GPUBindGroupLayout",
        id: GPU_BIND_GROUP_LAYOUT_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_bind_group_layout,
    })
}

pub(super) fn pipeline_layout_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_PIPELINE_LAYOUT_CLASS, || ClassSpec {
        name: "GPUPipelineLayout",
        id: GPU_PIPELINE_LAYOUT_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_pipeline_layout,
    })
}

pub(super) fn bind_group_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BIND_GROUP_CLASS, || ClassSpec {
        name: "GPUBindGroup",
        id: GPU_BIND_GROUP_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_bind_group,
    })
}

pub(super) fn compute_pipeline_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMPUTE_PIPELINE_CLASS, || ClassSpec {
        name: "GPUComputePipeline",
        id: GPU_COMPUTE_PIPELINE_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_compute_pipeline,
    })
}

pub(super) fn render_pipeline_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_RENDER_PIPELINE_CLASS, || ClassSpec {
        name: "GPURenderPipeline",
        id: GPU_RENDER_PIPELINE_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_render_pipeline,
    })
}

pub(super) fn command_encoder_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMMAND_ENCODER_CLASS, || ClassSpec {
        name: "GPUCommandEncoder",
        id: GPU_COMMAND_ENCODER_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "copyBufferToBuffer", length: 5, call: command_encoder_copy_buffer_to_buffer::<E> },
            MethodSpec { name: "beginComputePass", length: 0, call: command_encoder_begin_compute_pass::<E> },
            MethodSpec { name: "beginRenderPass", length: 1, call: command_encoder_begin_render_pass::<E> },
            MethodSpec { name: "copyBufferToTexture", length: 3, call: command_encoder_copy_buffer_to_texture::<E> },
            MethodSpec { name: "copyTextureToBuffer", length: 3, call: command_encoder_copy_texture_to_buffer::<E> },
            MethodSpec { name: "copyTextureToTexture", length: 3, call: command_encoder_copy_texture_to_texture::<E> },
            MethodSpec { name: "finish", length: 0, call: command_encoder_finish::<E> },
        ])),
        finalizer: finalize_command_encoder,
    })
}

pub(super) fn compute_pass_encoder_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMPUTE_PASS_ENCODER_CLASS, || ClassSpec {
        name: "GPUComputePassEncoder",
        id: GPU_COMPUTE_PASS_ENCODER_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "setPipeline", length: 1, call: compute_pass_set_pipeline::<E> },
            MethodSpec { name: "setBindGroup", length: 2, call: compute_pass_set_bind_group::<E> },
            MethodSpec { name: "dispatchWorkgroups", length: 1, call: compute_pass_dispatch_workgroups::<E> },
            MethodSpec { name: "end", length: 0, call: compute_pass_end::<E> },
        ])),
        finalizer: finalize_compute_pass_encoder,
    })
}

pub(super) fn render_pass_encoder_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_RENDER_PASS_ENCODER_CLASS, || ClassSpec {
        name: "GPURenderPassEncoder",
        id: GPU_RENDER_PASS_ENCODER_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "setPipeline", length: 1, call: render_pass_set_pipeline::<E> },
            MethodSpec { name: "setVertexBuffer", length: 2, call: render_pass_set_vertex_buffer::<E> },
            MethodSpec { name: "setIndexBuffer", length: 2, call: render_pass_set_index_buffer::<E> },
            MethodSpec { name: "setBindGroup", length: 2, call: render_pass_set_bind_group::<E> },
            MethodSpec { name: "draw", length: 1, call: render_pass_draw::<E> },
            MethodSpec { name: "drawIndexed", length: 1, call: render_pass_draw_indexed::<E> },
            MethodSpec { name: "setViewport", length: 6, call: render_pass_set_viewport::<E> },
            MethodSpec { name: "setScissorRect", length: 4, call: render_pass_set_scissor_rect::<E> },
            MethodSpec { name: "end", length: 0, call: render_pass_end::<E> },
        ])),
        finalizer: finalize_render_pass_encoder,
    })
}

pub(super) fn command_buffer_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMMAND_BUFFER_CLASS, || ClassSpec {
        name: "GPUCommandBuffer",
        id: GPU_COMMAND_BUFFER_CLASS,
        constructor: None,
        properties: &[],
        methods: &[],
        finalizer: finalize_command_buffer,
    })
}
