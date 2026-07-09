#![warn(missing_docs)]

//! QuickJS adapter for `webgpu-native-js`.

use std::any::Any;
use std::cell::RefCell;
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
        let rt = unsafe { qjs::JS_NewRuntime() };
        let rt = NonNull::new(rt).ok_or(Error::Null("JS_NewRuntime"))?;
        let ctx = unsafe { qjs::JS_NewContext(rt.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or(Error::Null("JS_NewContext"))?;
        let state = Rc::new(State::new());
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
    /// `device` must be a valid live `WGPUDevice` handle for this backend.
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
    /// # Safety
    ///
    /// `instance` must be a valid live `WGPUInstance` handle for this backend.
    pub unsafe fn tick(&self, instance: ffi_wgpu::WGPUInstance) -> Result<()> {
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
            .map_err(|_| Error::Exception("release queue is poisoned".to_owned()))?;
        Ok(())
    }

    /// Pumps only WebGPU callbacks, for event-loop regression tests.
    ///
    /// # Safety
    ///
    /// `instance` must be a valid live `WGPUInstance` handle for this backend.
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
    unhandled_rejections: Mutex<Vec<UnhandledRejection>>,
}

impl State {
    fn new() -> Self {
        Self {
            env: core::Environment::new(gpu_dispatch(), Arc::new(core::ReleaseQueue::new())),
            classes: Mutex::new(BTreeMap::new()),
            quickjs_to_core: Mutex::new(BTreeMap::new()),
            callbacks: Mutex::new(Vec::new()),
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
            gc_mark: None,
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
        let holder = Box::new(payload);
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
        NonNull::new(raw.cast::<Box<dyn Any + Send>>()).map(|ptr| unsafe { ptr.as_ref().as_ref() })
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
        unsafe { qjs::JS_DetachArrayBuffer(cx.ctx, value) };
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

    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value {
        unsafe { qjs::JS_DupValue(cx.ctx, value) }
    }

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        unsafe { qjs::JS_FreeValue(cx.ctx, value) };
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
        let core_id = {
            let Ok(map) = state.quickjs_to_core.lock() else {
                return;
            };
            let Some(core_id) = map.get(&quickjs_id).copied() else {
                return;
            };
            core_id
        };
        let spec = {
            let Ok(classes) = state.classes.lock() else {
                return;
            };
            let Some(entry) = classes.get(&core_id) else {
                return;
            };
            entry.spec
        };
        let raw = unsafe { qjs::JS_GetOpaque(value, quickjs_id) };
        let Some(raw) = NonNull::new(raw.cast::<Box<dyn Any + Send>>()) else {
            return;
        };
        let payload = unsafe { Box::from_raw(raw.as_ptr()) };
        if let Some(buffer) = payload.downcast_ref::<core::BufferPayload<Engine>>() {
            buffer.release_mapped_range_values(|value| unsafe {
                qjs::JS_FreeValueRT(rt, value);
            });
        }
        (spec.finalizer)(*payload, &state.env);
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
        buffer_set_label,
        buffer_destroy,
        buffer_get_mapped_range,
        buffer_add_ref,
        buffer_map_async,
        buffer_unmap,
        buffer_release,
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

unsafe extern "C" fn arraybuffer_free(
    _rt: *mut qjs::JSRuntime,
    opaque: *mut c_void,
    ptr: *mut c_void,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !ptr.is_null() {
            return;
        }
        let Some(owner) = NonNull::new(opaque.cast::<ArrayBufferOwner>()) else {
            return;
        };
        let owner = unsafe { Box::from_raw(owner.as_ptr()) };
        let _ = owner.queue.enqueue(core::ReleaseRequest::Buffer {
            buffer: owner.buffer,
            gpu: owner.gpu,
        });
    }));
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{c_int, core, ffi_wgpu as wgpu, qjs, CallbackKind, Context, Engine, Runtime};
    use webgpu_native_js_core::JsEngine;

    struct RequestState {
        status: AtomicUsize,
        handle: AtomicUsize,
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
        let adapter_state = Rc::new(RequestState::new());
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
            adapter_state.status.load(Ordering::SeqCst) as wgpu::WGPURequestAdapterStatus,
            wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success
        );
        let adapter = adapter_state.handle.load(Ordering::SeqCst) as wgpu::WGPUAdapter;
        assert!(!adapter.is_null());

        // AllowProcessEvents runs callbacks on the thread that calls
        // wgpuInstanceProcessEvents, so the userdata clone is single-threaded.
        let device_state = Rc::new(RequestState::new());
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
            device_state.status.load(Ordering::SeqCst) as wgpu::WGPURequestDeviceStatus,
            wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success
        );
        let device = device_state.handle.load(Ordering::SeqCst) as wgpu::WGPUDevice;
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

    impl RequestState {
        fn new() -> Self {
            Self {
                status: AtomicUsize::new(0),
                handle: AtomicUsize::new(0),
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
            let state = unsafe { Rc::from_raw(userdata1.cast::<RequestState>()) };
            state.status.store(status as usize, Ordering::SeqCst);
            state.handle.store(adapter as usize, Ordering::SeqCst);
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
            let state = unsafe { Rc::from_raw(userdata1.cast::<RequestState>()) };
            state.status.store(status as usize, Ordering::SeqCst);
            state.handle.store(device as usize, Ordering::SeqCst);
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
        core::GpuDispatch {
            instance_request_adapter: teardown_request_adapter,
            adapter_request_device: teardown_request_device,
            adapter_release: teardown_adapter_release,
            device_add_ref: teardown_device_noop,
            device_release: teardown_device_noop,
            device_create_buffer: teardown_create_buffer,
            buffer_set_label: teardown_set_label,
            buffer_destroy: teardown_buffer_noop,
            buffer_get_mapped_range: teardown_get_mapped_range,
            buffer_add_ref: teardown_buffer_noop,
            buffer_map_async: teardown_map_async,
            buffer_unmap: teardown_buffer_noop,
            buffer_release: teardown_buffer_release,
        }
    }

    unsafe fn teardown_request_adapter(
        _instance: wgpu::WGPUInstance,
        _options: *const wgpu::WGPURequestAdapterOptions,
        _info: wgpu::WGPURequestAdapterCallbackInfo,
    ) -> wgpu::WGPUFuture {
        wgpu::WGPUFuture { id: 0 }
    }

    unsafe fn teardown_request_device(
        _adapter: core::WGPUAdapter,
        _descriptor: *const wgpu::WGPUDeviceDescriptor,
        _info: wgpu::WGPURequestDeviceCallbackInfo,
    ) -> wgpu::WGPUFuture {
        wgpu::WGPUFuture { id: 0 }
    }

    unsafe fn teardown_adapter_release(_adapter: core::WGPUAdapter) {}
    unsafe fn teardown_device_noop(_device: core::WGPUDevice) {}
    unsafe fn teardown_create_buffer(
        _device: core::WGPUDevice,
        _descriptor: *const core::WGPUBufferDescriptor,
    ) -> core::WGPUBuffer {
        ptr::null_mut()
    }
    unsafe fn teardown_set_label(_buffer: core::WGPUBuffer, _label: core::WGPUStringView) {}
    unsafe fn teardown_buffer_noop(_buffer: core::WGPUBuffer) {}
    unsafe fn teardown_get_mapped_range(
        _buffer: core::WGPUBuffer,
        _offset: usize,
        _size: usize,
    ) -> *mut std::ffi::c_void {
        ptr::null_mut()
    }
    unsafe fn teardown_map_async(
        _buffer: core::WGPUBuffer,
        _mode: core::WGPUMapMode,
        _offset: usize,
        _size: usize,
        _info: wgpu::WGPUBufferMapCallbackInfo,
    ) -> wgpu::WGPUFuture {
        wgpu::WGPUFuture { id: 0 }
    }
    unsafe fn teardown_buffer_release(_buffer: core::WGPUBuffer) {
        TEARDOWN_BUFFER_RELEASES.fetch_add(1, Ordering::SeqCst);
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
                var mapped = device.createBuffer({ size: 8, usage: 2, mappedAtCreation: true });
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
}
