#![warn(missing_docs)]

//! Engine-independent WebGPU binding core.
//!
//! Descriptor conversion and wrapper behavior live here and are generic over
//! [`JsEngine`]. Engine adapters provide object allocation and JavaScript value
//! conversion only.

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Weak;
use std::sync::{Arc, Mutex, OnceLock};

pub use webgpu_native_js_ffi::native::*;

/// Native pipeline parent retained by a pipeline-derived bind-group layout.
#[derive(Clone, Copy)]
pub enum PipelineParent {
    /// Compute-pipeline parent.
    Compute(WGPUComputePipeline),
    /// Render-pipeline parent.
    Render(WGPURenderPipeline),
}

#[macro_use]
mod generated {
    use super::*;

    include!(concat!(env!("OUT_DIR"), "/generated_conversions.rs"));
}

use generated::*;

/// Result type used by the core crate.
pub type Result<T, E> = std::result::Result<T, E>;

/// Engine-neutral argument passed to a host-registered JavaScript function.
#[derive(Clone, Debug, PartialEq)]
pub enum HostValue {
    /// A JavaScript string, or the string coercion of a non-primitive value.
    String(String),
    /// A JavaScript number.
    Number(f64),
    /// A JavaScript boolean.
    Bool(bool),
    /// JavaScript `null`.
    Null,
    /// JavaScript `undefined`.
    Undefined,
}

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
const GPU_SAMPLER_CLASS: ClassId = ClassId(14);
const GPU_ERROR_CLASS: ClassId = ClassId(15);
const GPU_VALIDATION_ERROR_CLASS: ClassId = ClassId(16);
const GPU_OUT_OF_MEMORY_ERROR_CLASS: ClassId = ClassId(17);
const GPU_INTERNAL_ERROR_CLASS: ClassId = ClassId(18);
const GPU_DEVICE_LOST_INFO_CLASS: ClassId = ClassId(19);
const GPU_TEXTURE_CLASS: ClassId = ClassId(20);
const GPU_TEXTURE_VIEW_CLASS: ClassId = ClassId(21);
const GPU_RENDER_PIPELINE_CLASS: ClassId = ClassId(22);
const GPU_RENDER_PASS_ENCODER_CLASS: ClassId = ClassId(23);
const GPU_SUPPORTED_LIMITS_CLASS: ClassId = ClassId(24);
const GPU_ADAPTER_INFO_CLASS: ClassId = ClassId(25);
const GPU_QUERY_SET_CLASS: ClassId = ClassId(26);
const GPU_RENDER_BUNDLE_ENCODER_CLASS: ClassId = ClassId(27);
const GPU_RENDER_BUNDLE_CLASS: ClassId = ClassId(28);
const EVENT_TARGET_CLASS: ClassId = ClassId(29);
const EVENT_CLASS: ClassId = ClassId(30);
const GPU_UNCAPTURED_ERROR_EVENT_CLASS: ClassId = ClassId(31);
const DOM_EXCEPTION_CLASS: ClassId = ClassId(32);
const GPU_PIPELINE_ERROR_CLASS: ClassId = ClassId(33);
const WEBIDL_U32_MAX: u64 = u32::MAX as u64;
const WGPU_DEPTH_CLEAR_VALUE_UNDEFINED: f32 = f32::NAN;

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

pub use generated::{
    device_create_bind_group, device_create_bind_group_layout, device_create_command_encoder,
    device_create_compute_pipeline, device_create_pipeline_layout, device_create_query_set,
    device_create_render_bundle_encoder, device_create_render_pipeline, device_create_sampler,
    device_create_shader_module, device_create_texture, finalize_bind_group,
    finalize_bind_group_layout, finalize_command_encoder, finalize_compute_pipeline,
    finalize_pipeline_layout, finalize_query_set, finalize_render_bundle_encoder,
    finalize_render_pipeline, finalize_sampler, finalize_shader_module, finalize_texture,
    finalize_texture_view, query_set_count_get, query_set_label_get, query_set_label_set,
    query_set_type_get, sampler_label_get, sampler_label_set, texture_create_view,
    texture_depth_or_array_layers_get, texture_dimension_get, texture_format_get,
    texture_height_get, texture_mip_level_count_get, texture_sample_count_get, texture_usage_get,
    texture_width_get, BindGroupLayoutPayload, BindGroupPayload, CommandEncoderPayload,
    ComputePipelinePayload, GpuDispatch, PipelineLayoutPayload, QuerySetPayload, ReleaseRequest,
    RenderBundleEncoderPayload, RenderPipelinePayload, SamplerPayload, ShaderModulePayload,
    TexturePayload, TextureViewPayload,
};
/// A per-context environment shared by wrapper callbacks.
pub struct Environment {
    gpu: GpuDispatch,
    queue: Arc<ReleaseQueue>,
    settlements: Arc<SettlementQueue>,
    device_events: Arc<DeviceEventRegistry>,
    namespace_globals_installed: AtomicBool,
}

#[derive(Default)]
struct DeviceEventRegistry {
    states: Mutex<BTreeMap<usize, Vec<Arc<dyn Any + Send + Sync>>>>,
}

/// Cloneable, thread-safe producer handle for adopted-device events.
#[derive(Clone)]
pub struct DeviceEventForwarder {
    registry: Arc<DeviceEventRegistry>,
}

impl Environment {
    /// Creates an environment from WebGPU dispatch functions and a release queue.
    #[must_use]
    pub fn new(gpu: GpuDispatch, queue: Arc<ReleaseQueue>) -> Self {
        Self {
            gpu,
            queue,
            settlements: Arc::new(SettlementQueue::new()),
            device_events: Arc::new(DeviceEventRegistry::default()),
            namespace_globals_installed: AtomicBool::new(false),
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

    /// Returns a Send + Sync producer handle for adopted-device events.
    #[must_use]
    pub fn device_event_forwarder(&self) -> DeviceEventForwarder {
        DeviceEventForwarder {
            registry: Arc::clone(&self.device_events),
        }
    }

    /// Releases engine values retained by registered device event states.
    pub fn release_device_event_values<E: JsEngine + 'static>(&self, cx: E::Context<'_>) {
        let states = self
            .device_events
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .flatten()
            .filter_map(|events| Arc::clone(events).downcast::<DeviceEventState<E>>().ok())
            .collect::<Vec<_>>();
        let values = states
            .into_iter()
            .flat_map(|state| state.take_engine_values())
            .collect::<Vec<_>>();
        for value in values {
            E::release_value(cx, value);
        }
    }

    fn register_device_events<E: JsEngine + 'static>(
        &self,
        device: WGPUDevice,
        events: Arc<DeviceEventState<E>>,
    ) {
        events.set_registration(device, Arc::downgrade(&self.device_events));
        self.device_events
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(device as usize)
            .or_default()
            .push(events);
    }
}

impl DeviceEventForwarder {
    fn device_events<E: JsEngine + 'static>(
        &self,
        device: WGPUDevice,
    ) -> Vec<Arc<DeviceEventState<E>>> {
        self.registry
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(device as usize))
            .into_iter()
            .flatten()
            .filter_map(|events| Arc::clone(events).downcast::<DeviceEventState<E>>().ok())
            .collect()
    }

    /// Enqueues an uncaptured error for every wrapper of an adopted device.
    ///
    /// The registry mutex is deliberately blocking: producers wait for a
    /// concurrent registration or prune instead of dropping an event.
    pub fn forward_uncaptured_error<E: JsEngine + 'static>(
        &self,
        device: WGPUDevice,
        type_: WGPUErrorType,
        message: impl Into<String>,
    ) -> std::result::Result<(), QueueError> {
        if type_ != WGPUErrorType_WGPUErrorType_Validation
            && type_ != WGPUErrorType_WGPUErrorType_OutOfMemory
            && type_ != WGPUErrorType_WGPUErrorType_Internal
            && type_ != WGPUErrorType_WGPUErrorType_Unknown
        {
            return Err(QueueError::InvalidUncapturedErrorType(type_));
        }
        let states = self.device_events::<E>(device);
        if states.is_empty() {
            return Err(QueueError::UnknownDevice);
        }
        let message = message.into();
        for state in states {
            state.enqueue_uncaptured(type_, message.clone())?;
        }
        Ok(())
    }

    /// Enqueues device loss for every wrapper of an adopted device.
    ///
    /// The registry mutex is deliberately blocking: producers wait for a
    /// concurrent registration or prune instead of dropping an event.
    pub fn forward_device_lost<E: JsEngine + 'static>(
        &self,
        device: WGPUDevice,
        reason: WGPUDeviceLostReason,
        message: impl Into<String>,
    ) -> std::result::Result<(), QueueError> {
        let states = self.device_events::<E>(device);
        if states.is_empty() {
            return Err(QueueError::UnknownDevice);
        }
        let message = message.into();
        for state in states {
            state.mark_lost();
            state.enqueue_lost(reason, message.clone())?;
        }
        Ok(())
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
    /// Returns the binding environment associated with a context.
    fn environment<'a>(cx: Self::Context<'a>) -> &'a Environment;
    /// Gets an object property.
    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> Result<Self::Value, Self::Error>;
    /// Returns an object's own enumerable string-keyed property names.
    fn own_property_names(
        cx: Self::Context<'_>,
        obj: Self::Value,
    ) -> Result<Vec<String>, Self::Error>;
    /// Returns the engine's global object as a call-scoped owned value.
    fn global(cx: Self::Context<'_>) -> Self::Value;
    /// Creates an ordinary JavaScript object whose prototype is `Object.prototype`.
    fn new_object(cx: Self::Context<'_>) -> Result<Self::Value, Self::Error>;
    /// Defines an own data property with the supplied WebIDL descriptor attributes.
    fn define_data_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
        value: Self::Value,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Result<(), Self::Error>;
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
    /// Calls a JavaScript constructor with the provided arguments.
    fn construct(
        cx: Self::Context<'_>,
        ctor: Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// Returns true for JavaScript `undefined`.
    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Returns true for JavaScript `null`.
    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Returns true for a JavaScript object (including callable objects).
    fn is_object(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Returns true when a JavaScript value is callable.
    fn is_callable(cx: Self::Context<'_>, value: Self::Value) -> bool;
    /// Returns true when two JavaScript values have the same identity/value.
    fn same_value(cx: Self::Context<'_>, left: Self::Value, right: Self::Value) -> bool;
    /// Returns true only for a `Uint32Array` view.
    fn is_uint32array(cx: Self::Context<'_>, value: Self::Value) -> bool;
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
    /// Creates a payload-carrying instance with engine-native Error stack behavior.
    fn new_error_instance(
        cx: Self::Context<'_>,
        class: ClassId,
        payload: Box<dyn Any + Send>,
        name: &str,
        message: &str,
    ) -> Result<Self::Value, Self::Error>;
    /// Returns an object's payload when it belongs to the requested class.
    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: ClassId,
    ) -> Option<&'a (dyn Any + Send)>;
    /// Creates a JavaScript `undefined` value.
    fn undefined(cx: Self::Context<'_>) -> Self::Value;
    /// Creates a JavaScript `null` value.
    fn null(cx: Self::Context<'_>) -> Self::Value;
    /// Creates a JavaScript number value.
    fn number(cx: Self::Context<'_>, value: f64) -> Result<Self::Value, Self::Error>;
    /// Creates a JavaScript boolean value.
    fn boolean(cx: Self::Context<'_>, value: bool) -> Self::Value;
    /// Creates a JavaScript string value.
    fn string(cx: Self::Context<'_>, value: &str) -> Result<Self::Value, Self::Error>;
    /// Creates a synchronous JavaScript type error.
    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error;
    /// Creates a synchronous JavaScript operation error.
    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error;
    /// Creates a synchronous JavaScript range error.
    fn range_error(cx: Self::Context<'_>, message: &str) -> Self::Error;
    /// Creates a named rejection error object from a scoped context.
    fn async_error_value(cx: Self::Context<'_>, name: &str, message: &str) -> Self::Value;
    /// Converts an already-created engine error into a rejection value.
    fn error_value_from_error(cx: Self::Context<'_>, error: Self::Error) -> Self::Value;
    /// Creates a promise and its owned deferred resolving functions.
    fn new_promise(cx: Self::Context<'_>) -> Result<(Self::Value, Deferred<Self>), Self::Error>;
    /// Settles a batch of deferred promises inside one JavaScript frame.
    fn settle_deferreds(
        cx: Self::Context<'_>,
        settlements: Vec<DeferredSettlement<Self>>,
    ) -> Result<(), Self::Error>;
    /// Drains engine microtasks scheduled by promise settlement.
    fn drain_microtasks(cx: Self::Context<'_>) -> Result<(), Self::Error>;
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
        events: Arc<DeviceEventState<E>>,
        label: String,
    },
    ComputePipeline {
        deferred: Deferred<E>,
        pipeline: WGPUComputePipeline,
        status: WGPUCreatePipelineAsyncStatus,
        message: String,
        state: Arc<DeviceEventState<E>>,
        lost_at_start: bool,
        module: WGPUShaderModule,
        layout: WGPUPipelineLayout,
        label: String,
        queue: Arc<ReleaseQueue>,
        gpu: GpuDispatch,
    },
    RenderPipeline {
        deferred: Deferred<E>,
        pipeline: WGPURenderPipeline,
        status: WGPUCreatePipelineAsyncStatus,
        message: String,
        state: Arc<DeviceEventState<E>>,
        lost_at_start: bool,
        vertex_module: WGPUShaderModule,
        fragment_module: WGPUShaderModule,
        layout: WGPUPipelineLayout,
        label: String,
        queue: Arc<ReleaseQueue>,
        gpu: GpuDispatch,
    },
    Success {
        deferred: Deferred<E>,
    },
    Error {
        deferred: Deferred<E>,
        name: &'static str,
        message: String,
    },
    StartMap {
        buffer: WGPUBuffer,
        mode: WGPUMapMode,
        offset: usize,
        size: usize,
        request: Box<MapRequest<E>>,
        gpu: GpuDispatch,
    },
    PopErrorScope {
        deferred: Deferred<E>,
        status: WGPUPopErrorScopeStatus,
        type_: WGPUErrorType,
        message: String,
        synthetic_error: Option<SyntheticDeviceError>,
        state: Arc<DeviceEventState<E>>,
    },
    UncapturedError {
        state: Arc<DeviceEventState<E>>,
        type_: WGPUErrorType,
        message: String,
    },
    DeviceLost {
        state: Arc<DeviceEventState<E>>,
        reason: WGPUDeviceLostReason,
        message: String,
    },
}

// SAFETY: settlement requests are created by pure-Rust callbacks and are only
// drained by the engine-thread `tick()`. Engine values inside `Deferred` and
// `DeviceEventState` are moved as opaque tokens and never dereferenced off that
// thread. Every callback copies its WGPUStringView into an owned message String
// before enqueueing; the queue records therefore own all cross-thread text, and
// the C callbacks never touch an engine or call any webgpu.h function.
unsafe impl<E: JsEngine + 'static> Send for SettlementRequest<E> {}

enum SettlementOutcome<E: JsEngine + 'static> {
    Deferred(DeferredSettlement<E>),
    Retry(Box<SettlementRequest<E>>),
    UncapturedError {
        state: Arc<DeviceEventState<E>>,
        type_: WGPUErrorType,
        message: String,
    },
    None,
}

impl<E: JsEngine + 'static> SettlementRequest<E> {
    fn prepare(mut self, cx: E::Context<'_>) -> SettlementOutcome<E> {
        match self {
            Self::Adapter {
                deferred,
                ref mut native,
            } => {
                let adapter = native.take_adapter();
                let value = E::new_instance(
                    cx,
                    GPU_ADAPTER_CLASS,
                    Box::new(AdapterPayload::<E>::new(adapter)),
                );
                SettlementOutcome::Deferred(match value {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => {
                        let _ = native.queue.enqueue(ReleaseRequest::Adapter {
                            adapter,
                            gpu: native.gpu,
                        });
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                })
            }
            Self::Device {
                deferred,
                ref mut native,
                events,
                label,
            } => {
                let device = native.take_device();
                if let Err(error) = events.initialize(cx) {
                    events.release_after_failed_wrap(cx);
                    let _ = native.queue.enqueue(ReleaseRequest::Device {
                        device,
                        gpu: native.gpu,
                    });
                    return SettlementOutcome::Deferred((
                        deferred,
                        Err(E::error_value_from_error(cx, error)),
                    ));
                }
                let value = E::new_instance(
                    cx,
                    GPU_DEVICE_CLASS,
                    Box::new(DevicePayload::<E>::new(device, Arc::clone(&events), label)),
                );
                SettlementOutcome::Deferred(match value {
                    Ok(value) => {
                        E::environment(cx).register_device_events(device, events);
                        (deferred, Ok(value))
                    }
                    Err(error) => {
                        events.release_after_failed_wrap(cx);
                        let _ = native.queue.enqueue(ReleaseRequest::Device {
                            device,
                            gpu: native.gpu,
                        });
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                })
            }
            Self::ComputePipeline {
                deferred,
                pipeline,
                status,
                message,
                state,
                lost_at_start,
                module,
                layout,
                label,
                queue,
                gpu,
            } => {
                if state.is_lost() && !lost_at_start && !state.is_lost_settled() {
                    return SettlementOutcome::Retry(Box::new(Self::ComputePipeline {
                        deferred,
                        pipeline,
                        status,
                        message,
                        state,
                        lost_at_start,
                        module,
                        layout,
                        label,
                        queue,
                        gpu,
                    }));
                }
                if (status != WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_Success
                    || pipeline.is_null())
                    && !(state.is_lost() && (lost_at_start || state.is_lost_settled()))
                {
                    enqueue_compute_pipeline_release(&queue, pipeline, module, layout, gpu);
                    let reason = if status
                        == WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_ValidationError
                    {
                        PipelineErrorReason::Validation
                    } else {
                        PipelineErrorReason::Internal
                    };
                    let message = if message.is_empty() {
                        "createComputePipelineAsync failed".to_owned()
                    } else {
                        message
                    };
                    let rejection = match new_gpu_pipeline_error::<E>(cx, message, reason) {
                        Ok(error) => error,
                        Err(error) => E::error_value_from_error(cx, error),
                    };
                    return SettlementOutcome::Deferred((deferred, Err(rejection)));
                }
                let value = E::new_instance(
                    cx,
                    GPU_COMPUTE_PIPELINE_CLASS,
                    Box::new(ComputePipelinePayload {
                        pipeline,
                        module,
                        layout,
                        label: Mutex::new(label),
                    }),
                );
                SettlementOutcome::Deferred(match value {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => {
                        enqueue_compute_pipeline_release(&queue, pipeline, module, layout, gpu);
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                })
            }
            Self::RenderPipeline {
                deferred,
                pipeline,
                status,
                message,
                state,
                lost_at_start,
                vertex_module,
                fragment_module,
                layout,
                label,
                queue,
                gpu,
            } => {
                if state.is_lost() && !lost_at_start && !state.is_lost_settled() {
                    return SettlementOutcome::Retry(Box::new(Self::RenderPipeline {
                        deferred,
                        pipeline,
                        status,
                        message,
                        state,
                        lost_at_start,
                        vertex_module,
                        fragment_module,
                        layout,
                        label,
                        queue,
                        gpu,
                    }));
                }
                if (status != WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_Success
                    || pipeline.is_null())
                    && !(state.is_lost() && (lost_at_start || state.is_lost_settled()))
                {
                    enqueue_render_pipeline_release(
                        &queue,
                        pipeline,
                        vertex_module,
                        fragment_module,
                        layout,
                        gpu,
                    );
                    let reason = if status
                        == WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_ValidationError
                    {
                        PipelineErrorReason::Validation
                    } else {
                        PipelineErrorReason::Internal
                    };
                    let message = if message.is_empty() {
                        "createRenderPipelineAsync failed".to_owned()
                    } else {
                        message
                    };
                    let rejection = match new_gpu_pipeline_error::<E>(cx, message, reason) {
                        Ok(error) => error,
                        Err(error) => E::error_value_from_error(cx, error),
                    };
                    return SettlementOutcome::Deferred((deferred, Err(rejection)));
                }
                let value = E::new_instance(
                    cx,
                    GPU_RENDER_PIPELINE_CLASS,
                    Box::new(RenderPipelinePayload {
                        render_pipeline: pipeline,
                        vertex_module,
                        fragment_module,
                        layout,
                        label: Mutex::new(label),
                    }),
                );
                SettlementOutcome::Deferred(match value {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => {
                        enqueue_render_pipeline_release(
                            &queue,
                            pipeline,
                            vertex_module,
                            fragment_module,
                            layout,
                            gpu,
                        );
                        (deferred, Err(E::error_value_from_error(cx, error)))
                    }
                })
            }
            Self::Success { deferred } => {
                SettlementOutcome::Deferred((deferred, Ok(E::undefined(cx))))
            }
            Self::Error {
                deferred,
                name,
                message,
            } => {
                // S8: OperationError/AbortError are specified as DOMExceptions;
                // this binding records the deviation and creates a named Error.
                SettlementOutcome::Deferred((
                    deferred,
                    Err(E::async_error_value(cx, name, &message)),
                ))
            }
            Self::StartMap {
                buffer,
                mode,
                offset,
                size,
                request,
                gpu,
            } => {
                start_buffer_map(buffer, mode, offset, size, request, gpu);
                SettlementOutcome::None
            }
            Self::PopErrorScope {
                deferred,
                status,
                type_,
                message,
                synthetic_error,
                state,
            } => {
                if status != WGPUPopErrorScopeStatus_WGPUPopErrorScopeStatus_Success {
                    // S8: WebGPU specifies a DOMException here. This binding's
                    // recorded deviation is a plain Error carrying name/message.
                    let message = if message.is_empty() {
                        "popErrorScope failed".to_owned()
                    } else {
                        format!("popErrorScope failed: {message}")
                    };
                    return SettlementOutcome::Deferred((
                        deferred,
                        Err(E::async_error_value(cx, "OperationError", &message)),
                    ));
                }
                if state.is_lost() {
                    return SettlementOutcome::Deferred((deferred, Ok(E::null(cx))));
                }
                let (type_, message) = if type_ == WGPUErrorType_WGPUErrorType_NoError {
                    synthetic_error.map_or((type_, message), |error| (error.type_, error.message))
                } else {
                    (type_, message)
                };
                if type_ == WGPUErrorType_WGPUErrorType_NoError {
                    return SettlementOutcome::Deferred((deferred, Ok(E::null(cx))));
                }
                SettlementOutcome::Deferred(match new_gpu_error::<E>(cx, type_, message) {
                    Ok(value) => (deferred, Ok(value)),
                    Err(error) => (deferred, Err(E::error_value_from_error(cx, error))),
                })
            }
            Self::UncapturedError {
                state,
                type_,
                message,
            } => {
                if state.is_lost() {
                    SettlementOutcome::None
                } else {
                    SettlementOutcome::UncapturedError {
                        state,
                        type_,
                        message,
                    }
                }
            }
            Self::DeviceLost {
                state,
                reason,
                message,
            } => {
                state.mark_lost_settled();
                let mut state = state
                    .js
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(js) = state.as_mut() else {
                    return SettlementOutcome::None;
                };
                let Some(deferred) = js.lost_deferred.take() else {
                    return SettlementOutcome::None;
                };
                js.lost_registration.take();
                drop(state);
                let value = new_device_lost_info::<E>(cx, reason, message)
                    .map_err(|error| E::error_value_from_error(cx, error));
                SettlementOutcome::Deferred((deferred, value))
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
    ) -> std::result::Result<usize, TickError<E::Error>> {
        let mut requests = Vec::new();
        let mut retries = Vec::new();
        let mut uncaptured = Vec::new();
        loop {
            let request = {
                let mut queued = self
                    .requests
                    .lock()
                    .map_err(|_| TickError::Queue(QueueError::Poisoned("settlement queue")))?;
                queued.pop_front()
            };
            let Some(request) = request else {
                break;
            };
            let request = request
                .downcast::<SettlementRequest<E>>()
                .map_err(|_| TickError::Queue(QueueError::UnexpectedSettlementType))?;
            match request.prepare(cx) {
                SettlementOutcome::Deferred(request) => requests.push(request),
                SettlementOutcome::Retry(request) => retries.push(request),
                SettlementOutcome::UncapturedError {
                    state,
                    type_,
                    message,
                } => uncaptured.push((state, type_, message)),
                SettlementOutcome::None => {}
            }
        }
        if !retries.is_empty() {
            let mut queued = self
                .requests
                .lock()
                .map_err(|_| TickError::Queue(QueueError::Poisoned("settlement queue")))?;
            queued.extend(
                retries
                    .into_iter()
                    .map(|request| request as Box<dyn Any + Send>),
            );
        }
        let count = requests.len();
        if !requests.is_empty() {
            E::settle_deferreds(cx, requests).map_err(TickError::Engine)?;
        }
        // A30 step 2b: event-handler dispatch follows the single batched promise
        // settlement frame and precedes step 3's microtask drain. Dispatch every
        // queued event even if a handler throws, retaining the first error for
        // `tick()` to return only after A30 steps 3 and 4 have also run.
        let mut first_error = None;
        for (state, type_, message) in uncaptured {
            if let Err(error) = dispatch_uncaptured_error::<E>(cx, &state, type_, message) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        if let Some(error) = first_error {
            return Err(TickError::Engine(error));
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
                    | SettlementRequest::Error { deferred, .. }
                    | SettlementRequest::PopErrorScope { deferred, .. } => {
                        E::release_deferred(cx, deferred)
                    }
                    SettlementRequest::StartMap { mut request, .. } => {
                        if let Some(deferred) = request.deferred.take() {
                            E::release_deferred(cx, deferred);
                        }
                    }
                    SettlementRequest::ComputePipeline {
                        deferred,
                        pipeline,
                        status: _,
                        message: _,
                        state: _,
                        lost_at_start: _,
                        module,
                        layout,
                        label: _,
                        queue,
                        gpu,
                    } => {
                        E::release_deferred(cx, deferred);
                        enqueue_compute_pipeline_release(&queue, pipeline, module, layout, gpu);
                    }
                    SettlementRequest::RenderPipeline {
                        deferred,
                        pipeline,
                        status: _,
                        message: _,
                        state: _,
                        lost_at_start: _,
                        vertex_module,
                        fragment_module,
                        layout,
                        label: _,
                        queue,
                        gpu,
                    } => {
                        E::release_deferred(cx, deferred);
                        enqueue_render_pipeline_release(
                            &queue,
                            pipeline,
                            vertex_module,
                            fragment_module,
                            layout,
                            gpu,
                        );
                    }
                    SettlementRequest::UncapturedError { .. }
                    | SettlementRequest::DeviceLost { .. } => {}
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

/// JavaScript constructor callback.
pub type ConstructorFn<E> = fn(
    <E as JsEngine>::Context<'_>,
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

/// A JavaScript constructor specification.
pub struct ConstructorSpec<E: JsEngine + 'static> {
    /// Constructor arity.
    pub length: u8,
    /// Parent for instance-prototype inheritance.
    pub parent: Option<ClassParent>,
    /// Constructor callback.
    pub call: ConstructorFn<E>,
}

/// Parent of a registered class's instance prototype.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ClassParent {
    /// Another class registered by the binding.
    Class(ClassId),
    /// The JavaScript engine's intrinsic `Error.prototype`.
    IntrinsicError,
}

/// A JavaScript class specification.
pub struct ClassSpec<E: JsEngine + 'static> {
    /// Class name.
    pub name: &'static str,
    /// Class identifier requested by core.
    pub id: ClassId,
    /// Script constructor, when the interface is constructible.
    pub constructor: Option<ConstructorSpec<E>>,
    /// Properties installed on the class prototype.
    pub properties: &'static [PropertySpec<E>],
    /// Methods installed on the class prototype.
    pub methods: &'static [MethodSpec<E>],
    /// Finalizer callback.
    pub finalizer: FinalizerFn,
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
    /// No wrapper is registered for an adopted native device.
    UnknownDevice,
    /// An uncaptured-error type has no script-visible GPUError mapping.
    InvalidUncapturedErrorType(WGPUErrorType),
}

/// Failure from the engine-neutral four-step tick skeleton.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TickError<E> {
    /// Promise settlement or release queue failure.
    Queue(QueueError),
    /// Engine promise-settlement or microtask-drain failure.
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
    let settlements = env.settlements().drain::<E>(cx);
    let microtasks = E::drain_microtasks(cx).map_err(TickError::Engine);
    let releases = env.queue().drain().map_err(TickError::Queue);
    match (settlements, microtasks, releases) {
        (Err(error), _, _) | (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(_), Ok(()), Ok(drained)) => Ok(drained),
    }
}

/// Payload stored by a `GPUDevice` wrapper.
pub struct DevicePayload<E: JsEngine + 'static> {
    device: WGPUDevice,
    destroyed: AtomicBool,
    label: Mutex<String>,
    queue: HeldValue<E>,
    features: HeldValue<E>,
    limits: HeldValue<E>,
    adapter_info: HeldValue<E>,
    events: Arc<DeviceEventState<E>>,
}

fn promise_operation<E: JsEngine>(
    cx: E::Context<'_>,
    operation: impl FnOnce(&mut Option<Deferred<E>>) -> Result<(), E::Error>,
) -> Result<E::Value, E::Error> {
    let (promise, deferred) = E::new_promise(cx)?;
    let mut deferred = Some(deferred);
    match operation(&mut deferred) {
        Ok(()) => {
            if let Some(deferred) = deferred.take() {
                E::release_deferred(cx, deferred);
                return Err(E::operation_error(
                    cx,
                    "promise operation did not retain its deferred",
                ));
            }
            Ok(promise)
        }
        Err(error) => {
            let Some(deferred) = deferred.take() else {
                return Err(error);
            };
            let reason = E::error_value_from_error(cx, error);
            E::settle_deferreds(cx, vec![(deferred, Err(reason))])?;
            Ok(promise)
        }
    }
}

impl<E: JsEngine + 'static> DevicePayload<E> {
    fn new(device: WGPUDevice, events: Arc<DeviceEventState<E>>, label: String) -> Self {
        Self {
            device,
            destroyed: AtomicBool::new(false),
            label: Mutex::new(label),
            queue: HeldValue::empty(),
            features: HeldValue::empty(),
            limits: HeldValue::empty(),
            adapter_info: HeldValue::empty(),
            events,
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

struct DeviceEventJs<E: JsEngine + 'static> {
    handler: HeldValue<E>,
    listeners: Vec<RegisteredEventListener<E>>,
    next_listener_id: u64,
    lost_promise: HeldValue<E>,
    lost_deferred: Option<Deferred<E>>,
    lost_registration: Option<E::DeferredRegistration>,
}

struct RegisteredEventListener<E: JsEngine> {
    id: u64,
    type_: String,
    callback: Option<E::Value>,
    once: bool,
}

struct EventTargetPayload<E: JsEngine> {
    listeners: Mutex<EventTargetListeners<E>>,
}

struct EventTargetListeners<E: JsEngine> {
    entries: Vec<RegisteredEventListener<E>>,
    next_listener_id: u64,
}

impl<E: JsEngine> EventTargetPayload<E> {
    fn new() -> Self {
        Self {
            listeners: Mutex::new(EventTargetListeners {
                entries: Vec::new(),
                next_listener_id: 0,
            }),
        }
    }
}

struct DeviceEventRegistration {
    device: usize,
    registry: Weak<DeviceEventRegistry>,
}

struct DeviceEventState<E: JsEngine + 'static> {
    settlements: Arc<SettlementQueue>,
    self_weak: OnceLock<Weak<Self>>,
    registration: OnceLock<DeviceEventRegistration>,
    wrapper_finalized: AtomicBool,
    lost: AtomicBool,
    lost_settled: AtomicBool,
    js: Mutex<Option<Box<DeviceEventJs<E>>>>,
    error_scopes: Mutex<Vec<SyntheticErrorScope>>,
}

struct SyntheticDeviceError {
    type_: WGPUErrorType,
    message: String,
}

struct SyntheticErrorScope {
    filter: WGPUErrorFilter,
    error: Option<SyntheticDeviceError>,
}

trait DeviceErrorSink: Send + Sync {
    fn generate_validation_error(&self, message: String);
}

impl<E: JsEngine + 'static> DeviceEventState<E> {
    fn new(settlements: Arc<SettlementQueue>) -> Arc<Self> {
        let state = Arc::new(Self {
            settlements,
            self_weak: OnceLock::new(),
            registration: OnceLock::new(),
            wrapper_finalized: AtomicBool::new(false),
            lost: AtomicBool::new(false),
            lost_settled: AtomicBool::new(false),
            js: Mutex::new(None),
            error_scopes: Mutex::new(Vec::new()),
        });
        let _ = state.self_weak.set(Arc::downgrade(&state));
        state
    }

    fn set_registration(&self, device: WGPUDevice, registry: Weak<DeviceEventRegistry>) {
        let _ = self.registration.set(DeviceEventRegistration {
            device: device as usize,
            registry,
        });
    }

    fn mark_wrapper_finalized(&self) {
        self.wrapper_finalized.store(true, Ordering::Release);
        self.prune_registration_if_complete();
    }

    fn mark_lost_settled(&self) {
        self.lost_settled.store(true, Ordering::Release);
        self.prune_registration_if_complete();
    }

    fn mark_lost(&self) {
        self.lost.store(true, Ordering::Release);
        for scope in self
            .error_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter_mut()
        {
            scope.error = None;
        }
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Acquire)
    }

    fn is_lost_settled(&self) -> bool {
        self.lost_settled.load(Ordering::Acquire)
    }

    fn prune_registration_if_complete(&self) {
        if !self.wrapper_finalized.load(Ordering::Acquire)
            || !self.lost_settled.load(Ordering::Acquire)
        {
            return;
        }
        let Some(registration) = self.registration.get() else {
            return;
        };
        let Some(registry) = registration.registry.upgrade() else {
            return;
        };
        let mut states = registry
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut remove_key = false;
        if let Some(device_states) = states.get_mut(&registration.device) {
            device_states.retain(|candidate| {
                Arc::clone(candidate)
                    .downcast::<Self>()
                    .map_or(true, |candidate| {
                        !std::ptr::eq(Arc::as_ptr(&candidate), self)
                    })
            });
            remove_key = device_states.is_empty();
        }
        if remove_key {
            states.remove(&registration.device);
        }
    }

    fn take_engine_values(&self) -> Vec<E::Value> {
        let mut state = self
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = state.as_mut() else {
            return Vec::new();
        };
        let mut values = [state.handler.take(), state.lost_promise.take()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        values.extend(
            state
                .listeners
                .drain(..)
                .filter_map(|listener| listener.callback),
        );
        values
    }

    fn initialize(&self, cx: E::Context<'_>) -> Result<(), E::Error> {
        let (promise, deferred) = E::new_promise(cx)?;
        let mut js = Box::new(DeviceEventJs {
            handler: HeldValue::empty(),
            listeners: Vec::new(),
            next_listener_id: 0,
            lost_promise: HeldValue::empty(),
            lost_deferred: Some(deferred),
            lost_registration: None,
        });
        js.lost_promise.set(E::duplicate_value(cx, promise));
        js.lost_registration = Some(E::register_deferred(
            cx,
            NonNull::from(&mut js.lost_deferred),
        ));
        *self
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(js);
        Ok(())
    }

    fn enqueue_uncaptured(
        &self,
        type_: WGPUErrorType,
        message: String,
    ) -> std::result::Result<(), QueueError> {
        if self.is_lost() {
            return Ok(());
        }
        let Some(state) = self.self_weak.get().and_then(Weak::upgrade) else {
            return Ok(());
        };
        self.settlements
            .enqueue::<E>(SettlementRequest::UncapturedError {
                state,
                type_,
                message,
            })
    }

    fn push_error_scope(&self, filter: WGPUErrorFilter) {
        self.error_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(SyntheticErrorScope {
                filter,
                error: None,
            });
    }

    fn pop_error_scope(&self) -> Option<SyntheticDeviceError> {
        self.error_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
            .and_then(|scope| scope.error)
    }

    fn enqueue_lost(
        &self,
        reason: WGPUDeviceLostReason,
        message: String,
    ) -> std::result::Result<(), QueueError> {
        let Some(state) = self.self_weak.get().and_then(Weak::upgrade) else {
            return Ok(());
        };
        self.settlements
            .enqueue::<E>(SettlementRequest::DeviceLost {
                state,
                reason,
                message,
            })
    }

    fn release_after_failed_wrap(&self, cx: E::Context<'_>) {
        let Some(mut js) = self
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };
        js.lost_registration.take();
        if let Some(deferred) = js.lost_deferred.take() {
            E::release_deferred(cx, deferred);
        }
        if let Some(promise) = js.lost_promise.take() {
            E::release_value(cx, promise);
        }
    }
}

impl<E: JsEngine + 'static> DeviceErrorSink for DeviceEventState<E> {
    fn generate_validation_error(&self, message: String) {
        if self.is_lost() {
            return;
        }
        let mut scopes = self
            .error_scopes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(scope) = scopes
            .iter_mut()
            .rev()
            .find(|scope| scope.filter == WGPUErrorFilter_WGPUErrorFilter_Validation)
        {
            if scope.error.is_none() {
                scope.error = Some(SyntheticDeviceError {
                    type_: WGPUErrorType_WGPUErrorType_Validation,
                    message,
                });
            }
            return;
        }
        drop(scopes);
        let _ = self.enqueue_uncaptured(WGPUErrorType_WGPUErrorType_Validation, message);
    }
}

// SAFETY: after engine teardown, every engine-value slot is emptied by
// `release_device_event_values`, and every outstanding deferred slot is emptied
// by the adapter registration machinery. What can remain callback-owned is
// Send-safe Rust data: atomics, weak/strong Arcs, mutexes, native scalar enums,
// and owned Strings in the settlement queue. Arbitrary-thread callbacks only
// enqueue those Rust-owned records; they never inspect an engine token or call
// an engine or webgpu.h function. Before teardown, engine-token access and
// settlement happen only on the designated JavaScript `tick()` thread.
unsafe impl<E: JsEngine + 'static> Send for DeviceEventState<E> {}
// SAFETY: the same lifetime and teardown argument as Send applies; every shared
// field is synchronized, and callbacks use only the pure-Rust enqueue path.
unsafe impl<E: JsEngine + 'static> Sync for DeviceEventState<E> {}

// SAFETY: `DevicePayload` stores an adopted `WGPUDevice`, cached engine values,
// and synchronized event state. A finalizer only copies the native handle into
// `ReleaseRequest::Device` and passes opaque values to adapter release closures;
// native release and all engine access run on the creating `tick()` thread.
unsafe impl<E: JsEngine + 'static> Send for DevicePayload<E> {}

struct HeldValue<E: JsEngine> {
    value: Mutex<Option<E::Value>>,
}

impl<E: JsEngine> HeldValue<E> {
    fn empty() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    fn get(&self) -> Option<E::Value> {
        *self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set(&self, value: E::Value) {
        *self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
    }

    fn set_if_empty(&self, value: E::Value) -> Option<E::Value> {
        let mut held = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(incumbent) = *held {
            Some(incumbent)
        } else {
            *held = Some(value);
            None
        }
    }

    fn take(&self) -> Option<E::Value> {
        self.value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

// SAFETY: `set`/`set_if_empty`/`get` on the engine thread and `take` from a
// potentially arbitrary finalizer thread all acquire `value`, so the mutex
// release/acquire operations establish the required happens-before edges for the slot itself.
// The finalizer only copies the opaque engine value into the adapter-provided
// release closure; it never dereferences it or calls a context-taking engine API.
unsafe impl<E: JsEngine> Send for HeldValue<E> {}

// SAFETY: a JSC finalizer may move this payload's Box to an arbitrary thread
// and inspect it to release held listener tokens. The listener list is guarded
// by its mutex; opaque engine-value handles are only moved into adapter release
// closures and are never dereferenced off the creating `tick()` thread.
unsafe impl<E: JsEngine> Send for EventTargetPayload<E> {}

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

    /// Removes tracked mapped ranges and passes their held values to `release`.
    pub fn release_mapped_range_values(&self, mut release: impl FnMut(E::Value)) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        for range in std::mem::take(&mut state.ranges) {
            release(range.value);
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
        let held = [
            device.queue.take(),
            device.features.take(),
            device.limits.take(),
            device.adapter_info.take(),
        ];
        for value in held.into_iter().flatten() {
            release(value);
        }
        for value in device.events.take_engine_values() {
            release(value);
        }
    }
    if let Some(adapter) = payload.downcast_ref::<AdapterPayload<E>>() {
        let held = [
            adapter.features.take(),
            adapter.limits.take(),
            adapter.info.take(),
        ];
        for value in held.into_iter().flatten() {
            release(value);
        }
    }
    if let Some(event) = payload.downcast_ref::<EventPayload<E>>() {
        if let Some(error) = event.error.take() {
            release(error);
        }
    }
    if let Some(target) = payload.downcast_ref::<EventTargetPayload<E>>() {
        let entries = target
            .listeners
            .lock()
            .map(|mut listeners| std::mem::take(&mut listeners.entries))
            .unwrap_or_default();
        for callback in entries.into_iter().filter_map(|entry| entry.callback) {
            release(callback);
        }
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
    pending_map: Option<u64>,
    canceling_map: Option<u64>,
    next_map_id: u64,
    map_mode: WGPUMapMode,
    error_sink: Arc<dyn DeviceErrorSink>,
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

// SAFETY: `BufferState` crosses threads only behind `BufferPayload::state`'s
// mutex, which orders a finalizer's access after any engine-thread mutation. On
// a finalizer thread, adapters may remove and copy opaque engine-value tokens
// for deferred release, and the core finalizer may copy native handles into the
// release queue; neither path dereferences a token or mapped pointer or calls an
// engine or WebGPU API. Engine values and mapped pointers are dereferenced only
// on the JavaScript thread. A queued native handle is used only later on the
// creating `tick()` thread, without accessing the finalized state again.
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
pub struct AdapterPayload<E: JsEngine> {
    adapter: WGPUAdapter,
    features: HeldValue<E>,
    limits: HeldValue<E>,
    info: HeldValue<E>,
}

impl<E: JsEngine> AdapterPayload<E> {
    fn new(adapter: WGPUAdapter) -> Self {
        Self {
            adapter,
            features: HeldValue::empty(),
            limits: HeldValue::empty(),
            info: HeldValue::empty(),
        }
    }
}

// SAFETY: `AdapterPayload` stores an adopted `WGPUAdapter` and cached engine
// values. A finalizer only copies the native handle into
// `ReleaseRequest::Adapter` and passes opaque values to adapter release
// closures; native release and all engine access run on the creating `tick()`
// thread.
unsafe impl<E: JsEngine> Send for AdapterPayload<E> {}

struct SupportedLimitsPayload {
    limits: WGPULimits,
    compatibility: WGPUCompatibilityModeLimits,
}

// SAFETY: the copied structs contain no live pointers after query completion.
unsafe impl Send for SupportedLimitsPayload {}

struct AdapterInfoPayload {
    vendor: String,
    architecture: String,
    device: String,
    description: String,
    subgroup_min_size: u32,
    subgroup_max_size: u32,
    is_fallback_adapter: bool,
}

// SAFETY: the payload contains only owned strings and scalar values.
unsafe impl Send for AdapterInfoPayload {}

enum FeatureSource {
    Adapter(WGPUAdapter),
    Device(WGPUDevice),
}

enum LimitsSource {
    Adapter(WGPUAdapter),
    Device(WGPUDevice),
}

enum AdapterInfoSource {
    Adapter(WGPUAdapter),
    Device(WGPUDevice),
}

fn new_feature_set<E: JsEngine>(
    cx: E::Context<'_>,
    source: FeatureSource,
) -> Result<E::Value, E::Error> {
    // I2 recorded deviation: JavaScript has no readonly Set constructor, so
    // trusted scripts receive a mutable Set with conformant read behavior.
    let gpu = E::environment(cx).gpu();
    let mut supported = WGPUSupportedFeatures {
        featureCount: 0,
        features: ptr::null(),
    };
    // SAFETY: the selected handle belongs to this dispatch table and `supported`
    // is a live writable out-struct for the duration of the call.
    unsafe {
        match source {
            FeatureSource::Adapter(adapter) => {
                (gpu.adapter_get_features)(adapter, ptr::from_mut(&mut supported));
            }
            FeatureSource::Device(device) => {
                (gpu.device_get_features)(device, ptr::from_mut(&mut supported));
            }
        }
    }
    let copied = if supported.featureCount == 0 {
        Ok(Vec::new())
    } else if supported.features.is_null() {
        Err(E::operation_error(
            cx,
            "feature query returned a null feature list",
        ))
    } else {
        // SAFETY: the query returned `featureCount` caller-owned elements, and
        // they remain live until the immediately following FreeMembers call.
        let features =
            unsafe { std::slice::from_raw_parts(supported.features, supported.featureCount) };
        let mut names = features
            .iter()
            .filter_map(|feature| feature_name_to_str(*feature))
            .collect::<Vec<_>>();
        names.sort_unstable();
        Ok(names)
    };
    // SAFETY: `supported` is exactly the caller-owned result returned above and
    // has not previously been freed.
    unsafe { (gpu.supported_features_free_members)(supported) };
    let names = copied?;

    let values = names
        .into_iter()
        .map(|name| E::string(cx, name))
        .collect::<Result<Vec<_>, _>>()?;
    let global = E::global(cx);
    let array_ctor = E::get_property(cx, global, "Array")?;
    let array = E::construct(cx, array_ctor, &values)?;
    let set_ctor = E::get_property(cx, global, "Set")?;
    E::construct(cx, set_ctor, &[array])
}

fn initial_limits() -> (WGPULimits, WGPUCompatibilityModeLimits) {
    let u32_undefined = WGPU_LIMIT_U32_UNDEFINED;
    let compatibility = WGPUCompatibilityModeLimits {
        chain: WGPUChainedStruct {
            next: ptr::null_mut(),
            sType: WGPUSType_WGPUSType_CompatibilityModeLimits,
        },
        maxStorageBuffersInVertexStage: u32_undefined,
        maxStorageTexturesInVertexStage: u32_undefined,
        maxStorageBuffersInFragmentStage: u32_undefined,
        maxStorageTexturesInFragmentStage: u32_undefined,
    };
    let limits = WGPULimits {
        nextInChain: ptr::null_mut(),
        maxTextureDimension1D: u32_undefined,
        maxTextureDimension2D: u32_undefined,
        maxTextureDimension3D: u32_undefined,
        maxTextureArrayLayers: u32_undefined,
        maxBindGroups: u32_undefined,
        maxBindGroupsPlusVertexBuffers: u32_undefined,
        maxBindingsPerBindGroup: u32_undefined,
        maxDynamicUniformBuffersPerPipelineLayout: u32_undefined,
        maxDynamicStorageBuffersPerPipelineLayout: u32_undefined,
        maxSampledTexturesPerShaderStage: u32_undefined,
        maxSamplersPerShaderStage: u32_undefined,
        maxStorageBuffersPerShaderStage: u32_undefined,
        maxStorageTexturesPerShaderStage: u32_undefined,
        maxUniformBuffersPerShaderStage: u32_undefined,
        maxUniformBufferBindingSize: WGPU_LIMIT_U64_UNDEFINED as u64,
        maxStorageBufferBindingSize: WGPU_LIMIT_U64_UNDEFINED as u64,
        minUniformBufferOffsetAlignment: u32_undefined,
        minStorageBufferOffsetAlignment: u32_undefined,
        maxVertexBuffers: u32_undefined,
        maxBufferSize: WGPU_LIMIT_U64_UNDEFINED as u64,
        maxVertexAttributes: u32_undefined,
        maxVertexBufferArrayStride: u32_undefined,
        maxInterStageShaderVariables: u32_undefined,
        maxColorAttachments: u32_undefined,
        maxColorAttachmentBytesPerSample: u32_undefined,
        maxComputeWorkgroupStorageSize: u32_undefined,
        maxComputeInvocationsPerWorkgroup: u32_undefined,
        maxComputeWorkgroupSizeX: u32_undefined,
        maxComputeWorkgroupSizeY: u32_undefined,
        maxComputeWorkgroupSizeZ: u32_undefined,
        maxComputeWorkgroupsPerDimension: u32_undefined,
        maxImmediateSize: u32_undefined,
    };
    (limits, compatibility)
}

fn is_known_required_limit(name: &str) -> bool {
    matches!(
        name,
        "maxTextureDimension1D"
            | "maxTextureDimension2D"
            | "maxTextureDimension3D"
            | "maxTextureArrayLayers"
            | "maxBindGroups"
            | "maxBindGroupsPlusVertexBuffers"
            | "maxImmediateSize"
            | "maxBindingsPerBindGroup"
            | "maxDynamicUniformBuffersPerPipelineLayout"
            | "maxDynamicStorageBuffersPerPipelineLayout"
            | "maxSampledTexturesPerShaderStage"
            | "maxSamplersPerShaderStage"
            | "maxStorageBuffersPerShaderStage"
            | "maxStorageBuffersInVertexStage"
            | "maxStorageBuffersInFragmentStage"
            | "maxStorageTexturesPerShaderStage"
            | "maxStorageTexturesInVertexStage"
            | "maxStorageTexturesInFragmentStage"
            | "maxUniformBuffersPerShaderStage"
            | "maxUniformBufferBindingSize"
            | "maxStorageBufferBindingSize"
            | "minUniformBufferOffsetAlignment"
            | "minStorageBufferOffsetAlignment"
            | "maxVertexBuffers"
            | "maxBufferSize"
            | "maxVertexAttributes"
            | "maxVertexBufferArrayStride"
            | "maxInterStageShaderVariables"
            | "maxColorAttachments"
            | "maxColorAttachmentBytesPerSample"
            | "maxComputeWorkgroupStorageSize"
            | "maxComputeInvocationsPerWorkgroup"
            | "maxComputeWorkgroupSizeX"
            | "maxComputeWorkgroupSizeY"
            | "maxComputeWorkgroupSizeZ"
            | "maxComputeWorkgroupsPerDimension"
    )
}

fn convert_required_limits<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    names: &[String],
) -> Result<(WGPULimits, WGPUCompatibilityModeLimits), E::Error> {
    let (mut limits, mut compatibility) = initial_limits();
    for name in names {
        let value = E::get_property(cx, value, name)?;
        if E::is_undefined(cx, value) {
            continue;
        }
        macro_rules! set_u32 {
            ($target:expr) => {
                $target = enforce_u32::<E>(cx, value, "required limit")?
            };
        }
        macro_rules! set_u64 {
            ($target:expr) => {
                $target = enforce_u64::<E>(cx, value, "required limit")?
            };
        }
        match name.as_str() {
            "maxTextureDimension1D" => set_u32!(limits.maxTextureDimension1D),
            "maxTextureDimension2D" => set_u32!(limits.maxTextureDimension2D),
            "maxTextureDimension3D" => set_u32!(limits.maxTextureDimension3D),
            "maxTextureArrayLayers" => set_u32!(limits.maxTextureArrayLayers),
            "maxBindGroups" => set_u32!(limits.maxBindGroups),
            "maxBindGroupsPlusVertexBuffers" => {
                set_u32!(limits.maxBindGroupsPlusVertexBuffers)
            }
            "maxImmediateSize" => set_u32!(limits.maxImmediateSize),
            "maxBindingsPerBindGroup" => set_u32!(limits.maxBindingsPerBindGroup),
            "maxDynamicUniformBuffersPerPipelineLayout" => {
                set_u32!(limits.maxDynamicUniformBuffersPerPipelineLayout)
            }
            "maxDynamicStorageBuffersPerPipelineLayout" => {
                set_u32!(limits.maxDynamicStorageBuffersPerPipelineLayout)
            }
            "maxSampledTexturesPerShaderStage" => {
                set_u32!(limits.maxSampledTexturesPerShaderStage)
            }
            "maxSamplersPerShaderStage" => set_u32!(limits.maxSamplersPerShaderStage),
            "maxStorageBuffersPerShaderStage" => {
                set_u32!(limits.maxStorageBuffersPerShaderStage)
            }
            "maxStorageBuffersInVertexStage" => {
                set_u32!(compatibility.maxStorageBuffersInVertexStage)
            }
            "maxStorageBuffersInFragmentStage" => {
                set_u32!(compatibility.maxStorageBuffersInFragmentStage)
            }
            "maxStorageTexturesPerShaderStage" => {
                set_u32!(limits.maxStorageTexturesPerShaderStage)
            }
            "maxStorageTexturesInVertexStage" => {
                set_u32!(compatibility.maxStorageTexturesInVertexStage)
            }
            "maxStorageTexturesInFragmentStage" => {
                set_u32!(compatibility.maxStorageTexturesInFragmentStage)
            }
            "maxUniformBuffersPerShaderStage" => {
                set_u32!(limits.maxUniformBuffersPerShaderStage)
            }
            "maxUniformBufferBindingSize" => set_u64!(limits.maxUniformBufferBindingSize),
            "maxStorageBufferBindingSize" => set_u64!(limits.maxStorageBufferBindingSize),
            "minUniformBufferOffsetAlignment" => {
                set_u32!(limits.minUniformBufferOffsetAlignment)
            }
            "minStorageBufferOffsetAlignment" => {
                set_u32!(limits.minStorageBufferOffsetAlignment)
            }
            "maxVertexBuffers" => set_u32!(limits.maxVertexBuffers),
            "maxBufferSize" => set_u64!(limits.maxBufferSize),
            "maxVertexAttributes" => set_u32!(limits.maxVertexAttributes),
            "maxVertexBufferArrayStride" => set_u32!(limits.maxVertexBufferArrayStride),
            "maxInterStageShaderVariables" => {
                set_u32!(limits.maxInterStageShaderVariables)
            }
            "maxColorAttachments" => set_u32!(limits.maxColorAttachments),
            "maxColorAttachmentBytesPerSample" => {
                set_u32!(limits.maxColorAttachmentBytesPerSample)
            }
            "maxComputeWorkgroupStorageSize" => {
                set_u32!(limits.maxComputeWorkgroupStorageSize)
            }
            "maxComputeInvocationsPerWorkgroup" => {
                set_u32!(limits.maxComputeInvocationsPerWorkgroup)
            }
            "maxComputeWorkgroupSizeX" => set_u32!(limits.maxComputeWorkgroupSizeX),
            "maxComputeWorkgroupSizeY" => set_u32!(limits.maxComputeWorkgroupSizeY),
            "maxComputeWorkgroupSizeZ" => set_u32!(limits.maxComputeWorkgroupSizeZ),
            "maxComputeWorkgroupsPerDimension" => {
                set_u32!(limits.maxComputeWorkgroupsPerDimension)
            }
            _ => unreachable!("required-limit names are validated before conversion"),
        }
    }
    Ok((limits, compatibility))
}

fn new_supported_limits<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    source: LimitsSource,
) -> Result<E::Value, E::Error> {
    let (mut limits, mut compatibility) = initial_limits();
    limits.nextInChain = ptr::from_mut(&mut compatibility.chain);
    let gpu = E::environment(cx).gpu();
    // SAFETY: the selected handle belongs to this dispatch table; `limits` and
    // its compatibility chain remain live and writable through the call.
    let status = unsafe {
        match source {
            LimitsSource::Adapter(adapter) => {
                (gpu.adapter_get_limits)(adapter, ptr::from_mut(&mut limits))
            }
            LimitsSource::Device(device) => {
                (gpu.device_get_limits)(device, ptr::from_mut(&mut limits))
            }
        }
    };
    if status != WGPUStatus_WGPUStatus_Success {
        return Err(E::operation_error(cx, "native limits query failed"));
    }
    limits.nextInChain = ptr::null_mut();
    compatibility.chain.next = ptr::null_mut();
    let _ = E::register_class(cx, supported_limits_class::<E>())?;
    E::new_instance(
        cx,
        GPU_SUPPORTED_LIMITS_CLASS,
        Box::new(SupportedLimitsPayload {
            limits,
            compatibility,
        }),
    )
}

fn output_string_to_owned(view: WGPUStringView) -> String {
    if view.data.is_null() || view.length == wgpu_strlen() {
        return String::new();
    }
    // SAFETY: a successful adapter-info query returns `length` readable bytes
    // that remain live until AdapterInfoFreeMembers below.
    let bytes = unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length) };
    String::from_utf8_lossy(bytes).into_owned()
}

fn new_adapter_info<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    source: AdapterInfoSource,
) -> Result<E::Value, E::Error> {
    let empty = WGPUStringView {
        data: ptr::null(),
        length: wgpu_strlen(),
    };
    let mut info = WGPUAdapterInfo {
        nextInChain: ptr::null_mut(),
        vendor: empty,
        architecture: empty,
        device: empty,
        description: empty,
        backendType: WGPUBackendType_WGPUBackendType_Undefined,
        adapterType: 0,
        vendorID: 0,
        deviceID: 0,
        subgroupMinSize: 0,
        subgroupMaxSize: 0,
    };
    let gpu = E::environment(cx).gpu();
    // SAFETY: the selected handle belongs to this dispatch table and `info` is
    // a live writable out-struct through the call.
    let status = unsafe {
        match source {
            AdapterInfoSource::Adapter(adapter) => {
                (gpu.adapter_get_info)(adapter, ptr::from_mut(&mut info))
            }
            AdapterInfoSource::Device(device) => {
                (gpu.device_get_adapter_info)(device, ptr::from_mut(&mut info))
            }
        }
    };
    let payload = (status == WGPUStatus_WGPUStatus_Success).then(|| AdapterInfoPayload {
        vendor: output_string_to_owned(info.vendor),
        architecture: output_string_to_owned(info.architecture),
        device: output_string_to_owned(info.device),
        description: output_string_to_owned(info.description),
        subgroup_min_size: info.subgroupMinSize,
        subgroup_max_size: info.subgroupMaxSize,
        // The C pin has no isFallbackAdapter field. Its closest stable signal is
        // the CPU adapter classification used for fallback/no-op adapters.
        is_fallback_adapter: info.adapterType == WGPUAdapterType_WGPUAdapterType_CPU,
    });
    // SAFETY: `info` is exactly the caller-owned result returned above and has
    // not previously been freed; all exposed strings were copied first.
    unsafe { (gpu.adapter_info_free_members)(info) };
    let payload =
        payload.ok_or_else(|| E::operation_error(cx, "native adapter-info query failed"))?;
    let _ = E::register_class(cx, adapter_info_class::<E>())?;
    E::new_instance(cx, GPU_ADAPTER_INFO_CLASS, Box::new(payload))
}

/// Payload stored by a `GPUQueue` wrapper.
pub struct QueuePayload {
    queue: WGPUQueue,
    label: Mutex<String>,
}

// SAFETY: `QueuePayload` stores a `WGPUQueue`. Off-thread finalization only
// enqueues `ReleaseRequest::Queue`; queue operations run from JS methods on the
// engine thread, and `wgpuQueueRelease` runs during `tick()`-thread drain.
// SAFETY: The `WGPUQueue` is used by JS methods or released during `tick()` drain.
unsafe impl Send for QueuePayload {}

/// Payload stored by a `GPUCommandBuffer` wrapper.
pub struct CommandBufferPayload {
    state: Arc<Mutex<CommandBufferState>>,
    label: Mutex<String>,
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
    invalid: bool,
    error_sink: Arc<dyn DeviceErrorSink>,
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
    label: Mutex<String>,
}

// SAFETY: `ComputePassEncoderPayload` stores a `WGPUComputePassEncoder` inside
// shared state and a parent command-encoder state reference. JS pass methods
// dereference the pass on the engine thread; finalization only copies the pass
// handle into `ReleaseRequest::ComputePassEncoder`, drained on the creating
// `tick()` thread.
// SAFETY: The `WGPUComputePassEncoder` is used on the engine thread or released in `tick()`.
unsafe impl Send for ComputePassEncoderPayload {}

/// Payload stored by a `GPURenderPassEncoder` wrapper.
pub struct RenderPassEncoderPayload {
    state: Arc<Mutex<RenderPassState>>,
    label: Mutex<String>,
}

// SAFETY: the native render-pass handle is used only on the engine thread and
// copied into the release queue by finalization, matching the compute pass.
unsafe impl Send for RenderPassEncoderPayload {}

/// Payload stored by a reusable `GPURenderBundle` wrapper.
pub struct RenderBundlePayload {
    render_bundle: WGPURenderBundle,
    invalid: bool,
    label: Mutex<String>,
}

/// Returns the native handle stored by a `GPURenderBundle` wrapper.
///
/// The engine payload lookup includes the wrapper's registered `ClassSpec`
/// identity, so values of every other JavaScript class return `None` rather
/// than being interpreted as render-bundle payloads.
///
/// # Lifetime
///
/// The returned handle is **borrowed from `value`**. It has no independent
/// native reference. The host must keep the JavaScript wrapper alive for every
/// native use of this handle (for example, by retaining it in a global), or
/// take its own native reference before allowing the wrapper to be collected.
#[must_use]
pub fn native_render_bundle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Option<WGPURenderBundle> {
    E::payload(cx, value, GPU_RENDER_BUNDLE_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundlePayload>())
        .map(|payload| payload.render_bundle)
}

// SAFETY: the native bundle handle is only copied into the release queue by
// finalization and passed to native render-pass recording on the engine thread.
unsafe impl Send for RenderBundlePayload {}

struct CommandEncoderState {
    encoder: WGPUCommandEncoder,
    ended: bool,
    locked: bool,
    pending_validation_error: Option<String>,
    error_sink: Arc<dyn DeviceErrorSink>,
}

// SAFETY: `CommandEncoderState` contains a `WGPUCommandEncoder` and an ended
// flag protected by a `Mutex`. JS methods dereference the encoder only on the
// engine thread; finalizers may lock the state off-thread only to copy the
// handle into `ReleaseRequest::CommandEncoder`, whose release runs during
// `tick()`-thread drain.
// SAFETY: The `WGPUCommandEncoder` is copied by finalizers and dereferenced in engine/`tick()`.
unsafe impl Send for CommandEncoderState {}

struct RenderBundleEncoderState {
    render_bundle_encoder: WGPURenderBundleEncoder,
    ended: bool,
    error_sink: Arc<dyn DeviceErrorSink>,
}

// SAFETY: the non-thread-safe native encoder is dereferenced only by JS methods
// on the engine thread; finalization copies it into the tick-thread release queue.
unsafe impl Send for RenderBundleEncoderState {}

#[derive(Clone, Copy)]
enum LiveRenderCommands {
    Pass(WGPURenderPassEncoder),
    Bundle(WGPURenderBundleEncoder),
}

#[derive(Clone, Copy)]
enum LiveDebugCommands {
    Command(WGPUCommandEncoder),
    ComputePass(WGPUComputePassEncoder),
    RenderPass(WGPURenderPassEncoder),
    RenderBundle(WGPURenderBundleEncoder),
}

impl LiveDebugCommands {
    unsafe fn push_debug_group(self, gpu: GpuDispatch, label: WGPUStringView) {
        match self {
            Self::Command(encoder) => unsafe {
                (gpu.command_encoder_push_debug_group)(encoder, label)
            },
            Self::ComputePass(pass) => unsafe {
                (gpu.compute_pass_encoder_push_debug_group)(pass, label)
            },
            Self::RenderPass(pass) => unsafe {
                (gpu.render_pass_encoder_push_debug_group)(pass, label)
            },
            Self::RenderBundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_push_debug_group)(bundle, label)
            },
        }
    }

    unsafe fn pop_debug_group(self, gpu: GpuDispatch) {
        match self {
            Self::Command(encoder) => unsafe { (gpu.command_encoder_pop_debug_group)(encoder) },
            Self::ComputePass(pass) => unsafe { (gpu.compute_pass_encoder_pop_debug_group)(pass) },
            Self::RenderPass(pass) => unsafe { (gpu.render_pass_encoder_pop_debug_group)(pass) },
            Self::RenderBundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_pop_debug_group)(bundle)
            },
        }
    }

    unsafe fn insert_debug_marker(self, gpu: GpuDispatch, label: WGPUStringView) {
        match self {
            Self::Command(encoder) => unsafe {
                (gpu.command_encoder_insert_debug_marker)(encoder, label)
            },
            Self::ComputePass(pass) => unsafe {
                (gpu.compute_pass_encoder_insert_debug_marker)(pass, label)
            },
            Self::RenderPass(pass) => unsafe {
                (gpu.render_pass_encoder_insert_debug_marker)(pass, label)
            },
            Self::RenderBundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_insert_debug_marker)(bundle, label)
            },
        }
    }
}

impl LiveRenderCommands {
    unsafe fn set_pipeline(self, gpu: GpuDispatch, pipeline: WGPURenderPipeline) {
        match self {
            Self::Pass(pass) => unsafe { (gpu.render_pass_encoder_set_pipeline)(pass, pipeline) },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_set_pipeline)(bundle, pipeline)
            },
        }
    }

    unsafe fn set_vertex_buffer(
        self,
        gpu: GpuDispatch,
        slot: u32,
        buffer: WGPUBuffer,
        offset: u64,
        size: u64,
    ) {
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_set_vertex_buffer)(pass, slot, buffer, offset, size)
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_set_vertex_buffer)(bundle, slot, buffer, offset, size)
            },
        }
    }

    unsafe fn set_index_buffer(
        self,
        gpu: GpuDispatch,
        buffer: WGPUBuffer,
        format: WGPUIndexFormat,
        offset: u64,
        size: u64,
    ) {
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_set_index_buffer)(pass, buffer, format, offset, size)
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_set_index_buffer)(bundle, buffer, format, offset, size)
            },
        }
    }

    unsafe fn set_bind_group(
        self,
        gpu: GpuDispatch,
        index: u32,
        bind_group: WGPUBindGroup,
        dynamic_offsets: &[u32],
    ) {
        let offsets = if dynamic_offsets.is_empty() {
            ptr::null()
        } else {
            dynamic_offsets.as_ptr()
        };
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_set_bind_group)(
                    pass,
                    index,
                    bind_group,
                    dynamic_offsets.len(),
                    offsets,
                )
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_set_bind_group)(
                    bundle,
                    index,
                    bind_group,
                    dynamic_offsets.len(),
                    offsets,
                )
            },
        }
    }

    unsafe fn draw(
        self,
        gpu: GpuDispatch,
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    ) {
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_draw)(
                    pass,
                    vertex_count,
                    instance_count,
                    first_vertex,
                    first_instance,
                )
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_draw)(
                    bundle,
                    vertex_count,
                    instance_count,
                    first_vertex,
                    first_instance,
                )
            },
        }
    }

    unsafe fn draw_indexed(self, gpu: GpuDispatch, args: (u32, u32, u32, i32, u32)) {
        let (index_count, instance_count, first_index, base_vertex, first_instance) = args;
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_draw_indexed)(
                    pass,
                    index_count,
                    instance_count,
                    first_index,
                    base_vertex,
                    first_instance,
                )
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_draw_indexed)(
                    bundle,
                    index_count,
                    instance_count,
                    first_index,
                    base_vertex,
                    first_instance,
                )
            },
        }
    }

    unsafe fn draw_indirect(
        self,
        gpu: GpuDispatch,
        indirect_buffer: WGPUBuffer,
        indirect_offset: u64,
    ) {
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_draw_indirect)(pass, indirect_buffer, indirect_offset)
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_draw_indirect)(bundle, indirect_buffer, indirect_offset)
            },
        }
    }

    unsafe fn draw_indexed_indirect(
        self,
        gpu: GpuDispatch,
        indirect_buffer: WGPUBuffer,
        indirect_offset: u64,
    ) {
        match self {
            Self::Pass(pass) => unsafe {
                (gpu.render_pass_encoder_draw_indexed_indirect)(
                    pass,
                    indirect_buffer,
                    indirect_offset,
                )
            },
            Self::Bundle(bundle) => unsafe {
                (gpu.render_bundle_encoder_draw_indexed_indirect)(
                    bundle,
                    indirect_buffer,
                    indirect_offset,
                )
            },
        }
    }
}

struct ComputePassState {
    pass: WGPUComputePassEncoder,
    ended: bool,
    parent: Arc<Mutex<CommandEncoderState>>,
    error_sink: Arc<dyn DeviceErrorSink>,
}

// SAFETY: `ComputePassState` contains a `WGPUComputePassEncoder` and a parent
// command-encoder state reference. JS pass methods dereference the pass only on
// the engine thread; finalizers may lock the state off-thread only to copy the
// pass into `ReleaseRequest::ComputePassEncoder`, drained on the creating `tick()`
// thread.
// SAFETY: The `WGPUComputePassEncoder` is copied by finalizers and dereferenced in engine/`tick()`.
unsafe impl Send for ComputePassState {}

struct RenderPassState {
    pass: WGPURenderPassEncoder,
    ended: bool,
    parent: Arc<Mutex<CommandEncoderState>>,
    error_sink: Arc<dyn DeviceErrorSink>,
}

// SAFETY: the handle and parent state follow the same thread/release discipline
// as `ComputePassState` and are protected by the state mutex.
unsafe impl Send for RenderPassState {}

#[derive(Clone, Copy)]
struct MappedRange<E: JsEngine> {
    value: E::Value,
    offset: usize,
    size: usize,
    /// The one native pointer requested for this JS range. BufferMapping.md
    /// guarantees that it remains valid until unmap; every detach/copy-back path
    /// runs strictly before `wgpuBufferUnmap` or `wgpuBufferDestroy`.
    native_ptr: *mut c_void,
    map_mode: WGPUMapMode,
}

/// Registers the GPUBuffer class.
pub fn register_buffer_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, buffer_class::<E>())
}

/// Registers the script-visible WebGPU error classes.
pub fn register_error_classes<E: JsEngine + 'static>(cx: E::Context<'_>) -> Result<(), E::Error> {
    for spec in [
        gpu_error_class::<E>(),
        gpu_validation_error_class::<E>(),
        gpu_out_of_memory_error_class::<E>(),
        gpu_internal_error_class::<E>(),
    ] {
        let _ = E::register_class(cx, spec)?;
    }
    Ok(())
}

/// Registers the minimal `DOMException` base used by WebGPU exceptions.
pub fn register_dom_exception_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, dom_exception_class::<E>())
}

/// Registers the minimal DOM event classes required by WebGPU.
pub fn register_event_classes<E: JsEngine + 'static>(cx: E::Context<'_>) -> Result<(), E::Error> {
    for spec in [
        event_class::<E>(),
        event_target_class::<E>(),
        uncaptured_error_event_class::<E>(),
    ] {
        let _ = E::register_class(cx, spec)?;
    }
    Ok(())
}

/// Registers the script-visible `GPUDeviceLostInfo` class.
pub fn register_device_lost_info_class<E: JsEngine + 'static>(
    cx: E::Context<'_>,
) -> Result<ClassId, E::Error> {
    E::register_class(cx, gpu_device_lost_info_class::<E>())
}

fn register_all_classes<E: JsEngine + 'static>(cx: E::Context<'_>) -> Result<(), E::Error> {
    register_event_classes::<E>(cx)?;
    let _ = register_dom_exception_class::<E>(cx)?;
    register_error_classes::<E>(cx)?;
    let _ = register_device_lost_info_class::<E>(cx)?;
    let _ = E::register_class(cx, supported_limits_class::<E>())?;
    let _ = E::register_class(cx, adapter_info_class::<E>())?;
    register_generated_classes::<E>(cx)?;
    let environment = E::environment(cx);
    if !environment
        .namespace_globals_installed
        .load(Ordering::Acquire)
    {
        register_generated_namespaces::<E>(cx)?;
        environment
            .namespace_globals_installed
            .store(true, Ordering::Release);
    }
    Ok(())
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
    register_all_classes::<E>(cx)?;
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
    register_all_classes::<E>(cx)?;
    let env = E::environment(cx);
    unsafe {
        (env.gpu().device_add_ref)(device);
    }
    let events = DeviceEventState::new(Arc::clone(env.settlements()));
    if let Err(error) = events.initialize(cx) {
        events.release_after_failed_wrap(cx);
        let _ = env.queue().enqueue(ReleaseRequest::Device {
            device,
            gpu: env.gpu(),
        });
        return Err(error);
    }
    let value = E::new_instance(
        cx,
        GPU_DEVICE_CLASS,
        Box::new(DevicePayload::<E>::new(
            device,
            Arc::clone(&events),
            String::new(),
        )),
    );
    match value {
        Ok(value) => {
            env.register_device_events(device, events);
            Ok(value)
        }
        Err(error) => {
            events.release_after_failed_wrap(cx);
            let _ = env.queue().enqueue(ReleaseRequest::Device {
                device,
                gpu: env.gpu(),
            });
            Err(error)
        }
    }
}

struct ErrorPayload {
    message: String,
}

struct DomExceptionPayload {
    name: String,
    message: String,
    reason: Option<PipelineErrorReason>,
}

#[derive(Clone, Copy)]
enum PipelineErrorReason {
    Validation,
    Internal,
}

impl PipelineErrorReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Internal => "internal",
        }
    }
}

struct EventPayload<E: JsEngine> {
    type_: String,
    cancelable: bool,
    default_prevented: AtomicBool,
    error: HeldValue<E>,
}

struct DeviceLostInfoPayload {
    message: String,
    reason: &'static str,
}

fn new_device_lost_info<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    reason: WGPUDeviceLostReason,
    message: String,
) -> Result<E::Value, E::Error> {
    let reason = if reason == WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed {
        "destroyed"
    } else {
        "unknown"
    };
    E::new_instance(
        cx,
        GPU_DEVICE_LOST_INFO_CLASS,
        Box::new(DeviceLostInfoPayload { message, reason }),
    )
}

fn device_lost_info_message_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_LOST_INFO_CLASS)
        .and_then(|payload| payload.downcast_ref::<DeviceLostInfoPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUDeviceLostInfo.message called on an incompatible object",
            )
        })?;
    E::string(cx, &payload.message)
}

fn device_lost_info_reason_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_LOST_INFO_CLASS)
        .and_then(|payload| payload.downcast_ref::<DeviceLostInfoPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUDeviceLostInfo.reason called on an incompatible object",
            )
        })?;
    E::string(cx, payload.reason)
}

fn gpu_device_lost_info_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_DEVICE_LOST_INFO_CLASS, || ClassSpec {
        name: "GPUDeviceLostInfo",
        id: GPU_DEVICE_LOST_INFO_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "reason",
                get: Some(device_lost_info_reason_get::<E>),
                set: None,
            },
            PropertySpec {
                name: "message",
                get: Some(device_lost_info_message_get::<E>),
                set: None,
            },
        ])),
        methods: &[],
        finalizer: |_payload, _env| {},
    })
}

fn new_gpu_error<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    type_: WGPUErrorType,
    message: String,
) -> Result<E::Value, E::Error> {
    let class = if type_ == WGPUErrorType_WGPUErrorType_Validation {
        GPU_VALIDATION_ERROR_CLASS
    } else if type_ == WGPUErrorType_WGPUErrorType_OutOfMemory {
        GPU_OUT_OF_MEMORY_ERROR_CLASS
    } else if type_ == WGPUErrorType_WGPUErrorType_Internal
        || type_ == WGPUErrorType_WGPUErrorType_Unknown
    {
        GPU_INTERNAL_ERROR_CLASS
    } else {
        return Err(E::operation_error(cx, "unknown WebGPU error type"));
    };
    E::new_instance(cx, class, Box::new(ErrorPayload { message }))
}

fn event_init_cancelable<E: JsEngine>(
    cx: E::Context<'_>,
    init: Option<E::Value>,
) -> Result<bool, E::Error> {
    let Some(init) = init else {
        return Ok(false);
    };
    if E::is_undefined(cx, init) {
        return Ok(false);
    }
    if !E::is_object(cx, init) {
        return Err(E::type_error(cx, "EventInit must be an object"));
    }
    Ok(E::to_bool(cx, E::get_property(cx, init, "cancelable")?))
}

fn new_event<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    class: ClassId,
    type_: String,
    cancelable: bool,
    error: Option<E::Value>,
) -> Result<E::Value, E::Error> {
    let value = E::new_instance(
        cx,
        class,
        Box::new(EventPayload::<E> {
            type_,
            cancelable,
            default_prevented: AtomicBool::new(false),
            error: HeldValue::empty(),
        }),
    )?;
    if let Some(error) = error {
        let payload = E::payload(cx, value, class)
            .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
            .ok_or_else(|| E::operation_error(cx, "new Event payload is unavailable"))?;
        payload.error.set(E::duplicate_value(cx, error));
    }
    Ok(value)
}

fn new_gpu_uncaptured_error_event<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    type_: String,
    cancelable: bool,
    error: E::Value,
) -> Result<E::Value, E::Error> {
    new_event::<E>(
        cx,
        GPU_UNCAPTURED_ERROR_EVENT_CLASS,
        type_,
        cancelable,
        Some(error),
    )
}

fn dom_exception_payload<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Option<&DomExceptionPayload> {
    [DOM_EXCEPTION_CLASS, GPU_PIPELINE_ERROR_CLASS]
        .into_iter()
        .find_map(|class| E::payload(cx, this, class))
        .and_then(|payload| payload.downcast_ref::<DomExceptionPayload>())
}

fn optional_dom_string<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    arena: &Arena,
    default: &str,
) -> Result<String, E::Error> {
    let Some(value) = value else {
        return Ok(default.to_owned());
    };
    if E::is_undefined(cx, value) {
        Ok(default.to_owned())
    } else {
        Ok(E::to_str(cx, value, arena)?.to_owned())
    }
}

fn dom_exception_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let arena = Arena::new();
    let message = optional_dom_string::<E>(cx, args.first().copied(), &arena, "")?;
    let name = optional_dom_string::<E>(cx, args.get(1).copied(), &arena, "Error")?;
    E::new_error_instance(
        cx,
        DOM_EXCEPTION_CLASS,
        Box::new(DomExceptionPayload {
            name: name.clone(),
            message: message.clone(),
            reason: None,
        }),
        &name,
        &message,
    )
}

/// Implements the `GPUPipelineError` constructor emitted by codegen.
pub fn gpu_pipeline_error_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let arena = Arena::new();
    let message = optional_dom_string::<E>(cx, args.first().copied(), &arena, "")?;
    let init = required_argument::<E>(cx, args, 1, "GPUPipelineErrorInit is required")?;
    if !E::is_object(cx, init) {
        return Err(E::type_error(cx, "GPUPipelineErrorInit must be an object"));
    }
    let reason = E::get_property(cx, init, "reason")?;
    if E::is_undefined(cx, reason) {
        return Err(E::type_error(cx, "GPUPipelineErrorInit.reason is required"));
    }
    let reason = match E::to_str(cx, reason, &arena)? {
        "validation" => PipelineErrorReason::Validation,
        "internal" => PipelineErrorReason::Internal,
        _ => {
            return Err(E::type_error(
                cx,
                "GPUPipelineErrorInit.reason must be 'validation' or 'internal'",
            ));
        }
    };
    new_gpu_pipeline_error::<E>(cx, message, reason)
}

fn new_gpu_pipeline_error<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    message: String,
    reason: PipelineErrorReason,
) -> Result<E::Value, E::Error> {
    E::new_error_instance(
        cx,
        GPU_PIPELINE_ERROR_CLASS,
        Box::new(DomExceptionPayload {
            name: "GPUPipelineError".to_owned(),
            message: message.clone(),
            reason: Some(reason),
        }),
        "GPUPipelineError",
        &message,
    )
}

/// Gets the inherited `DOMException.name` attribute for a `GPUPipelineError`.
pub fn gpu_pipeline_error_name_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = dom_exception_payload::<E>(cx, this).ok_or_else(|| {
        E::type_error(cx, "GPUPipelineError.name called on an incompatible object")
    })?;
    E::string(cx, &payload.name)
}

/// Gets the inherited `DOMException.message` attribute for a `GPUPipelineError`.
pub fn gpu_pipeline_error_message_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = dom_exception_payload::<E>(cx, this).ok_or_else(|| {
        E::type_error(
            cx,
            "GPUPipelineError.message called on an incompatible object",
        )
    })?;
    E::string(cx, &payload.message)
}

/// Gets the `GPUPipelineError.reason` attribute.
pub fn gpu_pipeline_error_reason_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let reason = dom_exception_payload::<E>(cx, this)
        .and_then(|payload| payload.reason)
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUPipelineError.reason called on an incompatible object",
            )
        })?;
    E::string(cx, reason.as_str())
}

/// Finalizes a `GPUPipelineError`; its payload contains only Rust-owned strings.
pub fn finalize_pipeline_error(_payload: Box<dyn Any + Send>, _env: &Environment) {}

fn dom_exception_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(DOM_EXCEPTION_CLASS, || ClassSpec {
        name: "DOMException",
        id: DOM_EXCEPTION_CLASS,
        constructor: Some(ConstructorSpec {
            length: 0,
            parent: Some(ClassParent::IntrinsicError),
            call: dom_exception_constructor::<E>,
        }),
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "name",
                get: Some(gpu_pipeline_error_name_get::<E>),
                set: None,
            },
            PropertySpec {
                name: "message",
                get: Some(gpu_pipeline_error_message_get::<E>),
                set: None,
            },
        ])),
        methods: &[],
        finalizer: |_payload, _env| {},
    })
}

fn event_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let arena = Arena::new();
    let type_ = E::to_str(
        cx,
        args.first().copied().unwrap_or_else(|| E::undefined(cx)),
        &arena,
    )?
    .to_owned();
    let cancelable = event_init_cancelable::<E>(cx, args.get(1).copied())?;
    new_event::<E>(cx, EVENT_CLASS, type_, cancelable, None)
}

fn event_target_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    E::new_instance(
        cx,
        EVENT_TARGET_CLASS,
        Box::new(EventTargetPayload::<E>::new()),
    )
}

/// Implements the `GPUUncapturedErrorEvent` constructor emitted by codegen.
pub fn gpu_uncaptured_error_event_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let arena = Arena::new();
    let type_ = E::to_str(
        cx,
        args.first().copied().unwrap_or_else(|| E::undefined(cx)),
        &arena,
    )?
    .to_owned();
    let init = required_argument::<E>(cx, args, 1, "GPUUncapturedErrorEventInit")?;
    if !E::is_object(cx, init) {
        return Err(E::type_error(
            cx,
            "GPUUncapturedErrorEventInit must be an object",
        ));
    }
    let error = E::get_property(cx, init, "error")?;
    if [
        GPU_ERROR_CLASS,
        GPU_VALIDATION_ERROR_CLASS,
        GPU_OUT_OF_MEMORY_ERROR_CLASS,
        GPU_INTERNAL_ERROR_CLASS,
    ]
    .into_iter()
    .all(|class| E::payload(cx, error, class).is_none())
    {
        return Err(E::type_error(cx, "GPUError is required"));
    }
    let cancelable = event_init_cancelable::<E>(cx, Some(init))?;
    new_gpu_uncaptured_error_event::<E>(cx, type_, cancelable, error)
}

fn event_type_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    E::string(cx, &event_type_value::<E>(cx, this)?)
}

fn event_cancelable_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let cancelable = event_payload::<E>(cx, this)
        .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
        .map(|payload| payload.cancelable)
        .ok_or_else(|| E::type_error(cx, "Event.cancelable called on an incompatible object"))?;
    Ok(E::boolean(cx, cancelable))
}

fn event_default_prevented_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    Ok(E::boolean(cx, event_default_prevented::<E>(cx, this)?))
}

fn event_prevent_default<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = event_payload::<E>(cx, this)
        .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
        .ok_or_else(|| {
            E::type_error(cx, "Event.preventDefault called on an incompatible object")
        })?;
    if payload.cancelable {
        payload.default_prevented.store(true, Ordering::Release);
    }
    Ok(E::undefined(cx))
}

/// Gets the `[SameObject]` `GPUUncapturedErrorEvent.error` attribute.
pub fn gpu_uncaptured_error_event_error_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let error = E::payload(cx, this, GPU_UNCAPTURED_ERROR_EVENT_CLASS)
        .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
        .and_then(|payload| payload.error.get())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUUncapturedErrorEvent.error called on an incompatible object",
            )
        })?;
    Ok(E::return_held_value(cx, error))
}

fn event_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(EVENT_CLASS, || ClassSpec {
        name: "Event",
        id: EVENT_CLASS,
        constructor: Some(ConstructorSpec {
            length: 1,
            parent: None,
            call: event_constructor::<E>,
        }),
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "type",
                get: Some(event_type_get::<E>),
                set: None,
            },
            PropertySpec {
                name: "cancelable",
                get: Some(event_cancelable_get::<E>),
                set: None,
            },
            PropertySpec {
                name: "defaultPrevented",
                get: Some(event_default_prevented_get::<E>),
                set: None,
            },
        ])),
        methods: Box::leak(Box::new([MethodSpec {
            name: "preventDefault",
            length: 0,
            call: event_prevent_default::<E>,
        }])),
        finalizer: |_payload, _env| {},
    })
}

fn event_target_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(EVENT_TARGET_CLASS, || ClassSpec {
        name: "EventTarget",
        id: EVENT_TARGET_CLASS,
        constructor: Some(ConstructorSpec {
            length: 0,
            parent: None,
            call: event_target_constructor::<E>,
        }),
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec {
                name: "addEventListener",
                length: 2,
                call: event_target_add_event_listener::<E>,
            },
            MethodSpec {
                name: "removeEventListener",
                length: 2,
                call: event_target_remove_event_listener::<E>,
            },
            MethodSpec {
                name: "dispatchEvent",
                length: 1,
                call: event_target_dispatch_event::<E>,
            },
        ])),
        finalizer: |_payload, _env| {},
    })
}

/// Implements the illegal `GPUDevice` constructor installed for interface identity.
pub fn device_illegal_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    Err(E::type_error(cx, "GPUDevice is not constructible"))
}

/// Finalizes a `GPUUncapturedErrorEvent`; held values are released by the adapter.
pub fn finalize_uncaptured_error_event(_payload: Box<dyn Any + Send>, _env: &Environment) {}

fn gpu_error_message_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = [
        GPU_ERROR_CLASS,
        GPU_VALIDATION_ERROR_CLASS,
        GPU_OUT_OF_MEMORY_ERROR_CLASS,
        GPU_INTERNAL_ERROR_CLASS,
    ]
    .into_iter()
    .find_map(|class| E::payload(cx, this, class))
    .and_then(|payload| payload.downcast_ref::<ErrorPayload>())
    .ok_or_else(|| E::type_error(cx, "GPUError.message called on an incompatible object"))?;
    E::string(cx, &payload.message)
}

fn gpu_error_illegal_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    Err(E::type_error(cx, "GPUError is not constructible"))
}

fn construct_gpu_error<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
    class: ClassId,
) -> Result<E::Value, E::Error> {
    let arena = Arena::new();
    let message = E::to_str(
        cx,
        args.first().copied().unwrap_or_else(|| E::undefined(cx)),
        &arena,
    )?;
    E::new_instance(
        cx,
        class,
        Box::new(ErrorPayload {
            message: message.to_owned(),
        }),
    )
}

fn gpu_validation_error_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    construct_gpu_error::<E>(cx, args, GPU_VALIDATION_ERROR_CLASS)
}

fn gpu_out_of_memory_error_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    construct_gpu_error::<E>(cx, args, GPU_OUT_OF_MEMORY_ERROR_CLASS)
}

fn gpu_internal_error_constructor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    construct_gpu_error::<E>(cx, args, GPU_INTERNAL_ERROR_CLASS)
}

fn gpu_error_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_ERROR_CLASS, || ClassSpec {
        name: "GPUError",
        id: GPU_ERROR_CLASS,
        constructor: Some(ConstructorSpec {
            length: 0,
            parent: None,
            call: gpu_error_illegal_constructor::<E>,
        }),
        properties: Box::leak(Box::new([PropertySpec {
            name: "message",
            get: Some(gpu_error_message_get::<E>),
            set: None,
        }])),
        methods: &[],
        finalizer: |_payload, _env| {},
    })
}

fn gpu_validation_error_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    error_subclass::<E>(
        GPU_VALIDATION_ERROR_CLASS,
        "GPUValidationError",
        gpu_validation_error_constructor::<E>,
    )
}

fn gpu_out_of_memory_error_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    error_subclass::<E>(
        GPU_OUT_OF_MEMORY_ERROR_CLASS,
        "GPUOutOfMemoryError",
        gpu_out_of_memory_error_constructor::<E>,
    )
}

fn gpu_internal_error_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    error_subclass::<E>(
        GPU_INTERNAL_ERROR_CLASS,
        "GPUInternalError",
        gpu_internal_error_constructor::<E>,
    )
}

fn error_subclass<E: JsEngine + 'static>(
    id: ClassId,
    name: &'static str,
    constructor: ConstructorFn<E>,
) -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(id, || ClassSpec {
        name,
        id,
        constructor: Some(ConstructorSpec {
            length: 1,
            parent: Some(ClassParent::Class(GPU_ERROR_CLASS)),
            call: constructor,
        }),
        properties: Box::leak(Box::new([PropertySpec {
            name: "message",
            get: Some(gpu_error_message_get::<E>),
            set: None,
        }])),
        methods: &[],
        finalizer: |_payload, _env| {},
    })
}

/// Implements the cached `GPUDevice.lost` getter.
pub fn device_lost_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUDevice.lost called on an incompatible object"))?;
    let state = payload
        .events
        .js
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let promise = state
        .as_ref()
        .and_then(|state| state.lost_promise.get())
        .ok_or_else(|| E::operation_error(cx, "GPUDevice.lost promise is unavailable"))?;
    Ok(E::return_held_value(cx, promise))
}

/// Implements the `GPUDevice.onuncapturederror` getter.
pub fn device_on_uncaptured_error_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUDevice.onuncapturederror called on an incompatible object",
            )
        })?;
    let state = payload
        .events
        .js
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok(state
        .as_ref()
        .and_then(|state| state.handler.get())
        .map_or_else(|| E::null(cx), |handler| E::return_held_value(cx, handler)))
}

/// Implements the `GPUDevice.onuncapturederror` setter.
pub fn device_on_uncaptured_error_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUDevice.onuncapturederror called on an incompatible object",
            )
        })?;
    let replacement =
        if E::is_null(cx, value) || E::is_undefined(cx, value) || !E::is_callable(cx, value) {
            None
        } else {
            Some(E::duplicate_value(cx, value))
        };
    let old = {
        let mut state = payload
            .events
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = state.as_mut() else {
            if let Some(replacement) = replacement {
                E::release_value(cx, replacement);
            }
            return Err(E::operation_error(
                cx,
                "GPUDevice event state is unavailable",
            ));
        };
        let old = state.handler.take();
        if let Some(replacement) = replacement {
            if old.is_none() {
                let id = state.next_listener_id;
                state.next_listener_id += 1;
                state.listeners.push(RegisteredEventListener {
                    id,
                    type_: "uncapturederror".to_owned(),
                    callback: None,
                    once: false,
                });
            }
            state.handler.set(replacement);
        } else if old.is_some() {
            state
                .listeners
                .retain(|listener| listener.callback.is_some());
        }
        old
    };
    if let Some(old) = old {
        E::release_value(cx, old);
    }
    Ok(())
}

fn listener_type<E: JsEngine>(cx: E::Context<'_>, args: &[E::Value]) -> Result<String, E::Error> {
    let arena = Arena::new();
    Ok(E::to_str(
        cx,
        args.first().copied().unwrap_or_else(|| E::undefined(cx)),
        &arena,
    )?
    .to_owned())
}

fn listener_once<E: JsEngine>(cx: E::Context<'_>, args: &[E::Value]) -> Result<bool, E::Error> {
    let Some(options) = args.get(2).copied() else {
        return Ok(false);
    };
    if !E::is_object(cx, options) {
        return Ok(false);
    }
    Ok(E::to_bool(cx, E::get_property(cx, options, "once")?))
}

fn add_listener<E: JsEngine>(
    cx: E::Context<'_>,
    entries: &mut Vec<RegisteredEventListener<E>>,
    next_listener_id: &mut u64,
    type_: String,
    callback: E::Value,
    once: bool,
) {
    if entries.iter().any(|listener| {
        listener.type_ == type_
            && listener
                .callback
                .is_some_and(|existing| E::same_value(cx, existing, callback))
    }) {
        return;
    }
    let id = *next_listener_id;
    *next_listener_id += 1;
    entries.push(RegisteredEventListener {
        id,
        type_,
        callback: Some(E::duplicate_value(cx, callback)),
        once,
    });
}

/// Implements `EventTarget.addEventListener` for `GPUDevice` and `EventTarget`.
pub fn event_target_add_event_listener<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let type_ = listener_type::<E>(cx, args)?;
    let callback = args.get(1).copied().unwrap_or_else(|| E::undefined(cx));
    if E::is_null(cx, callback) || E::is_undefined(cx, callback) || !E::is_callable(cx, callback) {
        return Ok(E::undefined(cx));
    }
    let once = listener_once::<E>(cx, args)?;
    if let Some(payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
    {
        let mut state = payload
            .events
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = state
            .as_mut()
            .ok_or_else(|| E::operation_error(cx, "GPUDevice event state is unavailable"))?;
        add_listener::<E>(
            cx,
            &mut state.listeners,
            &mut state.next_listener_id,
            type_,
            callback,
            once,
        );
        return Ok(E::undefined(cx));
    }
    let payload = E::payload(cx, this, EVENT_TARGET_CLASS)
        .and_then(|payload| payload.downcast_ref::<EventTargetPayload<E>>())
        .ok_or_else(|| E::type_error(cx, "addEventListener called on an incompatible object"))?;
    let mut listeners = payload
        .listeners
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let EventTargetListeners {
        entries,
        next_listener_id,
    } = &mut *listeners;
    add_listener::<E>(cx, entries, next_listener_id, type_, callback, once);
    Ok(E::undefined(cx))
}

fn remove_listener<E: JsEngine>(
    cx: E::Context<'_>,
    entries: &mut Vec<RegisteredEventListener<E>>,
    type_: &str,
    callback: E::Value,
) -> Option<E::Value> {
    let index = entries.iter().position(|listener| {
        listener.type_ == type_
            && listener
                .callback
                .is_some_and(|existing| E::same_value(cx, existing, callback))
    })?;
    entries.remove(index).callback
}

/// Implements `EventTarget.removeEventListener` for `GPUDevice` and `EventTarget`.
pub fn event_target_remove_event_listener<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let type_ = listener_type::<E>(cx, args)?;
    let callback = args.get(1).copied().unwrap_or_else(|| E::undefined(cx));
    let removed = if let Some(payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
    {
        let mut state = payload
            .events
            .js
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .as_mut()
            .and_then(|state| remove_listener::<E>(cx, &mut state.listeners, &type_, callback))
    } else {
        let payload = E::payload(cx, this, EVENT_TARGET_CLASS)
            .and_then(|payload| payload.downcast_ref::<EventTargetPayload<E>>())
            .ok_or_else(|| {
                E::type_error(cx, "removeEventListener called on an incompatible object")
            })?;
        let mut listeners = payload
            .listeners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        remove_listener::<E>(cx, &mut listeners.entries, &type_, callback)
    };
    if let Some(removed) = removed {
        E::release_value(cx, removed);
    }
    Ok(E::undefined(cx))
}

fn event_payload<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Option<&(dyn Any + Send)> {
    E::payload(cx, value, EVENT_CLASS)
        .or_else(|| E::payload(cx, value, GPU_UNCAPTURED_ERROR_EVENT_CLASS))
}

fn event_type_value<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    event: E::Value,
) -> Result<String, E::Error> {
    event_payload::<E>(cx, event)
        .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
        .map(|payload| payload.type_.clone())
        .ok_or_else(|| E::type_error(cx, "dispatchEvent requires an Event"))
}

fn dispatch_callbacks<E: JsEngine>(
    cx: E::Context<'_>,
    receiver: E::Value,
    event: E::Value,
    callbacks: Vec<E::Value>,
) -> Result<(), E::Error> {
    let mut first_error = None;
    for callback in callbacks {
        if let Err(error) = E::call(cx, callback, receiver, &[event]) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        E::release_value(cx, callback);
    }
    first_error.map_or(Ok(()), Err)
}

fn snapshot_device_listeners<E: JsEngine>(
    cx: E::Context<'_>,
    state: &DeviceEventState<E>,
    type_: &str,
) -> (Vec<E::Value>, Vec<E::Value>) {
    let mut state = state
        .js
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(state) = state.as_mut() else {
        return (Vec::new(), Vec::new());
    };
    let callbacks = state
        .listeners
        .iter()
        .filter(|listener| listener.type_ == type_)
        .filter_map(|listener| {
            listener
                .callback
                .or_else(|| state.handler.get())
                .map(|callback| E::duplicate_value(cx, callback))
        })
        .collect();
    let once_ids = state
        .listeners
        .iter()
        .filter(|listener| listener.type_ == type_ && listener.once)
        .map(|listener| listener.id)
        .collect::<Vec<_>>();
    let mut released = Vec::new();
    state.listeners.retain(|listener| {
        let remove = once_ids.contains(&listener.id);
        if remove {
            released.extend(listener.callback);
        }
        !remove
    });
    (callbacks, released)
}

fn snapshot_target_listeners<E: JsEngine>(
    cx: E::Context<'_>,
    payload: &EventTargetPayload<E>,
    type_: &str,
) -> (Vec<E::Value>, Vec<E::Value>) {
    let mut listeners = payload
        .listeners
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let callbacks = listeners
        .entries
        .iter()
        .filter(|listener| listener.type_ == type_)
        .filter_map(|listener| listener.callback)
        .map(|callback| E::duplicate_value(cx, callback))
        .collect();
    let once_ids = listeners
        .entries
        .iter()
        .filter(|listener| listener.type_ == type_ && listener.once)
        .map(|listener| listener.id)
        .collect::<Vec<_>>();
    let mut released = Vec::new();
    listeners.entries.retain(|listener| {
        let remove = once_ids.contains(&listener.id);
        if remove {
            released.extend(listener.callback);
        }
        !remove
    });
    (callbacks, released)
}

fn event_default_prevented<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    event: E::Value,
) -> Result<bool, E::Error> {
    event_payload::<E>(cx, event)
        .and_then(|payload| payload.downcast_ref::<EventPayload<E>>())
        .map(|payload| payload.default_prevented.load(Ordering::Acquire))
        .ok_or_else(|| {
            E::type_error(
                cx,
                "Event.defaultPrevented called on an incompatible object",
            )
        })
}

/// Implements `EventTarget.dispatchEvent` for `GPUDevice` and `EventTarget`.
pub fn event_target_dispatch_event<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let event = args.first().copied().unwrap_or_else(|| E::undefined(cx));
    let type_ = event_type_value::<E>(cx, event)?;
    let (callbacks, released) = if let Some(payload) = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
    {
        snapshot_device_listeners::<E>(cx, &payload.events, &type_)
    } else {
        let payload = E::payload(cx, this, EVENT_TARGET_CLASS)
            .and_then(|payload| payload.downcast_ref::<EventTargetPayload<E>>())
            .ok_or_else(|| E::type_error(cx, "dispatchEvent called on an incompatible object"))?;
        snapshot_target_listeners::<E>(cx, payload, &type_)
    };
    for callback in released {
        E::release_value(cx, callback);
    }
    dispatch_callbacks::<E>(cx, this, event, callbacks)?;
    Ok(E::boolean(cx, !event_default_prevented::<E>(cx, event)?))
}

fn dispatch_uncaptured_error<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    state: &DeviceEventState<E>,
    type_: WGPUErrorType,
    message: String,
) -> Result<(), E::Error> {
    let error = new_gpu_error::<E>(cx, type_, message)?;
    let event = new_gpu_uncaptured_error_event::<E>(cx, "uncapturederror".to_owned(), true, error)?;
    let (callbacks, released) = snapshot_device_listeners::<E>(cx, state, "uncapturederror");
    for callback in released {
        E::release_value(cx, callback);
    }
    dispatch_callbacks::<E>(cx, E::global(cx), event, callbacks)
}

/// Implements `GPUDevice.pushErrorScope`.
pub fn device_push_error_scope<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = device_wrapper_payload::<E>(cx, this)?;
    let filter = convert_gpu_error_filter::<E>(
        cx,
        args.first().copied().unwrap_or_else(|| E::undefined(cx)),
    )?;
    unsafe {
        (E::environment(cx).gpu().device_push_error_scope)(payload.device, filter);
    }
    payload.events.push_error_scope(filter);
    Ok(E::undefined(cx))
}

/// Implements `GPUDevice.popErrorScope`.
pub fn device_pop_error_scope<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        let payload = device_wrapper_payload::<E>(cx, this)?;
        let mut request = Box::new(PopErrorScopeRequest::<E> {
            deferred: deferred.take(),
            settlements: Arc::clone(E::environment(cx).settlements()),
            synthetic_error: payload.events.pop_error_scope(),
            state: Arc::clone(&payload.events),
            _registration: None,
        });
        request._registration = Some(E::register_deferred(
            cx,
            NonNull::from(&mut request.deferred),
        ));
        let info = WGPUPopErrorScopeCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(pop_error_scope_callback::<E>),
            userdata1: Box::into_raw(request).cast(),
            userdata2: ptr::null_mut(),
        };
        unsafe {
            (E::environment(cx).gpu().device_pop_error_scope)(payload.device, info);
        }
        Ok(())
    })
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
    if converted.mapped_at_creation && converted.size % 4 != 0 {
        return Err(E::range_error(
            cx,
            "mappedAtCreation buffer size must be a multiple of 4",
        ));
    }
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
        pending_map: None,
        canceling_map: None,
        next_map_id: 0,
        map_mode: if converted.mapped_at_creation {
            WGPUMapMode_Write
        } else {
            0
        },
        error_sink: Arc::clone(&device_payload.events) as Arc<dyn DeviceErrorSink>,
        ranges: Vec::new(),
    };
    match E::new_instance(
        cx,
        GPU_BUFFER_CLASS,
        Box::new(BufferPayload::<E> {
            state: Arc::new(Mutex::new(state)),
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
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if !state.destroyed {
            state.canceling_map = state.pending_map.take();
            detach_all_ranges::<E>(cx, state, false)?;
            unsafe {
                (E::environment(cx).gpu().buffer_destroy)(state.buffer);
            }
            state.destroyed = true;
            state.mapped = false;
            state.map_mode = 0;
        }
        Ok(E::undefined(cx))
    })
}

/// Implements `GPUTexture.destroy`; native destruction is distinct from wrapper release.
pub fn texture_destroy<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_TEXTURE_CLASS)
        .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        .ok_or_else(|| E::type_error(cx, "GPUTexture.destroy called on an incompatible object"))?;
    if !payload.destroyed.swap(true, Ordering::AcqRel) {
        unsafe {
            (E::environment(cx).gpu().texture_destroy)(payload.texture);
        }
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUQuerySet.destroy`; native destruction is distinct from wrapper release.
pub fn query_set_destroy<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_QUERY_SET_CLASS)
        .and_then(|payload| payload.downcast_ref::<QuerySetPayload>())
        .ok_or_else(|| E::type_error(cx, "GPUQuerySet.destroy called on an incompatible object"))?;
    if !payload.destroyed.swap(true, Ordering::AcqRel) {
        unsafe {
            (E::environment(cx).gpu().query_set_destroy)(payload.query_set);
        }
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUDevice.destroy`; native destruction is distinct from wrapper release.
pub fn device_destroy<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = device_wrapper_payload::<E>(cx, this)?;
    if !payload.destroyed.swap(true, Ordering::AcqRel) {
        payload.events.mark_lost();
        unsafe {
            (E::environment(cx).gpu().device_destroy)(payload.device);
        }
    }
    Ok(E::undefined(cx))
}

/// Implements `GPU.requestAdapter`.
pub fn gpu_request_adapter<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        let Some(payload) = E::payload(cx, this, GPU_CLASS)
            .and_then(|payload| payload.downcast_ref::<GpuPayload>())
        else {
            return Err(E::type_error(
                cx,
                "GPU.requestAdapter called on an incompatible object",
            ));
        };
        let mut request = Box::new(AdapterRequest::<E> {
            deferred: deferred.take(),
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
            (E::environment(cx).gpu().instance_request_adapter)(
                payload.instance,
                ptr::null(),
                info,
            );
        }
        Ok(())
    })
}

/// Implements `GPUAdapter.requestDevice`.
pub fn adapter_request_device<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        adapter_request_device_inner::<E>(cx, this, args, deferred)
    })
}

fn adapter_request_device_inner<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
    deferred: &mut Option<Deferred<E>>,
) -> Result<(), E::Error> {
    let Some(payload) = E::payload(cx, this, GPU_ADAPTER_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterPayload<E>>())
    else {
        return Err(E::type_error(
            cx,
            "GPUAdapter.requestDevice called on an incompatible object",
        ));
    };
    let descriptor_value = args.first().copied().unwrap_or_else(|| E::undefined(cx));
    let label_value = dictionary_member::<E>(cx, descriptor_value, "label")?;
    let label_arena = Arena::new();
    let label = if E::is_undefined(cx, label_value) {
        String::new()
    } else {
        E::to_str(cx, label_value, &label_arena)?.to_owned()
    };
    let required_features_value = dictionary_member::<E>(cx, descriptor_value, "requiredFeatures")?;
    let required_features = if E::is_undefined(cx, required_features_value) {
        Vec::new()
    } else {
        convert_sequence::<E, _>(cx, required_features_value, "requiredFeatures", |value| {
            convert_gpu_feature_name::<E>(cx, value)
        })?
    };
    let required_limits_value = dictionary_member::<E>(cx, descriptor_value, "requiredLimits")?;
    let required_limit_names = if E::is_undefined(cx, required_limits_value) {
        Vec::new()
    } else {
        if !E::is_object(cx, required_limits_value) {
            return Err(E::type_error(cx, "requiredLimits"));
        }
        E::own_property_names(cx, required_limits_value)?
    };
    if let Some(name) = required_limit_names
        .iter()
        .find(|name| !is_known_required_limit(name))
    {
        return Err(E::operation_error(
            cx,
            &format!("unknown required limit: {name}"),
        ));
    }
    let (mut required_limits, mut compatibility_limits) =
        convert_required_limits::<E>(cx, required_limits_value, &required_limit_names)?;
    required_limits.nextInChain = ptr::from_mut(&mut compatibility_limits.chain);

    let arena = Arena::new();
    let required_features = arena.alloc_slice(required_features);
    let required_features_ptr = if required_features.is_empty() {
        ptr::null()
    } else {
        required_features.as_ptr()
    };
    let required_limits_ptr = if required_limit_names.is_empty() {
        ptr::null()
    } else {
        ptr::from_ref(&required_limits)
    };

    let events = DeviceEventState::<E>::new(Arc::clone(E::environment(cx).settlements()));
    let mut request = Box::new(DeviceRequest::<E> {
        deferred: deferred.take(),
        settlements: Arc::clone(E::environment(cx).settlements()),
        release_queue: Arc::clone(E::environment(cx).queue()),
        gpu: E::environment(cx).gpu(),
        events: Arc::clone(&events),
        label: label.clone(),
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
    // One callback-owned strong reference is shared by both callback infos.
    // webgpu.h guarantees uncaptured-error callbacks stop before device loss,
    // so the terminal device-lost callback can reclaim this reference.
    let event_userdata = Arc::into_raw(Arc::clone(&events))
        .cast_mut()
        .cast::<c_void>();
    let descriptor = WGPUDeviceDescriptor {
        nextInChain: ptr::null_mut(),
        label: WGPUStringView::from_bytes(label.as_bytes()),
        requiredFeatureCount: required_features.len(),
        requiredFeatures: required_features_ptr,
        requiredLimits: required_limits_ptr,
        defaultQueue: WGPUQueueDescriptor {
            nextInChain: ptr::null_mut(),
            label: WGPUStringView::from_bytes(b""),
        },
        deviceLostCallbackInfo: WGPUDeviceLostCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(device_lost_callback::<E>),
            userdata1: event_userdata,
            userdata2: ptr::null_mut(),
        },
        uncapturedErrorCallbackInfo: WGPUUncapturedErrorCallbackInfo {
            nextInChain: ptr::null_mut(),
            callback: Some(uncaptured_error_callback::<E>),
            userdata1: event_userdata,
            userdata2: ptr::null_mut(),
        },
    };
    unsafe {
        (E::environment(cx).gpu().adapter_request_device)(
            payload.adapter,
            ptr::from_ref(&descriptor),
            info,
        );
    }
    Ok(())
}

/// Implements `GPUDevice.createComputePipelineAsync`.
pub fn device_create_compute_pipeline_async<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        let payload = device_wrapper_payload::<E>(cx, this)?;
        let arena = Arena::new();
        let descriptor = required_argument::<E>(cx, args, 0, "GPUComputePipelineDescriptor")?;
        let converted = convert_compute_pipeline_descriptor::<E>(cx, descriptor, &arena)?;
        let label = unsafe { string_view_to_owned(converted.native.label) };
        let gpu = E::environment(cx).gpu();
        let _ = E::register_class(cx, compute_pipeline_class::<E>())?;
        unsafe {
            (gpu.shader_module_add_ref)(converted.module);
            if !converted.layout.is_null() {
                (gpu.pipeline_layout_add_ref)(converted.layout);
            }
        }
        let mut request = Box::new(ComputePipelineRequest::<E> {
            deferred: deferred.take(),
            settlements: Arc::clone(E::environment(cx).settlements()),
            release_queue: Arc::clone(E::environment(cx).queue()),
            gpu,
            state: Arc::clone(&payload.events),
            lost_at_start: payload.events.is_lost(),
            module: converted.module,
            layout: converted.layout,
            label,
            _registration: None,
        });
        request._registration = Some(E::register_deferred(
            cx,
            NonNull::from(&mut request.deferred),
        ));
        let info = WGPUCreateComputePipelineAsyncCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(create_compute_pipeline_callback::<E>),
            userdata1: Box::into_raw(request).cast(),
            userdata2: ptr::null_mut(),
        };
        unsafe {
            (gpu.device_create_compute_pipeline_async)(
                payload.device,
                ptr::from_ref(&converted.native),
                info,
            );
        }
        Ok(())
    })
}

/// Implements `GPUDevice.createRenderPipelineAsync`.
pub fn device_create_render_pipeline_async<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        let payload = device_wrapper_payload::<E>(cx, this)?;
        let arena = Arena::new();
        let descriptor = required_argument::<E>(cx, args, 0, "GPURenderPipelineDescriptor")?;
        let converted = convert_render_pipeline_descriptor::<E>(cx, descriptor, &arena)?;
        let label = unsafe { string_view_to_owned(converted.native.label) };
        let gpu = E::environment(cx).gpu();
        let _ = E::register_class(cx, render_pipeline_class::<E>())?;
        unsafe {
            (gpu.shader_module_add_ref)(converted.vertex_module);
            if !converted.fragment_module.is_null() {
                (gpu.shader_module_add_ref)(converted.fragment_module);
            }
            if !converted.layout.is_null() {
                (gpu.pipeline_layout_add_ref)(converted.layout);
            }
        }
        let mut request = Box::new(RenderPipelineRequest::<E> {
            deferred: deferred.take(),
            settlements: Arc::clone(E::environment(cx).settlements()),
            release_queue: Arc::clone(E::environment(cx).queue()),
            gpu,
            state: Arc::clone(&payload.events),
            lost_at_start: payload.events.is_lost(),
            vertex_module: converted.vertex_module,
            fragment_module: converted.fragment_module,
            layout: converted.layout,
            label,
            _registration: None,
        });
        request._registration = Some(E::register_deferred(
            cx,
            NonNull::from(&mut request.deferred),
        ));
        let info = WGPUCreateRenderPipelineAsyncCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(create_render_pipeline_callback::<E>),
            userdata1: Box::into_raw(request).cast(),
            userdata2: ptr::null_mut(),
        };
        unsafe {
            (gpu.device_create_render_pipeline_async)(
                payload.device,
                ptr::from_ref(&converted.native),
                info,
            );
        }
        Ok(())
    })
}

/// Implements `GPUBuffer.mapAsync`.
pub fn buffer_map_async<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    promise_operation::<E>(cx, |deferred| {
        buffer_map_async_inner::<E>(cx, this, args, deferred)
    })
}

fn buffer_map_async_inner<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
    deferred: &mut Option<Deferred<E>>,
) -> Result<(), E::Error> {
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
    let (buffer, state, map_id, defer_native_start) = {
        let Ok(mut state) = payload.state.lock() else {
            return Err(E::operation_error(cx, "GPUBuffer state is poisoned"));
        };
        if state.destroyed {
            let error_sink = Arc::clone(&state.error_sink);
            drop(state);
            error_sink.generate_validation_error("GPUBuffer is destroyed".to_owned());
            let deferred = deferred
                .take()
                .ok_or_else(|| E::operation_error(cx, "mapAsync deferred is unavailable"))?;
            E::environment(cx)
                .settlements()
                .enqueue::<E>(SettlementRequest::Error {
                    deferred,
                    name: "OperationError",
                    message: "GPUBuffer is destroyed".to_owned(),
                })
                .map_err(|_| E::operation_error(cx, "mapAsync settlement queue is unavailable"))?;
            return Ok(());
        }
        if state.mapped || state.pending_map.is_some() {
            let error_sink = Arc::clone(&state.error_sink);
            drop(state);
            error_sink.generate_validation_error(
                "GPUBuffer.mapAsync requires the buffer to be unmapped".to_owned(),
            );
            return Err(E::operation_error(
                cx,
                "GPUBuffer.mapAsync requires the buffer to be unmapped",
            ));
        }
        state.next_map_id = state.next_map_id.wrapping_add(1);
        let map_id = state.next_map_id;
        state.pending_map = Some(map_id);
        (
            state.buffer,
            Arc::clone(&payload.state),
            map_id,
            state.canceling_map.is_some() && !state.destroyed,
        )
    };
    let mut request = Box::new(MapRequest::<E> {
        deferred: deferred.take(),
        settlements: Arc::clone(E::environment(cx).settlements()),
        _registration: None,
        mode,
        map_id,
        state,
    });
    request._registration = Some(E::register_deferred(
        cx,
        NonNull::from(&mut request.deferred),
    ));
    let gpu = E::environment(cx).gpu();
    if defer_native_start {
        E::environment(cx)
            .settlements()
            .enqueue::<E>(SettlementRequest::StartMap {
                buffer,
                mode,
                offset,
                size,
                request,
                gpu,
            })
            .map_err(|_| E::operation_error(cx, "mapAsync start queue is unavailable"))?;
    } else {
        start_buffer_map(buffer, mode, offset, size, request, gpu);
    }
    Ok(())
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
    with_buffer_state::<E, _, _>(cx, this, |state| {
        if state.destroyed || !state.mapped {
            return Err(E::operation_error(cx, "buffer is not mapped"));
        }
        let size = match explicit_size {
            Some(size) => size,
            None => usize::try_from(state.size.saturating_sub(offset as u64))
                .ok()
                .filter(|len| *len <= u32::MAX as usize)
                .ok_or_else(|| E::operation_error(cx, "mapped range size is unsupported"))?,
        };
        if state
            .ranges
            .iter()
            .any(|range| mapped_ranges_overlap(offset, size, range.offset, range.size))
        {
            return Err(E::operation_error(
                cx,
                "mapped range overlaps an existing mapped range",
            ));
        }
        let ptr = mapped_range_ptr::<E>(cx, state, offset, size);
        if ptr.is_null() {
            return Err(E::operation_error(
                cx,
                "wgpuBufferGetMappedRange returned null for current map mode",
            ));
        }
        // SAFETY: `mapped_range_ptr` returned a non-null mapped range for
        // `size` bytes, and the native range remains valid until unmap.
        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) };
        let value = E::new_arraybuffer_copy(cx, bytes)?;
        let tracked = E::duplicate_value(cx, value);
        state.ranges.push(MappedRange {
            value: tracked,
            offset,
            size,
            native_ptr: ptr,
            map_mode: state.map_mode,
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
        let pending = state.pending_map.take();
        if pending.is_some() {
            state.canceling_map = pending;
        }
        if state.mapped {
            detach_all_ranges::<E>(cx, state, true)?;
        }
        if state.mapped || pending.is_some() {
            unsafe { (E::environment(cx).gpu().buffer_unmap)(state.buffer) };
        }
        state.mapped = false;
        state.map_mode = 0;
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

fn stored_label_get<E: JsEngine>(
    cx: E::Context<'_>,
    label: &Mutex<String>,
    poisoned: &'static str,
) -> Result<E::Value, E::Error> {
    let label = label.lock().map_err(|_| E::operation_error(cx, poisoned))?;
    E::string(cx, &label)
}

fn converted_label<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<String, E::Error> {
    let arena = Arena::new();
    Ok(E::to_str(cx, value, &arena)?.to_owned())
}

fn store_label<E: JsEngine>(
    cx: E::Context<'_>,
    stored: &Mutex<String>,
    label: String,
    poisoned: &'static str,
) -> Result<(), E::Error> {
    *stored
        .lock()
        .map_err(|_| E::operation_error(cx, poisoned))? = label;
    Ok(())
}

/// Implements the readonly `GPUBuffer.mapState` getter.
pub fn buffer_map_state_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    with_buffer_state::<E, _, _>(cx, this, |state| {
        let map_state = if state.mapped {
            "mapped"
        } else if state.pending_map.is_some() {
            "pending"
        } else {
            "unmapped"
        };
        E::string(cx, map_state)
    })
}

/// Implements the `GPUDevice.label` getter.
pub fn device_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUDevice.label called on an incompatible object"))?;
    stored_label_get::<E>(cx, &payload.label, "GPUDevice label is poisoned")
}

/// Implements the `GPUDevice.label` setter.
pub fn device_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUDevice.label called on an incompatible object"))?;
    unsafe {
        (E::environment(cx).gpu().device_set_label)(
            payload.device,
            WGPUStringView::from_bytes(label.as_bytes()),
        );
    }
    store_label::<E>(cx, &payload.label, label, "GPUDevice label is poisoned")
}

/// Implements the `GPUQueue.label` getter.
pub fn queue_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
        .ok_or_else(|| E::type_error(cx, "GPUQueue.label called on an incompatible object"))?;
    stored_label_get::<E>(cx, &payload.label, "GPUQueue label is poisoned")
}

/// Implements the `GPUQueue.label` setter.
pub fn queue_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
        .ok_or_else(|| E::type_error(cx, "GPUQueue.label called on an incompatible object"))?;
    unsafe {
        (E::environment(cx).gpu().queue_set_label)(
            payload.queue,
            WGPUStringView::from_bytes(label.as_bytes()),
        );
    }
    store_label::<E>(cx, &payload.label, label, "GPUQueue label is poisoned")
}

/// Implements the `GPUComputePassEncoder.label` getter.
pub fn compute_pass_encoder_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUComputePassEncoder.label called on an incompatible object",
            )
        })?;
    stored_label_get::<E>(
        cx,
        &payload.label,
        "GPUComputePassEncoder label is poisoned",
    )
}

/// Implements the `GPUComputePassEncoder.label` setter.
pub fn compute_pass_encoder_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUComputePassEncoder.label called on an incompatible object",
            )
        })?;
    let pass = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUComputePassEncoder state is poisoned"))?
        .pass;
    if !pass.is_null() {
        unsafe {
            (E::environment(cx).gpu().compute_pass_encoder_set_label)(
                pass,
                WGPUStringView::from_bytes(label.as_bytes()),
            );
        }
    }
    store_label::<E>(
        cx,
        &payload.label,
        label,
        "GPUComputePassEncoder label is poisoned",
    )
}

/// Implements the `GPURenderPassEncoder.label` getter.
pub fn render_pass_encoder_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPURenderPassEncoder.label called on an incompatible object",
            )
        })?;
    stored_label_get::<E>(cx, &payload.label, "GPURenderPassEncoder label is poisoned")
}

/// Implements the `GPURenderPassEncoder.label` setter.
pub fn render_pass_encoder_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPURenderPassEncoder.label called on an incompatible object",
            )
        })?;
    let pass = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPURenderPassEncoder state is poisoned"))?
        .pass;
    if !pass.is_null() {
        unsafe {
            (E::environment(cx).gpu().render_pass_encoder_set_label)(
                pass,
                WGPUStringView::from_bytes(label.as_bytes()),
            );
        }
    }
    store_label::<E>(
        cx,
        &payload.label,
        label,
        "GPURenderPassEncoder label is poisoned",
    )
}

/// Implements the `GPUCommandBuffer.label` getter.
pub fn command_buffer_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_COMMAND_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<CommandBufferPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUCommandBuffer.label called on an incompatible object",
            )
        })?;
    stored_label_get::<E>(cx, &payload.label, "GPUCommandBuffer label is poisoned")
}

/// Implements the `GPUCommandBuffer.label` setter.
pub fn command_buffer_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_COMMAND_BUFFER_CLASS)
        .and_then(|payload| payload.downcast_ref::<CommandBufferPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUCommandBuffer.label called on an incompatible object",
            )
        })?;
    let command_buffer = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandBuffer state is poisoned"))?
        .command_buffer;
    if !command_buffer.is_null() {
        unsafe {
            (E::environment(cx).gpu().command_buffer_set_label)(
                command_buffer,
                WGPUStringView::from_bytes(label.as_bytes()),
            );
        }
    }
    store_label::<E>(
        cx,
        &payload.label,
        label,
        "GPUCommandBuffer label is poisoned",
    )
}

/// Implements the `GPURenderBundle.label` getter.
pub fn render_bundle_label_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_RENDER_BUNDLE_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundlePayload>())
        .ok_or_else(|| {
            E::type_error(cx, "GPURenderBundle.label called on an incompatible object")
        })?;
    stored_label_get::<E>(cx, &payload.label, "GPURenderBundle label is poisoned")
}

/// Implements the `GPURenderBundle.label` setter.
pub fn render_bundle_label_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    value: E::Value,
) -> Result<(), E::Error> {
    let label = converted_label::<E>(cx, value)?;
    let payload = E::payload(cx, this, GPU_RENDER_BUNDLE_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundlePayload>())
        .ok_or_else(|| {
            E::type_error(cx, "GPURenderBundle.label called on an incompatible object")
        })?;
    if !payload.render_bundle.is_null() {
        unsafe {
            (E::environment(cx).gpu().render_bundle_set_label)(
                payload.render_bundle,
                WGPUStringView::from_bytes(label.as_bytes()),
            );
        }
    }
    store_label::<E>(
        cx,
        &payload.label,
        label,
        "GPURenderBundle label is poisoned",
    )
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

/// Implements `GPUTexture.textureBindingViewDimension` outside compatibility mode.
pub fn texture_binding_view_dimension_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    E::payload(cx, this, GPU_TEXTURE_CLASS)
        .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUTexture.textureBindingViewDimension called on an incompatible object",
            )
        })?;
    Ok(E::undefined(cx))
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
    match E::new_instance(
        cx,
        GPU_QUEUE_CLASS,
        Box::new(QueuePayload {
            queue,
            label: Mutex::new(String::new()),
        }),
    ) {
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

/// Implements the cached `GPUAdapter.features` getter.
pub fn adapter_features_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterPayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUAdapter.features called on an incompatible object"))?;
    if let Some(value) = payload.features.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_feature_set::<E>(cx, FeatureSource::Adapter(payload.adapter))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.features.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements the cached `GPUDevice.features` getter.
pub fn device_features_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = device_wrapper_payload::<E>(cx, this)?;
    if let Some(value) = payload.features.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_feature_set::<E>(cx, FeatureSource::Device(payload.device))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.features.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements the cached `GPUAdapter.limits` getter.
pub fn adapter_limits_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterPayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUAdapter.limits called on an incompatible object"))?;
    if let Some(value) = payload.limits.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_supported_limits::<E>(cx, LimitsSource::Adapter(payload.adapter))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.limits.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements the cached `GPUDevice.limits` getter.
pub fn device_limits_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = device_wrapper_payload::<E>(cx, this)?;
    if let Some(value) = payload.limits.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_supported_limits::<E>(cx, LimitsSource::Device(payload.device))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.limits.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements the cached `GPUAdapter.info` getter.
pub fn adapter_info_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterPayload<E>>())
        .ok_or_else(|| E::type_error(cx, "GPUAdapter.info called on an incompatible object"))?;
    if let Some(value) = payload.info.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_adapter_info::<E>(cx, AdapterInfoSource::Adapter(payload.adapter))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.info.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements the cached `GPUDevice.adapterInfo` getter.
pub fn device_adapter_info_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = device_wrapper_payload::<E>(cx, this)?;
    if let Some(value) = payload.adapter_info.get() {
        return Ok(E::return_held_value(cx, value));
    }
    let value = new_adapter_info::<E>(cx, AdapterInfoSource::Device(payload.device))?;
    let duplicate = E::duplicate_value(cx, value);
    if let Some(incumbent) = payload.adapter_info.set_if_empty(duplicate) {
        E::release_value(cx, duplicate);
        return Ok(E::return_held_value(cx, incumbent));
    }
    Ok(value)
}

/// Implements `GPUComputePipeline.getBindGroupLayout`.
pub fn compute_pipeline_get_bind_group_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_COMPUTE_PIPELINE_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePipelinePayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUComputePipeline.getBindGroupLayout called on an incompatible object",
            )
        })?;
    let index = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "index"))?;
    let index = enforce_u32::<E>(cx, index, "index")?;
    new_derived_bind_group_layout::<E>(cx, PipelineParent::Compute(payload.pipeline), index)
}

/// Implements `GPURenderPipeline.getBindGroupLayout`.
pub fn render_pipeline_get_bind_group_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_RENDER_PIPELINE_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPipelinePayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPURenderPipeline.getBindGroupLayout called on an incompatible object",
            )
        })?;
    let index = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "index"))?;
    let index = enforce_u32::<E>(cx, index, "index")?;
    new_derived_bind_group_layout::<E>(cx, PipelineParent::Render(payload.render_pipeline), index)
}

fn new_derived_bind_group_layout<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    parent: PipelineParent,
    index: u32,
) -> Result<E::Value, E::Error> {
    let gpu = E::environment(cx).gpu();
    // SAFETY: the parent handle is live and belongs to this dispatch table.
    let layout = unsafe {
        match parent {
            PipelineParent::Compute(pipeline) => {
                (gpu.compute_pipeline_get_bind_group_layout)(pipeline, index)
            }
            PipelineParent::Render(pipeline) => {
                (gpu.render_pipeline_get_bind_group_layout)(pipeline, index)
            }
        }
    };
    if layout.is_null() {
        return Err(E::operation_error(cx, "getBindGroupLayout returned null"));
    }
    if let Err(error) = E::register_class(cx, bind_group_layout_class::<E>()) {
        // SAFETY: `layout` is the non-null owned result returned above.
        unsafe { (gpu.bind_group_layout_release)(layout) };
        return Err(error);
    }
    // SAFETY: the parent handle is live; this reference is balanced by the
    // derived layout finalizer or the allocation-failure cleanup below.
    unsafe {
        match parent {
            PipelineParent::Compute(pipeline) => (gpu.compute_pipeline_add_ref)(pipeline),
            PipelineParent::Render(pipeline) => (gpu.render_pipeline_add_ref)(pipeline),
        }
    }
    match E::new_instance(
        cx,
        GPU_BIND_GROUP_LAYOUT_CLASS,
        Box::new(BindGroupLayoutPayload {
            layout,
            parent_pipeline: Some(parent),
            label: Mutex::new(String::new()),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            // SAFETY: both owned references were acquired above and have not
            // been released yet.
            unsafe {
                (gpu.bind_group_layout_release)(layout);
                match parent {
                    PipelineParent::Compute(pipeline) => (gpu.compute_pipeline_release)(pipeline),
                    PipelineParent::Render(pipeline) => (gpu.render_pipeline_release)(pipeline),
                }
            }
            Err(error)
        }
    }
}

fn supported_limit_value<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    read: impl FnOnce(&SupportedLimitsPayload) -> u64,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_SUPPORTED_LIMITS_CLASS)
        .and_then(|payload| payload.downcast_ref::<SupportedLimitsPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUSupportedLimits getter called on an incompatible object",
            )
        })?;
    let value = read(payload);
    if value > 9_007_199_254_740_991 {
        return Err(E::operation_error(
            cx,
            "WebGPU limit exceeds JavaScript's exact integer range",
        ));
    }
    E::number(cx, value as f64)
}

macro_rules! supported_limit_getters {
    ($(($function:ident, $field:ident, $source:ident)),+ $(,)?) => {$ (
        fn $function<E: JsEngine + 'static>(
            cx: E::Context<'_>,
            this: E::Value,
        ) -> Result<E::Value, E::Error> {
            supported_limit_value::<E>(cx, this, |payload| payload.$source.$field as u64)
        }
    )+};
}

supported_limit_getters!(
    (
        limit_max_texture_dimension_1d,
        maxTextureDimension1D,
        limits
    ),
    (
        limit_max_texture_dimension_2d,
        maxTextureDimension2D,
        limits
    ),
    (
        limit_max_texture_dimension_3d,
        maxTextureDimension3D,
        limits
    ),
    (
        limit_max_texture_array_layers,
        maxTextureArrayLayers,
        limits
    ),
    (limit_max_bind_groups, maxBindGroups, limits),
    (
        limit_max_bind_groups_plus_vertex_buffers,
        maxBindGroupsPlusVertexBuffers,
        limits
    ),
    (limit_max_immediate_size, maxImmediateSize, limits),
    (
        limit_max_bindings_per_bind_group,
        maxBindingsPerBindGroup,
        limits
    ),
    (
        limit_max_dynamic_uniform_buffers_per_pipeline_layout,
        maxDynamicUniformBuffersPerPipelineLayout,
        limits
    ),
    (
        limit_max_dynamic_storage_buffers_per_pipeline_layout,
        maxDynamicStorageBuffersPerPipelineLayout,
        limits
    ),
    (
        limit_max_sampled_textures_per_shader_stage,
        maxSampledTexturesPerShaderStage,
        limits
    ),
    (
        limit_max_samplers_per_shader_stage,
        maxSamplersPerShaderStage,
        limits
    ),
    (
        limit_max_storage_buffers_per_shader_stage,
        maxStorageBuffersPerShaderStage,
        limits
    ),
    (
        limit_max_storage_buffers_in_vertex_stage,
        maxStorageBuffersInVertexStage,
        compatibility
    ),
    (
        limit_max_storage_buffers_in_fragment_stage,
        maxStorageBuffersInFragmentStage,
        compatibility
    ),
    (
        limit_max_storage_textures_per_shader_stage,
        maxStorageTexturesPerShaderStage,
        limits
    ),
    (
        limit_max_storage_textures_in_vertex_stage,
        maxStorageTexturesInVertexStage,
        compatibility
    ),
    (
        limit_max_storage_textures_in_fragment_stage,
        maxStorageTexturesInFragmentStage,
        compatibility
    ),
    (
        limit_max_uniform_buffers_per_shader_stage,
        maxUniformBuffersPerShaderStage,
        limits
    ),
    (
        limit_max_uniform_buffer_binding_size,
        maxUniformBufferBindingSize,
        limits
    ),
    (
        limit_max_storage_buffer_binding_size,
        maxStorageBufferBindingSize,
        limits
    ),
    (
        limit_min_uniform_buffer_offset_alignment,
        minUniformBufferOffsetAlignment,
        limits
    ),
    (
        limit_min_storage_buffer_offset_alignment,
        minStorageBufferOffsetAlignment,
        limits
    ),
    (limit_max_vertex_buffers, maxVertexBuffers, limits),
    (limit_max_buffer_size, maxBufferSize, limits),
    (limit_max_vertex_attributes, maxVertexAttributes, limits),
    (
        limit_max_vertex_buffer_array_stride,
        maxVertexBufferArrayStride,
        limits
    ),
    (
        limit_max_inter_stage_shader_variables,
        maxInterStageShaderVariables,
        limits
    ),
    (limit_max_color_attachments, maxColorAttachments, limits),
    (
        limit_max_color_attachment_bytes_per_sample,
        maxColorAttachmentBytesPerSample,
        limits
    ),
    (
        limit_max_compute_workgroup_storage_size,
        maxComputeWorkgroupStorageSize,
        limits
    ),
    (
        limit_max_compute_invocations_per_workgroup,
        maxComputeInvocationsPerWorkgroup,
        limits
    ),
    (
        limit_max_compute_workgroup_size_x,
        maxComputeWorkgroupSizeX,
        limits
    ),
    (
        limit_max_compute_workgroup_size_y,
        maxComputeWorkgroupSizeY,
        limits
    ),
    (
        limit_max_compute_workgroup_size_z,
        maxComputeWorkgroupSizeZ,
        limits
    ),
    (
        limit_max_compute_workgroups_per_dimension,
        maxComputeWorkgroupsPerDimension,
        limits
    ),
);

fn adapter_info_string_get<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    read: impl FnOnce(&AdapterInfoPayload) -> &str,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_INFO_CLASS)
        .and_then(|payload| payload.downcast_ref::<AdapterInfoPayload>())
        .ok_or_else(|| {
            E::type_error(cx, "GPUAdapterInfo getter called on an incompatible object")
        })?;
    E::string(cx, read(payload))
}

fn adapter_info_vendor<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    adapter_info_string_get::<E>(cx, this, |p| &p.vendor)
}
fn adapter_info_architecture<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    adapter_info_string_get::<E>(cx, this, |p| &p.architecture)
}
fn adapter_info_device<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    adapter_info_string_get::<E>(cx, this, |p| &p.device)
}
fn adapter_info_description<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    adapter_info_string_get::<E>(cx, this, |p| &p.description)
}

fn adapter_info_subgroup_min_size<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_INFO_CLASS)
        .and_then(|p| p.downcast_ref::<AdapterInfoPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUAdapterInfo.subgroupMinSize called on an incompatible object",
            )
        })?;
    E::number(cx, payload.subgroup_min_size as f64)
}
fn adapter_info_subgroup_max_size<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_INFO_CLASS)
        .and_then(|p| p.downcast_ref::<AdapterInfoPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUAdapterInfo.subgroupMaxSize called on an incompatible object",
            )
        })?;
    E::number(cx, payload.subgroup_max_size as f64)
}
fn adapter_info_is_fallback<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_ADAPTER_INFO_CLASS)
        .and_then(|p| p.downcast_ref::<AdapterInfoPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPUAdapterInfo.isFallbackAdapter called on an incompatible object",
            )
        })?;
    let scalar = E::number(cx, f64::from(payload.is_fallback_adapter))?;
    let global = E::global(cx);
    let boolean = E::get_property(cx, global, "Boolean")?;
    E::call(cx, boolean, E::undefined(cx), &[scalar])
}

fn finalize_value_payload(_payload: Box<dyn Any + Send>, _env: &Environment) {}

fn supported_limits_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_SUPPORTED_LIMITS_CLASS, || ClassSpec {
        name: "GPUSupportedLimits",
        id: GPU_SUPPORTED_LIMITS_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "maxTextureDimension1D",
                get: Some(limit_max_texture_dimension_1d::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxTextureDimension2D",
                get: Some(limit_max_texture_dimension_2d::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxTextureDimension3D",
                get: Some(limit_max_texture_dimension_3d::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxTextureArrayLayers",
                get: Some(limit_max_texture_array_layers::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxBindGroups",
                get: Some(limit_max_bind_groups::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxBindGroupsPlusVertexBuffers",
                get: Some(limit_max_bind_groups_plus_vertex_buffers::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxImmediateSize",
                get: Some(limit_max_immediate_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxBindingsPerBindGroup",
                get: Some(limit_max_bindings_per_bind_group::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxDynamicUniformBuffersPerPipelineLayout",
                get: Some(limit_max_dynamic_uniform_buffers_per_pipeline_layout::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxDynamicStorageBuffersPerPipelineLayout",
                get: Some(limit_max_dynamic_storage_buffers_per_pipeline_layout::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxSampledTexturesPerShaderStage",
                get: Some(limit_max_sampled_textures_per_shader_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxSamplersPerShaderStage",
                get: Some(limit_max_samplers_per_shader_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageBuffersPerShaderStage",
                get: Some(limit_max_storage_buffers_per_shader_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageBuffersInVertexStage",
                get: Some(limit_max_storage_buffers_in_vertex_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageBuffersInFragmentStage",
                get: Some(limit_max_storage_buffers_in_fragment_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageTexturesPerShaderStage",
                get: Some(limit_max_storage_textures_per_shader_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageTexturesInVertexStage",
                get: Some(limit_max_storage_textures_in_vertex_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageTexturesInFragmentStage",
                get: Some(limit_max_storage_textures_in_fragment_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxUniformBuffersPerShaderStage",
                get: Some(limit_max_uniform_buffers_per_shader_stage::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxUniformBufferBindingSize",
                get: Some(limit_max_uniform_buffer_binding_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxStorageBufferBindingSize",
                get: Some(limit_max_storage_buffer_binding_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "minUniformBufferOffsetAlignment",
                get: Some(limit_min_uniform_buffer_offset_alignment::<E>),
                set: None,
            },
            PropertySpec {
                name: "minStorageBufferOffsetAlignment",
                get: Some(limit_min_storage_buffer_offset_alignment::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxVertexBuffers",
                get: Some(limit_max_vertex_buffers::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxBufferSize",
                get: Some(limit_max_buffer_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxVertexAttributes",
                get: Some(limit_max_vertex_attributes::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxVertexBufferArrayStride",
                get: Some(limit_max_vertex_buffer_array_stride::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxInterStageShaderVariables",
                get: Some(limit_max_inter_stage_shader_variables::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxColorAttachments",
                get: Some(limit_max_color_attachments::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxColorAttachmentBytesPerSample",
                get: Some(limit_max_color_attachment_bytes_per_sample::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeWorkgroupStorageSize",
                get: Some(limit_max_compute_workgroup_storage_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeInvocationsPerWorkgroup",
                get: Some(limit_max_compute_invocations_per_workgroup::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeWorkgroupSizeX",
                get: Some(limit_max_compute_workgroup_size_x::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeWorkgroupSizeY",
                get: Some(limit_max_compute_workgroup_size_y::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeWorkgroupSizeZ",
                get: Some(limit_max_compute_workgroup_size_z::<E>),
                set: None,
            },
            PropertySpec {
                name: "maxComputeWorkgroupsPerDimension",
                get: Some(limit_max_compute_workgroups_per_dimension::<E>),
                set: None,
            },
        ])),
        methods: &[],
        finalizer: finalize_value_payload,
    })
}

fn adapter_info_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_ADAPTER_INFO_CLASS, || ClassSpec {
        name: "GPUAdapterInfo",
        id: GPU_ADAPTER_INFO_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec {
                name: "vendor",
                get: Some(adapter_info_vendor::<E>),
                set: None,
            },
            PropertySpec {
                name: "architecture",
                get: Some(adapter_info_architecture::<E>),
                set: None,
            },
            PropertySpec {
                name: "device",
                get: Some(adapter_info_device::<E>),
                set: None,
            },
            PropertySpec {
                name: "description",
                get: Some(adapter_info_description::<E>),
                set: None,
            },
            PropertySpec {
                name: "subgroupMinSize",
                get: Some(adapter_info_subgroup_min_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "subgroupMaxSize",
                get: Some(adapter_info_subgroup_max_size::<E>),
                set: None,
            },
            PropertySpec {
                name: "isFallbackAdapter",
                get: Some(adapter_info_is_fallback::<E>),
                set: None,
            },
        ])),
        methods: &[],
        finalizer: finalize_value_payload,
    })
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
    let source = convert_buffer_source::<E>(cx, data_value)?;
    let data_offset = optional_gpu_size64_to_u64::<E>(cx, args.get(3).copied(), "dataOffset", 0)?;
    let byte_offset = data_offset
        .checked_mul(source.bytes_per_element)
        .ok_or_else(|| E::operation_error(cx, "dataOffset is outside the source range"))?;
    if byte_offset > source.byte_length {
        return Err(E::operation_error(
            cx,
            "dataOffset is outside the source range",
        ));
    }
    let byte_size = match args.get(4).copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            let size = optional_gpu_size64_to_u64::<E>(cx, Some(value), "size", 0)?;
            size.checked_mul(source.bytes_per_element)
                .ok_or_else(|| E::operation_error(cx, "size is outside the source range"))?
        }
        _ => source.byte_length - byte_offset,
    };
    let relative_end = byte_offset
        .checked_add(byte_size)
        .ok_or_else(|| E::operation_error(cx, "size is outside the source range"))?;
    if relative_end > source.byte_length {
        return Err(E::operation_error(cx, "size is outside the source range"));
    }
    if byte_size % 4 != 0 {
        return Err(E::operation_error(
            cx,
            "writeBuffer size must be a multiple of 4 bytes",
        ));
    }
    let start = source
        .byte_offset
        .checked_add(byte_offset)
        .ok_or_else(|| E::type_error(cx, "dataOffset"))?;
    let end = start
        .checked_add(byte_size)
        .ok_or_else(|| E::type_error(cx, "size"))?;
    let start = usize::try_from(start).map_err(|_| E::type_error(cx, "dataOffset"))?;
    let end = usize::try_from(end).map_err(|_| E::type_error(cx, "size"))?;
    let size = usize::try_from(byte_size).map_err(|_| E::type_error(cx, "size"))?;
    let buffer = buffer_handle::<E>(cx, buffer_value)?;
    unsafe {
        (E::environment(cx).gpu().queue_write_buffer)(
            queue_payload.queue,
            buffer,
            offset,
            source.bytes[start..end].as_ptr().cast(),
            size,
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUQueue.writeTexture`.
pub fn queue_write_texture<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(queue_payload) = E::payload(cx, this, GPU_QUEUE_CLASS)
        .and_then(|payload| payload.downcast_ref::<QueuePayload>())
    else {
        return Err(E::type_error(
            cx,
            "GPUQueue.writeTexture called on an incompatible object",
        ));
    };
    let destination_value = args
        .first()
        .copied()
        .ok_or_else(|| E::type_error(cx, "destination"))?;
    let data_value = args
        .get(1)
        .copied()
        .ok_or_else(|| E::type_error(cx, "data"))?;
    let layout_value = args
        .get(2)
        .copied()
        .ok_or_else(|| E::type_error(cx, "dataLayout"))?;
    let size_value = args
        .get(3)
        .copied()
        .ok_or_else(|| E::type_error(cx, "size"))?;
    let destination = convert_texel_copy_texture_info::<E>(cx, destination_value)?;
    let layout = convert_texel_copy_buffer_layout::<E>(cx, layout_value)?;
    let write_size = convert_gpu_extent3d::<E>(cx, size_value)?;
    let source = convert_buffer_source::<E>(cx, data_value)?;
    let start = usize::try_from(source.byte_offset).map_err(|_| E::type_error(cx, "data"))?;
    let end_u64 = source
        .byte_offset
        .checked_add(source.byte_length)
        .ok_or_else(|| E::type_error(cx, "data"))?;
    let end = usize::try_from(end_u64).map_err(|_| E::type_error(cx, "data"))?;
    let data_size = usize::try_from(source.byte_length).map_err(|_| E::type_error(cx, "data"))?;
    unsafe {
        (E::environment(cx).gpu().queue_write_texture)(
            queue_payload.queue,
            ptr::from_ref(&destination),
            source.bytes[start..end].as_ptr().cast(),
            data_size,
            ptr::from_ref(&layout),
            ptr::from_ref(&write_size),
        );
    }
    Ok(E::undefined(cx))
}

struct ConvertedBufferSource {
    bytes: Vec<u8>,
    byte_offset: u64,
    byte_length: u64,
    bytes_per_element: u64,
}

fn convert_buffer_source<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<ConvertedBufferSource, E::Error> {
    if E::arraybuffer_len(cx, value).is_some() {
        let bytes = E::arraybuffer_copy(cx, value)
            .ok_or_else(|| E::type_error(cx, "data must be an ArrayBuffer or ArrayBufferView"))?;
        let byte_length = u64::try_from(bytes.len()).map_err(|_| E::type_error(cx, "data"))?;
        return Ok(ConvertedBufferSource {
            bytes,
            byte_offset: 0,
            byte_length,
            bytes_per_element: 1,
        });
    }

    let backing = E::get_property(cx, value, "buffer")?;
    let Some(backing_length) = E::arraybuffer_len(cx, backing) else {
        return Err(E::type_error(
            cx,
            "data must be an ArrayBuffer or ArrayBufferView (or pass data.buffer)",
        ));
    };
    let byte_offset = enforce_u64::<E>(
        cx,
        required_member::<E>(cx, value, "byteOffset")?,
        "data.byteOffset",
    )?;
    let byte_length = enforce_u64::<E>(
        cx,
        required_member::<E>(cx, value, "byteLength")?,
        "data.byteLength",
    )?;
    let constructor = E::get_property(cx, value, "constructor")?;
    let bytes_per_element = if E::is_undefined(cx, constructor) || E::is_null(cx, constructor) {
        1
    } else {
        let value = E::get_property(cx, constructor, "BYTES_PER_ELEMENT")?;
        if E::is_undefined(cx, value) {
            1
        } else {
            let value = enforce_u64::<E>(cx, value, "data.constructor.BYTES_PER_ELEMENT")?;
            if value == 0 {
                return Err(E::type_error(cx, "data.constructor.BYTES_PER_ELEMENT"));
            }
            value
        }
    };
    let backing_length = u64::try_from(backing_length).map_err(|_| E::type_error(cx, "data"))?;
    let view_end = byte_offset
        .checked_add(byte_length)
        .ok_or_else(|| E::type_error(cx, "data.byteLength"))?;
    if view_end > backing_length {
        return Err(E::type_error(cx, "data.byteLength"));
    }
    let bytes = E::arraybuffer_copy(cx, backing)
        .ok_or_else(|| E::type_error(cx, "data must reference an attached ArrayBuffer"))?;
    let copied_length = u64::try_from(bytes.len()).map_err(|_| E::type_error(cx, "data"))?;
    if view_end > copied_length {
        return Err(E::type_error(cx, "data.byteLength"));
    }
    Ok(ConvertedBufferSource {
        bytes,
        byte_offset,
        byte_length,
        bytes_per_element,
    })
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
    let mut invalid_sink = None;
    let mut seen = HashSet::with_capacity(command_states.len());
    for state in &command_states {
        let duplicate = !seen.insert(Arc::as_ptr(state) as usize);
        let state = state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandBuffer state is poisoned"))?;
        if duplicate && invalid_sink.is_none() {
            invalid_sink = Some((
                Arc::clone(&state.error_sink),
                "GPUCommandBuffer occurs more than once in submit",
            ));
        }
        if state.consumed && invalid_sink.is_none() {
            invalid_sink = Some((
                Arc::clone(&state.error_sink),
                "GPUCommandBuffer is consumed",
            ));
        }
        if state.invalid && invalid_sink.is_none() {
            invalid_sink = Some((Arc::clone(&state.error_sink), "GPUCommandBuffer is invalid"));
        }
        command_handles.push(state.command_buffer);
    }
    for state in &command_states {
        state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandBuffer state is poisoned"))?
            .consumed = true;
    }
    if let Some((error_sink, message)) = invalid_sink {
        error_sink.generate_validation_error(message.to_owned());
        return Ok(E::undefined(cx));
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
    promise_operation::<E>(cx, |deferred| {
        let Some(queue_payload) = E::payload(cx, this, GPU_QUEUE_CLASS)
            .and_then(|payload| payload.downcast_ref::<QueuePayload>())
        else {
            return Err(E::type_error(
                cx,
                "GPUQueue.onSubmittedWorkDone called on an incompatible object",
            ));
        };
        let mut request = Box::new(QueueWorkDoneRequest::<E> {
            deferred: deferred.take(),
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
        Ok(())
    })
}

/// Implements `GPUCommandEncoder.copyBufferToBuffer`.
pub fn command_encoder_copy_buffer_to_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
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

/// Implements `GPUCommandEncoder.clearBuffer`.
pub fn command_encoder_clear_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let buffer = buffer_handle::<E>(cx, required_argument::<E>(cx, args, 0, "buffer")?)?;
    let offset = optional_gpu_size64_to_u64::<E>(cx, args.get(1).copied(), "offset", 0)?;
    let size = optional_gpu_size64_to_u64::<E>(cx, args.get(2).copied(), "size", u64::MAX)?;
    unsafe {
        (E::environment(cx).gpu().command_encoder_clear_buffer)(encoder, buffer, offset, size);
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.resolveQuerySet`.
pub fn command_encoder_resolve_query_set<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let query_set = query_set_handle::<E>(cx, required_argument::<E>(cx, args, 0, "querySet")?)?;
    let first_query = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "firstQuery")?,
        "firstQuery",
    )?;
    let query_count = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 2, "queryCount")?,
        "queryCount",
    )?;
    let destination = buffer_handle::<E>(cx, required_argument::<E>(cx, args, 3, "destination")?)?;
    let destination_offset = enforce_u64::<E>(
        cx,
        required_argument::<E>(cx, args, 4, "destinationOffset")?,
        "destinationOffset",
    )?;
    unsafe {
        (E::environment(cx).gpu().command_encoder_resolve_query_set)(
            encoder,
            query_set,
            first_query,
            query_count,
            destination,
            destination_offset,
        );
    }
    Ok(E::undefined(cx))
}

/// Implements the shared `GPUDebugCommandsMixin.pushDebugGroup` body.
pub fn debug_commands_push_debug_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_debug_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let arena = Arena::new();
    let label = E::to_str(
        cx,
        required_argument::<E>(cx, args, 0, "groupLabel")?,
        &arena,
    )?;
    unsafe {
        encoder.push_debug_group(
            E::environment(cx).gpu(),
            WGPUStringView::from_bytes(label.as_bytes()),
        )
    };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPUDebugCommandsMixin.popDebugGroup` body.
pub fn debug_commands_pop_debug_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_debug_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    unsafe { encoder.pop_debug_group(E::environment(cx).gpu()) };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPUDebugCommandsMixin.insertDebugMarker` body.
pub fn debug_commands_insert_debug_marker<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_debug_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let arena = Arena::new();
    let label = E::to_str(
        cx,
        required_argument::<E>(cx, args, 0, "markerLabel")?,
        &arena,
    )?;
    unsafe {
        encoder.insert_debug_marker(
            E::environment(cx).gpu(),
            WGPUStringView::from_bytes(label.as_bytes()),
        )
    };
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.copyBufferToTexture`.
pub fn command_encoder_copy_buffer_to_texture<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let source =
        convert_texel_copy_buffer_info::<E>(cx, required_argument::<E>(cx, args, 0, "source")?)?;
    let destination = convert_texel_copy_texture_info::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "destination")?,
    )?;
    let copy_size =
        convert_gpu_extent3d::<E>(cx, required_argument::<E>(cx, args, 2, "copySize")?)?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .command_encoder_copy_buffer_to_texture)(
            encoder,
            ptr::from_ref(&source),
            ptr::from_ref(&destination),
            ptr::from_ref(&copy_size),
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.copyTextureToBuffer`.
pub fn command_encoder_copy_texture_to_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let source =
        convert_texel_copy_texture_info::<E>(cx, required_argument::<E>(cx, args, 0, "source")?)?;
    let destination = convert_texel_copy_buffer_info::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "destination")?,
    )?;
    let copy_size =
        convert_gpu_extent3d::<E>(cx, required_argument::<E>(cx, args, 2, "copySize")?)?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .command_encoder_copy_texture_to_buffer)(
            encoder,
            ptr::from_ref(&source),
            ptr::from_ref(&destination),
            ptr::from_ref(&copy_size),
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.copyTextureToTexture`.
pub fn command_encoder_copy_texture_to_texture<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_command_encoder::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let source =
        convert_texel_copy_texture_info::<E>(cx, required_argument::<E>(cx, args, 0, "source")?)?;
    let destination = convert_texel_copy_texture_info::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "destination")?,
    )?;
    let copy_size =
        convert_gpu_extent3d::<E>(cx, required_argument::<E>(cx, args, 2, "copySize")?)?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .command_encoder_copy_texture_to_texture)(
            encoder,
            ptr::from_ref(&source),
            ptr::from_ref(&destination),
            ptr::from_ref(&copy_size),
        );
    }
    Ok(E::undefined(cx))
}

/// Implements `GPUCommandEncoder.beginRenderPass`.
pub fn command_encoder_begin_render_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let parent = command_encoder_state::<E>(cx, this)?;
    let (encoder, error_sink, finished, locked) = {
        let state = parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
        (
            state.encoder,
            Arc::clone(&state.error_sink),
            state.ended,
            state.locked,
        )
    };
    if finished {
        error_sink.generate_validation_error("GPUCommandEncoder is finished".to_owned());
        let _ = E::register_class(cx, render_pass_encoder_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_RENDER_PASS_ENCODER_CLASS,
            Box::new(RenderPassEncoderPayload {
                state: Arc::new(Mutex::new(RenderPassState {
                    pass: ptr::null_mut(),
                    ended: false,
                    parent,
                    error_sink,
                })),
                label: Mutex::new(String::new()),
            }),
        );
    }
    if locked {
        parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
            .pending_validation_error
            .get_or_insert_with(|| {
                "GPUCommandEncoder was used while locked by an active pass".to_owned()
            });
        let _ = E::register_class(cx, render_pass_encoder_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_RENDER_PASS_ENCODER_CLASS,
            Box::new(RenderPassEncoderPayload {
                state: Arc::new(Mutex::new(RenderPassState {
                    pass: ptr::null_mut(),
                    ended: false,
                    parent,
                    error_sink,
                })),
                label: Mutex::new(String::new()),
            }),
        );
    }
    let arena = Arena::new();
    let mut created_texture_views = CreatedTextureViewCapture::new::<E>(cx);
    let descriptor = convert_render_pass_descriptor::<E>(
        cx,
        required_argument::<E>(cx, args, 0, "descriptor")?,
        &arena,
        &mut created_texture_views,
    )?;
    let label = unsafe { string_view_to_owned(descriptor.label) };
    if created_texture_views.has_depth_slice_error() {
        parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
            .pending_validation_error
            .get_or_insert_with(|| {
                "GPURenderPassColorAttachment.depthSlice must be provided only for 3d views and be less than the mip depth".to_owned()
            });
    }
    parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
        .locked = true;
    let pass = unsafe {
        (E::environment(cx).gpu().command_encoder_begin_render_pass)(
            encoder,
            ptr::from_ref(&descriptor),
        )
    };
    if pass.is_null() {
        parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
            .locked = false;
        return Err(E::operation_error(
            cx,
            "wgpuCommandEncoderBeginRenderPass returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, render_pass_encoder_class::<E>()) {
        unsafe { (E::environment(cx).gpu().render_pass_encoder_release)(pass) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_RENDER_PASS_ENCODER_CLASS,
        Box::new(RenderPassEncoderPayload {
            state: Arc::new(Mutex::new(RenderPassState {
                pass,
                ended: false,
                parent,
                error_sink,
            })),
            label: Mutex::new(label),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().render_pass_encoder_release)(pass) };
            Err(error)
        }
    }
}

/// Implements `GPUCommandEncoder.beginComputePass`.
pub fn command_encoder_begin_compute_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let parent = command_encoder_state::<E>(cx, this)?;
    let (encoder, error_sink, finished, locked) = {
        let state = parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
        (
            state.encoder,
            Arc::clone(&state.error_sink),
            state.ended,
            state.locked,
        )
    };
    if finished {
        error_sink.generate_validation_error("GPUCommandEncoder is finished".to_owned());
        let _ = E::register_class(cx, compute_pass_encoder_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_COMPUTE_PASS_ENCODER_CLASS,
            Box::new(ComputePassEncoderPayload {
                state: Arc::new(Mutex::new(ComputePassState {
                    pass: ptr::null_mut(),
                    ended: false,
                    parent,
                    error_sink,
                })),
                label: Mutex::new(String::new()),
            }),
        );
    }
    if locked {
        parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
            .pending_validation_error
            .get_or_insert_with(|| {
                "GPUCommandEncoder was used while locked by an active pass".to_owned()
            });
        let _ = E::register_class(cx, compute_pass_encoder_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_COMPUTE_PASS_ENCODER_CLASS,
            Box::new(ComputePassEncoderPayload {
                state: Arc::new(Mutex::new(ComputePassState {
                    pass: ptr::null_mut(),
                    ended: false,
                    parent,
                    error_sink,
                })),
                label: Mutex::new(String::new()),
            }),
        );
    }
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_compute_pass_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let label = native.as_ref().map_or_else(String::new, |native| unsafe {
        string_view_to_owned(native.label)
    });
    parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
        .locked = true;
    let pass = unsafe {
        (E::environment(cx).gpu().command_encoder_begin_compute_pass)(
            encoder,
            native.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    if pass.is_null() {
        parent
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?
            .locked = false;
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
                error_sink,
            })),
            label: Mutex::new(label),
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
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_command_buffer_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let label = native.as_ref().map_or_else(String::new, |native| unsafe {
        string_view_to_owned(native.label)
    });
    let (encoder, error_sink, finished, locked, pending_validation_error) = {
        let mut state = state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
        let finished = state.ended;
        let locked = state.locked;
        state.ended = true;
        state.locked = false;
        (
            state.encoder,
            Arc::clone(&state.error_sink),
            finished,
            locked,
            state.pending_validation_error.take(),
        )
    };
    let invalid = locked || pending_validation_error.is_some();
    if let Some(message) = pending_validation_error {
        error_sink.generate_validation_error(message);
    } else if locked {
        error_sink
            .generate_validation_error("GPUCommandEncoder is locked by an active pass".to_owned());
    }
    if finished {
        error_sink.generate_validation_error("GPUCommandEncoder is finished".to_owned());
    }
    if finished || invalid {
        let _ = E::register_class(cx, command_buffer_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_COMMAND_BUFFER_CLASS,
            Box::new(CommandBufferPayload {
                state: Arc::new(Mutex::new(CommandBufferState {
                    command_buffer: ptr::null_mut(),
                    consumed: false,
                    invalid: true,
                    error_sink,
                })),
                label: Mutex::new(label),
            }),
        );
    }
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
                invalid: false,
                error_sink,
            })),
            label: Mutex::new(label),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().command_buffer_release)(command_buffer) };
            Err(error)
        }
    }
}

/// Implements `GPURenderBundleEncoder.finish` with the B10 encoder discipline.
pub fn render_bundle_encoder_finish<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_RENDER_BUNDLE_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundleEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "GPURenderBundleEncoder is required"))?;
    let arena = Arena::new();
    let native = match args.first().copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            Some(convert_render_bundle_descriptor::<E>(cx, value, &arena)?)
        }
        _ => None,
    };
    let label = native.as_ref().map_or_else(String::new, |native| unsafe {
        string_view_to_owned(native.label)
    });
    let (encoder, error_sink, finished) = {
        let mut state = payload
            .state
            .lock()
            .map_err(|_| E::operation_error(cx, "GPURenderBundleEncoder state is poisoned"))?;
        let finished = state.ended;
        state.ended = true;
        (
            state.render_bundle_encoder,
            Arc::clone(&state.error_sink),
            finished,
        )
    };
    if finished {
        error_sink.generate_validation_error("GPURenderBundleEncoder is finished".to_owned());
        let _ = E::register_class(cx, render_bundle_class::<E>())?;
        return E::new_instance(
            cx,
            GPU_RENDER_BUNDLE_CLASS,
            Box::new(RenderBundlePayload {
                render_bundle: ptr::null_mut(),
                invalid: true,
                label: Mutex::new(label),
            }),
        );
    }
    let render_bundle = unsafe {
        (E::environment(cx).gpu().render_bundle_encoder_finish)(
            encoder,
            native.as_ref().map_or(ptr::null(), ptr::from_ref),
        )
    };
    if render_bundle.is_null() {
        return Err(E::operation_error(
            cx,
            "wgpuRenderBundleEncoderFinish returned null",
        ));
    }
    if let Err(error) = E::register_class(cx, render_bundle_class::<E>()) {
        unsafe { (E::environment(cx).gpu().render_bundle_release)(render_bundle) };
        return Err(error);
    }
    match E::new_instance(
        cx,
        GPU_RENDER_BUNDLE_CLASS,
        Box::new(RenderBundlePayload {
            render_bundle,
            invalid: false,
            label: Mutex::new(label),
        }),
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe { (E::environment(cx).gpu().render_bundle_release)(render_bundle) };
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
    let Some(pass) = live_compute_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
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
    let Some(pass) = live_compute_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
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
    let dynamic_offsets = convert_dynamic_offsets::<E>(cx, args)?;
    unsafe {
        (E::environment(cx).gpu().compute_pass_encoder_set_bind_group)(
            pass,
            index,
            bind_group,
            dynamic_offsets.len(),
            if dynamic_offsets.is_empty() {
                ptr::null()
            } else {
                dynamic_offsets.as_ptr()
            },
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
    let Some(pass) = live_compute_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
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

/// Implements `GPUComputePassEncoder.dispatchWorkgroupsIndirect`.
pub fn compute_pass_dispatch_workgroups_indirect<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_compute_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let indirect_buffer =
        buffer_handle::<E>(cx, required_argument::<E>(cx, args, 0, "indirectBuffer")?)?;
    let indirect_offset = enforce_u64::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "indirectOffset")?,
        "indirectOffset",
    )?;
    // As with setVertexBuffer/setIndexBuffer, native command recording owns
    // resource liveness after this call; the wrapper does not retain a JS value.
    unsafe {
        (E::environment(cx)
            .gpu()
            .compute_pass_encoder_dispatch_workgroups_indirect)(
            pass,
            indirect_buffer,
            indirect_offset,
        )
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
        state
            .error_sink
            .generate_validation_error("GPUComputePassEncoder is ended".to_owned());
        return Ok(E::undefined(cx));
    }
    let mut parent = state
        .parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if parent.ended {
        state
            .error_sink
            .generate_validation_error("GPUCommandEncoder is finished".to_owned());
        return Ok(E::undefined(cx));
    }
    if state.pass.is_null() {
        state
            .error_sink
            .generate_validation_error("GPUComputePassEncoder is invalid".to_owned());
        drop(parent);
        state.ended = true;
        return Ok(E::undefined(cx));
    }
    if !parent.locked {
        state.error_sink.generate_validation_error(
            "GPUCommandEncoder is not locked by this compute pass".to_owned(),
        );
        return Ok(E::undefined(cx));
    }
    parent.locked = false;
    drop(parent);
    unsafe { (E::environment(cx).gpu().compute_pass_encoder_end)(state.pass) };
    state.ended = true;
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.setPipeline` body.
pub fn render_pass_set_pipeline<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let pipeline =
        render_pipeline_handle::<E>(cx, required_argument::<E>(cx, args, 0, "pipeline")?)?;
    unsafe { encoder.set_pipeline(E::environment(cx).gpu(), pipeline) };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.setVertexBuffer` body.
pub fn render_pass_set_vertex_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let slot = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 0, "slot")?, "slot")?;
    let buffer_value = required_argument::<E>(cx, args, 1, "buffer")?;
    let buffer = if E::is_null(cx, buffer_value) {
        ptr::null_mut()
    } else {
        buffer_handle::<E>(cx, buffer_value)?
    };
    let offset = optional_gpu_size64_to_u64::<E>(cx, args.get(2).copied(), "offset", 0)?;
    let size = optional_gpu_size64_to_u64::<E>(cx, args.get(3).copied(), "size", u64::MAX)?;
    unsafe { encoder.set_vertex_buffer(E::environment(cx).gpu(), slot, buffer, offset, size) };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.setIndexBuffer` body.
pub fn render_pass_set_index_buffer<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let buffer = buffer_handle::<E>(cx, required_argument::<E>(cx, args, 0, "buffer")?)?;
    let format =
        convert_gpu_index_format::<E>(cx, required_argument::<E>(cx, args, 1, "indexFormat")?)?;
    let offset = optional_gpu_size64_to_u64::<E>(cx, args.get(2).copied(), "offset", 0)?;
    let size = optional_gpu_size64_to_u64::<E>(cx, args.get(3).copied(), "size", u64::MAX)?;
    unsafe { encoder.set_index_buffer(E::environment(cx).gpu(), buffer, format, offset, size) };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPUBindingCommandsMixin.setBindGroup` body.
pub fn render_pass_set_bind_group<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let index = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 0, "index")?, "index")?;
    let bind_group = bind_group_handle::<E>(cx, required_argument::<E>(cx, args, 1, "bindGroup")?)?;
    let dynamic_offsets = convert_dynamic_offsets::<E>(cx, args)?;
    unsafe {
        encoder.set_bind_group(
            E::environment(cx).gpu(),
            index,
            bind_group,
            &dynamic_offsets,
        )
    };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.draw` body.
pub fn render_pass_draw<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let vertex_count = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 0, "vertexCount")?,
        "vertexCount",
    )?;
    let instance_count = optional_u32::<E>(cx, args.get(1).copied(), "instanceCount", 1)?;
    let first_vertex = optional_u32::<E>(cx, args.get(2).copied(), "firstVertex", 0)?;
    let first_instance = optional_u32::<E>(cx, args.get(3).copied(), "firstInstance", 0)?;
    unsafe {
        encoder.draw(
            E::environment(cx).gpu(),
            vertex_count,
            instance_count,
            first_vertex,
            first_instance,
        )
    };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.drawIndexed` body.
pub fn render_pass_draw_indexed<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let index_count = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 0, "indexCount")?,
        "indexCount",
    )?;
    let instance_count = optional_u32::<E>(cx, args.get(1).copied(), "instanceCount", 1)?;
    let first_index = optional_u32::<E>(cx, args.get(2).copied(), "firstIndex", 0)?;
    let base_vertex = match args.get(3).copied() {
        Some(value) if !E::is_undefined(cx, value) => enforce_i32::<E>(cx, value, "baseVertex")?,
        _ => 0,
    };
    let first_instance = optional_u32::<E>(cx, args.get(4).copied(), "firstInstance", 0)?;
    unsafe {
        encoder.draw_indexed(
            E::environment(cx).gpu(),
            (
                index_count,
                instance_count,
                first_index,
                base_vertex,
                first_instance,
            ),
        )
    };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.drawIndirect` body.
pub fn render_pass_draw_indirect<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let indirect_buffer =
        buffer_handle::<E>(cx, required_argument::<E>(cx, args, 0, "indirectBuffer")?)?;
    let indirect_offset = enforce_u64::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "indirectOffset")?,
        "indirectOffset",
    )?;
    // Shared render-command ownership matches setVertexBuffer/setIndexBuffer:
    // the native encoder retains command resources, not the JS wrapper.
    unsafe { encoder.draw_indirect(E::environment(cx).gpu(), indirect_buffer, indirect_offset) };
    Ok(E::undefined(cx))
}

/// Implements the shared `GPURenderCommandsMixin.drawIndexedIndirect` body.
pub fn render_pass_draw_indexed_indirect<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(encoder) = live_render_commands::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let indirect_buffer =
        buffer_handle::<E>(cx, required_argument::<E>(cx, args, 0, "indirectBuffer")?)?;
    let indirect_offset = enforce_u64::<E>(
        cx,
        required_argument::<E>(cx, args, 1, "indirectOffset")?,
        "indirectOffset",
    )?;
    unsafe {
        encoder.draw_indexed_indirect(E::environment(cx).gpu(), indirect_buffer, indirect_offset)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.setViewport`.
pub fn render_pass_set_viewport<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let x = restricted_f32::<E>(cx, required_argument::<E>(cx, args, 0, "x")?, "x")?;
    let y = restricted_f32::<E>(cx, required_argument::<E>(cx, args, 1, "y")?, "y")?;
    let width = restricted_f32::<E>(cx, required_argument::<E>(cx, args, 2, "width")?, "width")?;
    let height = restricted_f32::<E>(cx, required_argument::<E>(cx, args, 3, "height")?, "height")?;
    let min_depth = restricted_f32::<E>(
        cx,
        required_argument::<E>(cx, args, 4, "minDepth")?,
        "minDepth",
    )?;
    let max_depth = restricted_f32::<E>(
        cx,
        required_argument::<E>(cx, args, 5, "maxDepth")?,
        "maxDepth",
    )?;
    unsafe {
        (E::environment(cx).gpu().render_pass_encoder_set_viewport)(
            pass, x, y, width, height, min_depth, max_depth,
        )
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.setScissorRect`.
pub fn render_pass_set_scissor_rect<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let x = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 0, "x")?, "x")?;
    let y = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 1, "y")?, "y")?;
    let width = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 2, "width")?, "width")?;
    let height = enforce_u32::<E>(cx, required_argument::<E>(cx, args, 3, "height")?, "height")?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .render_pass_encoder_set_scissor_rect)(pass, x, y, width, height)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.setBlendConstant`.
pub fn render_pass_set_blend_constant<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let color = convert_gpu_color::<E>(cx, required_argument::<E>(cx, args, 0, "color")?)?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .render_pass_encoder_set_blend_constant)(pass, ptr::from_ref(&color))
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.setStencilReference`.
pub fn render_pass_set_stencil_reference<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let reference = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 0, "reference")?,
        "reference",
    )?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .render_pass_encoder_set_stencil_reference)(pass, reference)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.beginOcclusionQuery`.
pub fn render_pass_begin_occlusion_query<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    let query_index = enforce_u32::<E>(
        cx,
        required_argument::<E>(cx, args, 0, "queryIndex")?,
        "queryIndex",
    )?;
    unsafe {
        (E::environment(cx)
            .gpu()
            .render_pass_encoder_begin_occlusion_query)(pass, query_index)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.endOcclusionQuery`.
pub fn render_pass_end_occlusion_query<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    unsafe {
        (E::environment(cx)
            .gpu()
            .render_pass_encoder_end_occlusion_query)(pass)
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.executeBundles` without wrapper-side retention.
pub fn render_pass_execute_bundles<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let value = required_argument::<E>(cx, args, 0, "bundles")?;
    let bundles = convert_render_bundle_sequence::<E>(cx, value)?;
    let Some(pass) = live_render_pass::<E>(cx, this)? else {
        return Ok(E::undefined(cx));
    };
    unsafe {
        (E::environment(cx).gpu().render_pass_encoder_execute_bundles)(
            pass,
            bundles.len(),
            if bundles.is_empty() {
                ptr::null()
            } else {
                bundles.as_ptr()
            },
        )
    };
    Ok(E::undefined(cx))
}

/// Implements `GPURenderPassEncoder.end`.
pub fn render_pass_end<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    _args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .ok_or_else(|| {
            E::type_error(
                cx,
                "GPURenderPassEncoder method called on an incompatible object",
            )
        })?;
    let mut state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPURenderPassEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPURenderPassEncoder is ended".to_owned());
        return Ok(E::undefined(cx));
    }
    let mut parent = state
        .parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if parent.ended {
        state
            .error_sink
            .generate_validation_error("GPUCommandEncoder is finished".to_owned());
        return Ok(E::undefined(cx));
    }
    if state.pass.is_null() {
        state
            .error_sink
            .generate_validation_error("GPURenderPassEncoder is invalid".to_owned());
        drop(parent);
        state.ended = true;
        return Ok(E::undefined(cx));
    }
    if !parent.locked {
        state.error_sink.generate_validation_error(
            "GPUCommandEncoder is not locked by this render pass".to_owned(),
        );
        return Ok(E::undefined(cx));
    }
    parent.locked = false;
    drop(parent);
    unsafe { (E::environment(cx).gpu().render_pass_encoder_end)(state.pass) };
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

/// Finalizes a `GPUCommandBuffer` payload by enqueuing its release.
pub fn finalize_command_buffer(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<CommandBufferPayload>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    if state.invalid {
        return;
    }
    let _ = env.queue().enqueue(ReleaseRequest::CommandBuffer {
        command_buffer: state.command_buffer,
        gpu: env.gpu(),
    });
}

/// Finalizes a reusable `GPURenderBundle` payload by enqueuing its release.
pub fn finalize_render_bundle(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<RenderBundlePayload>() else {
        return;
    };
    if payload.invalid {
        return;
    }
    let _ = env.queue().enqueue(ReleaseRequest::RenderBundle {
        render_bundle: payload.render_bundle,
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
    if state.pass.is_null() {
        return;
    }
    let _ = env.queue().enqueue(ReleaseRequest::ComputePassEncoder {
        pass: state.pass,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPURenderPassEncoder` payload by enqueuing its release.
pub fn finalize_render_pass_encoder(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<RenderPassEncoderPayload>() else {
        return;
    };
    let Ok(state) = payload.state.lock() else {
        return;
    };
    if state.pass.is_null() {
        return;
    }
    let _ = env.queue().enqueue(ReleaseRequest::RenderPassEncoder {
        pass: state.pass,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUDevice` payload by enqueuing its release.
pub fn finalize_device<E: JsEngine + 'static>(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<DevicePayload<E>>() else {
        return;
    };
    payload.events.mark_wrapper_finalized();
    let _ = env.queue().enqueue(ReleaseRequest::Device {
        device: payload.device,
        gpu: env.gpu(),
    });
}

/// Finalizes a `GPUAdapter` payload by enqueuing its release.
pub fn finalize_adapter<E: JsEngine + 'static>(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<AdapterPayload<E>>() else {
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
    events: Arc<DeviceEventState<E>>,
    label: String,
    _registration: Option<E::DeferredRegistration>,
}

struct MapRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    _registration: Option<E::DeferredRegistration>,
    mode: WGPUMapMode,
    map_id: u64,
    state: Arc<Mutex<BufferState<E>>>,
}

fn start_buffer_map<E: JsEngine + 'static>(
    buffer: WGPUBuffer,
    mode: WGPUMapMode,
    offset: usize,
    size: usize,
    request: Box<MapRequest<E>>,
    gpu: GpuDispatch,
) {
    let info = WGPUBufferMapCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(buffer_map_callback::<E>),
        userdata1: Box::into_raw(request).cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe { (gpu.buffer_map_async)(buffer, mode, offset, size, info) };
}

struct QueueWorkDoneRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    _registration: Option<E::DeferredRegistration>,
}

struct PopErrorScopeRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    synthetic_error: Option<SyntheticDeviceError>,
    state: Arc<DeviceEventState<E>>,
    _registration: Option<E::DeferredRegistration>,
}

struct ComputePipelineRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    release_queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
    state: Arc<DeviceEventState<E>>,
    lost_at_start: bool,
    module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
    label: String,
    _registration: Option<E::DeferredRegistration>,
}

struct RenderPipelineRequest<E: JsEngine + 'static> {
    deferred: Option<Deferred<E>>,
    settlements: Arc<SettlementQueue>,
    release_queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
    state: Arc<DeviceEventState<E>>,
    lost_at_start: bool,
    vertex_module: WGPUShaderModule,
    fragment_module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
    label: String,
    _registration: Option<E::DeferredRegistration>,
}

unsafe fn string_view_to_owned(view: WGPUStringView) -> String {
    if view.data.is_null() || view.length == wgpu_strlen() {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length) };
    // SAFETY: descriptor string views are created from `E::to_str`, whose
    // contract returns valid UTF-8, and remain arena-backed for this call.
    unsafe { std::str::from_utf8_unchecked(bytes) }.to_owned()
}

unsafe fn callback_message(message: WGPUStringView, fallback: &'static str) -> String {
    let backend = unsafe { callback_string(message) };
    if backend.is_empty() {
        fallback.to_owned()
    } else {
        format!("{fallback}: {backend}")
    }
}

unsafe fn callback_string(message: WGPUStringView) -> String {
    if message.data.is_null() {
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
    }
}

fn enqueue_compute_pipeline_release(
    queue: &ReleaseQueue,
    pipeline: WGPUComputePipeline,
    module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
    gpu: GpuDispatch,
) {
    if pipeline.is_null() {
        let _ = queue.enqueue(ReleaseRequest::ShaderModule { module, gpu });
        if !layout.is_null() {
            let _ = queue.enqueue(ReleaseRequest::PipelineLayout { layout, gpu });
        }
    } else {
        let _ = queue.enqueue(ReleaseRequest::ComputePipeline {
            pipeline,
            module,
            layout,
            gpu,
        });
    }
}

fn enqueue_render_pipeline_release(
    queue: &ReleaseQueue,
    pipeline: WGPURenderPipeline,
    vertex_module: WGPUShaderModule,
    fragment_module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
    gpu: GpuDispatch,
) {
    if pipeline.is_null() {
        let _ = queue.enqueue(ReleaseRequest::ShaderModule {
            module: vertex_module,
            gpu,
        });
        if !fragment_module.is_null() {
            let _ = queue.enqueue(ReleaseRequest::ShaderModule {
                module: fragment_module,
                gpu,
            });
        }
        if !layout.is_null() {
            let _ = queue.enqueue(ReleaseRequest::PipelineLayout { layout, gpu });
        }
    } else {
        let _ = queue.enqueue(ReleaseRequest::RenderPipeline {
            render_pipeline: pipeline,
            vertex_module,
            fragment_module,
            layout,
            gpu,
        });
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
                // A8's documented post-teardown exception: this is the single place a
                // WebGPU callback may call webgpu.h. The header exempts the ProcessEvents
                // callstack from its re-entrancy prohibition, and AllowProcessEvents
                // confines that callstack to the owning thread.
                unsafe { (request.gpu.adapter_release)(adapter) };
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
                name: "OperationError",
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
                // A8's documented post-teardown exception: this is the single place a
                // WebGPU callback may call webgpu.h. The header exempts the ProcessEvents
                // callstack from its re-entrancy prohibition, and AllowProcessEvents
                // confines that callstack to the owning thread.
                unsafe { (request.gpu.device_release)(device) };
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
                events: Arc::clone(&request.events),
                label: request.label,
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
                name: "OperationError",
                message: unsafe { callback_message(message, "requestDevice failed") },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn create_compute_pipeline_callback<E: JsEngine + 'static>(
    status: WGPUCreatePipelineAsyncStatus,
    pipeline: WGPUComputePipeline,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = NonNull::new(userdata1.cast::<ComputePipelineRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            enqueue_compute_pipeline_release(
                &request.release_queue,
                pipeline,
                request.module,
                request.layout,
                request.gpu,
            );
            return;
        };
        let settlement = SettlementRequest::ComputePipeline {
            deferred,
            pipeline,
            status,
            message: unsafe { callback_string(message) },
            state: request.state,
            lost_at_start: request.lost_at_start,
            module: request.module,
            layout: request.layout,
            label: request.label,
            queue: Arc::clone(&request.release_queue),
            gpu: request.gpu,
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn create_render_pipeline_callback<E: JsEngine + 'static>(
    status: WGPUCreatePipelineAsyncStatus,
    pipeline: WGPURenderPipeline,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = NonNull::new(userdata1.cast::<RenderPipelineRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            enqueue_render_pipeline_release(
                &request.release_queue,
                pipeline,
                request.vertex_module,
                request.fragment_module,
                request.layout,
                request.gpu,
            );
            return;
        };
        let settlement = SettlementRequest::RenderPipeline {
            deferred,
            pipeline,
            status,
            message: unsafe { callback_string(message) },
            state: request.state,
            lost_at_start: request.lost_at_start,
            vertex_module: request.vertex_module,
            fragment_module: request.fragment_module,
            layout: request.layout,
            label: request.label,
            queue: Arc::clone(&request.release_queue),
            gpu: request.gpu,
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
        let canceled = request.state.lock().map_or(true, |mut state| {
            if state.pending_map != Some(request.map_id) {
                if state.canceling_map == Some(request.map_id) {
                    state.canceling_map = None;
                }
                return true;
            }
            state.pending_map = None;
            if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success {
                state.mapped = true;
                state.map_mode = request.mode;
            }
            false
        });
        let settlement = if canceled {
            SettlementRequest::Error {
                deferred,
                name: "AbortError",
                message: "mapAsync was canceled".to_owned(),
            }
        } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success {
            SettlementRequest::Success { deferred }
        } else {
            let (name, fallback) = if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Error {
                ("OperationError", "mapAsync error")
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Aborted {
                ("AbortError", "mapAsync aborted")
            } else if status == WGPUMapAsyncStatus_WGPUMapAsyncStatus_CallbackCancelled {
                ("AbortError", "mapAsync callback cancelled")
            } else {
                ("OperationError", "mapAsync failed")
            };
            SettlementRequest::Error {
                deferred,
                name,
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
                name: "OperationError",
                message: unsafe { callback_message(message, "onSubmittedWorkDone failed") },
            }
        };
        let _ = request.settlements.enqueue::<E>(settlement);
    }));
}

unsafe extern "C" fn uncaptured_error_callback<E: JsEngine + 'static>(
    _device: *const WGPUDevice,
    type_: WGPUErrorType,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = NonNull::new(userdata1.cast::<DeviceEventState<E>>()) else {
            return;
        };
        // SAFETY: userdata1 comes from Arc::into_raw in adapter_request_device.
        // Its strong reference remains owned by the shared callback userdata
        // until device_lost_callback reclaims it. The pinned webgpu.h docs
        // guarantee uncaptured-error callbacks do not fire after device loss.
        let state = unsafe { &*raw.as_ptr() };
        // The view is callback-borrowed, so copy it before returning. From this
        // point onward the record is fully Rust-owned and Send.
        let message = unsafe { callback_string(message) };
        let _ = state.enqueue_uncaptured(type_, message);
    }));
}

unsafe extern "C" fn device_lost_callback<E: JsEngine + 'static>(
    _device: *const WGPUDevice,
    reason: WGPUDeviceLostReason,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = NonNull::new(userdata1.cast::<DeviceEventState<E>>()) else {
            return;
        };
        // SAFETY: userdata1 remains backed by the one strong Arc reference
        // created in adapter_request_device. DeviceLost is a future event and
        // this is its terminal callback (including CallbackCancelled).
        let state = unsafe { &*raw.as_ptr() };
        state.mark_lost();
        let message = unsafe { callback_string(message) };
        let _ = state.enqueue_lost(reason, message);
        // SAFETY: the terminal callback has finished using the shared userdata,
        // and webgpu.h guarantees no later uncaptured-error callback can fire.
        // This reconstructs and immediately drops exactly the Arc::into_raw
        // strong reference created in adapter_request_device.
        unsafe { drop(Arc::from_raw(raw.as_ptr())) };
    }));
}

unsafe extern "C" fn pop_error_scope_callback<E: JsEngine + 'static>(
    status: WGPUPopErrorScopeStatus,
    type_: WGPUErrorType,
    message: WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(raw) = ptr::NonNull::new(userdata1.cast::<PopErrorScopeRequest<E>>()) else {
            return;
        };
        let mut request = unsafe { Box::from_raw(raw.as_ptr()) };
        let Some(deferred) = request.deferred.take() else {
            return;
        };
        let settlement = SettlementRequest::PopErrorScope {
            deferred,
            status,
            type_,
            message: unsafe { callback_string(message) },
            synthetic_error: request.synthetic_error.take(),
            state: request.state,
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
    samplers: Vec<WGPUSampler>,
    texture_views: Vec<WGPUTextureView>,
    created_texture_views: Vec<WGPUTextureView>,
}

struct CreatedTextureViewCapture {
    views: Vec<WGPUTextureView>,
    depth_slice_error: bool,
    queue: Arc<ReleaseQueue>,
    gpu: GpuDispatch,
}

impl CreatedTextureViewCapture {
    fn new<E: JsEngine>(cx: E::Context<'_>) -> Self {
        Self {
            views: Vec::new(),
            depth_slice_error: false,
            queue: Arc::clone(E::environment(cx).queue()),
            gpu: E::environment(cx).gpu(),
        }
    }

    fn push(&mut self, view: WGPUTextureView) {
        self.views.push(view);
    }

    fn take(&mut self) -> Vec<WGPUTextureView> {
        std::mem::take(&mut self.views)
    }

    fn check_depth_slice<E: JsEngine + 'static>(
        &mut self,
        cx: E::Context<'_>,
        view: E::Value,
        depth_slice: Option<u32>,
    ) -> Result<(), E::Error> {
        let (dimension, mip_depth) = if let Some(payload) =
            E::payload(cx, view, GPU_TEXTURE_VIEW_CLASS)
                .and_then(|payload| payload.downcast_ref::<TextureViewPayload>())
        {
            (payload.dimension, payload.mip_depth)
        } else if let Some(payload) = E::payload(cx, view, GPU_TEXTURE_CLASS)
            .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        {
            (
                default_texture_view_dimension(payload.dimension, payload.depth_or_array_layers),
                payload.depth_or_array_layers,
            )
        } else {
            return Err(E::type_error(
                cx,
                "GPUTexture or GPUTextureView is required",
            ));
        };
        let is_3d = dimension == WGPUTextureViewDimension_WGPUTextureViewDimension_3D;
        let invalid = if is_3d {
            depth_slice.is_none_or(|slice| slice >= mip_depth)
        } else {
            depth_slice.is_some()
        };
        if invalid {
            self.depth_slice_error = true;
        }
        Ok(())
    }

    fn has_depth_slice_error(&self) -> bool {
        self.depth_slice_error
    }
}

impl Drop for CreatedTextureViewCapture {
    fn drop(&mut self) {
        for texture_view in self.views.drain(..) {
            let _ = self.queue.enqueue(ReleaseRequest::TextureViewOnly {
                texture_view,
                gpu: self.gpu,
            });
        }
    }
}

struct ConvertedComputePipelineDescriptor {
    native: WGPUComputePipelineDescriptor,
    module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
}

struct ConvertedRenderPipelineDescriptor {
    native: WGPURenderPipelineDescriptor,
    vertex_module: WGPUShaderModule,
    fragment_module: WGPUShaderModule,
    layout: WGPUPipelineLayout,
}

fn convert_sequence<E: JsEngine, T>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
    convert: impl FnMut(E::Value) -> Result<T, E::Error>,
) -> Result<Vec<T>, E::Error> {
    let Some(iterator_method) = sequence_iterator_method::<E>(cx, value)? else {
        return Err(E::type_error(cx, &format!("{name} is not iterable")));
    };
    convert_sequence_from_method::<E, _>(cx, value, iterator_method, name, convert)
}

fn convert_dynamic_offsets<E: JsEngine>(
    cx: E::Context<'_>,
    args: &[E::Value],
) -> Result<Vec<u32>, E::Error> {
    // Four or more supplied arguments select the Uint32Array-window overload.
    if args.len() >= 4 {
        let data = required_argument::<E>(cx, args, 2, "dynamicOffsetsData")?;
        if !E::is_uint32array(cx, data) {
            return Err(E::type_error(cx, "dynamicOffsetsData"));
        }
        let source = convert_buffer_source::<E>(cx, data)?;
        if source.bytes_per_element != 4 || source.byte_length % 4 != 0 {
            return Err(E::type_error(cx, "dynamicOffsetsData"));
        }
        let start = enforce_u64::<E>(
            cx,
            required_argument::<E>(cx, args, 3, "dynamicOffsetsDataStart")?,
            "dynamicOffsetsDataStart",
        )?;
        let length = enforce_u32::<E>(
            cx,
            required_argument::<E>(cx, args, 4, "dynamicOffsetsDataLength")?,
            "dynamicOffsetsDataLength",
        )?;
        let end = start
            .checked_add(u64::from(length))
            .filter(|end| *end <= source.byte_length / 4)
            .ok_or_else(|| {
                E::range_error(
                    cx,
                    "dynamicOffsetsDataStart + dynamicOffsetsDataLength exceeds dynamicOffsetsData length",
                )
            })?;
        let byte_start = source
            .byte_offset
            .checked_add(start.checked_mul(4).ok_or_else(|| {
                E::range_error(
                    cx,
                    "dynamicOffsetsDataStart exceeds dynamicOffsetsData length",
                )
            })?)
            .ok_or_else(|| {
                E::range_error(
                    cx,
                    "dynamicOffsetsDataStart exceeds dynamicOffsetsData length",
                )
            })?;
        let byte_end = source
            .byte_offset
            .checked_add(end.checked_mul(4).ok_or_else(|| {
                E::range_error(
                    cx,
                    "dynamicOffsetsDataLength exceeds dynamicOffsetsData length",
                )
            })?)
            .ok_or_else(|| {
                E::range_error(
                    cx,
                    "dynamicOffsetsDataLength exceeds dynamicOffsetsData length",
                )
            })?;
        let byte_start = usize::try_from(byte_start)
            .map_err(|_| E::range_error(cx, "dynamicOffsetsDataStart"))?;
        let byte_end = usize::try_from(byte_end)
            .map_err(|_| E::range_error(cx, "dynamicOffsetsDataLength"))?;
        let bytes = source.bytes.get(byte_start..byte_end).ok_or_else(|| {
            E::range_error(
                cx,
                "dynamicOffsetsDataStart + dynamicOffsetsDataLength exceeds dynamicOffsetsData length",
            )
        })?;
        return bytes
            .chunks_exact(4)
            .map(|bytes| {
                <[u8; 4]>::try_from(bytes)
                    .map(u32::from_ne_bytes)
                    .map_err(|_| E::type_error(cx, "dynamicOffsetsData"))
            })
            .collect();
    }
    match args.get(2).copied() {
        Some(value) if !E::is_undefined(cx, value) => {
            convert_sequence::<E, _>(cx, value, "dynamicOffsets", |item| {
                enforce_u32::<E>(cx, item, "dynamicOffsets")
            })
        }
        _ => Ok(Vec::new()),
    }
}

fn sequence_iterator_method<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Option<E::Value>, E::Error> {
    let global = E::global(cx);
    let symbol = E::get_property(cx, global, "Symbol")?;
    let iterator_key = E::get_property(cx, symbol, "iterator")?;
    let iterator_method = E::get_property_value(cx, value, iterator_key)?;
    if E::is_undefined(cx, iterator_method) || E::is_null(cx, iterator_method) {
        Ok(None)
    } else {
        Ok(Some(iterator_method))
    }
}

fn convert_sequence_from_method<E: JsEngine, T>(
    cx: E::Context<'_>,
    value: E::Value,
    iterator_method: E::Value,
    _name: &'static str,
    mut convert: impl FnMut(E::Value) -> Result<T, E::Error>,
) -> Result<Vec<T>, E::Error> {
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

fn convert_command_buffer_sequence<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Vec<Arc<Mutex<CommandBufferState>>>, E::Error> {
    convert_sequence::<E, _>(cx, value, "commands", |item| {
        command_buffer_state::<E>(cx, item)
    })
}

fn convert_render_bundle_sequence<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Vec<WGPURenderBundle>, E::Error> {
    convert_sequence::<E, _>(cx, value, "bundles", |item| {
        E::payload(cx, item, GPU_RENDER_BUNDLE_CLASS)
            .and_then(|payload| payload.downcast_ref::<RenderBundlePayload>())
            .map(|payload| payload.render_bundle)
            .ok_or_else(|| E::type_error(cx, "GPURenderBundle is required"))
    })
}

fn required_member<E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
) -> Result<E::Value, E::Error> {
    let value = dictionary_member::<E>(cx, obj, name)?;
    if E::is_undefined(cx, value) {
        Err(E::type_error(cx, name))
    } else {
        Ok(value)
    }
}

fn dictionary_member<E: JsEngine>(
    cx: E::Context<'_>,
    obj: E::Value,
    name: &'static str,
) -> Result<E::Value, E::Error> {
    if E::is_undefined(cx, obj) || E::is_null(cx, obj) {
        Ok(E::undefined(cx))
    } else {
        E::get_property(cx, obj, name)
    }
}

fn optional_gpu_size_to_u64<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    name: &'static str,
    default: u64,
) -> Result<u64, E::Error> {
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
    Ok(value)
}

fn optional_gpu_size64_to_u64<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    name: &'static str,
    default: u64,
) -> Result<u64, E::Error> {
    let Some(value) = value else {
        return Ok(default);
    };
    if E::is_undefined(cx, value) {
        Ok(default)
    } else {
        enforce_u64::<E>(cx, value, name)
    }
}

fn optional_gpu_size_to_usize<E: JsEngine>(
    cx: E::Context<'_>,
    value: Option<E::Value>,
    name: &'static str,
    default: usize,
) -> Result<usize, E::Error> {
    let default = u64::try_from(default).map_err(|_| E::type_error(cx, name))?;
    let value = optional_gpu_size_to_u64::<E>(cx, value, name, default)?;
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

fn required_argument<E: JsEngine>(
    cx: E::Context<'_>,
    args: &[E::Value],
    index: usize,
    name: &'static str,
) -> Result<E::Value, E::Error> {
    args.get(index)
        .copied()
        .ok_or_else(|| E::type_error(cx, name))
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

fn device_wrapper_payload<'a, E: JsEngine + 'static>(
    cx: E::Context<'a>,
    value: E::Value,
) -> Result<&'a DevicePayload<E>, E::Error> {
    E::payload(cx, value, GPU_DEVICE_CLASS)
        .and_then(|payload| payload.downcast_ref::<DevicePayload<E>>())
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

fn texture_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTexture, E::Error> {
    E::payload(cx, value, GPU_TEXTURE_CLASS)
        .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        .map(|payload| payload.texture)
        .ok_or_else(|| E::type_error(cx, "GPUTexture is required"))
}

fn texture_wrapper_payload<'a, E: JsEngine + 'static>(
    cx: E::Context<'a>,
    value: E::Value,
) -> Result<&'a TexturePayload, E::Error> {
    E::payload(cx, value, GPU_TEXTURE_CLASS)
        .and_then(|payload| payload.downcast_ref::<TexturePayload>())
        .ok_or_else(|| E::type_error(cx, "GPUTexture method called on an incompatible object"))
}

fn default_texture_view_dimension(
    dimension: WGPUTextureDimension,
    depth_or_array_layers: u32,
) -> WGPUTextureViewDimension {
    match dimension {
        value if value == WGPUTextureDimension_WGPUTextureDimension_1D => {
            WGPUTextureViewDimension_WGPUTextureViewDimension_1D
        }
        value if value == WGPUTextureDimension_WGPUTextureDimension_3D => {
            WGPUTextureViewDimension_WGPUTextureViewDimension_3D
        }
        _ if depth_or_array_layers > 1 => WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray,
        _ => WGPUTextureViewDimension_WGPUTextureViewDimension_2D,
    }
}

fn texture_mip_level_depth(depth_or_array_layers: u32, base_mip_level: u32) -> u32 {
    depth_or_array_layers
        .checked_shr(base_mip_level)
        .unwrap_or(0)
        .max(1)
}

fn texture_view_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUTextureView, E::Error> {
    E::payload(cx, value, GPU_TEXTURE_VIEW_CLASS)
        .and_then(|payload| payload.downcast_ref::<TextureViewPayload>())
        .map(|payload| payload.texture_view)
        .ok_or_else(|| E::type_error(cx, "GPUTextureView is required"))
}

fn query_set_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPUQuerySet, E::Error> {
    E::payload(cx, value, GPU_QUERY_SET_CLASS)
        .and_then(|payload| payload.downcast_ref::<QuerySetPayload>())
        .map(|payload| payload.query_set)
        .ok_or_else(|| E::type_error(cx, "GPUQuerySet is required"))
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

fn render_pipeline_handle<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<WGPURenderPipeline, E::Error> {
    E::payload(cx, value, GPU_RENDER_PIPELINE_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPipelinePayload>())
        .map(|payload| payload.render_pipeline)
        .ok_or_else(|| E::type_error(cx, "GPURenderPipeline is required"))
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
) -> Result<Option<WGPUCommandEncoder>, E::Error> {
    let state = command_encoder_state::<E>(cx, value)?;
    let mut state = state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPUCommandEncoder is finished".to_owned());
        Ok(None)
    } else if state.locked {
        state.pending_validation_error.get_or_insert_with(|| {
            "GPUCommandEncoder was used while locked by an active pass".to_owned()
        });
        Ok(None)
    } else {
        Ok(Some(state.encoder))
    }
}

fn live_compute_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Option<WGPUComputePassEncoder>, E::Error> {
    let payload = E::payload(cx, value, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "GPUComputePassEncoder is required"))?;
    let state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUComputePassEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPUComputePassEncoder is ended".to_owned());
        return Ok(None);
    }
    let parent = state
        .parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if parent.ended {
        state
            .error_sink
            .generate_validation_error("GPUCommandEncoder is finished".to_owned());
        return Ok(None);
    }
    if state.pass.is_null() {
        state
            .error_sink
            .generate_validation_error("GPUComputePassEncoder is invalid".to_owned());
        return Ok(None);
    }
    if !parent.locked {
        state.error_sink.generate_validation_error(
            "GPUCommandEncoder is not locked by this compute pass".to_owned(),
        );
        return Ok(None);
    }
    Ok(Some(state.pass))
}

fn live_render_pass<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Option<WGPURenderPassEncoder>, E::Error> {
    let payload = E::payload(cx, value, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "GPURenderPassEncoder is required"))?;
    let state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPURenderPassEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPURenderPassEncoder is ended".to_owned());
        return Ok(None);
    }
    let parent = state
        .parent
        .lock()
        .map_err(|_| E::operation_error(cx, "GPUCommandEncoder state is poisoned"))?;
    if parent.ended {
        state
            .error_sink
            .generate_validation_error("GPUCommandEncoder is finished".to_owned());
        return Ok(None);
    }
    if state.pass.is_null() {
        state
            .error_sink
            .generate_validation_error("GPURenderPassEncoder is invalid".to_owned());
        return Ok(None);
    }
    if !parent.locked {
        state.error_sink.generate_validation_error(
            "GPUCommandEncoder is not locked by this render pass".to_owned(),
        );
        return Ok(None);
    }
    Ok(Some(state.pass))
}

fn live_render_commands<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Option<LiveRenderCommands>, E::Error> {
    if E::payload(cx, value, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .is_some()
    {
        return live_render_pass::<E>(cx, value).map(|pass| pass.map(LiveRenderCommands::Pass));
    }
    let payload = E::payload(cx, value, GPU_RENDER_BUNDLE_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundleEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "render command encoder is required"))?;
    let state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPURenderBundleEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPURenderBundleEncoder is finished".to_owned());
        return Ok(None);
    }
    Ok(Some(LiveRenderCommands::Bundle(
        state.render_bundle_encoder,
    )))
}

fn live_debug_commands<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    value: E::Value,
) -> Result<Option<LiveDebugCommands>, E::Error> {
    if E::payload(cx, value, GPU_COMMAND_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<CommandEncoderPayload>())
        .is_some()
    {
        return live_command_encoder::<E>(cx, value)
            .map(|encoder| encoder.map(LiveDebugCommands::Command));
    }
    if E::payload(cx, value, GPU_COMPUTE_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<ComputePassEncoderPayload>())
        .is_some()
    {
        return live_compute_pass::<E>(cx, value)
            .map(|pass| pass.map(LiveDebugCommands::ComputePass));
    }
    if E::payload(cx, value, GPU_RENDER_PASS_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderPassEncoderPayload>())
        .is_some()
    {
        return live_render_pass::<E>(cx, value)
            .map(|pass| pass.map(LiveDebugCommands::RenderPass));
    }
    let payload = E::payload(cx, value, GPU_RENDER_BUNDLE_ENCODER_CLASS)
        .and_then(|payload| payload.downcast_ref::<RenderBundleEncoderPayload>())
        .ok_or_else(|| E::type_error(cx, "debug command encoder is required"))?;
    let state = payload
        .state
        .lock()
        .map_err(|_| E::operation_error(cx, "GPURenderBundleEncoder state is poisoned"))?;
    if state.ended {
        state
            .error_sink
            .generate_validation_error("GPURenderBundleEncoder is finished".to_owned());
        return Ok(None);
    }
    Ok(Some(LiveDebugCommands::RenderBundle(
        state.render_bundle_encoder,
    )))
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

fn enforce_i32<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<i32, E::Error> {
    let number = E::to_f64(cx, value)?;
    if !number.is_finite()
        || number.fract() != 0.0
        || !(-2_147_483_648.0..2_147_483_648.0).contains(&number)
    {
        return Err(E::type_error(cx, name));
    }
    Ok(number as i32)
}

fn clamp_u16<E: JsEngine>(cx: E::Context<'_>, value: E::Value) -> Result<u16, E::Error> {
    let number = E::to_f64(cx, value)?;
    let number = if number.is_nan() { 0.0 } else { number };
    Ok(number.clamp(0.0, f64::from(u16::MAX)).round_ties_even() as u16)
}

fn restricted_f32<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<f32, E::Error> {
    let number = E::to_f64(cx, value)?;
    let converted = number as f32;
    if !number.is_finite() || !converted.is_finite() {
        return Err(E::type_error(cx, name));
    }
    Ok(converted)
}

fn restricted_f64<E: JsEngine>(
    cx: E::Context<'_>,
    value: E::Value,
    name: &'static str,
) -> Result<f64, E::Error> {
    let number = E::to_f64(cx, value)?;
    if !number.is_finite() {
        return Err(E::type_error(cx, name));
    }
    Ok(number)
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
    flush: bool,
) -> Result<(), E::Error> {
    let ranges = std::mem::take(&mut state.ranges);
    let mut first_error = None;
    for range in ranges {
        let mut copy_back = Vec::new();
        let should_copy_back = flush && range.map_mode == WGPUMapMode_Write;
        let out = if should_copy_back {
            copy_back.resize(range.size, 0);
            Some(copy_back.as_mut_slice())
        } else {
            None
        };
        let detach_result = E::detach_arraybuffer(cx, range.value, out);
        let detached = E::arraybuffer_len(cx, range.value) == Some(0);
        if let Err(error) = detach_result {
            if first_error.is_none() {
                first_error = Some(error);
            }
        } else if !detached {
            if first_error.is_none() {
                first_error = Some(E::operation_error(cx, "mapped range detach failed"));
            }
        } else if should_copy_back {
            // SAFETY: `native_ptr` was returned for `size` bytes when this JS
            // range was created and remains valid until native unmap/destroy,
            // both of which run only after `detach_all_ranges` returns.
            let dst = unsafe {
                std::slice::from_raw_parts_mut(range.native_ptr.cast::<u8>(), range.size)
            };
            dst.copy_from_slice(&copy_back);
        }
        E::release_value(cx, range.value);
    }
    first_error.map_or(Ok(()), Err)
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

fn mapped_ranges_overlap(
    first_offset: usize,
    first_size: usize,
    second_offset: usize,
    second_size: usize,
) -> bool {
    let Some(first_end) = first_offset.checked_add(first_size) else {
        return true;
    };
    let Some(second_end) = second_offset.checked_add(second_size) else {
        return true;
    };
    // WebGPU's CTS-required boundary rule treats ranges as disjoint when either
    // starts at or after the other's end. Consequently, an empty range at a
    // boundary is disjoint, while one strictly inside a non-empty range overlaps.
    // This is intentionally not the naive empty-set interpretation of [start, end).
    !(first_offset >= second_end || second_offset >= first_end)
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
