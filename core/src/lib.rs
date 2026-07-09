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
use std::sync::{Arc, Mutex, OnceLock};

pub use webgpu_native_js_ffi::native::{
    WGPUAdapter, WGPUBuffer, WGPUBufferDescriptor, WGPUBufferMapCallbackInfo, WGPUBufferUsage,
    WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents, WGPUDevice, WGPUMapAsyncStatus,
    WGPUMapAsyncStatus_WGPUMapAsyncStatus_Aborted,
    WGPUMapAsyncStatus_WGPUMapAsyncStatus_CallbackCancelled,
    WGPUMapAsyncStatus_WGPUMapAsyncStatus_Error, WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success,
    WGPUMapMode, WGPURequestAdapterCallbackInfo, WGPURequestAdapterStatus,
    WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success, WGPURequestDeviceCallbackInfo,
    WGPURequestDeviceStatus, WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success,
    WGPUStringView, WGPU_WHOLE_MAP_SIZE,
};

/// Result type used by the core crate.
pub type Result<T, E> = std::result::Result<T, E>;

const GPU_BUFFER_CLASS: ClassId = ClassId(1);
const GPU_DEVICE_CLASS: ClassId = ClassId(2);
const GPU_CLASS: ClassId = ClassId(3);
const GPU_ADAPTER_CLASS: ClassId = ClassId(4);
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
    /// `wgpuBufferSetLabel`.
    pub buffer_set_label: unsafe fn(WGPUBuffer, WGPUStringView),
    /// `wgpuBufferDestroy`.
    pub buffer_destroy: unsafe fn(WGPUBuffer),
    /// `wgpuBufferGetMappedRange`.
    pub buffer_get_mapped_range: unsafe fn(WGPUBuffer, usize, usize) -> *mut c_void,
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
}

/// A per-context environment shared by wrapper callbacks.
pub struct Environment {
    gpu: GpuDispatch,
    queue: Arc<ReleaseQueue>,
}

impl Environment {
    /// Creates an environment from WebGPU dispatch functions and a release queue.
    #[must_use]
    pub fn new(gpu: GpuDispatch, queue: Arc<ReleaseQueue>) -> Self {
        Self { gpu, queue }
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
}

/// Per-call bump-style arena for transient conversion data.
#[derive(Default)]
pub struct Arena {
    strings: RefCell<Vec<Box<[u8]>>>,
}

impl Arena {
    /// Creates an empty arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Copies a string into the arena and returns the arena-owned bytes.
    pub fn alloc_str<'a>(&'a self, value: &str) -> &'a str {
        let mut strings = self.strings.borrow_mut();
        strings.push(value.as_bytes().to_vec().into_boxed_slice());
        let Some(bytes) = strings.last() else {
            return "";
        };
        let ptr = bytes.as_ptr();
        let len = bytes.len();
        // SAFETY: the bytes are copied from a valid `str` and stored in `self`.
        // The returned borrow is tied to the arena lifetime.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) }
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
    /// Engine-owned context data that may outlive a single JS callback.
    type AsyncContext: Copy + 'static;

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
    /// Returns true for JavaScript `undefined`.
    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool;
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
    /// Returns an async context token for callbacks that outlive this call.
    fn async_context(cx: Self::Context<'_>) -> Self::AsyncContext;
    /// Reconstructs a call context from an async context token.
    fn context_from_async(cx: Self::AsyncContext) -> Self::Context<'static>;
    /// Creates JavaScript `undefined` from an async context token.
    fn async_undefined(cx: Self::AsyncContext) -> Self::Value;
    /// Creates a rejection reason from an async context token.
    fn async_error_value(cx: Self::AsyncContext, message: &str) -> Self::Value;
    /// Converts an already-created engine error into a rejection value.
    fn error_value_from_error(cx: Self::AsyncContext, error: Self::Error) -> Self::Value;
    /// Creates a promise and its owned deferred resolving functions.
    fn new_promise(cx: Self::Context<'_>) -> Result<(Self::Value, Deferred<Self>), Self::Error>;
    /// Settles a deferred promise. This consumes the resolving functions.
    fn settle_deferred(
        cx: Self::AsyncContext,
        deferred: Deferred<Self>,
        result: std::result::Result<Self::Value, Self::Value>,
    );
    /// Creates a script-visible ArrayBuffer over external memory.
    fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        ptr: *mut u8,
        len: usize,
    ) -> Result<Self::Value, Self::Error>;
    /// Creates a script-visible ArrayBuffer by copying bytes.
    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> Result<Self::Value, Self::Error>;
    /// Detaches a script-visible ArrayBuffer.
    fn detach_arraybuffer(cx: Self::Context<'_>, value: Self::Value);
    /// Reads an ArrayBuffer byte length through the engine API.
    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize>;
    /// Copies an ArrayBuffer's bytes into `dst`.
    fn arraybuffer_copy_to(cx: Self::Context<'_>, value: Self::Value, dst: &mut [u8]) -> bool;
    /// Duplicates a value so core can hold it beyond the current call.
    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value;
    /// Releases a value previously duplicated for core.
    fn release_value(cx: Self::Context<'_>, value: Self::Value);
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
}

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
}

/// Payload stored by a `GPUDevice` wrapper.
pub struct DevicePayload {
    device: WGPUDevice,
}

impl DevicePayload {
    /// Returns the native device handle.
    #[must_use]
    pub fn device(&self) -> WGPUDevice {
        self.device
    }
}

unsafe impl Send for DevicePayload {}

/// Payload stored by a `GPUBuffer` wrapper.
pub struct BufferPayload<E: JsEngine> {
    state: Arc<Mutex<BufferState<E>>>,
}

impl<E: JsEngine> BufferPayload<E> {
    /// Returns the shared buffer state.
    #[must_use]
    pub fn state(&self) -> &Arc<Mutex<BufferState<E>>> {
        &self.state
    }
}

/// Mutable state of a `GPUBuffer` wrapper.
pub struct BufferState<E: JsEngine> {
    buffer: WGPUBuffer,
    parent_device: WGPUDevice,
    size: u64,
    usage: u64,
    label: String,
    destroyed: bool,
    mapped: bool,
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

unsafe impl<E: JsEngine> Send for BufferPayload<E> {}
unsafe impl<E: JsEngine> Send for BufferState<E> {}

/// Payload stored by a `GPU` wrapper.
pub struct GpuPayload {
    instance: webgpu_native_js_ffi::native::WGPUInstance,
}

unsafe impl Send for GpuPayload {}

/// Payload stored by a `GPUAdapter` wrapper.
pub struct AdapterPayload {
    adapter: WGPUAdapter,
}

unsafe impl Send for AdapterPayload {}

#[derive(Clone, Copy)]
struct MappedRange<E: JsEngine> {
    value: E::Value,
    offset: usize,
    size: usize,
    strategy: MappedRangeStrategy,
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
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn wrap_device<E: JsEngine + 'static>(
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
    E::new_instance(cx, GPU_DEVICE_CLASS, Box::new(DevicePayload { device }))
}

/// Implements `GPUDevice.createBuffer`.
pub fn device_create_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(device_payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload>())
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
        ranges: Vec::new(),
    };
    E::new_instance(
        cx,
        GPU_BUFFER_CLASS,
        Box::new(BufferPayload::<E> {
            state: Arc::new(Mutex::new(state)),
        }),
    )
}

/// Implements `GPUBuffer.destroy`.
pub fn buffer_destroy<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if !state.destroyed {
            detach_all_ranges::<E>(cx, state)?;
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
    let request = Box::new(AdapterRequest::<E> {
        async_cx: E::async_context(cx),
        deferred,
    });
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
    let request = Box::new(DeviceRequest::<E> {
        async_cx: E::async_context(cx),
        deferred,
    });
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
    if mode > WEBIDL_U32_MAX {
        return Err(E::type_error(cx, "mode"));
    }
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
    let request = Box::new(MapRequest::<E> {
        async_cx: E::async_context(cx),
        deferred,
        state,
    });
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
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if state.destroyed || !state.mapped {
            return Err(E::operation_error(cx, "buffer is not mapped"));
        }
        let size = match args.get(1).copied() {
            Some(value) if !E::is_undefined(cx, value) => {
                optional_gpu_size_to_usize::<E>(cx, Some(value), "size", 0)?
            }
            _ => state
                .size
                .checked_sub(offset as u64)
                .and_then(|len| usize::try_from(len).ok())
                .filter(|len| *len <= u32::MAX as usize)
                .ok_or_else(|| E::type_error(cx, "size"))?,
        };
        let ptr = unsafe {
            (E::environment(cx).gpu().buffer_get_mapped_range)(state.buffer, offset, size)
        };
        if ptr.is_null() {
            return Err(E::operation_error(
                cx,
                "wgpuBufferGetMappedRange returned null",
            ));
        }
        let value = match E::MAPPED_RANGE_STRATEGY {
            MappedRangeStrategy::ZeroCopyDetach => {
                E::new_external_arraybuffer(cx, ptr.cast::<u8>(), size)?
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
        });
        Ok(value)
    })
}

/// Implements `GPUBuffer.unmap`.
pub fn buffer_unmap<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if state.destroyed {
            return Ok(E::undefined(cx));
        }
        if state.mapped {
            detach_all_ranges::<E>(cx, state)?;
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

/// Finalizes a `GPUDevice` payload by enqueuing its release.
pub fn finalize_device(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<DevicePayload>() else {
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
    async_cx: E::AsyncContext,
    deferred: Deferred<E>,
}

struct DeviceRequest<E: JsEngine + 'static> {
    async_cx: E::AsyncContext,
    deferred: Deferred<E>,
}

struct MapRequest<E: JsEngine + 'static> {
    async_cx: E::AsyncContext,
    deferred: Deferred<E>,
    state: Arc<Mutex<BufferState<E>>>,
}

unsafe extern "C" fn request_adapter_callback<E: JsEngine + 'static>(
    status: WGPURequestAdapterStatus,
    adapter: WGPUAdapter,
    _message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<AdapterRequest<E>>()) else {
            return;
        };
        let request = unsafe { Box::from_raw(raw.as_ptr()) };
        if status == WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success && !adapter.is_null()
        {
            let cx = E::context_from_async(request.async_cx);
            let value =
                E::new_instance(cx, GPU_ADAPTER_CLASS, Box::new(AdapterPayload { adapter }));
            match value {
                Ok(value) => E::settle_deferred(request.async_cx, request.deferred, Ok(value)),
                Err(reason) => {
                    let reason = E::error_value_from_error(request.async_cx, reason);
                    E::settle_deferred(request.async_cx, request.deferred, Err(reason));
                }
            }
        } else {
            let reason = E::async_error_value(request.async_cx, "requestAdapter failed");
            E::settle_deferred(request.async_cx, request.deferred, Err(reason));
        }
    }));
}

unsafe extern "C" fn request_device_callback<E: JsEngine + 'static>(
    status: WGPURequestDeviceStatus,
    device: WGPUDevice,
    _message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<DeviceRequest<E>>()) else {
            return;
        };
        let request = unsafe { Box::from_raw(raw.as_ptr()) };
        if status == WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success && !device.is_null() {
            let cx = E::context_from_async(request.async_cx);
            let value = E::new_instance(cx, GPU_DEVICE_CLASS, Box::new(DevicePayload { device }));
            match value {
                Ok(value) => E::settle_deferred(request.async_cx, request.deferred, Ok(value)),
                Err(reason) => {
                    let reason = E::error_value_from_error(request.async_cx, reason);
                    E::settle_deferred(request.async_cx, request.deferred, Err(reason));
                }
            }
        } else {
            let reason = E::async_error_value(request.async_cx, "requestDevice failed");
            E::settle_deferred(request.async_cx, request.deferred, Err(reason));
        }
    }));
}

unsafe extern "C" fn buffer_map_callback<E: JsEngine + 'static>(
    status: WGPUMapAsyncStatus,
    _message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<MapRequest<E>>()) else {
            return;
        };
        let request = unsafe { Box::from_raw(raw.as_ptr()) };
        if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success {
            if let Ok(mut state) = request.state.lock() {
                state.mapped = true;
            }
            let value = E::async_undefined(request.async_cx);
            E::settle_deferred(request.async_cx, request.deferred, Ok(value));
        } else {
            let reason = if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Error {
                "mapAsync error"
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Aborted {
                "mapAsync aborted"
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_CallbackCancelled {
                "mapAsync callback cancelled"
            } else {
                "mapAsync failed"
            };
            let reason = E::async_error_value(request.async_cx, reason);
            E::settle_deferred(request.async_cx, request.deferred, Err(reason));
        }
    }));
}

#[derive(Debug, Eq, PartialEq)]
struct BufferDescriptor {
    size: u64,
    usage: u64,
    mapped_at_creation: bool,
    label: String,
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
    f(&mut state)
}

fn detach_all_ranges<E: JsEngine>(
    cx: E::Context<'_>,
    state: &mut BufferState<E>,
) -> Result<(), E::Error> {
    let ranges = std::mem::take(&mut state.ranges);
    for range in ranges {
        if range.strategy == MappedRangeStrategy::CopyInCopyOut {
            let ptr = unsafe {
                (E::environment(cx).gpu().buffer_get_mapped_range)(
                    state.buffer,
                    range.offset,
                    range.size,
                )
            };
            if ptr.is_null() {
                return Err(E::operation_error(cx, "mapped range is unavailable"));
            }
            let dst = unsafe { std::slice::from_raw_parts_mut(ptr.cast::<u8>(), range.size) };
            if !E::arraybuffer_copy_to(cx, range.value, dst) {
                return Err(E::operation_error(cx, "mapped range copy-back failed"));
            }
        }
        E::detach_arraybuffer(cx, range.value);
        let detached = E::arraybuffer_len(cx, range.value) == Some(0);
        E::release_value(cx, range.value);
        if !detached {
            return Err(E::operation_error(cx, "mapped range detach failed"));
        }
    }
    Ok(())
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
        properties: &[],
        methods: Box::leak(Box::new([MethodSpec {
            name: "createBuffer",
            length: 1,
            call: device_create_buffer::<E>,
        }])),
        finalizer: finalize_device,
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
        return unsafe { &*(*ptr as *const ClassSpec<E>) };
    }
    let spec = Box::leak(Box::new(init()));
    specs.push((type_id, id, spec as *const ClassSpec<E> as usize));
    spec
}

#[cfg(any(test, feature = "mock"))]
pub mod mock;
