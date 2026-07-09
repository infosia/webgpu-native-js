//! Mock JavaScript engine used by core unit tests.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ptr;
use std::sync::Arc;

use crate::{
    Arena, ClassId, ClassSpec, Environment, GpuDispatch, JsEngine, ReleaseQueue, Result,
    WGPUBuffer, WGPUBufferDescriptor, WGPUDevice, WGPUStringView, WGPUStringViewExt,
};

/// Mock JavaScript value handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Value(usize);

/// Mock JavaScript context.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    runtime: &'a Runtime,
    scope: Option<&'a Scope<'a>>,
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
        }
    }

    /// Returns a context handle.
    #[must_use]
    pub fn context(&self) -> Context<'_> {
        Context {
            runtime: self,
            scope: None,
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
            scope: Some(&scope),
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
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let count = self.owned.borrow().len();
        self.runtime
            .reclaimed_values
            .set(self.runtime.reclaimed_values.get() + count);
        self.owned.borrow_mut().clear();
    }
}

#[derive(Clone)]
enum MockValue {
    Undefined,
    Number(f64),
    Bool(bool),
    String(String),
    Object(BTreeMap<String, Value>),
    Instance {
        class: ClassId,
        payload: &'static (dyn Any + Send),
    },
}

/// Mock engine marker type.
pub struct Engine;

impl JsEngine for Engine {
    type Value = Value;
    type Context<'a> = Context<'a>;
    type Error = String;

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
                if let Some(scope) = cx.scope {
                    scope.owned.borrow_mut().push(value);
                }
                Ok(value)
            }
            _ => Ok(cx.runtime.undefined()),
        }
    }

    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(cx.runtime.get(value), MockValue::Undefined)
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> Result<f64, Self::Error> {
        match cx.runtime.get(value) {
            MockValue::Number(value) => Ok(value),
            MockValue::Bool(value) => Ok(if value { 1.0 } else { 0.0 }),
            MockValue::String(value) => value.parse::<f64>().map_err(|_| "number".to_owned()),
            MockValue::Undefined | MockValue::Object(_) | MockValue::Instance { .. } => {
                Err("number".to_owned())
            }
        }
    }

    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool {
        match cx.runtime.get(value) {
            MockValue::Undefined => false,
            MockValue::Bool(value) => value,
            MockValue::Number(value) => value != 0.0 && !value.is_nan(),
            MockValue::String(value) => !value.is_empty(),
            MockValue::Object(_) | MockValue::Instance { .. } => true,
        }
    }

    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a Arena,
    ) -> Result<&'a str, Self::Error> {
        match cx.runtime.get(value) {
            MockValue::Undefined => Ok(arena.alloc_str("undefined")),
            MockValue::Number(value) => Ok(arena.alloc_str(&value.to_string())),
            MockValue::Bool(value) => Ok(arena.alloc_str(if value { "true" } else { "false" })),
            MockValue::String(value) => Ok(arena.alloc_str(&value)),
            MockValue::Object(_) | MockValue::Instance { .. } => {
                Ok(arena.alloc_str("[object Object]"))
            }
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
}

thread_local! {
    static GPU_STATE: RefCell<MockGpuState> = RefCell::new(MockGpuState::default());
}

#[derive(Default)]
struct MockGpuState {
    next: usize,
    device_add_refs: usize,
    device_releases: usize,
    buffer_releases: usize,
    buffer_destroys: usize,
    labels: Vec<Vec<u8>>,
    descriptors: Vec<RecordedDescriptor>,
    null_create_buffer: bool,
    native_order: Vec<&'static str>,
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
        device_add_ref,
        device_release,
        device_create_buffer,
        buffer_set_label,
        buffer_destroy,
        buffer_release,
    }
}

/// Creates a non-null fake device handle.
#[must_use]
pub fn fake_device() -> WGPUDevice {
    ptr::NonNull::<u8>::dangling().as_ptr().cast()
}

fn fake_buffer(id: usize) -> WGPUBuffer {
    id as WGPUBuffer
}

unsafe fn device_add_ref(_device: WGPUDevice) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.device_add_refs += 1;
        state.native_order.push("device_add_ref");
    });
}

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
        fake_buffer(state.next)
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

unsafe fn buffer_release(_buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_releases += 1;
        state.native_order.push("buffer_release");
    });
}

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
        buffer_destroy, buffer_label_get, buffer_label_set, buffer_size_get, buffer_usage_get,
        convert_buffer_descriptor, device_create_buffer, finalize_buffer, finalize_device,
        wrap_device, BufferPayload, DevicePayload, JsEngine,
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
    fn r10_label_bytes_survive_the_create_buffer_call() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
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
            assert_eq!(rt.reclaimed_values(), 0);
        });
        assert_eq!(rt.reclaimed_values(), 4);
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
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
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
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        assert!(device_create_buffer::<Engine>(cx, device, &[desc]).is_err());
    }

    #[test]
    fn r14_destroy_is_idempotent_and_release_is_queued_later() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
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
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
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

    #[test]
    fn r5_r6_finalizers_enqueue_only_and_drain_releases_child_before_parent_ref() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = wrap_device::<Engine>(cx, fake_device()).expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");

        let buffer_payload = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload>())
            .expect("payload");
        finalize_buffer(
            Box::new(BufferPayload {
                state: Arc::clone(buffer_payload.state()),
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

        unsafe fn noop_device(_device: WGPUDevice) {}
        unsafe fn noop_create(
            _device: WGPUDevice,
            _descriptor: *const WGPUBufferDescriptor,
        ) -> WGPUBuffer {
            ptr::null_mut()
        }
        unsafe fn noop_label(_buffer: WGPUBuffer, _label: WGPUStringView) {}
        unsafe fn noop_destroy(_buffer: WGPUBuffer) {}

        let parent = Box::into_raw(Box::new(Parent {
            marker: 0xfeed_face,
        }));
        let child = Box::into_raw(Box::new(Child { parent }));
        let queue = ReleaseQueue::new();
        queue
            .enqueue(crate::ReleaseRequest::BufferWithDeviceRef {
                buffer: child.cast(),
                device: parent.cast(),
                gpu: GpuDispatch {
                    device_add_ref: noop_device,
                    device_release: parent_release,
                    device_create_buffer: noop_create,
                    buffer_set_label: noop_label,
                    buffer_destroy: noop_destroy,
                    buffer_release: child_release,
                },
            })
            .expect("enqueue");

        assert_eq!(queue.drain().expect("drain"), 1);
    }
}
