#![warn(missing_docs)]

//! QuickJS adapter for `webgpu-native-js`.

use std::any::Any;
use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
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
    state: Arc<State>,
}

impl Runtime {
    /// Creates a QuickJS runtime configured with the WebGPU binding environment.
    pub fn new() -> Result<Self> {
        let rt = unsafe { qjs::JS_NewRuntime() };
        let rt = NonNull::new(rt).ok_or(Error::Null("JS_NewRuntime"))?;
        let ctx = unsafe { qjs::JS_NewContext(rt.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or(Error::Null("JS_NewContext"))?;
        let state = Arc::new(State::new());
        let raw_state = Arc::into_raw(Arc::clone(&state))
            .cast::<c_void>()
            .cast_mut();
        unsafe {
            qjs::JS_SetRuntimeOpaque(rt.as_ptr(), raw_state);
        }
        Ok(Self { rt, ctx, state })
    }

    /// Returns the raw QuickJS context.
    #[must_use]
    pub fn raw_context(&self) -> *mut qjs::JSContext {
        self.ctx.as_ptr()
    }

    /// Wraps an adopted WebGPU device.
    pub fn wrap_device(&self, device: ffi_wgpu::WGPUDevice) -> Result<qjs::JSValue> {
        let value = core::wrap_device::<Engine>(
            Context {
                ctx: self.raw_context(),
            },
            device,
        )
        .map_err(|value| Error::Exception(exception_or_value(self.raw_context(), value)))?;
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
        self.set_global_value(
            name,
            Engine::undefined(Context {
                ctx: self.raw_context(),
            }),
        )
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
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeContext(self.ctx.as_ptr());
            qjs::JS_FreeRuntime(self.rt.as_ptr());
            let raw = qjs::JS_GetRuntimeOpaque(self.rt.as_ptr()).cast::<State>();
            if !raw.is_null() {
                drop(Arc::from_raw(raw));
            }
        }
    }
}

struct State {
    env: core::Environment,
    classes: Mutex<BTreeMap<core::ClassId, ClassEntry>>,
    quickjs_to_core: Mutex<BTreeMap<qjs::JSClassID, core::ClassId>>,
}

impl State {
    fn new() -> Self {
        Self {
            env: core::Environment::new(gpu_dispatch(), Arc::new(core::ReleaseQueue::new())),
            classes: Mutex::new(BTreeMap::new()),
            quickjs_to_core: Mutex::new(BTreeMap::new()),
        }
    }
}

struct ClassEntry {
    quickjs_id: qjs::JSClassID,
    spec: &'static core::ClassSpec<Engine>,
}

/// QuickJS engine marker type.
pub struct Engine;

/// QuickJS context handle.
#[derive(Clone, Copy)]
pub struct Context {
    ctx: *mut qjs::JSContext,
}

impl core::JsEngine for Engine {
    type Value = qjs::JSValue;
    type Context<'a> = Context;
    type Error = qjs::JSValue;

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
        install_methods(cx.ctx, proto, spec)?;
        install_properties(cx.ctx, proto, spec)?;
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
        Ok(unsafe { qjs::JS_NewFloat64(cx.ctx, value) })
    }

    fn string(cx: Self::Context<'_>, value: &str) -> core::Result<Self::Value, Self::Error> {
        Ok(unsafe { qjs::JS_NewStringLen(cx.ctx, value.as_ptr().cast(), value.len()) })
    }

    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        throw_message(cx.ctx, message, true)
    }

    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        throw_message(cx.ctx, message, false)
    }
}

fn install_methods(
    ctx: *mut qjs::JSContext,
    proto: qjs::JSValue,
    spec: &'static core::ClassSpec<Engine>,
) -> core::Result<(), qjs::JSValue> {
    for (index, method) in spec.methods.iter().enumerate() {
        let Ok(name) = CString::new(method.name) else {
            return Err(Engine::type_error(Context { ctx }, "invalid method name"));
        };
        let func = match (spec.name, method.name) {
            ("GPUDevice", "createBuffer") => unsafe {
                qjs::JS_NewCFunction(ctx, Some(qjs_device_create_buffer), name.as_ptr(), 1)
            },
            ("GPUBuffer", "destroy") => unsafe {
                qjs::JS_NewCFunction(ctx, Some(qjs_buffer_destroy), name.as_ptr(), 0)
            },
            _ => unsafe {
                qjs::JS_NewCFunctionMagic(
                    ctx,
                    Some(qjs_method),
                    name.as_ptr(),
                    i32::from(method.length),
                    qjs::JSCFunctionEnum_JS_CFUNC_generic_magic,
                    magic(spec.id, CallbackKind::Method, index),
                )
            },
        };
        if unsafe { qjs::JS_IsException(func) } {
            return Err(func);
        }
        if unsafe {
            qjs::JS_DefinePropertyValueStr(
                ctx,
                proto,
                name.as_ptr(),
                func,
                (qjs::JS_PROP_CONFIGURABLE | qjs::JS_PROP_WRITABLE) as c_int,
            )
        } < 0
        {
            return Err(take_exception_value(ctx));
        }
    }
    Ok(())
}

fn install_properties(
    ctx: *mut qjs::JSContext,
    proto: qjs::JSValue,
    spec: &'static core::ClassSpec<Engine>,
) -> core::Result<(), qjs::JSValue> {
    for (index, property) in spec.properties.iter().enumerate() {
        let Ok(name) = CString::new(property.name) else {
            return Err(Engine::type_error(Context { ctx }, "invalid property name"));
        };
        let atom = unsafe { qjs::JS_NewAtom(ctx, name.as_ptr()) };
        let getter = if property.get.is_some() {
            match (spec.name, property.name) {
                ("GPUBuffer", "label") => new_getter(ctx, name.as_ptr(), qjs_buffer_label_get, 0),
                ("GPUBuffer", "size") => new_getter(ctx, name.as_ptr(), qjs_buffer_size_get, 0),
                ("GPUBuffer", "usage") => new_getter(ctx, name.as_ptr(), qjs_buffer_usage_get, 0),
                _ => new_getter(
                    ctx,
                    name.as_ptr(),
                    qjs_getter,
                    magic(spec.id, CallbackKind::Getter, index),
                ),
            }
        } else {
            Engine::undefined(Context { ctx })
        };
        let setter = if property.set.is_some() {
            match (spec.name, property.name) {
                ("GPUBuffer", "label") => new_setter(ctx, name.as_ptr(), qjs_buffer_label_set, 0),
                _ => new_setter(
                    ctx,
                    name.as_ptr(),
                    qjs_setter,
                    magic(spec.id, CallbackKind::Setter, index),
                ),
            }
        } else {
            Engine::undefined(Context { ctx })
        };
        if unsafe {
            qjs::JS_DefinePropertyGetSet(
                ctx,
                proto,
                atom,
                getter,
                setter,
                qjs::JS_PROP_CONFIGURABLE as c_int,
            )
        } < 0
        {
            return Err(take_exception_value(ctx));
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

#[derive(Clone, Copy)]
enum CallbackKind {
    Method = 1,
    Getter = 2,
    Setter = 3,
}

fn magic(class: core::ClassId, kind: CallbackKind, index: usize) -> c_int {
    ((class.0 as c_int) << 16) | ((kind as c_int) << 12) | (index as c_int)
}

fn decode_magic(value: c_int) -> (core::ClassId, CallbackKind, usize) {
    let class = core::ClassId(((value >> 16) & 0xffff) as u32);
    let kind = match (value >> 12) & 0xf {
        2 => CallbackKind::Getter,
        3 => CallbackKind::Setter,
        _ => CallbackKind::Method,
    };
    (class, kind, (value & 0xfff) as usize)
}

unsafe extern "C" fn qjs_method(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    argc: c_int,
    argv: *mut qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        let (class, _, index) = decode_magic(magic_value);
        let state = state_from_context(ctx);
        let classes = state
            .classes
            .lock()
            .map_err(|_| Engine::operation_error(Context { ctx }, "class registry is poisoned"))?;
        let Some(method) = classes
            .get(&class)
            .and_then(|entry| entry.spec.methods.get(index))
        else {
            return Err(Engine::operation_error(
                Context { ctx },
                &format!("method is not registered: class={} index={index}", class.0),
            ));
        };
        let args = if argc <= 0 || argv.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        };
        (method.call)(Context { ctx }, this_val, args)
    })
}

unsafe extern "C" fn qjs_device_create_buffer(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    argc: c_int,
    argv: *mut qjs::JSValue,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        let args = if argc <= 0 || argv.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        };
        core::device_create_buffer::<Engine>(Context { ctx }, this_val, args)
    })
}

unsafe extern "C" fn qjs_buffer_destroy(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    _argc: c_int,
    _argv: *mut qjs::JSValue,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        core::buffer_destroy::<Engine>(Context { ctx }, this_val, &[])
    })
}

unsafe extern "C" fn qjs_buffer_label_get(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    _magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        core::buffer_label_get::<Engine>(Context { ctx }, this_val)
    })
}

unsafe extern "C" fn qjs_buffer_size_get(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    _magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        core::buffer_size_get::<Engine>(Context { ctx }, this_val)
    })
}

unsafe extern "C" fn qjs_buffer_usage_get(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    _magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        core::buffer_usage_get::<Engine>(Context { ctx }, this_val)
    })
}

unsafe extern "C" fn qjs_buffer_label_set(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    value: qjs::JSValue,
    _magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        core::buffer_label_set::<Engine>(Context { ctx }, this_val, value)?;
        Ok(Engine::undefined(Context { ctx }))
    })
}

unsafe extern "C" fn qjs_getter(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        let (class, _, index) = decode_magic(magic_value);
        let state = state_from_context(ctx);
        let classes = state
            .classes
            .lock()
            .map_err(|_| Engine::operation_error(Context { ctx }, "class registry is poisoned"))?;
        let Some(getter) = classes
            .get(&class)
            .and_then(|entry| entry.spec.properties.get(index))
            .and_then(|property| property.get)
        else {
            return Err(Engine::operation_error(
                Context { ctx },
                "getter is not registered",
            ));
        };
        getter(Context { ctx }, this_val)
    })
}

unsafe extern "C" fn qjs_setter(
    ctx: *mut qjs::JSContext,
    this_val: qjs::JSValue,
    value: qjs::JSValue,
    magic_value: c_int,
) -> qjs::JSValue {
    catch_callback(ctx, || {
        let (class, _, index) = decode_magic(magic_value);
        let state = state_from_context(ctx);
        let classes = state
            .classes
            .lock()
            .map_err(|_| Engine::operation_error(Context { ctx }, "class registry is poisoned"))?;
        let Some(setter) = classes
            .get(&class)
            .and_then(|entry| entry.spec.properties.get(index))
            .and_then(|property| property.set)
        else {
            return Err(Engine::operation_error(
                Context { ctx },
                "setter is not registered",
            ));
        };
        setter(Context { ctx }, this_val, value)?;
        Ok(Engine::undefined(Context { ctx }))
    })
}

fn catch_callback<F>(ctx: *mut qjs::JSContext, f: F) -> qjs::JSValue
where
    F: FnOnce() -> core::Result<qjs::JSValue, qjs::JSValue>,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(Ok(value)) => value,
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
        (spec.finalizer)(*payload, &state.env);
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
        device_add_ref,
        device_release,
        device_create_buffer,
        buffer_set_label,
        buffer_destroy,
        buffer_release,
    }
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

unsafe fn buffer_release(buffer: core::WGPUBuffer) {
    unsafe { ffi_wgpu::wgpuBufferRelease(buffer) };
}

#[cfg(test)]
mod tests {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::{ffi_wgpu as wgpu, Runtime};

    struct RequestState {
        status: AtomicUsize,
        handle: AtomicUsize,
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
            let state = unsafe { Arc::from_raw(userdata1.cast::<RequestState>()) };
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
            let state = unsafe { Arc::from_raw(userdata1.cast::<RequestState>()) };
            state.status.store(status as usize, Ordering::SeqCst);
            state.handle.store(device as usize, Ordering::SeqCst);
        }));
    }

    #[test]
    fn script_runs_buffer_vertical_slice() {
        let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        assert!(!instance.is_null());

        let adapter_state = Arc::new(RequestState::new());
        let adapter_callback_state = Arc::into_raw(Arc::clone(&adapter_state)).cast_mut().cast();
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

        let device_state = Arc::new(RequestState::new());
        let device_callback_state = Arc::into_raw(Arc::clone(&device_state)).cast_mut().cast();
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

        let runtime = Runtime::new().expect("quickjs runtime");
        runtime.eval("var smoke = 1;", "smoke.js").expect("smoke");
        let wrapped = runtime.wrap_device(device).expect("wrap device");
        runtime
            .set_global_value("device", wrapped)
            .expect("set device");
        runtime
            .eval(
                include_str!("../tests/scripts/buffer_slice.js"),
                "buffer_slice.js",
            )
            .expect("script");
        runtime.clear_global("device").expect("clear device");
        assert!(runtime.drain_releases().expect("drain") >= 2);

        unsafe {
            wgpu::wgpuDeviceRelease(device);
            wgpu::wgpuAdapterRelease(adapter);
            wgpu::wgpuInstanceRelease(instance);
        }
    }
}
