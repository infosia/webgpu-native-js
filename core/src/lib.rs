#![warn(missing_docs)]

//! Engine-independent WebGPU binding core.
//!
//! Descriptor conversion and wrapper behavior live here and are generic over
//! [`JsEngine`]. Engine adapters provide object allocation and JavaScript value
//! conversion only.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::ptr;
use std::sync::{Arc, Mutex};

pub use webgpu_native_js_ffi::native::{
    WGPUBool, WGPUBuffer, WGPUBufferDescriptor, WGPUBufferUsage, WGPUDevice, WGPUStringView,
};

/// Result type used by the core crate.
pub type Result<T, E> = std::result::Result<T, E>;

const GPU_BUFFER_CLASS: ClassId = ClassId(1);
const GPU_DEVICE_CLASS: ClassId = ClassId(2);

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
pub struct BufferPayload {
    state: Arc<Mutex<BufferState>>,
}

impl BufferPayload {
    /// Returns the shared buffer state.
    #[must_use]
    pub fn state(&self) -> &Arc<Mutex<BufferState>> {
        &self.state
    }
}

/// Mutable state of a `GPUBuffer` wrapper.
pub struct BufferState {
    buffer: WGPUBuffer,
    parent_device: WGPUDevice,
    size: u64,
    usage: u64,
    label: String,
    destroyed: bool,
}

impl BufferState {
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

unsafe impl Send for BufferPayload {}
unsafe impl Send for BufferState {}

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
    };
    E::new_instance(
        cx,
        GPU_BUFFER_CLASS,
        Box::new(BufferPayload {
            state: Arc::new(Mutex::new(state)),
        }),
    )
}

/// Implements `GPUBuffer.destroy`.
pub fn buffer_destroy<E: JsEngine>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if !state.destroyed {
            unsafe {
                (E::environment(cx).gpu().buffer_destroy)(state.buffer);
            }
            state.destroyed = true;
        }
        Ok(E::undefined(cx))
    })
}

/// Implements the `GPUBuffer.label` getter.
pub fn buffer_label_get<E: JsEngine>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| E::string(cx, &state.label))
}

/// Implements the `GPUBuffer.label` setter.
pub fn buffer_label_set<E: JsEngine>(
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
pub fn buffer_size_get<E: JsEngine>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| E::number(cx, state.size as f64))
}

/// Implements the `GPUBuffer.usage` getter.
pub fn buffer_usage_get<E: JsEngine>(
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

/// Finalizes a `GPUBuffer` payload by enqueuing buffer release and parent release.
pub fn finalize_buffer(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<BufferPayload>() else {
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

fn enforce_u64<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<u64, E::Error> {
    let number = E::to_f64(cx, value)?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > u64::MAX as f64 {
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
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > u32::MAX as f64 {
        return Err(E::type_error(cx, name));
    }
    Ok(number as u32)
}

fn with_buffer_state<E, F, R>(cx: E::Context<'_>, this: E::Value, f: F) -> Result<R, E::Error>
where
    E: JsEngine,
    F: FnOnce(&mut BufferState) -> Result<R, E::Error>,
{
    let Some(payload) = E::payload(cx, this, GPU_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<BufferPayload>())
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

fn device_class<E: JsEngine>() -> &'static ClassSpec<E> {
    Box::leak(Box::new(ClassSpec {
        name: "GPUDevice",
        id: GPU_DEVICE_CLASS,
        properties: &[],
        methods: Box::leak(Box::new([MethodSpec {
            name: "createBuffer",
            length: 1,
            call: device_create_buffer::<E>,
        }])),
        finalizer: finalize_device,
    }))
}

fn buffer_class<E: JsEngine>() -> &'static ClassSpec<E> {
    Box::leak(Box::new(ClassSpec {
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
        methods: Box::leak(Box::new([MethodSpec {
            name: "destroy",
            length: 0,
            call: buffer_destroy::<E>,
        }])),
        finalizer: finalize_buffer,
    }))
}

#[cfg(any(test, feature = "mock"))]
pub mod mock;
