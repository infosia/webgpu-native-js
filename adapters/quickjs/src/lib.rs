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
        Ok(Self { rt, ctx, state })
    }

    /// Returns the raw QuickJS context.
    #[must_use]
    pub fn raw_context(&self) -> *mut qjs::JSContext {
        self.ctx.as_ptr()
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
    pub fn wrap_gpu(&self, instance: ffi_wgpu::WGPUInstance) -> Result<qjs::JSValue> {
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
        unsafe { ffi_wgpu::wgpuInstanceProcessEvents(instance) };
        loop {
            let mut job_ctx = ptr::null_mut();
            let rc = unsafe { qjs::JS_ExecutePendingJob(self.rt.as_ptr(), &mut job_ctx) };
            if rc > 0 {
                continue;
            }
            if rc == 0 {
                break;
            }
            let ctx = if job_ctx.is_null() {
                self.raw_context()
            } else {
                job_ctx
            };
            return Err(Error::Exception(take_exception(
                ctx,
                "JS_ExecutePendingJob",
            )));
        }
        if let Some(message) = self.state.take_unhandled_rejection(self.raw_context()) {
            return Err(Error::Exception(message));
        }
        self.drain_releases()
            .map_err(|_| Error::Exception("release queue is poisoned".to_owned()))
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
                // `AllowProcessEvents` callbacks only run from `tick()`, and
                // `tick()` cannot run concurrently with `Drop` for this
                // single-threaded runtime. Leave request boxes allocated for a
                // later backend callback; it will observe `None` and return.
                state.release_outstanding_deferreds(self.ctx.as_ptr());
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
    outstanding_deferreds: Mutex<Vec<usize>>,
    unhandled_rejections: Mutex<Vec<UnhandledRejection>>,
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
            outstanding_deferreds: Mutex::new(Vec::new()),
            unhandled_rejections: Mutex::new(Vec::new()),
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

    fn register_deferred(&self, slot: NonNull<Option<core::Deferred<Engine>>>) {
        if let Ok(mut slots) = self.outstanding_deferreds.lock() {
            slots.push(slot.as_ptr() as usize);
        }
    }

    fn unregister_deferred(&self, slot: NonNull<Option<core::Deferred<Engine>>>) {
        if let Ok(mut slots) = self.outstanding_deferreds.lock() {
            if let Some(index) = slots
                .iter()
                .position(|candidate| *candidate == slot.as_ptr() as usize)
            {
                slots.swap_remove(index);
            }
        }
    }

    fn release_outstanding_deferreds(&self, ctx: *mut qjs::JSContext) {
        let slots = self
            .outstanding_deferreds
            .lock()
            .map(|mut slots| std::mem::take(&mut *slots))
            .unwrap_or_default();
        for slot in slots {
            let slot = slot as *mut Option<core::Deferred<Engine>>;
            if slot.is_null() {
                continue;
            }
            let Some(deferred) = (unsafe { &mut *slot }).take() else {
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
    type AsyncContext = *mut qjs::JSContext;

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
            Err(value)
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

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> core::Result<f64, Self::Error> {
        let mut out = 0.0;
        if unsafe { qjs::JS_ToFloat64(cx.ctx, &mut out, value) } < 0 {
            Err(take_exception_value(cx.ctx))
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
            return Err(take_exception_value(cx.ctx));
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
            return Err(proto);
        }
        install_methods(cx, state, proto, spec)?;
        install_properties(cx, state, proto, spec)?;
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
            return Err(object);
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

    fn trace_payload(
        _cx: Self::Context<'_>,
        payload: &(dyn Any + Send),
        visit: &mut dyn FnMut(Self::Value),
    ) {
        if let Some(buffer) = payload.downcast_ref::<core::BufferPayload<Self>>() {
            buffer.trace_mapped_range_values(visit);
        }
    }

    fn undefined(_cx: Self::Context<'_>) -> Self::Value {
        qjs_value_with_tag(qjs::JS_TAG_UNDEFINED as i64)
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
        throw_message(cx.ctx, message, true)
    }

    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        throw_message(cx.ctx, message, false)
    }

    fn async_context(cx: Self::Context<'_>) -> Self::AsyncContext {
        cx.ctx
    }

    unsafe fn with_async_scope<R>(
        cx: Self::AsyncContext,
        f: impl FnOnce(Self::Context<'_>) -> R,
    ) -> R {
        with_scope(cx, f)
    }

    fn async_error_value(cx: Self::Context<'_>, message: &str) -> Self::Value {
        match CString::new(message) {
            Ok(message) => {
                let value = unsafe { qjs::JS_NewString(cx.ctx, message.as_ptr()) };
                cx.scope.track(value);
                value
            }
            Err(_) => Self::undefined(cx),
        }
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
            return Err(promise);
        }
        cx.scope.track(promise);
        Ok((promise, core::Deferred::new(resolving[0], resolving[1])))
    }

    fn settle_deferred(
        cx: Self::Context<'_>,
        deferred: core::Deferred<Self>,
        result: std::result::Result<Self::Value, Self::Value>,
    ) {
        let (func, arg) = match result {
            Ok(value) => (deferred.resolve(), value),
            Err(value) => (deferred.reject(), value),
        };
        let this = qjs_value_with_tag(qjs::JS_TAG_UNDEFINED as i64);
        let mut argv = [arg];
        cx.scope.escape(arg);
        let call = unsafe { qjs::JS_Call(cx.ctx, func, this, 1, argv.as_mut_ptr()) };
        unsafe {
            qjs::JS_FreeValue(cx.ctx, call);
            qjs::JS_FreeValue(cx.ctx, arg);
            qjs::JS_FreeValue(cx.ctx, deferred.resolve());
            qjs::JS_FreeValue(cx.ctx, deferred.reject());
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
            Err(value)
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
            Err(value)
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

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        unsafe { qjs::JS_FreeValue(cx.ctx, value) };
    }

    fn register_deferred(cx: Self::Context<'_>, slot: NonNull<Option<core::Deferred<Self>>>) {
        state_from_context(cx.ctx).register_deferred(slot);
    }

    fn unregister_deferred(cx: Self::Context<'_>, slot: NonNull<Option<core::Deferred<Self>>>) {
        state_from_context(cx.ctx).unregister_deferred(slot);
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
            return Err(func);
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
            return Err(take_exception_value(cx.ctx));
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
            return Err(take_exception_value(cx.ctx));
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
        Ok(Err(error)) => error,
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
        if let Some(buffer) = payload
            .payload
            .downcast_ref::<core::BufferPayload<Engine>>()
        {
            buffer.release_mapped_range_values(|value| unsafe {
                qjs::JS_FreeValueRT(rt, value);
            });
        }
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
        let scope = Scope::new(ptr::null_mut());
        let cx = Context {
            ctx: ptr::null_mut(),
            scope: &scope,
        };
        let mut visit = |value| unsafe {
            qjs::JS_MarkValue(rt, value, mark_func);
        };
        Engine::trace_payload(cx, payload.payload.as_ref(), &mut visit);
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

fn take_exception_value(ctx: *mut qjs::JSContext) -> qjs::JSValue {
    unsafe { qjs::JS_GetException(ctx) }
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
    core::GpuDispatch {
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

unsafe fn instance_request_adapter(
    instance: ffi_wgpu::WGPUInstance,
    options: *const ffi_wgpu::WGPURequestAdapterOptions,
    callback_info: ffi_wgpu::WGPURequestAdapterCallbackInfo,
) -> ffi_wgpu::WGPUFuture {
    unsafe { ffi_wgpu::wgpuInstanceRequestAdapter(instance, options, callback_info) }
}

unsafe fn adapter_request_device(
    adapter: ffi_wgpu::WGPUAdapter,
    descriptor: *const ffi_wgpu::WGPUDeviceDescriptor,
    callback_info: ffi_wgpu::WGPURequestDeviceCallbackInfo,
) -> ffi_wgpu::WGPUFuture {
    unsafe { ffi_wgpu::wgpuAdapterRequestDevice(adapter, descriptor, callback_info) }
}

unsafe fn adapter_release(adapter: ffi_wgpu::WGPUAdapter) {
    unsafe { ffi_wgpu::wgpuAdapterRelease(adapter) };
}

unsafe fn device_add_ref(device: core::WGPUDevice) {
    unsafe { ffi_wgpu::wgpuDeviceAddRef(device) };
}

unsafe fn device_release(device: core::WGPUDevice) {
    unsafe { ffi_wgpu::wgpuDeviceRelease(device) };
}

unsafe fn device_create_buffer(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUBufferDescriptor,
) -> core::WGPUBuffer {
    unsafe { ffi_wgpu::wgpuDeviceCreateBuffer(device, descriptor) }
}

unsafe fn device_get_queue(device: core::WGPUDevice) -> core::WGPUQueue {
    unsafe { ffi_wgpu::wgpuDeviceGetQueue(device) }
}

unsafe fn device_create_shader_module(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUShaderModuleDescriptor,
) -> core::WGPUShaderModule {
    unsafe { ffi_wgpu::wgpuDeviceCreateShaderModule(device, descriptor) }
}

unsafe fn device_create_bind_group_layout(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUBindGroupLayoutDescriptor,
) -> core::WGPUBindGroupLayout {
    unsafe { ffi_wgpu::wgpuDeviceCreateBindGroupLayout(device, descriptor) }
}

unsafe fn device_create_pipeline_layout(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUPipelineLayoutDescriptor,
) -> core::WGPUPipelineLayout {
    unsafe { ffi_wgpu::wgpuDeviceCreatePipelineLayout(device, descriptor) }
}

unsafe fn device_create_bind_group(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUBindGroupDescriptor,
) -> core::WGPUBindGroup {
    unsafe { ffi_wgpu::wgpuDeviceCreateBindGroup(device, descriptor) }
}

unsafe fn device_create_compute_pipeline(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUComputePipelineDescriptor,
) -> core::WGPUComputePipeline {
    unsafe { ffi_wgpu::wgpuDeviceCreateComputePipeline(device, descriptor) }
}

unsafe fn device_create_command_encoder(
    device: core::WGPUDevice,
    descriptor: *const core::WGPUCommandEncoderDescriptor,
) -> core::WGPUCommandEncoder {
    unsafe { ffi_wgpu::wgpuDeviceCreateCommandEncoder(device, descriptor) }
}

unsafe fn buffer_set_label(buffer: core::WGPUBuffer, label: core::WGPUStringView) {
    unsafe { ffi_wgpu::wgpuBufferSetLabel(buffer, label) };
}

unsafe fn buffer_destroy(buffer: core::WGPUBuffer) {
    unsafe { ffi_wgpu::wgpuBufferDestroy(buffer) };
}

unsafe fn buffer_get_mapped_range(
    buffer: core::WGPUBuffer,
    offset: usize,
    size: usize,
) -> *mut c_void {
    unsafe { ffi_wgpu::wgpuBufferGetMappedRange(buffer, offset, size) }
}

unsafe fn buffer_get_const_mapped_range(
    buffer: core::WGPUBuffer,
    offset: usize,
    size: usize,
) -> *const c_void {
    unsafe { ffi_wgpu::wgpuBufferGetConstMappedRange(buffer, offset, size) }
}

unsafe fn buffer_add_ref(buffer: core::WGPUBuffer) {
    unsafe { ffi_wgpu::wgpuBufferAddRef(buffer) };
}

unsafe fn buffer_map_async(
    buffer: core::WGPUBuffer,
    mode: core::WGPUMapMode,
    offset: usize,
    size: usize,
    callback_info: ffi_wgpu::WGPUBufferMapCallbackInfo,
) -> ffi_wgpu::WGPUFuture {
    unsafe { ffi_wgpu::wgpuBufferMapAsync(buffer, mode, offset, size, callback_info) }
}

unsafe fn buffer_unmap(buffer: core::WGPUBuffer) {
    unsafe { ffi_wgpu::wgpuBufferUnmap(buffer) };
}

unsafe fn buffer_release(buffer: core::WGPUBuffer) {
    unsafe { ffi_wgpu::wgpuBufferRelease(buffer) };
}

unsafe fn queue_add_ref(queue: core::WGPUQueue) {
    unsafe { ffi_wgpu::wgpuQueueAddRef(queue) };
}

unsafe fn queue_release(queue: core::WGPUQueue) {
    unsafe { ffi_wgpu::wgpuQueueRelease(queue) };
}

unsafe fn queue_write_buffer(
    queue: core::WGPUQueue,
    buffer: core::WGPUBuffer,
    offset: u64,
    data: *const c_void,
    size: usize,
) {
    unsafe { ffi_wgpu::wgpuQueueWriteBuffer(queue, buffer, offset, data, size) };
}

unsafe fn queue_submit(
    queue: core::WGPUQueue,
    count: usize,
    commands: *const core::WGPUCommandBuffer,
) {
    unsafe { ffi_wgpu::wgpuQueueSubmit(queue, count, commands) };
}

unsafe fn queue_on_submitted_work_done(
    queue: core::WGPUQueue,
    info: core::WGPUQueueWorkDoneCallbackInfo,
) -> core::WGPUFuture {
    unsafe { ffi_wgpu::wgpuQueueOnSubmittedWorkDone(queue, info) }
}

unsafe fn shader_module_add_ref(module: core::WGPUShaderModule) {
    unsafe { ffi_wgpu::wgpuShaderModuleAddRef(module) };
}

unsafe fn shader_module_release(module: core::WGPUShaderModule) {
    unsafe { ffi_wgpu::wgpuShaderModuleRelease(module) };
}

unsafe fn bind_group_layout_add_ref(layout: core::WGPUBindGroupLayout) {
    unsafe { ffi_wgpu::wgpuBindGroupLayoutAddRef(layout) };
}

unsafe fn bind_group_layout_release(layout: core::WGPUBindGroupLayout) {
    unsafe { ffi_wgpu::wgpuBindGroupLayoutRelease(layout) };
}

unsafe fn pipeline_layout_add_ref(layout: core::WGPUPipelineLayout) {
    unsafe { ffi_wgpu::wgpuPipelineLayoutAddRef(layout) };
}

unsafe fn pipeline_layout_release(layout: core::WGPUPipelineLayout) {
    unsafe { ffi_wgpu::wgpuPipelineLayoutRelease(layout) };
}

unsafe fn bind_group_add_ref(bind_group: core::WGPUBindGroup) {
    unsafe { ffi_wgpu::wgpuBindGroupAddRef(bind_group) };
}

unsafe fn bind_group_release(bind_group: core::WGPUBindGroup) {
    unsafe { ffi_wgpu::wgpuBindGroupRelease(bind_group) };
}

unsafe fn compute_pipeline_add_ref(pipeline: core::WGPUComputePipeline) {
    unsafe { ffi_wgpu::wgpuComputePipelineAddRef(pipeline) };
}

unsafe fn compute_pipeline_release(pipeline: core::WGPUComputePipeline) {
    unsafe { ffi_wgpu::wgpuComputePipelineRelease(pipeline) };
}

unsafe fn command_encoder_release(encoder: core::WGPUCommandEncoder) {
    unsafe { ffi_wgpu::wgpuCommandEncoderRelease(encoder) };
}

unsafe fn command_encoder_copy_buffer_to_buffer(
    encoder: core::WGPUCommandEncoder,
    source: core::WGPUBuffer,
    source_offset: u64,
    destination: core::WGPUBuffer,
    destination_offset: u64,
    size: u64,
) {
    unsafe {
        ffi_wgpu::wgpuCommandEncoderCopyBufferToBuffer(
            encoder,
            source,
            source_offset,
            destination,
            destination_offset,
            size,
        )
    };
}

unsafe fn command_encoder_begin_compute_pass(
    encoder: core::WGPUCommandEncoder,
    descriptor: *const core::WGPUComputePassDescriptor,
) -> core::WGPUComputePassEncoder {
    unsafe { ffi_wgpu::wgpuCommandEncoderBeginComputePass(encoder, descriptor) }
}

unsafe fn command_encoder_finish(
    encoder: core::WGPUCommandEncoder,
    descriptor: *const core::WGPUCommandBufferDescriptor,
) -> core::WGPUCommandBuffer {
    unsafe { ffi_wgpu::wgpuCommandEncoderFinish(encoder, descriptor) }
}

unsafe fn command_buffer_release(command_buffer: core::WGPUCommandBuffer) {
    unsafe { ffi_wgpu::wgpuCommandBufferRelease(command_buffer) };
}

unsafe fn compute_pass_encoder_release(pass: core::WGPUComputePassEncoder) {
    unsafe { ffi_wgpu::wgpuComputePassEncoderRelease(pass) };
}

unsafe fn compute_pass_encoder_set_pipeline(
    pass: core::WGPUComputePassEncoder,
    pipeline: core::WGPUComputePipeline,
) {
    unsafe { ffi_wgpu::wgpuComputePassEncoderSetPipeline(pass, pipeline) };
}

unsafe fn compute_pass_encoder_set_bind_group(
    pass: core::WGPUComputePassEncoder,
    index: u32,
    bind_group: core::WGPUBindGroup,
    offset_count: usize,
    offsets: *const u32,
) {
    unsafe {
        ffi_wgpu::wgpuComputePassEncoderSetBindGroup(pass, index, bind_group, offset_count, offsets)
    };
}

unsafe fn compute_pass_encoder_dispatch_workgroups(
    pass: core::WGPUComputePassEncoder,
    x: u32,
    y: u32,
    z: u32,
) {
    unsafe { ffi_wgpu::wgpuComputePassEncoderDispatchWorkgroups(pass, x, y, z) };
}

unsafe fn compute_pass_encoder_end(pass: core::WGPUComputePassEncoder) {
    unsafe { ffi_wgpu::wgpuComputePassEncoderEnd(pass) };
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
    use std::cell::Cell;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{c_int, core, ffi_wgpu as wgpu, qjs, CallbackKind, Context, Engine, Runtime};
    use webgpu_native_js_core::JsEngine;

    static COUNTED_BUFFER_RELEASES: AtomicUsize = AtomicUsize::new(0);

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

    fn global_value(runtime: &Runtime, name: &str) -> qjs::JSValue {
        let name = std::ffi::CString::new(name).expect("global name");
        let global = unsafe { qjs::JS_GetGlobalObject(runtime.raw_context()) };
        let value = unsafe { qjs::JS_GetPropertyStr(runtime.raw_context(), global, name.as_ptr()) };
        unsafe { qjs::JS_FreeValue(runtime.raw_context(), global) };
        assert!(!unsafe { qjs::JS_IsException(value) }, "global lookup");
        value
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

    unsafe fn counted_buffer_release(buffer: core::WGPUBuffer) {
        COUNTED_BUFFER_RELEASES.fetch_add(1, Ordering::SeqCst);
        unsafe { wgpu::wgpuBufferRelease(buffer) };
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
                var bytes = new ArrayBuffer(8);
                var write = new Uint8Array(bytes);
                write.set([3, 1, 4, 1, 5, 9, 2, 6]);
                device.queue.writeBuffer(src, 0, bytes, 0, 8);
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
    fn mapped_at_creation_detaches_on_unmap() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                var mapped = device.createBuffer({ size: 8, usage: 1, mappedAtCreation: true });
                var createdRange = mapped.getMappedRange(0, 4);
                var createdView = new Uint8Array(createdRange);
                createdView[0] = 7;
                mapped.unmap();
                if (createdRange.byteLength !== 0 || createdView.byteLength !== 0) {
                    throw new Error('mappedAtCreation range was not detached');
                }

                "#,
            "mapping.js",
        );
        let _ = runtime.drain_releases().expect("drain detached ranges");
        eval_drop(
            &runtime,
            "mapped = null; createdRange = null; createdView = null;",
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
    /// Documents a known deviation from WebIDL: plain array-like objects are
    /// accepted by today's temporary length/index sequence conversion.
    fn sequence_conversion_array_like_is_known_webidl_deviation() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                // Known deviation: WebIDL sequence<T> is iterator-based and
                // rejects plain array-like objects. Phase 4 codegen will replace
                // today's temporary length/index conversion.
                var deviationBgl = device.createBindGroupLayout({ entries: [] });
                var arrayLikeLayouts = { length: 1, 0: deviationBgl };
                var deviationLayout = device.createPipelineLayout({ bindGroupLayouts: arrayLikeLayouts });
            "#,
            "sequence-array-like-deviation.js",
        );
        eval_drop(
            &runtime,
            "deviationBgl = deviationLayout = null;",
            "sequence-array-like-deviation-cleanup.js",
        );
        runtime.clear_global("device").expect("clear device");
        runtime.run_gc();
        runtime.run_gc();
        let _ = runtime.drain_releases().expect("drain");
    }

    #[test]
    /// Documents a known deviation from WebIDL: iterable sequence values such
    /// as `Set` are rejected until Phase 4 codegen defines iterator conversion.
    fn sequence_conversion_set_rejection_is_known_webidl_deviation() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let wrapped = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        eval_drop(
            &runtime,
            r#"
                // Known deviation: WebIDL sequence<T> accepts iterable objects
                // such as Set. Phase 4 codegen will replace today's temporary
                // length/index conversion.
                var setBgl = device.createBindGroupLayout({ entries: [] });
                var threw = false;
                try {
                    device.createPipelineLayout({ bindGroupLayouts: new Set([setBgl]) });
                } catch (e) {
                    threw = true;
                }
                if (!threw) {
                    throw new Error('Set sequence should be rejected by current deviation');
                }
            "#,
            "sequence-set-deviation.js",
        );
        eval_drop(
            &runtime,
            "setBgl = null;",
            "sequence-set-deviation-cleanup.js",
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
        let gpu = runtime.wrap_gpu(setup.instance).expect("wrap gpu");
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
    fn two_concurrent_request_adapter_promises_settle_independently() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("quickjs runtime");
        let gpu = runtime.wrap_gpu(setup.instance).expect("wrap gpu");
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
