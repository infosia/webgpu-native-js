#![warn(missing_docs)]

//! Engine-independent WebGPU binding core.
//!
//! Descriptor conversion and wrapper behavior live here and are generic over
//! [`JsEngine`]. Engine adapters provide object allocation and JavaScript value
//! conversion only.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, OnceLock};

pub use webgpu_native_js_ffi::native::*;

/// Result type used by the core crate.
pub type Result<T, E> = std::result::Result<T, E>;

const GPU_BUFFER_CLASS: ClassId = ClassId(1);
const GPU_DEVICE_CLASS: ClassId = ClassId(2);
const GPU_CLASS: ClassId = ClassId(3);
const GPU_ADAPTER_CLASS: ClassId = ClassId(4);
const GPU_QUEUE_CLASS: ClassId = ClassId(5);
const GPU_SHADER_MODULE_CLASS: ClassId = ClassId(6);
const GPU_BIND_GROUP_LAYOUT_CLASS: ClassId = ClassId(7);
const GPU_PIPELINE_LAYOUT_CLASS: ClassId = ClassId(8);
const GPU_BIND_GROUP_CLASS: ClassId = ClassId(9);
const GPU_COMPUTE_PIPELINE_CLASS: ClassId = ClassId(10);
const GPU_COMMAND_ENCODER_CLASS: ClassId = ClassId(11);
const GPU_COMMAND_BUFFER_CLASS: ClassId = ClassId(12);
const GPU_COMPUTE_PASS_ENCODER_CLASS: ClassId = ClassId(13);
const WEBIDL_U32_MAX: u64 = u32::MAX as u64;

/// A JavaScript class identifier scoped to an engine context.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClassId(pub u32);

/// Returns the `WGPUStringView` sentinel length as stored in the generated ABI.
#[must_use]
pub const fn wgpu_strlen() -> usize {
    webgpu_native_js_ffi::native::WGPU_STRLEN as usize
}

/// Convenience helpers for the generated `WGPUStringView` ABI type.
pub trait WGPUStringViewExt {
    /// Returns a non-null input string view over the provided bytes.
    #[must_use]
    fn from_bytes(bytes: &[u8]) -> Self;

    /// Returns whether this view is a valid string view shape.
    #[must_use]
    fn is_valid(&self) -> bool;
}

impl WGPUStringViewExt for WGPUStringView {
    fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.is_empty() {
            Self {
                data: ptr::NonNull::<std::ffi::c_char>::dangling().as_ptr(),
                length: 0,
            }
        } else {
            Self {
                data: bytes.as_ptr().cast(),
                length: bytes.len(),
            }
        }
    }

    fn is_valid(&self) -> bool {
        !self.data.is_null() || self.length == wgpu_strlen()
    }
}

/// Function-pointer dispatch for the WebGPU C ABI calls used by this slice.
#[derive(Clone, Copy)]
pub struct GpuDispatch {
    /// `wgpuInstanceProcessEvents`.
    pub instance_process_events: unsafe fn(webgpu_native_js_ffi::native::WGPUInstance),
    /// `wgpuInstanceRequestAdapter`.
    pub instance_request_adapter: unsafe fn(
        webgpu_native_js_ffi::native::WGPUInstance,
        *const webgpu_native_js_ffi::native::WGPURequestAdapterOptions,
        WGPURequestAdapterCallbackInfo,
    ) -> webgpu_native_js_ffi::native::WGPUFuture,
    /// `wgpuAdapterRequestDevice`.
    pub adapter_request_device: unsafe fn(
        WGPUAdapter,
        *const webgpu_native_js_ffi::native::WGPUDeviceDescriptor,
        WGPURequestDeviceCallbackInfo,
    ) -> webgpu_native_js_ffi::native::WGPUFuture,
    /// `wgpuAdapterRelease`.
    pub adapter_release: unsafe fn(WGPUAdapter),
    /// `wgpuDeviceAddRef`.
    pub device_add_ref: unsafe fn(WGPUDevice),
    /// `wgpuDeviceRelease`.
    pub device_release: unsafe fn(WGPUDevice),
    /// `wgpuDeviceCreateBuffer`.
    pub device_create_buffer: unsafe fn(WGPUDevice, *const WGPUBufferDescriptor) -> WGPUBuffer,
    /// `wgpuDeviceGetQueue`.
    pub device_get_queue: unsafe fn(WGPUDevice) -> WGPUQueue,
    /// `wgpuDeviceCreateShaderModule`.
    pub device_create_shader_module:
        unsafe fn(WGPUDevice, *const WGPUShaderModuleDescriptor) -> WGPUShaderModule,
    /// `wgpuDeviceCreateBindGroupLayout`.
    pub device_create_bind_group_layout:
        unsafe fn(WGPUDevice, *const WGPUBindGroupLayoutDescriptor) -> WGPUBindGroupLayout,
    /// `wgpuDeviceCreatePipelineLayout`.
    pub device_create_pipeline_layout:
        unsafe fn(WGPUDevice, *const WGPUPipelineLayoutDescriptor) -> WGPUPipelineLayout,
    /// `wgpuDeviceCreateBindGroup`.
    pub device_create_bind_group:
        unsafe fn(WGPUDevice, *const WGPUBindGroupDescriptor) -> WGPUBindGroup,
    /// `wgpuDeviceCreateComputePipeline`.
    pub device_create_compute_pipeline:
        unsafe fn(WGPUDevice, *const WGPUComputePipelineDescriptor) -> WGPUComputePipeline,
    /// `wgpuDeviceCreateCommandEncoder`.
    pub device_create_command_encoder:
        unsafe fn(WGPUDevice, *const WGPUCommandEncoderDescriptor) -> WGPUCommandEncoder,
    /// `wgpuBufferSetLabel`.
    pub buffer_set_label: unsafe fn(WGPUBuffer, WGPUStringView),
    /// `wgpuBufferDestroy`.
    pub buffer_destroy: unsafe fn(WGPUBuffer),
    /// `wgpuBufferGetMappedRange`.
    pub buffer_get_mapped_range: unsafe fn(WGPUBuffer, usize, usize) -> *mut c_void,
    /// `wgpuBufferGetConstMappedRange`.
    pub buffer_get_const_mapped_range: unsafe fn(WGPUBuffer, usize, usize) -> *const c_void,
    /// `wgpuBufferAddRef`.
    pub buffer_add_ref: unsafe fn(WGPUBuffer),
    /// `wgpuBufferMapAsync`.
    pub buffer_map_async: unsafe fn(
        WGPUBuffer,
        WGPUMapMode,
        usize,
        usize,
        WGPUBufferMapCallbackInfo,
    ) -> webgpu_native_js_ffi::native::WGPUFuture,
    /// `wgpuBufferUnmap`.
    pub buffer_unmap: unsafe fn(WGPUBuffer),
    /// `wgpuBufferRelease`.
    pub buffer_release: unsafe fn(WGPUBuffer),
    /// `wgpuQueueAddRef`.
    pub queue_add_ref: unsafe fn(WGPUQueue),
    /// `wgpuQueueRelease`.
    pub queue_release: unsafe fn(WGPUQueue),
    /// `wgpuQueueWriteBuffer`.
    pub queue_write_buffer: unsafe fn(WGPUQueue, WGPUBuffer, u64, *const c_void, usize),
    /// `wgpuQueueSubmit`.
    pub queue_submit: unsafe fn(WGPUQueue, usize, *const WGPUCommandBuffer),
    /// `wgpuQueueOnSubmittedWorkDone`.
    pub queue_on_submitted_work_done:
        unsafe fn(WGPUQueue, WGPUQueueWorkDoneCallbackInfo) -> WGPUFuture,
    /// `wgpuShaderModuleAddRef`.
    pub shader_module_add_ref: unsafe fn(WGPUShaderModule),
    /// `wgpuShaderModuleRelease`.
    pub shader_module_release: unsafe fn(WGPUShaderModule),
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
    /// `wgpuCommandEncoderCopyBufferToBuffer`.
    pub command_encoder_copy_buffer_to_buffer:
        unsafe fn(WGPUCommandEncoder, WGPUBuffer, u64, WGPUBuffer, u64, u64),
    /// `wgpuCommandEncoderBeginComputePass`.
    pub command_encoder_begin_compute_pass:
        unsafe fn(WGPUCommandEncoder, *const WGPUComputePassDescriptor) -> WGPUComputePassEncoder,
    /// `wgpuCommandEncoderFinish`.
    pub command_encoder_finish:
        unsafe fn(WGPUCommandEncoder, *const WGPUCommandBufferDescriptor) -> WGPUCommandBuffer,
    /// `wgpuCommandBufferRelease`.
    pub command_buffer_release: unsafe fn(WGPUCommandBuffer),
    /// `wgpuComputePassEncoderRelease`.
    pub compute_pass_encoder_release: unsafe fn(WGPUComputePassEncoder),
    /// `wgpuComputePassEncoderSetPipeline`.
    pub compute_pass_encoder_set_pipeline: unsafe fn(WGPUComputePassEncoder, WGPUComputePipeline),
    /// `wgpuComputePassEncoderSetBindGroup`.
    pub compute_pass_encoder_set_bind_group:
        unsafe fn(WGPUComputePassEncoder, u32, WGPUBindGroup, usize, *const u32),
    /// `wgpuComputePassEncoderDispatchWorkgroups`.
    pub compute_pass_encoder_dispatch_workgroups: unsafe fn(WGPUComputePassEncoder, u32, u32, u32),
    /// `wgpuComputePassEncoderEnd`.
    pub compute_pass_encoder_end: unsafe fn(WGPUComputePassEncoder),
}

/// A per-context environment shared by wrapper callbacks.
pub struct Environment {
    gpu: GpuDispatch,
    queue: Arc<ReleaseQueue>,
    settlements: Arc<SettlementQueue>,
}

impl Environment {
    /// Creates an environment from WebGPU dispatch functions and a release queue.
    #[must_use]
    pub fn new(gpu: GpuDispatch, queue: Arc<ReleaseQueue>) -> Self {
        Self {
            gpu,
            queue,
            settlements: Arc::new(SettlementQueue::new()),
        }
    }

    /// Returns the WebGPU dispatch table.
    #[must_use]
    pub fn gpu(&self) -> GpuDispatch {
        self.gpu
    }

    /// Returns the release queue.
    #[must_use]
    pub fn queue(&self) -> &Arc<ReleaseQueue> {
        &self.queue
    }

    /// Returns the async settlement queue.
    #[must_use]
    pub fn settlements(&self) -> &Arc<SettlementQueue> {
        &self.settlements
    }
}

/// Per-call bump-style arena for transient conversion data.
#[derive(Default)]
pub struct Arena {
    allocations: RefCell<Vec<Box<dyn Any>>>,
}

impl Arena {
    /// Creates an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Copies a string into the arena and returns the arena-owned bytes.
    pub fn alloc_str(&self, value: &str) -> &str {
        let bytes = self.alloc_slice(value.as_bytes().to_vec());
        // SAFETY: the bytes are copied from a valid `str` and stored in `self`.
        // The returned borrow is tied to the arena lifetime.
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    /// Copies a slice allocation into address-stable arena storage.
    pub fn alloc_slice<T: Copy + 'static>(&self, value: Vec<T>) -> &[T] {
        let values = value.into_boxed_slice();
        let ptr = values.as_ptr();
        let len = values.len();
        self.allocations.borrow_mut().push(Box::new(values));
        // SAFETY: the boxed slice allocation is owned by `self.allocations`.
        // Moving its owning Box does not move the slice, and the returned borrow
        // is tied to the arena lifetime.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

/// JavaScript engine boundary used by descriptor conversion and wrapper logic.
pub trait JsEngine: Sized {
    /// JavaScript value representation for this engine.
    type Value: Copy;
    /// JavaScript context representation for this engine.
    type Context<'a>: Copy;
    /// Error representation for this engine.
    type Error;
    /// Engine-owned registration for a deferred slot held by an async request.
    type DeferredRegistration: Send + 'static;
    /// Mapped range behavior supported by this engine.
    const MAPPED_RANGE_STRATEGY: MappedRangeStrategy;

    /// Returns the binding environment associated with a context.
    fn environment<'a>(cx: Self::Context<'a>) -> &'a Environment;
    /// Gets an object property.
    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> Result<Self::Value, Self::Error>;
    /// Returns the engine's global object as a call-scoped owned value.
    fn global(cx: Self::Context<'_>) -> Self::Value;
    /// Gets an object property whose key is itself a JavaScript value.
    fn get_property_value(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Calls a JavaScript function with the provided receiver and arguments.
    fn call(
        cx: Self::Context<'_>,
        f: Self::Value,
        this: Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// Returns true for JavaScript `undefined`.
    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Returns true for JavaScript `null`.
    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Converts with JavaScript `ToNumber`.
    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> Result<f64, Self::Error>;
    /// Converts with JavaScript `ToBoolean`.
    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Converts to a UTF-8 string borrowed from the provided arena.
    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a Arena,
    ) -> Result<&'a str, Self::Error>;
    /// Registers a JavaScript class.
    fn register_class(
        cx: Self::Context<'_>,
        spec: &'static ClassSpec<Self>,
    ) -> Result<ClassId, Self::Error>;
    /// Creates an instance carrying a Rust payload.
    fn new_instance(
        cx: Self::Context<'_>,
        class: ClassId,
        payload: Box<dyn Any + Send>,
    ) -> Result<Self::Value, Self::Error>;
    /// Returns an object's payload when it belongs to the requested class.
    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: ClassId,
    ) -> Option<&'a (dyn Any + Send)>;
    /// Creates a JavaScript `undefined` value.
    fn undefined(cx: Self::Context<'_>) -> Self::Value;
    /// Creates a JavaScript number value.
    fn number(cx: Self::Context<'_>, value: f64) -> Result<Self::Value, Self::Error>;
    /// Creates a JavaScript string value.
    fn string(cx: Self::Context<'_>, value: &str) -> Result<Self::Value, Self::Error>;
    /// Creates a synchronous JavaScript type error.
    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error;
    /// Creates a synchronous JavaScript operation error.
    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error;
    /// Creates a rejection reason from a scoped context.
    fn async_error_value(cx: Self::Context<'_>, message: &str) -> Self::Value;
    /// Converts an already-created engine error into a rejection value.
    fn error_value_from_error(cx: Self::Context<'_>, error: Self::Error) -> Self::Value;
    /// Creates a promise and its owned deferred resolving functions.
    fn new_promise(cx: Self::Context<'_>) -> Result<(Self::Value, Deferred<Self>), Self::Error>;
    /// Settles a batch of deferred promises inside one JavaScript frame.
    fn settle_deferreds(cx: Self::Context<'_>, settlements: Vec<DeferredSettlement<Self>>);
    /// Drains engine microtasks scheduled by promise settlement.
    fn drain_microtasks(cx: Self::Context<'_>) -> Result<(), Self::Error>;
    /// Creates a script-visible ArrayBuffer over external memory.
    ///
    /// # Safety
    ///
    /// `ptr..ptr + len` must name the live mapped range returned by
    /// `wgpuBufferGetMappedRange(owner, ..)`. The caller must keep the
    /// `owner` reference passed here alive until the ArrayBuffer's engine
    /// finalizer releases it, and must track the returned value so it is
    /// detached before calling `wgpuBufferUnmap` or `wgpuBufferDestroy`.
    unsafe fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        ptr: *mut u8,
        len: usize,
        owner: WGPUBuffer,
    ) -> Result<Self::Value, Self::Error>;
    /// Creates a script-visible ArrayBuffer by copying bytes.
    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> Result<Self::Value, Self::Error>;
    /// Detaches a script-visible ArrayBuffer, optionally capturing its bytes first.
    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        value: Self::Value,
        out: Option<&mut [u8]>,
    ) -> Result<(), Self::Error>;
    /// Reads an ArrayBuffer byte length through the engine API.
    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize>;
    /// Copies ArrayBuffer bytes through the engine API.
    fn arraybuffer_copy(cx: Self::Context<'_>, value: Self::Value) -> Option<Vec<u8>>;
    /// Duplicates a value so core can hold it beyond the current call.
    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value;
    /// Produces a callback return value from a core-held value.
    ///
    /// This does not create another persistent core hold. Refcounted engines
    /// duplicate callback-return ownership; tracing engines may return the same
    /// identity because the existing core hold keeps it protected.
    fn return_held_value(cx: Self::Context<'_>, held: Self::Value) -> Self::Value;
    /// Releases a value previously duplicated for core.
    fn release_value(cx: Self::Context<'_>, value: Self::Value);
    /// Registers a deferred slot owned by a raw async callback request.
    fn register_deferred(
        cx: Self::Context<'_>,
        slot: NonNull<Option<Deferred<Self>>>,
    ) -> Self::DeferredRegistration;
    /// Releases a deferred without settling it during engine teardown.
    fn release_deferred(cx: Self::Context<'_>, deferred: Deferred<Self>);
}

/// Engine strategy for script-visible mapped ranges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappedRangeStrategy {
    /// Expose backend memory directly and detach it before unmap.
    ZeroCopyDetach,
    /// Copy into a script buffer and copy back before unmap.
    CopyInCopyOut,
}

/// Owned promise resolving functions.
pub struct Deferred<E: JsEngine> {
    resolve: E::Value,
    reject: E::Value,
}

impl<E: JsEngine> Deferred<E> {
    /// Creates a deferred from owned resolving functions.
    #[must_use]
    pub fn new(resolve: E::Value, reject: E::Value) -> Self {
        Self { resolve, reject }
    }

    /// Returns the owned resolve function.
    #[must_use]
    pub fn resolve(&self) -> E::Value {
        self.resolve
    }

    /// Returns the owned reject function.
    #[must_use]
    pub fn reject(&self) -> E::Value {
        self.reject
    }
}

/// A deferred promise paired with the value it should settle with.
pub type DeferredSettlement<E> = (
    Deferred<E>,
    std::result::Result<<E as JsEngine>::Value, <E as JsEngine>::Value>,
);

enum PendingNativeHandle {
    Adapter(WGPUAdapter),
    Device(WGPUDevice),
    Transferred,
}

struct PendingNative {
    handle: PendingNativeHandle,
    queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
}

impl PendingNative {
    fn take_adapter(&mut self) -> WGPUAdapter {
        match std::mem::replace(&mut self.handle, PendingNativeHandle::Transferred) {
            PendingNativeHandle::Adapter(adapter) => adapter,
            PendingNativeHandle::Device(_) | PendingNativeHandle::Transferred => ptr::null_mut(),
        }
    }

    fn take_device(&mut self) -> WGPUDevice {
        match std::mem::replace(&mut self.handle, PendingNativeHandle::Transferred) {
            PendingNativeHandle::Device(device) => device,
            PendingNativeHandle::Adapter(_) | PendingNativeHandle::Transferred => ptr::null_mut(),
        }
    }
}

impl Drop for PendingNative {
    fn drop(&mut self) {
        let request = match self.handle {
            PendingNativeHandle::Adapter(adapter) => ReleaseRequest::Adapter {
                adapter,
                gpu: self.gpu,
            },
            PendingNativeHandle::Device(device) => ReleaseRequest::Device {
                device,
                gpu: self.gpu,
            },
            PendingNativeHandle::Transferred => return,
        };
        let _ = self.queue.enqueue(request);
    }
}

enum SettlementRequest<E: JsEngine + 'static> {
    Adapter {
        deferred: Deferred<E>,
        native: PendingNative,
    },
    Device {
        deferred: Deferred<E>,
        native: PendingNative,
    },
    Success {
        deferred: Deferred<E>,
    },
    Error {
        deferred: Deferred<E>,
        message: String,
    },
}

// SAFETY: settlement requests are created by `AllowProcessEvents` callbacks and
// are only drained by the engine-thread `tick()` after `wgpuInstanceProcessEvents`
// returns. Engine values inside `Deferred` are moved through this queue but are
// never dereferenced off the tick thread.
unsafe impl<E: JsEngine + 'static> Send for SettlementRequest<E> {}

impl<E: JsEngine + 'static> SettlementRequest<E> {
    fn settle(mut self, cx: E::Context<'_>) -> DeferredSettlement<E> {
        match self {
            Self::Adapter {
                deferred,
                ref mut native,
            } => {
                let adapter = native.take_adapter();
                let value =
                    E::new_instance(cx, GPU_ADAPTER_CLASS, Box::new(AdapterPayload { adapter }));
                match value {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => {
                        let _ = native.queue.enqueue(ReleaseRequest::Adapter {
                            adapter,
                            gpu: native.gpu,
                        });
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                }
            }
            Self::Device {
                deferred,
                ref mut native,
            } => {
                let device = native.take_device();
                let value = E::new_instance(
                    cx,
                    GPU_DEVICE_CLASS,
                    Box::new(DevicePayload::<E>::new(device)),
                );
                match value {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => {
                        let _ = native.queue.enqueue(ReleaseRequest::Device {
                            device,
                            gpu: native.gpu,
                        });
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                }
            }
            Self::Success { deferred } => (deferred, Ok(E::undefined(cx))),
            Self::Error { deferred, message } => {
                (deferred, Err(E::async_error_value(cx, &message)))
            }
        }
    }
}

/// Thread-safe FIFO queue of async promise settlements recorded by WebGPU callbacks.
#[derive(Default)]
pub struct SettlementQueue {
    requests: Mutex<VecDeque<Box<dyn Any + Send>>>,
}

impl SettlementQueue {
    /// Creates an empty settlement queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn enqueue<E: JsEngine + 'static>(
        &self,
        request: SettlementRequest<E>,
    ) -> std::result::Result<(), QueueError> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| QueueError::Poisoned("settlement queue"))?;
        requests.push_back(Box::new(request));
        Ok(())
    }

    /// Drains queued settlement records for the selected engine.
    ///
    /// `AllowProcessEvents` callbacks only fire inside `wgpuInstanceProcessEvents`,
    /// which the host calls from `tick()`. Therefore callbacks record here and
    /// only the engine-thread `tick()` drains and touches JavaScript.
    pub fn drain<E: JsEngine + 'static>(
        &self,
        cx: E::Context<'_>,
    ) -> std::result::Result<usize, QueueError> {
        let mut requests = Vec::new();
        loop {
            let request = {
                let mut queued = self
                    .requests
                    .lock()
                    .map_err(|_| QueueError::Poisoned("settlement queue"))?;
                queued.pop_front()
            };
            let Some(request) = request else {
                break;
            };
            let request = request
                .downcast::<SettlementRequest<E>>()
                .map_err(|_| QueueError::UnexpectedSettlementType)?;
            requests.push(request.settle(cx));
        }
        let count = requests.len();
        if !requests.is_empty() {
            E::settle_deferreds(cx, requests);
        }
        Ok(count)
    }

    /// Releases queued settlements for the selected engine without settling them.
    pub fn release_pending<E: JsEngine + 'static>(&self, cx: E::Context<'_>) {
        let requests = self
            .requests
            .lock()
            .map(|mut requests| std::mem::take(&mut *requests))
            .unwrap_or_default();
        for request in requests {
            if let Ok(request) = request.downcast::<SettlementRequest<E>>() {
                match *request {
                    SettlementRequest::Adapter { deferred, .. }
                    | SettlementRequest::Device { deferred, .. }
                    | SettlementRequest::Success { deferred }
                    | SettlementRequest::Error { deferred, .. } => {
                        E::release_deferred(cx, deferred)
                    }
                }
            }
        }
    }
}

/// JavaScript property getter callback.
pub type GetterFn<E> = fn(
    <E as JsEngine>::Context<'_>,
    <E as JsEngine>::Value,
) -> Result<<E as JsEngine>::Value, <E as JsEngine>::Error>;

/// JavaScript property setter callback.
pub type SetterFn<E> = fn(
    <E as JsEngine>::Context<'_>,
    <E as JsEngine>::Value,
    <E as JsEngine>::Value,
) -> Result<(), <E as JsEngine>::Error>;

/// JavaScript method callback.
pub type MethodFn<E> = fn(
    <E as JsEngine>::Context<'_>,
    <E as JsEngine>::Value,
    &[<E as JsEngine>::Value],
) -> Result<<E as JsEngine>::Value, <E as JsEngine>::Error>;

/// JavaScript finalizer callback.
pub type FinalizerFn = fn(Box<dyn Any + Send>, &Environment);

/// A JavaScript property specification.
pub struct PropertySpec<E: JsEngine + 'static> {
    /// Property name.
    pub name: &'static str,
    /// Optional getter.
    pub get: Option<GetterFn<E>>,
    /// Optional setter.
    pub set: Option<SetterFn<E>>,
}

/// A JavaScript method specification.
pub struct MethodSpec<E: JsEngine + 'static> {
    /// Method name.
    pub name: &'static str,
    /// Method arity.
    pub length: u8,
    /// Method callback.
    pub call: MethodFn<E>,
}

/// A JavaScript class specification.
pub struct ClassSpec<E: JsEngine + 'static> {
    /// Class name.
    pub name: &'static str,
    /// Class identifier requested by core.
    pub id: ClassId,
    /// Properties installed on the class prototype.
    pub properties: &'static [PropertySpec<E>],
    /// Methods installed on the class prototype.
    pub methods: &'static [MethodSpec<E>],
    /// Finalizer callback.
    pub finalizer: FinalizerFn,
}

/// One release request enqueued by finalizers and drained by the host tick.
pub enum ReleaseRequest {
    /// Release an adapter.
    Adapter {
        /// Adapter handle to release.
        adapter: WGPUAdapter,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release an adopted device.
    Device {
        /// Device handle to release.
        device: WGPUDevice,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a buffer, then the parent device reference held by the buffer.
    BufferWithDeviceRef {
        /// Buffer handle to release.
        buffer: WGPUBuffer,
        /// Parent device reference owned by the buffer wrapper.
        device: WGPUDevice,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a standalone buffer reference.
    Buffer {
        /// Buffer handle to release.
        buffer: WGPUBuffer,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a queue.
    Queue {
        /// Queue handle to release.
        queue: WGPUQueue,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a shader module.
    ShaderModule {
        /// Shader module handle to release.
        module: WGPUShaderModule,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a bind group layout.
    BindGroupLayout {
        /// Bind group layout handle to release.
        layout: WGPUBindGroupLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a pipeline layout.
    PipelineLayout {
        /// Pipeline layout handle to release.
        layout: WGPUPipelineLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a bind group and the native buffer refs it stores.
    BindGroup {
        /// Bind group handle to release.
        bind_group: WGPUBindGroup,
        /// Layout reference held by the bind group wrapper.
        layout: WGPUBindGroupLayout,
        /// Buffer references held by the bind group wrapper.
        buffers: Vec<WGPUBuffer>,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a compute pipeline.
    ComputePipeline {
        /// Compute pipeline handle to release.
        pipeline: WGPUComputePipeline,
        /// Shader module reference held by the compute pipeline wrapper.
        module: WGPUShaderModule,
        /// Explicit layout reference held by the wrapper, or null for auto layout.
        layout: WGPUPipelineLayout,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a command encoder.
    CommandEncoder {
        /// Command encoder handle to release.
        encoder: WGPUCommandEncoder,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a command buffer.
    CommandBuffer {
        /// Command buffer handle to release.
        command_buffer: WGPUCommandBuffer,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a compute pass encoder.
    ComputePassEncoder {
        /// Compute pass encoder handle to release.
        pass: WGPUComputePassEncoder,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
}

// SAFETY: `ReleaseRequest` carries WGPU adapter, device, buffer, queue, shader
// module, bind group layout, pipeline layout, bind group, compute pipeline,
// command encoder, command buffer, and compute pass encoder handles from
// JavaScriptCore finalizers to the release queue. Finalizers only enqueue these
// handle values; the native handles are dereferenced only by `ReleaseRequest::run`,
// which is called by `ReleaseQueue::drain()` from the host `tick()` thread that
// created the WebGPU objects.
// SAFETY: WGPU handles in requests are released only by `run()` during `tick()` drain.
unsafe impl Send for ReleaseRequest {}

impl ReleaseRequest {
    fn run(self) {
        match self {
            Self::Adapter { adapter, gpu } => unsafe {
                (gpu.adapter_release)(adapter);
            },
            Self::Device { device, gpu } => unsafe {
                (gpu.device_release)(device);
            },
            Self::BufferWithDeviceRef {
                buffer,
                device,
                gpu,
            } => unsafe {
                (gpu.buffer_release)(buffer);
                (gpu.device_release)(device);
            },
            Self::Buffer { buffer, gpu } => unsafe {
                (gpu.buffer_release)(buffer);
            },
            Self::Queue { queue, gpu } => unsafe {
                (gpu.queue_release)(queue);
            },
            Self::ShaderModule { module, gpu } => unsafe {
                (gpu.shader_module_release)(module);
            },
            Self::BindGroupLayout { layout, gpu } => unsafe {
                (gpu.bind_group_layout_release)(layout);
            },
            Self::PipelineLayout { layout, gpu } => unsafe {
                (gpu.pipeline_layout_release)(layout);
            },
            Self::BindGroup {
                bind_group,
                layout,
                buffers,
                gpu,
            } => unsafe {
                (gpu.bind_group_release)(bind_group);
                (gpu.bind_group_layout_release)(layout);
                for buffer in buffers {
                    (gpu.buffer_release)(buffer);
                }
            },
            Self::ComputePipeline {
                pipeline,
                module,
                layout,
                gpu,
            } => unsafe {
                (gpu.compute_pipeline_release)(pipeline);
                (gpu.shader_module_release)(module);
                if !layout.is_null() {
                    (gpu.pipeline_layout_release)(layout);
                }
            },
            Self::CommandEncoder { encoder, gpu } => unsafe {
                (gpu.command_encoder_release)(encoder);
            },
            Self::CommandBuffer {
                command_buffer,
                gpu,
            } => unsafe {
                (gpu.command_buffer_release)(command_buffer);
            },
            Self::ComputePassEncoder { pass, gpu } => unsafe {
                (gpu.compute_pass_encoder_release)(pass);
            },
        }
    }
}

/// Thread-safe FIFO release queue.
#[derive(Default)]
pub struct ReleaseQueue {
    requests: Mutex<VecDeque<ReleaseRequest>>,
}

impl ReleaseQueue {
    /// Creates an empty release queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a release request.
    pub fn enqueue(&self, request: ReleaseRequest) -> std::result::Result<(), QueueError> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| QueueError::Poisoned("release queue"))?;
        requests.push_back(request);
        Ok(())
    }

    /// Drains all currently queued release requests on the calling thread.
    pub fn drain(&self) -> std::result::Result<usize, QueueError> {
        let mut drained = 0;
        loop {
            let request = {
                let mut requests = self
                    .requests
                    .lock()
                    .map_err(|_| QueueError::Poisoned("release queue"))?;
                requests.pop_front()
            };
            let Some(request) = request else {
                return Ok(drained);
            };
            request.run();
            drained += 1;
        }
    }

    /// Returns the current queue length.
    pub fn len(&self) -> std::result::Result<usize, QueueError> {
        self.requests
            .lock()
            .map(|requests| requests.len())
            .map_err(|_| QueueError::Poisoned("release queue"))
    }

    /// Returns whether the queue is empty.
    pub fn is_empty(&self) -> std::result::Result<bool, QueueError> {
        self.len().map(|len| len == 0)
    }
}

/// Release queue error.
#[derive(Debug, Eq, PartialEq)]
pub enum QueueError {
    /// A mutex was poisoned.
    Poisoned(&'static str),
    /// A settlement queued for a different engine was encountered.
    UnexpectedSettlementType,
}

/// Failure from the engine-neutral four-step tick skeleton.
#[derive(Debug)]
#[non_exhaustive]
pub enum TickError<E> {
    /// Promise settlement or release queue failure.
    Queue(QueueError),
    /// Engine microtask drain failure.
    Engine(E),
}

/// Runs one host tick in the required cross-engine order.
///
/// The order is WebGPU `ProcessEvents`, one batched settlement drain, engine
/// microtasks, then the native release queue.
///
/// # Safety
///
/// `instance` must be a live instance from `E`'s configured backend and must
/// remain valid for this call. The caller must invoke this on the designated
/// engine/tick thread.
pub unsafe fn tick<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    instance: webgpu_native_js_ffi::native::WGPUInstance,
) -> std::result::Result<usize, TickError<E::Error>> {
    let env = E::environment(cx);
    unsafe { (env.gpu().instance_process_events)(instance) };
    env.settlements().drain::<E>(cx).map_err(TickError::Queue)?;
    E::drain_microtasks(cx).map_err(TickError::Engine)?;
    env.queue().drain().map_err(TickError::Queue)
}

/// Payload stored by a `GPUDevice` wrapper.
pub struct DevicePayload<E: JsEngine> {
    device: WGPUDevice,
    queue: HeldValue<E>,
}

impl<E: JsEngine> DevicePayload<E> {
    fn new(device: WGPUDevice) -> Self {
        Self {
            device,
            queue: HeldValue::empty(),
        }
    }

    /// Returns the native device handle.
    #[must_use]
    pub fn device(&self) -> WGPUDevice {
        self.device
    }

    fn cached_queue(&self) -> Option<E::Value> {
        self.queue.get()
    }

    fn cache_queue(&self, value: E::Value) {
        self.queue.set(value);
    }
}

// SAFETY: `DevicePayload` stores an adopted `WGPUDevice` plus a cached engine
// value in `HeldValue`. A finalizer may move the payload to another thread, but
// it only copies the native handle into `ReleaseRequest::Device` and passes the
// opaque cached value to the adapter-provided release closure. Neither value is
// dereferenced as a native/engine object off-thread; actual native release runs
// when the queue drains on the creating `tick()` thread.
unsafe impl<E: JsEngine> Send for DevicePayload<E> {}

struct HeldValue<E: JsEngine> {
    value: std::cell::UnsafeCell<Option<E::Value>>,
}

impl<E: JsEngine> HeldValue<E> {
    fn empty() -> Self {
        Self {
            value: std::cell::UnsafeCell::new(None),
        }
    }

    fn get(&self) -> Option<E::Value> {
        // SAFETY: wrapper access and GC tracing are confined to the engine
        // thread and cannot run concurrently with a property getter.
        unsafe { *self.value.get() }
    }

    fn set(&self, value: E::Value) {
        // SAFETY: the queue cache is initialized at most once by the engine
        // thread; GC tracing can only observe it between JS entry points.
        unsafe { *self.value.get() = Some(value) };
    }

    fn take(&self) -> Option<E::Value> {
        // SAFETY: payload value release runs after the wrapper is unreachable,
        // with no concurrent getter or trace operation.
        unsafe { &mut *self.value.get() }.take()
    }
}

// SAFETY: the held engine value is never dereferenced off the engine thread;
// finalizers only pass it to the adapter-provided value-release operation.
unsafe impl<E: JsEngine> Send for HeldValue<E> {}

/// Payload stored by a `GPUBuffer` wrapper.
pub struct BufferPayload<E: JsEngine> {
    state: Arc<Mutex<BufferState<E>>>,
    traced_values: Arc<TracedValues<E>>,
}

impl<E: JsEngine> BufferPayload<E> {
    /// Returns the shared buffer state.
    #[must_use]
    pub fn state(&self) -> &Arc<Mutex<BufferState<E>>> {
        &self.state
    }

    /// Visits every mapped range value held by this payload.
    pub fn trace_mapped_range_values(&self, mut visit: impl FnMut(E::Value)) {
        self.traced_values.visit(&mut visit);
    }

    /// Removes tracked mapped ranges and passes their held values to `release`.
    pub fn release_mapped_range_values(&self, mut release: impl FnMut(E::Value)) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        self.traced_values.clear();
        for range in std::mem::take(&mut state.ranges) {
            release(range.value);
        }
    }
}

/// Visits every engine value retained by a core wrapper payload.
pub fn trace_payload_values<E: JsEngine + 'static>(
    payload: &(dyn Any + Send),
    visit: &mut dyn FnMut(E::Value),
) {
    if let Some(buffer) = payload.downcast_ref::<BufferPayload<E>>() {
        buffer.trace_mapped_range_values(&mut *visit);
    }
    if let Some(device) = payload.downcast_ref::<DevicePayload<E>>() {
        if let Some(queue) = device.cached_queue() {
            visit(queue);
        }
    }
}

/// Removes every engine value retained by a core wrapper payload.
pub fn release_payload_values<E: JsEngine + 'static>(
    payload: &(dyn Any + Send),
    release: &mut dyn FnMut(E::Value),
) {
    if let Some(buffer) = payload.downcast_ref::<BufferPayload<E>>() {
        buffer.release_mapped_range_values(&mut *release);
    }
    if let Some(device) = payload.downcast_ref::<DevicePayload<E>>() {
        if let Some(queue) = device.queue.take() {
            release(queue);
        }
    }
}

struct TracedValues<E: JsEngine> {
    values: std::cell::UnsafeCell<Vec<E::Value>>,
}

impl<E: JsEngine> TracedValues<E> {
    fn new() -> Self {
        Self {
            values: std::cell::UnsafeCell::new(Vec::new()),
        }
    }

    fn push(&self, value: E::Value) {
        // SAFETY: mapped range tracking is mutated only by JS entry points on
        // the engine thread. `gc_mark` may read the same vector during QuickJS
        // GC, which cannot run concurrently with those entry points.
        unsafe { &mut *self.values.get() }.push(value);
    }

    fn clear(&self) {
        // SAFETY: see `push`.
        unsafe { &mut *self.values.get() }.clear();
    }

    fn visit(&self, visit: &mut dyn FnMut(E::Value)) {
        // SAFETY: see `push`; this path must stay allocation- and lock-free for
        // engine GC tracing.
        for value in unsafe { &*self.values.get() }.iter().copied() {
            visit(value);
        }
    }
}

// SAFETY: `TracedValues` stores JS mapped-range values, not WGPU handles. The
// vector is mutated by buffer methods and read by engine GC tracing on the engine
// thread; it is present in `BufferPayload` only so a finalizer can move the
// payload, clear the vector, and release the held JS values without concurrent
// access from WebGPU callbacks.
// SAFETY: Contains JS mapped-range values only; no WGPU handles are dereferenced off-thread.
unsafe impl<E: JsEngine> Send for TracedValues<E> {}
// SAFETY: `TracedValues` uses interior mutability for GC tracing. This is sound
// for QuickJS because tracing and finalizers run on the engine thread and cannot
// race JS entry points. A future engine with any-thread tracing/finalizers must
// replace this storage or add synchronization before enabling mapped ranges.
// Shared references are used only to visit or clear JS values, not to dereference
// native handles.
// SAFETY: Shared access is engine GC/finalizer bookkeeping and does not use WGPU handles.
unsafe impl<E: JsEngine> Sync for TracedValues<E> {}

/// Mutable state of a `GPUBuffer` wrapper.
pub struct BufferState<E: JsEngine> {
    buffer: WGPUBuffer,
    parent_device: WGPUDevice,
    size: u64,
    usage: u64,
    label: String,
    destroyed: bool,
    mapped: bool,
    map_mode: WGPUMapMode,
    ranges: Vec<MappedRange<E>>,
}

impl<E: JsEngine> BufferState<E> {
    /// Returns the native buffer handle.
    #[must_use]
    pub fn buffer(&self) -> WGPUBuffer {
        self.buffer
    }

    /// Returns the parent device reference owned by this buffer wrapper.
    #[must_use]
    pub fn parent_device(&self) -> WGPUDevice {
        self.parent_device
    }
}

// SAFETY: `BufferPayload` owns shared `BufferState` containing `WGPUBuffer` and
// its parent `WGPUDevice` reference. A finalizer may move the payload and lock the
// state to copy those handle values into `ReleaseRequest::BufferWithDeviceRef`;
// native buffer/device calls happen either in JS methods on the engine thread or
// during release-queue drain on the creating `tick()` thread.
// SAFETY: `WGPUBuffer`/parent `WGPUDevice` are copied by finalizers and released in `tick()`.
unsafe impl<E: JsEngine> Send for BufferPayload<E> {}
// SAFETY: `BufferState` carries `WGPUBuffer` and `WGPUDevice` handles plus JS
// mapped-range bookkeeping. Moving the state between threads is limited to the
// finalizer path described above; the handles are dereferenced by buffer methods
// on the engine thread or by `ReleaseRequest::run()` on the `tick()` thread.
// SAFETY: Buffer state handles are used on the engine thread or by `tick()` drain only.
unsafe impl<E: JsEngine> Send for BufferState<E> {}

/// Payload stored by a `GPU` wrapper.
pub struct GpuPayload {
    instance: webgpu_native_js_ffi::native::WGPUInstance,
}

// SAFETY: `GpuPayload` stores the `WGPUInstance` exposed as `navigator.gpu`.
// `WGPUInstance` is the one handle allowed to be used across threads by the
// underlying API contract; this payload currently has no releasing finalizer.
// SAFETY: `WGPUInstance` is cross-thread-capable and this payload has no release finalizer.
unsafe impl Send for GpuPayload {}

/// Payload stored by a `GPUAdapter` wrapper.
pub struct AdapterPayload {
    adapter: WGPUAdapter,
}

// SAFETY: `AdapterPayload` stores a `WGPUAdapter`. If a finalizer runs off the
// engine thread, it only moves the adapter handle into `ReleaseRequest::Adapter`;
// `wgpuAdapterRelease` is called later by release-queue drain on the creating
// `tick()` thread.
// SAFETY: The `WGPUAdapter` is only enqueued off-thread and released during `tick()` drain.
unsafe impl Send for AdapterPayload {}

/// Payload stored by a `GPUQueue` wrapper.
pub struct QueuePayload {
    queue: WGPUQueue,
}

// SAFETY: `QueuePayload` stores a `WGPUQueue`. Off-thread finalization only
// enqueues `ReleaseRequest::Queue`; queue operations run from JS methods on the
// engine thread, and `wgpuQueueRelease` runs during `tick()`-thread drain.
// SAFETY: The `WGPUQueue` is used by JS methods or released during `tick()` drain.
unsafe impl Send for QueuePayload {}

/// Payload stored by a `GPUShaderModule` wrapper.
pub struct ShaderModulePayload {
    module: WGPUShaderModule,
}

// SAFETY: `ShaderModulePayload` stores a `WGPUShaderModule`. The finalizer only
// moves the handle into `ReleaseRequest::ShaderModule`; the module is
// dereferenced for release when the queue drains on the creating `tick()` thread.
// SAFETY: The `WGPUShaderModule` is finalizer-enqueued and released on the `tick()` thread.
unsafe impl Send for ShaderModulePayload {}

/// Payload stored by a `GPUBindGroupLayout` wrapper.
pub struct BindGroupLayoutPayload {
    layout: WGPUBindGroupLayout,
}

// SAFETY: `BindGroupLayoutPayload` stores a `WGPUBindGroupLayout`. Finalization
// only enqueues that handle; `wgpuBindGroupLayoutRelease` is invoked by
// `ReleaseRequest::run()` on the `tick()` thread.
// SAFETY: The `WGPUBindGroupLayout` is released only by `tick()`-thread drain.
unsafe impl Send for BindGroupLayoutPayload {}

/// Payload stored by a `GPUPipelineLayout` wrapper.
pub struct PipelineLayoutPayload {
    layout: WGPUPipelineLayout,
}

// SAFETY: `PipelineLayoutPayload` stores a `WGPUPipelineLayout`. Off-thread
// finalization only moves the handle into the release queue; the native release
// call happens when `tick()` drains the queue on the creating thread.
// SAFETY: The `WGPUPipelineLayout` is only enqueued off-thread and released in `tick()`.
unsafe impl Send for PipelineLayoutPayload {}

/// Payload stored by a `GPUBindGroup` wrapper.
pub struct BindGroupPayload {
    bind_group: WGPUBindGroup,
    layout: WGPUBindGroupLayout,
    buffers: Vec<WGPUBuffer>,
}

// SAFETY: `BindGroupPayload` stores a `WGPUBindGroup` and the `WGPUBuffer`
// references retained for its entries. A finalizer only transfers those handle
// values into `ReleaseRequest::BindGroup`; the bind group and buffers are
// released by queue drain on the creating `tick()` thread.
// SAFETY: `WGPUBindGroup`, its layout, and retained `WGPUBuffer`s are released
// during `tick()` drain.
unsafe impl Send for BindGroupPayload {}

/// Payload stored by a `GPUComputePipeline` wrapper.
pub struct ComputePipelinePayload {
    pipeline: WGPUComputePipeline,
    module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
}

// SAFETY: `ComputePipelinePayload` stores a `WGPUComputePipeline`. The handle may
// be moved by an off-thread finalizer, but it is only dereferenced by compute
// pass methods on the engine thread or by `wgpuComputePipelineRelease` during
// `tick()`-thread release drain.
// SAFETY: The `WGPUComputePipeline` is used on the engine thread or released in `tick()`.
unsafe impl Send for ComputePipelinePayload {}

/// Payload stored by a `GPUCommandEncoder` wrapper.
pub struct CommandEncoderPayload {
    state: Arc<Mutex<CommandEncoderState>>,
}

// SAFETY: `CommandEncoderPayload` stores a `WGPUCommandEncoder` inside shared
// state. JS command-encoder methods dereference it on the engine thread; a
// finalizer may run elsewhere but only locks the state, copies the handle into
// `ReleaseRequest::CommandEncoder`, and leaves native release to `tick()`-thread
// queue drain.
// SAFETY: The `WGPUCommandEncoder` is used on the engine thread or released in `tick()`.
unsafe impl Send for CommandEncoderPayload {}

/// Payload stored by a `GPUCommandBuffer` wrapper.
pub struct CommandBufferPayload {
    state: Arc<Mutex<CommandBufferState>>,
}

// SAFETY: `CommandBufferPayload` stores a `WGPUCommandBuffer`. Queue submission
// dereferences it from JS on the engine thread; finalization only enqueues
// `ReleaseRequest::CommandBuffer`, whose native release runs during drain on the
// creating `tick()` thread.
// SAFETY: The `WGPUCommandBuffer` is submitted on the engine thread or released in `tick()`.
unsafe impl Send for CommandBufferPayload {}

struct CommandBufferState {
    command_buffer: WGPUCommandBuffer,
    consumed: bool,
}

// SAFETY: `CommandBufferState` contains a `WGPUCommandBuffer` and a consumed
// flag protected by a `Mutex`. JS queue methods dereference the command buffer
// only on the engine thread; finalizers may lock the state off-thread only to
// copy the handle into `ReleaseRequest::CommandBuffer`, drained on the creating
// `tick()` thread.
// SAFETY: The `WGPUCommandBuffer` is copied by finalizers and submitted in engine/`tick()`.
unsafe impl Send for CommandBufferState {}

/// Payload stored by a `GPUComputePassEncoder` wrapper.
pub struct ComputePassEncoderPayload {
    state: Arc<Mutex<ComputePassState>>,
}

// SAFETY: `ComputePassEncoderPayload` stores a `WGPUComputePassEncoder` inside
// shared state and a parent command-encoder state reference. JS pass methods
// dereference the pass on the engine thread; finalization only copies the pass
// handle into `ReleaseRequest::ComputePassEncoder`, drained on the creating
// `tick()` thread.
// SAFETY: The `WGPUComputePassEncoder` is used on the engine thread or released in `tick()`.
unsafe impl Send for ComputePassEncoderPayload {}

struct CommandEncoderState {
    encoder: WGPUCommandEncoder,
    ended: bool,
}

// SAFETY: `CommandEncoderState` contains a `WGPUCommandEncoder` and an ended
// flag protected by a `Mutex`. JS methods dereference the encoder only on the
// engine thread; finalizers may lock the state off-thread only to copy the
// handle into `ReleaseRequest::CommandEncoder`, whose release runs during
// `tick()`-thread drain.
// SAFETY: The `WGPUCommandEncoder` is copied by finalizers and dereferenced in engine/`tick()`.
unsafe impl Send for CommandEncoderState {}

struct ComputePassState {
    pass: WGPUComputePassEncoder,
    ended: bool,
    parent: Arc<Mutex<CommandEncoderState>>,
}

// SAFETY: `ComputePassState` contains a `WGPUComputePassEncoder` and a parent
// command-encoder state reference. JS pass methods dereference the pass only on
// the engine thread; finalizers may lock the state off-thread only to copy the
// pass into `ReleaseRequest::ComputePassEncoder`, drained on the creating `tick()`
// thread.
// SAFETY: The `WGPUComputePassEncoder` is copied by finalizers and dereferenced in engine/`tick()`.
unsafe impl Send for ComputePassState {}

#[derive(Clone, Copy)]
struct MappedRange<E: JsEngine> {
    value: E::Value,
    offset: usize,
    size: usize,
    strategy: MappedRangeStrategy,
    map_mode: WGPUMapMode,
}

/// Registers the GPUDevice class.
pub fn register_device_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, device_class::<E>())
}

/// Registers the GPUBuffer class.
pub fn register_buffer_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, buffer_class::<E>())
}

/// Registers the GPU class.
pub fn register_gpu_class<E: JsEngine + 'static>(cx: E::Context<'_>) -> Result<ClassId, E::Error> {
    E::register_class(cx, gpu_class::<E>())
}

/// Registers the GPUAdapter class.
pub fn register_adapter_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, adapter_class::<E>())
}

/// Wraps a native instance as a JavaScript `GPU`.
pub fn wrap_gpu<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    instance: webgpu_native_js_ffi::native::WGPUInstance,
) -> Result<E::Value, E::Error> {
    if instance.is_null() {
        return Err(E::operation_error(
            cx,
            "wrap_gpu received a null WGPUInstance",
        ));
    }
    let _ = register_gpu_class::<E>(cx)?;
    let _ = register_adapter_class::<E>(cx)?;
    let _ = register_device_class::<E>(cx)?;
    let _ = register_buffer_class::<E>(cx)?;
    E::new_instance(cx, GPU_CLASS, Box::new(GpuPayload { instance }))
}

/// Wraps an adopted native device as a JavaScript `GPUDevice`.
///
/// # Safety
///
/// `device` must be non-null, must belong to the dispatch table in
/// `E::environment(cx)`, and the caller must own or have borrowed a live native
/// reference for the duration of this call. This function takes its own native
/// reference before returning.
pub unsafe fn wrap_device<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    device: WGPUDevice,
) -> Result<E::Value, E::Error> {
    if device.is_null() {
        return Err(E::operation_error(
            cx,
            "wrap_device received a null WGPUDevice",
        ));
    }
    let env = E::environment(cx);
    unsafe {
        (env.gpu().device_add_ref)(device);
    }
    let _ = register_device_class::<E>(cx)?;
    let _ = register_buffer_class::<E>(cx)?;
    E::new_instance(
        cx,
        GPU_DEVICE_CLASS,
        Box::new(DevicePayload::<E>::new(device)),
    )
}

/// Implements `GPUDevice.createBuffer`.
pub fn device_create_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(device_payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
    else {
        return Err(E::type_error(
            cx,
            "GPUDevice.createBuffer called on an incompatible object",
        ));
    };
    let Some(descriptor) = args.first().copied() else {
        return Err(E::type_error(cx, "GPUBufferDescriptor is required"));
    };

    let arena = Arena::new();
    let converted = convert_buffer_descriptor::<E>(cx, descriptor, &arena)?;
    let env = E::environment(cx);
    let gpu = env.gpu();
    let native = WGPUBufferDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(converted.label.as_bytes()),
        usage: converted.usage,
        size: converted.size,
        mappedAtCreation: u32::from(converted.mapped_at_creation),
    };
    let buffer =
        unsafe { (gpu.device_create_buffer)(device_payload.device, ptr::from_ref(&native)) };
    // Only wgpuDeviceCreateBuffer is contractually nullable; the other createXxx
    // null checks in this file are defensive against backend failures.
    if buffer.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateBuffer returned null",
        ));
    }
    unsafe {
        (gpu.device_add_ref)(device_payload.device);
    }
    let state = BufferState {
        buffer,
        parent_device: device_payload.device,
        size: converted.size,
        usage: converted.usage,
        label: converted.label,
        destroyed: false,
        mapped: converted.mapped_at_creation,
        map_mode: if converted.mapped_at_creation {
            WGPUMapMode_Write
        } else {
            0
        },
        ranges: Vec::new(),
    };
    match E::new_instance(
        cx,
        GPU_BUFFER_CLASS,
        Box::new(BufferPayload::<E> {
            state: Arc::new(Mutex::new(state)),
            traced_values: Arc::new(TracedValues::new()),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.buffer_release)(buffer);
                (gpu.device_release)(device_payload.device);
            }
            Err(error)
        }
    }
}

/// Implements `GPUBuffer.destroy`.
pub fn buffer_destroy<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    with_buffer_payload_state::<E, _, _>(cx, this, |payload, state| {
        if !state.destroyed {
            detach_all_ranges::<E>(cx, payload, state, false)?;
            unsafe {
                (E::environment(cx).gpu().buffer_destroy)(state.buffer);
            }
            state.destroyed = true;
        }
        Ok(E::undefined(cx))
    })
}

/// Implements `GPU.requestAdapter`.
pub fn gpu_request_adapter<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(payload) =
        E::payload(cx, this, GPU_CLASS).and_then(|payload| payload.downcast_ref::<GpuPayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPU.requestAdapter called on an incompatible object",
        ));
    };
    let (promise, deferred) = E::new_promise(cx)?;
    let mut request = Box::new(AdapterRequest::<E> {
        deferred: Some(deferred),
        settlements: Arc::clone(E::environment(cx).settlements()),
        release_queue: Arc::clone(E::environment(cx).queue()),
        gpu: E::environment(cx).gpu(),
        _registration: None,
    });
    request._registration = Some(E::register_deferred(
        cx,
        NonNull::from(&mut request.deferred),
    ));
    let info = WGPURequestAdapterCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(request_adapter_callback::<E>),
        userdata1: Box::into_raw(request).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe {
        (E::environment(cx).gpu().instance_request_adapter)(payload.instance, ptr::null(), info);
    }
    Ok(promise)
}

/// Implements `GPUAdapter.requestDevice`.
pub fn adapter_request_device<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(payload) = E::payload(cx, this, GPU_ADAPTER_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterPayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPUAdapter.requestDevice called on an incompatible object",
        ));
    };
    let (promise, deferred) = E::new_promise(cx)?;
    let mut request = Box::new(DeviceRequest::<E> {
        deferred: Some(deferred),
        settlements: Arc::clone(E::environment(cx).settlements()),
        release_queue: Arc::clone(E::environment(cx).queue()),
        gpu: E::environment(cx).gpu(),
        _registration: None,
    });
    request._registration = Some(E::register_deferred(
        cx,
        NonNull::from(&mut request.deferred),
    ));
    let info = WGPURequestDeviceCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(request_device_callback::<E>),
        userdata1: Box::into_raw(request).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe {
        (E::environment(cx).gpu().adapter_request_device)(payload.adapter, ptr::null(), info);
    }
    Ok(promise)
}

/// Implements `GPUBuffer.mapAsync`.
pub fn buffer_map_async<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let mode_value = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUMapModeFlags is required"))?;
    let mode = u64::from(enforce_u32::<E>(cx, mode_value, "mode")?);
    let offset = optional_gpu_size_to_usize::<E>(cx, args.get(1).copied(), "offset", 0)?;
    let size = match args.get(2).copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            optional_gpu_size_to_usize::<E>(cx, Some(value), "size", 0)?
        }
        _ => WGPU_WHOLE_MAP_SIZE as usize,
    };

    let Some(payload) = E::payload(cx, this, GPU_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<BufferPayload<E>>())
    else {
        return Err(E::type_error(
            cx,
            "GPUBuffer.mapAsync called on an incompatible object",
        ));
    };
    let (buffer, state) = {
        let Ok(state) = payload.state.lock() else {
            return Err(E::operation_error(cx, "GPUBuffer state is poisoned"));
        };
        if state.destroyed {
            return Err(E::operation_error(cx, "GPUBuffer is destroyed"));
        }
        (state.buffer, Arc::clone(&payload.state))
    };
    let (promise, deferred) = E::new_promise(cx)?;
    let mut request = Box::new(MapRequest::<E> {
        deferred: Some(deferred),
        settlements: Arc::clone(E::environment(cx).settlements()),
        _registration: None,
        mode,
        state,
    });
    request._registration = Some(E::register_deferred(
        cx,
        NonNull::from(&mut request.deferred),
    ));
    let info = WGPUBufferMapCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(buffer_map_callback::<E>),
        userdata1: Box::into_raw(request).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe {
        (E::environment(cx).gpu().buffer_map_async)(buffer, mode, offset, size, info);
    }
    Ok(promise)
}

/// Implements `GPUBuffer.getMappedRange`.
pub fn buffer_get_mapped_range<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let offset = optional_gpu_size_to_usize::<E>(cx, args.first().copied(), "offset", 0)?;
    let explicit_size = match args.get(1).copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(optional_gpu_size_to_usize::<E>(cx, Some(value), "size", 0)?)
        }
        _ => None,
    };
    with_buffer_payload_state::<E, _, _>(cx, this, |payload, state| {
        if state.destroyed || !state.mapped {
            return Err(E::operation_error(cx, "buffer is not mapped"));
        }
        let size = match explicit_size {
            Some(size) => size,
            None => state
                .size
                .checked_sub(offset as u64)
                .and_then(|len| usize::try_from(len).ok())
                .filter(|len| *len <= u32::MAX as usize)
                .ok_or_else(|| E::type_error(cx, "size"))?,
        };
        let ptr = mapped_range_ptr::<E>(cx, state, offset, size);
        if ptr.is_null() {
            return Err(E::operation_error(
                cx,
                "wgpuBufferGetMappedRange returned null for current map mode",
            ));
        }
        let value = match E::MAPPED_RANGE_STRATEGY {
            MappedRangeStrategy::ZeroCopyDetach => {
                unsafe {
                    (E::environment(cx).gpu().buffer_add_ref)(state.buffer);
                }
                // SAFETY: `wgpuBufferGetMappedRange` returned a non-null mapped
                // range for `size` bytes, and the range is tracked until unmap.
                unsafe { E::new_external_arraybuffer(cx, ptr.cast::<u8>(), size, state.buffer)? }
            }
            MappedRangeStrategy::CopyInCopyOut => {
                let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) };
                E::new_arraybuffer_copy(cx, bytes)?
            }
        };
        let tracked = E::duplicate_value(cx, value);
        state.ranges.push(MappedRange {
            value: tracked,
            offset,
            size,
            strategy: E::MAPPED_RANGE_STRATEGY,
            map_mode: state.map_mode,
        });
        payload.traced_values.push(tracked);
        Ok(value)
    })
}

/// Implements `GPUBuffer.unmap`.
pub fn buffer_unmap<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    with_buffer_payload_state::<E, _, _>(cx, this, |payload, state| {
        if state.destroyed {
            return Ok(E::undefined(cx));
        }
        if state.mapped {
            detach_all_ranges::<E>(cx, payload, state, true)?;
            unsafe {
                (E::environment(cx).gpu().buffer_unmap)(state.buffer);
            }
            state.mapped = false;
        }
        Ok(E::undefined(cx))
    })
}

/// Implements the `GPUBuffer.label` getter.
pub fn buffer_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| E::string(cx, &state.label))
}

/// Implements the `GPUBuffer.label` setter.
pub fn buffer_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let arena = Arena::new();
    let label = E::to_str(cx, value, &arena)?;
    with_buffer_state::<E, _, _>(cx, this, |state| {
        state.label.clear();
        state.label.push_str(label);
        let view = WGPUStringView::from_bytes(label.as_bytes());
        unsafe {
            (E::environment(cx).gpu().buffer_set_label)(state.buffer, view);
        }
        Ok(())
    })
}

/// Implements the `GPUBuffer.size` getter.
pub fn buffer_size_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| E::number(cx, state.size as f64))
}

/// Implements the `GPUBuffer.usage` getter.
pub fn buffer_usage_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| E::number(cx, state.usage as f64))
}

/// Implements the `GPUDevice.queue` getter.
pub fn device_queue_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let Some(device_payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
    else {
        return Err(E::type_error(
            cx,
            "GPUDevice.queue called on an incompatible object",
        ));
    };
    if let Some(queue) = device_payload.cached_queue() {
        return Ok(E::return_held_value(cx, queue));
    }
    let env = E::environment(cx);
    let queue = unsafe { (env.gpu().device_get_queue)(device_payload.device) };
    if queue.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceGetQueue returned null"));
    }
    if let Err(error) = E::register_class(cx, queue_class::<E>()) {
        unsafe { (env.gpu().queue_release)(queue) };
        return Err(error);
    }
    match E::new_instance(cx, GPU_QUEUE_CLASS, Box::new(QueuePayload { queue })) {
        Ok(value) => {
            device_payload.cache_queue(E::duplicate_value(cx, value));
            Ok(value)
        }
        Err(error) => {
            unsafe { (env.gpu().queue_release)(queue) };
            Err(error)
        }
    }
}

/// Implements `GPUQueue.writeBuffer`.
pub fn queue_write_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(queue_payload) = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPUQueue.writeBuffer called on an incompatible object",
        ));
    };
    let buffer_value = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "buffer"))?;
    let offset = enforce_u64::<E>(
        cx,
        args.get(1)
            .copied()
            .ok_or_else(|| E::type_error(cx, "bufferOffset"))?,
        "bufferOffset",
    )?;
    let data_value = args
        .get(2)
        .copied()
        .ok_or_else(|| E::type_error(cx, "data"))?;
    let data =
        E::arraybuffer_copy(cx, data_value).ok_or_else(|| E::type_error(cx, "ArrayBuffer"))?;
    let data_offset = optional_gpu_size_to_usize::<E>(cx, args.get(3).copied(), "dataOffset", 0)?;
    let size = match args.get(4).copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            optional_gpu_size_to_usize::<E>(cx, Some(value), "size", 0)?
        }
        _ => data
            .len()
            .checked_sub(data_offset)
            .ok_or_else(|| E::type_error(cx, "size"))?,
    };
    let end = data_offset
        .checked_add(size)
        .ok_or_else(|| E::type_error(cx, "size"))?;
    if end > data.len() {
        return Err(E::type_error(cx, "size"));
    }
    let buffer = buffer_handle::<E>(cx, buffer_value)?;
    unsafe {
        (E::environment(cx).gpu().queue_write_buffer)(
            queue_payload.queue,
            buffer,
            offset,
            data[data_offset..end].as_ptr().cast(),
            size,
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUQueue.submit`.
pub fn queue_submit<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(queue_payload) = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPUQueue.submit called on an incompatible object",
        ));
    };
    let commands_value = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "commands"))?;
    let arena = Arena::new();
    let command_states = convert_command_buffer_sequence::<E>(cx, commands_value)?;
    let mut command_handles = Vec::with_capacity(command_states.len());
    for state in &command_states {
        let state = state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandBuffer state is poisoned"))?;
        if state.consumed {
            return Err(E::operation_error(cx, "GPUCommandBuffer is consumed"));
        }
        command_handles.push(state.command_buffer);
    }
    for state in &command_states {
        state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandBuffer state is poisoned"))?
            .consumed = true;
    }
    let commands = arena.alloc_slice(command_handles);
    unsafe {
        (E::environment(cx).gpu().queue_submit)(
            queue_payload.queue,
            commands.len(),
            if commands.is_empty() {
                ptr::null()
            } else {
                commands.as_ptr()
            },
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUQueue.onSubmittedWorkDone`.
pub fn queue_on_submitted_work_done<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(queue_payload) = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPUQueue.onSubmittedWorkDone called on an incompatible object",
        ));
    };
    let (promise, deferred) = E::new_promise(cx)?;
    let mut request = Box::new(QueueWorkDoneRequest::<E> {
        deferred: Some(deferred),
        settlements: Arc::clone(E::environment(cx).settlements()),
        _registration: None,
    });
    request._registration = Some(E::register_deferred(
        cx,
        NonNull::from(&mut request.deferred),
    ));
    let info = WGPUQueueWorkDoneCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(queue_work_done_callback::<E>),
        userdata1: Box::into_raw(request).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe {
        (E::environment(cx).gpu().queue_on_submitted_work_done)(queue_payload.queue, info);
    }
    Ok(promise)
}

/// Implements `GPUDevice.createShaderModule`.
pub fn device_create_shader_module<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let desc = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUShaderModuleDescriptor"))?;
    let arena = Arena::new();
    let native = convert_shader_module_descriptor::<E>(cx, desc, &arena)?;
    let module = unsafe {
        (E::environment(cx).gpu().device_create_shader_module)(device, ptr::from_ref(&native))
    };
    if module.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateShaderModule returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, shader_module_class::<E>()) {
        unsafe { (E::environment(cx).gpu().shader_module_release)(module) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_SHADER_MODULE_CLASS,
        Box::new(ShaderModulePayload { module }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().shader_module_release)(module) };
            Err(error)
        }
    }
}

/// Implements `GPUDevice.createBindGroupLayout`.
pub fn device_create_bind_group_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let desc = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUBindGroupLayoutDescriptor"))?;
    let arena = Arena::new();
    let native = convert_bind_group_layout_descriptor::<E>(cx, desc, &arena)?;
    let layout = unsafe {
        (E::environment(cx).gpu().device_create_bind_group_layout)(device, ptr::from_ref(&native))
    };
    if layout.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateBindGroupLayout returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, bind_group_layout_class::<E>()) {
        unsafe { (E::environment(cx).gpu().bind_group_layout_release)(layout) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_BIND_GROUP_LAYOUT_CLASS,
        Box::new(BindGroupLayoutPayload { layout }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().bind_group_layout_release)(layout) };
            Err(error)
        }
    }
}

/// Implements `GPUDevice.createPipelineLayout`.
pub fn device_create_pipeline_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let desc = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUPipelineLayoutDescriptor"))?;
    let arena = Arena::new();
    let native = convert_pipeline_layout_descriptor::<E>(cx, desc, &arena)?;
    let layout = unsafe {
        (E::environment(cx).gpu().device_create_pipeline_layout)(device, ptr::from_ref(&native))
    };
    if layout.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreatePipelineLayout returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, pipeline_layout_class::<E>()) {
        unsafe { (E::environment(cx).gpu().pipeline_layout_release)(layout) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_PIPELINE_LAYOUT_CLASS,
        Box::new(PipelineLayoutPayload { layout }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().pipeline_layout_release)(layout) };
            Err(error)
        }
    }
}

/// Implements `GPUDevice.createBindGroup`.
pub fn device_create_bind_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let desc = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUBindGroupDescriptor"))?;
    let arena = Arena::new();
    let converted = convert_bind_group_descriptor::<E>(cx, desc, &arena)?;
    let bind_group = unsafe {
        (E::environment(cx).gpu().device_create_bind_group)(
            device,
            ptr::from_ref(&converted.native),
        )
    };
    if bind_group.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateBindGroup returned null",
        ));
    }
    let gpu = E::environment(cx).gpu();
    unsafe { (gpu.bind_group_layout_add_ref)(converted.layout) };
    for buffer in &converted.buffers {
        unsafe { (gpu.buffer_add_ref)(*buffer) };
    }
    if let Err(error) = E::register_class(cx, bind_group_class::<E>()) {
        unsafe {
            (gpu.bind_group_release)(bind_group);
            (gpu.bind_group_layout_release)(converted.layout);
            for buffer in &converted.buffers {
                (gpu.buffer_release)(*buffer);
            }
        }
        return Err(error);
    }
    let retained_layout = converted.layout;
    let retained_buffers = converted.buffers.clone();
    match E::new_instance(
        cx,
        GPU_BIND_GROUP_CLASS,
        Box::new(BindGroupPayload {
            bind_group,
            layout: converted.layout,
            buffers: converted.buffers,
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.bind_group_release)(bind_group);
                (gpu.bind_group_layout_release)(retained_layout);
                for buffer in &retained_buffers {
                    (gpu.buffer_release)(*buffer);
                }
            }
            Err(error)
        }
    }
}

/// Implements `GPUDevice.createComputePipeline`.
pub fn device_create_compute_pipeline<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let desc = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "GPUComputePipelineDescriptor"))?;
    let arena = Arena::new();
    let converted = convert_compute_pipeline_descriptor::<E>(cx, desc, &arena)?;
    let pipeline = unsafe {
        (E::environment(cx).gpu().device_create_compute_pipeline)(
            device,
            ptr::from_ref(&converted.native),
        )
    };
    if pipeline.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateComputePipeline returned null",
        ));
    }
    let gpu = E::environment(cx).gpu();
    unsafe {
        (gpu.shader_module_add_ref)(converted.module);
        if !converted.layout.is_null() {
            (gpu.pipeline_layout_add_ref)(converted.layout);
        }
    }
    if let Err(error) = E::register_class(cx, compute_pipeline_class::<E>()) {
        unsafe {
            (gpu.compute_pipeline_release)(pipeline);
            (gpu.shader_module_release)(converted.module);
            if !converted.layout.is_null() {
                (gpu.pipeline_layout_release)(converted.layout);
            }
        };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_COMPUTE_PIPELINE_CLASS,
        Box::new(ComputePipelinePayload {
            pipeline,
            module: converted.module,
            layout: converted.layout,
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (gpu.compute_pipeline_release)(pipeline);
                (gpu.shader_module_release)(converted.module);
                if !converted.layout.is_null() {
                    (gpu.pipeline_layout_release)(converted.layout);
                }
            };
            Err(error)
        }
    }
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
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_command_encoder_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let encoder = unsafe {
        (E::environment(cx).gpu().device_create_command_encoder)(
            device,
            native.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    if encoder.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuDeviceCreateCommandEncoder returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, command_encoder_class::<E>()) {
        unsafe { (E::environment(cx).gpu().command_encoder_release)(encoder) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_COMMAND_ENCODER_CLASS,
        Box::new(CommandEncoderPayload {
            state: Arc::new(Mutex::new(CommandEncoderState {
                encoder,
                ended: false,
            })),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().command_encoder_release)(encoder) };
            Err(error)
        }
    }
}

/// Implements `GPUCommandEncoder.copyBufferToBuffer`.
pub fn command_encoder_copy_buffer_to_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let encoder = live_command_encoder::<E>(cx, this)?;
    let src = buffer_handle::<E>(
        cx,
        args.first()
            .copied()
            .ok_or_else(|| E::type_error(cx, "source"))?,
    )?;
    let src_offset = enforce_u64::<E>(
        cx,
        args.get(1)
            .copied()
            .ok_or_else(|| E::type_error(cx, "sourceOffset"))?,
        "sourceOffset",
    )?;
    let dst = buffer_handle::<E>(
        cx,
        args.get(2)
            .copied()
            .ok_or_else(|| E::type_error(cx, "destination"))?,
    )?;
    let dst_offset = enforce_u64::<E>(
        cx,
        args.get(3)
            .copied()
            .ok_or_else(|| E::type_error(cx, "destinationOffset"))?,
        "destinationOffset",
    )?;
    let size = enforce_u64::<E>(
        cx,
        args.get(4)
            .copied()
            .ok_or_else(|| E::type_error(cx, "size"))?,
        "size",
    )?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .command_encoder_copy_buffer_to_buffer)(
            encoder, src, src_offset, dst, dst_offset, size,
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.beginComputePass`.
pub fn command_encoder_begin_compute_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let parent = command_encoder_state::<E>(cx, this)?;
    let encoder = {
        let state = parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
        if state.ended {
            return Err(E::operation_error(cx, "GPUCommandEncoder is finished"));
        }
        state.encoder
    };
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_compute_pass_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let pass = unsafe {
        (E::environment(cx).gpu().command_encoder_begin_compute_pass)(
            encoder,
            native.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    if pass.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuCommandEncoderBeginComputePass returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, compute_pass_encoder_class::<E>()) {
        unsafe { (E::environment(cx).gpu().compute_pass_encoder_release)(pass) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_COMPUTE_PASS_ENCODER_CLASS,
        Box::new(ComputePassEncoderPayload {
            state: Arc::new(Mutex::new(ComputePassState {
                pass,
                ended: false,
                parent,
            })),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().compute_pass_encoder_release)(pass) };
            Err(error)
        }
    }
}

/// Implements `GPUCommandEncoder.finish`.
pub fn command_encoder_finish<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let state = command_encoder_state::<E>(cx, this)?;
    let encoder = {
        let mut state = state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
        if state.ended {
            return Err(E::operation_error(cx, "GPUCommandEncoder is finished"));
        }
        state.ended = true;
        state.encoder
    };
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_command_buffer_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let command_buffer = unsafe {
        (E::environment(cx).gpu().command_encoder_finish)(
            encoder,
            native.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    if command_buffer.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuCommandEncoderFinish returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, command_buffer_class::<E>()) {
        unsafe { (E::environment(cx).gpu().command_buffer_release)(command_buffer) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_COMMAND_BUFFER_CLASS,
        Box::new(CommandBufferPayload {
            state: Arc::new(Mutex::new(CommandBufferState {
                command_buffer,
                consumed: false,
            })),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().command_buffer_release)(command_buffer) };
            Err(error)
        }
    }
}

/// Implements `GPUComputePassEncoder.setPipeline`.
pub fn compute_pass_set_pipeline<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let pass = live_compute_pass::<E>(cx, this)?;
    let pipeline = compute_pipeline_handle::<E>(
        cx,
        args.first()
            .copied()
            .ok_or_else(|| E::type_error(cx, "pipeline"))?,
    )?;
    unsafe { (E::environment(cx).gpu().compute_pass_encoder_set_pipeline)(pass, pipeline) };
    Ok(E::undefined(cx))
}

/// Implements `GPUComputePassEncoder.setBindGroup`.
pub fn compute_pass_set_bind_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let pass = live_compute_pass::<E>(cx, this)?;
    let index = enforce_u32::<E>(
        cx,
        args.first()
            .copied()
            .ok_or_else(|| E::type_error(cx, "index"))?,
        "index",
    )?;
    let bind_group = bind_group_handle::<E>(
        cx,
        args.get(1)
            .copied()
            .ok_or_else(|| E::type_error(cx, "bindGroup"))?,
    )?;
    unsafe {
        (E::environment(cx).gpu().compute_pass_encoder_set_bind_group)(
            pass,
            index,
            bind_group,
            0,
            ptr::null(),
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUComputePassEncoder.dispatchWorkgroups`.
pub fn compute_pass_dispatch_workgroups<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let pass = live_compute_pass::<E>(cx, this)?;
    let x = enforce_u32::<E>(
        cx,
        args.first()
            .copied()
            .ok_or_else(|| E::type_error(cx, "workgroupCountX"))?,
        "workgroupCountX",
    )?;
    let y = optional_u32::<E>(cx, args.get(1).copied(), "workgroupCountY", 1)?;
    let z = optional_u32::<E>(cx, args.get(2).copied(), "workgroupCountZ", 1)?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .compute_pass_encoder_dispatch_workgroups)(pass, x, y, z)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPUComputePassEncoder.end`.
pub fn compute_pass_end<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUComputePassEncoder method called on an incompatible object",
            )
        })?;
    let mut state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUComputePassEncoder state is poisoned"))?;
    if state.ended {
        return Err(E::operation_error(cx, "GPUComputePassEncoder is ended"));
    }
    unsafe { (E::environment(cx).gpu().compute_pass_encoder_end)(state.pass) };
    state.ended = true;
    Ok(E::undefined(cx))
}

/// Finalizes a `GPUQueue` payload by enqueuing its release.
pub fn finalize_queue(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<QueuePayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::Queue {
        queue: payload.queue,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUShaderModule` payload by enqueuing its release.
pub fn finalize_shader_module(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<ShaderModulePayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::ShaderModule {
        module: payload.module,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUBindGroupLayout` payload by enqueuing its release.
pub fn finalize_bind_group_layout(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BindGroupLayoutPayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::BindGroupLayout {
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUPipelineLayout` payload by enqueuing its release.
pub fn finalize_pipeline_layout(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<PipelineLayoutPayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::PipelineLayout {
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUBindGroup` payload by releasing it and stored buffer refs.
pub fn finalize_bind_group(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BindGroupPayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::BindGroup {
        bind_group: payload.bind_group,
        layout: payload.layout,
        buffers: payload.buffers,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUComputePipeline` payload by enqueuing its release.
pub fn finalize_compute_pipeline(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<ComputePipelinePayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::ComputePipeline {
        pipeline: payload.pipeline,
        module: payload.module,
        layout: payload.layout,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUCommandEncoder` payload by enqueuing its release.
pub fn finalize_command_encoder(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<CommandEncoderPayload>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::CommandEncoder {
        encoder: state.encoder,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUCommandBuffer` payload by enqueuing its release.
pub fn finalize_command_buffer(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<CommandBufferPayload>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::CommandBuffer {
        command_buffer: state.command_buffer,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUComputePassEncoder` payload by enqueuing its release.
pub fn finalize_compute_pass_encoder(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<ComputePassEncoderPayload>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::ComputePassEncoder {
        pass: state.pass,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUDevice` payload by enqueuing its release.
pub fn finalize_device<E: JsEngine + 'static>(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<DevicePayload<E>>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::Device {
        device: payload.device,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUAdapter` payload by enqueuing its release.
pub fn finalize_adapter(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<AdapterPayload>() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::Adapter {
        adapter: payload.adapter,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUBuffer` payload by enqueuing buffer release and parent release.
pub fn finalize_buffer<E: JsEngine + 'static>(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BufferPayload<E>>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    let _ = env.queue().enqueue(ReleaseRequest::BufferWithDeviceRef {
        buffer: state.buffer,
        device: state.parent_device,
        gpu: env.gpu(),
    });
}

struct AdapterRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    release_queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
    _registration: Option<E::DeferredRegistration>,
}

struct DeviceRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    release_queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
    _registration: Option<E::DeferredRegistration>,
}

struct MapRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    _registration: Option<E::DeferredRegistration>,
    mode: WGPUMapMode,
    state: Arc<Mutex<BufferState<E>>>,
}

struct QueueWorkDoneRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    _registration: Option<E::DeferredRegistration>,
}

unsafe fn callback_message(message: WGPUStringView, fallback: &'static str) -> String {
    let backend = if message.data.is_null() {
        String::new()
    } else if message.length == wgpu_strlen() {
        unsafe { std::ffi::CStr::from_ptr(message.data) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::from_utf8_lossy(unsafe {
            std::slice::from_raw_parts(message.data.cast::<u8>(), message.length)
        })
        .into_owned()
    };
    if backend.is_empty() {
        fallback.to_owned()
    } else {
        format!("{fallback}: {backend}")
    }
}

unsafe extern "C" fn request_adapter_callback<E: JsEngine + 'static>(
    status: WGPURequestAdapterStatus,
    adapter: WGPUAdapter,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<AdapterRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            if !adapter.is_null() {
                let _ = request.release_queue.enqueue(ReleaseRequest::Adapter {
                    adapter,
                    gpu: request.gpu,
                });
            }
            return;
        };
        let settlement = if status == WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success
            && !adapter.is_null()
        {
            SettlementRequest::Adapter {
                deferred,
                native: PendingNative {
                    handle: PendingNativeHandle::Adapter(adapter),
                    queue: Arc::clone(&request.release_queue),
                    gpu: request.gpu,
                },
            }
        } else {
            if !adapter.is_null() {
                let _ = request.release_queue.enqueue(ReleaseRequest::Adapter {
                    adapter,
                    gpu: request.gpu,
                });
            }
            SettlementRequest::Error {
                deferred,
                message: unsafe { callback_message(message, "requestAdapter failed") },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn request_device_callback<E: JsEngine + 'static>(
    status: WGPURequestDeviceStatus,
    device: WGPUDevice,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<DeviceRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            if !device.is_null() {
                let _ = request.release_queue.enqueue(ReleaseRequest::Device {
                    device,
                    gpu: request.gpu,
                });
            }
            return;
        };
        let settlement = if status == WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success
            && !device.is_null()
        {
            SettlementRequest::Device {
                deferred,
                native: PendingNative {
                    handle: PendingNativeHandle::Device(device),
                    queue: Arc::clone(&request.release_queue),
                    gpu: request.gpu,
                },
            }
        } else {
            if !device.is_null() {
                let _ = request.release_queue.enqueue(ReleaseRequest::Device {
                    device,
                    gpu: request.gpu,
                });
            }
            SettlementRequest::Error {
                deferred,
                message: unsafe { callback_message(message, "requestDevice failed") },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn buffer_map_callback<E: JsEngine + 'static>(
    status: WGPUMapAsyncStatus,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<MapRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            return;
        };
        let settlement = if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success {
            if let Ok(mut state) = request.state.lock() {
                state.mapped = true;
                state.map_mode = request.mode;
            }
            SettlementRequest::Success { deferred }
        } else {
            let fallback = if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Error {
                "mapAsync error"
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Aborted {
                "mapAsync aborted"
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_CallbackCancelled {
                "mapAsync callback cancelled"
            } else {
                "mapAsync failed"
            };
            SettlementRequest::Error {
                deferred,
                message: unsafe { callback_message(message, fallback) },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn queue_work_done_callback<E: JsEngine + 'static>(
    status: WGPUQueueWorkDoneStatus,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<QueueWorkDoneRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            return;
        };
        let settlement = if status == WGPUQueueWorkDoneStatus_WGPUQueueWorkDoneStatus_Success {
            SettlementRequest::Success { deferred }
        } else {
            SettlementRequest::Error {
                deferred,
                message: unsafe { callback_message(message, "onSubmittedWorkDone failed") },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

#[derive(Debug, Eq, PartialEq)]
struct BufferDescriptor {
    size: u64,
    usage: u64,
    mapped_at_creation: bool,
    label: String,
}

struct ConvertedBindGroupDescriptor {
    native: WGPUBindGroupDescriptor,
    layout: WGPUBindGroupLayout,
    buffers: Vec<WGPUBuffer>,
}

struct ConvertedComputePipelineDescriptor {
    native: WGPUComputePipelineDescriptor,
    module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
}

fn convert_shader_module_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUShaderModuleDescriptor, E::Error> {
    let code_value = required_member::<E>(cx, value, "code")?;
    let code = E::to_str(cx, code_value, arena)?;
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    let source = arena
        .alloc_slice(vec![WGPUShaderSourceWGSL {
            chain: WGPUChainedStruct {
                next: ptr::null_mut(),
                sType: WGPUSType_WGPUSType_ShaderSourceWGSL,
            },
            code: WGPUStringView::from_bytes(code.as_bytes()),
        }])
        .as_ptr();
    // SAFETY: the vector literal above contains exactly one initialized source,
    // and `Arena::alloc_slice` keeps its allocation address-stable for `arena`.
    let chain = unsafe { ptr::addr_of!((*source).chain) }.cast_mut();
    Ok(WGPUShaderModuleDescriptor {
        nextInChain: chain,
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

fn convert_bind_group_layout_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUBindGroupLayoutDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    let entries_value = E::get_property(cx, value, "entries")?;
    let entries = if E::is_undefined(cx, entries_value) {
        &[][..]
    } else {
        convert_bind_group_layout_entries::<E>(cx, entries_value, arena)?
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

fn convert_pipeline_layout_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUPipelineLayoutDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    let layouts_value = E::get_property(cx, value, "bindGroupLayouts")?;
    let layouts = if E::is_undefined(cx, layouts_value) {
        &[][..]
    } else {
        convert_bind_group_layout_sequence::<E>(cx, layouts_value, arena)?
    };
    Ok(WGPUPipelineLayoutDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        bindGroupLayoutCount: layouts.len(),
        bindGroupLayouts: if layouts.is_empty() {
            ptr::null()
        } else {
            layouts.as_ptr()
        },
        immediateSize: 0,
    })
}

fn convert_bind_group_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedBindGroupDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    let layout = bind_group_layout_handle::<E>(cx, required_member::<E>(cx, value, "layout")?)?;
    let entries_value = E::get_property(cx, value, "entries")?;
    let (entries, buffers) = if E::is_undefined(cx, entries_value) {
        (&[][..], Vec::new())
    } else {
        convert_bind_group_entries::<E>(cx, entries_value, arena)?
    };
    Ok(ConvertedBindGroupDescriptor {
        native: WGPUBindGroupDescriptor {
            nextInChain: ptr::null_mut(),
            label: WGPUStringView::from_bytes(label.as_bytes()),
            layout,
            entryCount: entries.len(),
            entries: if entries.is_empty() {
                ptr::null()
            } else {
                entries.as_ptr()
            },
        },
        layout,
        buffers,
    })
}

fn convert_compute_pipeline_descriptor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<ConvertedComputePipelineDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    let layout_value = E::get_property(cx, value, "layout")?;
    let layout = if E::is_undefined(cx, layout_value) || E::is_null(cx, layout_value) {
        ptr::null_mut()
    } else {
        pipeline_layout_handle::<E>(cx, layout_value)?
    };
    let compute_value = required_member::<E>(cx, value, "compute")?;
    let module = shader_module_handle::<E>(cx, required_member::<E>(cx, compute_value, "module")?)?;
    let entry_point = optional_nullable_string::<E>(cx, compute_value, "entryPoint", arena)?;
    Ok(ConvertedComputePipelineDescriptor {
        native: WGPUComputePipelineDescriptor {
            nextInChain: ptr::null_mut(),
            label: WGPUStringView::from_bytes(label.as_bytes()),
            layout,
            compute: WGPUComputeState {
                nextInChain: ptr::null_mut(),
                module,
                entryPoint: entry_point,
                constantCount: 0,
                constants: ptr::null(),
            },
        },
        module,
        layout,
    })
}

fn convert_command_encoder_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUCommandEncoderDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    Ok(WGPUCommandEncoderDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

fn convert_command_buffer_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUCommandBufferDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    Ok(WGPUCommandBufferDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
    })
}

fn convert_compute_pass_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<WGPUComputePassDescriptor, E::Error> {
    let label = optional_non_null_string::<E>(cx, value, "label", arena)?;
    Ok(WGPUComputePassDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        timestampWrites: ptr::null(),
    })
}

fn convert_buffer_descriptor<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &Arena,
) -> Result<BufferDescriptor, E::Error> {
    let size_value = required_member::<E>(cx, value, "size")?;
    let usage_value = required_member::<E>(cx, value, "usage")?;
    let mapped_value = E::get_property(cx, value, "mappedAtCreation")?;
    let label_value = E::get_property(cx, value, "label")?;
    let label = if E::is_undefined(cx, label_value) {
        ""
    } else {
        E::to_str(cx, label_value, arena)?
    };
    Ok(BufferDescriptor {
        size: enforce_u64::<E>(cx, size_value, "size")?,
        usage: u64::from(enforce_u32::<E>(cx, usage_value, "usage")?),
        mapped_at_creation: if E::is_undefined(cx, mapped_value) {
            false
        } else {
            E::to_bool(cx, mapped_value)
        },
        label: label.to_owned(),
    })
}

fn optional_non_null_string<'a, E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
    arena: &'a Arena,
) -> Result<&'a str, E::Error> {
    let value = E::get_property(cx, obj, name)?;
    if E::is_undefined(cx, value) {
        Ok("")
    } else {
        E::to_str(cx, value, arena)
    }
}

fn optional_nullable_string<E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
    arena: &Arena,
) -> Result<WGPUStringView, E::Error> {
    let value = E::get_property(cx, obj, name)?;
    if E::is_undefined(cx, value) || E::is_null(cx, value) {
        Ok(WGPUStringView {
            data: ptr::null(),
            length: wgpu_strlen(),
        })
    } else {
        Ok(WGPUStringView::from_bytes(
            E::to_str(cx, value, arena)?.as_bytes(),
        ))
    }
}

fn convert_sequence<E: JsEngine, T>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
    mut convert: impl FnMut(E::Value) -> Result<T, E::Error>,
) -> Result<Vec<T>, E::Error> {
    let global = E::global(cx);
    let symbol = E::get_property(cx, global, "Symbol")?;
    let iterator_key = E::get_property(cx, symbol, "iterator")?;
    let iterator_method = E::get_property_value(cx, value, iterator_key)?;
    if E::is_undefined(cx, iterator_method) || E::is_null(cx, iterator_method) {
        return Err(E::type_error(cx, &format!("{name} is not iterable")));
    }

    let iterator = E::call(cx, iterator_method, value, &[])?;
    let next = E::get_property(cx, iterator, "next")?;
    let mut converted = Vec::new();
    loop {
        let result = E::call(cx, next, iterator, &[])?;
        let done = E::get_property(cx, result, "done")?;
        if E::to_bool(cx, done) {
            return Ok(converted);
        }
        let item = E::get_property(cx, result, "value")?;
        converted.push(convert(item)?);
    }
}

fn convert_bind_group_layout_entries<'a, E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &'a Arena,
) -> Result<&'a [WGPUBindGroupLayoutEntry], E::Error> {
    let entries = convert_sequence::<E, _>(cx, value, "entries", |item| {
        convert_bind_group_layout_entry::<E>(cx, item)
    })?;
    Ok(arena.alloc_slice(entries))
}

fn convert_bind_group_layout_entry<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupLayoutEntry, E::Error> {
    let binding = enforce_u32::<E>(cx, required_member::<E>(cx, value, "binding")?, "binding")?;
    let visibility = u64::from(enforce_u32::<E>(
        cx,
        required_member::<E>(cx, value, "visibility")?,
        "visibility",
    )?);
    let buffer_value = E::get_property(cx, value, "buffer")?;
    let buffer = if E::is_undefined(cx, buffer_value) {
        WGPUBufferBindingLayout {
            nextInChain: ptr::null_mut(),
            type_: WGPUBufferBindingType_WGPUBufferBindingType_BindingNotUsed,
            hasDynamicOffset: 0,
            minBindingSize: 0,
        }
    } else {
        convert_buffer_binding_layout::<E>(cx, buffer_value)?
    };
    Ok(WGPUBindGroupLayoutEntry {
        nextInChain: ptr::null_mut(),
        binding,
        visibility,
        bindingArraySize: 0,
        buffer,
        sampler: unsafe { std::mem::zeroed() },
        texture: unsafe { std::mem::zeroed() },
        storageTexture: unsafe { std::mem::zeroed() },
    })
}

fn convert_buffer_binding_layout<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBufferBindingLayout, E::Error> {
    let type_value = E::get_property(cx, value, "type")?;
    let type_ = if E::is_undefined(cx, type_value) {
        WGPUBufferBindingType_WGPUBufferBindingType_Undefined
    } else {
        let enum_arena = Arena::new();
        match E::to_str(cx, type_value, &enum_arena)? {
            "uniform" => WGPUBufferBindingType_WGPUBufferBindingType_Uniform,
            "storage" => WGPUBufferBindingType_WGPUBufferBindingType_Storage,
            "read-only-storage" => WGPUBufferBindingType_WGPUBufferBindingType_ReadOnlyStorage,
            _ => return Err(E::type_error(cx, "GPUBufferBindingType")),
        }
    };
    let dynamic = E::get_property(cx, value, "hasDynamicOffset")?;
    let min = E::get_property(cx, value, "minBindingSize")?;
    Ok(WGPUBufferBindingLayout {
        nextInChain: ptr::null_mut(),
        type_,
        hasDynamicOffset: if E::is_undefined(cx, dynamic) {
            0
        } else {
            u32::from(E::to_bool(cx, dynamic))
        },
        minBindingSize: if E::is_undefined(cx, min) {
            0
        } else {
            enforce_u64::<E>(cx, min, "minBindingSize")?
        },
    })
}

fn convert_bind_group_entries<'a, E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &'a Arena,
) -> Result<(&'a [WGPUBindGroupEntry], Vec<WGPUBuffer>), E::Error> {
    let mut buffers = Vec::new();
    let entries = convert_sequence::<E, _>(cx, value, "entries", |item| {
        let entry = convert_bind_group_entry::<E>(cx, item)?;
        if !entry.buffer.is_null() {
            buffers.push(entry.buffer);
        }
        Ok(entry)
    })?;
    Ok((arena.alloc_slice(entries), buffers))
}

fn convert_bind_group_entry<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupEntry, E::Error> {
    let binding = optional_u32::<E>(
        cx,
        Some(E::get_property(cx, value, "binding")?),
        "binding",
        0,
    )?;
    let resource = required_member::<E>(cx, value, "resource")?;
    let buffer_value = E::get_property(cx, resource, "buffer")?;
    let buffer = if E::is_undefined(cx, buffer_value) {
        buffer_handle::<E>(cx, resource)?
    } else {
        buffer_handle::<E>(cx, buffer_value)?
    };
    let offset = optional_member_u64::<E>(cx, resource, "offset", 0)?;
    let size_value = E::get_property(cx, resource, "size")?;
    let size = if E::is_undefined(cx, size_value) {
        WGPU_WHOLE_SIZE as u64
    } else {
        enforce_u64::<E>(cx, size_value, "size")?
    };
    Ok(WGPUBindGroupEntry {
        nextInChain: ptr::null_mut(),
        binding,
        buffer,
        offset,
        size,
        sampler: ptr::null_mut(),
        textureView: ptr::null_mut(),
    })
}

fn convert_bind_group_layout_sequence<'a, E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
    arena: &'a Arena,
) -> Result<&'a [WGPUBindGroupLayout], E::Error> {
    let layouts = convert_sequence::<E, _>(cx, value, "bindGroupLayouts", |item| {
        bind_group_layout_handle::<E>(cx, item)
    })?;
    Ok(arena.alloc_slice(layouts))
}

fn convert_command_buffer_sequence<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Vec<Arc<Mutex<CommandBufferState>>>, E::Error> {
    convert_sequence::<E, _>(cx, value, "commands", |item| {
        command_buffer_state::<E>(cx, item)
    })
}

fn required_member<E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
) -> Result<E::Value, E::Error> {
    let value = E::get_property(cx, obj, name)?;
    if E::is_undefined(cx, value) {
        Err(E::type_error(cx, name))
    } else {
        Ok(value)
    }
}

fn optional_gpu_size_to_usize<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    name: &'static str,
    default: usize,
) -> Result<usize, E::Error> {
    let Some(value) = value else {
        return Ok(default);
    };
    if E::is_undefined(cx, value) {
        return Ok(default);
    }
    let value = enforce_u64::<E>(cx, value, name)?;
    if value > WEBIDL_U32_MAX {
        return Err(E::type_error(cx, name));
    }
    usize::try_from(value).map_err(|_| E::type_error(cx, name))
}

fn optional_u32<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    name: &'static str,
    default: u32,
) -> Result<u32, E::Error> {
    let Some(value) = value else {
        return Ok(default);
    };
    if E::is_undefined(cx, value) {
        Ok(default)
    } else {
        enforce_u32::<E>(cx, value, name)
    }
}

fn optional_member_u64<E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
    default: u64,
) -> Result<u64, E::Error> {
    let value = E::get_property(cx, obj, name)?;
    if E::is_undefined(cx, value) {
        Ok(default)
    } else {
        enforce_u64::<E>(cx, value, name)
    }
}

fn device_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUDevice, E::Error> {
    E::payload(cx, value, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .map(|payload| payload.device)
        .ok_or_else(|| E::type_error(cx, "GPUDevice method called on an incompatible object"))
}

fn buffer_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBuffer, E::Error> {
    E::payload(cx, value, GPU_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<BufferPayload<E>>())
        .and_then(|payload| payload.state.lock().ok().map(|state| state.buffer))
        .ok_or_else(|| E::type_error(cx, "GPUBuffer is required"))
}

fn shader_module_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUShaderModule, E::Error> {
    E::payload(cx, value, GPU_SHADER_MODULE_CLASS)
        .and_then(|payload| payload.downcast_ref::<ShaderModulePayload>())
        .map(|payload| payload.module)
        .ok_or_else(|| E::type_error(cx, "GPUShaderModule is required"))
}

fn bind_group_layout_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroupLayout, E::Error> {
    E::payload(cx, value, GPU_BIND_GROUP_LAYOUT_CLASS)
        .and_then(|payload| payload.downcast_ref::<BindGroupLayoutPayload>())
        .map(|payload| payload.layout)
        .ok_or_else(|| E::type_error(cx, "GPUBindGroupLayout is required"))
}

fn pipeline_layout_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUPipelineLayout, E::Error> {
    E::payload(cx, value, GPU_PIPELINE_LAYOUT_CLASS)
        .and_then(|payload| payload.downcast_ref::<PipelineLayoutPayload>())
        .map(|payload| payload.layout)
        .ok_or_else(|| E::type_error(cx, "GPUPipelineLayout is required"))
}

fn bind_group_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUBindGroup, E::Error> {
    E::payload(cx, value, GPU_BIND_GROUP_CLASS)
        .and_then(|payload| payload.downcast_ref::<BindGroupPayload>())
        .map(|payload| payload.bind_group)
        .ok_or_else(|| E::type_error(cx, "GPUBindGroup is required"))
}

fn compute_pipeline_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUComputePipeline, E::Error> {
    E::payload(cx, value, GPU_COMPUTE_PIPELINE_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePipelinePayload>())
        .map(|payload| payload.pipeline)
        .ok_or_else(|| E::type_error(cx, "GPUComputePipeline is required"))
}

fn command_buffer_state<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Arc<Mutex<CommandBufferState>>, E::Error> {
    E::payload(cx, value, GPU_COMMAND_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<CommandBufferPayload>())
        .map(|payload| Arc::clone(&payload.state))
        .ok_or_else(|| E::type_error(cx, "GPUCommandBuffer is required"))
}

fn command_encoder_state<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Arc<Mutex<CommandEncoderState>>, E::Error> {
    E::payload(cx, value, GPU_COMMAND_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<CommandEncoderPayload>())
        .map(|payload| Arc::clone(&payload.state))
        .ok_or_else(|| E::type_error(cx, "GPUCommandEncoder is required"))
}

fn live_command_encoder<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUCommandEncoder, E::Error> {
    let state = command_encoder_state::<E>(cx, value)?;
    let state = state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if state.ended {
        Err(E::operation_error(cx, "GPUCommandEncoder is finished"))
    } else {
        Ok(state.encoder)
    }
}

fn live_compute_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUComputePassEncoder, E::Error> {
    let payload = E::payload(cx, value, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "GPUComputePassEncoder is required"))?;
    let state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUComputePassEncoder state is poisoned"))?;
    if state.ended {
        return Err(E::operation_error(cx, "GPUComputePassEncoder is ended"));
    }
    let parent = state
        .parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if parent.ended {
        return Err(E::operation_error(cx, "GPUCommandEncoder is finished"));
    }
    Ok(state.pass)
}

fn enforce_u64<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<u64, E::Error> {
    let number = E::to_f64(cx, value)?;
    if !number.is_finite()
        || number < 0.0
        || number.fract() != 0.0
        || number >= 18_446_744_073_709_551_616.0
    {
        return Err(E::type_error(cx, name));
    }
    Ok(number as u64)
}

fn enforce_u32<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<u32, E::Error> {
    let number = E::to_f64(cx, value)?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number >= 4_294_967_296.0 {
        return Err(E::type_error(cx, name));
    }
    Ok(number as u32)
}

fn with_buffer_state<E, F, R>(cx: E::Context<'_>, this: E::Value, f: F) -> Result<R, E::Error>
where
    E: JsEngine + 'static,
    F: FnOnce(&mut BufferState<E>) -> Result<R, E::Error>,
{
    with_buffer_payload_state(cx, this, |_payload, state| f(state))
}

fn with_buffer_payload_state<E, F, R>(
    cx: E::Context<'_>,
    this: E::Value,
    f: F,
) -> Result<R, E::Error>
where
    E: JsEngine + 'static,
    F: FnOnce(&BufferPayload<E>, &mut BufferState<E>) -> Result<R, E::Error>,
{
    let Some(payload) = E::payload(cx, this, GPU_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<BufferPayload<E>>())
    else {
        return Err(E::type_error(
            cx,
            "GPUBuffer method called on an incompatible object",
        ));
    };
    let Ok(mut state) = payload.state.lock() else {
        return Err(E::operation_error(cx, "GPUBuffer state is poisoned"));
    };
    f(payload, &mut state)
}

fn detach_all_ranges<E: JsEngine>(
    cx: E::Context<'_>,
    payload: &BufferPayload<E>,
    state: &mut BufferState<E>,
    flush: bool,
) -> Result<(), E::Error> {
    let ranges = std::mem::take(&mut state.ranges);
    for range in ranges {
        let mut copy_back = Vec::new();
        let should_copy_back = flush
            && range.strategy == MappedRangeStrategy::CopyInCopyOut
            && range.map_mode == WGPUMapMode_Write;
        let out = if should_copy_back {
            copy_back.resize(range.size, 0);
            Some(copy_back.as_mut_slice())
        } else {
            None
        };
        if let Err(error) = E::detach_arraybuffer(cx, range.value, out) {
            E::release_value(cx, range.value);
            return Err(error);
        }
        let detached = E::arraybuffer_len(cx, range.value) == Some(0);
        if !detached {
            E::release_value(cx, range.value);
            return Err(E::operation_error(cx, "mapped range detach failed"));
        }
        if should_copy_back {
            let ptr = mapped_range_ptr::<E>(cx, state, range.offset, range.size);
            if ptr.is_null() {
                E::release_value(cx, range.value);
                return Err(E::operation_error(cx, "mapped range is unavailable"));
            }
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr.cast::<u8>(), range.size) };
            dst.copy_from_slice(&copy_back);
        }
        E::release_value(cx, range.value);
    }
    payload.traced_values.clear();
    Ok(())
}

fn mapped_range_ptr<E: JsEngine>(
    cx: E::Context<'_>,
    state: &BufferState<E>,
    offset: usize,
    size: usize,
) -> *mut c_void {
    if state.map_mode == WGPUMapMode_Read {
        let ptr = unsafe {
            (E::environment(cx).gpu().buffer_get_const_mapped_range)(state.buffer, offset, size)
        };
        // WebIDL exposes one writable ArrayBuffer for read and write mappings.
        // For read mappings this is the implementation's read-staging memory:
        // script writes are invisible, and `unmap()` copies back only for write
        // mappings, so casting away const here does not create a write-back path.
        ptr.cast_mut()
    } else {
        unsafe { (E::environment(cx).gpu().buffer_get_mapped_range)(state.buffer, offset, size) }
    }
}

fn gpu_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_CLASS, || ClassSpec {
        name: "GPU",
        id: GPU_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([MethodSpec {
            name: "requestAdapter",
            length: 0,
            call: gpu_request_adapter::<E>,
        }])),
        finalizer: |_payload, _env| {},
    })
}

fn adapter_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_ADAPTER_CLASS, || ClassSpec {
        name: "GPUAdapter",
        id: GPU_ADAPTER_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([MethodSpec {
            name: "requestDevice",
            length: 0,
            call: adapter_request_device::<E>,
        }])),
        finalizer: finalize_adapter,
    })
}

fn device_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_DEVICE_CLASS, || ClassSpec {
        name: "GPUDevice",
        id: GPU_DEVICE_CLASS,
        properties: Box::leak(Box::new([PropertySpec {
            name: "queue",
            get: Some(device_queue_get::<E>),
            set: None,
        }])),
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "createBuffer",
                length: 1,
                call: device_create_buffer::<E>,
            },
            MethodSpec {
                name: "createShaderModule",
                length: 1,
                call: device_create_shader_module::<E>,
            },
            MethodSpec {
                name: "createBindGroupLayout",
                length: 1,
                call: device_create_bind_group_layout::<E>,
            },
            MethodSpec {
                name: "createPipelineLayout",
                length: 1,
                call: device_create_pipeline_layout::<E>,
            },
            MethodSpec {
                name: "createBindGroup",
                length: 1,
                call: device_create_bind_group::<E>,
            },
            MethodSpec {
                name: "createComputePipeline",
                length: 1,
                call: device_create_compute_pipeline::<E>,
            },
            MethodSpec {
                name: "createCommandEncoder",
                length: 0,
                call: device_create_command_encoder::<E>,
            },
        ])),
        finalizer: finalize_device::<E>,
    })
}

fn queue_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_QUEUE_CLASS, || ClassSpec {
        name: "GPUQueue",
        id: GPU_QUEUE_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "writeBuffer",
                length: 3,
                call: queue_write_buffer::<E>,
            },
            MethodSpec {
                name: "submit",
                length: 1,
                call: queue_submit::<E>,
            },
            MethodSpec {
                name: "onSubmittedWorkDone",
                length: 0,
                call: queue_on_submitted_work_done::<E>,
            },
        ])),
        finalizer: finalize_queue,
    })
}

fn shader_module_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_SHADER_MODULE_CLASS, || ClassSpec {
        name: "GPUShaderModule",
        id: GPU_SHADER_MODULE_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_shader_module,
    })
}

fn bind_group_layout_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BIND_GROUP_LAYOUT_CLASS, || ClassSpec {
        name: "GPUBindGroupLayout",
        id: GPU_BIND_GROUP_LAYOUT_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_bind_group_layout,
    })
}

fn pipeline_layout_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_PIPELINE_LAYOUT_CLASS, || ClassSpec {
        name: "GPUPipelineLayout",
        id: GPU_PIPELINE_LAYOUT_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_pipeline_layout,
    })
}

fn bind_group_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BIND_GROUP_CLASS, || ClassSpec {
        name: "GPUBindGroup",
        id: GPU_BIND_GROUP_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_bind_group,
    })
}

fn compute_pipeline_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMPUTE_PIPELINE_CLASS, || ClassSpec {
        name: "GPUComputePipeline",
        id: GPU_COMPUTE_PIPELINE_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_compute_pipeline,
    })
}

fn command_encoder_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMMAND_ENCODER_CLASS, || ClassSpec {
        name: "GPUCommandEncoder",
        id: GPU_COMMAND_ENCODER_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "copyBufferToBuffer",
                length: 5,
                call: command_encoder_copy_buffer_to_buffer::<E>,
            },
            MethodSpec {
                name: "beginComputePass",
                length: 0,
                call: command_encoder_begin_compute_pass::<E>,
            },
            MethodSpec {
                name: "finish",
                length: 0,
                call: command_encoder_finish::<E>,
            },
        ])),
        finalizer: finalize_command_encoder,
    })
}

fn command_buffer_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMMAND_BUFFER_CLASS, || ClassSpec {
        name: "GPUCommandBuffer",
        id: GPU_COMMAND_BUFFER_CLASS,
        properties: &[],
        methods: &[],
        finalizer: finalize_command_buffer,
    })
}

fn compute_pass_encoder_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_COMPUTE_PASS_ENCODER_CLASS, || ClassSpec {
        name: "GPUComputePassEncoder",
        id: GPU_COMPUTE_PASS_ENCODER_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "setPipeline",
                length: 1,
                call: compute_pass_set_pipeline::<E>,
            },
            MethodSpec {
                name: "setBindGroup",
                length: 2,
                call: compute_pass_set_bind_group::<E>,
            },
            MethodSpec {
                name: "dispatchWorkgroups",
                length: 1,
                call: compute_pass_dispatch_workgroups::<E>,
            },
            MethodSpec {
                name: "end",
                length: 0,
                call: compute_pass_end::<E>,
            },
        ])),
        finalizer: finalize_compute_pass_encoder,
    })
}

fn buffer_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_BUFFER_CLASS, || ClassSpec {
        name: "GPUBuffer",
        id: GPU_BUFFER_CLASS,
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "label",
                get: Some(buffer_label_get::<E>),
                set: Some(buffer_label_set::<E>),
            },
            PropertySpec {
                name: "size",
                get: Some(buffer_size_get::<E>),
                set: None,
            },
            PropertySpec {
                name: "usage",
                get: Some(buffer_usage_get::<E>),
                set: None,
            },
        ])),
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "destroy",
                length: 0,
                call: buffer_destroy::<E>,
            },
            MethodSpec {
                name: "mapAsync",
                length: 1,
                call: buffer_map_async::<E>,
            },
            MethodSpec {
                name: "getMappedRange",
                length: 0,
                call: buffer_get_mapped_range::<E>,
            },
            MethodSpec {
                name: "unmap",
                length: 0,
                call: buffer_unmap::<E>,
            },
        ])),
        finalizer: finalize_buffer::<E>,
    })
}

fn class_spec_once<E, F>(id: ClassId, init: F) -> &'static ClassSpec<E>
where
    E: JsEngine + 'static,
    F: FnOnce() -> ClassSpec<E>,
{
    // This registry stores leaked `ClassSpec<E>` addresses as type-erased cache
    // entries keyed by `(TypeId, ClassId)`. The `usize` is not a WGPU handle and
    // is never passed to native code; it is cast back only after the matching
    // engine type has been established by `TypeId`.
    static SPECS: OnceLock<Mutex<Vec<(std::any::TypeId, ClassId, usize)>>> = OnceLock::new();
    let type_id = std::any::TypeId::of::<E>();
    let specs = SPECS.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut specs) = specs.lock() else {
        return Box::leak(Box::new(init()));
    };
    if let Some((_, _, ptr)) = specs
        .iter()
        .find(|(existing_type, existing_id, _)| *existing_type == type_id && *existing_id == id)
    {
        // SAFETY: `ptr` came from `Box::leak` for the same `(E, id)` pair in
        // this process and remains valid for the program lifetime.
        return unsafe { &*(*ptr as *const ClassSpec<E>) };
    }
    let spec = Box::leak(Box::new(init()));
    specs.push((type_id, id, spec as *const ClassSpec<E> as usize));
    spec
}

#[cfg(any(test, feature = "mock"))]
pub mod mock;
