//! Mock JavaScript engine used by core unit tests.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;

use crate::{
    Arena, ClassId, ClassSpec, Deferred, Environment, GpuDispatch, JsEngine, MappedRangeStrategy,
    ReleaseQueue, Result, WGPUAdapter, WGPUAdapterInfo, WGPUBuffer, WGPUBufferDescriptor,
    WGPUBufferMapCallbackInfo, WGPUDevice, WGPULimits, WGPUMapAsyncStatus, WGPUMapMode,
    WGPURequestAdapterCallbackInfo, WGPURequestDeviceCallbackInfo, WGPUStatus, WGPUStringView,
    WGPUStringViewExt, WGPUSupportedFeatures,
};
use crate::{
    WGPUBindGroup, WGPUBindGroupDescriptor, WGPUBindGroupLayout, WGPUBindGroupLayoutDescriptor,
    WGPUCommandBuffer, WGPUCommandBufferDescriptor, WGPUCommandEncoder,
    WGPUCommandEncoderDescriptor, WGPUComputePassDescriptor, WGPUComputePassEncoder,
    WGPUComputePipeline, WGPUComputePipelineDescriptor, WGPUCreateComputePipelineAsyncCallbackInfo,
    WGPUCreateRenderPipelineAsyncCallbackInfo, WGPUDeviceLostCallbackInfo, WGPUErrorFilter,
    WGPUErrorType, WGPUExtent3D, WGPUFuture, WGPUIndexFormat, WGPUPipelineLayout,
    WGPUPipelineLayoutDescriptor, WGPUPopErrorScopeCallbackInfo, WGPUPopErrorScopeStatus,
    WGPUQuerySet, WGPUQuerySetDescriptor, WGPUQueryType, WGPUQueue, WGPUQueueWorkDoneCallbackInfo,
    WGPURenderBundle, WGPURenderBundleDescriptor, WGPURenderBundleEncoder,
    WGPURenderBundleEncoderDescriptor, WGPURenderPassDescriptor, WGPURenderPassEncoder,
    WGPURenderPipeline, WGPURenderPipelineDescriptor, WGPUSampler, WGPUSamplerDescriptor,
    WGPUShaderModule, WGPUShaderModuleDescriptor, WGPUTexelCopyBufferInfo,
    WGPUTexelCopyBufferLayout, WGPUTexelCopyTextureInfo, WGPUTexture, WGPUTextureDescriptor,
    WGPUTextureDimension, WGPUTextureFormat, WGPUTextureUsage, WGPUTextureView,
    WGPUTextureViewDescriptor, WGPUUncapturedErrorCallbackInfo,
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
    global: Value,
    symbol_iterator: Value,
    classes: RefCell<BTreeMap<ClassId, &'static str>>,
    reclaimed_values: Cell<usize>,
    reclaimed_handles: RefCell<Vec<Value>>,
    detach_noop: Cell<bool>,
    detach_noop_value: Cell<Option<Value>>,
    duplicated_values: RefCell<BTreeMap<Value, usize>>,
    property_errors: RefCell<BTreeMap<(Value, String), String>>,
    property_value_errors: RefCell<BTreeMap<(Value, Value), String>>,
    call_errors: RefCell<BTreeMap<Value, String>>,
    construct_errors: RefCell<BTreeMap<Value, String>>,
    property_value_calls: Cell<usize>,
    calls: Cell<usize>,
    constructs: Cell<usize>,
    call_args: RefCell<Vec<Value>>,
    call_history: RefCell<Vec<Vec<Value>>>,
    construct_history: RefCell<Vec<Vec<Value>>>,
    coercion_unmap: Cell<Option<Value>>,
    iterator_end_pass: Cell<Option<Value>>,
    settle_calls: Cell<usize>,
    settlement_batch_sizes: RefCell<Vec<usize>>,
    settlement_attempts: RefCell<BTreeMap<Value, usize>>,
    held_returns: Cell<usize>,
    fail_new_instance: Cell<Option<ClassId>>,
}

impl Runtime {
    /// Creates a mock runtime with the provided WebGPU dispatch.
    #[must_use]
    pub fn new(gpu: GpuDispatch) -> Self {
        let symbol_iterator = Value(1);
        let symbol = Value(2);
        let array_constructor = Value(3);
        let set_constructor = Value(4);
        let boolean = Value(5);
        let global = Value(6);
        let mut symbol_properties = BTreeMap::new();
        symbol_properties.insert("iterator".to_owned(), symbol_iterator);
        let mut global_properties = BTreeMap::new();
        global_properties.insert("Symbol".to_owned(), symbol);
        global_properties.insert("Array".to_owned(), array_constructor);
        global_properties.insert("Set".to_owned(), set_constructor);
        global_properties.insert("Boolean".to_owned(), boolean);
        Self {
            env: Environment::new(gpu, Arc::new(ReleaseQueue::new())),
            values: RefCell::new(vec![
                MockValue::Undefined,
                MockValue::SymbolIterator,
                MockValue::Object(symbol_properties),
                MockValue::Constructor(MockConstructor::Array),
                MockValue::Constructor(MockConstructor::Set),
                MockValue::Callable(MockCallable::Boolean),
                MockValue::Object(global_properties),
            ]),
            global,
            symbol_iterator,
            classes: RefCell::new(BTreeMap::new()),
            reclaimed_values: Cell::new(0),
            reclaimed_handles: RefCell::new(Vec::new()),
            detach_noop: Cell::new(false),
            detach_noop_value: Cell::new(None),
            duplicated_values: RefCell::new(BTreeMap::new()),
            property_errors: RefCell::new(BTreeMap::new()),
            property_value_errors: RefCell::new(BTreeMap::new()),
            call_errors: RefCell::new(BTreeMap::new()),
            construct_errors: RefCell::new(BTreeMap::new()),
            property_value_calls: Cell::new(0),
            calls: Cell::new(0),
            constructs: Cell::new(0),
            call_args: RefCell::new(Vec::new()),
            call_history: RefCell::new(Vec::new()),
            construct_history: RefCell::new(Vec::new()),
            coercion_unmap: Cell::new(None),
            iterator_end_pass: Cell::new(None),
            settle_calls: Cell::new(0),
            settlement_batch_sizes: RefCell::new(Vec::new()),
            settlement_attempts: RefCell::new(BTreeMap::new()),
            held_returns: Cell::new(0),
            fail_new_instance: Cell::new(None),
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

    fn set_property_error(&self, object: Value, property: &str, error: &str) {
        self.property_errors
            .borrow_mut()
            .insert((object, property.to_owned()), error.to_owned());
    }

    fn set_property_value_error(&self, object: Value, key: Value, error: &str) {
        self.property_value_errors.borrow_mut().insert(
            (self.canonical(object), self.canonical(key)),
            error.to_owned(),
        );
    }

    fn set_call_error(&self, callable: Value, error: &str) {
        self.call_errors
            .borrow_mut()
            .insert(self.canonical(callable), error.to_owned());
    }

    fn set_construct_error(&self, constructor: Value, error: &str) {
        self.construct_errors
            .borrow_mut()
            .insert(self.canonical(constructor), error.to_owned());
    }

    fn reenter_unmap_on_next_coercion(&self, buffer: Value) {
        self.coercion_unmap.set(Some(buffer));
    }

    fn end_pass_on_next_iteration(&self, pass: Value) {
        self.iterator_end_pass.set(Some(pass));
    }

    fn live_scoped_values(&self) -> usize {
        self.values
            .borrow()
            .iter()
            .filter(|value| matches!(value, MockValue::Scoped(_)))
            .count()
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

    fn set_like(&self, values: &[Value]) -> Value {
        self.iterable(values, None)
    }

    fn sparse_array(&self, length: usize, entries: &[(usize, Value)]) -> Value {
        let mut values = vec![self.undefined(); length];
        for &(index, value) in entries {
            values[index] = value;
        }
        self.iterable(&values, None)
    }

    fn throwing_iterable(&self, values: &[Value], throw_on_next: usize) -> Value {
        self.iterable(values, Some(throw_on_next))
    }

    fn iterable(&self, values: &[Value], throw_on_next: Option<usize>) -> Value {
        let iterator_method = self.insert(MockValue::Callable(MockCallable::Iterator));
        self.insert(MockValue::Iterable {
            values: values.to_vec(),
            iterator_method,
            throw_on_next,
        })
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
                ..
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

    /// Makes detach silently no-op for one selected ArrayBuffer.
    pub fn set_detach_noop_for(&self, value: Value) {
        self.detach_noop_value.set(Some(self.canonical(value)));
    }

    /// Reads a copy of an ArrayBuffer's bytes while it is attached.
    #[must_use]
    pub fn read_arraybuffer(&self, value: Value) -> Option<Vec<u8>> {
        self.with_value(self.canonical(value), |stored| match stored {
            MockValue::ArrayBuffer {
                bytes,
                detached: false,
            } => Some(bytes.clone()),
            MockValue::ExternalArrayBuffer {
                ptr,
                len,
                detached: false,
                ..
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

    fn tracked_alias(&self, scope: &Scope<'_>, value: Value) -> Value {
        let owned = self.insert(MockValue::Scoped(self.canonical(value)));
        scope.owned.borrow_mut().push(owned);
        owned
    }

    fn get(&self, value: Value) -> MockValue {
        let current = self.canonical(value);
        match self
            .values
            .borrow()
            .get(current.0)
            .cloned()
            .unwrap_or(MockValue::Undefined)
        {
            MockValue::Reclaimed => MockValue::Undefined,
            value => value,
        }
    }

    fn canonical(&self, value: Value) -> Value {
        let mut current = value;
        loop {
            let stored = self
                .values
                .borrow()
                .get(current.0)
                .cloned()
                .unwrap_or(MockValue::Undefined);
            match stored {
                MockValue::Scoped(inner) => current = inner,
                _ => return current,
            }
        }
    }

    fn with_value<R>(&self, value: Value, f: impl FnOnce(&mut MockValue) -> R) -> Option<R> {
        self.values.borrow_mut().get_mut(value.0).map(f)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.with_scope(|cx| self.env.release_device_event_values::<Engine>(cx));
            let duplicated = self.duplicated_values.borrow();
            assert!(
                duplicated.is_empty(),
                "mock duplicated values were not released: {duplicated:?}"
            );
        }
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let mut owned = self.owned.borrow_mut();
        let count = owned.len();
        for value in owned.iter().copied() {
            assert!(
                value.0 < self.runtime.values.borrow().len(),
                "mock scope owned a value that the runtime never issued: {value:?}"
            );
            let reclaimed = self.runtime.with_value(value, |stored| {
                assert!(
                    matches!(stored, MockValue::Scoped(_)),
                    "mock scope value was not an owned engine result: {value:?}"
                );
                *stored = MockValue::Reclaimed;
            });
            assert!(
                reclaimed.is_some(),
                "mock scope failed to reclaim {value:?}"
            );
        }
        self.runtime
            .reclaimed_values
            .set(self.runtime.reclaimed_values.get() + count);
        self.runtime
            .reclaimed_handles
            .borrow_mut()
            .extend(owned.iter().copied());
        owned.clear();
        assert!(
            owned.is_empty(),
            "mock scope did not reclaim every owned value"
        );
    }
}

#[derive(Clone)]
enum MockValue {
    Undefined,
    Scoped(Value),
    Reclaimed,
    SymbolIterator,
    Null,
    Number(f64),
    Bool(bool),
    String(String),
    Object(BTreeMap<String, Value>),
    Iterable {
        values: Vec<Value>,
        iterator_method: Value,
        throw_on_next: Option<usize>,
    },
    Iterator {
        values: Vec<Value>,
        next_method: Value,
        next_index: Rc<Cell<usize>>,
        throw_on_next: Option<usize>,
    },
    Callable(MockCallable),
    Constructor(MockConstructor),
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
        owner: WGPUBuffer,
        detached: bool,
    },
    Instance {
        class: ClassId,
        payload: &'static (dyn Any + Send),
    },
}

#[derive(Clone, Copy)]
enum MockCallable {
    Iterator,
    Next,
    Handler,
    Boolean,
}

#[derive(Clone, Copy)]
enum MockConstructor {
    Array,
    Set,
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
        if let Some(error) = cx
            .runtime
            .property_errors
            .borrow()
            .get(&(cx.runtime.canonical(obj), key.to_owned()))
            .cloned()
        {
            return Err(error);
        }
        let value = match cx.runtime.get(obj) {
            MockValue::Object(map) => map
                .get(key)
                .copied()
                .unwrap_or_else(|| cx.runtime.undefined()),
            MockValue::Iterator { next_method, .. } if key == "next" => next_method,
            _ => cx.runtime.undefined(),
        };
        Ok(cx.runtime.tracked_alias(cx.scope, value))
    }

    fn own_property_names(
        cx: Self::Context<'_>,
        obj: Self::Value,
    ) -> Result<Vec<String>, Self::Error> {
        match cx.runtime.get(obj) {
            MockValue::Object(map) => Ok(map.keys().cloned().collect()),
            _ => Ok(Vec::new()),
        }
    }

    fn global(cx: Self::Context<'_>) -> Self::Value {
        cx.runtime.tracked_alias(cx.scope, cx.runtime.global)
    }

    fn get_property_value(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: Self::Value,
    ) -> Result<Self::Value, Self::Error> {
        cx.runtime
            .property_value_calls
            .set(cx.runtime.property_value_calls.get() + 1);
        let obj = cx.runtime.canonical(obj);
        let key = cx.runtime.canonical(key);
        if let Some(error) = cx
            .runtime
            .property_value_errors
            .borrow()
            .get(&(obj, key))
            .cloned()
        {
            return Err(error);
        }
        let value = match cx.runtime.get(obj) {
            MockValue::Iterable {
                iterator_method, ..
            } if key == cx.runtime.symbol_iterator => iterator_method,
            _ => cx.runtime.undefined(),
        };
        Ok(cx.runtime.tracked_alias(cx.scope, value))
    }

    fn call(
        cx: Self::Context<'_>,
        f: Self::Value,
        this: Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error> {
        cx.runtime.calls.set(cx.runtime.calls.get() + 1);
        let args = args
            .iter()
            .map(|value| cx.runtime.canonical(*value))
            .collect::<Vec<_>>();
        *cx.runtime.call_args.borrow_mut() = args.clone();
        cx.runtime.call_history.borrow_mut().push(args.clone());
        let f = cx.runtime.canonical(f);
        if let Some(error) = cx.runtime.call_errors.borrow().get(&f).cloned() {
            return Err(error);
        }
        let result = match cx.runtime.get(f) {
            MockValue::Callable(MockCallable::Iterator) => {
                let MockValue::Iterable {
                    values,
                    throw_on_next,
                    ..
                } = cx.runtime.get(this)
                else {
                    return Err("TypeError: invalid iterator receiver".to_owned());
                };
                let next_method = cx.runtime.insert(MockValue::Callable(MockCallable::Next));
                cx.runtime.insert(MockValue::Iterator {
                    values,
                    next_method,
                    next_index: Rc::new(Cell::new(0)),
                    throw_on_next,
                })
            }
            MockValue::Callable(MockCallable::Next) => {
                let MockValue::Iterator {
                    values,
                    next_index,
                    throw_on_next,
                    ..
                } = cx.runtime.get(this)
                else {
                    return Err("TypeError: invalid next receiver".to_owned());
                };
                let index = next_index.get();
                if let Some(pass) = cx.runtime.iterator_end_pass.take() {
                    crate::render_pass_end::<Self>(cx, pass, &[])?;
                }
                if throw_on_next == Some(index) {
                    return Err(format!("iterator next {index} failed"));
                }
                let (done, value) = if let Some(value) = values.get(index).copied() {
                    next_index.set(index + 1);
                    (false, value)
                } else {
                    (true, cx.runtime.undefined())
                };
                let done = cx.runtime.bool(done);
                cx.runtime.object(&[("done", done), ("value", value)])
            }
            MockValue::Callable(MockCallable::Handler) => cx.runtime.undefined(),
            MockValue::Callable(MockCallable::Boolean) => cx.runtime.bool(
                args.first()
                    .is_some_and(|value| match cx.runtime.get(*value) {
                        MockValue::Undefined | MockValue::Null => false,
                        MockValue::Bool(value) => value,
                        MockValue::Number(value) => value != 0.0 && !value.is_nan(),
                        MockValue::String(value) => !value.is_empty(),
                        _ => true,
                    }),
            ),
            _ => return Err("TypeError: value is not callable".to_owned()),
        };
        Ok(cx.runtime.tracked_alias(cx.scope, result))
    }

    fn construct(
        cx: Self::Context<'_>,
        ctor: Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error> {
        cx.runtime.constructs.set(cx.runtime.constructs.get() + 1);
        let args = args
            .iter()
            .map(|value| cx.runtime.canonical(*value))
            .collect::<Vec<_>>();
        cx.runtime.construct_history.borrow_mut().push(args.clone());
        let ctor = cx.runtime.canonical(ctor);
        if let Some(error) = cx.runtime.construct_errors.borrow().get(&ctor).cloned() {
            return Err(error);
        }
        let result = match cx.runtime.get(ctor) {
            MockValue::Constructor(MockConstructor::Array) => cx.runtime.iterable(&args, None),
            MockValue::Constructor(MockConstructor::Set) => {
                let values = match args.first().map(|value| cx.runtime.get(*value)) {
                    Some(MockValue::Iterable { values, .. }) => values,
                    None | Some(MockValue::Undefined) => Vec::new(),
                    _ => return Err("TypeError: Set argument is not iterable".to_owned()),
                };
                let mut unique = Vec::new();
                for value in values {
                    if !unique.contains(&value) {
                        unique.push(value);
                    }
                }
                cx.runtime.iterable(&unique, None)
            }
            _ => return Err("TypeError: value is not a constructor".to_owned()),
        };
        Ok(cx.runtime.tracked_alias(cx.scope, result))
    }

    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(cx.runtime.get(value), MockValue::Undefined)
    }

    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(cx.runtime.get(value), MockValue::Null)
    }

    fn is_object(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(
            cx.runtime.get(value),
            MockValue::Object(_)
                | MockValue::Iterable { .. }
                | MockValue::Iterator { .. }
                | MockValue::Callable(_)
                | MockValue::Constructor(_)
                | MockValue::Promise { .. }
                | MockValue::Resolver { .. }
                | MockValue::ArrayBuffer { .. }
                | MockValue::ExternalArrayBuffer { .. }
                | MockValue::Instance { .. }
        )
    }

    fn is_callable(cx: Self::Context<'_>, value: Self::Value) -> bool {
        matches!(
            cx.runtime.get(value),
            MockValue::Callable(_) | MockValue::Resolver { .. }
        )
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> Result<f64, Self::Error> {
        if let Some(buffer) = cx.runtime.coercion_unmap.take() {
            let _ = crate::buffer_unmap::<Self>(cx, buffer, &[])?;
        }
        match cx.runtime.get(value) {
            MockValue::Number(value) => Ok(value),
            MockValue::Bool(value) => Ok(if value { 1.0 } else { 0.0 }),
            MockValue::String(value) => value.parse::<f64>().map_err(|_| "number".to_owned()),
            MockValue::Undefined
            | MockValue::Scoped(_)
            | MockValue::Reclaimed
            | MockValue::SymbolIterator
            | MockValue::Null
            | MockValue::Object(_)
            | MockValue::Iterable { .. }
            | MockValue::Iterator { .. }
            | MockValue::Callable(_)
            | MockValue::Constructor(_)
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
            MockValue::Scoped(_) | MockValue::Reclaimed => false,
            MockValue::SymbolIterator => true,
            MockValue::Null => false,
            MockValue::Bool(value) => value,
            MockValue::Number(value) => value != 0.0 && !value.is_nan(),
            MockValue::String(value) => !value.is_empty(),
            MockValue::Object(_)
            | MockValue::Iterable { .. }
            | MockValue::Iterator { .. }
            | MockValue::Callable(_)
            | MockValue::Constructor(_)
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
            MockValue::Scoped(_) | MockValue::Reclaimed => Ok(arena.alloc_str("undefined")),
            MockValue::SymbolIterator => Ok(arena.alloc_str("Symbol(Symbol.iterator)")),
            MockValue::Null => Ok(arena.alloc_str("null")),
            MockValue::Number(value) => Ok(arena.alloc_str(&value.to_string())),
            MockValue::Bool(value) => Ok(arena.alloc_str(if value { "true" } else { "false" })),
            MockValue::String(value) => Ok(arena.alloc_str(&value)),
            MockValue::Object(_)
            | MockValue::Iterable { .. }
            | MockValue::Iterator { .. }
            | MockValue::Callable(_)
            | MockValue::Constructor(_)
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
        if cx.runtime.fail_new_instance.take() == Some(class) {
            return Err("new instance failed".to_owned());
        }
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

    fn null(cx: Self::Context<'_>) -> Self::Value {
        cx.runtime.null()
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

    fn range_error(_cx: Self::Context<'_>, message: &str) -> Self::Error {
        format!("RangeError: {message}")
    }

    fn async_error_value(cx: Self::Context<'_>, name: &str, message: &str) -> Self::Value {
        let name = cx.runtime.string(name);
        let message = cx.runtime.string(message);
        cx.runtime.object(&[("name", name), ("message", message)])
    }

    fn error_value_from_error(cx: Self::Context<'_>, error: Self::Error) -> Self::Value {
        if let Some((name, message)) = error.split_once(": ") {
            return Self::async_error_value(cx, name, message);
        }
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

    fn settle_deferreds(
        cx: Self::Context<'_>,
        settlements: Vec<crate::DeferredSettlement<Self>>,
    ) -> Result<(), Self::Error> {
        cx.runtime
            .settle_calls
            .set(cx.runtime.settle_calls.get() + 1);
        cx.runtime
            .settlement_batch_sizes
            .borrow_mut()
            .push(settlements.len());
        GPU_STATE.with(|state| state.borrow_mut().native_order.push("settle"));
        for (deferred, result) in settlements {
            let runtime = cx.runtime;
            let resolver = match result {
                Ok(_) => deferred.resolve(),
                Err(_) => deferred.reject(),
            };
            let MockValue::Resolver { promise, resolve } = runtime.get(resolver) else {
                continue;
            };
            *runtime
                .settlement_attempts
                .borrow_mut()
                .entry(promise)
                .or_default() += 1;
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
        Ok(())
    }

    fn drain_microtasks(_cx: Self::Context<'_>) -> Result<(), Self::Error> {
        GPU_STATE.with(|state| state.borrow_mut().native_order.push("microtasks"));
        Ok(())
    }

    unsafe fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        ptr: *mut u8,
        len: usize,
        owner: WGPUBuffer,
    ) -> Result<Self::Value, Self::Error> {
        Ok(cx.runtime.insert(MockValue::ExternalArrayBuffer {
            ptr,
            len,
            owner,
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
        if cx.runtime.detach_noop.get()
            || cx.runtime.detach_noop_value.get() == Some(cx.runtime.canonical(value))
        {
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
                MockValue::ExternalArrayBuffer {
                    owner, detached, ..
                } => {
                    if !*detached {
                        let _ = cx
                            .runtime
                            .env
                            .queue()
                            .enqueue(crate::ReleaseRequest::Buffer {
                                buffer: *owner,
                                gpu: cx.runtime.env.gpu(),
                            });
                    }
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
        let value = cx.runtime.canonical(value);
        let mut duplicated = cx.runtime.duplicated_values.borrow_mut();
        *duplicated.entry(value).or_insert(0) += 1;
        value
    }

    fn return_held_value(cx: Self::Context<'_>, held: Self::Value) -> Self::Value {
        cx.runtime
            .held_returns
            .set(cx.runtime.held_returns.get() + 1);
        held
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
    static TEST_RELEASE_ORDER: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct MockGpuState {
    next: usize,
    adapter_releases: usize,
    device_add_refs: usize,
    device_get_queue_calls: usize,
    buffer_add_refs: usize,
    queue_add_refs: usize,
    shader_module_add_refs: usize,
    sampler_add_refs: usize,
    texture_add_refs: usize,
    texture_view_add_refs: usize,
    bind_group_layout_add_refs: usize,
    pipeline_layout_add_refs: usize,
    device_releases: usize,
    device_destroys: usize,
    buffer_releases: usize,
    queue_releases: usize,
    shader_module_releases: usize,
    sampler_releases: usize,
    texture_releases: usize,
    texture_view_releases: usize,
    bind_group_layout_releases: usize,
    pipeline_layout_releases: usize,
    bind_group_releases: usize,
    compute_pipeline_releases: usize,
    compute_pipeline_add_refs: usize,
    render_pipeline_releases: usize,
    render_pipeline_add_refs: usize,
    supported_features_free_members: usize,
    adapter_info_free_members: usize,
    limits_fail: bool,
    limits_oversized: bool,
    command_encoder_releases: usize,
    command_buffer_releases: usize,
    compute_pass_encoder_releases: usize,
    render_pass_encoder_releases: usize,
    render_bundle_encoder_releases: usize,
    render_bundle_releases: usize,
    query_set_add_refs: usize,
    query_set_releases: usize,
    query_set_destroys: usize,
    buffer_destroys: usize,
    texture_destroys: usize,
    buffer_unmaps: usize,
    mapped_range_calls: usize,
    const_mapped_range_calls: usize,
    labels: Vec<Vec<u8>>,
    descriptors: Vec<RecordedDescriptor>,
    sampler_descriptors: Vec<RecordedSamplerDescriptor>,
    texture_descriptors: Vec<RecordedTextureDescriptor>,
    texture_view_descriptors: Vec<RecordedTextureViewDescriptor>,
    null_texture_view_descriptors: usize,
    query_set_descriptors: Vec<RecordedQuerySetDescriptor>,
    render_bundle_encoder_descriptors: Vec<RecordedRenderBundleEncoderDescriptor>,
    pipeline_constants: Vec<(&'static str, RecordedConstants)>,
    query_sets: BTreeMap<WGPUQuerySet, RecordedQuerySetDescriptor>,
    textures: BTreeMap<WGPUTexture, RecordedTextureDescriptor>,
    null_create_buffer: bool,
    null_create_sampler: bool,
    native_order: Vec<&'static str>,
    buffers: BTreeMap<WGPUBuffer, Vec<u8>>,
    mapped_ranges: BTreeMap<WGPUBuffer, Vec<MockMappedRange>>,
    error_scope_stack: Vec<WGPUErrorFilter>,
    pushed_error_filters: Vec<WGPUErrorFilter>,
    next_pop_error: Option<MockPopError>,
    next_pipeline_async_error: Option<(crate::WGPUCreatePipelineAsyncStatus, String)>,
    device_lost_callback: Option<WGPUDeviceLostCallbackInfo>,
    uncaptured_error_callback: Option<WGPUUncapturedErrorCallbackInfo>,
    requested_features: Vec<Vec<crate::WGPUFeatureName>>,
    requested_limits: Vec<Option<(WGPULimits, crate::WGPUCompatibilityModeLimits)>>,
    recording_calls: BTreeMap<&'static str, usize>,
    vertex_buffer_ranges: Vec<(u64, u64)>,
    index_buffer_ranges: Vec<(u64, u64)>,
    indirect_calls: Vec<(&'static str, WGPUBuffer, u64)>,
    encoder_retained_indirect_buffers: Vec<WGPUBuffer>,
    command_buffer_retained_indirect_buffers: Vec<WGPUBuffer>,
    released_indirect_buffers: Vec<WGPUBuffer>,
    bundle_encoder_retained_indirect_buffers: Vec<WGPUBuffer>,
    render_bundle_retained_indirect_buffers: Vec<WGPUBuffer>,
    released_bundle_indirect_buffers: Vec<WGPUBuffer>,
}

struct MockPopError {
    status: WGPUPopErrorScopeStatus,
    type_: WGPUErrorType,
    message: String,
}

type RecordedConstants = Vec<(Vec<u8>, f64)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MockMappedRange {
    offset: usize,
    end: usize,
    is_const: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedDescriptor {
    size: u64,
    usage: u64,
    mapped: u32,
    label: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
struct RecordedSamplerDescriptor {
    label: Vec<u8>,
    address_mode_u: crate::WGPUAddressMode,
    address_mode_v: crate::WGPUAddressMode,
    address_mode_w: crate::WGPUAddressMode,
    mag_filter: crate::WGPUFilterMode,
    min_filter: crate::WGPUFilterMode,
    mipmap_filter: crate::WGPUMipmapFilterMode,
    lod_min_clamp: f32,
    lod_max_clamp: f32,
    compare: crate::WGPUCompareFunction,
    max_anisotropy: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedTextureDescriptor {
    width: u32,
    height: u32,
    depth_or_array_layers: u32,
    mip_level_count: u32,
    sample_count: u32,
    dimension: WGPUTextureDimension,
    format: WGPUTextureFormat,
    usage: WGPUTextureUsage,
    view_formats: Vec<WGPUTextureFormat>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedTextureViewDescriptor {
    texture: WGPUTexture,
    format: WGPUTextureFormat,
    dimension: crate::WGPUTextureViewDimension,
    usage: WGPUTextureUsage,
    aspect: crate::WGPUTextureAspect,
    base_mip_level: u32,
    mip_level_count: u32,
    base_array_layer: u32,
    array_layer_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedQuerySetDescriptor {
    label: Vec<u8>,
    type_: WGPUQueryType,
    count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordedRenderBundleEncoderDescriptor {
    label: Vec<u8>,
    color_formats: Vec<WGPUTextureFormat>,
    depth_stencil_format: WGPUTextureFormat,
    sample_count: u32,
    depth_read_only: u32,
    stencil_read_only: u32,
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

// Mock dispatch functions deliberately use the generated field names. Keeping this
// plain naming convention lets one small callback build the complete table.
macro_rules! mock_gpu_dispatch {
    ($(($field:ident, $symbol:ident, unsafe fn($($argument:ident: $argument_type:ty),*) $(-> $result:ty)?),)*) => {
        GpuDispatch { $($field),* }
    };
}

/// Returns mock WebGPU dispatch functions.
#[must_use]
pub fn dispatch() -> GpuDispatch {
    for_each_gpu_dispatch_entry!(mock_gpu_dispatch)
}

unsafe fn instance_process_events(_instance: crate::WGPUInstance) {
    GPU_STATE.with(|state| state.borrow_mut().native_order.push("process_events"));
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
    descriptor: *const webgpu_native_js_ffi::native::WGPUDeviceDescriptor,
    info: WGPURequestDeviceCallbackInfo,
) -> webgpu_native_js_ffi::native::WGPUFuture {
    if let Some(descriptor) = unsafe { descriptor.as_ref() } {
        GPU_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.device_lost_callback = Some(descriptor.deviceLostCallbackInfo);
            state.uncaptured_error_callback = Some(descriptor.uncapturedErrorCallbackInfo);
            let features = if descriptor.requiredFeatureCount == 0 {
                Vec::new()
            } else {
                // SAFETY: requestDevice keeps the arena-backed feature array live through this call.
                unsafe {
                    std::slice::from_raw_parts(
                        descriptor.requiredFeatures,
                        descriptor.requiredFeatureCount,
                    )
                    .to_vec()
                }
            };
            let limits = unsafe { descriptor.requiredLimits.as_ref() }.map(|limits| {
                let compatibility = unsafe {
                    limits
                        .nextInChain
                        .cast::<crate::WGPUCompatibilityModeLimits>()
                        .as_ref()
                        .expect("required-limits compatibility chain")
                };
                (*limits, *compatibility)
            });
            state.requested_features.push(features);
            state.requested_limits.push(limits);
        });
    }
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

unsafe fn adapter_release(_adapter: WGPUAdapter) {
    GPU_STATE.with(|state| state.borrow_mut().adapter_releases += 1);
}

unsafe fn device_release(_device: WGPUDevice) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.device_releases += 1;
        state.native_order.push("device_release");
    });
}

unsafe fn device_destroy(_device: WGPUDevice) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.device_destroys += 1;
        state.native_order.push("device_destroy");
    });
}

unsafe fn ordered_device_release(device: WGPUDevice) {
    TEST_RELEASE_ORDER.with(|order| order.borrow_mut().push(device as usize));
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
    GPU_STATE.with(|state| {
        state.borrow_mut().device_get_queue_calls += 1;
        fake_handle(1001)
    })
}

static MOCK_FEATURES: [crate::WGPUFeatureName; 3] = [
    crate::WGPUFeatureName_WGPUFeatureName_TimestampQuery,
    crate::WGPUFeatureName_WGPUFeatureName_DepthClipControl,
    crate::WGPUFeatureName_WGPUFeatureName_SubgroupSizeControl,
];

unsafe fn adapter_get_features(_adapter: WGPUAdapter, features: *mut WGPUSupportedFeatures) {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_features(features) };
}

unsafe fn device_get_features(_device: WGPUDevice, features: *mut WGPUSupportedFeatures) {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_features(features) };
}

unsafe fn write_mock_features(features: *mut WGPUSupportedFeatures) {
    // SAFETY: callers provide either null or a live writable out-struct.
    if let Some(features) = unsafe { features.as_mut() } {
        features.featureCount = MOCK_FEATURES.len();
        features.features = MOCK_FEATURES.as_ptr();
    }
}

unsafe fn supported_features_free_members(_features: WGPUSupportedFeatures) {
    GPU_STATE.with(|state| state.borrow_mut().supported_features_free_members += 1);
}

unsafe fn adapter_get_limits(_adapter: WGPUAdapter, limits: *mut WGPULimits) -> WGPUStatus {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_limits(limits) }
}

unsafe fn device_get_limits(_device: WGPUDevice, limits: *mut WGPULimits) -> WGPUStatus {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_limits(limits) }
}

unsafe fn write_mock_limits(limits: *mut WGPULimits) -> WGPUStatus {
    if GPU_STATE.with(|state| state.borrow().limits_fail) {
        return crate::WGPUStatus_WGPUStatus_Error;
    }
    // SAFETY: callers provide either null or a live writable out-struct.
    let Some(limits) = (unsafe { limits.as_mut() }) else {
        return crate::WGPUStatus_WGPUStatus_Error;
    };
    limits.maxTextureDimension1D = 1;
    limits.maxTextureDimension2D = 2;
    limits.maxTextureDimension3D = 3;
    limits.maxTextureArrayLayers = 4;
    limits.maxBindGroups = 5;
    limits.maxBindGroupsPlusVertexBuffers = 6;
    limits.maxImmediateSize = 7;
    limits.maxBindingsPerBindGroup = 8;
    limits.maxDynamicUniformBuffersPerPipelineLayout = 9;
    limits.maxDynamicStorageBuffersPerPipelineLayout = 10;
    limits.maxSampledTexturesPerShaderStage = 11;
    limits.maxSamplersPerShaderStage = 12;
    limits.maxStorageBuffersPerShaderStage = 13;
    limits.maxStorageTexturesPerShaderStage = 14;
    limits.maxUniformBuffersPerShaderStage = 15;
    limits.maxUniformBufferBindingSize = 16;
    limits.maxStorageBufferBindingSize = 17;
    limits.minUniformBufferOffsetAlignment = 256;
    limits.minStorageBufferOffsetAlignment = 19;
    limits.maxVertexBuffers = 20;
    limits.maxBufferSize = 21;
    limits.maxVertexAttributes = 22;
    limits.maxVertexBufferArrayStride = 23;
    limits.maxInterStageShaderVariables = 24;
    limits.maxColorAttachments = 25;
    limits.maxColorAttachmentBytesPerSample = 26;
    limits.maxComputeWorkgroupStorageSize = 27;
    limits.maxComputeInvocationsPerWorkgroup = 28;
    limits.maxComputeWorkgroupSizeX = 29;
    limits.maxComputeWorkgroupSizeY = 30;
    limits.maxComputeWorkgroupSizeZ = 31;
    limits.maxComputeWorkgroupsPerDimension = 32;
    if GPU_STATE.with(|state| state.borrow().limits_oversized) {
        limits.maxBufferSize = 9_007_199_254_740_992;
    }
    if !limits.nextInChain.is_null() {
        let compatibility = limits
            .nextInChain
            .cast::<crate::WGPUCompatibilityModeLimits>();
        // SAFETY: the core initialized this known sType chain with the
        // compatibility struct as its first field.
        unsafe {
            (*compatibility).maxStorageBuffersInVertexStage = 33;
            (*compatibility).maxStorageTexturesInVertexStage = 34;
            (*compatibility).maxStorageBuffersInFragmentStage = 35;
            (*compatibility).maxStorageTexturesInFragmentStage = 36;
        }
    }
    crate::WGPUStatus_WGPUStatus_Success
}

unsafe fn adapter_get_info(_adapter: WGPUAdapter, info: *mut WGPUAdapterInfo) -> WGPUStatus {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_adapter_info(info) }
}

unsafe fn device_get_adapter_info(_device: WGPUDevice, info: *mut WGPUAdapterInfo) -> WGPUStatus {
    // SAFETY: the mock dispatcher forwards the caller's out-pointer unchanged.
    unsafe { write_mock_adapter_info(info) }
}

unsafe fn write_mock_adapter_info(info: *mut WGPUAdapterInfo) -> WGPUStatus {
    // SAFETY: callers provide either null or a live writable out-struct.
    let Some(info) = (unsafe { info.as_mut() }) else {
        return crate::WGPUStatus_WGPUStatus_Error;
    };
    info.vendor = WGPUStringView::from_bytes(b"mock-vendor");
    info.architecture = WGPUStringView::from_bytes(b"mock-architecture");
    info.device = WGPUStringView::from_bytes(b"mock-device");
    info.description = WGPUStringView::from_bytes(b"mock-description");
    info.adapterType = crate::WGPUAdapterType_WGPUAdapterType_CPU;
    info.subgroupMinSize = 4;
    info.subgroupMaxSize = 32;
    crate::WGPUStatus_WGPUStatus_Success
}

unsafe fn adapter_info_free_members(_info: WGPUAdapterInfo) {
    GPU_STATE.with(|state| state.borrow_mut().adapter_info_free_members += 1);
}

unsafe fn device_push_error_scope(_device: WGPUDevice, filter: WGPUErrorFilter) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.error_scope_stack.push(filter);
        state.pushed_error_filters.push(filter);
    });
}

unsafe fn device_pop_error_scope(
    _device: WGPUDevice,
    info: WGPUPopErrorScopeCallbackInfo,
) -> WGPUFuture {
    assert_eq!(
        info.mode,
        crate::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        "popErrorScope callbacks must be process-events driven"
    );
    let result = GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(result) = state.next_pop_error.take() {
            let _ = state.error_scope_stack.pop();
            result
        } else if state.error_scope_stack.pop().is_some() {
            MockPopError {
                status: crate::WGPUPopErrorScopeStatus_WGPUPopErrorScopeStatus_Success,
                type_: crate::WGPUErrorType_WGPUErrorType_NoError,
                message: String::new(),
            }
        } else {
            MockPopError {
                status: crate::WGPUPopErrorScopeStatus_WGPUPopErrorScopeStatus_Error,
                type_: crate::WGPUErrorType_WGPUErrorType_NoError,
                message: "error scope stack is empty".to_owned(),
            }
        }
    });
    if let Some(callback) = info.callback {
        unsafe {
            callback(
                result.status,
                result.type_,
                WGPUStringView::from_bytes(result.message.as_bytes()),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    WGPUFuture { id: 3 }
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

unsafe fn device_create_sampler(
    _device: WGPUDevice,
    descriptor: *const WGPUSamplerDescriptor,
) -> WGPUSampler {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.null_create_sampler {
            return ptr::null_mut();
        }
        let descriptor = unsafe { &*descriptor };
        state.sampler_descriptors.push(RecordedSamplerDescriptor {
            label: read_view(descriptor.label),
            address_mode_u: descriptor.addressModeU,
            address_mode_v: descriptor.addressModeV,
            address_mode_w: descriptor.addressModeW,
            mag_filter: descriptor.magFilter,
            min_filter: descriptor.minFilter,
            mipmap_filter: descriptor.mipmapFilter,
            lod_min_clamp: descriptor.lodMinClamp,
            lod_max_clamp: descriptor.lodMaxClamp,
            compare: descriptor.compare,
            max_anisotropy: descriptor.maxAnisotropy,
        });
        state.next += 1;
        fake_handle(2500 + state.next)
    })
}

unsafe fn device_create_texture(
    _device: WGPUDevice,
    descriptor: *const WGPUTextureDescriptor,
) -> WGPUTexture {
    let Some(descriptor) = (unsafe { descriptor.as_ref() }) else {
        return ptr::null_mut();
    };
    let view_formats = if descriptor.viewFormatCount == 0 || descriptor.viewFormats.is_null() {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(descriptor.viewFormats, descriptor.viewFormatCount).to_vec()
        }
    };
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let recorded = RecordedTextureDescriptor {
            width: descriptor.size.width,
            height: descriptor.size.height,
            depth_or_array_layers: descriptor.size.depthOrArrayLayers,
            mip_level_count: descriptor.mipLevelCount,
            sample_count: descriptor.sampleCount,
            dimension: descriptor.dimension,
            format: descriptor.format,
            usage: descriptor.usage,
            view_formats,
        };
        state.next += 1;
        let texture = fake_handle(2600 + state.next);
        state.texture_descriptors.push(recorded.clone());
        state.textures.insert(texture, recorded);
        texture
    })
}

unsafe fn device_create_query_set(
    _device: WGPUDevice,
    descriptor: *const WGPUQuerySetDescriptor,
) -> WGPUQuerySet {
    let Some(descriptor) = (unsafe { descriptor.as_ref() }) else {
        return ptr::null_mut();
    };
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let recorded = RecordedQuerySetDescriptor {
            label: read_view(descriptor.label),
            type_: descriptor.type_,
            count: descriptor.count,
        };
        state.next += 1;
        let query_set = fake_handle(2650 + state.next);
        state.query_set_descriptors.push(recorded.clone());
        state.query_sets.insert(query_set, recorded);
        query_set
    })
}

unsafe fn query_set_add_ref(_query_set: WGPUQuerySet) {
    GPU_STATE.with(|state| state.borrow_mut().query_set_add_refs += 1);
}

unsafe fn query_set_release(_query_set: WGPUQuerySet) {
    GPU_STATE.with(|state| state.borrow_mut().query_set_releases += 1);
}

unsafe fn query_set_destroy(_query_set: WGPUQuerySet) {
    GPU_STATE.with(|state| state.borrow_mut().query_set_destroys += 1);
}

unsafe fn query_set_get_type(query_set: WGPUQuerySet) -> WGPUQueryType {
    GPU_STATE.with(|state| {
        state
            .borrow()
            .query_sets
            .get(&query_set)
            .map_or(0, |descriptor| descriptor.type_)
    })
}

unsafe fn query_set_get_count(query_set: WGPUQuerySet) -> u32 {
    GPU_STATE.with(|state| {
        state
            .borrow()
            .query_sets
            .get(&query_set)
            .map_or(0, |descriptor| descriptor.count)
    })
}

unsafe fn query_set_set_label(_query_set: WGPUQuerySet, label: WGPUStringView) {
    GPU_STATE.with(|state| state.borrow_mut().labels.push(read_view(label)));
}

unsafe fn texture_create_view(
    texture: WGPUTexture,
    descriptor: *const WGPUTextureViewDescriptor,
) -> WGPUTextureView {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(descriptor) = (unsafe { descriptor.as_ref() }) else {
            state.null_texture_view_descriptors += 1;
            state.next += 1;
            return fake_handle(2700 + state.next);
        };
        state
            .texture_view_descriptors
            .push(RecordedTextureViewDescriptor {
                texture,
                format: descriptor.format,
                dimension: descriptor.dimension,
                usage: descriptor.usage,
                aspect: descriptor.aspect,
                base_mip_level: descriptor.baseMipLevel,
                mip_level_count: descriptor.mipLevelCount,
                base_array_layer: descriptor.baseArrayLayer,
                array_layer_count: descriptor.arrayLayerCount,
            });
        state.next += 1;
        fake_handle(2700 + state.next)
    })
}

unsafe fn texture_destroy(_texture: WGPUTexture) {
    GPU_STATE.with(|state| state.borrow_mut().texture_destroys += 1);
}

fn texture_value<T: Copy>(
    texture: WGPUTexture,
    get: impl FnOnce(&RecordedTextureDescriptor) -> T,
    fallback: T,
) -> T {
    GPU_STATE.with(|state| state.borrow().textures.get(&texture).map_or(fallback, get))
}

unsafe fn texture_get_width(texture: WGPUTexture) -> u32 {
    texture_value(texture, |value| value.width, 0)
}

unsafe fn texture_get_height(texture: WGPUTexture) -> u32 {
    texture_value(texture, |value| value.height, 0)
}

unsafe fn texture_get_depth_or_array_layers(texture: WGPUTexture) -> u32 {
    texture_value(texture, |value| value.depth_or_array_layers, 0)
}

unsafe fn texture_get_mip_level_count(texture: WGPUTexture) -> u32 {
    texture_value(texture, |value| value.mip_level_count, 0)
}

unsafe fn texture_get_sample_count(texture: WGPUTexture) -> u32 {
    texture_value(texture, |value| value.sample_count, 0)
}

unsafe fn texture_get_dimension(texture: WGPUTexture) -> WGPUTextureDimension {
    texture_value(
        texture,
        |value| value.dimension,
        crate::WGPUTextureDimension_WGPUTextureDimension_Undefined,
    )
}

unsafe fn texture_get_format(texture: WGPUTexture) -> WGPUTextureFormat {
    texture_value(
        texture,
        |value| value.format,
        crate::WGPUTextureFormat_WGPUTextureFormat_Undefined,
    )
}

unsafe fn texture_get_usage(texture: WGPUTexture) -> WGPUTextureUsage {
    texture_value(texture, |value| value.usage, 0)
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
    descriptor: *const WGPUComputePipelineDescriptor,
) -> WGPUComputePipeline {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let compute = unsafe { &(*descriptor).compute };
        state.pipeline_constants.push(("compute", unsafe {
            read_constants(compute.constants, compute.constantCount)
        }));
        state.next += 1;
        fake_handle(6000 + state.next)
    })
}

unsafe fn device_create_render_pipeline(
    _device: WGPUDevice,
    descriptor: *const WGPURenderPipelineDescriptor,
) -> WGPURenderPipeline {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let descriptor = unsafe { &*descriptor };
        state.pipeline_constants.push(("vertex", unsafe {
            read_constants(descriptor.vertex.constants, descriptor.vertex.constantCount)
        }));
        if let Some(fragment) = unsafe { descriptor.fragment.as_ref() } {
            state.pipeline_constants.push(("fragment", unsafe {
                read_constants(fragment.constants, fragment.constantCount)
            }));
        }
        state.next += 1;
        fake_handle(6500 + state.next)
    })
}

unsafe fn device_create_compute_pipeline_async(
    device: WGPUDevice,
    descriptor: *const WGPUComputePipelineDescriptor,
    info: WGPUCreateComputePipelineAsyncCallbackInfo,
) -> WGPUFuture {
    assert_eq!(
        info.mode,
        crate::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents
    );
    let error = GPU_STATE.with(|state| state.borrow_mut().next_pipeline_async_error.take());
    let pipeline = if error.is_none() {
        unsafe { device_create_compute_pipeline(device, descriptor) }
    } else {
        ptr::null_mut()
    };
    if let Some(callback) = info.callback {
        let (status, message) = error.unwrap_or((
            crate::WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_Success,
            String::new(),
        ));
        unsafe {
            callback(
                status,
                pipeline,
                WGPUStringView::from_bytes(message.as_bytes()),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    WGPUFuture { id: 50 }
}

unsafe fn device_create_render_pipeline_async(
    device: WGPUDevice,
    descriptor: *const WGPURenderPipelineDescriptor,
    info: WGPUCreateRenderPipelineAsyncCallbackInfo,
) -> WGPUFuture {
    assert_eq!(
        info.mode,
        crate::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents
    );
    let error = GPU_STATE.with(|state| state.borrow_mut().next_pipeline_async_error.take());
    let pipeline = if error.is_none() {
        unsafe { device_create_render_pipeline(device, descriptor) }
    } else {
        ptr::null_mut()
    };
    if let Some(callback) = info.callback {
        let (status, message) = error.unwrap_or((
            crate::WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_Success,
            String::new(),
        ));
        unsafe {
            callback(
                status,
                pipeline,
                WGPUStringView::from_bytes(message.as_bytes()),
                info.userdata1,
                info.userdata2,
            );
        }
    }
    WGPUFuture { id: 51 }
}

unsafe fn read_constants(
    constants: *const crate::WGPUConstantEntry,
    count: usize,
) -> Vec<(Vec<u8>, f64)> {
    if count == 0 {
        assert!(constants.is_null());
        return Vec::new();
    }
    assert!(!constants.is_null());
    unsafe { std::slice::from_raw_parts(constants, count) }
        .iter()
        .map(|entry| (read_view(entry.key), entry.value))
        .collect()
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

unsafe fn device_create_render_bundle_encoder(
    _device: WGPUDevice,
    descriptor: *const WGPURenderBundleEncoderDescriptor,
) -> WGPURenderBundleEncoder {
    if descriptor.is_null() {
        return ptr::null_mut();
    }
    let descriptor = unsafe { &*descriptor };
    let color_formats = if descriptor.colorFormatCount == 0 {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(descriptor.colorFormats, descriptor.colorFormatCount)
                .to_vec()
        }
    };
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state
            .render_bundle_encoder_descriptors
            .push(RecordedRenderBundleEncoderDescriptor {
                label: read_view(descriptor.label),
                color_formats,
                depth_stencil_format: descriptor.depthStencilFormat,
                sample_count: descriptor.sampleCount,
                depth_read_only: descriptor.depthReadOnly,
                stencil_read_only: descriptor.stencilReadOnly,
            });
        state.next += 1;
        fake_handle(12_000 + state.next)
    })
}

unsafe fn buffer_set_label(_buffer: WGPUBuffer, label: WGPUStringView) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.labels.push(read_view(label));
        state.native_order.push("buffer_set_label");
    });
}

unsafe fn buffer_destroy(buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_destroys += 1;
        state.native_order.push("buffer_destroy");
        state.mapped_ranges.remove(&buffer);
    });
}

fn overlaps_outstanding_range(
    state: &MockGpuState,
    buffer: WGPUBuffer,
    offset: usize,
    end: usize,
    is_const: bool,
) -> bool {
    state.mapped_ranges.get(&buffer).is_some_and(|ranges| {
        ranges
            .iter()
            .any(|range| offset < range.end && range.offset < end && !(is_const && range.is_const))
    })
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
        if overlaps_outstanding_range(&state, buffer, offset, end, false) {
            return ptr::null_mut();
        }
        state
            .mapped_ranges
            .entry(buffer)
            .or_default()
            .push(MockMappedRange {
                offset,
                end,
                is_const: false,
            });
        let bytes = state
            .buffers
            .get_mut(&buffer)
            .expect("buffer checked above");
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
        if overlaps_outstanding_range(&state, buffer, offset, end, true) {
            return ptr::null();
        }
        state
            .mapped_ranges
            .entry(buffer)
            .or_default()
            .push(MockMappedRange {
                offset,
                end,
                is_const: true,
            });
        let bytes = state.buffers.get(&buffer).expect("buffer checked above");
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

unsafe fn buffer_unmap(buffer: WGPUBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.buffer_unmaps += 1;
        state.mapped_ranges.remove(&buffer);
    });
}

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

unsafe fn queue_write_texture(
    _queue: WGPUQueue,
    _destination: *const WGPUTexelCopyTextureInfo,
    _data: *const std::ffi::c_void,
    _data_size: usize,
    _data_layout: *const WGPUTexelCopyBufferLayout,
    _write_size: *const WGPUExtent3D,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("queue_write_texture")
            .or_default() += 1;
    });
    // T7: yawgpu Noop does not execute texture writes; the mock records no texels either.
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

unsafe fn shader_module_add_ref(_module: WGPUShaderModule) {
    GPU_STATE.with(|state| state.borrow_mut().shader_module_add_refs += 1);
}
unsafe fn shader_module_release(_module: WGPUShaderModule) {
    GPU_STATE.with(|state| state.borrow_mut().shader_module_releases += 1);
}
unsafe fn sampler_add_ref(_sampler: WGPUSampler) {
    GPU_STATE.with(|state| state.borrow_mut().sampler_add_refs += 1);
}
unsafe fn sampler_release(_sampler: WGPUSampler) {
    GPU_STATE.with(|state| state.borrow_mut().sampler_releases += 1);
}
unsafe fn texture_add_ref(_texture: WGPUTexture) {
    GPU_STATE.with(|state| state.borrow_mut().texture_add_refs += 1);
}
unsafe fn texture_release(_texture: WGPUTexture) {
    GPU_STATE.with(|state| state.borrow_mut().texture_releases += 1);
}
unsafe fn texture_view_add_ref(_texture_view: WGPUTextureView) {
    GPU_STATE.with(|state| state.borrow_mut().texture_view_add_refs += 1);
}
unsafe fn texture_view_release(_texture_view: WGPUTextureView) {
    GPU_STATE.with(|state| state.borrow_mut().texture_view_releases += 1);
}
unsafe fn sampler_set_label(_sampler: WGPUSampler, label: WGPUStringView) {
    GPU_STATE.with(|state| state.borrow_mut().labels.push(read_view(label)));
}
unsafe fn bind_group_layout_add_ref(_layout: WGPUBindGroupLayout) {
    GPU_STATE.with(|state| state.borrow_mut().bind_group_layout_add_refs += 1);
}
unsafe fn bind_group_layout_release(_layout: WGPUBindGroupLayout) {
    GPU_STATE.with(|state| state.borrow_mut().bind_group_layout_releases += 1);
}
unsafe fn pipeline_layout_add_ref(_layout: WGPUPipelineLayout) {
    GPU_STATE.with(|state| state.borrow_mut().pipeline_layout_add_refs += 1);
}
unsafe fn pipeline_layout_release(_layout: WGPUPipelineLayout) {
    GPU_STATE.with(|state| state.borrow_mut().pipeline_layout_releases += 1);
}
unsafe fn bind_group_add_ref(_bind_group: WGPUBindGroup) {}
unsafe fn bind_group_release(_bind_group: WGPUBindGroup) {
    GPU_STATE.with(|state| state.borrow_mut().bind_group_releases += 1);
}
unsafe fn compute_pipeline_add_ref(_pipeline: WGPUComputePipeline) {
    GPU_STATE.with(|state| state.borrow_mut().compute_pipeline_add_refs += 1);
}
unsafe fn compute_pipeline_release(_pipeline: WGPUComputePipeline) {
    GPU_STATE.with(|state| state.borrow_mut().compute_pipeline_releases += 1);
}
unsafe fn render_pipeline_add_ref(_pipeline: WGPURenderPipeline) {
    GPU_STATE.with(|state| state.borrow_mut().render_pipeline_add_refs += 1);
}
unsafe fn render_pipeline_release(_pipeline: WGPURenderPipeline) {
    GPU_STATE.with(|state| state.borrow_mut().render_pipeline_releases += 1);
}

unsafe fn compute_pipeline_get_bind_group_layout(
    _pipeline: WGPUComputePipeline,
    group_index: u32,
) -> WGPUBindGroupLayout {
    if group_index == u32::MAX {
        ptr::null_mut()
    } else {
        fake_handle(10_000 + group_index as usize)
    }
}

unsafe fn render_pipeline_get_bind_group_layout(
    _pipeline: WGPURenderPipeline,
    group_index: u32,
) -> WGPUBindGroupLayout {
    if group_index == u32::MAX {
        ptr::null_mut()
    } else {
        fake_handle(11_000 + group_index as usize)
    }
}
unsafe fn command_encoder_release(_encoder: WGPUCommandEncoder) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.command_encoder_releases += 1;
        let retained = std::mem::take(&mut state.encoder_retained_indirect_buffers);
        state.released_indirect_buffers.extend(retained);
    });
}

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

unsafe fn command_encoder_begin_render_pass(
    _encoder: WGPUCommandEncoder,
    _descriptor: *const WGPURenderPassDescriptor,
) -> WGPURenderPassEncoder {
    fake_handle(8002)
}

unsafe fn command_encoder_copy_buffer_to_texture(
    _encoder: WGPUCommandEncoder,
    _source: *const WGPUTexelCopyBufferInfo,
    _destination: *const WGPUTexelCopyTextureInfo,
    _copy_size: *const WGPUExtent3D,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("copy_buffer_to_texture")
            .or_default() += 1;
    });
}

unsafe fn command_encoder_copy_texture_to_buffer(
    _encoder: WGPUCommandEncoder,
    _source: *const WGPUTexelCopyTextureInfo,
    _destination: *const WGPUTexelCopyBufferInfo,
    _copy_size: *const WGPUExtent3D,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("copy_texture_to_buffer")
            .or_default() += 1;
    });
}

unsafe fn command_encoder_copy_texture_to_texture(
    _encoder: WGPUCommandEncoder,
    _source: *const WGPUTexelCopyTextureInfo,
    _destination: *const WGPUTexelCopyTextureInfo,
    _copy_size: *const WGPUExtent3D,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("copy_texture_to_texture")
            .or_default() += 1;
    });
}

unsafe fn command_encoder_finish(
    _encoder: WGPUCommandEncoder,
    _descriptor: *const WGPUCommandBufferDescriptor,
) -> WGPUCommandBuffer {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let retained = std::mem::take(&mut state.encoder_retained_indirect_buffers);
        state
            .command_buffer_retained_indirect_buffers
            .extend(retained);
    });
    fake_handle(9001)
}

unsafe fn command_buffer_release(_command_buffer: WGPUCommandBuffer) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.command_buffer_releases += 1;
        let retained = std::mem::take(&mut state.command_buffer_retained_indirect_buffers);
        state.released_indirect_buffers.extend(retained);
    });
}
unsafe fn compute_pass_encoder_release(_pass: WGPUComputePassEncoder) {
    GPU_STATE.with(|state| state.borrow_mut().compute_pass_encoder_releases += 1);
}
unsafe fn compute_pass_encoder_set_pipeline(
    _pass: WGPUComputePassEncoder,
    _pipeline: WGPUComputePipeline,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("compute_set_pipeline")
            .or_default() += 1;
    });
}
unsafe fn compute_pass_encoder_set_bind_group(
    _pass: WGPUComputePassEncoder,
    _index: u32,
    _bind_group: WGPUBindGroup,
    _offset_count: usize,
    _offsets: *const u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("compute_set_bind_group")
            .or_default() += 1;
    });
}
unsafe fn compute_pass_encoder_dispatch_workgroups(
    _pass: WGPUComputePassEncoder,
    _x: u32,
    _y: u32,
    _z: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("dispatch_workgroups")
            .or_default() += 1;
    });
}
unsafe fn compute_pass_encoder_dispatch_workgroups_indirect(
    _pass: WGPUComputePassEncoder,
    indirect_buffer: WGPUBuffer,
    indirect_offset: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("dispatch_workgroups_indirect")
            .or_default() += 1;
        state.indirect_calls.push((
            "dispatch_workgroups_indirect",
            indirect_buffer,
            indirect_offset,
        ));
        state
            .encoder_retained_indirect_buffers
            .push(indirect_buffer);
    });
}
unsafe fn compute_pass_encoder_end(_pass: WGPUComputePassEncoder) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("compute_end")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_release(_pass: WGPURenderPassEncoder) {
    GPU_STATE.with(|state| state.borrow_mut().render_pass_encoder_releases += 1);
}
unsafe fn render_pass_encoder_set_pipeline(
    _pass: WGPURenderPassEncoder,
    _pipeline: WGPURenderPipeline,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("render_set_pipeline")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_set_vertex_buffer(
    _pass: WGPURenderPassEncoder,
    _slot: u32,
    _buffer: WGPUBuffer,
    offset: u64,
    size: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("render_set_vertex_buffer")
            .or_default() += 1;
        state.vertex_buffer_ranges.push((offset, size));
    });
}
unsafe fn render_pass_encoder_set_index_buffer(
    _pass: WGPURenderPassEncoder,
    _buffer: WGPUBuffer,
    _format: WGPUIndexFormat,
    offset: u64,
    size: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("render_set_index_buffer")
            .or_default() += 1;
        state.index_buffer_ranges.push((offset, size));
    });
}
unsafe fn render_pass_encoder_set_bind_group(
    _pass: WGPURenderPassEncoder,
    _index: u32,
    _bind_group: WGPUBindGroup,
    _offset_count: usize,
    _offsets: *const u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("render_set_bind_group")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_draw(
    _pass: WGPURenderPassEncoder,
    _vertex_count: u32,
    _instance_count: u32,
    _first_vertex: u32,
    _first_instance: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("draw")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_draw_indexed(
    _pass: WGPURenderPassEncoder,
    _index_count: u32,
    _instance_count: u32,
    _first_index: u32,
    _base_vertex: i32,
    _first_instance: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("draw_indexed")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_draw_indirect(
    _pass: WGPURenderPassEncoder,
    indirect_buffer: WGPUBuffer,
    indirect_offset: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state.recording_calls.entry("draw_indirect").or_default() += 1;
        state
            .indirect_calls
            .push(("draw_indirect", indirect_buffer, indirect_offset));
        state
            .encoder_retained_indirect_buffers
            .push(indirect_buffer);
    });
}
unsafe fn render_pass_encoder_draw_indexed_indirect(
    _pass: WGPURenderPassEncoder,
    indirect_buffer: WGPUBuffer,
    indirect_offset: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("draw_indexed_indirect")
            .or_default() += 1;
        state
            .indirect_calls
            .push(("draw_indexed_indirect", indirect_buffer, indirect_offset));
        state
            .encoder_retained_indirect_buffers
            .push(indirect_buffer);
    });
}
unsafe fn render_pass_encoder_set_viewport(
    _pass: WGPURenderPassEncoder,
    _x: f32,
    _y: f32,
    _width: f32,
    _height: f32,
    _min_depth: f32,
    _max_depth: f32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("set_viewport")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_set_scissor_rect(
    _pass: WGPURenderPassEncoder,
    _x: u32,
    _y: u32,
    _width: u32,
    _height: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("set_scissor_rect")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_begin_occlusion_query(
    _pass: WGPURenderPassEncoder,
    _query_index: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("begin_occlusion_query")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_end_occlusion_query(_pass: WGPURenderPassEncoder) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("end_occlusion_query")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_execute_bundles(
    _pass: WGPURenderPassEncoder,
    bundle_count: usize,
    bundles: *const WGPURenderBundle,
) {
    assert!(bundle_count == 0 || !bundles.is_null());
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("execute_bundles")
            .or_default() += 1;
    });
}
unsafe fn render_pass_encoder_end(_pass: WGPURenderPassEncoder) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("render_end")
            .or_default() += 1;
    });
}

unsafe fn render_bundle_encoder_release(_encoder: WGPURenderBundleEncoder) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.render_bundle_encoder_releases += 1;
        let retained = std::mem::take(&mut state.bundle_encoder_retained_indirect_buffers);
        state.released_bundle_indirect_buffers.extend(retained);
    });
}
unsafe fn render_bundle_encoder_set_pipeline(
    _encoder: WGPURenderBundleEncoder,
    _pipeline: WGPURenderPipeline,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_set_pipeline")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_set_vertex_buffer(
    _encoder: WGPURenderBundleEncoder,
    _slot: u32,
    _buffer: WGPUBuffer,
    _offset: u64,
    _size: u64,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_set_vertex_buffer")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_set_index_buffer(
    _encoder: WGPURenderBundleEncoder,
    _buffer: WGPUBuffer,
    _format: WGPUIndexFormat,
    _offset: u64,
    _size: u64,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_set_index_buffer")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_set_bind_group(
    _encoder: WGPURenderBundleEncoder,
    _index: u32,
    _bind_group: WGPUBindGroup,
    _offset_count: usize,
    _offsets: *const u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_set_bind_group")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_draw(
    _encoder: WGPURenderBundleEncoder,
    _vertex_count: u32,
    _instance_count: u32,
    _first_vertex: u32,
    _first_instance: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_draw")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_draw_indexed(
    _encoder: WGPURenderBundleEncoder,
    _index_count: u32,
    _instance_count: u32,
    _first_index: u32,
    _base_vertex: i32,
    _first_instance: u32,
) {
    GPU_STATE.with(|state| {
        *state
            .borrow_mut()
            .recording_calls
            .entry("bundle_draw_indexed")
            .or_default() += 1;
    });
}
unsafe fn render_bundle_encoder_draw_indirect(
    _encoder: WGPURenderBundleEncoder,
    indirect_buffer: WGPUBuffer,
    indirect_offset: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("bundle_draw_indirect")
            .or_default() += 1;
        state
            .indirect_calls
            .push(("bundle_draw_indirect", indirect_buffer, indirect_offset));
        state
            .bundle_encoder_retained_indirect_buffers
            .push(indirect_buffer);
    });
}
unsafe fn render_bundle_encoder_draw_indexed_indirect(
    _encoder: WGPURenderBundleEncoder,
    indirect_buffer: WGPUBuffer,
    indirect_offset: u64,
) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state
            .recording_calls
            .entry("bundle_draw_indexed_indirect")
            .or_default() += 1;
        state.indirect_calls.push((
            "bundle_draw_indexed_indirect",
            indirect_buffer,
            indirect_offset,
        ));
        state
            .bundle_encoder_retained_indirect_buffers
            .push(indirect_buffer);
    });
}
unsafe fn render_bundle_encoder_finish(
    _encoder: WGPURenderBundleEncoder,
    _descriptor: *const WGPURenderBundleDescriptor,
) -> WGPURenderBundle {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let retained = std::mem::take(&mut state.bundle_encoder_retained_indirect_buffers);
        state
            .render_bundle_retained_indirect_buffers
            .extend(retained);
    });
    fake_handle(13_001)
}
unsafe fn render_bundle_release(_bundle: WGPURenderBundle) {
    GPU_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.render_bundle_releases += 1;
        let retained = std::mem::take(&mut state.render_bundle_retained_indirect_buffers);
        state.released_bundle_indirect_buffers.extend(retained);
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
        adapter_request_device, buffer_destroy, buffer_get_mapped_range, buffer_label_get,
        buffer_label_set, buffer_map_async, buffer_size_get, buffer_unmap, buffer_usage_get,
        command_encoder_finish, convert_bind_group_descriptor, convert_bind_group_entry,
        convert_bind_group_layout_descriptor, convert_blend_component,
        convert_buffer_binding_layout, convert_buffer_descriptor, convert_color_dict,
        convert_color_target_state, convert_command_buffer_descriptor,
        convert_command_encoder_descriptor, convert_compute_pass_descriptor,
        convert_compute_pipeline_descriptor, convert_depth_stencil_state, convert_gpu_color,
        convert_gpu_extent3d, convert_gpu_origin3d, convert_multisample_state,
        convert_pipeline_layout_descriptor, convert_primitive_state, convert_query_set_descriptor,
        convert_render_pass_color_attachment, convert_render_pass_depth_stencil_attachment,
        convert_render_pass_descriptor, convert_render_pipeline_descriptor,
        convert_sampler_binding_layout, convert_sampler_descriptor,
        convert_shader_module_descriptor, convert_stencil_face_state,
        convert_storage_texture_binding_layout, convert_texel_copy_buffer_info,
        convert_texel_copy_buffer_layout, convert_texel_copy_texture_info,
        convert_texture_binding_layout, convert_texture_descriptor,
        convert_texture_view_descriptor, convert_vertex_attribute, convert_vertex_buffer_layout,
        device_create_bind_group, device_create_buffer, device_create_command_encoder,
        device_create_compute_pipeline, device_create_query_set, device_create_render_pipeline,
        device_create_sampler, device_create_texture, device_destroy, device_lost_get,
        device_lost_info_message_get, device_lost_info_reason_get, device_on_uncaptured_error_get,
        device_on_uncaptured_error_set, device_pop_error_scope, device_push_error_scope,
        device_queue_get, finalize_bind_group, finalize_buffer, finalize_compute_pipeline,
        finalize_device, finalize_query_set, finalize_queue, finalize_render_pipeline,
        finalize_sampler, finalize_texture, finalize_texture_view, gpu_request_adapter,
        queue_on_submitted_work_done, queue_submit, queue_work_done_callback, queue_write_buffer,
        queue_write_texture, request_adapter_callback, request_device_callback,
        texture_depth_or_array_layers_get, texture_dimension_get, texture_format_get,
        texture_height_get, texture_mip_level_count_get, texture_sample_count_get,
        texture_usage_get, texture_width_get, wrap_device, AdapterPayload, AdapterRequest,
        BindGroupLayoutPayload, BindGroupPayload, BufferPayload, ComputePipelinePayload,
        DeviceEventState, DevicePayload, DeviceRequest, ErrorPayload, JsEngine, PendingNative,
        PendingNativeHandle, PipelineLayoutPayload, QuerySetPayload, QueueError, QueuePayload,
        QueueWorkDoneRequest, RenderPipelinePayload, SamplerPayload, SettlementRequest,
        ShaderModulePayload, TexturePayload, TextureViewPayload,
    };
    use std::sync::atomic::AtomicBool;
    use std::sync::{Mutex, Weak};

    struct SendPtr<T>(*mut T);

    // SAFETY: tests move the typed native pointer between threads only as an
    // opaque registry key. The receiving thread never dereferences it or calls
    // webgpu.h; it passes it solely to the enqueue-only event forwarder.
    unsafe impl<T> Send for SendPtr<T> {}

    impl<T> SendPtr<T> {
        fn new(ptr: *mut T) -> Self {
            Self(ptr)
        }

        fn get(self) -> *mut T {
            self.0
        }
    }

    fn descriptor(rt: &Runtime, fields: &[(&str, Value)]) -> Value {
        rt.object(fields)
    }

    fn assert_rejection(rt: &Runtime, promise: Value, name: &str, message: &str) {
        let reason = rt
            .promise_result(promise)
            .expect("promise must settle")
            .expect_err("promise must reject");
        let MockValue::Object(properties) = rt.get(reason) else {
            panic!("rejection reason is not an error object");
        };
        assert!(matches!(
            properties.get("name").copied().map(|value| rt.get(value)),
            Some(MockValue::String(actual)) if actual == name
        ));
        assert!(matches!(
            properties.get("message").copied().map(|value| rt.get(value)),
            Some(MockValue::String(actual)) if actual == message
        ));
    }

    fn shader_module(_rt: &Runtime, cx: Context<'_>, handle: usize) -> Value {
        Engine::new_instance(
            cx,
            crate::GPU_SHADER_MODULE_CLASS,
            Box::new(ShaderModulePayload {
                module: fake_handle(handle),
            }),
        )
        .expect("shader module")
    }

    fn array_buffer_view(
        rt: &Runtime,
        backing: Value,
        byte_offset: f64,
        byte_length: f64,
        bytes_per_element: Option<f64>,
    ) -> Value {
        let constructor = match bytes_per_element {
            Some(value) => rt.object(&[("BYTES_PER_ELEMENT", rt.number(value))]),
            None => rt.object(&[]),
        };
        rt.object(&[
            ("buffer", backing),
            ("byteOffset", rt.number(byte_offset)),
            ("byteLength", rt.number(byte_length)),
            ("constructor", constructor),
        ])
    }

    #[test]
    fn js_engine_global_returns_a_scope_tracked_global_object() {
        let rt = runtime();
        let reclaimed_before = rt.reclaimed_values();
        rt.with_scope(|cx| {
            let global = Engine::global(cx);
            let symbol = Engine::get_property(cx, global, "Symbol").expect("Symbol");
            assert!(!Engine::is_undefined(cx, symbol));
        });
        assert_eq!(rt.reclaimed_values() - reclaimed_before, 2);
    }

    #[test]
    fn js_engine_get_property_value_tracks_success_and_propagates_error() {
        let rt = runtime();
        let iterable = rt.set_like(&[rt.number(7.0)]);
        let reclaimed_before = rt.reclaimed_values();
        rt.with_scope(|cx| {
            let method = Engine::get_property_value(cx, iterable, rt.symbol_iterator)
                .expect("Symbol.iterator method");
            assert!(matches!(rt.get(method), MockValue::Callable(_)));
        });
        assert_eq!(rt.reclaimed_values() - reclaimed_before, 1);
        assert_eq!(rt.property_value_calls.get(), 1);

        rt.set_property_value_error(iterable, rt.symbol_iterator, "symbol getter failed");
        rt.with_scope(|cx| {
            assert_eq!(
                Engine::get_property_value(cx, iterable, rt.symbol_iterator)
                    .expect_err("symbol getter must fail"),
                "symbol getter failed"
            );
        });
        assert_eq!(rt.property_value_calls.get(), 2);
    }

    #[test]
    fn js_engine_call_tracks_success_and_propagates_error() {
        let rt = runtime();
        let iterable = rt.set_like(&[rt.number(11.0)]);
        let MockValue::Iterable {
            iterator_method, ..
        } = rt.get(iterable)
        else {
            panic!("set-like mock must be iterable");
        };
        let reclaimed_before = rt.reclaimed_values();
        rt.with_scope(|cx| {
            let iterator = Engine::call(cx, iterator_method, iterable, &[]).expect("iterator");
            assert!(matches!(rt.get(iterator), MockValue::Iterator { .. }));
        });
        assert_eq!(rt.reclaimed_values() - reclaimed_before, 1);
        assert_eq!(rt.calls.get(), 1);

        rt.set_call_error(iterator_method, "iterator method failed");
        rt.with_scope(|cx| {
            assert_eq!(
                Engine::call(cx, iterator_method, iterable, &[])
                    .expect_err("iterator call must fail"),
                "iterator method failed"
            );
        });
        assert_eq!(rt.calls.get(), 2);
    }

    #[test]
    fn i1_construct_records_arguments_and_propagates_failure() {
        let rt = runtime();
        rt.with_scope(|cx| {
            let global = Engine::global(cx);
            let array = Engine::get_property(cx, global, "Array").expect("Array");
            let one = rt.string("one");
            let two = rt.string("two");
            let value = Engine::construct(cx, array, &[one, two]).expect("construct Array");
            assert!(matches!(rt.get(value), MockValue::Iterable { .. }));
            assert_eq!(rt.constructs.get(), 1);
            assert_eq!(rt.construct_history.borrow().as_slice(), &[vec![one, two]]);

            rt.set_construct_error(array, "constructor failed");
            assert_eq!(
                Engine::construct(cx, array, &[]).expect_err("construct must fail"),
                "constructor failed"
            );
            assert_eq!(rt.constructs.get(), 2);
        });
    }

    fn release_device_held_values(rt: &Runtime, cx: Context<'_>, device: Value) {
        let payload = Engine::payload(cx, device, crate::GPU_DEVICE_CLASS)
            .and_then(|payload| payload.downcast_ref::<DevicePayload<Engine>>())
            .expect("device payload");
        crate::release_payload_values::<Engine>(payload, &mut |value| {
            Engine::release_value(cx, value);
        });
        assert!(rt.duplicated_values.borrow().is_empty());
    }

    #[test]
    fn arena_alloc_slice_keeps_heterogeneous_allocations_address_stable() {
        let arena = Arena::new();
        let numbers = arena.alloc_slice(vec![1_u32, 2, 3]);
        let number_ptr = numbers.as_ptr();
        let bytes = arena.alloc_slice(vec![b'a', b'b']);
        assert_eq!(numbers, [1, 2, 3]);
        assert_eq!(numbers.as_ptr(), number_ptr);
        assert_eq!(bytes, b"ab");
    }

    #[test]
    fn release_queue_drains_core_requests_in_fifo_order() {
        TEST_RELEASE_ORDER.with(|order| order.borrow_mut().clear());
        let queue = ReleaseQueue::new();
        let mut gpu = dispatch();
        gpu.device_release = ordered_device_release;
        for id in [1_usize, 2, 3] {
            queue
                .enqueue(crate::ReleaseRequest::Device {
                    device: fake_handle(id),
                    gpu,
                })
                .expect("enqueue");
        }
        assert_eq!(queue.drain(), Ok(3));
        TEST_RELEASE_ORDER.with(|order| assert_eq!(&*order.borrow(), &[1, 2, 3]));
    }

    #[test]
    fn a30_core_tick_batches_settlements_and_owns_step_order() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        for _ in 0..2 {
            let (_, deferred) = Engine::new_promise(cx).expect("promise");
            rt.env
                .settlements()
                .enqueue::<Engine>(crate::SettlementRequest::Success { deferred })
                .expect("enqueue settlement");
        }
        rt.queue()
            .enqueue(crate::ReleaseRequest::Device {
                device: fake_device(),
                gpu: dispatch(),
            })
            .expect("enqueue release");

        let drained = unsafe { crate::tick::<Engine>(cx, fake_handle(99)) }.expect("tick");

        assert_eq!(drained, 1);
        assert_eq!(rt.settle_calls.get(), 1);
        assert_eq!(&*rt.settlement_batch_sizes.borrow(), &[2]);
        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().native_order,
                ["process_events", "settle", "microtasks", "device_release"]
            );
        });
    }

    #[test]
    fn core_tick_reports_unexpected_settlement_type() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let (_, deferred) = MockEngine::<true>::new_promise(cx).expect("promise");
        rt.env
            .settlements()
            .enqueue::<MockEngine<true>>(crate::SettlementRequest::Success { deferred })
            .expect("enqueue settlement");

        let error = unsafe { crate::tick::<Engine>(cx, fake_handle(99)) }.expect_err("tick error");
        assert!(matches!(
            error,
            crate::TickError::Queue(QueueError::UnexpectedSettlementType)
        ));
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
    fn g12_sampler_descriptor_happy_path_converts_every_kind() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(
            &rt,
            &[
                ("label", rt.string("shadow")),
                ("addressModeU", rt.string("repeat")),
                ("addressModeV", rt.string("mirror-repeat")),
                ("addressModeW", rt.string("clamp-to-edge")),
                ("magFilter", rt.string("linear")),
                ("minFilter", rt.string("linear")),
                ("mipmapFilter", rt.string("linear")),
                ("lodMinClamp", rt.number(1.25)),
                ("lodMaxClamp", rt.number(12.5)),
                ("compare", rt.string("greater-equal")),
                ("maxAnisotropy", rt.number(8.0)),
            ],
        );
        let arena = Arena::new();
        let converted =
            convert_sampler_descriptor::<Engine>(cx, desc, &arena).expect("sampler descriptor");

        assert_eq!(read_view(converted.label), b"shadow");
        assert_eq!(
            converted.addressModeU,
            crate::WGPUAddressMode_WGPUAddressMode_Repeat
        );
        assert_eq!(
            converted.addressModeV,
            crate::WGPUAddressMode_WGPUAddressMode_MirrorRepeat
        );
        assert_eq!(
            converted.addressModeW,
            crate::WGPUAddressMode_WGPUAddressMode_ClampToEdge
        );
        assert_eq!(
            converted.magFilter,
            crate::WGPUFilterMode_WGPUFilterMode_Linear
        );
        assert_eq!(
            converted.minFilter,
            crate::WGPUFilterMode_WGPUFilterMode_Linear
        );
        assert_eq!(
            converted.mipmapFilter,
            crate::WGPUMipmapFilterMode_WGPUMipmapFilterMode_Linear
        );
        assert_eq!(converted.lodMinClamp, 1.25);
        assert_eq!(converted.lodMaxClamp, 12.5);
        assert_eq!(
            converted.compare,
            crate::WGPUCompareFunction_WGPUCompareFunction_GreaterEqual
        );
        assert_eq!(converted.maxAnisotropy, 8);
    }

    #[test]
    fn g12_sampler_enum_conversions_reject_unknown_strings() {
        for (member, expected) in [
            ("addressModeU", "GPUAddressMode"),
            ("magFilter", "GPUFilterMode"),
            ("mipmapFilter", "GPUMipmapFilterMode"),
            ("compare", "GPUCompareFunction"),
        ] {
            let rt = runtime();
            let cx = rt.context();
            let desc = descriptor(&rt, &[(member, rt.string("unknown"))]);
            let arena = Arena::new();
            assert_eq!(
                convert_sampler_descriptor::<Engine>(cx, desc, &arena)
                    .expect_err("unknown enum must fail"),
                format!("TypeError: {expected}")
            );
        }
    }

    #[test]
    fn g12_sampler_defaults_follow_webidl() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        let converted = convert_sampler_descriptor::<Engine>(cx, rt.undefined(), &arena)
            .expect("default sampler descriptor");

        assert_eq!(read_view(converted.label), b"");
        assert_eq!(
            converted.addressModeU,
            crate::WGPUAddressMode_WGPUAddressMode_ClampToEdge
        );
        assert_eq!(converted.addressModeV, converted.addressModeU);
        assert_eq!(converted.addressModeW, converted.addressModeU);
        assert_eq!(
            converted.magFilter,
            crate::WGPUFilterMode_WGPUFilterMode_Nearest
        );
        assert_eq!(converted.minFilter, converted.magFilter);
        assert_eq!(
            converted.mipmapFilter,
            crate::WGPUMipmapFilterMode_WGPUMipmapFilterMode_Nearest
        );
        assert_eq!(converted.lodMinClamp, 0.0);
        assert_eq!(converted.lodMaxClamp, 32.0);
        assert_eq!(
            converted.compare,
            crate::WGPUCompareFunction_WGPUCompareFunction_Undefined
        );
        assert_eq!(converted.maxAnisotropy, 1);
    }

    #[test]
    fn g12_sampler_max_anisotropy_uses_webidl_clamp() {
        for (input, expected) in [
            (2.5, 2),
            (3.5, 4),
            (70_000.0, u16::MAX),
            (f64::INFINITY, u16::MAX),
        ] {
            let rt = runtime();
            let cx = rt.context();
            let desc = descriptor(
                &rt,
                &[
                    ("magFilter", rt.string("linear")),
                    ("minFilter", rt.string("linear")),
                    ("mipmapFilter", rt.string("linear")),
                    ("maxAnisotropy", rt.number(input)),
                ],
            );
            let arena = Arena::new();
            let converted = convert_sampler_descriptor::<Engine>(cx, desc, &arena)
                .expect("clamped sampler descriptor");
            assert_eq!(converted.maxAnisotropy, expected, "input={input}");
        }
    }

    #[test]
    fn g12_sampler_restricted_floats_reject_non_finite_values() {
        for member in ["lodMinClamp", "lodMaxClamp"] {
            let rt = runtime();
            let cx = rt.context();
            let desc = descriptor(&rt, &[(member, rt.number(f64::INFINITY))]);
            let arena = Arena::new();
            assert!(convert_sampler_descriptor::<Engine>(cx, desc, &arena).is_err());
        }
    }

    #[test]
    fn t1_extent_and_origin_dictionary_arms_apply_webidl_defaults() {
        let rt = runtime();
        let cx = rt.context();
        let extent =
            convert_gpu_extent3d::<Engine>(cx, descriptor(&rt, &[("width", rt.number(7.0))]))
                .expect("extent dictionary");
        assert_eq!(
            (extent.width, extent.height, extent.depthOrArrayLayers),
            (7, 1, 1)
        );

        let origin =
            convert_gpu_origin3d::<Engine>(cx, descriptor(&rt, &[])).expect("origin dictionary");
        assert_eq!((origin.x, origin.y, origin.z), (0, 0, 0));
    }

    #[test]
    fn t1_extent_and_origin_sequence_arms_use_iterator_protocol_and_trailing_defaults() {
        let rt = runtime();
        let cx = rt.context();
        let extent =
            convert_gpu_extent3d::<Engine>(cx, rt.set_like(&[rt.number(4.0), rt.number(5.0)]))
                .expect("extent sequence");
        assert_eq!(
            (extent.width, extent.height, extent.depthOrArrayLayers),
            (4, 5, 1)
        );

        let origin = convert_gpu_origin3d::<Engine>(
            cx,
            rt.set_like(&[rt.number(2.0), rt.number(3.0), rt.number(4.0)]),
        )
        .expect("origin sequence");
        assert_eq!((origin.x, origin.y, origin.z), (2, 3, 4));

        let empty_origin =
            convert_gpu_origin3d::<Engine>(cx, rt.set_like(&[])).expect("empty origin sequence");
        assert_eq!((empty_origin.x, empty_origin.y, empty_origin.z), (0, 0, 0));
    }

    #[test]
    fn t1_union_rejects_wrong_lengths_missing_width_and_invalid_coordinates() {
        for values in [Vec::new(), vec![1.0, 2.0, 3.0, 4.0]] {
            let rt = runtime();
            let cx = rt.context();
            let values = values
                .into_iter()
                .map(|value| rt.number(value))
                .collect::<Vec<_>>();
            let error =
                convert_gpu_extent3d::<Engine>(cx, rt.set_like(&values)).expect_err("wrong length");
            assert!(error.contains("sequence length must be 1..=3"), "{error}");
        }

        let rt = runtime();
        let cx = rt.context();
        assert_eq!(
            convert_gpu_extent3d::<Engine>(cx, descriptor(&rt, &[])).expect_err("missing width"),
            "TypeError: width"
        );
        for value in [-1.0, 1.5, f64::INFINITY, f64::from(u32::MAX) + 1.0] {
            let error = convert_gpu_origin3d::<Engine>(cx, rt.set_like(&[rt.number(value)]))
                .expect_err("invalid coordinate");
            assert_eq!(error, "TypeError: coordinate");
        }
    }

    #[test]
    fn t1_union_propagates_iterator_failures() {
        let rt = runtime();
        let cx = rt.context();
        let iterable = rt.throwing_iterable(&[rt.number(1.0), rt.number(2.0)], 1);
        assert_eq!(
            convert_gpu_extent3d::<Engine>(cx, iterable).expect_err("iterator throw"),
            "iterator next 1 failed"
        );
    }

    #[test]
    fn t1_union_rejects_primitive_values_before_iterator_probe() {
        let rt = runtime();
        let cx = rt.context();
        assert_eq!(
            convert_gpu_extent3d::<Engine>(cx, rt.string("12"))
                .expect_err("primitive string must not select sequence arm"),
            "TypeError: GPUExtent3D must be an object"
        );
    }

    #[test]
    fn t2_texture_descriptor_converts_union_enums_flags_and_enum_sequence() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        let size = rt.set_like(&[rt.number(8.0), rt.number(4.0), rt.number(2.0)]);
        let view_formats = rt.set_like(&[rt.string("rgba8unorm"), rt.string("rgba8unorm-srgb")]);
        let value = descriptor(
            &rt,
            &[
                ("size", size),
                ("mipLevelCount", rt.number(3.0)),
                ("sampleCount", rt.number(4.0)),
                ("dimension", rt.string("2d")),
                ("format", rt.string("rgba8unorm")),
                ("usage", rt.number(20.0)),
                ("viewFormats", view_formats),
            ],
        );
        let native =
            convert_texture_descriptor::<Engine>(cx, value, &arena).expect("texture descriptor");
        assert_eq!(
            (
                native.size.width,
                native.size.height,
                native.size.depthOrArrayLayers
            ),
            (8, 4, 2)
        );
        assert_eq!((native.mipLevelCount, native.sampleCount), (3, 4));
        assert_eq!(
            native.dimension,
            crate::WGPUTextureDimension_WGPUTextureDimension_2D
        );
        assert_eq!(
            native.format,
            crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm
        );
        assert_eq!(native.usage, 20);
        assert_eq!(native.viewFormatCount, 2);
        assert_eq!(
            unsafe { *native.viewFormats },
            crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm
        );
    }

    #[test]
    fn t2_texture_descriptor_defaults_and_enum_errors_follow_webidl() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        let value = descriptor(
            &rt,
            &[
                ("size", descriptor(&rt, &[("width", rt.number(1.0))])),
                ("format", rt.string("r8unorm")),
                ("usage", rt.number(4.0)),
            ],
        );
        let native = convert_texture_descriptor::<Engine>(cx, value, &arena)
            .expect("default texture descriptor");
        assert_eq!((native.mipLevelCount, native.sampleCount), (1, 1));
        assert_eq!(
            native.dimension,
            crate::WGPUTextureDimension_WGPUTextureDimension_2D
        );
        assert_eq!(native.viewFormatCount, 0);
        assert!(native.viewFormats.is_null());

        let invalid = descriptor(
            &rt,
            &[
                ("size", descriptor(&rt, &[("width", rt.number(1.0))])),
                ("format", rt.string("not-a-format")),
                ("usage", rt.number(4.0)),
            ],
        );
        assert_eq!(
            convert_texture_descriptor::<Engine>(cx, invalid, &arena)
                .expect_err("format rejection"),
            "TypeError: GPUTextureFormat"
        );
    }

    #[test]
    fn t3_view_descriptor_absence_uses_c_undefined_sentinels() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        let native = convert_texture_view_descriptor::<Engine>(cx, rt.undefined(), &arena)
            .expect("absent view descriptor");
        assert_eq!(
            native.format,
            crate::WGPUTextureFormat_WGPUTextureFormat_Undefined
        );
        assert_eq!(
            native.dimension,
            crate::WGPUTextureViewDimension_WGPUTextureViewDimension_Undefined
        );
        assert_eq!(
            native.aspect,
            crate::WGPUTextureAspect_WGPUTextureAspect_Undefined
        );
        assert_eq!(native.mipLevelCount, crate::WGPU_MIP_LEVEL_COUNT_UNDEFINED);
        assert_eq!(
            native.arrayLayerCount,
            crate::WGPU_ARRAY_LAYER_COUNT_UNDEFINED
        );
        assert_eq!(
            (native.baseMipLevel, native.baseArrayLayer, native.usage),
            (0, 0, 0)
        );
    }

    #[test]
    fn b4_null_non_nullable_labels_stringify_consistently() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        let buffer_desc = descriptor(
            &rt,
            &[
                ("size", rt.number(4.0)),
                ("usage", rt.number(8.0)),
                ("label", rt.null()),
            ],
        );
        let buffer = convert_buffer_descriptor::<Engine>(cx, buffer_desc, &arena)
            .expect("buffer descriptor");
        assert_eq!(buffer.label, "null");

        let shader_desc = descriptor(
            &rt,
            &[
                ("code", rt.string("@compute fn main() {}")),
                ("label", rt.null()),
            ],
        );
        let shader = convert_shader_module_descriptor::<Engine>(cx, shader_desc, &arena)
            .expect("shader descriptor");
        assert_eq!(read_view(shader.label), b"null");

        let empty = rt.set_like(&[]);
        let bind_group_layout_desc = descriptor(&rt, &[("label", rt.null()), ("entries", empty)]);
        let bind_group_layout =
            convert_bind_group_layout_descriptor::<Engine>(cx, bind_group_layout_desc, &arena)
                .expect("bind group layout descriptor");
        assert_eq!(read_view(bind_group_layout.label), b"null");

        let pipeline_layout_desc =
            descriptor(&rt, &[("label", rt.null()), ("bindGroupLayouts", empty)]);
        let pipeline_layout =
            convert_pipeline_layout_descriptor::<Engine>(cx, pipeline_layout_desc, &arena)
                .expect("pipeline layout descriptor");
        assert_eq!(read_view(pipeline_layout.label), b"null");

        let layout_value = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(41),
                parent_pipeline: None,
            }),
        )
        .expect("bind group layout");
        let bind_group_desc = descriptor(
            &rt,
            &[
                ("label", rt.null()),
                ("layout", layout_value),
                ("entries", empty),
            ],
        );
        let bind_group = convert_bind_group_descriptor::<Engine>(cx, bind_group_desc, &arena)
            .expect("bind group descriptor");
        assert_eq!(read_view(bind_group.native.label), b"null");

        let module = Engine::new_instance(
            cx,
            crate::GPU_SHADER_MODULE_CLASS,
            Box::new(ShaderModulePayload {
                module: fake_handle(42),
            }),
        )
        .expect("module");
        let compute = descriptor(&rt, &[("module", module)]);
        let compute_pipeline_desc = descriptor(
            &rt,
            &[
                ("label", rt.null()),
                ("layout", rt.string("auto")),
                ("compute", compute),
            ],
        );
        let compute_pipeline =
            convert_compute_pipeline_descriptor::<Engine>(cx, compute_pipeline_desc, &arena)
                .expect("compute pipeline descriptor");
        assert_eq!(read_view(compute_pipeline.native.label), b"null");

        let label_only = descriptor(&rt, &[("label", rt.null())]);
        assert_eq!(
            read_view(
                convert_command_encoder_descriptor::<Engine>(cx, label_only, &arena)
                    .expect("command encoder descriptor")
                    .label
            ),
            b"null"
        );
        assert_eq!(
            read_view(
                convert_command_buffer_descriptor::<Engine>(cx, label_only, &arena)
                    .expect("command buffer descriptor")
                    .label
            ),
            b"null"
        );
        assert_eq!(
            read_view(
                convert_compute_pass_descriptor::<Engine>(cx, label_only, &arena)
                    .expect("compute pass descriptor")
                    .label
            ),
            b"null"
        );
    }

    #[test]
    fn g12_sampler_create_label_and_release_are_balanced() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("label", rt.string("created"))]);
        let sampler = device_create_sampler::<Engine>(cx, device, &[desc]).expect("sampler");

        let label = crate::sampler_label_get::<Engine>(cx, sampler).expect("label getter");
        assert!(matches!(rt.get(label), MockValue::String(value) if value == "created"));
        crate::sampler_label_set::<Engine>(cx, sampler, rt.string("renamed"))
            .expect("label setter");
        let label = crate::sampler_label_get::<Engine>(cx, sampler).expect("updated label");
        assert!(matches!(rt.get(label), MockValue::String(value) if value == "renamed"));

        let payload = Engine::payload(cx, sampler, crate::GPU_SAMPLER_CLASS)
            .and_then(|payload| payload.downcast_ref::<SamplerPayload>())
            .expect("sampler payload");
        finalize_sampler(
            Box::new(SamplerPayload {
                sampler: payload.sampler,
                label: Mutex::new("renamed".to_owned()),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("sampler release"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.sampler_descriptors.len(), 1);
            assert_eq!(state.sampler_descriptors[0].label, b"created");
            assert_eq!(
                state.labels.last().map(Vec::as_slice),
                Some(b"renamed".as_slice())
            );
            assert_eq!(state.sampler_add_refs, 0);
            assert_eq!(state.sampler_releases, 1);
        });
    }

    #[test]
    fn t2_t3_texture_attributes_destroy_view_retention_and_releases_are_balanced() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                (
                    "size",
                    descriptor(
                        &rt,
                        &[
                            ("width", rt.number(8.0)),
                            ("height", rt.number(4.0)),
                            ("depthOrArrayLayers", rt.number(2.0)),
                        ],
                    ),
                ),
                ("mipLevelCount", rt.number(3.0)),
                ("format", rt.string("rgba8unorm")),
                ("usage", rt.number(20.0)),
            ],
        );
        let texture = device_create_texture::<Engine>(cx, device, &[desc]).expect("texture");

        for (value, expected) in [
            (texture_width_get::<Engine>(cx, texture), 8.0),
            (texture_height_get::<Engine>(cx, texture), 4.0),
            (
                texture_depth_or_array_layers_get::<Engine>(cx, texture),
                2.0,
            ),
            (texture_mip_level_count_get::<Engine>(cx, texture), 3.0),
            (texture_sample_count_get::<Engine>(cx, texture), 1.0),
            (texture_usage_get::<Engine>(cx, texture), 20.0),
        ] {
            let value = value.expect("numeric texture getter");
            assert!(matches!(rt.get(value), MockValue::Number(actual) if actual == expected));
        }
        let dimension = texture_dimension_get::<Engine>(cx, texture).expect("dimension");
        assert!(matches!(rt.get(dimension), MockValue::String(value) if value == "2d"));
        let format = texture_format_get::<Engine>(cx, texture).expect("format");
        assert!(matches!(rt.get(format), MockValue::String(value) if value == "rgba8unorm"));

        crate::texture_destroy::<Engine>(cx, texture, &[]).expect("destroy");
        crate::texture_destroy::<Engine>(cx, texture, &[]).expect("idempotent destroy");
        let view = crate::texture_create_view::<Engine>(cx, texture, &[]).expect("default view");
        let texture_payload = Engine::payload(cx, texture, crate::GPU_TEXTURE_CLASS)
            .and_then(|payload| payload.downcast_ref::<TexturePayload>())
            .expect("texture payload");
        let view_payload = Engine::payload(cx, view, crate::GPU_TEXTURE_VIEW_CLASS)
            .and_then(|payload| payload.downcast_ref::<TextureViewPayload>())
            .expect("view payload");
        assert_eq!(view_payload.texture, texture_payload.texture);

        finalize_texture(
            Box::new(TexturePayload {
                texture: texture_payload.texture,
                destroyed: AtomicBool::new(true),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("texture release"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.texture_destroys, 1);
            assert_eq!(state.texture_add_refs, 1);
            assert_eq!(state.texture_releases, 1);
            assert_eq!(state.texture_view_descriptors.len(), 1);
            let view = &state.texture_view_descriptors[0];
            assert_eq!(
                view.format,
                crate::WGPUTextureFormat_WGPUTextureFormat_Undefined
            );
            assert_eq!(
                view.dimension,
                crate::WGPUTextureViewDimension_WGPUTextureViewDimension_Undefined
            );
            assert_eq!(
                view.aspect,
                crate::WGPUTextureAspect_WGPUTextureAspect_Undefined
            );
        });

        finalize_texture_view(
            Box::new(TextureViewPayload {
                texture_view: view_payload.texture_view,
                texture: view_payload.texture,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("view release"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.texture_view_releases, 1);
            assert_eq!(state.texture_releases, 2);
        });
    }

    #[test]
    fn r13_sampler_create_rejects_a_null_native_handle() {
        reset_gpu();
        GPU_STATE.with(|state| state.borrow_mut().null_create_sampler = true);
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        assert_eq!(
            device_create_sampler::<Engine>(cx, device, &[]).expect_err("null sampler must fail"),
            "OperationError: wgpuDeviceCreateSampler returned null"
        );
        GPU_STATE.with(|state| assert_eq!(state.borrow().sampler_releases, 0));
    }

    #[test]
    fn query_set_create_readback_destroy_release_and_occlusion_recording() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("type", rt.string("occlusion")),
                ("count", rt.number(4.0)),
                ("label", rt.string("created")),
            ],
        );
        let query_set = device_create_query_set::<Engine>(cx, device, &[desc]).expect("query set");

        let type_ = crate::query_set_type_get::<Engine>(cx, query_set).expect("type");
        assert!(matches!(rt.get(type_), MockValue::String(value) if value == "occlusion"));
        let count = crate::query_set_count_get::<Engine>(cx, query_set).expect("count");
        assert!(matches!(rt.get(count), MockValue::Number(value) if value == 4.0));
        let label = crate::query_set_label_get::<Engine>(cx, query_set).expect("label");
        assert!(matches!(rt.get(label), MockValue::String(value) if value == "created"));
        crate::query_set_label_set::<Engine>(cx, query_set, rt.string("renamed"))
            .expect("set label");

        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let pass_desc = descriptor(
            &rt,
            &[
                ("colorAttachments", rt.set_like(&[])),
                ("occlusionQuerySet", query_set),
            ],
        );
        let pass = crate::command_encoder_begin_render_pass::<Engine>(cx, encoder, &[pass_desc])
            .expect("render pass");
        crate::render_pass_begin_occlusion_query::<Engine>(cx, pass, &[rt.number(2.0)])
            .expect("begin query");
        crate::render_pass_end_occlusion_query::<Engine>(cx, pass, &[]).expect("end query");
        crate::render_pass_end::<Engine>(cx, pass, &[]).expect("end pass");

        crate::query_set_destroy::<Engine>(cx, query_set, &[]).expect("destroy");
        crate::query_set_destroy::<Engine>(cx, query_set, &[]).expect("idempotent destroy");
        let payload = Engine::payload(cx, query_set, crate::GPU_QUERY_SET_CLASS)
            .and_then(|payload| payload.downcast_ref::<QuerySetPayload>())
            .expect("query set payload");
        finalize_query_set(
            Box::new(QuerySetPayload {
                query_set: payload.query_set,
                destroyed: AtomicBool::new(true),
                label: Mutex::new(payload.label.lock().expect("label lock").clone()),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("query set release"), 1);

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.query_set_descriptors.len(), 1);
            assert_eq!(state.query_set_descriptors[0].count, 4);
            assert_eq!(state.query_set_destroys, 1);
            assert_eq!(state.query_set_add_refs, 0);
            assert_eq!(state.query_set_releases, 1);
            assert_eq!(state.recording_calls.get("begin_occlusion_query"), Some(&1));
            assert_eq!(state.recording_calls.get("end_occlusion_query"), Some(&1));
        });

        let bad = descriptor(
            &rt,
            &[
                ("type", rt.string("not-a-query-type")),
                ("count", rt.number(1.0)),
            ],
        );
        assert_eq!(
            convert_query_set_descriptor::<Engine>(cx, bad, &Arena::new())
                .expect_err("unknown enum must fail"),
            "TypeError: GPUQueryType"
        );
    }

    #[test]
    fn r8_rejects_missing_size() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("usage", rt.number(8.0))]);
        let arena = Arena::new();
        assert_eq!(
            convert_buffer_descriptor::<Engine>(cx, desc, &arena)
                .expect_err("missing size must fail"),
            "TypeError: size"
        );
    }

    #[test]
    fn r8_rejects_missing_usage() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("size", rt.number(256.0))]);
        let arena = Arena::new();
        assert_eq!(
            convert_buffer_descriptor::<Engine>(cx, desc, &arena)
                .expect_err("missing usage must fail"),
            "TypeError: usage"
        );
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
    fn b4_optional_entry_point_preserves_absence_and_stringifies_null() {
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
        assert_eq!(read_view(shader.label), b"null");

        let absent_compute = descriptor(&rt, &[("module", module)]);
        let absent_desc = descriptor(
            &rt,
            &[("layout", rt.string("auto")), ("compute", absent_compute)],
        );
        let absent = convert_compute_pipeline_descriptor::<Engine>(cx, absent_desc, &arena)
            .expect("pipeline descriptor");
        assert!(absent.native.compute.entryPoint.data.is_null());
        assert_eq!(
            absent.native.compute.entryPoint.length,
            crate::wgpu_strlen()
        );

        let null_compute = descriptor(&rt, &[("module", module), ("entryPoint", rt.null())]);
        let null_desc = descriptor(
            &rt,
            &[("layout", rt.string("auto")), ("compute", null_compute)],
        );
        let null = convert_compute_pipeline_descriptor::<Engine>(cx, null_desc, &arena)
            .expect("pipeline descriptor");
        assert!(!null.native.compute.entryPoint.data.is_null());
        assert_eq!(read_view(null.native.compute.entryPoint), b"null");

        let empty_compute = descriptor(&rt, &[("module", module), ("entryPoint", rt.string(""))]);
        let empty_desc = descriptor(
            &rt,
            &[("layout", rt.string("auto")), ("compute", empty_compute)],
        );
        let empty = convert_compute_pipeline_descriptor::<Engine>(cx, empty_desc, &arena)
            .expect("pipeline descriptor");
        assert!(!empty.native.compute.entryPoint.data.is_null());
        assert_eq!(empty.native.compute.entryPoint.length, 0);
    }

    #[test]
    fn compute_pipeline_auto_layout_emits_a_null_native_handle() {
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
        let compute = descriptor(&rt, &[("module", module)]);
        let desc = descriptor(&rt, &[("layout", rt.string("auto")), ("compute", compute)]);
        let arena = Arena::new();

        let converted = convert_compute_pipeline_descriptor::<Engine>(cx, desc, &arena)
            .expect("auto layout descriptor");

        assert!(converted.native.layout.is_null());
        assert!(converted.layout.is_null());
    }

    #[test]
    fn compute_pipeline_layout_is_required_and_non_null() {
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
        let compute = descriptor(&rt, &[("module", module)]);
        let arena = Arena::new();

        for layout in [None, Some(rt.null())] {
            let desc = match layout {
                Some(layout) => descriptor(&rt, &[("layout", layout), ("compute", compute)]),
                None => descriptor(&rt, &[("compute", compute)]),
            };
            assert_eq!(
                convert_compute_pipeline_descriptor::<Engine>(cx, desc, &arena)
                    .err()
                    .expect("missing or null layout must fail"),
                "TypeError: layout"
            );
        }
    }

    #[test]
    fn present_skip_policied_descriptor_members_are_rejected() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();

        let shader = descriptor(
            &rt,
            &[
                (
                    "code",
                    rt.string("@compute @workgroup_size(1) fn main() {}"),
                ),
                ("compilationHints", descriptor(&rt, &[])),
            ],
        );
        assert_eq!(
            convert_shader_module_descriptor::<Engine>(cx, shader, &arena)
                .expect_err("present compilationHints must fail"),
            "TypeError: compilationHints are not supported yet"
        );

        let pass = descriptor(&rt, &[("timestampWrites", descriptor(&rt, &[]))]);
        assert_eq!(
            convert_compute_pass_descriptor::<Engine>(cx, pass, &arena)
                .expect_err("present timestampWrites must fail"),
            "TypeError: timestampWrites are not supported yet"
        );
    }

    #[test]
    fn b5_empty_and_nonempty_sequences_have_valid_count_pointer_shapes() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();

        let empty_entries = rt.set_like(&[]);
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
        let entries = rt.set_like(&[entry]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        let one = convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
            .expect("one-entry layout");
        assert_eq!(one.entryCount, 1);
        assert!(!one.entries.is_null());
        assert_eq!(unsafe { (*one.entries).binding }, 0);
    }

    #[test]
    fn bind_group_layout_descriptor_requires_entries() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_layout_descriptor::<Engine>(cx, descriptor(&rt, &[]), &arena)
                .expect_err("absent entries must be rejected"),
            "TypeError: entries"
        );
    }

    #[test]
    fn pipeline_layout_descriptor_requires_bind_group_layouts() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();

        assert_eq!(
            convert_pipeline_layout_descriptor::<Engine>(cx, descriptor(&rt, &[]), &arena)
                .expect_err("absent bindGroupLayouts must be rejected"),
            "TypeError: bindGroupLayouts"
        );
    }

    #[test]
    fn j11_array_like_sequence_is_rejected_as_not_iterable() {
        let rt = runtime();
        let first = Engine::new_instance(
            rt.context(),
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(41),
                parent_pipeline: None,
            }),
        )
        .expect("first layout");
        let second = Engine::new_instance(
            rt.context(),
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(42),
                parent_pipeline: None,
            }),
        )
        .expect("second layout");
        let array_like = descriptor(
            &rt,
            &[("length", rt.number(2.0)), ("0", first), ("1", second)],
        );
        let desc = descriptor(&rt, &[("bindGroupLayouts", array_like)]);
        let arena = Arena::new();

        rt.with_scope(|cx| {
            assert_eq!(
                convert_pipeline_layout_descriptor::<Engine>(cx, desc, &arena)
                    .expect_err("array-like must be rejected"),
                "TypeError: bindGroupLayouts is not iterable"
            );
        });
        assert_eq!(arena.allocations.borrow().len(), 0);
    }

    #[test]
    fn j11_set_like_sequence_is_accepted_in_iteration_order() {
        let rt = runtime();
        let first_handle = fake_handle(51);
        let second_handle = fake_handle(52);
        let first = Engine::new_instance(
            rt.context(),
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: first_handle,
                parent_pipeline: None,
            }),
        )
        .expect("first layout");
        let second = Engine::new_instance(
            rt.context(),
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: second_handle,
                parent_pipeline: None,
            }),
        )
        .expect("second layout");
        let set_like = rt.set_like(&[first, second]);
        let desc = descriptor(&rt, &[("bindGroupLayouts", set_like)]);
        let arena = Arena::new();

        rt.with_scope(|cx| {
            let converted = convert_pipeline_layout_descriptor::<Engine>(cx, desc, &arena)
                .expect("set-like sequence");
            assert_eq!(converted.bindGroupLayoutCount, 2);
            let layouts = unsafe {
                std::slice::from_raw_parts(
                    converted.bindGroupLayouts,
                    converted.bindGroupLayoutCount,
                )
            };
            assert_eq!(layouts, [first_handle, second_handle]);
        });
    }

    #[test]
    fn b5_bind_group_layout_entry_requires_binding_and_visibility() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        for (entry, message) in [
            (
                descriptor(&rt, &[("visibility", rt.number(1.0))]),
                "TypeError: binding",
            ),
            (
                descriptor(&rt, &[("binding", rt.number(0.0))]),
                "TypeError: visibility",
            ),
        ] {
            let entries = rt.set_like(&[entry]);
            let desc = descriptor(&rt, &[("entries", entries)]);
            assert_eq!(
                convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
                    .expect_err("missing required layout entry member"),
                message
            );
        }

        let entry = descriptor(
            &rt,
            &[("binding", rt.number(0.0)), ("visibility", rt.number(1.0))],
        );
        rt.set_property_error(entry, "visibility", "visibility getter failed");
        let entries = rt.set_like(&[entry]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        assert_eq!(
            convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
                .expect_err("getter failure must propagate"),
            "visibility getter failed"
        );
    }

    fn assert_unsupported_layout_binding(kind: &str) {
        let rt = runtime();
        let cx = rt.context();
        let unsupported = descriptor(&rt, &[]);
        let entry = descriptor(
            &rt,
            &[
                ("binding", rt.number(0.0)),
                ("visibility", rt.number(1.0)),
                (kind, unsupported),
            ],
        );
        let entries = rt.set_like(&[entry]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
                .expect_err("unsupported layout binding must be rejected"),
            format!("TypeError: {kind} bindings are not supported yet")
        );
    }

    #[test]
    fn sampler_texture_and_storage_texture_layout_bindings_convert_nested_members() {
        let rt = runtime();
        let cx = rt.context();
        let entries = rt.set_like(&[
            descriptor(
                &rt,
                &[
                    ("binding", rt.number(0.0)),
                    ("visibility", rt.number(1.0)),
                    (
                        "sampler",
                        descriptor(&rt, &[("type", rt.string("comparison"))]),
                    ),
                ],
            ),
            descriptor(
                &rt,
                &[
                    ("binding", rt.number(1.0)),
                    ("visibility", rt.number(2.0)),
                    (
                        "texture",
                        descriptor(
                            &rt,
                            &[
                                ("sampleType", rt.string("uint")),
                                ("viewDimension", rt.string("3d")),
                                ("multisampled", rt.bool(true)),
                            ],
                        ),
                    ),
                ],
            ),
            descriptor(
                &rt,
                &[
                    ("binding", rt.number(2.0)),
                    ("visibility", rt.number(4.0)),
                    (
                        "storageTexture",
                        descriptor(
                            &rt,
                            &[
                                ("access", rt.string("read-write")),
                                ("format", rt.string("rgba8unorm")),
                                ("viewDimension", rt.string("2d-array")),
                            ],
                        ),
                    ),
                ],
            ),
        ]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        let arena = Arena::new();
        let converted = convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
            .expect("supported nested layout bindings");
        let native = unsafe { std::slice::from_raw_parts(converted.entries, converted.entryCount) };

        assert_eq!(
            native[0].sampler.type_,
            crate::WGPUSamplerBindingType_WGPUSamplerBindingType_Comparison
        );
        assert_eq!(
            native[1].texture.sampleType,
            crate::WGPUTextureSampleType_WGPUTextureSampleType_Uint
        );
        assert_eq!(
            native[1].texture.viewDimension,
            crate::WGPUTextureViewDimension_WGPUTextureViewDimension_3D
        );
        assert_eq!(native[1].texture.multisampled, 1);
        assert_eq!(
            native[2].storageTexture.access,
            crate::WGPUStorageTextureAccess_WGPUStorageTextureAccess_ReadWrite
        );
        assert_eq!(
            native[2].storageTexture.format,
            crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm
        );
        assert_eq!(
            native[2].storageTexture.viewDimension,
            crate::WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray
        );
    }

    #[test]
    fn new_layout_binding_enums_reject_unknown_values() {
        let rt = runtime();
        let cx = rt.context();
        let cases = [
            (
                convert_sampler_binding_layout::<Engine>(
                    cx,
                    descriptor(&rt, &[("type", rt.string("bad"))]),
                )
                .expect_err("sampler type"),
                "TypeError: GPUSamplerBindingType",
            ),
            (
                convert_texture_binding_layout::<Engine>(
                    cx,
                    descriptor(&rt, &[("sampleType", rt.string("bad"))]),
                )
                .expect_err("sample type"),
                "TypeError: GPUTextureSampleType",
            ),
            (
                convert_texture_binding_layout::<Engine>(
                    cx,
                    descriptor(&rt, &[("viewDimension", rt.string("bad"))]),
                )
                .expect_err("texture view dimension"),
                "TypeError: GPUTextureViewDimension",
            ),
            (
                convert_storage_texture_binding_layout::<Engine>(
                    cx,
                    descriptor(
                        &rt,
                        &[
                            ("access", rt.string("bad")),
                            ("format", rt.string("rgba8unorm")),
                        ],
                    ),
                )
                .expect_err("storage access"),
                "TypeError: GPUStorageTextureAccess",
            ),
            (
                convert_storage_texture_binding_layout::<Engine>(
                    cx,
                    descriptor(&rt, &[("format", rt.string("bad"))]),
                )
                .expect_err("storage format"),
                "TypeError: GPUTextureFormat",
            ),
            (
                convert_storage_texture_binding_layout::<Engine>(
                    cx,
                    descriptor(
                        &rt,
                        &[
                            ("format", rt.string("rgba8unorm")),
                            ("viewDimension", rt.string("bad")),
                        ],
                    ),
                )
                .expect_err("storage view dimension"),
                "TypeError: GPUTextureViewDimension",
            ),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn external_texture_layout_binding_is_rejected_early() {
        assert_unsupported_layout_binding("externalTexture");
    }

    #[test]
    fn absent_layout_binding_members_keep_binding_not_used_zeroes() {
        let rt = runtime();
        let cx = rt.context();
        let entry = descriptor(
            &rt,
            &[("binding", rt.number(0.0)), ("visibility", rt.number(1.0))],
        );
        let entries = rt.set_like(&[entry]);
        let desc = descriptor(&rt, &[("entries", entries)]);
        let arena = Arena::new();
        let converted = convert_bind_group_layout_descriptor::<Engine>(cx, desc, &arena)
            .expect("absent optional binding kinds");
        let native = unsafe { &*converted.entries };

        assert_eq!(
            native.buffer.type_,
            crate::WGPUBufferBindingType_WGPUBufferBindingType_BindingNotUsed
        );
        assert_eq!(
            native.sampler.type_,
            crate::WGPUSamplerBindingType_WGPUSamplerBindingType_BindingNotUsed
        );
        assert_eq!(
            native.texture.sampleType,
            crate::WGPUTextureSampleType_WGPUTextureSampleType_BindingNotUsed
        );
        assert_eq!(
            native.storageTexture.access,
            crate::WGPUStorageTextureAccess_WGPUStorageTextureAccess_BindingNotUsed
        );
        assert_eq!(native.bindingArraySize, 0);
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
    fn b7_write_buffer_method_rejects_size_that_would_truncate_on_32_bit_hosts() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(8.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let data = Engine::new_arraybuffer_copy(cx, &[0; 8]).expect("arraybuffer");
        let error = queue_write_buffer::<Engine>(
            cx,
            queue,
            &[
                buffer,
                rt.number(0.0),
                data,
                rt.number(0.0),
                rt.number(4_294_967_296.0),
            ],
        )
        .expect_err("oversized size must fail before narrowing");

        assert_eq!(error, "TypeError: size");
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_keeps_arraybuffer_whole_buffer_behavior() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let native = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let data = Engine::new_arraybuffer_copy(cx, &[1, 2, 3, 4]).expect("arraybuffer");

        queue_write_buffer::<Engine>(cx, queue, &[buffer, rt.number(0.0), data])
            .expect("whole ArrayBuffer write");

        assert_eq!(buffer_bytes(native).expect("bytes"), [1, 2, 3, 4]);
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_respects_uint8array_view_window() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let native = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let backing =
            Engine::new_arraybuffer_copy(cx, &[90, 91, 1, 2, 3, 4, 92, 93]).expect("arraybuffer");
        let view = array_buffer_view(&rt, backing, 2.0, 4.0, Some(1.0));

        queue_write_buffer::<Engine>(cx, queue, &[buffer, rt.number(0.0), view])
            .expect("Uint8Array write");

        assert_eq!(buffer_bytes(native).expect("bytes"), [1, 2, 3, 4]);
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_uses_elements_for_uint16array_data_offset_and_size() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let native = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let backing =
            Engine::new_arraybuffer_copy(cx, &[99, 98, 10, 11, 20, 21, 30, 31, 40, 41, 97, 96])
                .expect("arraybuffer");
        let view = array_buffer_view(&rt, backing, 2.0, 8.0, Some(2.0));

        queue_write_buffer::<Engine>(
            cx,
            queue,
            &[buffer, rt.number(0.0), view, rt.number(1.0), rt.number(2.0)],
        )
        .expect("Uint16Array element write");

        assert_eq!(buffer_bytes(native).expect("bytes"), [20, 21, 30, 31]);
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_uses_bytes_for_dataview_data_offset_and_size() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let native = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let backing =
            Engine::new_arraybuffer_copy(cx, &[0, 1, 2, 3, 4, 5, 6]).expect("arraybuffer");
        let view = array_buffer_view(&rt, backing, 1.0, 5.0, None);

        queue_write_buffer::<Engine>(
            cx,
            queue,
            &[buffer, rt.number(0.0), view, rt.number(2.0), rt.number(2.0)],
        )
        .expect("DataView byte write");

        assert_eq!(buffer_bytes(native).expect("bytes"), [3, 4, 0, 0]);
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_rejects_non_buffer_source() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");

        let error =
            queue_write_buffer::<Engine>(cx, queue, &[buffer, rt.number(0.0), rt.object(&[])])
                .expect_err("non-BufferSource must fail");

        assert_eq!(
            error,
            "TypeError: data must be an ArrayBuffer or ArrayBufferView (or pass data.buffer)"
        );
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b22_write_buffer_rejects_bounds_violations_before_narrowing() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let desc = descriptor(&rt, &[("size", rt.number(8.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let data = Engine::new_arraybuffer_copy(cx, &[0; 8]).expect("arraybuffer");

        for (args, expected) in [
            (
                vec![buffer, rt.number(0.0), data, rt.number(9.0)],
                "TypeError: dataOffset",
            ),
            (
                vec![buffer, rt.number(0.0), data, rt.number(4.0), rt.number(5.0)],
                "TypeError: size",
            ),
            (
                vec![buffer, rt.number(0.0), data, rt.number(4_294_967_296.0)],
                "TypeError: dataOffset",
            ),
            (
                vec![
                    buffer,
                    rt.number(0.0),
                    data,
                    rt.number(0.0),
                    rt.number(4_294_967_296.0),
                ],
                "TypeError: size",
            ),
        ] {
            assert_eq!(
                queue_write_buffer::<Engine>(cx, queue, &args)
                    .expect_err("bounds violation must fail"),
                expected
            );
        }
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn b21_device_queue_is_same_object_with_one_persistent_hold() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let first = device_queue_get::<Engine>(cx, device).expect("queue");
        let second = device_queue_get::<Engine>(cx, device).expect("cached queue");
        assert_eq!(first, second);

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.device_get_queue_calls, 1);
            assert_eq!(state.queue_add_refs, 0);
            assert_eq!(state.queue_releases, 0);
        });
        assert_eq!(rt.held_returns.get(), 1);
        assert_eq!(rt.duplicated_values.borrow().get(&first), Some(&1));

        let payload = Engine::payload(cx, device, crate::GPU_DEVICE_CLASS)
            .and_then(|payload| payload.downcast_ref::<DevicePayload<Engine>>())
            .expect("device payload");
        let mut released = 0;
        crate::release_payload_values::<Engine>(payload, &mut |value| {
            released += 1;
            Engine::release_value(cx, value);
        });
        crate::release_payload_values::<Engine>(payload, &mut |_| released += 1);
        assert_eq!(released, 2);
        assert!(rt.duplicated_values.borrow().is_empty());

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
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let first_resource = descriptor(&rt, &[("buffer", buffer)]);
        let first_entry = descriptor(
            &rt,
            &[("binding", rt.number(0.0)), ("resource", first_resource)],
        );
        let bad_entry = descriptor(&rt, &[("binding", rt.number(1.0))]);
        let entries = rt.set_like(&[first_entry, bad_entry]);
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("missing resource must fail"),
            "TypeError: resource"
        );
        assert_eq!(arena.allocations.borrow().len(), 0);
        assert_eq!(
            device_create_bind_group::<Engine>(cx, device, &[desc])
                .expect_err("createBindGroup must preserve the member error"),
            "TypeError: resource"
        );
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().buffer_add_refs, 0);
        });
    }

    #[test]
    fn bind_group_entry_requires_binding() {
        let rt = runtime();
        let cx = rt.context();
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let resource = descriptor(&rt, &[]);
        let entry = descriptor(&rt, &[("resource", resource)]);
        let desc = descriptor(
            &rt,
            &[("layout", layout), ("entries", rt.set_like(&[entry]))],
        );
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("absent binding must be rejected"),
            "TypeError: binding"
        );
    }

    #[test]
    fn bind_group_entry_requires_resource() {
        let rt = runtime();
        let cx = rt.context();
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let entry = descriptor(&rt, &[("binding", rt.number(0.0))]);
        let desc = descriptor(
            &rt,
            &[("layout", layout), ("entries", rt.set_like(&[entry]))],
        );
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("absent resource must be rejected"),
            "TypeError: resource"
        );
    }

    #[test]
    fn bind_group_descriptor_requires_layout() {
        let rt = runtime();
        let cx = rt.context();
        let desc = descriptor(&rt, &[("entries", rt.set_like(&[]))]);
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("absent layout must be rejected"),
            "TypeError: layout"
        );
    }

    #[test]
    fn bind_group_descriptor_requires_entries() {
        let rt = runtime();
        let cx = rt.context();
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let desc = descriptor(&rt, &[("layout", layout)]);
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("absent entries must be rejected"),
            "TypeError: entries"
        );
    }

    #[test]
    fn bind_group_entries_accept_sampler_and_texture_view_wrappers() {
        let rt = runtime();
        let cx = rt.context();
        let sampler_handle = fake_handle(81);
        let view_handle = fake_handle(82);
        let sampler = Engine::new_instance(
            cx,
            crate::GPU_SAMPLER_CLASS,
            Box::new(SamplerPayload {
                sampler: sampler_handle,
                label: Mutex::new(String::new()),
            }),
        )
        .expect("sampler");
        let view = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_VIEW_CLASS,
            Box::new(TextureViewPayload {
                texture_view: view_handle,
                texture: fake_handle(83),
            }),
        )
        .expect("texture view");

        let sampler_entry = descriptor(&rt, &[("binding", rt.number(0.0)), ("resource", sampler)]);
        let view_entry = descriptor(&rt, &[("binding", rt.number(1.0)), ("resource", view)]);
        let mut created = crate::CreatedTextureViewCapture::new::<Engine>(cx);
        let sampler_native = convert_bind_group_entry::<Engine>(cx, sampler_entry, &mut created)
            .expect("sampler resource");
        let view_native = convert_bind_group_entry::<Engine>(cx, view_entry, &mut created)
            .expect("texture view resource");

        assert_eq!(sampler_native.sampler, sampler_handle);
        assert!(sampler_native.buffer.is_null());
        assert!(sampler_native.textureView.is_null());
        assert_eq!(view_native.textureView, view_handle);
        assert!(view_native.buffer.is_null());
        assert!(view_native.sampler.is_null());
    }

    #[test]
    fn direct_buffer_and_texture_bindings_flatten_and_own_implicit_views() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(64.0)), ("usage", rt.number(8.0))],
            )],
        )
        .expect("buffer");
        let buffer_handle = crate::buffer_handle::<Engine>(cx, buffer).expect("buffer handle");
        let texture = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_CLASS,
            Box::new(TexturePayload {
                texture: fake_handle(840),
                destroyed: AtomicBool::new(false),
            }),
        )
        .expect("texture");
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(841),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let entries = rt.set_like(&[
            descriptor(&rt, &[("binding", rt.number(0.0)), ("resource", buffer)]),
            descriptor(&rt, &[("binding", rt.number(1.0)), ("resource", texture)]),
        ]);
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);
        let arena = Arena::new();
        let converted = convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
            .expect("direct binding resources");
        let native_entries = unsafe {
            std::slice::from_raw_parts(converted.native.entries, converted.native.entryCount)
        };
        assert_eq!(native_entries[0].buffer, buffer_handle);
        assert_eq!(native_entries[0].offset, 0);
        assert_eq!(native_entries[0].size, crate::WGPU_WHOLE_SIZE as u64);
        assert!(!native_entries[1].textureView.is_null());
        assert_eq!(converted.buffers, vec![buffer_handle]);
        assert!(converted.texture_views.is_empty());
        assert_eq!(
            converted.created_texture_views,
            vec![native_entries[1].textureView]
        );
        GPU_STATE.with(|state| assert_eq!(state.borrow().null_texture_view_descriptors, 1));
        for texture_view in converted.created_texture_views {
            rt.queue()
                .enqueue(crate::ReleaseRequest::TextureViewOnly {
                    texture_view,
                    gpu: rt.env.gpu(),
                })
                .expect("queue converted view release");
        }
        assert_eq!(rt.queue().drain().expect("release converted view"), 1);

        let bind_group =
            device_create_bind_group::<Engine>(cx, device, &[desc]).expect("bind group");
        GPU_STATE.with(|state| assert_eq!(state.borrow().texture_view_add_refs, 0));
        let payload = Engine::payload(cx, bind_group, crate::GPU_BIND_GROUP_CLASS)
            .and_then(|payload| payload.downcast_ref::<BindGroupPayload>())
            .expect("bind group payload");
        finalize_bind_group(
            Box::new(BindGroupPayload {
                bind_group: payload.bind_group,
                layout: payload.layout,
                buffers: payload.buffers.clone(),
                samplers: payload.samplers.clone(),
                texture_views: payload.texture_views.clone(),
                created_texture_views: payload.created_texture_views.clone(),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("release bind group"), 1);
        GPU_STATE.with(|state| assert_eq!(state.borrow().texture_view_releases, 2));

        let bad_entry = descriptor(&rt, &[("binding", rt.number(-1.0)), ("resource", texture)]);
        let bad_desc = descriptor(
            &rt,
            &[("layout", layout), ("entries", rt.set_like(&[bad_entry]))],
        );
        assert!(convert_bind_group_descriptor::<Engine>(cx, bad_desc, &arena).is_err());
        assert_eq!(
            rt.queue().drain().expect("release failed conversion view"),
            1
        );
        GPU_STATE.with(|state| assert_eq!(state.borrow().texture_view_releases, 3));

        rt.fail_new_instance.set(Some(crate::GPU_BIND_GROUP_CLASS));
        assert!(device_create_bind_group::<Engine>(cx, device, &[desc]).is_err());
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.texture_view_add_refs, 0);
            assert_eq!(state.texture_view_releases, 4);
        });
    }

    #[test]
    fn unknown_bind_group_resource_kind_throws_a_named_type_error() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let resource = descriptor(&rt, &[("sampler", descriptor(&rt, &[]))]);
        let entry = descriptor(&rt, &[("binding", rt.number(0.0)), ("resource", resource)]);
        let entries = rt.set_like(&[entry]);
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);
        let arena = Arena::new();

        assert_eq!(
            convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("sampler resource must be rejected"),
            "TypeError: resource must be a GPUBindingResource"
        );
    }

    #[test]
    fn j11_mid_iteration_throw_leaks_no_addref_and_allocates_no_entries() {
        reset_gpu();
        let rt = runtime();
        let arena = Arena::new();

        rt.with_scope(|cx| {
            let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
            let buffer_desc =
                descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
            let buffer =
                device_create_buffer::<Engine>(cx, device, &[buffer_desc]).expect("buffer");
            let layout = Engine::new_instance(
                cx,
                crate::GPU_BIND_GROUP_LAYOUT_CLASS,
                Box::new(BindGroupLayoutPayload {
                    layout: fake_handle(77),
                    parent_pipeline: None,
                }),
            )
            .expect("layout");
            let resource = descriptor(&rt, &[("buffer", buffer)]);
            let entry = descriptor(&rt, &[("binding", rt.number(0.0)), ("resource", resource)]);
            let entries = rt.throwing_iterable(&[entry, entry], 1);
            let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);

            let error = convert_bind_group_descriptor::<Engine>(cx, desc, &arena)
                .err()
                .expect("second next() must throw");
            assert_eq!(error, "iterator next 1 failed");
            assert_eq!(arena.allocations.borrow().len(), 0);
            assert_eq!(
                device_create_bind_group::<Engine>(cx, device, &[desc])
                    .expect_err("createBindGroup must propagate next() throw"),
                "iterator next 1 failed"
            );
        });

        assert_eq!(arena.allocations.borrow().len(), 0);
        GPU_STATE.with(|state| assert_eq!(state.borrow().buffer_add_refs, 0));
        assert_eq!(rt.live_scoped_values(), 0);
        assert!(rt.calls.get() >= 4);
    }

    /// This asserts we call `AddRef` once per stored resource. It does not prove
    /// the backend needs it. The C ABI has no refcount introspection, so this
    /// is the strongest available check.
    #[test]
    fn b8_bind_group_balances_buffer_sampler_and_texture_view_retention() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer_desc = descriptor(&rt, &[("size", rt.number(16.0)), ("usage", rt.number(8.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[buffer_desc]).expect("buffer");
        let sampler = Engine::new_instance(
            cx,
            crate::GPU_SAMPLER_CLASS,
            Box::new(SamplerPayload {
                sampler: fake_handle(78),
                label: Mutex::new(String::new()),
            }),
        )
        .expect("sampler");
        let texture_view = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_VIEW_CLASS,
            Box::new(TextureViewPayload {
                texture_view: fake_handle(79),
                texture: fake_handle(80),
            }),
        )
        .expect("texture view");
        let layout = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_LAYOUT_CLASS,
            Box::new(BindGroupLayoutPayload {
                layout: fake_handle(77),
                parent_pipeline: None,
            }),
        )
        .expect("layout");
        let buffer_resource = descriptor(&rt, &[("buffer", buffer)]);
        let buffer_entry = descriptor(
            &rt,
            &[("binding", rt.number(0.0)), ("resource", buffer_resource)],
        );
        let sampler_entry = descriptor(&rt, &[("binding", rt.number(1.0)), ("resource", sampler)]);
        let view_entry = descriptor(
            &rt,
            &[("binding", rt.number(2.0)), ("resource", texture_view)],
        );
        let entries = rt.set_like(&[buffer_entry, sampler_entry, view_entry]);
        let desc = descriptor(&rt, &[("layout", layout), ("entries", entries)]);

        let bind_group =
            device_create_bind_group::<Engine>(cx, device, &[desc]).expect("bind group");

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.buffer_add_refs, 1);
            assert_eq!(state.sampler_add_refs, 1);
            assert_eq!(state.texture_view_add_refs, 1);
            assert_eq!(state.bind_group_layout_add_refs, 1);
        });
        let payload = Engine::payload(cx, bind_group, crate::GPU_BIND_GROUP_CLASS)
            .and_then(|payload| payload.downcast_ref::<BindGroupPayload>())
            .expect("bind group payload");
        finalize_bind_group(
            Box::new(BindGroupPayload {
                bind_group: payload.bind_group,
                layout: payload.layout,
                buffers: payload.buffers.clone(),
                samplers: payload.samplers.clone(),
                texture_views: payload.texture_views.clone(),
                created_texture_views: payload.created_texture_views.clone(),
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain retained handles"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.buffer_releases, 1);
            assert_eq!(state.sampler_releases, 1);
            assert_eq!(state.texture_view_releases, 1);
            assert_eq!(state.bind_group_layout_releases, 1);
            assert_eq!(state.bind_group_releases, 1);
        });
    }

    #[test]
    fn b8_compute_pipeline_balances_module_and_explicit_layout_refs() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let module = Engine::new_instance(
            cx,
            crate::GPU_SHADER_MODULE_CLASS,
            Box::new(ShaderModulePayload {
                module: fake_handle(42),
            }),
        )
        .expect("module");
        let layout = Engine::new_instance(
            cx,
            crate::GPU_PIPELINE_LAYOUT_CLASS,
            Box::new(PipelineLayoutPayload {
                layout: fake_handle(43),
            }),
        )
        .expect("layout");
        let compute = descriptor(&rt, &[("module", module)]);
        let desc = descriptor(&rt, &[("layout", layout), ("compute", compute)]);
        let pipeline = device_create_compute_pipeline::<Engine>(cx, device, &[desc])
            .expect("compute pipeline");

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_add_refs, 1);
            assert_eq!(state.pipeline_layout_add_refs, 1);
        });
        let payload = Engine::payload(cx, pipeline, crate::GPU_COMPUTE_PIPELINE_CLASS)
            .and_then(|payload| payload.downcast_ref::<ComputePipelinePayload>())
            .expect("compute pipeline payload");
        finalize_compute_pipeline(
            Box::new(ComputePipelinePayload {
                pipeline: payload.pipeline,
                module: payload.module,
                layout: payload.layout,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain retained handles"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_releases, 1);
            assert_eq!(state.pipeline_layout_releases, 1);
            assert_eq!(state.compute_pipeline_releases, 1);
        });
    }

    #[test]
    fn t5_full_render_pipeline_descriptor_converts_every_c_field_and_nullable_holes() {
        let rt = runtime();
        let cx = rt.context();
        let vertex_module = shader_module(&rt, cx, 71);
        let fragment_module = shader_module(&rt, cx, 72);
        let layout = Engine::new_instance(
            cx,
            crate::GPU_PIPELINE_LAYOUT_CLASS,
            Box::new(PipelineLayoutPayload {
                layout: fake_handle(73),
            }),
        )
        .expect("pipeline layout");

        let attribute = descriptor(
            &rt,
            &[
                ("format", rt.string("float32x3")),
                ("offset", rt.number(4.0)),
                ("shaderLocation", rt.number(7.0)),
            ],
        );
        let attributes = rt.set_like(&[attribute]);
        let buffer = descriptor(
            &rt,
            &[
                ("arrayStride", rt.number(24.0)),
                ("stepMode", rt.string("instance")),
                ("attributes", attributes),
            ],
        );
        let buffers = rt.set_like(&[buffer, rt.null()]);
        let vertex = descriptor(
            &rt,
            &[
                ("module", vertex_module),
                ("entryPoint", rt.string("vs_main")),
                ("buffers", buffers),
            ],
        );
        let primitive = descriptor(
            &rt,
            &[
                ("topology", rt.string("triangle-strip")),
                ("stripIndexFormat", rt.string("uint32")),
                ("frontFace", rt.string("cw")),
                ("cullMode", rt.string("back")),
                ("unclippedDepth", rt.bool(true)),
            ],
        );
        let stencil_front = descriptor(
            &rt,
            &[
                ("compare", rt.string("equal")),
                ("failOp", rt.string("replace")),
                ("depthFailOp", rt.string("increment-clamp")),
                ("passOp", rt.string("invert")),
            ],
        );
        let stencil_back = descriptor(
            &rt,
            &[
                ("compare", rt.string("greater")),
                ("failOp", rt.string("zero")),
                ("depthFailOp", rt.string("decrement-wrap")),
                ("passOp", rt.string("keep")),
            ],
        );
        let depth_stencil = descriptor(
            &rt,
            &[
                ("format", rt.string("depth24plus-stencil8")),
                ("depthWriteEnabled", rt.bool(true)),
                ("depthCompare", rt.string("less")),
                ("stencilFront", stencil_front),
                ("stencilBack", stencil_back),
                ("stencilReadMask", rt.number(0x1122_3344 as f64)),
                ("stencilWriteMask", rt.number(0x5566_7788 as f64)),
                ("depthBias", rt.number(-7.0)),
                ("depthBiasSlopeScale", rt.number(1.25)),
                ("depthBiasClamp", rt.number(2.5)),
            ],
        );
        let multisample = descriptor(
            &rt,
            &[
                ("count", rt.number(4.0)),
                ("mask", rt.number(0x1234_5678 as f64)),
                ("alphaToCoverageEnabled", rt.bool(true)),
            ],
        );
        let color_blend = descriptor(
            &rt,
            &[
                ("operation", rt.string("subtract")),
                ("srcFactor", rt.string("src-alpha")),
                ("dstFactor", rt.string("one-minus-dst-alpha")),
            ],
        );
        let alpha_blend = descriptor(
            &rt,
            &[
                ("operation", rt.string("reverse-subtract")),
                ("srcFactor", rt.string("one")),
                ("dstFactor", rt.string("zero")),
            ],
        );
        let blend = descriptor(&rt, &[("color", color_blend), ("alpha", alpha_blend)]);
        let target = descriptor(
            &rt,
            &[
                ("format", rt.string("rgba8unorm")),
                ("blend", blend),
                ("writeMask", rt.number(3.0)),
            ],
        );
        let targets = rt.set_like(&[target, rt.null()]);
        let fragment = descriptor(
            &rt,
            &[
                ("module", fragment_module),
                ("entryPoint", rt.string("fs_main")),
                ("targets", targets),
            ],
        );
        let value = descriptor(
            &rt,
            &[
                ("label", rt.string("full-render")),
                ("layout", layout),
                ("vertex", vertex),
                ("primitive", primitive),
                ("depthStencil", depth_stencil),
                ("multisample", multisample),
                ("fragment", fragment),
            ],
        );
        let arena = Arena::new();
        let converted = convert_render_pipeline_descriptor::<Engine>(cx, value, &arena)
            .expect("full render descriptor");
        let native = &converted.native;
        assert!(native.nextInChain.is_null());
        assert_eq!(read_view(native.label), b"full-render");
        assert_eq!(native.layout, fake_handle(73));
        assert!(native.vertex.nextInChain.is_null());
        assert_eq!(native.vertex.module, fake_handle(71));
        assert_eq!(read_view(native.vertex.entryPoint), b"vs_main");
        assert_eq!(native.vertex.constantCount, 0);
        assert!(native.vertex.constants.is_null());
        assert_eq!(native.vertex.bufferCount, 2);
        let native_buffers =
            unsafe { std::slice::from_raw_parts(native.vertex.buffers, native.vertex.bufferCount) };
        assert!(native_buffers[0].nextInChain.is_null());
        assert_eq!(
            native_buffers[0].stepMode,
            crate::WGPUVertexStepMode_WGPUVertexStepMode_Instance
        );
        assert_eq!(native_buffers[0].arrayStride, 24);
        assert_eq!(native_buffers[0].attributeCount, 1);
        let native_attribute = unsafe { &*native_buffers[0].attributes };
        assert!(native_attribute.nextInChain.is_null());
        assert_eq!(
            native_attribute.format,
            crate::WGPUVertexFormat_WGPUVertexFormat_Float32x3
        );
        assert_eq!(native_attribute.offset, 4);
        assert_eq!(native_attribute.shaderLocation, 7);
        assert!(native_buffers[1].nextInChain.is_null());
        assert_eq!(
            native_buffers[1].stepMode,
            crate::WGPUVertexStepMode_WGPUVertexStepMode_Undefined
        );
        assert_eq!(native_buffers[1].arrayStride, 0);
        assert_eq!(native_buffers[1].attributeCount, 0);
        assert!(native_buffers[1].attributes.is_null());

        assert!(native.primitive.nextInChain.is_null());
        assert_eq!(
            native.primitive.topology,
            crate::WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleStrip
        );
        assert_eq!(
            native.primitive.stripIndexFormat,
            crate::WGPUIndexFormat_WGPUIndexFormat_Uint32
        );
        assert_eq!(
            native.primitive.frontFace,
            crate::WGPUFrontFace_WGPUFrontFace_CW
        );
        assert_eq!(
            native.primitive.cullMode,
            crate::WGPUCullMode_WGPUCullMode_Back
        );
        assert_eq!(native.primitive.unclippedDepth, 1);

        let depth = unsafe { native.depthStencil.as_ref() }.expect("depth/stencil pointer");
        assert!(depth.nextInChain.is_null());
        assert_eq!(
            depth.format,
            crate::WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8
        );
        assert_eq!(
            depth.depthWriteEnabled,
            crate::WGPUOptionalBool_WGPUOptionalBool_True
        );
        assert_eq!(
            depth.depthCompare,
            crate::WGPUCompareFunction_WGPUCompareFunction_Less
        );
        assert_eq!(
            depth.stencilFront.compare,
            crate::WGPUCompareFunction_WGPUCompareFunction_Equal
        );
        assert_eq!(
            depth.stencilFront.failOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_Replace
        );
        assert_eq!(
            depth.stencilFront.depthFailOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_IncrementClamp
        );
        assert_eq!(
            depth.stencilFront.passOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_Invert
        );
        assert_eq!(
            depth.stencilBack.compare,
            crate::WGPUCompareFunction_WGPUCompareFunction_Greater
        );
        assert_eq!(
            depth.stencilBack.failOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_Zero
        );
        assert_eq!(
            depth.stencilBack.depthFailOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_DecrementWrap
        );
        assert_eq!(
            depth.stencilBack.passOp,
            crate::WGPUStencilOperation_WGPUStencilOperation_Keep
        );
        assert_eq!(depth.stencilReadMask, 0x1122_3344);
        assert_eq!(depth.stencilWriteMask, 0x5566_7788);
        assert_eq!(depth.depthBias, -7);
        assert_eq!(depth.depthBiasSlopeScale, 1.25);
        assert_eq!(depth.depthBiasClamp, 2.5);

        assert!(native.multisample.nextInChain.is_null());
        assert_eq!(native.multisample.count, 4);
        assert_eq!(native.multisample.mask, 0x1234_5678);
        assert_eq!(native.multisample.alphaToCoverageEnabled, 1);
        let fragment = unsafe { native.fragment.as_ref() }.expect("fragment pointer");
        assert!(fragment.nextInChain.is_null());
        assert_eq!(fragment.module, fake_handle(72));
        assert_eq!(read_view(fragment.entryPoint), b"fs_main");
        assert_eq!(fragment.constantCount, 0);
        assert!(fragment.constants.is_null());
        assert_eq!(fragment.targetCount, 2);
        let native_targets = unsafe { std::slice::from_raw_parts(fragment.targets, 2) };
        assert!(native_targets[0].nextInChain.is_null());
        assert_eq!(
            native_targets[0].format,
            crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm
        );
        assert_eq!(native_targets[0].writeMask, 3);
        let native_blend = unsafe { native_targets[0].blend.as_ref() }.expect("blend pointer");
        assert_eq!(
            native_blend.color.operation,
            crate::WGPUBlendOperation_WGPUBlendOperation_Subtract
        );
        assert_eq!(
            native_blend.color.srcFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_SrcAlpha
        );
        assert_eq!(
            native_blend.color.dstFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_OneMinusDstAlpha
        );
        assert_eq!(
            native_blend.alpha.operation,
            crate::WGPUBlendOperation_WGPUBlendOperation_ReverseSubtract
        );
        assert_eq!(
            native_blend.alpha.srcFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_One
        );
        assert_eq!(
            native_blend.alpha.dstFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_Zero
        );
        assert!(native_targets[1].nextInChain.is_null());
        assert_eq!(
            native_targets[1].format,
            crate::WGPUTextureFormat_WGPUTextureFormat_Undefined
        );
        assert!(native_targets[1].blend.is_null());
        assert_eq!(native_targets[1].writeMask, 0);
    }

    #[test]
    fn t5_vertex_buffers_accept_undefined_as_a_nullable_element_hole() {
        let rt = runtime();
        let cx = rt.context();
        let module = shader_module(&rt, cx, 81);
        let value = descriptor(
            &rt,
            &[
                ("module", module),
                ("buffers", rt.set_like(&[rt.undefined()])),
            ],
        );
        let arena = Arena::new();
        let native = crate::convert_vertex_state::<Engine>(cx, value, &arena)
            .expect("undefined vertex-buffer hole");

        assert_eq!(native.bufferCount, 1);
        let hole = unsafe { &*native.buffers };
        assert!(hole.nextInChain.is_null());
        assert_eq!(
            hole.stepMode,
            crate::WGPUVertexStepMode_WGPUVertexStepMode_Undefined
        );
        assert_eq!(hole.arrayStride, 0);
        assert_eq!(hole.attributeCount, 0);
        assert!(hole.attributes.is_null());
    }

    #[test]
    fn t5_fragment_targets_accept_undefined_as_a_nullable_element_hole() {
        let rt = runtime();
        let cx = rt.context();
        let module = shader_module(&rt, cx, 82);
        let value = descriptor(
            &rt,
            &[
                ("module", module),
                ("targets", rt.set_like(&[rt.undefined()])),
            ],
        );
        let arena = Arena::new();
        let native = crate::convert_fragment_state::<Engine>(cx, value, &arena)
            .expect("undefined color-target hole");

        assert_eq!(native.targetCount, 1);
        let hole = unsafe { &*native.targets };
        assert!(hole.nextInChain.is_null());
        assert_eq!(
            hole.format,
            crate::WGPUTextureFormat_WGPUTextureFormat_Undefined
        );
        assert!(hole.blend.is_null());
        assert_eq!(hole.writeMask, 0);
    }

    #[test]
    fn t5_render_enum_families_reject_unknown_values_and_required_members() {
        let rt = runtime();
        let cx = rt.context();
        let arena = Arena::new();
        assert!(convert_vertex_attribute::<Engine>(
            cx,
            descriptor(
                &rt,
                &[
                    ("format", rt.string("bad")),
                    ("offset", rt.number(0.0)),
                    ("shaderLocation", rt.number(0.0))
                ]
            ),
        )
        .is_err());
        assert!(convert_vertex_buffer_layout::<Engine>(
            cx,
            descriptor(
                &rt,
                &[
                    ("arrayStride", rt.number(0.0)),
                    ("stepMode", rt.string("bad")),
                    ("attributes", rt.set_like(&[]))
                ]
            ),
            &arena,
        )
        .is_err());
        for (member, value) in [
            ("topology", "bad"),
            ("stripIndexFormat", "bad"),
            ("frontFace", "bad"),
            ("cullMode", "bad"),
        ] {
            assert!(convert_primitive_state::<Engine>(
                cx,
                descriptor(&rt, &[(member, rt.string(value))])
            )
            .is_err());
        }
        assert!(convert_stencil_face_state::<Engine>(
            cx,
            descriptor(&rt, &[("compare", rt.string("bad"))])
        )
        .is_err());
        assert!(convert_stencil_face_state::<Engine>(
            cx,
            descriptor(&rt, &[("failOp", rt.string("bad"))])
        )
        .is_err());
        assert!(convert_blend_component::<Engine>(
            cx,
            descriptor(&rt, &[("operation", rt.string("bad"))])
        )
        .is_err());
        assert!(convert_blend_component::<Engine>(
            cx,
            descriptor(&rt, &[("srcFactor", rt.string("bad"))])
        )
        .is_err());

        assert!(convert_vertex_attribute::<Engine>(cx, descriptor(&rt, &[])).is_err());
        assert!(convert_vertex_buffer_layout::<Engine>(cx, descriptor(&rt, &[]), &arena).is_err());
        let module = shader_module(&rt, cx, 81);
        let vertex = descriptor(&rt, &[("module", module)]);
        assert!(convert_render_pipeline_descriptor::<Engine>(
            cx,
            descriptor(&rt, &[("layout", rt.string("auto")), ("vertex", vertex)]),
            &arena,
        )
        .is_ok());
        assert!(convert_render_pipeline_descriptor::<Engine>(
            cx,
            descriptor(&rt, &[("layout", rt.string("auto"))]),
            &arena,
        )
        .is_err());
        assert!(convert_depth_stencil_state::<Engine>(cx, descriptor(&rt, &[])).is_err());
        for (member, value) in [
            ("depthBias", 2_147_483_648.0),
            ("depthBias", -2_147_483_649.0),
            ("depthBiasSlopeScale", f64::INFINITY),
            ("depthBiasClamp", f64::NEG_INFINITY),
        ] {
            let depth = descriptor(
                &rt,
                &[
                    ("format", rt.string("depth24plus")),
                    (member, rt.number(value)),
                ],
            );
            assert!(convert_depth_stencil_state::<Engine>(cx, depth).is_err());
        }
    }

    #[test]
    fn held_value_set_if_empty_keeps_first_value_and_one_release_owed() {
        let rt = runtime();
        let cx = rt.context();
        let first = rt.string("first");
        let second = rt.string("second");
        let held = crate::HeldValue::<Engine>::empty();

        assert_eq!(held.set_if_empty(Engine::duplicate_value(cx, first)), None);
        let duplicate = Engine::duplicate_value(cx, second);
        assert_eq!(held.set_if_empty(duplicate), Some(first));
        Engine::release_value(cx, duplicate);

        assert_eq!(held.get(), Some(first));
        assert_eq!(rt.duplicated_values.borrow().get(&first), Some(&1));
        assert!(!rt.duplicated_values.borrow().contains_key(&second));
        Engine::release_value(cx, held.take().expect("incumbent hold"));
        assert!(rt.duplicated_values.borrow().is_empty());
    }

    #[test]
    fn t5_render_state_defaults_preserve_idl_values_and_c_optional_sentinels() {
        let rt = runtime();
        let cx = rt.context();
        let depth = convert_depth_stencil_state::<Engine>(
            cx,
            descriptor(&rt, &[("format", rt.string("depth24plus"))]),
        )
        .expect("default depth/stencil state");
        assert_eq!(
            depth.depthWriteEnabled,
            crate::WGPUOptionalBool_WGPUOptionalBool_Undefined
        );
        assert_eq!(
            depth.depthCompare,
            crate::WGPUCompareFunction_WGPUCompareFunction_Undefined
        );
        for face in [depth.stencilFront, depth.stencilBack] {
            assert_eq!(
                face.compare,
                crate::WGPUCompareFunction_WGPUCompareFunction_Always
            );
            assert_eq!(
                face.failOp,
                crate::WGPUStencilOperation_WGPUStencilOperation_Keep
            );
            assert_eq!(
                face.depthFailOp,
                crate::WGPUStencilOperation_WGPUStencilOperation_Keep
            );
            assert_eq!(
                face.passOp,
                crate::WGPUStencilOperation_WGPUStencilOperation_Keep
            );
        }
        assert_eq!(depth.stencilReadMask, u32::MAX);
        assert_eq!(depth.stencilWriteMask, u32::MAX);
        assert_eq!(depth.depthBias, 0);
        assert_eq!(depth.depthBiasSlopeScale, 0.0);
        assert_eq!(depth.depthBiasClamp, 0.0);

        let primitive =
            convert_primitive_state::<Engine>(cx, rt.undefined()).expect("default primitive state");
        assert_eq!(
            primitive.topology,
            crate::WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleList
        );
        assert_eq!(
            primitive.stripIndexFormat,
            crate::WGPUIndexFormat_WGPUIndexFormat_Undefined
        );
        assert_eq!(primitive.frontFace, crate::WGPUFrontFace_WGPUFrontFace_CCW);
        assert_eq!(primitive.cullMode, crate::WGPUCullMode_WGPUCullMode_None);
        assert_eq!(primitive.unclippedDepth, 0);

        let multisample = convert_multisample_state::<Engine>(cx, rt.undefined())
            .expect("default multisample state");
        assert_eq!(multisample.count, 1);
        assert_eq!(multisample.mask, u32::MAX);
        assert_eq!(multisample.alphaToCoverageEnabled, 0);

        let component =
            convert_blend_component::<Engine>(cx, rt.undefined()).expect("default blend component");
        assert_eq!(
            component.operation,
            crate::WGPUBlendOperation_WGPUBlendOperation_Add
        );
        assert_eq!(
            component.srcFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_One
        );
        assert_eq!(
            component.dstFactor,
            crate::WGPUBlendFactor_WGPUBlendFactor_Zero
        );

        let arena = Arena::new();
        let target = convert_color_target_state::<Engine>(
            cx,
            descriptor(&rt, &[("format", rt.string("rgba8unorm"))]),
            &arena,
        )
        .expect("default color target");
        assert!(target.blend.is_null());
        assert_eq!(target.writeMask, 0xF);
    }

    #[test]
    fn t5_vertex_only_render_pipeline_retains_one_module_and_optional_layout() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let module = shader_module(&rt, cx, 91);
        let vertex = descriptor(&rt, &[("module", module)]);
        let desc = descriptor(&rt, &[("layout", rt.string("auto")), ("vertex", vertex)]);
        let pipeline = device_create_render_pipeline::<Engine>(cx, device, &[desc])
            .expect("vertex-only render pipeline");
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().shader_module_add_refs, 1);
            assert_eq!(state.borrow().pipeline_layout_add_refs, 0);
        });
        let payload = Engine::payload(cx, pipeline, crate::GPU_RENDER_PIPELINE_CLASS)
            .and_then(|payload| payload.downcast_ref::<RenderPipelinePayload>())
            .expect("render pipeline payload");
        assert_eq!(payload.vertex_module, fake_handle(91));
        assert!(payload.fragment_module.is_null());
        finalize_render_pipeline(
            Box::new(RenderPipelinePayload {
                render_pipeline: payload.render_pipeline,
                vertex_module: payload.vertex_module,
                fragment_module: payload.fragment_module,
                layout: payload.layout,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain render pipeline"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_releases, 1);
            assert_eq!(state.render_pipeline_releases, 1);
        });
    }

    #[test]
    fn pipeline_constants_reach_compute_vertex_and_fragment_c_structs() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let module = shader_module(&rt, cx, 99);

        let compute_constants =
            descriptor(&rt, &[("alpha", rt.number(1.5)), ("beta", rt.number(-2.0))]);
        let compute = descriptor(&rt, &[("module", module), ("constants", compute_constants)]);
        let compute_desc = descriptor(&rt, &[("layout", rt.string("auto")), ("compute", compute)]);
        device_create_compute_pipeline::<Engine>(cx, device, &[compute_desc])
            .expect("compute constants");

        let vertex = descriptor(
            &rt,
            &[
                ("module", module),
                (
                    "constants",
                    descriptor(&rt, &[("vertexValue", rt.number(3.0))]),
                ),
            ],
        );
        let fragment = descriptor(
            &rt,
            &[
                ("module", module),
                (
                    "constants",
                    descriptor(&rt, &[("fragmentValue", rt.number(4.0))]),
                ),
                ("targets", rt.set_like(&[])),
            ],
        );
        let render_desc = descriptor(
            &rt,
            &[
                ("layout", rt.string("auto")),
                ("vertex", vertex),
                ("fragment", fragment),
            ],
        );
        device_create_render_pipeline::<Engine>(cx, device, &[render_desc])
            .expect("render constants");

        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().pipeline_constants,
                [
                    (
                        "compute",
                        vec![(b"alpha".to_vec(), 1.5), (b"beta".to_vec(), -2.0)]
                    ),
                    ("vertex", vec![(b"vertexValue".to_vec(), 3.0)]),
                    ("fragment", vec![(b"fragmentValue".to_vec(), 4.0)]),
                ]
            );
        });
    }

    #[test]
    fn pipeline_constants_accept_empty_record_and_reject_non_numeric_value() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let module = shader_module(&rt, cx, 100);

        let empty_stage = descriptor(
            &rt,
            &[("module", module), ("constants", descriptor(&rt, &[]))],
        );
        let empty_desc = descriptor(
            &rt,
            &[("layout", rt.string("auto")), ("compute", empty_stage)],
        );
        device_create_compute_pipeline::<Engine>(cx, device, &[empty_desc])
            .expect("empty constants");
        GPU_STATE.with(|state| {
            assert_eq!(state.borrow().pipeline_constants, [("compute", Vec::new())]);
        });

        let invalid_stage = descriptor(
            &rt,
            &[
                ("module", module),
                (
                    "constants",
                    descriptor(&rt, &[("value", rt.string("not-a-number"))]),
                ),
            ],
        );
        let invalid_desc = descriptor(
            &rt,
            &[("layout", rt.string("auto")), ("compute", invalid_stage)],
        );
        assert!(
            device_create_compute_pipeline::<Engine>(cx, device, &[invalid_desc]).is_err(),
            "non-numeric constant must be a TypeError in real engines"
        );
    }

    #[test]
    fn t5_render_pipeline_retention_is_vertex_module_fragment_module_and_layout() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let vertex_module = shader_module(&rt, cx, 101);
        let fragment_module = shader_module(&rt, cx, 102);
        let layout = Engine::new_instance(
            cx,
            crate::GPU_PIPELINE_LAYOUT_CLASS,
            Box::new(PipelineLayoutPayload {
                layout: fake_handle(103),
            }),
        )
        .expect("pipeline layout");
        let vertex = descriptor(&rt, &[("module", vertex_module)]);
        let fragment = descriptor(
            &rt,
            &[("module", fragment_module), ("targets", rt.set_like(&[]))],
        );
        let desc = descriptor(
            &rt,
            &[
                ("layout", layout),
                ("vertex", vertex),
                ("fragment", fragment),
            ],
        );
        let pipeline =
            device_create_render_pipeline::<Engine>(cx, device, &[desc]).expect("render pipeline");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_add_refs, 2);
            assert_eq!(state.pipeline_layout_add_refs, 1);
        });
        let payload = Engine::payload(cx, pipeline, crate::GPU_RENDER_PIPELINE_CLASS)
            .and_then(|payload| payload.downcast_ref::<RenderPipelinePayload>())
            .expect("render pipeline payload");
        finalize_render_pipeline(
            Box::new(RenderPipelinePayload {
                render_pipeline: payload.render_pipeline,
                vertex_module: payload.vertex_module,
                fragment_module: payload.fragment_module,
                layout: payload.layout,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain render pipeline"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_releases, 2);
            assert_eq!(state.pipeline_layout_releases, 1);
            assert_eq!(state.render_pipeline_releases, 1);
        });
    }

    #[test]
    fn async_compute_and_render_pipelines_settle_in_one_batch_with_sync_retention() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let compute_module = shader_module(&rt, cx, 201);
        let vertex_module = shader_module(&rt, cx, 202);
        let fragment_module = shader_module(&rt, cx, 203);
        let layout = Engine::new_instance(
            cx,
            crate::GPU_PIPELINE_LAYOUT_CLASS,
            Box::new(PipelineLayoutPayload {
                layout: fake_handle(204),
            }),
        )
        .expect("pipeline layout");
        let compute_desc = descriptor(
            &rt,
            &[
                ("layout", layout),
                ("compute", descriptor(&rt, &[("module", compute_module)])),
            ],
        );
        let render_desc = descriptor(
            &rt,
            &[
                ("layout", layout),
                ("vertex", descriptor(&rt, &[("module", vertex_module)])),
                (
                    "fragment",
                    descriptor(
                        &rt,
                        &[("module", fragment_module), ("targets", rt.set_like(&[]))],
                    ),
                ),
            ],
        );
        let compute =
            crate::device_create_compute_pipeline_async::<Engine>(cx, device, &[compute_desc])
                .expect("compute promise");
        let render =
            crate::device_create_render_pipeline_async::<Engine>(cx, device, &[render_desc])
                .expect("render promise");
        assert_eq!(rt.env.settlements().drain::<Engine>(cx), Ok(2));
        let compute = rt
            .promise_result(compute)
            .expect("compute settled")
            .expect("compute resolved");
        let render = rt
            .promise_result(render)
            .expect("render settled")
            .expect("render resolved");
        assert!(Engine::payload(cx, compute, crate::GPU_COMPUTE_PIPELINE_CLASS).is_some());
        assert!(Engine::payload(cx, render, crate::GPU_RENDER_PIPELINE_CLASS).is_some());
        assert_eq!(rt.settle_calls.get(), 1);
        assert_eq!(rt.settlement_batch_sizes.borrow().as_slice(), &[2]);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_add_refs, 3);
            assert_eq!(state.pipeline_layout_add_refs, 2);
        });
        let compute_payload = Engine::payload(cx, compute, crate::GPU_COMPUTE_PIPELINE_CLASS)
            .and_then(|payload| payload.downcast_ref::<ComputePipelinePayload>())
            .expect("compute pipeline payload");
        finalize_compute_pipeline(
            Box::new(ComputePipelinePayload {
                pipeline: compute_payload.pipeline,
                module: compute_payload.module,
                layout: compute_payload.layout,
            }),
            &rt.env,
        );
        let render_payload = Engine::payload(cx, render, crate::GPU_RENDER_PIPELINE_CLASS)
            .and_then(|payload| payload.downcast_ref::<RenderPipelinePayload>())
            .expect("render pipeline payload");
        finalize_render_pipeline(
            Box::new(RenderPipelinePayload {
                render_pipeline: render_payload.render_pipeline,
                vertex_module: render_payload.vertex_module,
                fragment_module: render_payload.fragment_module,
                layout: render_payload.layout,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("release async pipelines"), 2);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.shader_module_releases, 3);
            assert_eq!(state.pipeline_layout_releases, 2);
            assert_eq!(state.compute_pipeline_releases, 1);
            assert_eq!(state.render_pipeline_releases, 1);
        });
    }

    #[test]
    fn async_pipeline_validation_failure_surfaces_reason_and_releases_retention() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let module = shader_module(&rt, cx, 211);
        let desc = descriptor(
            &rt,
            &[
                ("layout", rt.string("auto")),
                ("compute", descriptor(&rt, &[("module", module)])),
            ],
        );
        GPU_STATE.with(|state| {
            state.borrow_mut().next_pipeline_async_error = Some((
                crate::WGPUCreatePipelineAsyncStatus_WGPUCreatePipelineAsyncStatus_ValidationError,
                "bad pipeline descriptor".to_owned(),
            ));
        });
        let promise = crate::device_create_compute_pipeline_async::<Engine>(cx, device, &[desc])
            .expect("validation promise");
        assert_eq!(rt.env.settlements().drain::<Engine>(cx), Ok(1));
        let reason = rt
            .promise_result(promise)
            .expect("settled")
            .expect_err("rejected");
        let MockValue::Object(properties) = rt.get(reason) else {
            panic!("rejection reason is not an error object");
        };
        assert!(matches!(
            properties.get("name").copied().map(|value| rt.get(value)),
            Some(MockValue::String(name)) if name == "OperationError"
        ));
        assert!(matches!(
            properties.get("message").copied().map(|value| rt.get(value)),
            Some(MockValue::String(message))
                if message.contains("validation") && message.contains("bad pipeline descriptor")
        ));
        assert_eq!(rt.queue().drain().expect("release failed retention"), 1);
        GPU_STATE.with(|state| assert_eq!(state.borrow().shader_module_releases, 1));
    }

    #[test]
    fn a9_callback_rejection_preserves_backend_message() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let (promise, deferred) = Engine::new_promise(cx).expect("promise");
        let request = Box::new(QueueWorkDoneRequest::<Engine> {
            deferred: Some(deferred),
            settlements: Arc::clone(rt.env.settlements()),
            _registration: None,
        });
        unsafe {
            queue_work_done_callback::<Engine>(
                crate::WGPUQueueWorkDoneStatus_WGPUQueueWorkDoneStatus_Error,
                WGPUStringView::from_bytes(b"backend diagnostic"),
                Box::into_raw(request).cast(),
                ptr::null_mut(),
            );
        }
        assert_eq!(rt.env.settlements().drain::<Engine>(cx), Ok(1));
        let reason = rt
            .promise_result(promise)
            .expect("settled promise")
            .expect_err("rejected promise");
        let MockValue::Object(properties) = rt.get(reason) else {
            panic!("rejection reason is not an error object");
        };
        assert!(matches!(
            properties.get("name").copied().map(|value| rt.get(value)),
            Some(MockValue::String(name)) if name == "OperationError"
        ));
        assert!(matches!(
            properties.get("message").copied().map(|value| rt.get(value)),
            Some(MockValue::String(message)) if message.contains("backend diagnostic")
        ));
    }

    #[test]
    fn s1_push_error_scope_round_trips_each_generated_filter_and_rejects_unknown() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        for filter in ["validation", "out-of-memory", "internal"] {
            device_push_error_scope::<Engine>(cx, device, &[rt.string(filter)])
                .expect("push error scope");
        }
        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().pushed_error_filters,
                [
                    crate::WGPUErrorFilter_WGPUErrorFilter_Validation,
                    crate::WGPUErrorFilter_WGPUErrorFilter_OutOfMemory,
                    crate::WGPUErrorFilter_WGPUErrorFilter_Internal,
                ]
            );
        });
        let error = device_push_error_scope::<Engine>(cx, device, &[rt.string("future-filter")])
            .expect_err("unknown filter");
        assert!(error.starts_with("TypeError:"));
    }

    #[test]
    fn s2_pop_error_scope_resolves_null_and_each_error_class_with_message() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");

        device_push_error_scope::<Engine>(cx, device, &[rt.string("validation")])
            .expect("push null scope");
        let promise = device_pop_error_scope::<Engine>(cx, device, &[]).expect("pop null scope");
        rt.env
            .settlements()
            .drain::<Engine>(cx)
            .expect("drain null result");
        let value = rt
            .promise_result(promise)
            .expect("settled null promise")
            .expect("resolved null promise");
        assert!(matches!(rt.get(value), MockValue::Null));

        for (type_, class, message) in [
            (
                crate::WGPUErrorType_WGPUErrorType_Validation,
                crate::GPU_VALIDATION_ERROR_CLASS,
                "validation diagnostic",
            ),
            (
                crate::WGPUErrorType_WGPUErrorType_OutOfMemory,
                crate::GPU_OUT_OF_MEMORY_ERROR_CLASS,
                "out of memory diagnostic",
            ),
            (
                crate::WGPUErrorType_WGPUErrorType_Internal,
                crate::GPU_INTERNAL_ERROR_CLASS,
                "internal diagnostic",
            ),
        ] {
            device_push_error_scope::<Engine>(cx, device, &[rt.string("validation")])
                .expect("push error scope");
            GPU_STATE.with(|state| {
                state.borrow_mut().next_pop_error = Some(MockPopError {
                    status: crate::WGPUPopErrorScopeStatus_WGPUPopErrorScopeStatus_Success,
                    type_,
                    message: message.to_owned(),
                });
            });
            let promise =
                device_pop_error_scope::<Engine>(cx, device, &[]).expect("pop error scope");
            rt.env
                .settlements()
                .drain::<Engine>(cx)
                .expect("drain error result");
            let value = rt
                .promise_result(promise)
                .expect("settled error promise")
                .expect("resolved error promise");
            let MockValue::Instance {
                class: actual,
                payload,
            } = rt.get(value)
            else {
                panic!("pop did not resolve a GPUError instance");
            };
            assert_eq!(actual, class);
            assert_eq!(
                payload
                    .downcast_ref::<ErrorPayload>()
                    .expect("error payload")
                    .message,
                message
            );
        }
    }

    #[test]
    fn s2_empty_pop_rejects_named_operation_error_with_backend_message() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let promise = device_pop_error_scope::<Engine>(cx, device, &[]).expect("pop empty scope");
        rt.env
            .settlements()
            .drain::<Engine>(cx)
            .expect("drain rejection");
        let reason = rt
            .promise_result(promise)
            .expect("settled promise")
            .expect_err("empty pop must reject");
        let MockValue::Object(properties) = rt.get(reason) else {
            panic!("rejection reason is not an error object");
        };
        assert!(matches!(
            properties.get("name").copied().map(|value| rt.get(value)),
            Some(MockValue::String(name)) if name == "OperationError"
        ));
        assert!(matches!(
            properties.get("message").copied().map(|value| rt.get(value)),
            Some(MockValue::String(message)) if message.contains("error scope stack is empty")
        ));
    }

    #[test]
    fn s2_two_pop_settlements_keep_a30_single_batch() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        for _ in 0..2 {
            device_push_error_scope::<Engine>(cx, device, &[rt.string("validation")])
                .expect("push scope");
        }
        let first = device_pop_error_scope::<Engine>(cx, device, &[]).expect("first pop");
        let second = device_pop_error_scope::<Engine>(cx, device, &[]).expect("second pop");
        assert_eq!(rt.env.settlements().drain::<Engine>(cx), Ok(2));
        assert_eq!(rt.settle_calls.get(), 1);
        assert_eq!(&*rt.settlement_batch_sizes.borrow(), &[2]);
        assert!(matches!(rt.promise_result(first), Some(Ok(_))));
        assert!(matches!(rt.promise_result(second), Some(Ok(_))));
    }

    #[test]
    fn a28_late_adapter_callback_releases_success_handle_after_teardown() {
        reset_gpu();
        let request = {
            let rt = runtime();
            let cx = rt.context();
            let (_, deferred) = Engine::new_promise(cx).expect("promise");
            let mut request = Box::new(AdapterRequest::<Engine> {
                deferred: Some(deferred),
                settlements: Arc::clone(rt.env.settlements()),
                release_queue: Arc::clone(rt.queue()),
                gpu: dispatch(),
                _registration: None,
            });
            Engine::register_deferred(cx, std::ptr::NonNull::from(&mut request.deferred));
            request._registration = Some(());
            let deferred = request.deferred.take().expect("registered deferred");
            Engine::release_deferred(cx, deferred);
            assert_eq!(Arc::strong_count(&request.release_queue), 2);
            request
        };
        assert_eq!(Arc::strong_count(&request.release_queue), 1);

        unsafe {
            request_adapter_callback::<Engine>(
                crate::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success,
                fake_handle(92),
                WGPUStringView::from_bytes(b""),
                Box::into_raw(request).cast(),
                ptr::null_mut(),
            );
        }

        GPU_STATE.with(|state| assert_eq!(state.borrow().adapter_releases, 1));
    }

    #[test]
    fn a28_late_device_callback_releases_success_handle_after_teardown() {
        reset_gpu();
        let request = {
            let rt = runtime();
            let cx = rt.context();
            let (_, deferred) = Engine::new_promise(cx).expect("promise");
            let mut request = Box::new(DeviceRequest::<Engine> {
                deferred: Some(deferred),
                settlements: Arc::clone(rt.env.settlements()),
                release_queue: Arc::clone(rt.queue()),
                gpu: dispatch(),
                events: DeviceEventState::new(Arc::clone(rt.env.settlements())),
                _registration: None,
            });
            Engine::register_deferred(cx, std::ptr::NonNull::from(&mut request.deferred));
            request._registration = Some(());
            let deferred = request.deferred.take().expect("registered deferred");
            Engine::release_deferred(cx, deferred);
            assert_eq!(Arc::strong_count(&request.release_queue), 2);
            request
        };
        assert_eq!(Arc::strong_count(&request.release_queue), 1);

        unsafe {
            request_device_callback::<Engine>(
                crate::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success,
                fake_handle(93),
                WGPUStringView::from_bytes(b""),
                Box::into_raw(request).cast(),
                ptr::null_mut(),
            );
        }

        GPU_STATE.with(|state| assert_eq!(state.borrow().device_releases, 1));
    }

    #[test]
    fn settlement_teardown_and_unexpected_type_release_native_handles() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let (_, deferred) = Engine::new_promise(cx).expect("promise");
        rt.env
            .settlements()
            .enqueue::<Engine>(SettlementRequest::Adapter {
                deferred,
                native: PendingNative {
                    handle: PendingNativeHandle::Adapter(fake_handle(91)),
                    queue: Arc::clone(rt.queue()),
                    gpu: dispatch(),
                },
            })
            .expect("enqueue adapter");
        rt.env.settlements().release_pending::<Engine>(cx);
        assert_eq!(rt.queue().drain(), Ok(1));

        let (_, deferred) = MockEngine::<true>::new_promise(cx).expect("promise");
        rt.env
            .settlements()
            .enqueue::<MockEngine<true>>(SettlementRequest::Device {
                deferred,
                native: PendingNative {
                    handle: PendingNativeHandle::Device(fake_device()),
                    queue: Arc::clone(rt.queue()),
                    gpu: dispatch(),
                },
                events: DeviceEventState::new(Arc::clone(rt.env.settlements())),
            })
            .expect("enqueue device");
        assert_eq!(
            rt.env.settlements().drain::<Engine>(cx),
            Err(crate::TickError::Queue(
                QueueError::UnexpectedSettlementType
            ))
        );
        assert_eq!(rt.queue().drain(), Ok(1));
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.adapter_releases, 1);
            assert_eq!(state.device_releases, 1);
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
    fn a24_a32_mock_rejects_non_const_overlap_and_allows_const_overlap() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(8.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let native = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .and_then(|payload| payload.state().lock().ok().map(|state| state.buffer))
            .expect("native buffer");

        unsafe {
            assert!(!super::buffer_get_mapped_range(native, 0, 4).is_null());
            assert!(super::buffer_get_mapped_range(native, 2, 2).is_null());
            assert!(super::buffer_get_const_mapped_range(native, 2, 2).is_null());

            super::buffer_unmap(native);
            assert!(!super::buffer_get_const_mapped_range(native, 0, 4).is_null());
            assert!(!super::buffer_get_const_mapped_range(native, 2, 2).is_null());
            assert!(super::buffer_get_mapped_range(native, 2, 2).is_null());
        }
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
        assert_eq!(rt.reclaimed_values(), 4);
        assert_eq!(rt.live_scoped_values(), 0);
    }

    #[test]
    fn r23_non_object_property_result_is_scoped_and_reclaimed() {
        let rt = runtime();
        let object = rt.number(1.0);
        let mut result = rt.undefined();
        rt.with_scope(|cx| {
            result = Engine::get_property(cx, object, "missing").expect("property");
            assert_ne!(result, rt.undefined());
            assert!(Engine::is_undefined(cx, result));
            assert_eq!(rt.live_scoped_values(), 1);
        });
        assert_eq!(rt.live_scoped_values(), 0);
        assert!(Engine::is_undefined(rt.context(), result));
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
    fn mapped_at_creation_non_multiple_of_four_throws_range_error_before_native_call() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(2.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );

        assert_eq!(
            device_create_buffer::<Engine>(cx, device, &[desc]).expect_err("size 2 must fail"),
            "RangeError: mappedAtCreation buffer size must be a multiple of 4"
        );
        GPU_STATE.with(|state| assert!(state.borrow().descriptors.is_empty()));
    }

    #[test]
    fn promise_returning_operations_reject_conversion_and_receiver_errors() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let incompatible = rt.undefined();

        let request_adapter = gpu_request_adapter::<Engine>(cx, incompatible, &[])
            .expect("requestAdapter must return a promise");
        assert_rejection(
            &rt,
            request_adapter,
            "TypeError",
            "GPU.requestAdapter called on an incompatible object",
        );

        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(800))),
        )
        .expect("adapter");
        let invalid_request = descriptor(
            &rt,
            &[(
                "requiredFeatures",
                rt.set_like(&[rt.string("not-a-feature")]),
            )],
        );
        let request_device = adapter_request_device::<Engine>(cx, adapter, &[invalid_request])
            .expect("requestDevice must return a promise");
        assert_rejection(&rt, request_device, "TypeError", "GPUFeatureName");

        let pop = device_pop_error_scope::<Engine>(cx, incompatible, &[])
            .expect("popErrorScope must return a promise");
        assert_rejection(
            &rt,
            pop,
            "TypeError",
            "GPUDevice method called on an incompatible object",
        );

        let work = queue_on_submitted_work_done::<Engine>(cx, incompatible, &[])
            .expect("onSubmittedWorkDone must return a promise");
        assert_rejection(
            &rt,
            work,
            "TypeError",
            "GPUQueue.onSubmittedWorkDone called on an incompatible object",
        );

        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(1.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let map =
            buffer_map_async::<Engine>(cx, buffer, &[]).expect("mapAsync must return a promise");
        assert_rejection(&rt, map, "TypeError", "GPUMapModeFlags is required");
    }

    #[test]
    fn map_async_on_destroyed_buffer_rejects_operation_error_with_message() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(&rt, &[("size", rt.number(4.0)), ("usage", rt.number(1.0))]);
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        buffer_destroy::<Engine>(cx, buffer, &[]).expect("destroy");

        let promise = buffer_map_async::<Engine>(cx, buffer, &[rt.number(1.0)])
            .expect("destroyed mapAsync must return a promise");
        assert_rejection(&rt, promise, "OperationError", "GPUBuffer is destroyed");
    }

    #[test]
    fn device_destroy_calls_native_destroy_and_new_request_device_still_resolves() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(810))),
        )
        .expect("adapter");

        let first = adapter_request_device::<Engine>(cx, adapter, &[]).expect("first request");
        unsafe { crate::tick::<Engine>(cx, fake_handle(811)) }.expect("first tick");
        let first_device = rt
            .promise_result(first)
            .expect("first settled")
            .expect("first resolved");
        device_destroy::<Engine>(cx, first_device, &[]).expect("destroy device");
        device_destroy::<Engine>(cx, first_device, &[]).expect("destroy device again");

        let new_adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(813))),
        )
        .expect("new adapter");
        let second =
            adapter_request_device::<Engine>(cx, new_adapter, &[]).expect("second request");
        unsafe { crate::tick::<Engine>(cx, fake_handle(812)) }.expect("second tick");
        rt.promise_result(second)
            .expect("second settled")
            .expect("second resolved");
        GPU_STATE.with(|state| assert_eq!(state.borrow().device_destroys, 1));
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
    fn buffer_size_and_usage_getters_return_wrapper_metadata() {
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
        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().buffer_add_refs,
                if COPY { 0 } else { 2 },
                "zero-copy ranges must retain their owner once per range"
            );
        });
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

        if !COPY {
            assert_eq!(rt.queue().len(), Ok(2));
            assert_eq!(rt.queue().drain(), Ok(2));
            GPU_STATE.with(|state| assert_eq!(state.borrow().buffer_releases, 2));
        }

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
        rt.env.release_device_event_values::<MockEngine<COPY>>(cx);
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
    fn payload_value_dispatch_releases_all_buffer_ranges_once() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(8.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        let first =
            buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(0.0), rt.number(4.0)])
                .expect("first range");
        let second =
            buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.number(4.0), rt.number(4.0)])
                .expect("second range");
        let payload = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .expect("buffer payload");

        let mut released = Vec::new();
        crate::release_payload_values::<Engine>(payload, &mut |value| {
            released.push(value);
            Engine::release_value(cx, value);
        });
        crate::release_payload_values::<Engine>(payload, &mut |value| released.push(value));
        assert_eq!(released, [first, second]);
        rt.env.release_device_event_values::<Engine>(cx);
        assert!(rt.duplicated_values.borrow().is_empty());
    }

    #[test]
    fn r27_mock_coercion_reenters_unmap_before_mapped_range_state_lock() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(8.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        rt.reenter_unmap_on_next_coercion(buffer);

        assert_eq!(
            buffer_get_mapped_range::<Engine>(cx, buffer, &[rt.undefined(), rt.number(4.0)],)
                .expect_err("reentrant unmap must invalidate the range request"),
            "OperationError: buffer is not mapped"
        );
    }

    #[test]
    fn a26_arraybuffer_len_returns_none_for_non_arraybuffer() {
        let rt = runtime();
        let cx = rt.context();
        assert_eq!(Engine::arraybuffer_len(cx, rt.number(1.0)), None);
    }

    #[test]
    fn a16_unmap_is_idempotent() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(8.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<Engine>(cx, device, &[desc]).expect("buffer");
        buffer_unmap::<Engine>(cx, buffer, &[]).expect("first unmap");
        buffer_unmap::<Engine>(cx, buffer, &[]).expect("second unmap");
        GPU_STATE.with(|state| assert_eq!(state.borrow().buffer_unmaps, 1));
    }

    #[test]
    fn command_encoder_finish_converts_descriptor_before_ending_encoder() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let descriptor = descriptor(&rt, &[]);
        rt.set_property_error(descriptor, "label", "label getter failed");

        assert_eq!(
            command_encoder_finish::<Engine>(cx, encoder, &[descriptor])
                .expect_err("throwing label getter"),
            "label getter failed"
        );
        command_encoder_finish::<Engine>(cx, encoder, &[])
            .expect("encoder remains usable after conversion failure");
    }

    #[test]
    fn pass_end_rejects_after_parent_encoder_is_finished_without_calling_ffi() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");

        let compute_encoder =
            device_create_command_encoder::<Engine>(cx, device, &[]).expect("compute encoder");
        let compute_pass =
            crate::command_encoder_begin_compute_pass::<Engine>(cx, compute_encoder, &[])
                .expect("compute pass");
        command_encoder_finish::<Engine>(cx, compute_encoder, &[]).expect("finish parent");
        assert_eq!(
            crate::compute_pass_end::<Engine>(cx, compute_pass, &[])
                .expect_err("compute end after parent finish"),
            "OperationError: GPUCommandEncoder is finished"
        );

        let render_encoder =
            device_create_command_encoder::<Engine>(cx, device, &[]).expect("render encoder");
        let render_descriptor = descriptor(&rt, &[("colorAttachments", rt.set_like(&[]))]);
        let render_pass = crate::command_encoder_begin_render_pass::<Engine>(
            cx,
            render_encoder,
            &[render_descriptor],
        )
        .expect("render pass");
        command_encoder_finish::<Engine>(cx, render_encoder, &[]).expect("finish parent");
        assert_eq!(
            crate::render_pass_end::<Engine>(cx, render_pass, &[])
                .expect_err("render end after parent finish"),
            "OperationError: GPUCommandEncoder is finished"
        );

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.recording_calls.get("compute_end"), None);
            assert_eq!(state.recording_calls.get("render_end"), None);
        });
    }

    #[test]
    fn command_and_pass_primary_handles_release_exactly_once() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let compute_pass = crate::command_encoder_begin_compute_pass::<Engine>(cx, encoder, &[])
            .expect("compute pass");
        crate::compute_pass_end::<Engine>(cx, compute_pass, &[]).expect("end compute pass");
        let render_descriptor = descriptor(&rt, &[("colorAttachments", rt.set_like(&[]))]);
        let render_pass =
            crate::command_encoder_begin_render_pass::<Engine>(cx, encoder, &[render_descriptor])
                .expect("render pass");
        crate::render_pass_end::<Engine>(cx, render_pass, &[]).expect("end render pass");
        let command_buffer = command_encoder_finish::<Engine>(cx, encoder, &[]).expect("finish");

        let encoder_state = crate::command_encoder_state::<Engine>(cx, encoder).expect("state");
        let command_buffer_state =
            crate::command_buffer_state::<Engine>(cx, command_buffer).expect("state");
        let compute_state =
            Engine::payload(cx, compute_pass, crate::GPU_COMPUTE_PASS_ENCODER_CLASS)
                .and_then(|payload| payload.downcast_ref::<crate::ComputePassEncoderPayload>())
                .map(|payload| Arc::clone(&payload.state))
                .expect("compute state");
        let render_state = Engine::payload(cx, render_pass, crate::GPU_RENDER_PASS_ENCODER_CLASS)
            .and_then(|payload| payload.downcast_ref::<crate::RenderPassEncoderPayload>())
            .map(|payload| Arc::clone(&payload.state))
            .expect("render state");

        crate::finalize_command_encoder(
            Box::new(crate::CommandEncoderPayload {
                state: encoder_state,
            }),
            &rt.env,
        );
        crate::finalize_command_buffer(
            Box::new(crate::CommandBufferPayload {
                state: command_buffer_state,
            }),
            &rt.env,
        );
        crate::finalize_compute_pass_encoder(
            Box::new(crate::ComputePassEncoderPayload {
                state: compute_state,
            }),
            &rt.env,
        );
        crate::finalize_render_pass_encoder(
            Box::new(crate::RenderPassEncoderPayload {
                state: render_state,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("drain command handles"), 4);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.command_encoder_releases, 1);
            assert_eq!(state.command_buffer_releases, 1);
            assert_eq!(state.compute_pass_encoder_releases, 1);
            assert_eq!(state.render_pass_encoder_releases, 1);
        });
    }

    #[test]
    fn b19_queue_submit_core_guard_consumes_command_buffer_once() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let command = command_encoder_finish::<Engine>(cx, encoder, &[]).expect("command");
        let commands = rt.set_like(&[command]);

        queue_submit::<Engine>(cx, queue, &[commands]).expect("first submit");
        assert_eq!(
            queue_submit::<Engine>(cx, queue, &[commands]).expect_err("second submit"),
            "OperationError: GPUCommandBuffer is consumed"
        );
        release_device_held_values(&rt, cx, device);
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
    fn detach_failure_still_processes_and_releases_every_mapped_range() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<MockEngine<true>>(cx, fake_device()) }.expect("device");
        let desc = descriptor(
            &rt,
            &[
                ("size", rt.number(12.0)),
                ("usage", rt.number(2.0)),
                ("mappedAtCreation", rt.bool(true)),
            ],
        );
        let buffer = device_create_buffer::<MockEngine<true>>(cx, device, &[desc]).expect("buffer");
        let first = buffer_get_mapped_range::<MockEngine<true>>(
            cx,
            buffer,
            &[rt.number(0.0), rt.number(4.0)],
        )
        .expect("first range");
        let middle = buffer_get_mapped_range::<MockEngine<true>>(
            cx,
            buffer,
            &[rt.number(4.0), rt.number(4.0)],
        )
        .expect("middle range");
        let last = buffer_get_mapped_range::<MockEngine<true>>(
            cx,
            buffer,
            &[rt.number(8.0), rt.number(4.0)],
        )
        .expect("last range");
        rt.set_detach_noop_for(middle);

        let error = buffer_unmap::<MockEngine<true>>(cx, buffer, &[])
            .expect_err("middle detach verification must fail");

        assert_eq!(error, "OperationError: mapped range detach failed");
        assert_eq!(MockEngine::<true>::arraybuffer_len(cx, first), Some(0));
        assert_eq!(MockEngine::<true>::arraybuffer_len(cx, middle), Some(4));
        assert_eq!(MockEngine::<true>::arraybuffer_len(cx, last), Some(0));
        rt.env.release_device_event_values::<MockEngine<true>>(cx);
        assert!(
            rt.duplicated_values.borrow().is_empty(),
            "every mapped-range value must be released even after an earlier failure"
        );
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
        rt.env.release_device_event_values::<MockEngine<true>>(cx);
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

        let promise = buffer_map_async::<Engine>(cx, buffer, &[rt.number(2.0), too_large])
            .expect("mapAsync conversion error must return a promise");
        assert_rejection(&rt, promise, "TypeError", "offset");
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
        Engine::settle_deferreds(cx, vec![(deferred, Ok(first))]).expect("first settlement");
        Engine::settle_deferreds(
            cx,
            vec![(Deferred::new(resolve, reject), Err(rt.string("late")))],
        )
        .expect("late settlement call");

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
            }),
            Engine::environment(cx),
        );

        let device_payload = Engine::payload(cx, device, crate::GPU_DEVICE_CLASS)
            .and_then(|payload| payload.downcast_ref::<DevicePayload<Engine>>())
            .expect("payload");
        finalize_device::<Engine>(
            Box::new(DevicePayload::<Engine>::new(
                device_payload.device(),
                Arc::clone(&device_payload.events),
            )),
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

    fn binding_created_device(rt: &Runtime, cx: Context<'_>) -> Value {
        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(700))),
        )
        .expect("adapter");
        let promise = adapter_request_device::<Engine>(cx, adapter, &[]).expect("requestDevice");
        unsafe { crate::tick::<Engine>(cx, fake_handle(701)) }.expect("requestDevice tick");
        rt.promise_result(promise)
            .expect("requestDevice settled")
            .expect("requestDevice resolved")
    }

    #[test]
    fn c7_own_names_and_request_device_features_limits_reach_native() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let own = descriptor(
            &rt,
            &[("second", rt.number(2.0)), ("first", rt.number(1.0))],
        );
        assert_eq!(
            Engine::own_property_names(cx, own).expect("own names"),
            ["first", "second"]
        );

        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(710))),
        )
        .expect("adapter");
        let feature_values = [
            rt.string("timestamp-query"),
            rt.string("depth-clip-control"),
        ];
        let required_features = rt.set_like(&feature_values);
        let required_limits = descriptor(
            &rt,
            &[
                ("maxBufferSize", rt.number(4096.0)),
                ("maxTextureDimension2D", rt.number(2048.0)),
                ("maxBindGroups", rt.undefined()),
                ("maxStorageBuffersInVertexStage", rt.number(7.0)),
            ],
        );
        let request = descriptor(
            &rt,
            &[
                ("requiredFeatures", required_features),
                ("requiredLimits", required_limits),
            ],
        );
        let promise = adapter_request_device::<Engine>(cx, adapter, &[request]).expect("request");
        unsafe { crate::tick::<Engine>(cx, fake_handle(711)) }.expect("tick");
        let device = rt
            .promise_result(promise)
            .expect("settled")
            .expect("resolved");

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(
                state.requested_features.last().expect("features"),
                &[
                    crate::WGPUFeatureName_WGPUFeatureName_TimestampQuery,
                    crate::WGPUFeatureName_WGPUFeatureName_DepthClipControl,
                ]
            );
            let (limits, compatibility) = state
                .requested_limits
                .last()
                .expect("limits request")
                .as_ref()
                .expect("limits pointer");
            assert_eq!(limits.maxBufferSize, 4096);
            assert_eq!(limits.maxTextureDimension2D, 2048);
            assert_eq!(limits.maxBindGroups, crate::WGPU_LIMIT_U32_UNDEFINED);
            assert_eq!(
                limits.maxUniformBufferBindingSize,
                crate::WGPU_LIMIT_U64_UNDEFINED as u64
            );
            assert_eq!(compatibility.maxStorageBuffersInVertexStage, 7);
            assert_eq!(
                compatibility.maxStorageTexturesInFragmentStage,
                crate::WGPU_LIMIT_U32_UNDEFINED
            );
        });

        let timestamp_desc = descriptor(
            &rt,
            &[("type", rt.string("timestamp")), ("count", rt.number(2.0))],
        );
        device_create_query_set::<Engine>(cx, device, &[timestamp_desc])
            .expect("timestamp query set");
        GPU_STATE.with(|state| {
            assert_eq!(
                state
                    .borrow()
                    .query_set_descriptors
                    .last()
                    .expect("query set")
                    .type_,
                crate::WGPUQueryType_WGPUQueryType_Timestamp
            );
        });
    }

    #[test]
    fn c7_request_device_rejects_unknown_feature_and_limit_names() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(720))),
        )
        .expect("adapter");

        let unknown_feature = rt.set_like(&[rt.string("not-a-feature")]);
        let request = descriptor(&rt, &[("requiredFeatures", unknown_feature)]);
        let promise = adapter_request_device::<Engine>(cx, adapter, &[request])
            .expect("unknown feature must return a promise");
        assert_rejection(&rt, promise, "TypeError", "GPUFeatureName");

        let limits = descriptor(&rt, &[("notALimit", rt.number(1.0))]);
        let request = descriptor(&rt, &[("requiredLimits", limits)]);
        let promise = adapter_request_device::<Engine>(cx, adapter, &[request]).expect("promise");
        let reason = rt
            .promise_result(promise)
            .expect("settled")
            .expect_err("unknown limit rejection");
        let MockValue::Object(properties) = rt.get(reason) else {
            panic!("rejection must be named error object");
        };
        assert!(matches!(
            properties.get("name").map(|value| rt.get(*value)),
            Some(MockValue::String(name)) if name == "OperationError"
        ));
        assert!(GPU_STATE.with(|state| state.borrow().requested_limits.is_empty()));
    }

    #[test]
    fn s6_native_and_thread_forwarded_uncaptured_errors_dispatch_and_throw_through_tick() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = binding_created_device(&rt, cx);
        let handler = rt.insert(MockValue::Callable(MockCallable::Handler));
        device_on_uncaptured_error_set::<Engine>(cx, device, handler).expect("set handler");

        let info = GPU_STATE.with(|state| {
            state
                .borrow()
                .uncaptured_error_callback
                .expect("binding callback installed")
        });
        let native_device = fake_device();
        unsafe {
            info.callback.expect("callback")(
                ptr::from_ref(&native_device),
                crate::WGPUErrorType_WGPUErrorType_Validation,
                WGPUStringView::from_bytes(b"native validation"),
                info.userdata1,
                info.userdata2,
            );
            crate::tick::<Engine>(cx, fake_handle(702)).expect("uncaptured tick");
        }
        let error = rt.call_args.borrow()[0];
        assert!(Engine::payload(cx, error, crate::GPU_VALIDATION_ERROR_CLASS).is_some());
        let message = crate::gpu_error_message_get::<Engine>(cx, error).expect("message");
        assert!(
            matches!(rt.get(message), MockValue::String(value) if value == "native validation")
        );

        rt.set_call_error(handler, "handler exploded");
        let forwarder = rt.env.device_event_forwarder();
        let device = SendPtr::new(fake_device());
        std::thread::spawn(move || {
            forwarder
                .forward_uncaptured_error::<Engine>(
                    device.get(),
                    crate::WGPUErrorType_WGPUErrorType_OutOfMemory,
                    "thread error",
                )
                .expect("thread enqueue");
        })
        .join()
        .expect("thread join");
        assert_eq!(
            unsafe { crate::tick::<Engine>(cx, fake_handle(703)) },
            Err(crate::TickError::Engine("handler exploded".to_owned()))
        );
    }

    #[test]
    fn s7_lost_is_cached_maps_every_c_reason_and_settles_once() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let reasons = [
            (
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Unknown,
                "unknown",
            ),
            (
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "destroyed",
            ),
            (
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_CallbackCancelled,
                "unknown",
            ),
            (
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_FailedCreation,
                "unknown",
            ),
        ];
        for (index, (reason, expected)) in reasons.into_iter().enumerate() {
            let native = fake_handle(800 + index);
            let device = unsafe { wrap_device::<Engine>(cx, native) }.expect("device");
            let first = device_lost_get::<Engine>(cx, device).expect("lost");
            let second = device_lost_get::<Engine>(cx, device).expect("lost again");
            assert_eq!(first, second);

            let forwarder = rt.env.device_event_forwarder();
            if index == 0 {
                let native = SendPtr::new(native);
                std::thread::spawn(move || {
                    forwarder
                        .forward_device_lost::<Engine>(native.get(), reason, "lost message")
                        .expect("thread lost enqueue");
                })
                .join()
                .expect("thread join");
            } else {
                forwarder
                    .forward_device_lost::<Engine>(native, reason, "lost message")
                    .expect("lost enqueue");
            }
            unsafe { crate::tick::<Engine>(cx, fake_handle(900 + index)) }.expect("lost tick");
            let info = rt
                .promise_result(first)
                .expect("lost settled")
                .expect("lost resolved");
            let mapped = device_lost_info_reason_get::<Engine>(cx, info).expect("reason");
            let message = device_lost_info_message_get::<Engine>(cx, info).expect("message");
            assert!(matches!(rt.get(mapped), MockValue::String(value) if value == expected));
            assert!(matches!(rt.get(message), MockValue::String(value) if value == "lost message"));

            rt.env
                .device_event_forwarder()
                .forward_device_lost::<Engine>(
                    native,
                    crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                    "late loss",
                )
                .expect("late enqueue");
            unsafe { crate::tick::<Engine>(cx, fake_handle(950 + index)) }.expect("late tick");
            assert_eq!(rt.promise_result(first), Some(Ok(info)));
            assert_eq!(
                rt.settlement_attempts.borrow().get(&first),
                Some(&1),
                "late loss must not attempt to settle the promise twice"
            );
        }
    }

    #[test]
    fn s7_binding_created_device_lost_callback_resolves_cached_promise() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = binding_created_device(&rt, cx);
        let promise = device_lost_get::<Engine>(cx, device).expect("lost");
        let info = GPU_STATE.with(|state| {
            state
                .borrow()
                .device_lost_callback
                .expect("binding lost callback installed")
        });
        assert_eq!(
            info.mode,
            crate::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents
        );
        let native_device = fake_device();
        unsafe {
            info.callback.expect("callback")(
                ptr::from_ref(&native_device),
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                WGPUStringView::from_bytes(b"native lost"),
                info.userdata1,
                info.userdata2,
            );
            crate::tick::<Engine>(cx, fake_handle(980)).expect("lost tick");
        }
        let lost = rt
            .promise_result(promise)
            .expect("settled")
            .expect("resolved");
        let reason = device_lost_info_reason_get::<Engine>(cx, lost).expect("reason");
        let message = device_lost_info_message_get::<Engine>(cx, lost).expect("message");
        assert!(matches!(rt.get(reason), MockValue::String(value) if value == "destroyed"));
        assert!(matches!(rt.get(message), MockValue::String(value) if value == "native lost"));
    }

    #[test]
    fn c1_callback_userdata_strong_survives_registry_drop_until_late_lost_callback() {
        reset_gpu();
        let settlements = Arc::new(crate::SettlementQueue::new());
        let env = crate::Environment::new(dispatch(), Arc::new(crate::ReleaseQueue::new()));
        let events = DeviceEventState::<Engine>::new(Arc::clone(&settlements));
        env.register_device_events(fake_device(), Arc::clone(&events));
        let weak = Arc::downgrade(&events);
        let userdata = Arc::into_raw(Arc::clone(&events)).cast_mut().cast();

        drop(events);
        drop(env);
        assert_eq!(
            Weak::strong_count(&weak),
            1,
            "userdata owns the last strong ref"
        );

        unsafe {
            crate::device_lost_callback::<Engine>(
                ptr::null(),
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_CallbackCancelled,
                WGPUStringView::from_bytes(b"late teardown loss"),
                userdata,
                ptr::null_mut(),
            );
        }
        assert_eq!(
            Weak::strong_count(&weak),
            1,
            "queued record owns state after callback"
        );

        let rt = runtime();
        settlements.release_pending::<Engine>(rt.context());
        assert_eq!(
            Weak::strong_count(&weak),
            0,
            "dead-queue record releases state"
        );
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.device_releases, 0);
            assert_eq!(state.buffer_releases, 0);
        });
    }

    #[test]
    fn c1_prepare_failure_empties_engine_values_before_late_lost_callback() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(1_233))),
        )
        .expect("adapter");
        rt.fail_new_instance.set(Some(crate::GPU_DEVICE_CLASS));
        let promise = adapter_request_device::<Engine>(cx, adapter, &[]).expect("requestDevice");
        unsafe { crate::tick::<Engine>(cx, fake_handle(1_234)) }.expect("prepare-failure tick");
        assert!(matches!(rt.promise_result(promise), Some(Err(_))));
        assert!(
            rt.duplicated_values.borrow().is_empty(),
            "failed wrapper preparation must release its lost-promise hold"
        );

        let info = GPU_STATE.with(|state| {
            state
                .borrow()
                .device_lost_callback
                .expect("binding lost callback installed")
        });
        unsafe {
            info.callback.expect("callback")(
                ptr::null(),
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_FailedCreation,
                WGPUStringView::from_bytes(b"late failed creation"),
                info.userdata1,
                info.userdata2,
            );
            crate::tick::<Engine>(cx, fake_handle(1_235)).expect("late lost tick");
        }
        assert!(rt.duplicated_values.borrow().is_empty());
    }

    #[test]
    fn m2_address_reuse_forwards_only_to_the_new_device_state() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let native = fake_handle(1_234);
        let first_device = unsafe { wrap_device::<Engine>(cx, native) }.expect("first device");
        let first_promise = device_lost_get::<Engine>(cx, first_device).expect("first lost");
        let first_events = Engine::payload(cx, first_device, crate::GPU_DEVICE_CLASS)
            .and_then(|payload| payload.downcast_ref::<DevicePayload<Engine>>())
            .map(|payload| Arc::clone(&payload.events))
            .expect("first events");
        let first_payload =
            Engine::payload(cx, first_device, crate::GPU_DEVICE_CLASS).expect("first payload");
        crate::release_payload_values::<Engine>(first_payload, &mut |value| {
            Engine::release_value(cx, value);
        });
        finalize_device::<Engine>(
            Box::new(DevicePayload::<Engine>::new(native, first_events)),
            &rt.env,
        );
        rt.env
            .device_event_forwarder()
            .forward_device_lost::<Engine>(
                native,
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "first loss",
            )
            .expect("first forward");
        rt.env
            .settlements()
            .drain::<Engine>(cx)
            .expect("first drain");
        assert_eq!(
            rt.settlement_attempts.borrow().get(&first_promise),
            Some(&1)
        );
        assert_eq!(
            rt.env
                .device_event_forwarder()
                .forward_device_lost::<Engine>(
                    native,
                    crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                    "stale loss",
                ),
            Err(QueueError::UnknownDevice),
            "finalized and settled entry must be pruned"
        );

        let second_device = unsafe { wrap_device::<Engine>(cx, native) }.expect("second device");
        let second_promise = device_lost_get::<Engine>(cx, second_device).expect("second lost");
        rt.env
            .device_event_forwarder()
            .forward_device_lost::<Engine>(
                native,
                crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "second loss",
            )
            .expect("second forward");
        rt.env
            .settlements()
            .drain::<Engine>(cx)
            .expect("second drain");
        assert_eq!(
            rt.settlement_attempts.borrow().get(&first_promise),
            Some(&1)
        );
        assert_eq!(
            rt.settlement_attempts.borrow().get(&second_promise),
            Some(&1)
        );
    }

    #[test]
    fn uncaptured_forwarder_rejects_unmapped_types_and_unknown_maps_to_internal_error() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let native = fake_handle(1_235);
        let _device = unsafe { wrap_device::<Engine>(cx, native) }.expect("device");
        let forwarder = rt.env.device_event_forwarder();
        for type_ in [
            crate::WGPUErrorType_WGPUErrorType_NoError,
            0xdead_beef_u32 as crate::WGPUErrorType,
        ] {
            assert_eq!(
                forwarder.forward_uncaptured_error::<Engine>(native, type_, "invalid"),
                Err(QueueError::InvalidUncapturedErrorType(type_))
            );
        }
        let error = crate::new_gpu_error::<Engine>(
            cx,
            crate::WGPUErrorType_WGPUErrorType_Unknown,
            "unknown backend error".to_owned(),
        )
        .expect("unknown mapping");
        assert!(Engine::payload(cx, error, crate::GPU_INTERNAL_ERROR_CLASS).is_some());
    }

    #[test]
    fn non_callable_uncaptured_handler_is_coerced_to_null() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_handle(1_236)) }.expect("device");
        let handler = rt.insert(MockValue::Callable(MockCallable::Handler));
        device_on_uncaptured_error_set::<Engine>(cx, device, handler).expect("set handler");
        device_on_uncaptured_error_set::<Engine>(cx, device, rt.number(7.0))
            .expect("coerce non-callable");
        let value = device_on_uncaptured_error_get::<Engine>(cx, device).expect("get handler");
        assert!(Engine::is_null(cx, value));
    }

    #[test]
    fn throwing_uncaptured_handler_does_not_skip_events_or_tick_release_step() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let native = fake_handle(1_237);
        let device = unsafe { wrap_device::<Engine>(cx, native) }.expect("device");
        let handler = rt.insert(MockValue::Callable(MockCallable::Handler));
        device_on_uncaptured_error_set::<Engine>(cx, device, handler).expect("set handler");
        rt.set_call_error(handler, "first handler throw");
        let forwarder = rt.env.device_event_forwarder();
        for message in ["first", "second"] {
            forwarder
                .forward_uncaptured_error::<Engine>(
                    native,
                    crate::WGPUErrorType_WGPUErrorType_Validation,
                    message,
                )
                .expect("forward");
        }
        rt.queue()
            .enqueue(crate::ReleaseRequest::Device {
                device: fake_handle(1_238),
                gpu: dispatch(),
            })
            .expect("enqueue release");
        let calls_before = rt.calls.get();
        assert_eq!(
            unsafe { crate::tick::<Engine>(cx, fake_handle(1_239)) },
            Err(crate::TickError::Engine("first handler throw".to_owned()))
        );
        assert_eq!(
            rt.calls.get() - calls_before,
            2,
            "both events were dispatched"
        );
        assert_eq!(rt.queue().len(), Ok(0), "tick step 4 drained releases");
        GPU_STATE.with(|state| assert_eq!(state.borrow().device_releases, 1));
    }

    #[test]
    fn forwarders_deliver_all_records_while_main_thread_ticks() {
        reset_gpu();
        const N: usize = 8;
        let rt = runtime();
        let cx = rt.context();
        let handler = rt.insert(MockValue::Callable(MockCallable::Handler));
        let barrier = Arc::new(std::sync::Barrier::new(N + 1));
        let mut promises = Vec::new();
        let mut threads = Vec::new();
        for index in 0..N {
            let native = fake_handle(2_000 + index);
            let device = unsafe { wrap_device::<Engine>(cx, native) }.expect("device");
            device_on_uncaptured_error_set::<Engine>(cx, device, handler).expect("handler");
            promises.push(device_lost_get::<Engine>(cx, device).expect("lost"));
            let forwarder = rt.env.device_event_forwarder();
            let barrier = Arc::clone(&barrier);
            let native = SendPtr::new(native);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                let native = native.get();
                forwarder
                    .forward_uncaptured_error::<Engine>(
                        native,
                        crate::WGPUErrorType_WGPUErrorType_Validation,
                        format!("error-{index}"),
                    )
                    .expect("forward error");
                forwarder
                    .forward_device_lost::<Engine>(
                        native,
                        crate::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                        format!("lost-{index}"),
                    )
                    .expect("forward loss");
            }));
        }
        barrier.wait();
        for _ in 0..1_000 {
            unsafe { crate::tick::<Engine>(cx, fake_handle(2_100)) }.expect("concurrent tick");
            if threads.iter().all(std::thread::JoinHandle::is_finished)
                && rt
                    .env
                    .settlements()
                    .requests
                    .lock()
                    .expect("queue")
                    .is_empty()
            {
                break;
            }
            std::thread::yield_now();
        }
        for thread in threads {
            thread.join().expect("forward thread");
        }
        unsafe { crate::tick::<Engine>(cx, fake_handle(2_101)) }.expect("final tick");

        let mut messages = rt
            .call_history
            .borrow()
            .iter()
            .filter_map(|args| args.first().copied())
            .filter_map(|error| {
                Engine::payload(cx, error, crate::GPU_VALIDATION_ERROR_CLASS)
                    .and_then(|payload| payload.downcast_ref::<ErrorPayload>())
                    .map(|payload| payload.message.clone())
            })
            .collect::<Vec<_>>();
        messages.sort();
        let mut expected = (0..N)
            .map(|index| format!("error-{index}"))
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(messages, expected);
        for promise in promises {
            assert_eq!(rt.settlement_attempts.borrow().get(&promise), Some(&1));
        }
    }

    #[test]
    fn s7_teardown_with_pending_lost_is_clean() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_handle(999)) }.expect("device");
        let _ = device_lost_get::<Engine>(cx, device).expect("pending lost");
    }

    #[test]
    fn t6_gpu_color_and_render_pass_descriptors_cover_both_union_arms_and_holes() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let view = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_VIEW_CLASS,
            Box::new(TextureViewPayload {
                texture_view: fake_handle(501),
                texture: fake_handle(502),
            }),
        )
        .expect("texture view");

        let dict_color = descriptor(
            &rt,
            &[
                ("r", rt.number(0.1)),
                ("g", rt.number(0.2)),
                ("b", rt.number(0.3)),
                ("a", rt.number(0.4)),
            ],
        );
        let sequence_color = rt.set_like(&[
            rt.number(0.1),
            rt.number(0.2),
            rt.number(0.3),
            rt.number(0.4),
        ]);
        let dict = convert_gpu_color::<Engine>(cx, dict_color).expect("dict color");
        let sequence = convert_gpu_color::<Engine>(cx, sequence_color).expect("sequence color");
        assert_eq!((dict.r, dict.g, dict.b, dict.a), (0.1, 0.2, 0.3, 0.4));
        assert_eq!(
            (sequence.r, sequence.g, sequence.b, sequence.a),
            (dict.r, dict.g, dict.b, dict.a)
        );
        assert!(convert_gpu_color::<Engine>(
            cx,
            rt.set_like(&[rt.number(1.0), rt.number(2.0), rt.number(3.0)])
        )
        .is_err());
        assert!(convert_color_dict::<Engine>(
            cx,
            descriptor(
                &rt,
                &[
                    ("r", rt.number(0.0)),
                    ("g", rt.number(0.0)),
                    ("b", rt.number(0.0))
                ]
            )
        )
        .is_err());
        assert!(convert_gpu_color::<Engine>(
            cx,
            rt.set_like(&[
                rt.number(f64::INFINITY),
                rt.number(0.0),
                rt.number(0.0),
                rt.number(1.0)
            ])
        )
        .is_err());

        let color_attachment = descriptor(
            &rt,
            &[
                ("view", view),
                ("depthSlice", rt.number(7.0)),
                ("resolveTarget", view),
                ("clearValue", dict_color),
                ("loadOp", rt.string("clear")),
                ("storeOp", rt.string("discard")),
            ],
        );
        let mut created_texture_views = crate::CreatedTextureViewCapture::new::<Engine>(cx);
        let color = convert_render_pass_color_attachment::<Engine>(
            cx,
            color_attachment,
            &mut created_texture_views,
        )
        .expect("color attachment");
        assert!(color.nextInChain.is_null());
        assert_eq!(color.view, fake_handle(501));
        assert_eq!(color.depthSlice, 7);
        assert_eq!(color.resolveTarget, fake_handle(501));
        assert_eq!(color.loadOp, crate::WGPULoadOp_WGPULoadOp_Clear);
        assert_eq!(color.storeOp, crate::WGPUStoreOp_WGPUStoreOp_Discard);
        assert_eq!(
            (
                color.clearValue.r,
                color.clearValue.g,
                color.clearValue.b,
                color.clearValue.a
            ),
            (0.1, 0.2, 0.3, 0.4)
        );
        assert!(convert_render_pass_color_attachment::<Engine>(
            cx,
            descriptor(&rt, &[("view", view), ("storeOp", rt.string("store"))]),
            &mut created_texture_views,
        )
        .is_err());

        let depth_attachment = descriptor(
            &rt,
            &[
                ("view", view),
                ("depthClearValue", rt.number(0.75)),
                ("depthLoadOp", rt.string("clear")),
                ("depthStoreOp", rt.string("store")),
                ("depthReadOnly", rt.bool(true)),
                ("stencilClearValue", rt.number(23.0)),
                ("stencilLoadOp", rt.string("load")),
                ("stencilStoreOp", rt.string("discard")),
                ("stencilReadOnly", rt.bool(true)),
            ],
        );
        let depth = convert_render_pass_depth_stencil_attachment::<Engine>(
            cx,
            depth_attachment,
            &mut created_texture_views,
        )
        .expect("depth attachment");
        assert!(depth.nextInChain.is_null());
        assert_eq!(depth.view, fake_handle(501));
        assert_eq!(depth.depthLoadOp, crate::WGPULoadOp_WGPULoadOp_Clear);
        assert_eq!(depth.depthStoreOp, crate::WGPUStoreOp_WGPUStoreOp_Store);
        assert_eq!(depth.depthClearValue, 0.75);
        assert_eq!(depth.depthReadOnly, 1);
        assert_eq!(depth.stencilLoadOp, crate::WGPULoadOp_WGPULoadOp_Load);
        assert_eq!(depth.stencilStoreOp, crate::WGPUStoreOp_WGPUStoreOp_Discard);
        assert_eq!(depth.stencilClearValue, 23);
        assert_eq!(depth.stencilReadOnly, 1);

        let attachments = rt.set_like(&[color_attachment, rt.null()]);
        let pass = descriptor(
            &rt,
            &[
                ("label", rt.string("render-pass")),
                ("colorAttachments", attachments),
                ("depthStencilAttachment", depth_attachment),
            ],
        );
        let arena = Arena::new();
        let native =
            convert_render_pass_descriptor::<Engine>(cx, pass, &arena, &mut created_texture_views)
                .expect("render pass descriptor");
        assert!(native.nextInChain.is_null());
        assert_eq!(read_view(native.label), b"render-pass");
        assert_eq!(native.colorAttachmentCount, 2);
        let colors = unsafe { std::slice::from_raw_parts(native.colorAttachments, 2) };
        assert_eq!(colors[0].view, fake_handle(501));
        assert!(colors[1].view.is_null());
        assert_eq!(colors[1].depthSlice, u32::MAX);
        assert!(colors[1].resolveTarget.is_null());
        assert_eq!(colors[1].loadOp, crate::WGPULoadOp_WGPULoadOp_Undefined);
        assert_eq!(colors[1].storeOp, crate::WGPUStoreOp_WGPUStoreOp_Undefined);
        assert_eq!(
            (
                colors[1].clearValue.r,
                colors[1].clearValue.g,
                colors[1].clearValue.b,
                colors[1].clearValue.a
            ),
            (0.0, 0.0, 0.0, 0.0)
        );
        let native_depth = unsafe { native.depthStencilAttachment.as_ref() }.expect("depth");
        assert_eq!(native_depth.view, fake_handle(501));

        let texture = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_CLASS,
            Box::new(TexturePayload {
                texture: fake_handle(503),
                destroyed: AtomicBool::new(false),
            }),
        )
        .expect("texture");
        let implicit_color = convert_render_pass_color_attachment::<Engine>(
            cx,
            descriptor(
                &rt,
                &[
                    ("view", texture),
                    ("resolveTarget", texture),
                    ("loadOp", rt.string("load")),
                    ("storeOp", rt.string("store")),
                ],
            ),
            &mut created_texture_views,
        )
        .expect("texture color attachment");
        assert!(!implicit_color.view.is_null());
        assert!(!implicit_color.resolveTarget.is_null());
        assert_ne!(implicit_color.view, implicit_color.resolveTarget);
        let implicit_depth = convert_render_pass_depth_stencil_attachment::<Engine>(
            cx,
            descriptor(&rt, &[("view", texture)]),
            &mut created_texture_views,
        )
        .expect("texture depth attachment");
        assert!(!implicit_depth.view.is_null());
        GPU_STATE.with(|state| assert_eq!(state.borrow().null_texture_view_descriptors, 3));
        drop(created_texture_views);
        assert_eq!(rt.queue().drain().expect("release attachment views"), 3);
        GPU_STATE.with(|state| assert_eq!(state.borrow().texture_view_releases, 3));
    }

    #[test]
    fn t7_texel_copy_descriptors_assert_every_c_field_and_reject_bad_inputs() {
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(1024.0)), ("usage", rt.number(12.0))],
            )],
        )
        .expect("buffer");
        let texture = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_CLASS,
            Box::new(TexturePayload {
                texture: fake_handle(601),
                destroyed: AtomicBool::new(false),
            }),
        )
        .expect("texture");

        let layout_value = descriptor(
            &rt,
            &[
                ("offset", rt.number(32.0)),
                ("bytesPerRow", rt.number(256.0)),
                ("rowsPerImage", rt.number(9.0)),
            ],
        );
        let layout =
            convert_texel_copy_buffer_layout::<Engine>(cx, layout_value).expect("copy layout");
        assert_eq!(layout.offset, 32);
        assert_eq!(layout.bytesPerRow, 256);
        assert_eq!(layout.rowsPerImage, 9);
        let absent = convert_texel_copy_buffer_layout::<Engine>(cx, descriptor(&rt, &[]))
            .expect("default copy layout");
        assert_eq!(absent.offset, 0);
        assert_eq!(absent.bytesPerRow, crate::WGPU_COPY_STRIDE_UNDEFINED);
        assert_eq!(absent.rowsPerImage, crate::WGPU_COPY_STRIDE_UNDEFINED);

        let buffer_info_value = descriptor(
            &rt,
            &[
                ("buffer", buffer),
                ("offset", rt.number(32.0)),
                ("bytesPerRow", rt.number(256.0)),
                ("rowsPerImage", rt.number(9.0)),
            ],
        );
        let buffer_info =
            convert_texel_copy_buffer_info::<Engine>(cx, buffer_info_value).expect("buffer info");
        assert_eq!(
            buffer_info.buffer,
            crate::buffer_handle::<Engine>(cx, buffer).expect("handle")
        );
        assert_eq!(buffer_info.layout.offset, 32);
        assert_eq!(buffer_info.layout.bytesPerRow, 256);
        assert_eq!(buffer_info.layout.rowsPerImage, 9);

        let origin = descriptor(
            &rt,
            &[
                ("x", rt.number(2.0)),
                ("y", rt.number(3.0)),
                ("z", rt.number(4.0)),
            ],
        );
        let texture_info_value = descriptor(
            &rt,
            &[
                ("texture", texture),
                ("mipLevel", rt.number(5.0)),
                ("origin", origin),
                ("aspect", rt.string("depth-only")),
            ],
        );
        let texture_info = convert_texel_copy_texture_info::<Engine>(cx, texture_info_value)
            .expect("texture info");
        assert_eq!(texture_info.texture, fake_handle(601));
        assert_eq!(texture_info.mipLevel, 5);
        assert_eq!(
            (
                texture_info.origin.x,
                texture_info.origin.y,
                texture_info.origin.z
            ),
            (2, 3, 4)
        );
        assert_eq!(
            texture_info.aspect,
            crate::WGPUTextureAspect_WGPUTextureAspect_DepthOnly
        );

        assert!(convert_texel_copy_buffer_info::<Engine>(cx, descriptor(&rt, &[])).is_err());
        assert!(convert_texel_copy_texture_info::<Engine>(cx, descriptor(&rt, &[])).is_err());
        assert!(convert_texel_copy_texture_info::<Engine>(
            cx,
            descriptor(&rt, &[("texture", texture), ("aspect", rt.string("bad"))])
        )
        .is_err());
        assert!(convert_texel_copy_texture_info::<Engine>(
            cx,
            descriptor(
                &rt,
                &[
                    ("texture", texture),
                    (
                        "origin",
                        rt.set_like(&[
                            rt.number(0.0),
                            rt.number(0.0),
                            rt.number(0.0),
                            rt.number(0.0)
                        ])
                    )
                ]
            )
        )
        .is_err());
    }

    #[test]
    fn t6_compute_pass_validation_only_ops_call_every_ffi_pointer() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let pass = crate::command_encoder_begin_compute_pass::<Engine>(cx, encoder, &[])
            .expect("compute pass");
        let pipeline = Engine::new_instance(
            cx,
            crate::GPU_COMPUTE_PIPELINE_CLASS,
            Box::new(ComputePipelinePayload {
                pipeline: fake_handle(690),
                module: ptr::null_mut(),
                layout: ptr::null_mut(),
            }),
        )
        .expect("compute pipeline");
        let bind_group = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_CLASS,
            Box::new(BindGroupPayload {
                bind_group: fake_handle(691),
                layout: ptr::null_mut(),
                buffers: Vec::new(),
                samplers: Vec::new(),
                texture_views: Vec::new(),
                created_texture_views: Vec::new(),
            }),
        )
        .expect("bind group");

        crate::compute_pass_set_pipeline::<Engine>(cx, pass, &[pipeline]).expect("pipeline");
        crate::compute_pass_set_bind_group::<Engine>(cx, pass, &[rt.number(0.0), bind_group])
            .expect("bind group");
        crate::compute_pass_dispatch_workgroups::<Engine>(cx, pass, &[rt.number(2.0)])
            .expect("dispatch");
        crate::compute_pass_end::<Engine>(cx, pass, &[]).expect("end");

        GPU_STATE.with(|state| {
            let state = state.borrow();
            for name in [
                "compute_set_pipeline",
                "compute_set_bind_group",
                "dispatch_workgroups",
                "compute_end",
            ] {
                assert_eq!(state.recording_calls.get(name), Some(&1), "{name}");
            }
        });
    }

    #[test]
    fn indirect_dispatch_and_draw_forward_handles_offsets_and_reject_bad_arguments() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(64.0)), ("usage", rt.number(256.0))],
            )],
        )
        .expect("indirect buffer");
        let native = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let compute_pass = crate::command_encoder_begin_compute_pass::<Engine>(cx, encoder, &[])
            .expect("compute pass");

        crate::compute_pass_dispatch_workgroups_indirect::<Engine>(
            cx,
            compute_pass,
            &[buffer, rt.number(4_294_967_296.0)],
        )
        .expect("indirect dispatch");
        assert_eq!(
            crate::compute_pass_dispatch_workgroups_indirect::<Engine>(
                cx,
                compute_pass,
                &[rt.null(), rt.number(0.0)],
            )
            .expect_err("non-buffer argument"),
            "TypeError: GPUBuffer is required"
        );
        assert_eq!(
            crate::compute_pass_dispatch_workgroups_indirect::<Engine>(
                cx,
                compute_pass,
                &[buffer, rt.number(-1.0)],
            )
            .expect_err("negative offset"),
            "TypeError: indirectOffset"
        );
        crate::compute_pass_end::<Engine>(cx, compute_pass, &[]).expect("end compute pass");

        let render_descriptor = descriptor(&rt, &[("colorAttachments", rt.set_like(&[]))]);
        let render_pass =
            crate::command_encoder_begin_render_pass::<Engine>(cx, encoder, &[render_descriptor])
                .expect("render pass");
        crate::render_pass_draw_indirect::<Engine>(
            cx,
            render_pass,
            &[buffer, rt.number(1_099_511_627_776.0)],
        )
        .expect("indirect draw");
        crate::render_pass_draw_indexed_indirect::<Engine>(
            cx,
            render_pass,
            &[buffer, rt.number(8.0)],
        )
        .expect("indexed indirect draw");
        assert_eq!(
            crate::render_pass_draw_indirect::<Engine>(
                cx,
                render_pass,
                &[rt.string("buffer"), rt.number(0.0)],
            )
            .expect_err("non-buffer draw argument"),
            "TypeError: GPUBuffer is required"
        );
        assert_eq!(
            crate::render_pass_draw_indexed_indirect::<Engine>(
                cx,
                render_pass,
                &[buffer, rt.number(f64::NAN)],
            )
            .expect_err("NaN draw offset"),
            "TypeError: indirectOffset"
        );
        crate::render_pass_end::<Engine>(cx, render_pass, &[]).expect("end render pass");

        let command_buffer =
            command_encoder_finish::<Engine>(cx, encoder, &[]).expect("command buffer");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(
                state.indirect_calls,
                [
                    ("dispatch_workgroups_indirect", native, 4_294_967_296),
                    ("draw_indirect", native, 1_099_511_627_776),
                    ("draw_indexed_indirect", native, 8),
                ]
            );
            assert!(state.encoder_retained_indirect_buffers.is_empty());
            assert_eq!(
                state.command_buffer_retained_indirect_buffers,
                [native, native, native]
            );
        });

        let buffer_state = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .map(|payload| Arc::clone(&payload.state))
            .expect("buffer state");
        crate::finalize_buffer::<Engine>(
            Box::new(BufferPayload {
                state: buffer_state,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("release buffer wrapper"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.buffer_releases, 1);
            assert_eq!(
                state.command_buffer_retained_indirect_buffers,
                [native, native, native]
            );
        });

        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        queue_submit::<Engine>(cx, queue, &[rt.set_like(&[command_buffer])]).expect("submit");
        let command_buffer_state =
            crate::command_buffer_state::<Engine>(cx, command_buffer).expect("command state");
        crate::finalize_command_buffer(
            Box::new(crate::CommandBufferPayload {
                state: command_buffer_state,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain().expect("release command buffer"), 1);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.command_buffer_retained_indirect_buffers.is_empty());
            assert_eq!(state.released_indirect_buffers, [native, native, native]);
        });
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn t6_render_pass_state_machine_and_t7_copy_calls_are_validation_only() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let encoder = device_create_command_encoder::<Engine>(cx, device, &[]).expect("encoder");
        let view = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_VIEW_CLASS,
            Box::new(TextureViewPayload {
                texture_view: fake_handle(701),
                texture: fake_handle(702),
            }),
        )
        .expect("view");
        let texture = Engine::new_instance(
            cx,
            crate::GPU_TEXTURE_CLASS,
            Box::new(TexturePayload {
                texture: fake_handle(702),
                destroyed: AtomicBool::new(false),
            }),
        )
        .expect("texture");
        let attachment = descriptor(
            &rt,
            &[
                ("view", view),
                ("loadOp", rt.string("clear")),
                ("storeOp", rt.string("store")),
                (
                    "clearValue",
                    rt.set_like(&[
                        rt.number(0.0),
                        rt.number(0.0),
                        rt.number(0.0),
                        rt.number(1.0),
                    ]),
                ),
            ],
        );
        let pass_desc = descriptor(&rt, &[("colorAttachments", rt.set_like(&[attachment]))]);
        let pass = crate::command_encoder_begin_render_pass::<Engine>(cx, encoder, &[pass_desc])
            .expect("begin render pass");
        let pipeline = Engine::new_instance(
            cx,
            crate::GPU_RENDER_PIPELINE_CLASS,
            Box::new(RenderPipelinePayload {
                render_pipeline: fake_handle(703),
                vertex_module: ptr::null_mut(),
                fragment_module: ptr::null_mut(),
                layout: ptr::null_mut(),
            }),
        )
        .expect("render pipeline");
        crate::render_pass_set_pipeline::<Engine>(cx, pass, &[pipeline]).expect("pipeline");
        let render_buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(64.0)), ("usage", rt.number(48.0))],
            )],
        )
        .expect("render buffer");
        crate::render_pass_set_vertex_buffer::<Engine>(cx, pass, &[rt.number(0.0), rt.null()])
            .expect("nullable vertex buffer");
        crate::render_pass_set_vertex_buffer::<Engine>(
            cx,
            pass,
            &[
                rt.number(1.0),
                render_buffer,
                rt.number(4_294_967_296.0),
                rt.number(1_099_511_627_776.0),
            ],
        )
        .expect("vertex buffer range");
        crate::render_pass_set_index_buffer::<Engine>(
            cx,
            pass,
            &[render_buffer, rt.string("uint16")],
        )
        .expect("index buffer defaults");
        crate::render_pass_set_index_buffer::<Engine>(
            cx,
            pass,
            &[
                render_buffer,
                rt.string("uint32"),
                rt.number(1_099_511_627_776.0),
                rt.number(4_294_967_296.0),
            ],
        )
        .expect("index buffer GPUSize64 range");
        let bind_group = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_CLASS,
            Box::new(BindGroupPayload {
                bind_group: fake_handle(704),
                layout: ptr::null_mut(),
                buffers: Vec::new(),
                samplers: Vec::new(),
                texture_views: Vec::new(),
                created_texture_views: Vec::new(),
            }),
        )
        .expect("bind group");
        crate::render_pass_set_bind_group::<Engine>(cx, pass, &[rt.number(0.0), bind_group])
            .expect("bind group");
        crate::render_pass_set_viewport::<Engine>(
            cx,
            pass,
            &[
                rt.number(0.0),
                rt.number(0.0),
                rt.number(4.0),
                rt.number(4.0),
                rt.number(0.0),
                rt.number(1.0),
            ],
        )
        .expect("viewport");
        crate::render_pass_set_scissor_rect::<Engine>(
            cx,
            pass,
            &[
                rt.number(0.0),
                rt.number(0.0),
                rt.number(4.0),
                rt.number(4.0),
            ],
        )
        .expect("scissor");
        crate::render_pass_draw::<Engine>(cx, pass, &[rt.number(3.0)]).expect("draw defaults");
        crate::render_pass_draw_indexed::<Engine>(cx, pass, &[rt.number(3.0)])
            .expect("draw indexed defaults");
        assert!(crate::render_pass_set_viewport::<Engine>(
            cx,
            pass,
            &[
                rt.number(f64::INFINITY),
                rt.number(0.0),
                rt.number(4.0),
                rt.number(4.0),
                rt.number(0.0),
                rt.number(1.0),
            ],
        )
        .is_err());
        crate::render_pass_end::<Engine>(cx, pass, &[]).expect("end");
        assert_eq!(
            crate::render_pass_end::<Engine>(cx, pass, &[]).expect_err("double end"),
            "OperationError: GPURenderPassEncoder is ended"
        );
        assert_eq!(
            crate::render_pass_draw::<Engine>(cx, pass, &[rt.number(3.0)])
                .expect_err("use after end"),
            "OperationError: GPURenderPassEncoder is ended"
        );

        let copy_encoder =
            device_create_command_encoder::<Engine>(cx, device, &[]).expect("copy encoder");
        let buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(1024.0)), ("usage", rt.number(12.0))],
            )],
        )
        .expect("copy buffer");
        let buffer_info = descriptor(
            &rt,
            &[
                ("buffer", buffer),
                ("bytesPerRow", rt.number(256.0)),
                ("rowsPerImage", rt.number(1.0)),
            ],
        );
        let texture_info = descriptor(&rt, &[("texture", texture)]);
        let extent = descriptor(&rt, &[("width", rt.number(1.0))]);
        crate::command_encoder_copy_buffer_to_texture::<Engine>(
            cx,
            copy_encoder,
            &[buffer_info, texture_info, extent],
        )
        .expect("buffer to texture validation only");
        crate::command_encoder_copy_texture_to_buffer::<Engine>(
            cx,
            copy_encoder,
            &[texture_info, buffer_info, extent],
        )
        .expect("texture to buffer validation only");
        crate::command_encoder_copy_texture_to_texture::<Engine>(
            cx,
            copy_encoder,
            &[texture_info, texture_info, extent],
        )
        .expect("texture to texture validation only");
        let data = rt.insert(MockValue::ArrayBuffer {
            bytes: vec![0; 256],
            detached: false,
        });
        let queue = device_queue_get::<Engine>(cx, device).expect("queue");
        queue_write_texture::<Engine>(cx, queue, &[texture_info, data, buffer_info, extent])
            .expect("write texture validation only");
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(
                state.vertex_buffer_ranges,
                vec![(0, u64::MAX), (4_294_967_296, 1_099_511_627_776)]
            );
            assert_eq!(
                state.index_buffer_ranges,
                vec![(0, u64::MAX), (1_099_511_627_776, 4_294_967_296)]
            );
            for (name, expected) in [
                ("render_set_pipeline", 1),
                ("render_set_vertex_buffer", 2),
                ("render_set_index_buffer", 2),
                ("render_set_bind_group", 1),
                ("set_viewport", 1),
                ("set_scissor_rect", 1),
                ("draw", 1),
                ("draw_indexed", 1),
                ("render_end", 1),
                ("copy_buffer_to_texture", 1),
                ("copy_texture_to_buffer", 1),
                ("copy_texture_to_texture", 1),
                ("queue_write_texture", 1),
            ] {
                assert_eq!(state.recording_calls.get(name), Some(&expected), "{name}");
            }
        });
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn a4_render_bundle_descriptor_state_execution_reuse_and_release_balance() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let encoder_descriptor = descriptor(
            &rt,
            &[
                ("label", rt.string("bundle encoder")),
                (
                    "colorFormats",
                    rt.set_like(&[rt.string("rgba8unorm"), rt.null(), rt.undefined()]),
                ),
                ("depthStencilFormat", rt.string("depth24plus-stencil8")),
                ("sampleCount", rt.number(4.0)),
                ("depthReadOnly", rt.bool(true)),
                ("stencilReadOnly", rt.bool(true)),
            ],
        );
        let bundle_encoder =
            crate::device_create_render_bundle_encoder::<Engine>(cx, device, &[encoder_descriptor])
                .expect("bundle encoder");
        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().render_bundle_encoder_descriptors,
                vec![RecordedRenderBundleEncoderDescriptor {
                    label: b"bundle encoder".to_vec(),
                    color_formats: vec![
                        crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
                        crate::WGPUTextureFormat_WGPUTextureFormat_Undefined,
                        crate::WGPUTextureFormat_WGPUTextureFormat_Undefined,
                    ],
                    depth_stencil_format:
                        crate::WGPUTextureFormat_WGPUTextureFormat_Depth24PlusStencil8,
                    sample_count: 4,
                    depth_read_only: 1,
                    stencil_read_only: 1,
                }]
            );
        });

        let sparse_descriptor = descriptor(
            &rt,
            &[(
                "colorFormats",
                rt.sparse_array(2, &[(0, rt.string("rgba8unorm"))]),
            )],
        );
        crate::device_create_render_bundle_encoder::<Engine>(cx, device, &[sparse_descriptor])
            .expect("sparse colorFormats array");
        GPU_STATE.with(|state| {
            assert_eq!(
                state
                    .borrow()
                    .render_bundle_encoder_descriptors
                    .last()
                    .expect("sparse descriptor")
                    .color_formats,
                vec![
                    crate::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
                    crate::WGPUTextureFormat_WGPUTextureFormat_Undefined,
                ]
            );
        });

        let methods: Vec<_> = crate::render_bundle_encoder_class::<Engine>()
            .methods
            .iter()
            .map(|method| method.name)
            .collect();
        assert_eq!(
            methods,
            [
                "setPipeline",
                "setVertexBuffer",
                "setIndexBuffer",
                "setBindGroup",
                "draw",
                "drawIndexed",
                "drawIndirect",
                "drawIndexedIndirect",
                "finish",
            ]
        );
        assert!(!methods.contains(&"setViewport"));
        assert!(!methods.contains(&"setScissorRect"));
        assert!(!methods.contains(&"beginOcclusionQuery"));

        let pipeline = Engine::new_instance(
            cx,
            crate::GPU_RENDER_PIPELINE_CLASS,
            Box::new(RenderPipelinePayload {
                render_pipeline: fake_handle(14_001),
                vertex_module: ptr::null_mut(),
                fragment_module: ptr::null_mut(),
                layout: ptr::null_mut(),
            }),
        )
        .expect("render pipeline");
        let buffer = device_create_buffer::<Engine>(
            cx,
            device,
            &[descriptor(
                &rt,
                &[("size", rt.number(16.0)), ("usage", rt.number(48.0))],
            )],
        )
        .expect("buffer");
        let native_buffer = crate::buffer_handle::<Engine>(cx, buffer).expect("native buffer");
        let bind_group = Engine::new_instance(
            cx,
            crate::GPU_BIND_GROUP_CLASS,
            Box::new(BindGroupPayload {
                bind_group: fake_handle(14_002),
                layout: ptr::null_mut(),
                buffers: Vec::new(),
                samplers: Vec::new(),
                texture_views: Vec::new(),
                created_texture_views: Vec::new(),
            }),
        )
        .expect("bind group");
        crate::render_pass_set_pipeline::<Engine>(cx, bundle_encoder, &[pipeline])
            .expect("shared pipeline body");
        crate::render_pass_set_vertex_buffer::<Engine>(
            cx,
            bundle_encoder,
            &[rt.number(0.0), buffer],
        )
        .expect("shared vertex-buffer body");
        crate::render_pass_set_index_buffer::<Engine>(
            cx,
            bundle_encoder,
            &[buffer, rt.string("uint16")],
        )
        .expect("shared index-buffer body");
        crate::render_pass_set_bind_group::<Engine>(
            cx,
            bundle_encoder,
            &[rt.number(0.0), bind_group],
        )
        .expect("shared bind-group body");
        crate::render_pass_draw::<Engine>(cx, bundle_encoder, &[rt.number(3.0)])
            .expect("shared draw body");
        crate::render_pass_draw_indexed::<Engine>(cx, bundle_encoder, &[rt.number(3.0)])
            .expect("shared indexed-draw body");
        crate::render_pass_draw_indirect::<Engine>(
            cx,
            bundle_encoder,
            &[buffer, rt.number(4_294_967_296.0)],
        )
        .expect("shared indirect-draw body");
        crate::render_pass_draw_indexed_indirect::<Engine>(
            cx,
            bundle_encoder,
            &[buffer, rt.number(1_099_511_627_776.0)],
        )
        .expect("shared indexed-indirect-draw body");

        let bundle = crate::render_bundle_encoder_finish::<Engine>(
            cx,
            bundle_encoder,
            &[descriptor(&rt, &[("label", rt.string("finished bundle"))])],
        )
        .expect("finish bundle");
        assert_eq!(
            crate::render_pass_draw::<Engine>(cx, bundle_encoder, &[rt.number(3.0)])
                .expect_err("use after finish"),
            "OperationError: GPURenderBundleEncoder is finished"
        );
        assert_eq!(
            crate::render_bundle_encoder_finish::<Engine>(cx, bundle_encoder, &[])
                .expect_err("double finish"),
            "OperationError: GPURenderBundleEncoder is finished"
        );
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert!(state.bundle_encoder_retained_indirect_buffers.is_empty());
            assert_eq!(
                state.render_bundle_retained_indirect_buffers,
                [native_buffer, native_buffer]
            );
            assert_eq!(
                state.indirect_calls,
                [
                    ("bundle_draw_indirect", native_buffer, 4_294_967_296),
                    (
                        "bundle_draw_indexed_indirect",
                        native_buffer,
                        1_099_511_627_776,
                    ),
                ]
            );
        });

        let command_encoder =
            device_create_command_encoder::<Engine>(cx, device, &[]).expect("command encoder");
        let pass_descriptor = descriptor(&rt, &[("colorAttachments", rt.set_like(&[]))]);
        let pass = crate::command_encoder_begin_render_pass::<Engine>(
            cx,
            command_encoder,
            &[pass_descriptor],
        )
        .expect("render pass");
        let bundles = rt.set_like(&[bundle]);
        crate::render_pass_execute_bundles::<Engine>(cx, pass, &[bundles])
            .expect("first bundle execution");
        crate::render_pass_execute_bundles::<Engine>(cx, pass, &[bundles])
            .expect("bundle is reusable");
        let wrong = rt.set_like(&[pipeline]);
        assert_eq!(
            crate::render_pass_execute_bundles::<Engine>(cx, pass, &[wrong])
                .expect_err("non-bundle element"),
            "TypeError: GPURenderBundle is required"
        );

        let buffer_state = Engine::payload(cx, buffer, crate::GPU_BUFFER_CLASS)
            .and_then(|payload| payload.downcast_ref::<BufferPayload<Engine>>())
            .map(|payload| Arc::clone(&payload.state))
            .expect("buffer state");
        crate::finalize_buffer::<Engine>(
            Box::new(BufferPayload {
                state: buffer_state,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain(), Ok(1));
        GPU_STATE.with(|state| {
            assert_eq!(
                state.borrow().render_bundle_retained_indirect_buffers,
                [native_buffer, native_buffer]
            );
        });

        let bundle_encoder_state =
            Engine::payload(cx, bundle_encoder, crate::GPU_RENDER_BUNDLE_ENCODER_CLASS)
                .and_then(|payload| payload.downcast_ref::<crate::RenderBundleEncoderPayload>())
                .map(|payload| Arc::clone(&payload.state))
                .expect("bundle encoder state");
        crate::finalize_render_bundle_encoder(
            Box::new(crate::RenderBundleEncoderPayload {
                state: bundle_encoder_state,
            }),
            &rt.env,
        );
        let native_bundle = Engine::payload(cx, bundle, crate::GPU_RENDER_BUNDLE_CLASS)
            .and_then(|payload| payload.downcast_ref::<crate::RenderBundlePayload>())
            .map(|payload| payload.render_bundle)
            .expect("native render bundle");
        crate::finalize_render_bundle(
            Box::new(crate::RenderBundlePayload {
                render_bundle: native_bundle,
            }),
            &rt.env,
        );
        assert_eq!(rt.queue().drain(), Ok(2));
        GPU_STATE.with(|state| {
            let state = state.borrow();
            for name in [
                "bundle_set_pipeline",
                "bundle_set_vertex_buffer",
                "bundle_set_index_buffer",
                "bundle_set_bind_group",
                "bundle_draw",
                "bundle_draw_indexed",
                "bundle_draw_indirect",
                "bundle_draw_indexed_indirect",
            ] {
                assert_eq!(state.recording_calls.get(name), Some(&1), "{name}");
            }
            assert_eq!(state.recording_calls.get("execute_bundles"), Some(&2));
            assert_eq!(state.render_bundle_encoder_releases, 1);
            assert_eq!(state.render_bundle_releases, 1);
            assert!(state.render_bundle_retained_indirect_buffers.is_empty());
            assert_eq!(
                state.released_bundle_indirect_buffers,
                [native_buffer, native_buffer]
            );
        });
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn r25_execute_bundles_rechecks_liveness_after_iterator_conversion() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let device = unsafe { wrap_device::<Engine>(cx, fake_device()) }.expect("device");
        let command_encoder =
            device_create_command_encoder::<Engine>(cx, device, &[]).expect("command encoder");
        let pass_descriptor = descriptor(&rt, &[("colorAttachments", rt.set_like(&[]))]);
        let pass = crate::command_encoder_begin_render_pass::<Engine>(
            cx,
            command_encoder,
            &[pass_descriptor],
        )
        .expect("render pass");
        let bundle = Engine::new_instance(
            cx,
            crate::GPU_RENDER_BUNDLE_CLASS,
            Box::new(crate::RenderBundlePayload {
                render_bundle: fake_handle(15_100),
            }),
        )
        .expect("render bundle");
        let bundles = rt.set_like(&[bundle]);
        rt.end_pass_on_next_iteration(pass);

        assert_eq!(
            crate::render_pass_execute_bundles::<Engine>(cx, pass, &[bundles])
                .expect_err("iterator ended pass"),
            "OperationError: GPURenderPassEncoder is ended"
        );
        GPU_STATE.with(|state| {
            assert_eq!(
                state
                    .borrow()
                    .recording_calls
                    .get("execute_bundles")
                    .copied()
                    .unwrap_or(0),
                0
            );
        });
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn i2_i5_introspection_caches_copies_and_balances_free_members() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let events = DeviceEventState::<Engine>::new(Arc::clone(rt.env.settlements()));
        let device = Engine::new_instance(
            cx,
            crate::GPU_DEVICE_CLASS,
            Box::new(DevicePayload::<Engine>::new(fake_device(), events)),
        )
        .expect("device");
        let adapter = Engine::new_instance(
            cx,
            crate::GPU_ADAPTER_CLASS,
            Box::new(AdapterPayload::<Engine>::new(fake_handle(700))),
        )
        .expect("adapter");

        let device_features = crate::device_features_get::<Engine>(cx, device).expect("features");
        let repeated = crate::device_features_get::<Engine>(cx, device).expect("cached features");
        assert_eq!(rt.canonical(device_features), rt.canonical(repeated));
        let MockValue::Iterable { values, .. } = rt.get(device_features) else {
            panic!("features must be Set-like");
        };
        let names = values
            .into_iter()
            .map(|value| match rt.get(value) {
                MockValue::String(value) => value,
                _ => panic!("feature must be a string"),
            })
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["depth-clip-control", "timestamp-query"]);

        let device_limits = crate::device_limits_get::<Engine>(cx, device).expect("limits");
        assert_eq!(
            rt.canonical(device_limits),
            rt.canonical(crate::device_limits_get::<Engine>(cx, device).expect("cached limits"))
        );
        let limit_spec = crate::supported_limits_class::<Engine>();
        let expected_limits = [
            ("maxTextureDimension1D", 1.0),
            ("maxTextureDimension2D", 2.0),
            ("maxTextureDimension3D", 3.0),
            ("maxTextureArrayLayers", 4.0),
            ("maxBindGroups", 5.0),
            ("maxBindGroupsPlusVertexBuffers", 6.0),
            ("maxImmediateSize", 7.0),
            ("maxBindingsPerBindGroup", 8.0),
            ("maxDynamicUniformBuffersPerPipelineLayout", 9.0),
            ("maxDynamicStorageBuffersPerPipelineLayout", 10.0),
            ("maxSampledTexturesPerShaderStage", 11.0),
            ("maxSamplersPerShaderStage", 12.0),
            ("maxStorageBuffersPerShaderStage", 13.0),
            ("maxStorageBuffersInVertexStage", 33.0),
            ("maxStorageBuffersInFragmentStage", 35.0),
            ("maxStorageTexturesPerShaderStage", 14.0),
            ("maxStorageTexturesInVertexStage", 34.0),
            ("maxStorageTexturesInFragmentStage", 36.0),
            ("maxUniformBuffersPerShaderStage", 15.0),
            ("maxUniformBufferBindingSize", 16.0),
            ("maxStorageBufferBindingSize", 17.0),
            ("minUniformBufferOffsetAlignment", 256.0),
            ("minStorageBufferOffsetAlignment", 19.0),
            ("maxVertexBuffers", 20.0),
            ("maxBufferSize", 21.0),
            ("maxVertexAttributes", 22.0),
            ("maxVertexBufferArrayStride", 23.0),
            ("maxInterStageShaderVariables", 24.0),
            ("maxColorAttachments", 25.0),
            ("maxColorAttachmentBytesPerSample", 26.0),
            ("maxComputeWorkgroupStorageSize", 27.0),
            ("maxComputeInvocationsPerWorkgroup", 28.0),
            ("maxComputeWorkgroupSizeX", 29.0),
            ("maxComputeWorkgroupSizeY", 30.0),
            ("maxComputeWorkgroupSizeZ", 31.0),
            ("maxComputeWorkgroupsPerDimension", 32.0),
        ];
        assert_eq!(limit_spec.properties.len(), expected_limits.len());
        for (property, (expected_name, expected_value)) in
            limit_spec.properties.iter().zip(expected_limits)
        {
            assert_eq!(property.name, expected_name);
            let value =
                property.get.expect("limit getter")(cx, device_limits).expect(property.name);
            assert!(
                matches!(rt.get(value), MockValue::Number(value) if value == expected_value),
                "{} must equal {expected_value}",
                property.name
            );
        }

        let device_info =
            crate::device_adapter_info_get::<Engine>(cx, device).expect("adapterInfo");
        assert_eq!(
            rt.canonical(device_info),
            rt.canonical(
                crate::device_adapter_info_get::<Engine>(cx, device).expect("cached info")
            )
        );
        let vendor = crate::adapter_info_vendor::<Engine>(cx, device_info).expect("vendor");
        assert!(matches!(rt.get(vendor), MockValue::String(value) if value == "mock-vendor"));
        let fallback =
            crate::adapter_info_is_fallback::<Engine>(cx, device_info).expect("fallback");
        assert!(matches!(rt.get(fallback), MockValue::Bool(true)));

        let adapter_features =
            crate::adapter_features_get::<Engine>(cx, adapter).expect("adapter features");
        assert_eq!(
            rt.canonical(adapter_features),
            rt.canonical(
                crate::adapter_features_get::<Engine>(cx, adapter)
                    .expect("cached adapter features")
            )
        );
        let adapter_limits =
            crate::adapter_limits_get::<Engine>(cx, adapter).expect("adapter limits");
        assert_eq!(
            rt.canonical(adapter_limits),
            rt.canonical(
                crate::adapter_limits_get::<Engine>(cx, adapter).expect("cached adapter limits")
            )
        );
        let adapter_info = crate::adapter_info_get::<Engine>(cx, adapter).expect("adapter info");
        assert_eq!(
            rt.canonical(adapter_info),
            rt.canonical(
                crate::adapter_info_get::<Engine>(cx, adapter).expect("cached adapter info")
            )
        );

        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.supported_features_free_members, 2);
            assert_eq!(state.adapter_info_free_members, 2);
        });
        let adapter_payload =
            Engine::payload(cx, adapter, crate::GPU_ADAPTER_CLASS).expect("adapter payload");
        crate::release_payload_values::<Engine>(adapter_payload, &mut |value| {
            Engine::release_value(cx, value)
        });
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn i3_limits_failure_is_operation_error() {
        reset_gpu();
        GPU_STATE.with(|state| state.borrow_mut().limits_fail = true);
        let rt = runtime();
        let cx = rt.context();
        let events = DeviceEventState::<Engine>::new(Arc::clone(rt.env.settlements()));
        let device = Engine::new_instance(
            cx,
            crate::GPU_DEVICE_CLASS,
            Box::new(DevicePayload::<Engine>::new(fake_device(), events)),
        )
        .expect("device");
        assert_eq!(
            crate::device_limits_get::<Engine>(cx, device).expect_err("limits must fail"),
            "OperationError: native limits query failed"
        );
    }

    #[test]
    fn i3_oversized_limit_is_operation_error() {
        reset_gpu();
        GPU_STATE.with(|state| state.borrow_mut().limits_oversized = true);
        let rt = runtime();
        let cx = rt.context();
        let events = DeviceEventState::<Engine>::new(Arc::clone(rt.env.settlements()));
        let device = Engine::new_instance(
            cx,
            crate::GPU_DEVICE_CLASS,
            Box::new(DevicePayload::<Engine>::new(fake_device(), events)),
        )
        .expect("device");
        let limits = crate::device_limits_get::<Engine>(cx, device).expect("limits query");
        assert_eq!(
            crate::limit_max_buffer_size::<Engine>(cx, limits)
                .expect_err("oversized limit must fail loudly"),
            "OperationError: WebGPU limit exceeds JavaScript's exact integer range"
        );
        release_device_held_values(&rt, cx, device);
    }

    #[test]
    fn i6_pipeline_layout_is_new_and_releases_with_parent_retention() {
        reset_gpu();
        let rt = runtime();
        let cx = rt.context();
        let pipeline_handle = fake_handle(900);
        let pipeline = Engine::new_instance(
            cx,
            crate::GPU_COMPUTE_PIPELINE_CLASS,
            Box::new(ComputePipelinePayload {
                pipeline: pipeline_handle,
                module: fake_handle(901),
                layout: ptr::null_mut(),
            }),
        )
        .expect("pipeline");
        let index = rt.number(0.0);
        let first = crate::compute_pipeline_get_bind_group_layout::<Engine>(cx, pipeline, &[index])
            .expect("first layout");
        let second =
            crate::compute_pipeline_get_bind_group_layout::<Engine>(cx, pipeline, &[index])
                .expect("second layout");
        assert_ne!(rt.canonical(first), rt.canonical(second));
        for value in [first, second] {
            let payload = Engine::payload(cx, value, crate::GPU_BIND_GROUP_LAYOUT_CLASS)
                .and_then(|payload| payload.downcast_ref::<BindGroupLayoutPayload>())
                .expect("layout payload");
            crate::finalize_bind_group_layout(
                Box::new(BindGroupLayoutPayload {
                    layout: payload.layout,
                    parent_pipeline: payload.parent_pipeline,
                }),
                &rt.env,
            );
        }
        assert_eq!(rt.queue().drain().expect("drain"), 2);
        GPU_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.compute_pipeline_add_refs, 2);
            assert_eq!(state.bind_group_layout_releases, 2);
            assert_eq!(state.compute_pipeline_releases, 2);
        });
        let invalid = rt.number(u32::MAX as f64);
        assert_eq!(
            crate::compute_pipeline_get_bind_group_layout::<Engine>(cx, pipeline, &[invalid])
                .expect_err("null layout must fail"),
            "OperationError: getBindGroupLayout returned null"
        );
    }
}
