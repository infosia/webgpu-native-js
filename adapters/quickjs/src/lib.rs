#![warn(missing_docs)]

//! QuickJS adapter for `webgpu-native-js`.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use webgpu_native_js_core as core;
use webgpu_native_js_core::__gpu_dispatch_from_ffi;
use webgpu_native_js_core::JsEngine;
use webgpu_native_js_ffi::native as ffi_wgpu;

#[allow(
    dead_code,
    clippy::all,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod qjs {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

thread_local! {
    static DETACHING_ARRAYBUFFER: Cell<bool> = const { Cell::new(false) };
}

/// Adapter result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the QuickJS adapter.
#[derive(Debug)]
pub enum Error {
    /// A C API returned null where a live handle was required.
    Null(&'static str),
    /// QuickJS raised an exception.
    Exception(String),
    /// A Rust string could not be represented as a C string.
    Nul(std::ffi::NulError),
}

impl From<std::ffi::NulError> for Error {
    fn from(error: std::ffi::NulError) -> Self {
        Self::Nul(error)
    }
}

/// QuickJS runtime and context for the WebGPU binding slice.
pub struct Runtime {
    rt: NonNull<qjs::JSRuntime>,
    ctx: NonNull<qjs::JSContext>,
    state: Rc<State>,
}

/// Send + Sync producer handle for events on adopted devices.
#[derive(Clone)]
pub struct DeviceEventForwarder {
    inner: core::DeviceEventForwarder,
}

impl DeviceEventForwarder {
    /// Enqueues an adopted device's uncaptured error without touching QuickJS.
    /// Blocks during concurrent registry mutation so the event is not dropped.
    pub fn forward_uncaptured_error(
        &self,
        device: ffi_wgpu::WGPUDevice,
        type_: ffi_wgpu::WGPUErrorType,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.inner
            .forward_uncaptured_error::<Engine>(device, type_, message)
    }

    /// Enqueues adopted-device loss without touching QuickJS.
    /// Blocks during concurrent registry mutation so the event is not dropped.
    pub fn forward_device_lost(
        &self,
        device: ffi_wgpu::WGPUDevice,
        reason: ffi_wgpu::WGPUDeviceLostReason,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.inner
            .forward_device_lost::<Engine>(device, reason, message)
    }
}

impl Runtime {
    /// Creates a QuickJS runtime configured with the WebGPU binding environment.
    pub fn new() -> Result<Self> {
        Self::new_with_state(State::new())
    }

    #[cfg(test)]
    fn new_with_dispatch(gpu: core::GpuDispatch) -> Result<Self> {
        Self::new_with_state(State::new_with_dispatch(gpu))
    }

    fn new_with_state(state: State) -> Result<Self> {
        let rt = unsafe { qjs::JS_NewRuntime() };
        let rt = NonNull::new(rt).ok_or(Error::Null("JS_NewRuntime"))?;
        let ctx = unsafe { qjs::JS_NewContext(rt.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or(Error::Null("JS_NewContext"))?;
        let state = Rc::new(state);
        let raw_state = Rc::into_raw(Rc::clone(&state)).cast::<c_void>().cast_mut();
        unsafe {
            qjs::JS_SetRuntimeOpaque(rt.as_ptr(), raw_state);
            qjs::JS_SetHostPromiseRejectionTracker(
                rt.as_ptr(),
                Some(promise_rejection_tracker),
                raw_state,
            );
        }
        let source =
            c"(function(fns, values) { for (let i = 0; i < fns.length; i++) fns[i](values[i]); })";
        let name = c"webgpu-native-js-settle-trampoline.js";
        let trampoline = unsafe {
            qjs::JS_Eval(
                ctx.as_ptr(),
                source.as_ptr(),
                source.to_bytes().len(),
                name.as_ptr(),
                qjs::JS_EVAL_TYPE_GLOBAL as c_int,
            )
        };
        if unsafe { qjs::JS_IsException(trampoline) } {
            let message = take_exception(ctx.as_ptr(), "settle trampoline install");
            unsafe {
                qjs::JS_SetHostPromiseRejectionTracker(rt.as_ptr(), None, ptr::null_mut());
                qjs::JS_FreeContext(ctx.as_ptr());
                qjs::JS_FreeRuntime(rt.as_ptr());
                drop(Rc::from_raw(raw_state.cast::<State>()));
            }
            return Err(Error::Exception(message));
        }
        state.set_settle_trampoline(trampoline);
        Ok(Self { rt, ctx, state })
    }

    /// Returns the raw QuickJS context.
    #[must_use]
    pub fn raw_context(&self) -> *mut qjs::JSContext {
        self.ctx.as_ptr()
    }

    /// Returns a thread-safe adopted-device event producer.
    #[must_use]
    pub fn device_event_forwarder(&self) -> DeviceEventForwarder {
        DeviceEventForwarder {
            inner: self.state.env.device_event_forwarder(),
        }
    }

    /// Enqueues an uncaptured error for an adopted device, blocking during a
    /// concurrent registry mutation so the event is not dropped.
    pub fn forward_uncaptured_error(
        &self,
        device: ffi_wgpu::WGPUDevice,
        type_: ffi_wgpu::WGPUErrorType,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.device_event_forwarder()
            .forward_uncaptured_error(device, type_, message)
    }

    /// Enqueues loss for an adopted device, blocking during a concurrent
    /// registry mutation so the event is not dropped.
    pub fn forward_device_lost(
        &self,
        device: ffi_wgpu::WGPUDevice,
        reason: ffi_wgpu::WGPUDeviceLostReason,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.device_event_forwarder()
            .forward_device_lost(device, reason, message)
    }

    /// Wraps an adopted WebGPU device.
    ///
    /// # Safety
    ///
    /// `device` must be non-null, must come from this adapter's backend, and
    /// the caller must own or have borrowed a live native reference for the
    /// duration of this call. The core wrapper takes its own native reference.
    pub unsafe fn wrap_device(&self, device: ffi_wgpu::WGPUDevice) -> Result<qjs::JSValue> {
        let scope = Scope::new(self.raw_context());
        let value = unsafe {
            core::wrap_device::<Engine>(
                Context {
                    ctx: self.raw_context(),
                    scope: &scope,
                },
                device,
            )
        }
        .map_err(|value| Error::Exception(exception_or_value(self.raw_context(), value)))?;
        scope.escape(value);
        Ok(value)
    }

    /// Wraps a WebGPU instance as a JavaScript `GPU`.
    ///
    /// # Safety
    ///
    /// `instance` must be a live non-null handle from this runtime's backend
    /// and must remain live while the returned wrapper can be used.
    pub unsafe fn wrap_gpu(&self, instance: ffi_wgpu::WGPUInstance) -> Result<qjs::JSValue> {
        let scope = Scope::new(self.raw_context());
        let value = core::wrap_gpu::<Engine>(
            Context {
                ctx: self.raw_context(),
                scope: &scope,
            },
            instance,
        )
        .map_err(|value| Error::Exception(exception_or_value(self.raw_context(), value)))?;
        scope.escape(value);
        Ok(value)
    }

    /// Sets a global property to a JavaScript value. The runtime adopts the value.
    pub fn set_global_value(&self, name: &str, value: qjs::JSValue) -> Result<()> {
        let name = CString::new(name)?;
        let global = unsafe { qjs::JS_GetGlobalObject(self.raw_context()) };
        let rc =
            unsafe { qjs::JS_SetPropertyStr(self.raw_context(), global, name.as_ptr(), value) };
        unsafe { qjs::JS_FreeValue(self.raw_context(), global) };
        if rc < 0 {
            Err(Error::Exception(take_exception(
                self.raw_context(),
                "JS_SetPropertyStr",
            )))
        } else {
            Ok(())
        }
    }

    /// Clears a global property by assigning `undefined`.
    pub fn clear_global(&self, name: &str) -> Result<()> {
        with_scope(self.raw_context(), |cx| {
            self.set_global_value(name, Engine::undefined(cx))
        })
    }

    /// Evaluates a script and returns its completion value.
    pub fn eval(&self, source: &str, name: &str) -> Result<qjs::JSValue> {
        let input = CString::new(source)?;
        let name = CString::new(name)?;
        let value = unsafe {
            qjs::JS_Eval(
                self.raw_context(),
                input.as_ptr(),
                source.len(),
                name.as_ptr(),
                qjs::JS_EVAL_TYPE_GLOBAL as c_int,
            )
        };
        if unsafe { qjs::JS_IsException(value) } {
            Err(Error::Exception(take_exception(
                self.raw_context(),
                "JS_Eval",
            )))
        } else {
            Ok(value)
        }
    }

    /// Drains the core release queue.
    pub fn drain_releases(&self) -> std::result::Result<usize, core::QueueError> {
        self.state.env.queue().drain()
    }

    /// Pumps WebGPU callbacks, QuickJS microtasks, then queued releases.
    ///
    /// Returns the number of release requests drained.
    ///
    /// # Safety
    ///
    /// `instance` must be non-null, must come from this adapter's backend, and
    /// must remain live for the whole pump. Callers must not pass an instance
    /// that is concurrently being released.
    pub unsafe fn tick(&self, instance: ffi_wgpu::WGPUInstance) -> Result<usize> {
        let drained = with_scope(self.raw_context(), |cx| {
            match unsafe { core::tick::<Engine>(cx, instance) } {
                Ok(drained) => Ok(drained),
                Err(core::TickError::Queue(error)) => {
                    Err(Error::Exception(format!("tick queue error: {error:?}")))
                }
                Err(core::TickError::Engine(error)) => {
                    cx.scope.escape(error);
                    Err(Error::Exception(exception_or_value(cx.ctx, error)))
                }
                Err(_) => Err(Error::Exception("unknown tick failure".to_owned())),
            }
        })?;
        if let Some(message) = self.state.take_unhandled_rejection(self.raw_context()) {
            return Err(Error::Exception(message));
        }
        Ok(drained)
    }

    /// Pumps only WebGPU callbacks, for event-loop regression tests.
    ///
    /// # Safety
    ///
    /// `instance` must be non-null, must come from this adapter's backend, and
    /// must remain live for this event-processing call.
    pub unsafe fn process_events_only(&self, instance: ffi_wgpu::WGPUInstance) {
        unsafe { ffi_wgpu::wgpuInstanceProcessEvents(instance) };
    }

    /// Runs the engine garbage collector.
    pub fn run_gc(&self) {
        unsafe { qjs::JS_RunGC(self.rt.as_ptr()) };
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_SetHostPromiseRejectionTracker(self.rt.as_ptr(), None, ptr::null_mut());
            let raw = qjs::JS_GetRuntimeOpaque(self.rt.as_ptr()).cast::<State>();
            if let Some(state) = raw.as_ref() {
                state.release_unhandled_rejections(self.ctx.as_ptr());
                state.release_outstanding_deferreds(self.ctx.as_ptr());
                with_scope(self.ctx.as_ptr(), |cx| {
                    state.env.settlements().release_pending::<Engine>(cx);
                    state.env.release_device_event_values::<Engine>(cx);
                });
                if let Some(trampoline) = state.take_settle_trampoline() {
                    qjs::JS_FreeValue(self.ctx.as_ptr(), trampoline);
                }
            }
            qjs::JS_FreeContext(self.ctx.as_ptr());
            qjs::JS_FreeRuntime(self.rt.as_ptr());
            if let Some(state) = raw.as_ref() {
                let _ = state.env.queue().drain();
            }
            if !raw.is_null() {
                drop(Rc::from_raw(raw));
            }
        }
    }
}

struct State {
    env: core::Environment,
    classes: Mutex<BTreeMap<core::ClassId, ClassEntry>>,
    quickjs_to_core: Mutex<BTreeMap<qjs::JSClassID, core::ClassId>>,
    callbacks: Mutex<Vec<CallbackTarget>>,
    outstanding_deferreds: Arc<Mutex<Vec<DeferredSlot>>>,
    unhandled_rejections: Mutex<Vec<UnhandledRejection>>,
    settle_trampoline: Mutex<Option<qjs::JSValue>>,
}

/// Registration guard for a deferred slot owned by a pending WebGPU callback.
pub struct DeferredRegistration {
    slots: Arc<Mutex<Vec<DeferredSlot>>>,
    slot: DeferredSlot,
}

#[derive(Clone, Copy)]
struct DeferredSlot(NonNull<Option<core::Deferred<Engine>>>);

// SAFETY: `DeferredSlot` points into a callback-request `Box` that remains
// allocated until its `AllowProcessEvents` callback takes ownership. Runtime
// teardown and those callbacks run on the same engine thread, so the pointer is
// never dereferenced concurrently; registration drop removes it before the
// `Box` dies.
unsafe impl Send for DeferredSlot {}

impl Drop for DeferredRegistration {
    fn drop(&mut self) {
        if let Ok(mut slots) = self.slots.lock() {
            if let Some(index) = slots
                .iter()
                .position(|candidate| candidate.0 == self.slot.0)
            {
                slots.swap_remove(index);
            }
        }
    }
}

impl State {
    fn new() -> Self {
        Self::new_with_dispatch(gpu_dispatch())
    }

    fn new_with_dispatch(gpu: core::GpuDispatch) -> Self {
        Self {
            env: core::Environment::new(gpu, Arc::new(core::ReleaseQueue::new())),
            classes: Mutex::new(BTreeMap::new()),
            quickjs_to_core: Mutex::new(BTreeMap::new()),
            callbacks: Mutex::new(Vec::new()),
            outstanding_deferreds: Arc::new(Mutex::new(Vec::new())),
            unhandled_rejections: Mutex::new(Vec::new()),
            settle_trampoline: Mutex::new(None),
        }
    }

    fn track_unhandled(
        &self,
        ctx: *mut qjs::JSContext,
        promise: qjs::JSValue,
        reason: qjs::JSValue,
    ) {
        let Ok(mut rejections) = self.unhandled_rejections.lock() else {
            return;
        };
        if rejections
            .iter()
            .any(|entry| same_js_value(entry.promise, promise))
        {
            return;
        }
        rejections.push(UnhandledRejection {
            promise: unsafe { qjs::JS_DupValue(ctx, promise) },
            reason: unsafe { qjs::JS_DupValue(ctx, reason) },
        });
    }

    fn mark_handled(&self, ctx: *mut qjs::JSContext, promise: qjs::JSValue) {
        let Ok(mut rejections) = self.unhandled_rejections.lock() else {
            return;
        };
        if let Some(index) = rejections
            .iter()
            .position(|entry| same_js_value(entry.promise, promise))
        {
            let entry = rejections.swap_remove(index);
            unsafe {
                qjs::JS_FreeValue(ctx, entry.promise);
                qjs::JS_FreeValue(ctx, entry.reason);
            }
        }
    }

    fn take_unhandled_rejection(&self, ctx: *mut qjs::JSContext) -> Option<String> {
        let Ok(mut rejections) = self.unhandled_rejections.lock() else {
            return Some("promise rejection tracker is poisoned".to_owned());
        };
        let entry = rejections.pop()?;
        let message = exception_or_value(ctx, entry.reason);
        unsafe { qjs::JS_FreeValue(ctx, entry.promise) };
        for entry in rejections.drain(..) {
            unsafe {
                qjs::JS_FreeValue(ctx, entry.promise);
                qjs::JS_FreeValue(ctx, entry.reason);
            }
        }
        Some(format!("Unhandled promise rejection: {message}"))
    }

    fn release_unhandled_rejections(&self, ctx: *mut qjs::JSContext) {
        let Ok(mut rejections) = self.unhandled_rejections.lock() else {
            return;
        };
        for entry in rejections.drain(..) {
            unsafe {
                qjs::JS_FreeValue(ctx, entry.promise);
                qjs::JS_FreeValue(ctx, entry.reason);
            }
        }
    }

    fn set_settle_trampoline(&self, value: qjs::JSValue) {
        if let Ok(mut trampoline) = self.settle_trampoline.lock() {
            *trampoline = Some(value);
        }
    }

    fn take_settle_trampoline(&self) -> Option<qjs::JSValue> {
        self.settle_trampoline
            .lock()
            .ok()
            .and_then(|mut trampoline| trampoline.take())
    }

    fn settle_trampoline(&self) -> Option<qjs::JSValue> {
        self.settle_trampoline
            .lock()
            .ok()
            .and_then(|trampoline| *trampoline)
    }

    fn register_deferred(
        &self,
        slot: NonNull<Option<core::Deferred<Engine>>>,
    ) -> DeferredRegistration {
        let slot = DeferredSlot(slot);
        if let Ok(mut slots) = self.outstanding_deferreds.lock() {
            slots.push(slot);
        }
        DeferredRegistration {
            slots: Arc::clone(&self.outstanding_deferreds),
            slot,
        }
    }

    fn release_outstanding_deferreds(&self, ctx: *mut qjs::JSContext) {
        let slots = self
            .outstanding_deferreds
            .lock()
            .map(|mut slots| std::mem::take(&mut *slots))
            .unwrap_or_default();
        for slot in slots {
            let Some(deferred) = (unsafe { slot.0.as_ptr().as_mut() }).and_then(Option::take)
            else {
                continue;
            };
            unsafe {
                qjs::JS_FreeValue(ctx, deferred.resolve());
                qjs::JS_FreeValue(ctx, deferred.reject());
            }
        }
    }
}

struct ClassEntry {
    quickjs_id: qjs::JSClassID,
    spec: &'static core::ClassSpec<Engine>,
}

struct UnhandledRejection {
    promise: qjs::JSValue,
    reason: qjs::JSValue,
}

struct ArrayBufferOwner {
    buffer: core::WGPUBuffer,
    gpu: core::GpuDispatch,
    queue: Arc<core::ReleaseQueue>,
    released: bool,
}

struct ObjectPayload {
    spec: &'static core::ClassSpec<Engine>,
    payload: Box<dyn Any + Send>,
}

#[derive(Clone, Copy)]
struct CallbackTarget {
    class: core::ClassId,
    kind: CallbackKind,
    index: usize,
}

/// QuickJS engine marker type.
pub struct Engine;

/// QuickJS context handle.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    ctx: *mut qjs::JSContext,
    scope: &'a Scope,
}

struct Scope {
    ctx: *mut qjs::JSContext,
    values: RefCell<Vec<qjs::JSValue>>,
}

impl Scope {
    fn new(ctx: *mut qjs::JSContext) -> Self {
        Self {
            ctx,
            values: RefCell::new(Vec::new()),
        }
    }

    fn track(&self, value: qjs::JSValue) {
        self.values.borrow_mut().push(value);
    }

    fn escape(&self, value: qjs::JSValue) {
        let mut values = self.values.borrow_mut();
        if let Some(index) = values
            .iter()
            .position(|candidate| same_js_value(*candidate, value))
        {
            values.swap_remove(index);
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        for value in self.values.borrow_mut().drain(..) {
            unsafe { qjs::JS_FreeValue(self.ctx, value) };
        }
    }
}

fn with_scope<R>(ctx: *mut qjs::JSContext, f: impl FnOnce(Context<'_>) -> R) -> R {
    let scope = Scope::new(ctx);
    f(Context { ctx, scope: &scope })
}

impl core::JsEngine for Engine {
    type Value = qjs::JSValue;
    type Context<'a> = Context<'a>;
    type Error = qjs::JSValue;
    type DeferredRegistration = DeferredRegistration;

    const MAPPED_RANGE_STRATEGY: core::MappedRangeStrategy =
        core::MappedRangeStrategy::ZeroCopyDetach;

    fn environment<'a>(cx: Self::Context<'a>) -> &'a core::Environment {
        let state = state_from_context(cx.ctx);
        &state.env
    }

    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> core::Result<Self::Value, Self::Error> {
        let key = CString::new(key).map_err(|_| Self::type_error(cx, "invalid property name"))?;
        let value = unsafe { qjs::JS_GetPropertyStr(cx.ctx, obj, key.as_ptr()) };
        if unsafe { qjs::JS_IsException(value) } {
            Err(take_exception_value(cx))
        } else {
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn global(cx: Self::Context<'_>) -> Self::Value {
        let value = unsafe { qjs::JS_GetGlobalObject(cx.ctx) };
        cx.scope.track(value);
        value
    }

    fn get_property_value(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: Self::Value,
    ) -> core::Result<Self::Value, Self::Error> {
        let atom = unsafe { qjs::JS_ValueToAtom(cx.ctx, key) };
        if atom == qjs::JS_ATOM_NULL {
            return Err(take_exception_value(cx));
        }
        let value = unsafe { qjs::JS_GetProperty(cx.ctx, obj, atom) };
        unsafe { qjs::JS_FreeAtom(cx.ctx, atom) };
        if unsafe { qjs::JS_IsException(value) } {
            Err(take_exception_value(cx))
        } else {
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn call(
        cx: Self::Context<'_>,
        f: Self::Value,
        this: Self::Value,
        args: &[Self::Value],
    ) -> core::Result<Self::Value, Self::Error> {
        let argc = c_int::try_from(args.len())
            .map_err(|_| Self::type_error(cx, "too many call arguments"))?;
        let value = unsafe { qjs::JS_Call(cx.ctx, f, this, argc, args.as_ptr().cast_mut()) };
        if unsafe { qjs::JS_IsException(value) } {
            Err(take_exception_value(cx))
        } else {
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn is_undefined(_cx: Self::Context<'_>, value: Self::Value) -> bool {
        unsafe { qjs::JS_IsUndefined(value) }
    }

    fn is_null(_cx: Self::Context<'_>, value: Self::Value) -> bool {
        unsafe { qjs::JS_IsNull(value) }
    }

    fn is_callable(cx: Self::Context<'_>, value: Self::Value) -> bool {
        unsafe { qjs::JS_IsFunction(cx.ctx, value) }
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> core::Result<f64, Self::Error> {
        let mut out = 0.0;
        if unsafe { qjs::JS_ToFloat64(cx.ctx, &mut out, value) } < 0 {
            Err(take_exception_value(cx))
        } else {
            Ok(out)
        }
    }

    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool {
        unsafe { qjs::JS_ToBool(cx.ctx, value) != 0 }
    }

    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a core::Arena,
    ) -> core::Result<&'a str, Self::Error> {
        let mut len = 0usize;
        let raw = unsafe { qjs::JS_ToCStringLen(cx.ctx, &mut len, value) };
        if raw.is_null() {
            return Err(take_exception_value(cx));
        }
        let bytes = unsafe { std::slice::from_raw_parts(raw.cast::<u8>(), len) };
        let owned = String::from_utf8_lossy(bytes).into_owned();
        unsafe { qjs::JS_FreeCString(cx.ctx, raw) };
        Ok(arena.alloc_str(&owned))
    }

    fn register_class(
        cx: Self::Context<'_>,
        spec: &'static core::ClassSpec<Self>,
    ) -> core::Result<core::ClassId, Self::Error> {
        let state = state_from_context(cx.ctx);
        if state
            .classes
            .lock()
            .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
            .contains_key(&spec.id)
        {
            return Ok(spec.id);
        }

        let mut quickjs_id = qjs::JS_INVALID_CLASS_ID;
        unsafe {
            qjs::JS_NewClassID(qjs::JS_GetRuntime(cx.ctx), &mut quickjs_id);
        }
        let class_name =
            CString::new(spec.name).map_err(|_| Self::type_error(cx, "invalid class name"))?;
        let class_def = qjs::JSClassDef {
            class_name: class_name.as_ptr(),
            finalizer: Some(qjs_finalizer),
            gc_mark: Some(qjs_gc_mark),
            call: None,
            exotic: ptr::null_mut(),
        };
        if unsafe { qjs::JS_NewClass(qjs::JS_GetRuntime(cx.ctx), quickjs_id, &class_def) } != 0 {
            return Err(Self::operation_error(cx, "JS_NewClass failed"));
        }
        let proto = unsafe { qjs::JS_NewObject(cx.ctx) };
        if unsafe { qjs::JS_IsException(proto) } {
            return Err(take_exception_value(cx));
        }
        let setup = (|| {
            if let Some(parent) = spec
                .constructor
                .as_ref()
                .and_then(|constructor| constructor.parent)
            {
                let parent_id = state
                    .classes
                    .lock()
                    .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
                    .get(&parent)
                    .map(|entry| entry.quickjs_id)
                    .ok_or_else(|| Self::operation_error(cx, "parent class is not registered"))?;
                let parent_proto = unsafe { qjs::JS_GetClassProto(cx.ctx, parent_id) };
                if unsafe { qjs::JS_IsException(parent_proto) } {
                    return Err(take_exception_value(cx));
                }
                let rc = unsafe { qjs::JS_SetPrototype(cx.ctx, proto, parent_proto) };
                unsafe { qjs::JS_FreeValue(cx.ctx, parent_proto) };
                if rc < 0 {
                    return Err(take_exception_value(cx));
                }
            }
            install_methods(cx, state, proto, spec)?;
            install_properties(cx, state, proto, spec)?;
            if let Some(constructor) = &spec.constructor {
                let magic = allocate_magic(cx, state, spec.id, CallbackKind::Constructor, 0)?;
                let function = qjs::JSCFunctionType {
                    constructor_magic: Some(qjs_constructor),
                };
                let constructor_value = unsafe {
                    qjs::JS_NewCFunction2(
                        cx.ctx,
                        function.generic,
                        class_name.as_ptr(),
                        i32::from(constructor.length),
                        qjs::JSCFunctionEnum_JS_CFUNC_constructor_magic,
                        magic,
                    )
                };
                if unsafe { qjs::JS_IsException(constructor_value) } {
                    return Err(take_exception_value(cx));
                }
                if unsafe { qjs::JS_SetConstructor(cx.ctx, constructor_value, proto) } < 0 {
                    unsafe { qjs::JS_FreeValue(cx.ctx, constructor_value) };
                    return Err(take_exception_value(cx));
                }
                Ok(Some(constructor_value))
            } else {
                Ok(None)
            }
        })();
        let constructor = match setup {
            Ok(constructor) => constructor,
            Err(error) => {
                // proto has not yet been transferred to the class and owns all
                // successfully installed method/property values on every arm.
                unsafe { qjs::JS_FreeValue(cx.ctx, proto) };
                return Err(error);
            }
        };
        unsafe {
            qjs::JS_SetClassProto(cx.ctx, quickjs_id, proto);
        }
        state
            .classes
            .lock()
            .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
            .insert(spec.id, ClassEntry { quickjs_id, spec });
        state
            .quickjs_to_core
            .lock()
            .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
            .insert(quickjs_id, spec.id);
        if let Some(constructor) = constructor {
            let global = unsafe { qjs::JS_GetGlobalObject(cx.ctx) };
            let rc =
                unsafe { qjs::JS_SetPropertyStr(cx.ctx, global, class_name.as_ptr(), constructor) };
            unsafe { qjs::JS_FreeValue(cx.ctx, global) };
            if rc < 0 {
                return Err(take_exception_value(cx));
            }
        }
        Ok(spec.id)
    }

    fn new_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
    ) -> core::Result<Self::Value, Self::Error> {
        let state = state_from_context(cx.ctx);
        let classes = state
            .classes
            .lock()
            .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?;
        let Some(entry) = classes.get(&class) else {
            return Err(Self::operation_error(cx, "class is not registered"));
        };
        let object = unsafe { qjs::JS_NewObjectClass(cx.ctx, entry.quickjs_id) };
        if unsafe { qjs::JS_IsException(object) } {
            return Err(take_exception_value(cx));
        }
        let holder = Box::new(ObjectPayload {
            spec: entry.spec,
            payload,
        });
        unsafe {
            qjs::JS_SetOpaque(object, Box::into_raw(holder).cast());
        }
        cx.scope.track(object);
        Ok(object)
    }

    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: core::ClassId,
    ) -> Option<&'a (dyn Any + Send)> {
        let state = state_from_context(cx.ctx);
        let classes = state.classes.lock().ok()?;
        let entry = classes.get(&class)?;
        let raw = unsafe { qjs::JS_GetOpaque(obj, entry.quickjs_id) };
        NonNull::new(raw.cast::<ObjectPayload>())
            .map(|ptr| unsafe { ptr.as_ref().payload.as_ref() })
    }

    fn undefined(_cx: Self::Context<'_>) -> Self::Value {
        qjs_value_with_tag(qjs::JS_TAG_UNDEFINED as i64)
    }

    fn null(_cx: Self::Context<'_>) -> Self::Value {
        qjs_value_with_tag(qjs::JS_TAG_NULL as i64)
    }

    fn number(cx: Self::Context<'_>, value: f64) -> core::Result<Self::Value, Self::Error> {
        let value = unsafe { qjs::JS_NewFloat64(cx.ctx, value) };
        cx.scope.track(value);
        Ok(value)
    }

    fn string(cx: Self::Context<'_>, value: &str) -> core::Result<Self::Value, Self::Error> {
        let value = unsafe { qjs::JS_NewStringLen(cx.ctx, value.as_ptr().cast(), value.len()) };
        cx.scope.track(value);
        Ok(value)
    }

    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        let _ = throw_message(cx.ctx, message, true);
        take_exception_value(cx)
    }

    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        let _ = throw_message(cx.ctx, message, false);
        let error = take_exception_value(cx);
        set_error_name(cx, error, "OperationError")
    }

    fn async_error_value(cx: Self::Context<'_>, name: &str, message: &str) -> Self::Value {
        let error = unsafe { qjs::JS_NewError(cx.ctx) };
        if unsafe { qjs::JS_IsException(error) } {
            return take_exception_value(cx);
        }
        cx.scope.track(error);
        for (key, text) in [(c"name", name), (c"message", message)] {
            let value = unsafe { qjs::JS_NewStringLen(cx.ctx, text.as_ptr().cast(), text.len()) };
            if unsafe { qjs::JS_IsException(value) } {
                return take_exception_value(cx);
            }
            if unsafe { qjs::JS_SetPropertyStr(cx.ctx, error, key.as_ptr(), value) } < 0 {
                return take_exception_value(cx);
            }
        }
        error
    }

    fn error_value_from_error(_cx: Self::Context<'_>, error: Self::Error) -> Self::Value {
        error
    }

    fn new_promise(
        cx: Self::Context<'_>,
    ) -> core::Result<(Self::Value, core::Deferred<Self>), Self::Error> {
        let mut resolving = [Self::undefined(cx), Self::undefined(cx)];
        let promise = unsafe { qjs::JS_NewPromiseCapability(cx.ctx, resolving.as_mut_ptr()) };
        if unsafe { qjs::JS_IsException(promise) } {
            return Err(take_exception_value(cx));
        }
        cx.scope.track(promise);
        Ok((promise, core::Deferred::new(resolving[0], resolving[1])))
    }

    fn settle_deferreds(
        cx: Self::Context<'_>,
        settlements: Vec<core::DeferredSettlement<Self>>,
    ) -> core::Result<(), Self::Error> {
        if settlements.is_empty() {
            return Ok(());
        }
        let state = state_from_context(cx.ctx);
        let Some(trampoline) = state.settle_trampoline() else {
            for (deferred, result) in settlements {
                let value = result.unwrap_or_else(|value| value);
                cx.scope.escape(value);
                unsafe {
                    qjs::JS_FreeValue(cx.ctx, value);
                    qjs::JS_FreeValue(cx.ctx, deferred.resolve());
                    qjs::JS_FreeValue(cx.ctx, deferred.reject());
                }
            }
            return Err(Self::operation_error(
                cx,
                "settlement trampoline is unavailable",
            ));
        };
        let fns = unsafe { qjs::JS_NewArray(cx.ctx) };
        let values = unsafe { qjs::JS_NewArray(cx.ctx) };
        if unsafe { qjs::JS_IsException(fns) || qjs::JS_IsException(values) } {
            unsafe {
                qjs::JS_FreeValue(cx.ctx, fns);
                qjs::JS_FreeValue(cx.ctx, values);
            }
            for (deferred, result) in settlements {
                let value = result.unwrap_or_else(|value| value);
                cx.scope.escape(value);
                unsafe {
                    qjs::JS_FreeValue(cx.ctx, value);
                    qjs::JS_FreeValue(cx.ctx, deferred.resolve());
                    qjs::JS_FreeValue(cx.ctx, deferred.reject());
                }
            }
            return Err(take_exception_value(cx));
        }
        cx.scope.track(fns);
        cx.scope.track(values);
        let mut property_failed = false;
        for (index, (deferred, result)) in settlements.into_iter().enumerate() {
            let (func, value) = match result {
                Ok(value) => {
                    unsafe { qjs::JS_FreeValue(cx.ctx, deferred.reject()) };
                    (deferred.resolve(), value)
                }
                Err(value) => {
                    unsafe { qjs::JS_FreeValue(cx.ctx, deferred.resolve()) };
                    (deferred.reject(), value)
                }
            };
            cx.scope.escape(value);
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            let function_rc = unsafe { qjs::JS_SetPropertyUint32(cx.ctx, fns, index, func) };
            let value_rc = unsafe { qjs::JS_SetPropertyUint32(cx.ctx, values, index, value) };
            property_failed |= function_rc < 0 || value_rc < 0;
        }
        if property_failed {
            return Err(take_exception_value(cx));
        }
        let this = qjs_value_with_tag(qjs::JS_TAG_UNDEFINED as i64);
        let mut argv = [fns, values];
        let call = unsafe { qjs::JS_Call(cx.ctx, trampoline, this, 2, argv.as_mut_ptr()) };
        if unsafe { qjs::JS_IsException(call) } {
            unsafe { qjs::JS_FreeValue(cx.ctx, call) };
            return Err(take_exception_value(cx));
        }
        unsafe { qjs::JS_FreeValue(cx.ctx, call) };
        Ok(())
    }

    fn drain_microtasks(cx: Self::Context<'_>) -> core::Result<(), Self::Error> {
        loop {
            let mut job_ctx = ptr::null_mut();
            let rc = unsafe { qjs::JS_ExecutePendingJob(qjs::JS_GetRuntime(cx.ctx), &mut job_ctx) };
            if rc > 0 {
                continue;
            }
            if rc == 0 {
                return Ok(());
            }
            let error_cx = Context {
                ctx: if job_ctx.is_null() { cx.ctx } else { job_ctx },
                scope: cx.scope,
            };
            return Err(take_exception_value(error_cx));
        }
    }

    unsafe fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        ptr: *mut u8,
        len: usize,
        owner: core::WGPUBuffer,
    ) -> core::Result<Self::Value, Self::Error> {
        let env = Self::environment(cx);
        let owner_buffer = owner;
        let owner = Box::new(ArrayBufferOwner {
            buffer: owner_buffer,
            gpu: env.gpu(),
            queue: Arc::clone(env.queue()),
            released: false,
        });
        let opaque = Box::into_raw(owner).cast();
        let value = unsafe {
            qjs::JS_NewArrayBuffer(cx.ctx, ptr, len, Some(arraybuffer_free), opaque, false)
        };
        if unsafe { qjs::JS_IsException(value) } {
            unsafe {
                drop(Box::from_raw(opaque.cast::<ArrayBufferOwner>()));
                (env.gpu().buffer_release)(owner_buffer);
            }
            Err(take_exception_value(cx))
        } else {
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> core::Result<Self::Value, Self::Error> {
        let value = unsafe { qjs::JS_NewArrayBufferCopy(cx.ctx, bytes.as_ptr(), bytes.len()) };
        if unsafe { qjs::JS_IsException(value) } {
            Err(take_exception_value(cx))
        } else {
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        value: Self::Value,
        out: Option<&mut [u8]>,
    ) -> core::Result<(), Self::Error> {
        if let Some(out) = out {
            let mut len = 0usize;
            let ptr = unsafe { qjs::JS_GetArrayBuffer(cx.ctx, &mut len, value) };
            if ptr.is_null() || len != out.len() || unsafe { qjs::JS_HasException(cx.ctx) } {
                clear_pending_exception(cx.ctx);
                return Err(Self::type_error(cx, "ArrayBuffer"));
            }
            let src = unsafe { std::slice::from_raw_parts(ptr, len) };
            out.copy_from_slice(src);
        }
        crate::DETACHING_ARRAYBUFFER.with(|detaching| {
            let previous = detaching.replace(true);
            unsafe { qjs::JS_DetachArrayBuffer(cx.ctx, value) };
            detaching.set(previous);
        });
        Ok(())
    }

    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize> {
        if !unsafe { qjs::JS_IsArrayBuffer(value) } {
            return None;
        }
        let length = unsafe { qjs::JS_GetPropertyStr(cx.ctx, value, c"byteLength".as_ptr()) };
        if unsafe { qjs::JS_IsException(length) } {
            clear_pending_exception(cx.ctx);
            return None;
        }
        let mut len = 0u64;
        let rc = unsafe { qjs::JS_ToIndex(cx.ctx, &mut len, length) };
        unsafe { qjs::JS_FreeValue(cx.ctx, length) };
        if rc < 0 {
            clear_pending_exception(cx.ctx);
            return None;
        }
        usize::try_from(len).ok()
    }

    fn arraybuffer_copy(cx: Self::Context<'_>, value: Self::Value) -> Option<Vec<u8>> {
        let mut len = 0usize;
        let ptr = unsafe { qjs::JS_GetArrayBuffer(cx.ctx, &mut len, value) };
        if ptr.is_null() || unsafe { qjs::JS_HasException(cx.ctx) } {
            clear_pending_exception(cx.ctx);
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(ptr, len).to_vec() })
    }

    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value {
        unsafe { qjs::JS_DupValue(cx.ctx, value) }
    }

    fn return_held_value(cx: Self::Context<'_>, held: Self::Value) -> Self::Value {
        unsafe { qjs::JS_DupValue(cx.ctx, held) }
    }

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        unsafe { qjs::JS_FreeValue(cx.ctx, value) };
    }

    fn register_deferred(
        cx: Self::Context<'_>,
        slot: NonNull<Option<core::Deferred<Self>>>,
    ) -> Self::DeferredRegistration {
        state_from_context(cx.ctx).register_deferred(slot)
    }

    fn release_deferred(cx: Self::Context<'_>, deferred: core::Deferred<Self>) {
        unsafe {
            qjs::JS_FreeValue(cx.ctx, deferred.resolve());
            qjs::JS_FreeValue(cx.ctx, deferred.reject());
        }
    }
}

fn install_methods(
    cx: Context<'_>,
    state: &State,
    proto: qjs::JSValue,
    spec: &'static core::ClassSpec<Engine>,
) -> core::Result<(), qjs::JSValue> {
    for (index, method) in spec.methods.iter().enumerate() {
        let Ok(name) = CString::new(method.name) else {
            return Err(Engine::type_error(cx, "invalid method name"));
        };
        let func = unsafe {
            qjs::JS_NewCFunctionMagic(
                cx.ctx,
                Some(qjs_method),
                name.as_ptr(),
                i32::from(method.length),
                qjs::JSCFunctionEnum_JS_CFUNC_generic_magic,
                allocate_magic(cx, state, spec.id, CallbackKind::Method, index)?,
            )
        };
        if unsafe { qjs::JS_IsException(func) } {
            return Err(take_exception_value(cx));
        }
        if unsafe {
            qjs::JS_DefinePropertyValueStr(
                cx.ctx,
                proto,
                name.as_ptr(),
                func,
                (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_WRITABLE) as c_int,
            )
        } < 0
        {
            return Err(take_exception_value(cx));
        }
    }
    Ok(())
}

fn install_properties(
    cx: Context<'_>,
    state: &State,
    proto: qjs::JSValue,
    spec: &'static core::ClassSpec<Engine>,
) -> core::Result<(), qjs::JSValue> {
    for (index, property) in spec.properties.iter().enumerate() {
        let Ok(name) = CString::new(property.name) else {
            return Err(Engine::type_error(cx, "invalid property name"));
        };
        let atom = unsafe { qjs::JS_NewAtom(cx.ctx, name.as_ptr()) };
        let getter = if property.get.is_some() {
            new_getter(
                cx.ctx,
                name.as_ptr(),
                qjs_getter,
                allocate_magic(cx, state, spec.id, CallbackKind::Getter, index)?,
            )
        } else {
            Engine::undefined(cx)
        };
        let setter = if property.set.is_some() {
            new_setter(
                cx.ctx,
                name.as_ptr(),
                qjs_setter,
                allocate_magic(cx, state, spec.id, CallbackKind::Setter, index)?,
            )
        } else {
            Engine::undefined(cx)
        };
        let rc = unsafe {
            qjs::JS_DefinePropertyGetSet(
                cx.ctx,
                proto,
                atom,
                getter,
                setter,
                qjs::JS_PROP_CONFIGURABLE as c_int,
            )
        };
        unsafe { qjs::JS_FreeAtom(cx.ctx, atom) };
        if rc < 0 {
            return Err(take_exception_value(cx));
        }
    }
    Ok(())
}

fn new_getter(
    ctx: *mut qjs::JSContext,
    name: *const c_char,
    callback: unsafe extern "C" fn(*mut qjs::JSContext, qjs::JSValue, c_int) -> qjs::JSValue,
    magic: c_int,
) -> qjs::JSValue {
    let function = qjs::JSCFunctionType {
        getter_magic: Some(callback),
    };
    unsafe {
        qjs::JS_NewCFunction2(
            ctx,
            function.generic,
            name,
            0,
            qjs::JSCFunctionEnum_JS_CFUNC_getter_magic,
            magic,
        )
    }
}

fn new_setter(
    ctx: *mut qjs::JSContext,
    name: *const c_char,
    callback: unsafe extern "C" fn(
        *mut qjs::JSContext,
        qjs::JSValue,
        qjs::JSValue,
        c_int,
    ) -> qjs::JSValue,
    magic: c_int,
) -> qjs::JSValue {
    let function = qjs::JSCFunctionType {
        setter_magic: Some(callback),
    };
    unsafe {
        qjs::JS_NewCFunction2(
            ctx,
            function.generic,
            name,
            1,
            qjs::JSCFunctionEnum_JS_CFUNC_setter_magic,
            magic,
        )
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CallbackKind {
    Method = 1,
    Getter = 2,
    Setter = 3,
    Constructor = 4,
}

fn allocate_magic(
    cx: Context<'_>,
    state: &State,
    class: core::ClassId,
    kind: CallbackKind,
    index: usize,
) -> core::Result<c_int, qjs::JSValue> {
    let mut callbacks = state
        .callbacks
        .lock()
        .map_err(|_| Engine::operation_error(cx, "callback registry is poisoned"))?;
    if callbacks.len() >= i16::MAX as usize {
        return Err(Engine::operation_error(cx, "too many registered callbacks"));
    }
    callbacks.push(CallbackTarget { class, kind, index });
    Ok(callbacks.len() as c_int)
}

fn callback_target(
    cx: Context<'_>,
    magic_value: c_int,
    expected: CallbackKind,
) -> core::Result<CallbackTarget, qjs::JSValue> {
    let state = state_from_context(cx.ctx);
    let callbacks = state
        .callbacks
        .lock()
        .map_err(|_| Engine::operation_error(cx, "callback registry is poisoned"))?;
    let Some(target) = magic_value
        .checked_sub(1)
        .and_then(|index| callbacks.get(index as usize))
        .copied()
    else {
        return Err(Engine::operation_error(cx, "callback is not registered"));
    };
    if target.kind as c_int != expected as c_int {
        return Err(Engine::operation_error(cx, "callback kind mismatch"));
    }
    Ok(target)
}

unsafe extern "C" fn qjs_method(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    argc: c_int,
    argv: *mut qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, |cx| {
        let target = callback_target(cx, magic_value, CallbackKind::Method)?;
        let state = state_from_context(ctx);
        let method = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?;
            let Some(method) = classes
                .get(&target.class)
                .and_then(|entry| entry.spec.methods.get(target.index))
            else {
                return Err(Engine::operation_error(
                    cx,
                    &format!(
                        "method is not registered: class={} index={}",
                        target.class.0, target.index
                    ),
                ));
            };
            method.call
        };
        let args = if argc <= 0 || argv.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        };
        method(cx, this_val, args)
    })
}

unsafe extern "C" fn qjs_getter(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, |cx| {
        let target = callback_target(cx, magic_value, CallbackKind::Getter)?;
        let state = state_from_context(ctx);
        let getter = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?;
            let Some(getter) = classes
                .get(&target.class)
                .and_then(|entry| entry.spec.properties.get(target.index))
                .and_then(|property| property.get)
            else {
                return Err(Engine::operation_error(cx, "getter is not registered"));
            };
            getter
        };
        getter(cx, this_val)
    })
}

unsafe extern "C" fn qjs_setter(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    value: qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, |cx| {
        let target = callback_target(cx, magic_value, CallbackKind::Setter)?;
        let state = state_from_context(ctx);
        let setter = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?;
            let Some(setter) = classes
                .get(&target.class)
                .and_then(|entry| entry.spec.properties.get(target.index))
                .and_then(|property| property.set)
            else {
                return Err(Engine::operation_error(cx, "setter is not registered"));
            };
            setter
        };
        setter(cx, this_val, value)?;
        Ok(Engine::undefined(cx))
    })
}

unsafe extern "C" fn qjs_constructor(
    ctx: *mut qjs::JSContext,
    new_target: qjs::JSValue,
    argc: c_int,
    argv: *mut qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, |cx| {
        let target = callback_target(cx, magic_value, CallbackKind::Constructor)?;
        let state = state_from_context(ctx);
        let constructor = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?;
            classes
                .get(&target.class)
                .and_then(|entry| entry.spec.constructor.as_ref())
                .map(|constructor| constructor.call)
                .ok_or_else(|| Engine::operation_error(cx, "constructor is not registered"))?
        };
        let args = if argc <= 0 || argv.is_null() {
            &[]
        } else {
            // SAFETY: QuickJS provides argc live arguments for this callback.
            unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        };
        let value = constructor(cx, args)?;
        let prototype = unsafe { qjs::JS_GetPropertyStr(ctx, new_target, c"prototype".as_ptr()) };
        if unsafe { qjs::JS_IsException(prototype) } {
            return Err(take_exception_value(cx));
        }
        let rc = unsafe { qjs::JS_SetPrototype(ctx, value, prototype) };
        unsafe { qjs::JS_FreeValue(ctx, prototype) };
        if rc < 0 {
            return Err(take_exception_value(cx));
        }
        Ok(value)
    })
}

fn catch_callback<F>(ctx: *mut qjs::JSContext, f: F) -> qjs::JSValue
where
    F: FnOnce(Context<'_>) -> core::Result<qjs::JSValue, qjs::JSValue>,
{
    let scope = Scope::new(ctx);
    let cx = Context { ctx, scope: &scope };
    match catch_unwind(AssertUnwindSafe(|| f(cx))) {
        Ok(Ok(value)) => {
            scope.escape(value);
            value
        }
        Ok(Err(error)) => {
            scope.escape(error);
            unsafe { qjs::JS_Throw(ctx, error) }
        }
        Err(_) => throw_message(ctx, "Rust callback panicked", false),
    }
}

extern "C" fn qjs_finalizer(rt: *mut qjs::JSRuntime, value: qjs::JSValue) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let state = state_from_runtime(rt);
        let quickjs_id = unsafe { qjs::JS_GetClassID(value) };
        let raw = unsafe { qjs::JS_GetOpaque(value, quickjs_id) };
        let Some(raw) = NonNull::new(raw.cast::<ObjectPayload>()) else {
            return;
        };
        let payload = unsafe { Box::from_raw(raw.as_ptr()) };
        core::release_payload_values::<Engine>(payload.payload.as_ref(), &mut |value| unsafe {
            qjs::JS_FreeValueRT(rt, value);
        });
        (payload.spec.finalizer)(payload.payload, &state.env);
    }));
}

unsafe extern "C" fn qjs_gc_mark(
    rt: *mut qjs::JSRuntime,
    value: qjs::JSValue,
    mark_func: qjs::JS_MarkFunc,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let quickjs_id = unsafe { qjs::JS_GetClassID(value) };
        let raw = unsafe { qjs::JS_GetOpaque(value, quickjs_id) };
        let Some(raw) = NonNull::new(raw.cast::<ObjectPayload>()) else {
            return;
        };
        let payload = unsafe { raw.as_ref() };
        let mut visit = |value| unsafe {
            qjs::JS_MarkValue(rt, value, mark_func);
        };
        core::trace_payload_values::<Engine>(payload.payload.as_ref(), &mut visit);
    }));
}

extern "C" fn promise_rejection_tracker(
    ctx: *mut qjs::JSContext,
    promise: qjs::JSValue,
    reason: qjs::JSValue,
    is_handled: bool,
    opaque: *mut c_void,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(state) = NonNull::new(opaque.cast::<State>()) else {
            return;
        };
        let state = unsafe { state.as_ref() };
        if is_handled {
            state.mark_handled(ctx, promise);
        } else {
            state.track_unhandled(ctx, promise, reason);
        }
    }));
}

fn state_from_context(ctx: *mut qjs::JSContext) -> &'static State {
    let rt = unsafe { qjs::JS_GetRuntime(ctx) };
    state_from_runtime(rt)
}

fn state_from_runtime(rt: *mut qjs::JSRuntime) -> &'static State {
    let raw = unsafe { qjs::JS_GetRuntimeOpaque(rt) }.cast::<State>();
    unsafe { &*raw }
}

fn qjs_value_with_tag(tag: i64) -> qjs::JSValue {
    qjs::JSValue {
        u: qjs::JSValueUnion { int32: 0 },
        tag,
    }
}

fn same_js_value(left: qjs::JSValue, right: qjs::JSValue) -> bool {
    left.tag == right.tag && unsafe { left.u.ptr == right.u.ptr }
}

fn throw_message(ctx: *mut qjs::JSContext, message: &str, type_error: bool) -> qjs::JSValue {
    let fallback = c"webgpu-native-js error";
    match CString::new(message) {
        Ok(message) if type_error => unsafe { qjs::JS_ThrowTypeError(ctx, message.as_ptr()) },
        Ok(message) => unsafe { qjs::JS_ThrowInternalError(ctx, message.as_ptr()) },
        Err(_) => unsafe { qjs::JS_ThrowInternalError(ctx, fallback.as_ptr()) },
    }
}

fn set_error_name(cx: Context<'_>, error: qjs::JSValue, name: &str) -> qjs::JSValue {
    let name = unsafe { qjs::JS_NewStringLen(cx.ctx, name.as_ptr().cast(), name.len()) };
    if unsafe { qjs::JS_IsException(name) } {
        return take_exception_value(cx);
    }
    if unsafe { qjs::JS_SetPropertyStr(cx.ctx, error, c"name".as_ptr(), name) } < 0 {
        take_exception_value(cx)
    } else {
        error
    }
}

fn take_exception_value(cx: Context<'_>) -> qjs::JSValue {
    let value = unsafe { qjs::JS_GetException(cx.ctx) };
    cx.scope.track(value);
    value
}

fn clear_pending_exception(ctx: *mut qjs::JSContext) {
    if unsafe { qjs::JS_HasException(ctx) } {
        let exception = unsafe { qjs::JS_GetException(ctx) };
        unsafe { qjs::JS_FreeValue(ctx, exception) };
    }
}

fn take_exception(ctx: *mut qjs::JSContext, fallback: &'static str) -> String {
    let exception = unsafe { qjs::JS_GetException(ctx) };
    let message = exception_or_value(ctx, exception);
    if message.is_empty() {
        fallback.to_owned()
    } else {
        message
    }
}

fn exception_or_value(ctx: *mut qjs::JSContext, value: qjs::JSValue) -> String {
    let raw = unsafe { qjs::JS_ToCString(ctx, value) };
    let message = if raw.is_null() {
        String::new()
    } else {
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { qjs::JS_FreeCString(ctx, raw) };
        text
    };
    unsafe { qjs::JS_FreeValue(ctx, value) };
    message
}

fn gpu_dispatch() -> core::GpuDispatch {
    core::for_each_gpu_dispatch_entry!(__gpu_dispatch_from_ffi, ffi_wgpu)
}
unsafe extern "C" fn arraybuffer_free(
    _rt: *mut qjs::JSRuntime,
    opaque: *mut c_void,
    ptr: *mut c_void,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(owner) = NonNull::new(opaque.cast::<ArrayBufferOwner>()) else {
            return;
        };
        let detaching = DETACHING_ARRAYBUFFER.with(Cell::get);
        if !ptr.is_null() && detaching {
            let owner = unsafe { &mut *owner.as_ptr() };
            release_arraybuffer_owner(owner);
            return;
        }
        let mut owner = unsafe { Box::from_raw(owner.as_ptr()) };
        release_arraybuffer_owner(&mut owner);
    }));
}

fn release_arraybuffer_owner(owner: &mut ArrayBufferOwner) {
    if owner.released {
        return;
    }
    owner.released = true;
    let _ = owner.queue.enqueue(core::ReleaseRequest::Buffer {
        buffer: owner.buffer,
        gpu: owner.gpu,
    });
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{c_int, core, ffi_wgpu as wgpu, qjs, CallbackKind, Context, Engine, Runtime};
    use webgpu_native_js_core::JsEngine;

    static COUNTED_BUFFER_RELEASES: AtomicUsize = AtomicUsize::new(0);
    static COUNTED_SAMPLER_RELEASES: AtomicUsize = AtomicUsize::new(0);
    thread_local! {
        static RECORDED_ENTRY_POINTS: RefCell<Vec<Option<Vec<u8>>>> = const { RefCell::new(Vec::new()) };
    }

    struct SendPtr<T>(*mut T);

    // SAFETY: these tests move native pointers only as opaque keys for the
    // enqueue-only event forwarder. The receiving thread never dereferences the
    // pointer and never calls webgpu.h with it.
    unsafe impl<T> Send for SendPtr<T> {}

    impl<T> SendPtr<T> {
        fn new(ptr: *mut T) -> Self {
            Self(ptr)
        }

        fn get(self) -> *mut T {
            self.0
        }
    }

    struct AdapterRequestState {
        status: Cell<wgpu::WGPURequestAdapterStatus>,
        handle: Cell<wgpu::WGPUAdapter>,
    }

    struct DeviceRequestState {
        status: Cell<wgpu::WGPURequestDeviceStatus>,
        handle: Cell<wgpu::WGPUDevice>,
    }

    struct NativeSetup {
        instance: wgpu::WGPUInstance,
        adapter: wgpu::WGPUAdapter,
        device: wgpu::WGPUDevice,
    }

    impl Drop for NativeSetup {
        fn drop(&mut self) {
            unsafe {
                wgpu::wgpuDeviceRelease(self.device);
                wgpu::wgpuAdapterRelease(self.adapter);
                wgpu::wgpuInstanceRelease(self.instance);
            }
        }
    }

    fn native_setup() -> NativeSetup {
        let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        assert!(!instance.is_null());

        // AllowProcessEvents runs callbacks on the thread that calls
        // wgpuInstanceProcessEvents, so the userdata clone is single-threaded.
        let adapter_state = Rc::new(AdapterRequestState::new());
        let adapter_callback_state = Rc::into_raw(Rc::clone(&adapter_state)).cast_mut().cast();
        let adapter_info = wgpu::WGPURequestAdapterCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(adapter_callback),
            userdata1: adapter_callback_state,
            userdata2: ptr::null_mut(),
        };
        unsafe {
            wgpu::wgpuInstanceRequestAdapter(instance, ptr::null(), adapter_info);
            wgpu::wgpuInstanceProcessEvents(instance);
        }
        assert_eq!(
            adapter_state.status.get(),
            wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success
        );
        let adapter = adapter_state.handle.get();
        assert!(!adapter.is_null());

        // AllowProcessEvents runs callbacks on the thread that calls
        // wgpuInstanceProcessEvents, so the userdata clone is single-threaded.
        let device_state = Rc::new(DeviceRequestState::new());
        let device_callback_state = Rc::into_raw(Rc::clone(&device_state)).cast_mut().cast();
        let device_info = wgpu::WGPURequestDeviceCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(device_callback),
            userdata1: device_callback_state,
            userdata2: ptr::null_mut(),
        };
        unsafe {
            wgpu::wgpuAdapterRequestDevice(adapter, ptr::null(), device_info);
            wgpu::wgpuInstanceProcessEvents(instance);
        }
        assert_eq!(
            device_state.status.get(),
            wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success
        );
        let device = device_state.handle.get();
        assert!(!device.is_null());

        NativeSetup {
            instance,
            adapter,
            device,
        }
    }

    fn eval_drop(runtime: &Runtime, source: &str, name: &str) {
        let value = runtime.eval(source, name).expect(name);
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), value) };
    }

    fn engine_ok<T>(result: core::Result<T, qjs::JSValue>, message: &str) -> T {
        match result {
            Ok(value) => value,
            Err(_) => panic!("{message}"),
        }
    }

    fn engine_err<T>(result: core::Result<T, qjs::JSValue>, message: &str) -> qjs::JSValue {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    fn global_value(runtime: &Runtime, name: &str) -> qjs::JSValue {
        let name = std::ffi::CString::new(name).expect("global name");
        let global = unsafe { qjs::JS_GetGlobalObject(runtime.raw_context()) };
        let value = unsafe { qjs::JS_GetPropertyStr(runtime.raw_context(), global, name.as_ptr()) };
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), global) };
        assert!(!unsafe { qjs::JS_IsException(value) }, "global lookup");
        value
    }

    #[test]
    fn shared_j17_parity_script_matches_expected_output() {
        const SCRIPT: &str = include_str!("../../../tests/parity/parity.js");
        const EXPECTED: &str = include_str!("../../../tests/parity/expected.txt");

        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval_drop(&runtime, SCRIPT, "tests/parity/parity.js");
        runtime
            .forward_device_lost(
                setup.device,
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "parity loss",
            )
            .expect("forward parity loss");

        let mut done = false;
        for _ in 0..32 {
            unsafe { runtime.tick(setup.instance) }.expect("parity tick");
            let value = global_value(&runtime, "parityDone");
            done = unsafe { qjs::JS_ToBool(runtime.raw_context(), value) } != 0;
            unsafe { qjs::JS_FreeValue(runtime.raw_context(), value) };
            if done {
                break;
            }
        }
        assert!(done, "parity script did not finish within 32 ticks");

        let joined = runtime
            .eval("globalThis.parityLog.join('\\n')", "tests/parity/join.js")
            .expect("join parity log");
        let actual = format!(
            "{}\n",
            super::exception_or_value(runtime.raw_context(), joined)
        );
        assert_eq!(actual, EXPECTED);
    }

    #[test]
    fn js_engine_global_returns_a_scope_tracked_owned_value() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let scope = super::Scope::new(runtime.raw_context());
        let cx = Context {
            ctx: runtime.raw_context(),
            scope: &scope,
        };

        let global = Engine::global(cx);
        assert!(!unsafe { qjs::JS_IsUndefined(global) });
        assert_eq!(scope.values.borrow().len(), 1);
    }

    #[test]
    fn j18_j1_direct_resolver_defers_then_until_quickjs_pending_job() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let (promise, deferred) = super::with_scope(runtime.raw_context(), |cx| {
            let (promise, deferred) = Engine::new_promise(cx).unwrap_or_else(|_| panic!("promise"));
            cx.scope.escape(promise);
            (promise, deferred)
        });
        runtime
            .set_global_value("j18Promise", promise)
            .expect("set promise");
        eval_drop(
            &runtime,
            "var j18Ran = false; j18Promise.then(function () { j18Ran = true; });",
            "j18-j1-quickjs-setup.js",
        );

        let undefined = super::qjs_value_with_tag(qjs::JS_TAG_UNDEFINED as i64);
        let mut arguments = [undefined];
        // J18/J1 counterfactual: bypass settlement machinery and call the
        // resolver directly. QuickJS leaves the continuation pending, unlike
        // JSC F2; J1 exists so binding settlement hides this divergence.
        let result = unsafe {
            qjs::JS_Call(
                runtime.raw_context(),
                deferred.resolve(),
                undefined,
                1,
                arguments.as_mut_ptr(),
            )
        };
        assert!(
            !unsafe { qjs::JS_IsException(result) },
            "direct resolver call threw"
        );
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), result) };

        let before_job = global_value(&runtime, "j18Ran");
        assert_eq!(
            unsafe { qjs::JS_ToBool(runtime.raw_context(), before_job) },
            0,
            "QuickJS continuation must still be pending after resolver returns"
        );
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), before_job) };

        let mut job_context = ptr::null_mut();
        let jobs = unsafe {
            qjs::JS_ExecutePendingJob(qjs::JS_GetRuntime(runtime.raw_context()), &mut job_context)
        };
        assert_eq!(jobs, 1, "the promise continuation must be one pending job");
        let after_job = global_value(&runtime, "j18Ran");
        assert_ne!(
            unsafe { qjs::JS_ToBool(runtime.raw_context(), after_job) },
            0,
            "QuickJS continuation must run after JS_ExecutePendingJob"
        );
        unsafe {
            qjs::JS_FreeValue(runtime.raw_context(), after_job);
            qjs::JS_FreeValue(runtime.raw_context(), deferred.resolve());
            qjs::JS_FreeValue(runtime.raw_context(), deferred.reject());
        }
        runtime.clear_global("j18Promise").expect("clear promise");
    }

    #[test]
    fn js_engine_get_property_value_tracks_success_and_owned_error() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let object = runtime
            .eval(
                "({ answer: 42, get boom() { throw new Error('property boom'); } })",
                "property-value-primitives.js",
            )
            .expect("object");
        {
            let scope = super::Scope::new(runtime.raw_context());
            let cx = Context {
                ctx: runtime.raw_context(),
                scope: &scope,
            };
            let key = engine_ok(Engine::string(cx, "answer"), "answer key");
            let value = engine_ok(Engine::get_property_value(cx, object, key), "answer value");
            assert_eq!(engine_ok(Engine::to_f64(cx, value), "answer number"), 42.0);
            assert_eq!(scope.values.borrow().len(), 2);

            let error_key = engine_ok(Engine::string(cx, "boom"), "boom key");
            let error = engine_err(
                Engine::get_property_value(cx, object, error_key),
                "getter must throw",
            );
            assert!(!unsafe { qjs::JS_HasException(runtime.raw_context()) });
            let arena = core::Arena::new();
            assert!(engine_ok(Engine::to_str(cx, error, &arena), "error string")
                .contains("property boom"));
            assert_eq!(scope.values.borrow().len(), 4);
        }
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), object) };
    }

    #[test]
    fn js_engine_call_tracks_success_and_owned_error() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let function = runtime
            .eval(
                "(function (value) { if (value < 0) throw new Error('call boom'); return this.base + value; })",
                "call-primitive-function.js",
            )
            .expect("function");
        let receiver = runtime
            .eval("({ base: 40 })", "call-primitive-receiver.js")
            .expect("receiver");
        {
            let scope = super::Scope::new(runtime.raw_context());
            let cx = Context {
                ctx: runtime.raw_context(),
                scope: &scope,
            };
            let two = engine_ok(Engine::number(cx, 2.0), "two");
            let value = engine_ok(Engine::call(cx, function, receiver, &[two]), "call result");
            assert_eq!(engine_ok(Engine::to_f64(cx, value), "result number"), 42.0);

            let negative = engine_ok(Engine::number(cx, -1.0), "negative");
            let error = engine_err(
                Engine::call(cx, function, receiver, &[negative]),
                "function must throw",
            );
            assert!(!unsafe { qjs::JS_HasException(runtime.raw_context()) });
            let arena = core::Arena::new();
            assert!(
                engine_ok(Engine::to_str(cx, error, &arena), "error string").contains("call boom")
            );
            assert_eq!(scope.values.borrow().len(), 4);
        }
        unsafe {
            qjs::JS_FreeValue(runtime.raw_context(), function);
            qjs::JS_FreeValue(runtime.raw_context(), receiver);
        }
    }

    impl AdapterRequestState {
        fn new() -> Self {
            Self {
                status: Cell::new(
                    wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_CallbackCancelled,
                ),
                handle: Cell::new(ptr::null_mut()),
            }
        }
    }

    impl DeviceRequestState {
        fn new() -> Self {
            Self {
                status: Cell::new(
                    wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_CallbackCancelled,
                ),
                handle: Cell::new(ptr::null_mut()),
            }
        }
    }

    unsafe extern "C" fn adapter_callback(
        status: wgpu::WGPURequestAdapterStatus,
        adapter: wgpu::WGPUAdapter,
        _message: wgpu::WGPUStringView,
        userdata1: *mut std::ffi::c_void,
        _userdata2: *mut std::ffi::c_void,
    ) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            if userdata1.is_null() {
                return;
            }
            let state = unsafe { Rc::from_raw(userdata1.cast::<AdapterRequestState>()) };
            state.status.set(status);
            state.handle.set(adapter);
        }));
    }

    unsafe extern "C" fn device_callback(
        status: wgpu::WGPURequestDeviceStatus,
        device: wgpu::WGPUDevice,
        _message: wgpu::WGPUStringView,
        userdata1: *mut std::ffi::c_void,
        _userdata2: *mut std::ffi::c_void,
    ) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            if userdata1.is_null() {
                return;
            }
            let state = unsafe { Rc::from_raw(userdata1.cast::<DeviceRequestState>()) };
            state.status.set(status);
            state.handle.set(device);
        }));
    }

    const PANIC_CLASS: core::ClassId = core::ClassId(10_000);
    const TEARDOWN_CLASS: core::ClassId = core::ClassId(10_001);
    static TEARDOWN_BUFFER_RELEASES: AtomicUsize = AtomicUsize::new(0);

    fn panicking_method(
        _cx: Context<'_>,
        _this: qjs::JSValue,
        _args: &[qjs::JSValue],
    ) -> core::Result<qjs::JSValue, qjs::JSValue> {
        panic!("method panic");
    }

    fn panicking_getter(
        _cx: Context<'_>,
        _this: qjs::JSValue,
    ) -> core::Result<qjs::JSValue, qjs::JSValue> {
        panic!("getter panic");
    }

    fn panicking_setter(
        _cx: Context<'_>,
        _this: qjs::JSValue,
        _value: qjs::JSValue,
    ) -> core::Result<(), qjs::JSValue> {
        panic!("setter panic");
    }

    fn panicking_finalizer(_payload: Box<dyn std::any::Any + Send>, _env: &core::Environment) {
        panic!("finalizer panic");
    }

    fn panicking_spec() -> &'static core::ClassSpec<Engine> {
        Box::leak(Box::new(core::ClassSpec::<Engine> {
            name: "PanicClass",
            id: PANIC_CLASS,
            constructor: None,
            properties: Box::leak(Box::new([core::PropertySpec::<Engine> {
                name: "panicProp",
                get: Some(panicking_getter),
                set: Some(panicking_setter),
            }])),
            methods: Box::leak(Box::new([core::MethodSpec::<Engine> {
                name: "panicMethod",
                length: 0,
                call: panicking_method,
            }])),
            finalizer: panicking_finalizer,
        }))
    }

    fn teardown_finalizer(_payload: Box<dyn std::any::Any + Send>, env: &core::Environment) {
        let _ = env.queue().enqueue(core::ReleaseRequest::Buffer {
            buffer: 1usize as core::WGPUBuffer,
            gpu: teardown_dispatch(),
        });
    }

    fn teardown_spec() -> &'static core::ClassSpec<Engine> {
        Box::leak(Box::new(core::ClassSpec::<Engine> {
            name: "TeardownClass",
            id: TEARDOWN_CLASS,
            constructor: None,
            properties: &[],
            methods: &[],
            finalizer: teardown_finalizer,
        }))
    }

    fn teardown_dispatch() -> core::GpuDispatch {
        let mut gpu = super::gpu_dispatch();
        gpu.buffer_release = teardown_buffer_release;
        gpu
    }

    unsafe fn teardown_buffer_release(_buffer: core::WGPUBuffer) {
        TEARDOWN_BUFFER_RELEASES.fetch_add(1, Ordering::SeqCst);
    }

    fn counted_release_dispatch() -> core::GpuDispatch {
        let mut gpu = super::gpu_dispatch();
        gpu.buffer_release = counted_buffer_release;
        gpu
    }

    fn counted_sampler_release_dispatch() -> core::GpuDispatch {
        let mut gpu = super::gpu_dispatch();
        gpu.sampler_release = counted_sampler_release;
        gpu
    }

    fn entry_point_recording_dispatch() -> core::GpuDispatch {
        let mut gpu = super::gpu_dispatch();
        gpu.device_create_compute_pipeline = record_compute_pipeline_entry_point;
        gpu.compute_pipeline_release = release_recorded_compute_pipeline;
        gpu
    }

    unsafe fn record_compute_pipeline_entry_point(
        _device: core::WGPUDevice,
        descriptor: *const core::WGPUComputePipelineDescriptor,
    ) -> core::WGPUComputePipeline {
        let view = unsafe { (*descriptor).compute.entryPoint };
        let entry_point = if view.data.is_null() && view.length == core::wgpu_strlen() {
            None
        } else {
            Some(
                unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length) }.to_vec(),
            )
        };
        RECORDED_ENTRY_POINTS.with(|values| values.borrow_mut().push(entry_point));
        1usize as core::WGPUComputePipeline
    }

    unsafe fn release_recorded_compute_pipeline(_pipeline: core::WGPUComputePipeline) {}

    unsafe fn counted_buffer_release(buffer: core::WGPUBuffer) {
        COUNTED_BUFFER_RELEASES.fetch_add(1, Ordering::SeqCst);
        unsafe { wgpu::wgpuBufferRelease(buffer) };
    }

    unsafe fn counted_sampler_release(sampler: core::WGPUSampler) {
        COUNTED_SAMPLER_RELEASES.fetch_add(1, Ordering::SeqCst);
        unsafe { wgpu::wgpuSamplerRelease(sampler) };
    }

    fn callback_magic(runtime: &Runtime, kind: CallbackKind, index: usize) -> c_int {
        let state = super::state_from_context(runtime.raw_context());
        let callbacks = state.callbacks.lock().expect("callbacks");
        callbacks
            .iter()
            .position(|target| {
                target.class == PANIC_CLASS && target.kind == kind && target.index == index
            })
            .map(|index| (index + 1) as c_int)
            .expect("callback magic")
    }

    fn panic_test_instance(runtime: &Runtime) -> qjs::JSValue {
        super::with_scope(runtime.raw_context(), |cx| {
            assert!(Engine::register_class(cx, panicking_spec()).is_ok());
            let instance =
                Engine::new_instance(cx, PANIC_CLASS, Box::new(())).unwrap_or_else(|_| {
                    panic!("instance");
                });
            cx.scope.escape(instance);
            instance
        })
    }

    fn assert_exception(runtime: &Runtime, value: qjs::JSValue) {
        assert!(unsafe { qjs::JS_IsException(value) });
        let message = super::take_exception(runtime.raw_context(), "exception");
        assert!(message.contains("Rust callback panicked"));
    }

    #[test]
    fn extern_callbacks_contain_panicking_method_getter_and_setter() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let instance = panic_test_instance(&runtime);
        let method_magic = callback_magic(&runtime, CallbackKind::Method, 0);
        let getter_magic = callback_magic(&runtime, CallbackKind::Getter, 0);
        let setter_magic = callback_magic(&runtime, CallbackKind::Setter, 0);

        let method = unsafe {
            super::qjs_method(
                runtime.raw_context(),
                instance,
                0,
                ptr::null_mut(),
                method_magic,
            )
        };
        assert_exception(&runtime, method);

        let getter = unsafe { super::qjs_getter(runtime.raw_context(), instance, getter_magic) };
        assert_exception(&runtime, getter);

        let setter = unsafe {
            let value = super::with_scope(runtime.raw_context(), |cx| Engine::undefined(cx));
            super::qjs_setter(runtime.raw_context(), instance, value, setter_magic)
        };
        assert_exception(&runtime, setter);

        unsafe { qjs::JS_SetOpaque(instance, ptr::null_mut()) };
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), instance) };
    }

    #[test]
    fn extern_finalizer_contains_panic() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let instance = panic_test_instance(&runtime);
        let result = catch_unwind(AssertUnwindSafe(|| unsafe {
            super::qjs_finalizer(qjs::JS_GetRuntime(runtime.raw_context()), instance);
        }));
        assert!(result.is_ok());
        unsafe { qjs::JS_SetOpaque(instance, ptr::null_mut()) };
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), instance) };
    }

    #[test]
    fn script_runs_buffer_vertical_slice() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        eval_drop(&runtime, "var smoke = 1;", "smoke.js");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            include_str!("../tests/scripts/buffer_slice.js"),
            "buffer_slice.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        assert!(runtime.drain_releases().expect("drain") >= 2);
    }

    #[test]
    fn script_creates_labels_and_releases_sampler_on_tick() {
        let setup = native_setup();
        COUNTED_SAMPLER_RELEASES.store(0, Ordering::SeqCst);
        let runtime = Runtime::new_with_dispatch(counted_sampler_release_dispatch())
            .expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var transientSampler = device.createSampler({
                    label: "quickjs-sampler",
                    addressModeU: "repeat",
                    addressModeV: "mirror-repeat",
                    addressModeW: "clamp-to-edge",
                    magFilter: "linear",
                    minFilter: "nearest",
                    mipmapFilter: "linear",
                    lodMinClamp: 1.5,
                    lodMaxClamp: 12.5,
                    compare: "less-equal",
                    maxAnisotropy: 4
                });
                if (transientSampler.label !== "quickjs-sampler") {
                    throw new Error("sampler label mismatch");
                }
                transientSampler = null;
            "#,
            "sampler-slice.js",
        );
        runtime.run_gc();
        unsafe { runtime.tick(setup.instance) }.expect("sampler release tick");
        assert_eq!(COUNTED_SAMPLER_RELEASES.load(Ordering::SeqCst), 1);

        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("device drain");
    }

    #[test]
    fn descriptor_coercion_errors_throw_owned_values() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var bigintThrew = false;
                try {
                    device.createBuffer({ size: 10n, usage: 8 });
                } catch (e) {
                    bigintThrew = e instanceof TypeError;
                }
                if (!bigintThrew) throw new Error('BigInt coercion was returned instead of thrown');

                var getterThrew = false;
                try {
                    device.createBuffer({
                        get size() { throw new RangeError('size getter diagnostic'); },
                        usage: 8
                    });
                } catch (e) {
                    getterThrew = e instanceof RangeError && e.message === 'size getter diagnostic';
                }
                if (!getterThrew) throw new Error('property exception was not preserved');
            "#,
            "owned-errors.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn owned_error_value_is_an_actual_async_rejection_reason() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let promise = super::with_scope(runtime.raw_context(), |cx| {
            let (promise, deferred) = Engine::new_promise(cx).unwrap_or_else(|_| panic!("promise"));
            let error = Engine::type_error(cx, "async owned error");
            let reason = Engine::error_value_from_error(cx, error);
            Engine::settle_deferreds(cx, vec![(deferred, Err(reason))])
                .unwrap_or_else(|_| panic!("settle rejection"));
            cx.scope.escape(promise);
            promise
        });
        runtime
            .set_global_value("asyncFailure", promise)
            .expect("set promise");
        eval_drop(
            &runtime,
            r#"
                var asyncReasonOk = false;
                asyncFailure.catch(function (reason) {
                    asyncReasonOk = reason instanceof TypeError && reason.message === 'async owned error';
                });
            "#,
            "async-owned-error.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick rejection handler");
        eval_drop(
            &runtime,
            "if (!asyncReasonOk) throw new Error('rejection reason was not the owned error');",
            "async-owned-error-check.js",
        );
        runtime.clear_global("asyncFailure").expect("clear promise");
    }

    #[test]
    fn shared_error_class_constructor_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval_drop(
            &runtime,
            include_str!("../../../tests/error-classes.js"),
            "error-classes.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain releases");
    }

    #[test]
    fn shared_named_async_rejection_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval_drop(
            &runtime,
            include_str!("../../../tests/error-rejection.js"),
            "error-rejection.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick empty pop");
        let done = global_value(&runtime, "errorRejectionDone");
        assert!(unsafe { qjs::JS_ToBool(runtime.raw_context(), done) } != 0);
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), done) };
    }

    #[test]
    fn shared_device_event_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval_drop(
            &runtime,
            include_str!("../../../tests/device-events.js"),
            "device-events.js",
        );
        let forwarder = runtime.device_event_forwarder();
        let device = SendPtr::new(setup.device);
        std::thread::spawn(move || {
            let device = device.get();
            forwarder
                .forward_uncaptured_error(
                    device,
                    wgpu::WGPUErrorType_WGPUErrorType_Validation,
                    "script uncaptured",
                )
                .expect("forward uncaptured");
            forwarder
                .forward_device_lost(
                    device,
                    wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                    "script lost",
                )
                .expect("forward lost");
        })
        .join()
        .expect("forward thread");
        unsafe { runtime.tick(setup.instance) }.expect("device event tick");
        eval_drop(
            &runtime,
            "if (!uncapturedEventPassed || !deviceLostPassed) throw new Error('device event callback did not run');",
            "device-events-check.js",
        );
    }

    #[test]
    fn uncaptured_handler_can_unregister_itself_and_keeps_running() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var selfRemovingCalls = 0;
                var selfRemovingWork = 0;
                device.onuncapturederror = function () {
                    selfRemovingCalls++;
                    device.onuncapturederror = null;
                    var values = [];
                    for (var i = 0; i < 128; i++) values.push(i * i);
                    selfRemovingWork = values.reduce(function (a, b) { return a + b; }, 0);
                };
            "#,
            "uncaptured-self-remove.js",
        );
        let forwarder = runtime.device_event_forwarder();
        forwarder
            .forward_uncaptured_error(
                setup.device,
                wgpu::WGPUErrorType_WGPUErrorType_Validation,
                "first",
            )
            .expect("first forward");
        unsafe { runtime.tick(setup.instance) }.expect("first tick");
        forwarder
            .forward_uncaptured_error(
                setup.device,
                wgpu::WGPUErrorType_WGPUErrorType_Validation,
                "second",
            )
            .expect("second forward");
        unsafe { runtime.tick(setup.instance) }.expect("second tick");
        eval_drop(
            &runtime,
            r#"
                if (selfRemovingCalls !== 1) throw new Error('handler dispatched twice');
                if (selfRemovingWork !== 690880) throw new Error('handler stopped after removal');
                device.onuncapturederror = 42;
                if (device.onuncapturederror !== null) throw new Error('non-callable was not null');
            "#,
            "uncaptured-self-remove-check.js",
        );
    }

    #[test]
    fn throwing_uncaptured_handler_does_not_skip_the_next_queued_event() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var throwingHandlerCalls = 0;
                device.onuncapturederror = function () {
                    throwingHandlerCalls++;
                    if (throwingHandlerCalls === 1) throw new Error('first event throw');
                };
            "#,
            "uncaptured-throw-all.js",
        );
        let forwarder = runtime.device_event_forwarder();
        for message in ["first", "second"] {
            forwarder
                .forward_uncaptured_error(
                    setup.device,
                    wgpu::WGPUErrorType_WGPUErrorType_Validation,
                    message,
                )
                .expect("forward");
        }
        let error = unsafe { runtime.tick(setup.instance) }.expect_err("tick must report throw");
        assert!(format!("{error:?}").contains("first event throw"));
        eval_drop(
            &runtime,
            "if (throwingHandlerCalls !== 2) throw new Error('second event was skipped');",
            "uncaptured-throw-all-check.js",
        );
    }

    #[test]
    fn runtime_drop_with_handler_capturing_its_device_does_not_deadlock() {
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let setup = native_setup();
            let runtime = Runtime::new().expect("quickjs runtime");
            let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
            runtime
                .set_global_value("device", device)
                .expect("set device");
            eval_drop(
                &runtime,
                r#"
                    (function (capturedDevice) {
                        capturedDevice.onuncapturederror = function () {
                            return capturedDevice;
                        };
                    })(device);
                    device = null;
                "#,
                "self-referential-device-handler.js",
            );
            runtime.run_gc();
            drop(runtime);
            done_tx.send(()).expect("signal completion");
        });
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("Runtime::drop deadlocked on the DeviceEventJs mutex");
        thread.join().expect("teardown thread");
    }

    #[test]
    fn settlement_cold_fallback_escapes_scoped_values_before_free() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let state = super::state_from_context(runtime.raw_context());
        let trampoline = state.take_settle_trampoline().expect("trampoline");
        super::with_scope(runtime.raw_context(), |cx| {
            let (_, deferred) = Engine::new_promise(cx).unwrap_or_else(|_| panic!("promise"));
            let error = Engine::type_error(cx, "cold fallback");
            Engine::settle_deferreds(cx, vec![(deferred, Err(error))])
                .expect_err("missing trampoline must fail");
        });
        state.set_settle_trampoline(trampoline);
    }

    #[test]
    fn runtime_drop_releases_queued_unhandled_rejection_values() {
        let runtime = Runtime::new().expect("quickjs runtime");
        let promise = unsafe { qjs::JS_NewObject(runtime.raw_context()) };
        let reason =
            unsafe { qjs::JS_NewString(runtime.raw_context(), c"pending reason".as_ptr()) };
        let state = super::state_from_context(runtime.raw_context());
        state.track_unhandled(runtime.raw_context(), promise, reason);
        unsafe {
            qjs::JS_FreeValue(runtime.raw_context(), promise);
            qjs::JS_FreeValue(runtime.raw_context(), reason);
        }
        drop(runtime);
    }

    #[test]
    fn get_mapped_range_coercion_can_reenter_unmap_without_deadlock() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var reentrant = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
                var size = { valueOf() { reentrant.unmap(); return 4; } };
                var threw = false;
                try {
                    reentrant.getMappedRange(0, size);
                } catch (e) {
                    threw = true;
                }
                if (!threw) throw new Error('reentrant unmap must invalidate getMappedRange');
                reentrant = null;
            "#,
            "mapped-range-reentrant-coercion.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn required_layout_members_and_null_label_follow_webidl() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var nullLabel = device.createBuffer({ size: 4, usage: 8, label: null });
                if (nullLabel.label !== 'null') throw new Error('null label did not stringify');
                let missingEntriesError;
                try { device.createBindGroupLayout({}); }
                catch (error) { missingEntriesError = error; }
                if (!(missingEntriesError instanceof TypeError) ||
                    missingEntriesError.message !== 'entries') {
                    throw new Error('absent entries error was not named: ' + missingEntriesError);
                }
                let missingLayoutsError;
                try { device.createPipelineLayout({}); }
                catch (error) { missingLayoutsError = error; }
                if (!(missingLayoutsError instanceof TypeError) ||
                    missingLayoutsError.message !== 'bindGroupLayouts') {
                    throw new Error('absent bindGroupLayouts error was not named: ' + missingLayoutsError);
                }
                const emptyLayout = device.createBindGroupLayout({ entries: [] });
                let missingBindGroupEntriesError;
                try { device.createBindGroup({ layout: emptyLayout }); }
                catch (error) { missingBindGroupEntriesError = error; }
                if (!(missingBindGroupEntriesError instanceof TypeError) ||
                    missingBindGroupEntriesError.message !== 'entries') {
                    throw new Error('absent bind-group entries error was not named: ' + missingBindGroupEntriesError);
                }
                for (const entry of [{ visibility: 1 }, { binding: 0 }]) {
                    var threw = false;
                    try { device.createBindGroupLayout({ entries: [entry] }); }
                    catch (e) { threw = e instanceof TypeError; }
                    if (!threw) throw new Error('required layout member was defaulted');
                }
                nullLabel.destroy();
                nullLabel = null;
            "#,
            "required-layout-members.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn compute_pipeline_layout_and_present_unsupported_members_throw_named_type_errors() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                const source = '@compute @workgroup_size(1) fn main() {}';
                let error;
                try { device.createShaderModule({ code: source, compilationHints: [] }); }
                catch (caught) { error = caught; }
                if (!(error instanceof TypeError) ||
                    error.message !== 'compilationHints are not supported yet') {
                    throw new Error('compilationHints error was not named: ' + error);
                }

                const module = device.createShaderModule({ code: source });
                for (const layout of [undefined, null]) {
                    error = undefined;
                    const descriptor = { compute: { module } };
                    if (layout === null) descriptor.layout = null;
                    try { device.createComputePipeline(descriptor); }
                    catch (caught) { error = caught; }
                    if (!(error instanceof TypeError) || error.message !== 'layout') {
                        throw new Error('required layout error was not named: ' + error);
                    }
                }

                error = undefined;
                try {
                    device.createComputePipeline({
                        layout: 'auto',
                        compute: { module, constants: {} }
                    });
                } catch (caught) { error = caught; }
                if (!(error instanceof TypeError) ||
                    error.message !== 'constants are not supported yet') {
                    throw new Error('constants error was not named: ' + error);
                }

                const encoder = device.createCommandEncoder();
                error = undefined;
                try { encoder.beginComputePass({ timestampWrites: {} }); }
                catch (caught) { error = caught; }
                if (!(error instanceof TypeError) ||
                    error.message !== 'timestampWrites are not supported yet') {
                    throw new Error('timestampWrites error was not named: ' + error);
                }

            "#,
            "required-layout-and-unsupported-members.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn entry_point_null_reaches_c_as_the_string_null_from_script() {
        let setup = native_setup();
        RECORDED_ENTRY_POINTS.with(|values| values.borrow_mut().clear());
        let runtime =
            Runtime::new_with_dispatch(entry_point_recording_dispatch()).expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                const entryPointModule = device.createShaderModule({
                    code: '@compute @workgroup_size(1) fn main() {}'
                });
                device.createComputePipeline({
                    layout: 'auto', compute: { module: entryPointModule }
                });
                device.createComputePipeline({
                    layout: 'auto', compute: { module: entryPointModule, entryPoint: null }
                });
            "#,
            "entry-point-null.js",
        );
        RECORDED_ENTRY_POINTS.with(|values| {
            assert_eq!(
                values.borrow().as_slice(),
                &[None, Some(b"null".to_vec())],
                "script omission and present null must reach distinct C string views"
            );
        });
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn unsupported_bind_group_layout_kinds_throw_named_type_errors() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                for (const kind of ['sampler', 'texture', 'storageTexture', 'externalTexture']) {
                    const entry = { binding: 0, visibility: 1 };
                    entry[kind] = {};
                    let error;
                    try {
                        device.createBindGroupLayout({ entries: [entry] });
                    } catch (caught) {
                        error = caught;
                    }
                    if (!(error instanceof TypeError)) {
                        throw new Error(kind + ' binding did not throw TypeError');
                    }
                    if (error.message !== kind + ' bindings are not supported yet') {
                        throw new Error(kind + ' binding error was not named: ' + error.message);
                    }
                }
            "#,
            "unsupported-bind-group-layout-kinds.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn unsupported_bind_group_resource_kind_throws_a_named_type_error() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                const resourceLayout = device.createBindGroupLayout({ entries: [] });
                const directResourceBuffer = device.createBuffer({ size: 4, usage: 8 });
                let missingBindingError;
                try {
                    device.createBindGroup({
                        layout: resourceLayout,
                        entries: [{ resource: { buffer: directResourceBuffer } }]
                    });
                } catch (caught) {
                    missingBindingError = caught;
                }
                if (!(missingBindingError instanceof TypeError) ||
                    missingBindingError.message !== 'binding') {
                    throw new Error('absent binding error was not named: ' + missingBindingError);
                }
                let resourceError;
                try {
                    device.createBindGroup({
                        layout: resourceLayout,
                        entries: [{ binding: 0, resource: { sampler: {} } }]
                    });
                } catch (caught) {
                    resourceError = caught;
                }
                if (!(resourceError instanceof TypeError)) {
                    throw new Error('unsupported resource did not throw TypeError');
                }
                if (resourceError.message !== 'resource must be a GPUBufferBinding') {
                    throw new Error('unsupported resource error was not named: ' + resourceError.message);
                }
                let directResourceError;
                try {
                    device.createBindGroup({
                        layout: resourceLayout,
                        entries: [{ binding: 0, resource: directResourceBuffer }]
                    });
                } catch (caught) {
                    directResourceError = caught;
                }
                if (!(directResourceError instanceof TypeError) ||
                    directResourceError.message !== 'resource must be a GPUBufferBinding') {
                    throw new Error('direct buffer resource did not require GPUBufferBinding');
                }
                directResourceBuffer.destroy();
            "#,
            "unsupported-bind-group-resource.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn get_mapped_range_rejects_unmapped_and_destroyed_buffers() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var unmapped = device.createBuffer({ size: 8, usage: 2 });
                var unmappedThrew = false;
                try { unmapped.getMappedRange(); }
                catch (e) { unmappedThrew = e.name === 'OperationError'; }
                if (!unmappedThrew) throw new Error('unmapped getMappedRange did not throw');

                var destroyed = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
                destroyed.destroy();
                var destroyedThrew = false;
                try { destroyed.getMappedRange(); }
                catch (e) { destroyedThrew = e.name === 'OperationError'; }
                if (!destroyedThrew) throw new Error('destroyed getMappedRange did not throw');
                unmapped = destroyed = null;
            "#,
            "get-mapped-range-errors.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn device_queue_property_has_same_object_identity() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            "if (device.queue !== device.queue) throw new Error('GPUDevice.queue is not SameObject');",
            "device-queue-same-object.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn tick_surfaces_throwing_microtask_message() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        eval_drop(
            &runtime,
            "queueMicrotask(function () { throw new Error('boom'); });",
            "throwing_then.js",
        );
        let error = unsafe { runtime.tick(setup.instance) }.expect_err("tick error");
        assert!(format!("{error:?}").contains("boom"));
    }

    #[test]
    fn process_events_without_microtasks_does_not_resume_await() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        eval_drop(
            &runtime,
            "var ran = false; async function f() { await 0; ran = true; } f();",
            "await.js",
        );
        unsafe { runtime.process_events_only(setup.instance) };
        eval_drop(
            &runtime,
            "if (ran) throw new Error('await resumed too early');",
            "check1.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        eval_drop(
            &runtime,
            "if (!ran) throw new Error('await did not resume');",
            "check2.js",
        );
    }

    #[test]
    fn tick_reports_unhandled_rejection_after_microtasks_drain() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        eval_drop(
            &runtime,
            "Promise.resolve().then(function () { throw new Error('unhandled boom'); });",
            "unhandled.js",
        );
        let error = unsafe { runtime.tick(setup.instance) }.expect_err("tick error");
        assert!(format!("{error:?}").contains("unhandled boom"));
    }

    #[test]
    fn tick_ignores_rejection_handled_before_drain_finishes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        eval_drop(
            &runtime,
            "var handled = false; Promise.resolve().then(function () { throw new Error('handled boom'); }).catch(function () { handled = true; });",
            "handled.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        unsafe { runtime.tick(setup.instance) }.expect("second tick");
        eval_drop(
            &runtime,
            "if (!handled) throw new Error('catch did not run');",
            "handled-check.js",
        );
    }

    #[test]
    fn map_async_get_mapped_range_write_unmap_is_leak_free() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var asyncMapped = device.createBuffer({ size: 16, usage: 2 });
                var asyncDone = false;
                asyncMapped.mapAsync(2, 0, 8).then(function () {
                    var range = asyncMapped.getMappedRange(0, 8);
                    var view = new Uint8Array(range);
                    view[0] = 11;
                    view[7] = 22;
                    asyncMapped.unmap();
                    if (range.byteLength !== 0 || view.byteLength !== 0) {
                        throw new Error('mapAsync range was not detached');
                    }
                    asyncDone = true;
                });
            "#,
            "map-async.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        eval_drop(
            &runtime,
            "if (!asyncDone) throw new Error('mapAsync continuation did not run');",
            "map-async-check.js",
        );
        eval_drop(
            &runtime,
            "asyncMapped = null; asyncDone = undefined;",
            "map-async-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn queue_write_copy_submit_map_round_trip() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var src = device.createBuffer({ size: 8, usage: 12 });
                var dst = device.createBuffer({ size: 8, usage: 9 });
                var bytes = new ArrayBuffer(12);
                new Uint8Array(bytes).set([90, 91, 3, 1, 4, 1, 5, 9, 2, 6, 92, 93]);
                var write = new Uint8Array(bytes, 2, 8);
                write.set([3, 1, 4, 1, 5, 9, 2, 6]);
                device.queue.writeBuffer(src, 0, write);
                var encoder = device.createCommandEncoder();
                encoder.copyBufferToBuffer(src, 0, dst, 0, 8);
                var command = encoder.finish();
                device.queue.submit([command]);
                var copyDone = false;
                device.queue.onSubmittedWorkDone().then(function () {
                    return dst.mapAsync(1, 0, 8);
                }).then(function () {
                    var got = new Uint8Array(dst.getMappedRange());
                    var expected = [3, 1, 4, 1, 5, 9, 2, 6];
                    for (var i = 0; i < expected.length; i++) {
                        if (got[i] !== expected[i]) {
                            throw new Error('copy byte mismatch at ' + i + ': ' + got[i]);
                        }
                    }
                    dst.unmap();
                    copyDone = true;
                });
            "#,
            "copy-round-trip.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        unsafe { runtime.tick(setup.instance) }.expect("second tick");
        eval_drop(
            &runtime,
            "if (!copyDone) throw new Error('copy round trip did not finish');",
            "copy-round-trip-check.js",
        );
        eval_drop(
            &runtime,
            "src = null; dst = null; bytes = null; write = null; encoder = null; command = null; copyDone = undefined;",
            "copy-round-trip-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn mapped_at_creation_writes_reach_backend_and_ranges_detach() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var mapped = device.createBuffer({ size: 8, usage: 4, mappedAtCreation: true });
                var mappedReadback = device.createBuffer({ size: 8, usage: 9 });
                var createdRange = mapped.getMappedRange(0, 4);
                var createdView = new Uint8Array(createdRange);
                createdView.set([7, 8, 9, 10]);
                mapped.unmap();
                if (createdRange.byteLength !== 0 || createdView.byteLength !== 0) {
                    throw new Error('mappedAtCreation range was not detached');
                }
                var mappedEncoder = device.createCommandEncoder();
                mappedEncoder.copyBufferToBuffer(mapped, 0, mappedReadback, 0, 4);
                var mappedCommand = mappedEncoder.finish();
                device.queue.submit([mappedCommand]);
                var mappedBytesReachedBackend = false;
                device.queue.onSubmittedWorkDone().then(function () {
                    return mappedReadback.mapAsync(1, 0, 8);
                }).then(function () {
                    var got = new Uint8Array(mappedReadback.getMappedRange());
                    var expected = [7, 8, 9, 10];
                    for (var i = 0; i < expected.length; i++) {
                        if (got[i] !== expected[i]) {
                            throw new Error('mappedAtCreation backend byte mismatch at ' + i);
                        }
                    }
                    mappedReadback.unmap();
                    mappedBytesReachedBackend = true;
                });
                "#,
            "mapping.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("work-done tick");
        unsafe { runtime.tick(setup.instance) }.expect("map tick");
        eval_drop(
            &runtime,
            "if (!mappedBytesReachedBackend) throw new Error('mappedAtCreation bytes were not observed');",
            "mapping-check.js",
        );
        let _ = runtime.drain_releases().expect("drain detached ranges");
        eval_drop(
            &runtime,
            "mapped = mappedReadback = createdRange = createdView = mappedEncoder = mappedCommand = null; mappedBytesReachedBackend = undefined;",
            "cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn two_mapped_ranges_survive_until_runtime_drop() {
        let setup = native_setup();
        {
            let runtime = Runtime::new().expect("quickjs runtime");
            let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
            runtime
                .set_global_value("device", wrapped)
                .expect("set device");
            eval_drop(
                &runtime,
                r#"
                    var teardownMapped = device.createBuffer({ size: 16, usage: 2, mappedAtCreation: true });
                    var teardownFirst = teardownMapped.getMappedRange(0, 4);
                    var teardownSecond = teardownMapped.getMappedRange(8, 4);
                    new Uint8Array(teardownFirst)[0] = 17;
                    new Uint8Array(teardownSecond)[0] = 23;
                "#,
                "two-ranges-teardown.js",
            );
        }
    }

    #[test]
    fn pending_map_async_survives_until_runtime_drop() {
        let setup = native_setup();
        {
            let runtime = Runtime::new().expect("quickjs runtime");
            let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
            runtime
                .set_global_value("device", wrapped)
                .expect("set device");
            eval_drop(
                &runtime,
                r#"
                    var pendingMap = device.createBuffer({ size: 16, usage: 2 });
                    pendingMap.mapAsync(2, 0, 8);
                "#,
                "pending-map-teardown.js",
            );
        }
        unsafe { wgpu::wgpuInstanceProcessEvents(setup.instance) };
    }

    #[test]
    fn two_mapped_ranges_detach_together_on_unmap() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var twoRangeMapped = device.createBuffer({ size: 16, usage: 2, mappedAtCreation: true });
                var twoRangeFirst = twoRangeMapped.getMappedRange(0, 4);
                var twoRangeSecond = twoRangeMapped.getMappedRange(8, 4);
                var twoRangeFirstView = new Uint8Array(twoRangeFirst);
                var twoRangeSecondView = new Uint8Array(twoRangeSecond);
                twoRangeFirstView[0] = 31;
                twoRangeSecondView[0] = 37;
                twoRangeMapped.unmap();
                if (twoRangeFirst.byteLength !== 0 || twoRangeFirstView.byteLength !== 0) {
                    throw new Error('first range was not detached');
                }
                if (twoRangeSecond.byteLength !== 0 || twoRangeSecondView.byteLength !== 0) {
                    throw new Error('second range was not detached');
                }
            "#,
            "two-ranges-unmap.js",
        );
        eval_drop(
            &runtime,
            "twoRangeMapped = twoRangeFirst = twoRangeSecond = twoRangeFirstView = twoRangeSecondView = null;",
            "two-ranges-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn red_demo_detaching_only_first_range_leaves_second_readable() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var redMapped = device.createBuffer({ size: 16, usage: 2, mappedAtCreation: true });
                var redFirst = redMapped.getMappedRange(0, 4);
                var redSecond = redMapped.getMappedRange(8, 4);
                var redSecondView = new Uint8Array(redSecond);
                redSecondView[0] = 43;
            "#,
            "red-one-range.js",
        );
        let first = global_value(&runtime, "redFirst");
        crate::DETACHING_ARRAYBUFFER.with(|detaching| {
            let previous = detaching.replace(true);
            unsafe { qjs::JS_DetachArrayBuffer(runtime.raw_context(), first) };
            detaching.set(previous);
        });
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), first) };
        eval_drop(
            &runtime,
            r#"
                if (redSecond.byteLength !== 4 || redSecondView[0] !== 43) {
                    throw new Error('second range should remain readable when only first is detached');
                }
                redMapped.unmap();
            "#,
            "red-one-range-check.js",
        );
        eval_drop(
            &runtime,
            "redMapped = redFirst = redSecond = redSecondView = null;",
            "red-one-range-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn destroy_on_mapped_buffer_detaches_ranges() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var destroyMapped = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
                var destroyRange = destroyMapped.getMappedRange(0, 4);
                var destroyView = new Uint8Array(destroyRange);
                destroyView[0] = 5;
                destroyMapped.destroy();
                if (destroyRange.byteLength !== 0 || destroyView.byteLength !== 0) {
                    throw new Error('destroy did not detach mapped range');
                }
            "#,
            "destroy-mapped.js",
        );
        eval_drop(
            &runtime,
            "destroyMapped = null; destroyRange = null; destroyView = null;",
            "destroy-mapped-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn mapped_range_survives_after_buffer_wrapper_is_collected() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var rangeOwner = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
                var keptRange = rangeOwner.getMappedRange(0, 4);
                var keptView = new Uint8Array(keptRange);
                keptView[0] = 41;
                rangeOwner = null;
            "#,
            "range-keepalive.js",
        );
        runtime.run_gc();
        assert!(runtime.drain_releases().expect("drain") >= 1);
        eval_drop(
            &runtime,
            "if (keptView[0] !== 41) throw new Error('range did not survive buffer GC');",
            "range-keepalive-check.js",
        );
        eval_drop(
            &runtime,
            "keptView = null; keptRange = null;",
            "range-keepalive-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn mapped_range_dropped_without_unmap_releases_buffer_ref_on_gc() {
        let setup = native_setup();
        COUNTED_BUFFER_RELEASES.store(0, Ordering::SeqCst);
        let runtime =
            Runtime::new_with_dispatch(counted_release_dispatch()).expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var gcRangeBuffer = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
                var gcOnlyRange = gcRangeBuffer.getMappedRange(0, 4);
                var gcOnlyView = new Uint8Array(gcOnlyRange);
                gcOnlyView[0] = 61;
                gcOnlyRange = null;
                gcOnlyView = null;
                gcRangeBuffer = null;
            "#,
            "mapped-range-gc-only.js",
        );
        runtime.run_gc();
        runtime.run_gc();
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        assert_eq!(COUNTED_BUFFER_RELEASES.load(Ordering::SeqCst), 2);
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn creates_block03_objects_and_submits_compute_pass() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var objectShader = device.createShaderModule({
                    code: '@group(0) @binding(0) var<storage, read_write> data: array<u32>; @compute @workgroup_size(1) fn main() { data[0] = data[0] + 1u; }'
                });
                var objectBgl = device.createBindGroupLayout({
                    entries: [{ binding: 0, visibility: 4, buffer: { type: 'storage' } }]
                });
                var objectPipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [objectBgl] });
                var objectPipeline = device.createComputePipeline({
                    layout: objectPipelineLayout,
                    compute: { module: objectShader }
                });
                var objectBuffer = device.createBuffer({ size: 4, usage: 140 });
                var objectBindGroup = device.createBindGroup({
                    layout: objectBgl,
                    entries: [{ binding: 0, resource: { buffer: objectBuffer } }]
                });
                var objectEncoder = device.createCommandEncoder();
                var objectPass = objectEncoder.beginComputePass();
                objectPass.setPipeline(objectPipeline);
                objectPass.setBindGroup(0, objectBindGroup);
                objectPass.dispatchWorkgroups(1);
                objectPass.end();
                var objectCommand = objectEncoder.finish();
                device.queue.submit([objectCommand]);
            "#,
            "block03-objects.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        eval_drop(
            &runtime,
            "objectShader = objectBgl = objectPipelineLayout = objectPipeline = objectBuffer = objectBindGroup = objectEncoder = objectPass = objectCommand = null;",
            "block03-objects-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn invalid_encoder_and_pass_states_throw_synchronously() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                function mustThrow(fn, needle) {
                    var threw = false;
                    try {
                        fn();
                    } catch (e) {
                        threw = String(e).indexOf(needle) >= 0;
                    }
                    if (!threw) {
                        throw new Error('expected throw containing: ' + needle);
                    }
                }

                var twiceEncoder = device.createCommandEncoder();
                twiceEncoder.finish();
                mustThrow(function () { twiceEncoder.finish(); }, 'GPUCommandEncoder is finished');

                var afterFinishEncoder = device.createCommandEncoder();
                var tmpA = device.createBuffer({ size: 4, usage: 4 });
                var tmpB = device.createBuffer({ size: 4, usage: 8 });
                afterFinishEncoder.finish();
                mustThrow(function () {
                    afterFinishEncoder.copyBufferToBuffer(tmpA, 0, tmpB, 0, 4);
                }, 'GPUCommandEncoder is finished');

                var passEncoder = device.createCommandEncoder();
                var pass = passEncoder.beginComputePass();
                pass.end();
                mustThrow(function () { pass.end(); }, 'GPUComputePassEncoder is ended');
            "#,
            "invalid-states.js",
        );
        eval_drop(
            &runtime,
            "twiceEncoder = afterFinishEncoder = tmpA = tmpB = passEncoder = pass = null;",
            "invalid-states-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn command_buffer_submit_is_single_use() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                function mustThrow(fn, needle) {
                    var threw = false;
                    try {
                        fn();
                    } catch (e) {
                        threw = String(e).indexOf(needle) >= 0;
                    }
                    if (!threw) {
                        throw new Error('expected throw containing: ' + needle);
                    }
                }

                var singleUseEncoder = device.createCommandEncoder();
                var singleUseCommand = singleUseEncoder.finish();
                device.queue.submit([singleUseCommand]);
                mustThrow(function () {
                    device.queue.submit([singleUseCommand]);
                }, 'GPUCommandBuffer is consumed');
            "#,
            "command-buffer-single-use.js",
        );
        eval_drop(
            &runtime,
            "singleUseEncoder = singleUseCommand = null;",
            "command-buffer-single-use-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn sequence_conversion_rejects_array_like_objects() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var arrayLikeEncoder = device.createCommandEncoder();
                var arrayLikeCommand = arrayLikeEncoder.finish();
                var arrayLikeCommands = { length: 1, 0: arrayLikeCommand };
                var arrayLikeThrewTypeError = false;
                try {
                    device.queue.submit(arrayLikeCommands);
                } catch (e) {
                    arrayLikeThrewTypeError = e instanceof TypeError &&
                        String(e).indexOf('commands is not iterable') >= 0;
                }
                if (!arrayLikeThrewTypeError) {
                    throw new Error('array-like sequence must throw TypeError');
                }
            "#,
            "sequence-array-like-conformance.js",
        );
        eval_drop(
            &runtime,
            "arrayLikeEncoder = arrayLikeCommand = arrayLikeCommands = null;",
            "sequence-array-like-conformance-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn sequence_conversion_accepts_set_and_generator_iterables() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var setEncoder = device.createCommandEncoder();
                var setCommand = setEncoder.finish();
                device.queue.submit(new Set([setCommand]));

                var generatorEncoder = device.createCommandEncoder();
                var generatorCommand = generatorEncoder.finish();
                function* commandGenerator() {
                    yield generatorCommand;
                }
                device.queue.submit(commandGenerator());
            "#,
            "sequence-iterable-conformance.js",
        );
        eval_drop(
            &runtime,
            "setEncoder = setCommand = generatorEncoder = generatorCommand = null;",
            "sequence-iterable-conformance-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn sequence_conversion_propagates_mid_iteration_throw_without_consuming() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var throwingEncoder = device.createCommandEncoder();
                var throwingCommand = throwingEncoder.finish();
                function* throwingCommands() {
                    yield throwingCommand;
                    throw new Error('mid-iteration next failed');
                }
                var nextErrorPropagated = false;
                try {
                    device.queue.submit(throwingCommands());
                } catch (e) {
                    nextErrorPropagated = String(e).indexOf('mid-iteration next failed') >= 0;
                }
                if (!nextErrorPropagated) {
                    throw new Error('iterator next error did not propagate');
                }

                // Conversion failed before submit consumed any yielded command.
                device.queue.submit([throwingCommand]);
            "#,
            "sequence-mid-iteration-error.js",
        );
        eval_drop(
            &runtime,
            "throwingEncoder = throwingCommand = null;",
            "sequence-mid-iteration-error-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn bind_group_outlives_buffer_js_wrappers() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var lifetimeShader = device.createShaderModule({
                    code: '@group(0) @binding(0) var<storage, read_write> data: array<u32>; @compute @workgroup_size(1) fn main() { data[0] = data[0] + 1u; }'
                });
                var lifetimeBgl = device.createBindGroupLayout({
                    entries: [{ binding: 0, visibility: 4, buffer: { type: 'storage' } }]
                });
                var lifetimePipelineLayout = device.createPipelineLayout({ bindGroupLayouts: [lifetimeBgl] });
                var lifetimePipeline = device.createComputePipeline({
                    layout: lifetimePipelineLayout,
                    compute: { module: lifetimeShader }
                });
                var lifetimeBuffer = device.createBuffer({ size: 4, usage: 140 });
                var lifetimeBindGroup = device.createBindGroup({
                    layout: lifetimeBgl,
                    entries: [{ binding: 0, resource: { buffer: lifetimeBuffer } }]
                });
                lifetimeBuffer = null;
            "#,
            "bind-group-lifetime-setup.js",
        );
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain buffer wrapper");
        unsafe { runtime.tick(setup.instance) }.expect("tick after buffer wrapper drop");
        eval_drop(
            &runtime,
            r#"
                var lifetimeEncoder = device.createCommandEncoder();
                var lifetimePass = lifetimeEncoder.beginComputePass();
                lifetimePass.setPipeline(lifetimePipeline);
                lifetimePass.setBindGroup(0, lifetimeBindGroup);
                lifetimePass.dispatchWorkgroups(1);
                lifetimePass.end();
                var lifetimeCommand = lifetimeEncoder.finish();
                device.queue.submit([lifetimeCommand]);
            "#,
            "bind-group-lifetime-use.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("tick");
        eval_drop(
            &runtime,
            "lifetimeShader = lifetimeBgl = lifetimePipelineLayout = lifetimePipeline = lifetimeBindGroup = lifetimeEncoder = lifetimePass = lifetimeCommand = null;",
            "bind-group-lifetime-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn drop_runtime_keeps_opaque_valid_and_drains_finalizer_releases() {
        TEARDOWN_BUFFER_RELEASES.store(0, Ordering::SeqCst);
        {
            let runtime = Runtime::new().expect("quickjs runtime");
            let instance = super::with_scope(runtime.raw_context(), |cx| {
                assert!(Engine::register_class(cx, teardown_spec()).is_ok());
                let value = Engine::new_instance(cx, TEARDOWN_CLASS, Box::new(()))
                    .unwrap_or_else(|_| panic!("instance"));
                cx.scope.escape(value);
                value
            });
            runtime
                .set_global_value("teardownInstance", instance)
                .expect("set global");
        }
        assert_eq!(TEARDOWN_BUFFER_RELEASES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn request_adapter_request_device_promises_are_end_to_end() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval_drop(
            &runtime,
            "var gotDevice = false; gpu.requestAdapter().then(function (a) { return a.requestDevice(); }).then(function (d) { gotDevice = !!d; });",
            "request_path.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("adapter tick");
        unsafe { runtime.tick(setup.instance) }.expect("device tick");
        eval_drop(
            &runtime,
            "if (!gotDevice) throw new Error('device promise did not resolve');",
            "check.js",
        );
        eval_drop(&runtime, "gotDevice = undefined;", "cleanup.js");
        runtime.clear_global("gpu").expect("clear gpu");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn two_adapter_settlements_observe_one_tick_settle_before_then_order() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval_drop(
            &runtime,
            r#"
                var firstAdapter;
                gpu.requestAdapter().then(function (adapter) { firstAdapter = adapter; });
            "#,
            "adapter-prototype-source.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("prototype adapter tick");
        eval_drop(
            &runtime,
            r#"
                var order = [];
                var settleIndex = 0;
                Object.defineProperty(Object.getPrototypeOf(firstAdapter), 'then', {
                    configurable: true,
                    get: function () {
                        order.push('settle' + (++settleIndex));
                        return undefined;
                    }
                });
                gpu.requestAdapter().then(function () { order.push('then1'); });
                gpu.requestAdapter().then(function () { order.push('then2'); });
            "#,
            "adapter-settle-order.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("ordered adapter tick");
        eval_drop(
            &runtime,
            r#"
                var actual = order.join(',');
                if (actual !== 'settle1,settle2,then1,then2') {
                    throw new Error('settlement order was ' + actual);
                }
                // QuickJS cannot catch removal of the trampoline: N direct
                // resolver calls still defer continuations until
                // JS_ExecutePendingJob. JSC is the engine that goes red.
            "#,
            "adapter-settle-order-check.js",
        );
        eval_drop(
            &runtime,
            r#"
                delete Object.getPrototypeOf(firstAdapter).then;
                firstAdapter = undefined;
                order = undefined;
            "#,
            "adapter-settle-order-cleanup.js",
        );
        runtime.clear_global("gpu").expect("clear gpu");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    fn two_concurrent_request_adapter_promises_settle_independently() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval_drop(
            &runtime,
            r#"
                var adapters = [];
                gpu.requestAdapter().then(function (adapter) { adapters.push(adapter ? 'first' : 'missing-first'); });
                gpu.requestAdapter().then(function (adapter) { adapters.push(adapter ? 'second' : 'missing-second'); });
            "#,
            "concurrent-adapters.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("adapter tick");
        eval_drop(
            &runtime,
            "if (adapters.join(',') !== 'first,second') throw new Error('concurrent adapters settled incorrectly: ' + adapters.join(','));",
            "concurrent-adapters-check.js",
        );
        eval_drop(&runtime, "adapters = undefined;", "cleanup.js");
        runtime.clear_global("gpu").expect("clear gpu");
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }
}
