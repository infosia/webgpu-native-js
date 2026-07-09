//! Mock JavaScript engine used by core unit tests.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ptr;
use std::sync::Arc;

use crate::{
    Arena, ClassId, ClassSpec, Deferred, Environment, GpuDispatch, JsEngine, MappedRangeStrategy,
    ReleaseQueue, Result, WGPUAdapter, WGPUBuffer, WGPUBufferDescriptor, WGPUBufferMapCallbackInfo,
    WGPUDevice, WGPUMapAsyncStatus, WGPUMapMode, WGPURequestAdapterCallbackInfo,
    WGPURequestDeviceCallbackInfo, WGPUStringView, WGPUStringViewExt,
};
use crate::{
    WGPUBindGroup, WGPUBindGroupDescriptor, WGPUBindGroupLayout, WGPUBindGroupLayoutDescriptor,
    WGPUCommandBuffer, WGPUCommandBufferDescriptor, WGPUCommandEncoder,
    WGPUCommandEncoderDescriptor, WGPUComputePassDescriptor, WGPUComputePassEncoder,
    WGPUComputePipeline, WGPUComputePipelineDescriptor, WGPUFuture, WGPUPipelineLayout,
    WGPUPipelineLayoutDescriptor, WGPUQueue, WGPUQueueWorkDoneCallbackInfo, WGPUShaderModule,
    WGPUShaderModuleDescriptor,
};

/// Mock JavaScript value handle.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Value(usize);

/// Mock JavaScript context.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    runtime: &'a Runtime,
    scope: &'a Scope<'a>,
}

/// Mock per-call value ownership scope.
pub struct Scope<'a> {
    runtime: &'a Runtime,
    owned: RefCell<Vec<Value>>,
}

/// Mock JavaScript runtime.
pub struct Runtime {
    env: Environment,
    values: RefCell<Vec<MockValue>>,
    classes: RefCell<BTreeMap<ClassId, &'static str>>,
    reclaimed_values: Cell<usize>,
    reclaimed_handles: RefCell<Vec<Value>>,
    detach_noop: Cell<bool>,
    duplicated_values: RefCell<BTreeMap<Value, usize>>,
}

impl Runtime {
    /// Creates a mock runtime with the provided WebGPU dispatch.
    #[must_use]
    pub fn new(gpu: GpuDispatch) -> Self {
        Self {
            env: Environment::new(gpu, Arc::new(ReleaseQueue::new())),
            values: RefCell::new(vec![MockValue::Undefined]),
            classes: RefCell::new(BTreeMap::new()),
            reclaimed_values: Cell::new(0),
            reclaimed_handles: RefCell::new(Vec::new()),
            detach_noop: Cell::new(false),
            duplicated_values: RefCell::new(BTreeMap::new()),
        }
    }

    /// Returns a context handle with a long-lived ownership scope.
    #[must_use]
    pub fn context(&self) -> Context<'_> {
        let scope = Box::leak(Box::new(Scope {
            runtime: self,
            owned: RefCell::new(Vec::new()),
        }));
        Context {
            runtime: self,
            scope,
        }
    }

    /// Calls a closure with a per-call ownership scope.
    pub fn with_scope<R>(&self, f: impl FnOnce(Context<'_>) -> R) -> R {
        let scope = Scope {
            runtime: self,
            owned: RefCell::new(Vec::new()),
        };
        let result = f(Context {
            runtime: self,
            scope: &scope,
        });
        drop(scope);
        result
    }

    /// Returns how many scoped values have been reclaimed.
    #[must_use]
    pub fn reclaimed_values(&self) -> usize {
        self.reclaimed_values.get()
    }

    /// Returns the release queue.
    #[must_use]
    pub fn queue(&self) -> &Arc<ReleaseQueue> {
        self.env.queue()
    }

    /// Creates a number value.
    #[must_use]
    pub fn number(&self, value: f64) -> Value {
        self.insert(MockValue::Number(value))
    }

    /// Creates a boolean value.
    #[must_use]
    pub fn bool(&self, value: bool) -> Value {
        self.insert(MockValue::Bool(value))
    }

    /// Creates a null value.
    #[must_use]
    pub fn null(&self) -> Value {
        self.insert(MockValue::Null)
    }

    /// Creates a string value.
    #[must_use]
    pub fn string(&self, value: &str) -> Value {
        self.insert(MockValue::String(value.to_owned()))
    }

    /// Creates an object value from property pairs.
    #[must_use]
    pub fn object(&self, properties: &[(&str, Value)]) -> Value {
        let mut map = BTreeMap::new();
        for (key, value) in properties {
            map.insert((*key).to_owned(), *value);
        }
        self.insert(MockValue::Object(map))
    }

    /// Returns the undefined value.
    #[must_use]
    pub fn undefined(&self) -> Value {
        Value(0)
    }

    /// Replaces an ArrayBuffer's bytes for tests.
    pub fn write_arraybuffer(&self, value: Value, bytes: &[u8]) -> bool {
        self.with_value(value, |stored| match stored {
            MockValue::ArrayBuffer {
                bytes: stored,
                detached: false,
            } => {
                if stored.len() != bytes.len() {
                    return false;
                }
                stored.copy_from_slice(bytes);
                true
            }
            MockValue::ExternalArrayBuffer {
                ptr,
                len,
                detached: false,
            } => {
                if ptr.is_null() || *len != bytes.len() {
                    return false;
                }
                unsafe {
                    std::slice::from_raw_parts_mut(*ptr, *len).copy_from_slice(bytes);
                }
                true
            }
            _ => false,
        })
        .unwrap_or(false)
    }

    /// Configures detach to silently leave buffers attached.
    pub fn set_detach_noop(&self, value: bool) {
        self.detach_noop.set(value);
    }

    /// Reads a copy of an ArrayBuffer's bytes while it is attached.
    #[must_use]
    pub fn read_arraybuffer(&self, value: Value) -> Option<Vec<u8>> {
        self.with_value(value, |stored| match stored {
            MockValue::ArrayBuffer {
                bytes,
                detached: false,
            } => Some(bytes.clone()),
            MockValue::ExternalArrayBuffer {
                ptr,
                len,
                detached: false,
            } if !ptr.is_null() => Some(unsafe { std::slice::from_raw_parts(*ptr, *len).to_vec() }),
            _ => None,
        })
        .flatten()
    }

    /// Returns a settled promise result for tests.
    #[must_use]
    pub fn promise_result(&self, value: Value) -> Option<std::result::Result<Value, Value>> {
        self.with_value(value, |stored| match stored {
            MockValue::Promise {
                settled: true,
                result,
            } => *result,
            _ => None,
        })
        .flatten()
    }

    fn insert(&self, value: MockValue) -> Value {
        let mut values = self.values.borrow_mut();
        values.push(value);
        Value(values.len() - 1)
    }

    fn get(&self, value: Value) -> MockValue {
        self.values
            .borrow()
            .get(value.0)
            .cloned()
            .unwrap_or(MockValue::Undefined)
    }

    fn with_value<R>(&self, value: Value, f: impl FnOnce(&mut MockValue) -> R) -> Option<R> {
        self.values.borrow_mut().get_mut(value.0).map(f)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let duplicated = self.duplicated_values.borrow();
        assert!(
            duplicated.is_empty(),
            "mock duplicated values were not released: {duplicated:?}"
        );
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let count = self.owned.borrow().len();
        self.runtime
            .reclaimed_values
            .set(self.runtime.reclaimed_values.get() + count);
        self.runtime
            .reclaimed_handles
            .borrow_mut()
            .extend(self.owned.borrow().iter().copied());
        self.owned.borrow_mut().clear();
    }
}

#[derive(Clone)]
enum MockValue {
    Undefined,
    Null,
    Number(f64),
    Bool(bool),
    String(String),
    Object(BTreeMap<String, Value>),
    Promise {
        settled: bool,
        result: Option<std::result::Result<Value, Value>>,
    },
    Resolver {
        promise: Value,
        resolve: bool,
    },
    ArrayBuffer {
        bytes: Vec<u8>,
        detached: bool,
    },
    ExternalArrayBuffer {
        ptr: *mut u8,
        len: usize,
        detached: bool,
    },
    Instance {
        class: ClassId,
        payload: &'static (dyn Any + Send),
    },
}

/// Mock engine marker type parameterized by mapped-range behavior.
pub struct MockEngine<const COPY_IN_COPY_OUT: bool>;

/// Default mock engine using zero-copy mapped ranges.
pub type Engine = MockEngine<false>;

impl<const COPY_IN_COPY_OUT: bool> JsEngine for MockEngine<COPY_IN_COPY_OUT> {
    type Value = Value;
    type Context<'a> = Context<'a>;
    type Error = String;
    type DeferredRegistration = ();

    const MAPPED_RANGE_STRATEGY: MappedRangeStrategy = if COPY_IN_COPY_OUT {
        MappedRangeStrategy::CopyInCopyOut
    } else {
        MappedRangeStrategy::ZeroCopyDetach
    };

    fn environment<'a>(cx: Self::Context<'a>) -> &'a Environment {
        &cx.runtime.env
    }

    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> Result<Self::Value, Self::Error> {
        match cx.runtime.get(obj) {
            MockValue::Object(map) => {
                let value = map
                    .get(key)
                    .copied()
                    .unwrap_or_else(|| cx.runtime.undefined());
                cx.scope.owned.borrow_mut().push(value);
                Ok(value)
            }
            _ => Ok(cx.runtime.undefined()),
        }
    }

    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(cx.runtime.get(value), MockValue::Undefined)
    }

    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(cx.runtime.get(value), MockValue::Null)
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> Result<f64, Self::Error> {
        match cx.runtime.get(value) {
            MockValue::Number(value) => Ok(value),
            MockValue::Bool(value) => Ok(if value { 1.0 } else { 0.0 }),
            MockValue::String(value) => value.parse::<f64>().map_err(|_| "number".to_owned()),
            MockValue::Undefined
            | MockValue::Null
            | MockValue::Object(_)
            | MockValue::Promise { .. }
            | MockValue::Resolver { .. }
            | MockValue::ArrayBuffer { .. }
            | MockValue::ExternalArrayBuffer { .. }
            | MockValue::Instance { .. } => Err("number".to_owned()),
        }
    }

    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool {
        match cx.runtime.get(value) {
            MockValue::Undefined => false,
            MockValue::Null => false,
            MockValue::Bool(value) => value,
            MockValue::Number(value) => value != 0.0 && !value.is_nan(),
            MockValue::String(value) => !value.is_empty(),
            MockValue::Object(_)
            | MockValue::Promise { .. }
            | MockValue::Resolver { .. }
            | MockValue::ArrayBuffer { .. }
            | MockValue::ExternalArrayBuffer { .. }
            | MockValue::Instance { .. } => true,
        }
    }

    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a Arena,
    ) -> Result<&'a str, Self::Error> {
        match cx.runtime.get(value) {
            MockValue::Undefined => Ok(arena.alloc_str("undefined")),
            MockValue::Null => Ok(arena.alloc_str("null")),
            MockValue::Number(value) => Ok(arena.alloc_str(&value.to_string())),
            MockValue::Bool(value) => Ok(arena.alloc_str(if value { "true" } else { "false" })),
            MockValue::String(value) => Ok(arena.alloc_str(&value)),
            MockValue::Object(_)
            | MockValue::Promise { .. }
            | MockValue::Resolver { .. }
            | MockValue::ArrayBuffer { .. }
            | MockValue::ExternalArrayBuffer { .. }
            | MockValue::Instance { .. } => Ok(arena.alloc_str("[object Object]")),
        }
    }

    fn register_class(
        cx: Self::Context<'_>,
        spec: &'static ClassSpec<Self>,
    ) -> Result<ClassId, Self::Error> {
        cx.runtime.classes.borrow_mut().insert(spec.id, spec.name);
        Ok(spec.id)
    }

    fn new_instance(
        cx: Self::Context<'_>,
        class: ClassId,
        payload: Box<dyn Any + Send>,
    ) -> Result<Self::Value, Self::Error> {
        let payload = Box::leak(payload);
        Ok(cx.runtime.insert(MockValue::Instance { class, payload }))
    }

    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: ClassId,
    ) -> Option<&'a (dyn Any + Send)> {
        match cx.runtime.get(obj) {
            MockValue::Instance {
                class: actual,
                payload,
            } if actual == class => Some(payload),
            _ => None,
        }
    }

    fn trace_payload(
        cx: Self::Context<'_>,
        payload: &(dyn Any + Send),
        visit: &mut dyn FnMut(Self::Value),
    ) {
        if let Some(buffer) = payload.downcast_ref::<crate::BufferPayload<Self>>() {
            buffer.trace_mapped_range_values(|value| {
                assert!(
                    value.0 < cx.runtime.values.borrow().len(),
                    "mock traced value was not issued: {value:?}"
                );
                assert!(
                    !cx.runtime.reclaimed_handles.borrow().contains(&value),
                    "mock payload traced a reclaimed value: {value:?}"
                );
                visit(value);
            });
        }
    }

    fn undefined(cx: Self::Context<'_>) -> Self::Value {
        cx.runtime.undefined()
    }

    fn number(cx: Self::Context<'_>, value: f64) -> Result<Self::Value, Self::Error> {
        Ok(cx.runtime.number(value))
    }

    fn string(cx: Self::Context<'_>, value: &str) -> Result<Self::Value, Self::Error> {
        Ok(cx.runtime.string(value))
    }

    fn type_error(_cx: Self::Context<'_>, message: &str) -> Self::Error {
        format!("TypeError: {message}")
    }

    fn operation_error(_cx: Self::Context<'_>, message: &str) -> Self::Error {
        format!("OperationError: {message}")
    }

    fn async_error_value(cx: Self::Context<'_>, message: &str) -> Self::Value {
        cx.runtime.string(message)
    }

    fn error_value_from_error(cx: Self::Context<'_>, error: Self::Error) -> Self::Value {
        cx.runtime.string(&error)
    }

    fn new_promise(cx: Self::Context<'_>) -> Result<(Self::Value, Deferred<Self>), Self::Error> {
        let promise = cx.runtime.insert(MockValue::Promise {
            settled: false,
            result: None,
        });
        let resolve = cx.runtime.insert(MockValue::Resolver {
            promise,
            resolve: true,
        });
        let reject = cx.runtime.insert(MockValue::Resolver {
            promise,
            resolve: false,
        });
        Ok((promise, Deferred::new(resolve, reject)))
    }

    fn settle_deferreds(cx: Self::Context<'_>, settlements: Vec<crate::DeferredSettlement<Self>>) {
        for (deferred, result) in settlements {
            let runtime = cx.runtime;
            let resolver = match result {
                Ok(_) => deferred.resolve(),
                Err(_) => deferred.reject(),
            };
            let MockValue::Resolver { promise, resolve } = runtime.get(resolver) else {
                continue;
            };
            let actual_is_ok = result.is_ok();
            if resolve != actual_is_ok {
                continue;
            }
            let _ = runtime.with_value(promise, |value| {
                if let MockValue::Promise {
                    settled,
                    result: stored,
                } = value
                {
                    if !*settled {
                        *settled = true;
                        *stored = Some(result);
                    }
                }
            });
        }
    }

    unsafe fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        ptr: *mut u8,
        len: usize,
        _owner: WGPUBuffer,
    ) -> Result<Self::Value, Self::Error> {
        Ok(cx.runtime.insert(MockValue::ExternalArrayBuffer {
            ptr,
            len,
            detached: false,
        }))
    }

    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> Result<Self::Value, Self::Error> {
        Ok(cx.runtime.insert(MockValue::ArrayBuffer {
            bytes: bytes.to_vec(),
            detached: false,
        }))
    }

    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        value: Self::Value,
        out: Option<&mut [u8]>,
    ) -> Result<(), Self::Error> {
        if cx.runtime.detach_noop.get() {
            return Ok(());
        }
        cx.runtime
            .with_value(value, |stored| match stored {
                MockValue::ArrayBuffer { bytes, detached } => {
                    if *detached {
                        return Ok(());
                    }
                    if let Some(out) = out {
                        if out.len() != bytes.len() {
                            return Err("arraybuffer length mismatch".to_owned());
                        }
                        let product = bytes.clone();
                        *detached = true;
                        bytes.clear();
                        out.copy_from_slice(&product);
                    } else {
                        *detached = true;
                        bytes.clear();
                    }
                    Ok(())
                }
                MockValue::ExternalArrayBuffer { detached, .. } => {
                    *detached = true;
                    Ok(())
                }
                _ => Err("arraybuffer".to_owned()),
            })
            .unwrap_or_else(|| Err("arraybuffer".to_owned()))
    }

    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize> {
        match cx.runtime.get(value) {
            MockValue::ArrayBuffer {
                bytes, detached, ..
            } => Some(if detached { 0 } else { bytes.len() }),
            MockValue::ExternalArrayBuffer { len, detached, .. } => {
                Some(if detached { 0 } else { len })
            }
            _ => None,
        }
    }

    fn arraybuffer_copy(cx: Self::Context<'_>, value: Self::Value) -> Option<Vec<u8>> {
        cx.runtime.read_arraybuffer(value)
    }

    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value {
        let mut duplicated = cx.runtime.duplicated_values.borrow_mut();
        *duplicated.entry(value).or_insert(0) += 1;
        value
    }

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        let mut duplicated = cx.runtime.duplicated_values.borrow_mut();
        let count = duplicated
            .get_mut(&value)
            .unwrap_or_else(|| panic!("mock value released without duplicate: {value:?}"));
        *count -= 1;
        if *count == 0 {
            duplicated.remove(&value);
        }
    }

    fn register_deferred(
        _cx: Self::Context<'_>,
        _slot: std::ptr::NonNull<Option<Deferred<Self>>>,
    ) -> Self::DeferredRegistration {
    }

    fn release_deferred(_cx: Self::Context<'_>, _deferred: Deferred<Self>) {}
}

thread_local! {
    static GPU_STATE: RefCell<MockGpuState> = RefCell::new(MockGpuState::default());
}

#[derive(Default)]
struct MockGpuState {
    next: usize,
    device_add_refs: usize,
    buffer_add_refs: usize,
    queue_add_refs: usize,
    device_releases: usize,
    buffer_releases: usize,
    queue_releases: usize,
    buffer_destroys: usize,
    mapped_range_calls: usize,
    const_mapped_range_calls: usize,
    labels: Vec<Vec<u8>>,
    descriptors: Vec<RecordedDescriptor>,
    null_create_buffer: bool,
    native_order: Vec<&'static str>,
    buffers: BTreeMap<WGPUBuffer, Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedDescriptor {
    size: u64,
    usage: u64,
    mapped: u32,
    label: Vec<u8>,
}

/// Resets the mock GPU call log.
pub fn reset_gpu() {
    GPU_STATE.with(|state| {
        *state.borrow_mut() = MockGpuState::default();
    });
}

/// Configures the next create-buffer call to return null.
pub fn set_null_create_buffer(value: bool) {
    GPU_STATE.with(|state| state.borrow_mut().null_create_buffer = value);
}

/// Creates a runtime with the mock GPU dispatch.
#[must_use]
pub fn runtime() -> Runtime {
    Runtime::new(dispatch())
}

/// Returns mock WebGPU dispatch functions.
#[must_use]
pub fn dispatch() -> GpuDispatch {
    GpuDispatch {
        instance_request_adapter,
        adapter_request_device,
        adapter_release,
        device_add_ref,
        device_release,
        device_create_buffer,
        device_get_queue,
        device_create_shader_module,
        device_create_bind_group_layout,
        device_create_pipeline_layout,
        device_create_bind_group,
        device_create_compute_pipeline,
        device_create_command_encoder,
        buffer_set_label,
        buffer_destroy,
        buffer_get_mapped_range,
        buffer_get_const_mapped_range,
        buffer_add_ref,
        buffer_map_async,
        buffer_unmap,
        buffer_release,
        queue_add_ref,
        queue_release,
        queue_write_buffer,
        queue_submit,
        queue_on_submitted_work_done,
        shader_module_add_ref,
        shader_module_release,
        bind_group_layout_add_ref,
        bind_group_layout_release,
        pipeline_layout_add_ref,
        pipeline_layout_release,
        bind_group_add_ref,
        bind_group_release,
        compute_pipeline_add_ref,
        compute_pipeline_release,
        command_encoder_release,
        command_encoder_copy_buffer_to_buffer,
        command_encoder_begin_compute_pass,
        command_encoder_finish,
        command_buffer_release,
        compute_pass_encoder_release,
        compute_pass_encoder_set_pipeline,
        compute_pass_encoder_set_bind_group,
        compute_pass_encoder_dispatch_workgroups,
        compute_pass_encoder_end,
    }
}

/// Creates a non-null fake device handle.
#[must_use]
pub fn fake_device() -> WGPUDevice {
    ptr::NonNull::<u8>::dangling().as_ptr().cast()
}

/// Returns a copy of mock native buffer bytes.
#[must_use]
pub fn buffer_bytes(buffer: WGPUBuffer) -> Option<Vec<u8>> {
    GPU_STATE.with(|state| state.borrow().buffers.get(&buffer).cloned())
}

fn fake_buffer(id: usize) -> WGPUBuffer {
    id as WGPUBuffer
}

fn fake_handle<T>(id: usize) -> *mut T {
    id as *mut T
}

unsafe fn device_add_ref(_device: WGPUDevice) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.device_add_refs += 1;
        state.native_order.push("device_add_ref");
    });
}

unsafe fn instance_request_adapter(
    _instance: webgpu_native_js_ffi::native::WGPUInstance,
    _options: *const webgpu_native_js_ffi::native::WGPURequestAdapterOptions,
    info: WGPURequestAdapterCallbackInfo,
) -> webgpu_native_js_ffi::native::WGPUFuture {
    if let Some(callback) = info.callback {
        unsafe {
            callback(
                webgpu_native_js_ffi::native::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success,
                fake_device().cast::<webgpu_native_js_ffi::native::WGPUAdapterImpl>(),
                WGPUStringView::from_bytes(b""),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    webgpu_native_js_ffi::native::WGPUFuture { id: 1 }
}

unsafe fn adapter_request_device(
    _adapter: WGPUAdapter,
    _descriptor: *const webgpu_native_js_ffi::native::WGPUDeviceDescriptor,
    info: WGPURequestDeviceCallbackInfo,
) -> webgpu_native_js_ffi::native::WGPUFuture {
    if let Some(callback) = info.callback {
        unsafe {
            callback(
                webgpu_native_js_ffi::native::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success,
                fake_device(),
                WGPUStringView::from_bytes(b""),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    webgpu_native_js_ffi::native::WGPUFuture { id: 2 }
}

unsafe fn adapter_release(_adapter: WGPUAdapter) {}

unsafe fn device_release(_device: WGPUDevice) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.device_releases += 1;
        state.native_order.push("device_release");
    });
}

unsafe fn device_create_buffer(
    _device: WGPUDevice,
    descriptor: *const WGPUBufferDescriptor,
) -> WGPUBuffer {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.null_create_buffer {
            return ptr::null_mut();
        }
        let descriptor = unsafe { &*descriptor };
        state.descriptors.push(RecordedDescriptor {
            size: descriptor.size,
            usage: descriptor.usage,
            mapped: descriptor.mappedAtCreation,
            label: read_view(descriptor.label),
        });
        state.next += 1;
        let id = state.next;
        let buffer = fake_buffer(id);
        state
            .buffers
            .insert(buffer, vec![0; descriptor.size as usize]);
        buffer
    })
}

unsafe fn device_get_queue(_device: WGPUDevice) -> WGPUQueue {
    fake_handle(1001)
}

unsafe fn device_create_shader_module(
    _device: WGPUDevice,
    _descriptor: *const WGPUShaderModuleDescriptor,
) -> WGPUShaderModule {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(2000 + state.next)
    })
}

unsafe fn device_create_bind_group_layout(
    _device: WGPUDevice,
    _descriptor: *const WGPUBindGroupLayoutDescriptor,
) -> WGPUBindGroupLayout {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(3000 + state.next)
    })
}

unsafe fn device_create_pipeline_layout(
    _device: WGPUDevice,
    _descriptor: *const WGPUPipelineLayoutDescriptor,
) -> WGPUPipelineLayout {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(4000 + state.next)
    })
}

unsafe fn device_create_bind_group(
    _device: WGPUDevice,
    _descriptor: *const WGPUBindGroupDescriptor,
) -> WGPUBindGroup {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(5000 + state.next)
    })
}

unsafe fn device_create_compute_pipeline(
    _device: WGPUDevice,
    _descriptor: *const WGPUComputePipelineDescriptor,
) -> WGPUComputePipeline {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(6000 + state.next)
    })
}

unsafe fn device_create_command_encoder(
    _device: WGPUDevice,
    _descriptor: *const WGPUCommandEncoderDescriptor,
) -> WGPUCommandEncoder {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.next += 1;
        fake_handle(7000 + state.next)
    })
}

unsafe fn buffer_set_label(_buffer: WGPUBuffer, label: WGPUStringView) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.labels.push(read_view(label));
        state.native_order.push("buffer_set_label");
    });
}

unsafe fn buffer_destroy(_buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_destroys += 1;
        state.native_order.push("buffer_destroy");
    });
}

unsafe fn buffer_get_mapped_range(
    buffer: WGPUBuffer,
    offset: usize,
    size: usize,
) -> *mut std::ffi::c_void {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.mapped_range_calls += 1;
        let Some(bytes) = state.buffers.get_mut(&buffer) else {
            return ptr::null_mut();
        };
        let len = if size == crate::WGPU_WHOLE_MAP_SIZE as usize {
            bytes.len().saturating_sub(offset)
        } else {
            size
        };
        let Some(end) = offset.checked_add(len) else {
            return ptr::null_mut();
        };
        if end > bytes.len() {
            return ptr::null_mut();
        }
        unsafe { bytes.as_mut_ptr().add(offset).cast() }
    })
}

unsafe fn buffer_get_const_mapped_range(
    buffer: WGPUBuffer,
    offset: usize,
    size: usize,
) -> *const std::ffi::c_void {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.const_mapped_range_calls += 1;
        let Some(bytes) = state.buffers.get(&buffer) else {
            return ptr::null();
        };
        let len = if size == crate::WGPU_WHOLE_MAP_SIZE as usize {
            bytes.len().saturating_sub(offset)
        } else {
            size
        };
        let Some(end) = offset.checked_add(len) else {
            return ptr::null();
        };
        if end > bytes.len() {
            return ptr::null();
        }
        unsafe { bytes.as_ptr().add(offset).cast() }
    })
}

unsafe fn buffer_add_ref(_buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_add_refs += 1;
        state.native_order.push("buffer_add_ref");
    });
}

unsafe fn buffer_map_async(
    _buffer: WGPUBuffer,
    _mode: WGPUMapMode,
    _offset: usize,
    _size: usize,
    info: WGPUBufferMapCallbackInfo,
) -> webgpu_native_js_ffi::native::WGPUFuture {
    if let Some(callback) = info.callback {
        unsafe {
            callback(
                crate::WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success,
                WGPUStringView::from_bytes(b""),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    webgpu_native_js_ffi::native::WGPUFuture { id: 3 }
}

unsafe fn buffer_unmap(_buffer: WGPUBuffer) {}

unsafe fn buffer_release(_buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_releases += 1;
        state.native_order.push("buffer_release");
    });
}

unsafe fn queue_add_ref(_queue: WGPUQueue) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.queue_add_refs += 1;
        state.native_order.push("queue_add_ref");
    });
}

unsafe fn queue_release(_queue: WGPUQueue) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.queue_releases += 1;
        state.native_order.push("queue_release");
    });
}

unsafe fn queue_write_buffer(
    _queue: WGPUQueue,
    buffer: WGPUBuffer,
    offset: u64,
    data: *const std::ffi::c_void,
    size: usize,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(bytes) = state.buffers.get_mut(&buffer) else {
            return;
        };
        let Ok(offset) = usize::try_from(offset) else {
            return;
        };
        let Some(end) = offset.checked_add(size) else {
            return;
        };
        if data.is_null() || end > bytes.len() {
            return;
        }
        bytes[offset..end]
            .copy_from_slice(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) });
    });
}

unsafe fn queue_submit(_queue: WGPUQueue, _count: usize, _commands: *const WGPUCommandBuffer) {}

unsafe fn queue_on_submitted_work_done(
    _queue: WGPUQueue,
    info: WGPUQueueWorkDoneCallbackInfo,
) -> WGPUFuture {
    if let Some(callback) = info.callback {
        unsafe {
            callback(
                crate::WGPUQueueWorkDoneStatus_WGPUQueueWorkDoneStatus_Success,
                WGPUStringView::from_bytes(b""),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    WGPUFuture { id: 4 }
}

unsafe fn shader_module_add_ref(_module: WGPUShaderModule) {}
unsafe fn shader_module_release(_module: WGPUShaderModule) {}
unsafe fn bind_group_layout_add_ref(_layout: WGPUBindGroupLayout) {}
unsafe fn bind_group_layout_release(_layout: WGPUBindGroupLayout) {}
unsafe fn pipeline_layout_add_ref(_layout: WGPUPipelineLayout) {}
unsafe fn pipeline_layout_release(_layout: WGPUPipelineLayout) {}
unsafe fn bind_group_add_ref(_bind_group: WGPUBindGroup) {}
unsafe fn bind_group_release(_bind_group: WGPUBindGroup) {}
unsafe fn compute_pipeline_add_ref(_pipeline: WGPUComputePipeline) {}
unsafe fn compute_pipeline_release(_pipeline: WGPUComputePipeline) {}
unsafe fn command_encoder_release(_encoder: WGPUCommandEncoder) {}

unsafe fn command_encoder_copy_buffer_to_buffer(
    _encoder: WGPUCommandEncoder,
    source: WGPUBuffer,
    source_offset: u64,
    destination: WGPUBuffer,
    destination_offset: u64,
    size: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(src) = state.buffers.get(&source).cloned() else {
            return;
        };
        let Some(dst) = state.buffers.get_mut(&destination) else {
            return;
        };
        let (Ok(source_offset), Ok(destination_offset), Ok(size)) = (
            usize::try_from(source_offset),
            usize::try_from(destination_offset),
            usize::try_from(size),
        ) else {
            return;
        };
        let Some(src_end) = source_offset.checked_add(size) else {
            return;
        };
        let Some(dst_end) = destination_offset.checked_add(size) else {
            return;
        };
        if src_end <= src.len() && dst_end <= dst.len() {
            dst[destination_offset..dst_end].copy_from_slice(&src[source_offset..src_end]);
        }
    });
}

unsafe fn command_encoder_begin_compute_pass(
    _encoder: WGPUCommandEncoder,
    _descriptor: *const WGPUComputePassDescriptor,
) -> WGPUComputePassEncoder {
    fake_handle(8001)
}

unsafe fn command_encoder_finish(
    _encoder: WGPUCommandEncoder,
    _descriptor: *const WGPUCommandBufferDescriptor,
) -> WGPUCommandBuffer {
    fake_handle(9001)
}

unsafe fn command_buffer_release(_command_buffer: WGPUCommandBuffer) {}
unsafe fn compute_pass_encoder_release(_pass: WGPUComputePassEncoder) {}
unsafe fn compute_pass_encoder_set_pipeline(
    _pass: WGPUComputePassEncoder,
    _pipeline: WGPUComputePipeline,
) {
}
unsafe fn compute_pass_encoder_set_bind_group(
    _pass: WGPUComputePassEncoder,
    _index: u32,
    _bind_group: WGPUBindGroup,
    _offset_count: usize,
    _offsets: *const u32,
) {
}
unsafe fn compute_pass_encoder_dispatch_workgroups(
    _pass: WGPUComputePassEncoder,
    _x: u32,
    _y: u32,
    _z: u32,
) {
}
unsafe fn compute_pass_encoder_end(_pass: WGPUComputePassEncoder) {}

fn read_view(view: WGPUStringView) -> Vec<u8> {
    if view.data.is_null() || view.length == crate::wgpu_strlen() {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length).to_vec() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buffer_destroy, buffer_get_mapped_range, buffer_label_get, buffer_label_set,
        buffer_map_async, buffer_size_get, buffer_unmap, buffer_usage_get,
        convert_bind_group_descriptor, convert_bind_group_layout_descriptor,
        convert_buffer_binding_layout, convert_buffer_descriptor,
        convert_compute_pipeline_descriptor, convert_shader_module_descriptor,
        device_create_bind_group, device_create_buffer, device_queue_get, finalize_buffer,
        finalize_device, finalize_queue, optional_gpu_size_to_usize, wrap_device,
        BindGroupLayoutPayload, BufferPayload, DevicePayload, JsEngine, QueuePayload,
        ShaderModulePayload,
    };

    fn descriptor(rt: &Runtime, fields: &[(&str, Value)]) -> Value {
        rt.object(fields)
    }

    #[test]
    fn r8_accepts_required_size_usage_and_defaults() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[("size", rt.number(256.0)), ("usage", rt.number(8.0))],
        );
        let arena = Arena::new();
        let converted = convert_buffer_descriptor::<Engine>(cx, desc, &arena).expect("descriptor");
        assert_eq!(converted.size, 256);
        assert_eq!(converted.usage, 8);
        assert!(!converted.mapped_at_creation);
        assert_eq!(converted.label, "");
    }

    #[test]
    fn r8_rejects_missing_size() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("usage", rt.number(8.0))]);
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_rejects_missing_usage() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("size", rt.number(256.0))]);
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_rejects_usage_above_u32() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(256.0)),
                ("usage", rt.number(4_294_967_296.0)),
            ],
        );
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_rejects_size_at_two_to_the_64_boundary() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(18_446_744_073_709_551_616.0)),
                ("usage", rt.number(8.0)),
            ],
        );
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_rejects_non_finite_size_and_usage() {
        for (size, usage) in [
            (f64::NAN, 8.0),
            (f64::INFINITY, 8.0),
            (256.0, f64::NAN),
            (256.0, f64::INFINITY),
        ] {
            let rt = runtime();
            let cx = rt.context();
            let desc = descriptor(
                &rt,
                &[("size", rt.number(size)), ("usage", rt.number(usage))],
            );
            let arena = Arena::new();
            assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
        }
    }

    #[test]
    fn r8_rejects_negative_size() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("size", rt.number(-1.0)), ("usage", rt.number(8.0))]);
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_rejects_non_integral_size() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("size", rt.number(1.5)), ("usage", rt.number(8.0))]);
        let arena = Arena::new();
        assert!(convert_buffer_descriptor::<Engine>(cx, desc, &arena).is_err());
    }

    #[test]
    fn r8_mapped_at_creation_uses_to_boolean() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(256.0)),
                ("usage", rt.number(8.0)),
                ("mappedAtCreation", rt.string("false")),
            ],
        );
        let arena = Arena::new();
        let converted = convert_buffer_descriptor::<Engine>(cx, desc, &arena).expect("descriptor");
        assert!(converted.mapped_at_creation);
    }

    #[test]
    fn r9_string_view_validity_distinguishes_null_and_empty() {
        let empty = WGPUStringView::from_bytes(b"");
        assert!(empty.is_valid());
        assert!(!empty.data.is_null());
        assert_eq!(empty.length, 0);
        assert!(WGPUStringView {
            data: ptr::null(),
            length: crate::wgpu_strlen()
        }
        .is_valid());
        assert!(!WGPUStringView {
            data: ptr::null(),
            length: 1
        }
        .is_valid());
    }

    #[test]
    fn b3_wgsl_chain_sets_stype_and_points_at_source() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[(
                "code",
                rt.string("@compute @workgroup_size(1) fn main() {}"),
            )],
        );
        let arena = Arena::new();

        let native = convert_shader_module_descriptor::<Engine>(cx, desc, &arena)
            .expect("shader descriptor");

        assert!(!native.nextInChain.is_null());
        let chain = unsafe { &*native.nextInChain };
        assert_eq!(chain.sType, crate::WGPUSType_WGPUSType_ShaderSourceWGSL);
        let source = native.nextInChain.cast::<crate::WGPUShaderSourceWGSL>();
        assert_eq!(
            read_view(unsafe { (*source).code }),
            b"@compute @workgroup_size(1) fn main() {}".to_vec()
        );
    }

    #[test]
    fn b4_nullable_entry_point_differs_from_non_null_label() {
        let rt = runtime();
        let cx = rt.context();
        let module = Engine::new_instance(
            cx,
            crate::GPU_SHADER_MODULE_CLASS,
            Box::new(ShaderModulePayload {
                module: fake_handle(42),
            }),
        )
        .expect("shader module");
        let label_desc = descriptor(
            &rt,
            &[("code", rt.string("fn only() {}")), ("label", rt.null())],
        );
        let arena = Arena::new();
        let shader = convert_shader_module_descriptor::<Engine>(cx, label_desc, &arena)
            .expect("shader descriptor");
        assert!(!shader.label.data.is_null());
        assert_eq!(shader.label.length, 0);

        let absent_compute = descriptor(&rt, &[("module", module)]);
        let absent_desc = descriptor(&rt, &[("compute", absent_compute)]);
        let absent = convert_compute_pipeline_descriptor::<Engine>(cx, absent_desc, &arena)
            .expect("pipeline descriptor");
        assert!(absent.compute.entryPoint.data.is_null());
        assert_eq!(absent.compute.entryPoint.length, crate::wgpu_strlen());

        let empty_compute = descriptor(&rt, &[("module", module), ("entryPoint", rt.string(""))]);
        let empty_desc = descriptor(&rt, &[("compute", empty_compute)]);
        let empty = convert_compute_pipeline_descriptor::<Engine>(cx, empty_desc, &arena)
            .expect("pipeline descriptor");
        assert!(!empty.compute.entryPoint.data.is_null());
        assert_eq!(empty.compute.entryPoint.length, 0);
    }

    #[test]
    fn b5_empty_and_nonempty_sequences_have_valid_count_pointer_shapes() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();

        let empty_entries = descriptor(&rt, &[("length", rt.number(0.0))]);
        let empty_desc = descriptor(&rt, &[("entries", empty_entries)]);
        let empty = convert_bind_group_layout_descriptor::<Engine>(cx, empty_desc, &arena)
            .expect("empty layout");
        assert_eq!(empty.entryCount, 0);
        assert!(empty.entries.is_null());

        let buffer_layout = descriptor(&rt, &[("type", rt.string("uniform"))]);
        let entry = descriptor(
            &rt,
            &[
                ("binding", rt.number(0.0)),
                ("visibility", rt.number(1.0)),
                ("buffer", buffer_layout),
            ],
        );
        let entries = descriptor(&rt, &[("length", rt.number(1.0)), ("0", entry)]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        let one = convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
            .expect("one-entry layout");
        assert_eq!(one.entryCount, 1);
        assert!(!one.entries.is_null());
        assert_eq!(unsafe { (*one.entries).binding }, 0);
    }

    #[test]
    fn b6_buffer_binding_type_rejects_unknown_strings_and_numbers() {
        let rt = runtime();
        let cx = rt.context();

        let unknown = descriptor(&rt, &[("type", rt.string("definitely-not-a-buffer-type"))]);
        assert_eq!(
            convert_buffer_binding_layout::<Engine>(cx, unknown).expect_err("unknown enum"),
            "TypeError: GPUBufferBindingType"
        );

        let number = descriptor(&rt, &[("type", rt.number(1.0))]);
        assert_eq!(
            convert_buffer_binding_layout::<Engine>(cx, number).expect_err("numeric enum"),
            "TypeError: GPUBufferBindingType"
        );
    }

    #[test]
    fn b7_write_buffer_rejects_size_that_would_truncate_on_32_bit_hosts() {
        let rt = runtime();
        let cx = rt.context();

        let error =
            optional_gpu_size_to_usize::<Engine>(cx, Some(rt.number(4_294_967_296.0)), "size", 0)
                .expect_err("oversized size must fail before narrowing");

        assert_eq!(error, "TypeError: size");
    }

    #[test]
    fn device_queue_get_balances_returned_owned_queue_reference() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let _queue = device_queue_get::<Engine>(cx, device).expect("queue");

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.queue_add_refs, 0);
            assert_eq!(state.queue_releases, 0);
        });
        finalize_queue(
            Box::new(QueuePayload {
                queue: fake_handle(1001),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain queue release"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.queue_add_refs, 0);
            assert_eq!(state.queue_releases, 1);
        });
    }

    #[test]
    fn failed_bind_group_sequence_conversion_leaks_no_addref_and_allocates_no_entries() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer_desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[buffer_desc]).expect("buffer");
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
            }),
        )
        .expect("layout");
        let first_resource = descriptor(&rt, &[("buffer", buffer)]);
        let first_entry = descriptor(
            &rt,
            &[("binding", rt.number(0.0)), ("resource", first_resource)],
        );
        let bad_entry = descriptor(&rt, &[("binding", rt.number(1.0))]);
        let entries = descriptor(
            &rt,
            &[
                ("length", rt.number(2.0)),
                ("0", first_entry),
                ("1", bad_entry),
            ],
        );
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);
        let arena = Arena::new();

        assert!(convert_bind_group_descriptor::<Engine>(cx, desc, &arena).is_err());
        assert_eq!(arena.bind_group_entries.borrow().len(), 0);
        assert!(device_create_bind_group::<Engine>(cx, device, &[desc]).is_err());
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().buffer_add_refs, 0);
        });
    }

    /// This asserts we call `AddRef` once per stored buffer. It does not prove
    /// the backend needs it. The C ABI has no refcount introspection, so this
    /// is the strongest available check.
    #[test]
    fn b8_bind_group_addrefs_each_stored_buffer() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer_desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[buffer_desc]).expect("buffer");
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
            }),
        )
        .expect("layout");
        let resource = descriptor(&rt, &[("buffer", buffer)]);
        let entry = descriptor(&rt, &[("binding", rt.number(0.0)), ("resource", resource)]);
        let entries = descriptor(&rt, &[("length", rt.number(1.0)), ("0", entry)]);
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);

        let _bind_group =
            device_create_bind_group::<Engine>(cx, device, &[desc]).expect("bind group");

        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().buffer_add_refs, 1);
        });
    }

    #[test]
    fn b18_a29_read_mapping_uses_const_range_and_write_uses_mutable_range() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(8.0)), ("usage", rt.number(3.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");

        let _ = buffer_map_async::<Engine>(cx, buffer, &[rt.number(1.0)]).expect("read map");
        let _ = buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(0.0), rt.number(4.0)])
            .expect("read range");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.const_mapped_range_calls, 1);
            assert_eq!(state.mapped_range_calls, 0);
        });

        let _ = buffer_unmap::<Engine>(cx, buffer, &[]).expect("unmap");
        let _ = buffer_map_async::<Engine>(cx, buffer, &[rt.number(2.0)]).expect("write map");
        let _ = buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(0.0), rt.number(4.0)])
            .expect("write range");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.const_mapped_range_calls, 1);
            assert_eq!(state.mapped_range_calls, 1);
        });
        let _ = buffer_unmap::<Engine>(cx, buffer, &[]).expect("final unmap");
    }

    #[test]
    fn r10_label_bytes_survive_the_create_buffer_call() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(16.0)),
                ("usage", rt.number(8.0)),
                ("label", rt.string("abc")),
            ],
        );
        let _ = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().descriptors[0].label, b"abc");
        });
    }

    #[test]
    fn r23_heap_property_values_are_reclaimed_by_scope() {
        let rt = runtime();
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(16.0)),
                ("usage", rt.number(8.0)),
                ("label", rt.string("scoped")),
            ],
        );
        rt.with_scope(|cx| {
            let arena = Arena::new();
            let converted =
                convert_buffer_descriptor::<Engine>(cx, desc, &arena).expect("descriptor");
            assert_eq!(converted.label, "scoped");
        });
    }

    #[test]
    fn r11_accepts_integral_size_that_arrives_as_number() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(9_007_199_254_740_992.0)),
                ("usage", rt.number(8.0)),
            ],
        );
        let arena = Arena::new();
        let converted = convert_buffer_descriptor::<Engine>(cx, desc, &arena).expect("descriptor");
        assert_eq!(converted.size, 9_007_199_254_740_992);
    }

    #[test]
    fn r12_label_getter_reads_wrapper_copy() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        buffer_label_set::<Engine>(cx, buffer, rt.string("new")).expect("set label");
        let label = buffer_label_get::<Engine>(cx, buffer).expect("get label");
        assert_eq!(
            Engine::to_str(cx, label, &Arena::new()).expect("string"),
            "new"
        );
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().labels, vec![b"new".to_vec()]);
        });
    }

    #[test]
    fn r13_null_create_buffer_is_an_error() {
        reset_gpu();
        set_null_create_buffer(true);
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        assert!(device_create_buffer::<Engine>(cx, device, &[desc]).is_err());
    }

    #[test]
    fn r14_destroy_is_idempotent_and_release_is_queued_later() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let _ = buffer_destroy::<Engine>(cx, buffer, &[]).expect("destroy");
        let _ = buffer_destroy::<Engine>(cx, buffer, &[]).expect("destroy");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.buffer_destroys, 1);
            assert_eq!(state.buffer_releases, 0);
        });
    }

    #[test]
    fn r15_size_and_usage_getters_are_synchronous() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        assert_eq!(
            Engine::to_f64(cx, buffer_size_get::<Engine>(cx, buffer).expect("size")).expect("num"),
            16.0
        );
        assert_eq!(
            Engine::to_f64(cx, buffer_usage_get::<Engine>(cx, buffer).expect("usage"))
                .expect("num"),
            8.0
        );
    }

    fn assert_unmap_detaches_all_mapped_ranges<const COPY: bool>() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<MockEngine<COPY>>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(16.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<MockEngine<COPY>>(cx, device, &[desc]).expect("buffer");
        let native = MockEngine::<COPY>::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<MockEngine<COPY>>>())
            .and_then(|payload| payload.state().lock().ok().map(|state| state.buffer))
            .expect("native buffer");
        let first = buffer_get_mapped_range::<MockEngine<COPY>>(
            cx,
            buffer,
            &[rt.number(0.0), rt.number(4.0)],
        )
        .expect("range");
        let second = buffer_get_mapped_range::<MockEngine<COPY>>(
            cx,
            buffer,
            &[rt.number(4.0), rt.number(4.0)],
        )
        .expect("range");
        assert_eq!(MockEngine::<COPY>::arraybuffer_len(cx, first), Some(4));
        assert_eq!(MockEngine::<COPY>::arraybuffer_len(cx, second), Some(4));
        assert!(rt.write_arraybuffer(first, &[1, 2, 3, 4]));
        assert!(rt.write_arraybuffer(second, &[5, 6, 7, 8]));
        if MockEngine::<COPY>::MAPPED_RANGE_STRATEGY == MappedRangeStrategy::ZeroCopyDetach {
            assert_eq!(
                buffer_bytes(native).expect("bytes")[..8],
                [1, 2, 3, 4, 5, 6, 7, 8],
                "zero-copy mapped ranges must alias backend memory"
            );
        }

        let _ = buffer_unmap::<MockEngine<COPY>>(cx, buffer, &[]).expect("unmap");

        assert_eq!(MockEngine::<COPY>::arraybuffer_len(cx, first), Some(0));
        assert_eq!(MockEngine::<COPY>::arraybuffer_len(cx, second), Some(0));
        assert_eq!(rt.read_arraybuffer(first), None);
        assert_eq!(rt.read_arraybuffer(second), None);
        if MockEngine::<COPY>::MAPPED_RANGE_STRATEGY == MappedRangeStrategy::CopyInCopyOut {
            assert_eq!(
                buffer_bytes(native).expect("bytes")[..8],
                [1, 2, 3, 4, 5, 6, 7, 8]
            );
        }
        assert_eq!(
            buffer_bytes(native).expect("bytes")[..8],
            [1, 2, 3, 4, 5, 6, 7, 8],
            "script writes through mapped ranges must reach native memory"
        );
    }

    #[test]
    fn a15_unmap_detaches_all_mapped_ranges_zero_copy() {
        assert_unmap_detaches_all_mapped_ranges::<false>();
    }

    #[test]
    fn a10_a20_copy_in_copy_out_detaches_and_copies_back() {
        assert_unmap_detaches_all_mapped_ranges::<true>();
    }

    #[test]
    fn r19_a12_guard_fires_when_detach_silently_noops() {
        reset_gpu();
        let rt = runtime();
        rt.set_detach_noop(true);
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(16.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let range =
            buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(0.0), rt.number(4.0)])
                .expect("range");

        let error = buffer_unmap::<Engine>(cx, buffer, &[]).expect_err("unmap must fail");

        assert_eq!(error, "OperationError: mapped range detach failed");
        assert_eq!(Engine::arraybuffer_len(cx, range), Some(4));
    }

    #[test]
    #[should_panic(expected = "mock duplicated values were not released")]
    fn r19_mock_catches_tracked_range_dropped_without_release() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(16.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let _range =
            buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(0.0), rt.number(4.0)])
                .expect("range");
        let payload = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .expect("payload");
        payload.state().lock().expect("state").ranges.clear();
    }

    #[test]
    fn destroy_discards_copy_back_bytes_for_mapped_buffer() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<MockEngine<true>>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(8.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<MockEngine<true>>(cx, device, &[desc]).expect("buffer");
        let native = MockEngine::<true>::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<MockEngine<true>>>())
            .and_then(|payload| payload.state().lock().ok().map(|state| state.buffer))
            .expect("native buffer");
        let range = buffer_get_mapped_range::<MockEngine<true>>(
            cx,
            buffer,
            &[rt.number(0.0), rt.number(4.0)],
        )
        .expect("range");
        assert!(rt.write_arraybuffer(range, &[9, 8, 7, 6]));

        let _ = buffer_destroy::<MockEngine<true>>(cx, buffer, &[]).expect("destroy");

        assert_eq!(MockEngine::<true>::arraybuffer_len(cx, range), Some(0));
        assert_eq!(
            buffer_bytes(native).expect("bytes")[..4],
            [0, 0, 0, 0],
            "destroy detaches ranges but discards script-side mapped bytes"
        );
    }

    #[test]
    fn a21_rejects_offsets_that_would_truncate_on_32_bit_hosts() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(2.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let too_large = rt.number(4_294_967_296.0);

        assert!(
            buffer_map_async::<Engine>(cx, buffer, &[rt.number(2.0), too_large]).is_err(),
            "mapAsync offset=2^32 must be rejected on 64-bit hosts too"
        );
    }

    thread_local! {
        static PENDING_MAP_CALLBACKS: RefCell<Vec<WGPUBufferMapCallbackInfo>> = const { RefCell::new(Vec::new()) };
    }

    unsafe fn pending_buffer_map_async(
        _buffer: WGPUBuffer,
        _mode: WGPUMapMode,
        _offset: usize,
        _size: usize,
        info: WGPUBufferMapCallbackInfo,
    ) -> webgpu_native_js_ffi::native::WGPUFuture {
        PENDING_MAP_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(info));
        webgpu_native_js_ffi::native::WGPUFuture { id: 10 }
    }

    fn resolve_pending_map(index: usize, status: WGPUMapAsyncStatus) {
        let info = PENDING_MAP_CALLBACKS.with(|callbacks| callbacks.borrow_mut().remove(index));
        let callback = info.callback.expect("callback");
        unsafe {
            callback(
                status,
                WGPUStringView::from_bytes(b""),
                info.userdata1,
                info.userdata2,
            );
        }
    }

    #[test]
    fn a7_two_concurrent_map_async_operations_settle_independently() {
        reset_gpu();
        PENDING_MAP_CALLBACKS.with(|callbacks| callbacks.borrow_mut().clear());
        let mut gpu = dispatch();
        gpu.buffer_map_async = pending_buffer_map_async;
        let rt = Runtime::new(gpu);
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc_a = descriptor(&rt, &[("size", rt.number(8.0)), ("usage", rt.number(2.0))]);
        let desc_b = descriptor(&rt, &[("size", rt.number(8.0)), ("usage", rt.number(2.0))]);
        let first_buffer = device_create_buffer::<Engine>(cx, device, &[desc_a]).expect("buffer");
        let second_buffer = device_create_buffer::<Engine>(cx, device, &[desc_b]).expect("buffer");

        let first =
            buffer_map_async::<Engine>(cx, first_buffer, &[rt.number(2.0)]).expect("promise");
        let second =
            buffer_map_async::<Engine>(cx, second_buffer, &[rt.number(2.0)]).expect("promise");

        PENDING_MAP_CALLBACKS.with(|callbacks| assert_eq!(callbacks.borrow().len(), 2));
        resolve_pending_map(1, crate::WGPUMapAsyncStatus_WGPUMapAsyncStatus_Error);
        Engine::environment(cx)
            .settlements()
            .drain::<Engine>(cx)
            .expect("drain settlements");
        assert!(matches!(rt.promise_result(second), Some(Err(_))));
        assert_eq!(rt.promise_result(first), None);
        resolve_pending_map(0, crate::WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success);
        Engine::environment(cx)
            .settlements()
            .drain::<Engine>(cx)
            .expect("drain settlements");
        assert!(matches!(rt.promise_result(first), Some(Ok(_))));
    }

    #[test]
    fn a5_deferred_second_settle_is_ignored() {
        let rt = runtime();
        let cx = rt.context();
        let (promise, deferred) = Engine::new_promise(cx).expect("promise");
        let resolve = deferred.resolve();
        let reject = deferred.reject();
        let first = rt.number(1.0);
        Engine::settle_deferreds(cx, vec![(deferred, Ok(first))]);
        Engine::settle_deferreds(
            cx,
            vec![(Deferred::new(resolve, reject), Err(rt.string("late")))],
        );

        assert_eq!(rt.promise_result(promise), Some(Ok(first)));
    }

    #[test]
    fn r5_r6_finalizers_enqueue_only_and_drain_releases_child_before_parent_ref() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");

        let buffer_payload = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .expect("payload");
        finalize_buffer::<Engine>(
            Box::new(BufferPayload::<Engine> {
                state: Arc::clone(buffer_payload.state()),
                traced_values: Arc::new(crate::TracedValues::new()),
            }),
            Engine::environment(cx),
        );

        let device_payload = Engine::payload(cx, device, crate::GPU_DEVICE_CLASS)
            .and_then(|payload| payload.downcast_ref::<DevicePayload>())
            .expect("payload");
        finalize_device(
            Box::new(DevicePayload {
                device: device_payload.device(),
            }),
            Engine::environment(cx),
        );

        assert_eq!(rt.queue().len().expect("len"), 2);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.device_add_refs, 2);
            assert_eq!(state.buffer_releases, 0);
            assert_eq!(state.device_releases, 0);
        });
        assert_eq!(rt.queue().drain().expect("drain"), 2);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.device_releases, 2);
            assert_eq!(state.buffer_releases, 1);
            let order = &state.native_order;
            assert!(order
                .windows(2)
                .any(|window| window == ["buffer_release", "device_release"]));
        });
    }

    #[test]
    fn r5_asan_guard_parent_ref_outlives_child_release() {
        #[repr(C)]
        struct Parent {
            marker: u64,
        }

        #[repr(C)]
        struct Child {
            parent: *mut Parent,
        }

        unsafe fn parent_release(device: WGPUDevice) {
            let parent = device.cast::<Parent>();
            unsafe {
                (*parent).marker = 0xdead_beef;
                drop(Box::from_raw(parent));
            }
        }

        unsafe fn child_release(buffer: WGPUBuffer) {
            let child = buffer.cast::<Child>();
            unsafe {
                let child = Box::from_raw(child);
                let marker = ptr::read_volatile(ptr::addr_of!((*child.parent).marker));
                assert_eq!(marker, 0xfeed_face);
            }
        }

        let parent = Box::into_raw(Box::new(Parent {
            marker: 0xfeed_face,
        }));
        let child = Box::into_raw(Box::new(Child { parent }));
        let mut gpu = dispatch();
        gpu.device_release = parent_release;
        gpu.buffer_release = child_release;
        let queue = ReleaseQueue::new();
        queue
            .enqueue(crate::ReleaseRequest::BufferWithDeviceRef {
                buffer: child.cast(),
                device: parent.cast(),
                gpu,
            })
            .expect("enqueue");

        assert_eq!(queue.drain().expect("drain"), 1);
    }
}
