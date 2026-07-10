use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use webgpu_native_js_core as core;
use webgpu_native_js_core::JsEngine;
use webgpu_native_js_ffi::native as ffi_wgpu;

/// Opaque JavaScriptCore context storage.
pub enum OpaqueJsContext {}
/// Opaque JavaScriptCore value storage.
pub enum OpaqueJsValue {}
/// Opaque JavaScriptCore string storage.
pub enum OpaqueJsString {}
/// Opaque JavaScriptCore class storage.
pub enum OpaqueJsClass {}

/// JavaScriptCore execution-context handle.
pub type JSContextRef = *mut OpaqueJsContext;
/// JavaScriptCore global-context handle.
pub type JSGlobalContextRef = *mut OpaqueJsContext;
/// JavaScriptCore value handle.
pub type JSValueRef = *const OpaqueJsValue;
/// JavaScriptCore object handle.
pub type JSObjectRef = *mut OpaqueJsValue;
/// JavaScriptCore string handle.
pub type JSStringRef = *mut OpaqueJsString;
/// JavaScriptCore class handle.
pub type JSClassRef = *mut OpaqueJsClass;

type JSPropertyAttributes = u32;
type JSClassAttributes = u32;
type InitializeCallback = unsafe extern "C" fn(JSContextRef, JSObjectRef);
type FinalizeCallback = unsafe extern "C" fn(JSObjectRef);
type HasPropertyCallback = unsafe extern "C" fn(JSContextRef, JSObjectRef, JSStringRef) -> bool;
type GetPropertyCallback =
    unsafe extern "C" fn(JSContextRef, JSObjectRef, JSStringRef, *mut JSValueRef) -> JSValueRef;
type SetPropertyCallback = unsafe extern "C" fn(
    JSContextRef,
    JSObjectRef,
    JSStringRef,
    JSValueRef,
    *mut JSValueRef,
) -> bool;
type DeletePropertyCallback =
    unsafe extern "C" fn(JSContextRef, JSObjectRef, JSStringRef, *mut JSValueRef) -> bool;
type GetPropertyNamesCallback = unsafe extern "C" fn(JSContextRef, JSObjectRef, *mut c_void);
type CallAsFunctionCallback = unsafe extern "C" fn(
    JSContextRef,
    JSObjectRef,
    JSObjectRef,
    usize,
    *const JSValueRef,
    *mut JSValueRef,
) -> JSValueRef;
type CallAsConstructorCallback = unsafe extern "C" fn(
    JSContextRef,
    JSObjectRef,
    usize,
    *const JSValueRef,
    *mut JSValueRef,
) -> JSObjectRef;
type HasInstanceCallback =
    unsafe extern "C" fn(JSContextRef, JSObjectRef, JSValueRef, *mut JSValueRef) -> bool;
type ConvertToTypeCallback =
    unsafe extern "C" fn(JSContextRef, JSObjectRef, c_int, *mut JSValueRef) -> JSValueRef;

#[repr(C)]
struct JSClassDefinition {
    version: c_int,
    attributes: JSClassAttributes,
    class_name: *const c_char,
    parent_class: JSClassRef,
    static_values: *const c_void,
    static_functions: *const c_void,
    initialize: Option<InitializeCallback>,
    finalize: Option<FinalizeCallback>,
    has_property: Option<HasPropertyCallback>,
    get_property: Option<GetPropertyCallback>,
    set_property: Option<SetPropertyCallback>,
    delete_property: Option<DeletePropertyCallback>,
    get_property_names: Option<GetPropertyNamesCallback>,
    call_as_function: Option<CallAsFunctionCallback>,
    call_as_constructor: Option<CallAsConstructorCallback>,
    has_instance: Option<HasInstanceCallback>,
    convert_to_type: Option<ConvertToTypeCallback>,
}

impl JSClassDefinition {
    const fn empty() -> Self {
        Self {
            version: 0,
            attributes: 0,
            class_name: ptr::null(),
            parent_class: ptr::null_mut(),
            static_values: ptr::null(),
            static_functions: ptr::null(),
            initialize: None,
            finalize: None,
            has_property: None,
            get_property: None,
            set_property: None,
            delete_property: None,
            get_property_names: None,
            call_as_function: None,
            call_as_constructor: None,
            has_instance: None,
            convert_to_type: None,
        }
    }
}

const PROPERTY_NONE: JSPropertyAttributes = 0;

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    /// Creates a global JavaScript context with the supplied global-object class.
    fn JSGlobalContextCreate(global_object_class: JSClassRef) -> JSGlobalContextRef;
    /// Releases a global JavaScript context.
    fn JSGlobalContextRelease(ctx: JSGlobalContextRef);
    /// Returns a context's global object.
    fn JSContextGetGlobalObject(ctx: JSContextRef) -> JSObjectRef;
    /// Creates a JavaScript string by copying a nul-terminated UTF-8 string.
    fn JSStringCreateWithUTF8CString(string: *const c_char) -> JSStringRef;
    /// Returns the maximum UTF-8 buffer size needed for a JavaScript string.
    fn JSStringGetMaximumUTF8CStringSize(string: JSStringRef) -> usize;
    /// Copies a JavaScript string to a nul-terminated UTF-8 buffer.
    fn JSStringGetUTF8CString(
        string: JSStringRef,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> usize;
    /// Releases a JavaScript string.
    fn JSStringRelease(string: JSStringRef);
    /// Evaluates a JavaScript source string.
    fn JSEvaluateScript(
        ctx: JSContextRef,
        script: JSStringRef,
        this_object: JSObjectRef,
        source_url: JSStringRef,
        starting_line_number: c_int,
        exception: *mut JSValueRef,
    ) -> JSValueRef;
    /// Tests whether a value is JavaScript `undefined`.
    fn JSValueIsUndefined(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Tests whether a value is JavaScript `null`.
    fn JSValueIsNull(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Tests whether a value is a JavaScript BigInt.
    fn JSValueIsBigInt(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Converts a value with JavaScript `ToBoolean`.
    fn JSValueToBoolean(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Converts a value with JavaScript `ToNumber`.
    fn JSValueToNumber(ctx: JSContextRef, value: JSValueRef, exception: *mut JSValueRef) -> f64;
    /// Copies a value's JavaScript string conversion.
    fn JSValueToStringCopy(
        ctx: JSContextRef,
        value: JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSStringRef;
    /// Converts a value to an object.
    fn JSValueToObject(
        ctx: JSContextRef,
        value: JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Protects a value from garbage collection.
    fn JSValueProtect(ctx: JSContextRef, value: JSValueRef);
    /// Removes one garbage-collection protection from a value.
    fn JSValueUnprotect(ctx: JSContextRef, value: JSValueRef);
    /// Creates JavaScript `undefined`.
    fn JSValueMakeUndefined(ctx: JSContextRef) -> JSValueRef;
    /// Creates a JavaScript number.
    fn JSValueMakeNumber(ctx: JSContextRef, number: f64) -> JSValueRef;
    /// Creates a JavaScript string value.
    fn JSValueMakeString(ctx: JSContextRef, string: JSStringRef) -> JSValueRef;
    /// Creates a JavaScript class from a class definition.
    fn JSClassCreate(definition: *const JSClassDefinition) -> JSClassRef;
    /// Releases a JavaScript class.
    fn JSClassRelease(class: JSClassRef);
    /// Creates an object with a class and private data.
    fn JSObjectMake(ctx: JSContextRef, class: JSClassRef, data: *mut c_void) -> JSObjectRef;
    /// Creates a JavaScript array from argument values.
    fn JSObjectMakeArray(
        ctx: JSContextRef,
        argument_count: usize,
        arguments: *const JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Creates a JavaScript Error object from argument values.
    fn JSObjectMakeError(
        ctx: JSContextRef,
        argument_count: usize,
        arguments: *const JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Creates a promise and returns its resolving functions.
    fn JSObjectMakeDeferredPromise(
        ctx: JSContextRef,
        resolve: *mut JSObjectRef,
        reject: *mut JSObjectRef,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Calls a JavaScript object as a constructor.
    fn JSObjectCallAsConstructor(
        ctx: JSContextRef,
        object: JSObjectRef,
        argument_count: usize,
        arguments: *const JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Gets a named object property.
    fn JSObjectGetProperty(
        ctx: JSContextRef,
        object: JSObjectRef,
        property_name: JSStringRef,
        exception: *mut JSValueRef,
    ) -> JSValueRef;
    /// Gets an object property using a JavaScript property key.
    fn JSObjectGetPropertyForKey(
        ctx: JSContextRef,
        object: JSObjectRef,
        property_key: JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSValueRef;
    /// Sets a named object property.
    fn JSObjectSetProperty(
        ctx: JSContextRef,
        object: JSObjectRef,
        property_name: JSStringRef,
        value: JSValueRef,
        attributes: JSPropertyAttributes,
        exception: *mut JSValueRef,
    );
    /// Calls a JavaScript object as a function.
    fn JSObjectCallAsFunction(
        ctx: JSContextRef,
        object: JSObjectRef,
        this_object: JSObjectRef,
        argument_count: usize,
        arguments: *const JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSValueRef;
    /// Returns an object's private pointer.
    fn JSObjectGetPrivate(object: JSObjectRef) -> *mut c_void;
    /// Replaces an object's private pointer.
    fn JSObjectSetPrivate(object: JSObjectRef, data: *mut c_void) -> bool;
}

/// Adapter result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the JavaScriptCore adapter host API.
#[derive(Debug)]
pub enum Error {
    /// A C API returned null where a live handle was required.
    Null(&'static str),
    /// JavaScriptCore raised an exception.
    Exception(String),
    /// A Rust string contained an interior nul byte.
    Nul(std::ffi::NulError),
}

impl From<std::ffi::NulError> for Error {
    fn from(error: std::ffi::NulError) -> Self {
        Self::Nul(error)
    }
}

struct JsString(NonNull<OpaqueJsString>);

impl JsString {
    fn new(value: &str) -> Result<Self> {
        let value = CString::new(value)?;
        // SAFETY: JavaScriptCore copies the nul-terminated UTF-8 input.
        let raw = unsafe { JSStringCreateWithUTF8CString(value.as_ptr()) };
        NonNull::new(raw)
            .map(Self)
            .ok_or(Error::Null("JSStringCreateWithUTF8CString"))
    }

    fn as_raw(&self) -> JSStringRef {
        self.0.as_ptr()
    }

    fn to_string_lossy(&self) -> String {
        // SAFETY: the string is live for this method.
        let size = unsafe { JSStringGetMaximumUTF8CStringSize(self.as_raw()) };
        let mut bytes = vec![0_u8; size];
        // SAFETY: the destination has the exact maximum capacity JSC requested.
        let written = unsafe {
            JSStringGetUTF8CString(self.as_raw(), bytes.as_mut_ptr().cast(), bytes.len())
        };
        if written == 0 {
            String::new()
        } else {
            String::from_utf8_lossy(&bytes[..written.saturating_sub(1)]).into_owned()
        }
    }
}

impl Drop for JsString {
    fn drop(&mut self) {
        // SAFETY: this is the single release paired with the create call.
        unsafe { JSStringRelease(self.as_raw()) };
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ProtectedValue(JSValueRef);

// SAFETY: the opaque value is moved between the engine thread and a JSC
// finalizer thread only as an address-sized token. It is never dereferenced or
// passed to a context-taking JSC function off the engine/tick thread.
unsafe impl Send for ProtectedValue {}

#[cfg(test)]
impl ProtectedValue {
    fn defer_into(self, state: &FinalizerState) {
        state.defer_unprotect(self.0);
    }
}

struct FinalizerState {
    env: core::Environment,
    deferred_unprotects: Mutex<Vec<ProtectedValue>>,
    protected: Mutex<Vec<ProtectedValue>>,
    tearing_down: AtomicBool,
    protect_count: AtomicUsize,
    unprotect_count: AtomicUsize,
}

impl FinalizerState {
    fn new(gpu: core::GpuDispatch) -> Self {
        Self {
            env: core::Environment::new(gpu, Arc::new(core::ReleaseQueue::new())),
            deferred_unprotects: Mutex::new(Vec::new()),
            protected: Mutex::new(Vec::new()),
            tearing_down: AtomicBool::new(false),
            protect_count: AtomicUsize::new(0),
            unprotect_count: AtomicUsize::new(0),
        }
    }

    fn protect(&self, ctx: JSContextRef, value: JSValueRef) {
        // SAFETY: called only on the live context's engine thread.
        unsafe { JSValueProtect(ctx, value) };
        self.protect_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut protected) = self.protected.lock() {
            protected.push(ProtectedValue(value));
        }
    }

    fn unprotect(&self, ctx: JSContextRef, value: JSValueRef) {
        let removed = self.protected.lock().ok().and_then(|mut protected| {
            let index = protected
                .iter()
                .position(|candidate| candidate.0 == value)?;
            Some(protected.swap_remove(index))
        });
        if removed.is_some() {
            // SAFETY: called only on the live context's engine thread.
            unsafe { JSValueUnprotect(ctx, value) };
            self.unprotect_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn defer_unprotect(&self, value: JSValueRef) {
        if self.tearing_down.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut deferred) = self.deferred_unprotects.lock() {
            deferred.push(ProtectedValue(value));
        }
    }

    fn drain_deferred_unprotects(&self, ctx: JSContextRef) -> usize {
        let values = self
            .deferred_unprotects
            .lock()
            .map(|mut values| std::mem::take(&mut *values))
            .unwrap_or_default();
        let count = values.len();
        for value in values {
            self.unprotect(ctx, value.0);
        }
        count
    }

    fn begin_teardown(&self, ctx: JSContextRef) {
        self.tearing_down.store(true, Ordering::Release);
        let _ = self.drain_deferred_unprotects(ctx);
        let values = self
            .protected
            .lock()
            .map(|mut values| std::mem::take(&mut *values))
            .unwrap_or_default();
        for value in values {
            // SAFETY: teardown runs on the engine thread before context release.
            unsafe { JSValueUnprotect(ctx, value.0) };
            self.unprotect_count.fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct ClassEntry {
    class: JSClassRef,
    spec: &'static core::ClassSpec<Engine>,
    _name: CString,
}

struct ObjectPayload {
    spec: &'static core::ClassSpec<Engine>,
    payload: Box<dyn Any + Send>,
    finalizer: Arc<FinalizerState>,
}

#[derive(Clone, Copy)]
struct MethodTarget {
    call: core::MethodFn<Engine>,
}

/// JavaScriptCore adapter state shared by callbacks for one global context.
pub struct State {
    finalizer: Arc<FinalizerState>,
    classes: Mutex<BTreeMap<core::ClassId, ClassEntry>>,
    method_class: JSClassRef,
    outstanding_deferreds: Arc<Mutex<Vec<DeferredSlot>>>,
    settle_trampoline: Mutex<Option<JSValueRef>>,
}

impl State {
    fn new(gpu: core::GpuDispatch) -> Result<Self> {
        let mut definition = JSClassDefinition::empty();
        definition.class_name = c"webgpuNativeMethod".as_ptr();
        definition.finalize = Some(method_finalize);
        definition.call_as_function = Some(method_call);
        // SAFETY: the definition has the exact SDK layout and all pointers are
        // either static or null. JSClassCreate copies the definition.
        let method_class = unsafe { JSClassCreate(&definition) };
        if method_class.is_null() {
            return Err(Error::Null("JSClassCreate(method)"));
        }
        Ok(Self {
            finalizer: Arc::new(FinalizerState::new(gpu)),
            classes: Mutex::new(BTreeMap::new()),
            method_class,
            outstanding_deferreds: Arc::new(Mutex::new(Vec::new())),
            settle_trampoline: Mutex::new(None),
        })
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

    fn release_outstanding_deferreds(&self, ctx: JSContextRef) {
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
            self.finalizer.unprotect(ctx, deferred.resolve());
            self.finalizer.unprotect(ctx, deferred.reject());
        }
    }

    fn set_trampoline(&self, value: JSValueRef) {
        if let Ok(mut trampoline) = self.settle_trampoline.lock() {
            *trampoline = Some(value);
        }
    }

    fn trampoline(&self) -> Option<JSValueRef> {
        self.settle_trampoline
            .lock()
            .ok()
            .and_then(|trampoline| *trampoline)
    }

    fn take_trampoline(&self) -> Option<JSValueRef> {
        self.settle_trampoline
            .lock()
            .ok()
            .and_then(|mut trampoline| trampoline.take())
    }
}

#[derive(Clone, Copy)]
struct DeferredSlot(NonNull<Option<core::Deferred<Engine>>>);

// SAFETY: the pointer names a slot in an async request Box that remains alive
// until its AllowProcessEvents callback reclaims it. Teardown and callback
// access are confined to the engine thread; only the pointer token is moved.
unsafe impl Send for DeferredSlot {}

/// Registration guard for a deferred slot owned by a pending WebGPU callback.
pub struct DeferredRegistration {
    slots: Arc<Mutex<Vec<DeferredSlot>>>,
    slot: DeferredSlot,
}

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

/// JavaScriptCore engine marker type.
pub struct Engine;

/// Per-call JavaScriptCore context.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    ctx: JSContextRef,
    scope: &'a Scope,
}

/// Per-call value scope.
///
/// JavaScriptCore values are garbage-collected, so dropping this recorder does
/// not release them. The non-optional scope remains part of `Context` to keep
/// the engine boundary's value-ownership obligation explicit (J7/R22).
pub struct Scope {
    values: RefCell<Vec<JSValueRef>>,
}

impl Scope {
    fn new() -> Self {
        Self {
            values: RefCell::new(Vec::new()),
        }
    }

    fn track(&self, value: JSValueRef) {
        self.values.borrow_mut().push(value);
    }

    fn escape(&self, value: JSValueRef) {
        let mut values = self.values.borrow_mut();
        if let Some(index) = values.iter().position(|candidate| *candidate == value) {
            values.swap_remove(index);
        }
    }
}

fn with_scope<R>(ctx: JSContextRef, f: impl FnOnce(Context<'_>) -> R) -> R {
    let scope = Scope::new();
    f(Context { ctx, scope: &scope })
}

/// A JavaScriptCore global context configured for WebGPU bindings.
pub struct Runtime {
    ctx: NonNull<OpaqueJsContext>,
    global_class: NonNull<OpaqueJsClass>,
    state: Box<State>,
}

impl Runtime {
    /// Creates a JavaScriptCore runtime configured with the WebGPU environment.
    pub fn new() -> Result<Self> {
        Self::new_with_dispatch(gpu_dispatch())
    }

    fn new_with_dispatch(gpu: core::GpuDispatch) -> Result<Self> {
        let mut global_definition = JSClassDefinition::empty();
        global_definition.class_name = c"webgpuNativeGlobal".as_ptr();
        // SAFETY: the class definition contains only static or null pointers.
        let global_class = unsafe { JSClassCreate(&global_definition) };
        let global_class =
            NonNull::new(global_class).ok_or(Error::Null("JSClassCreate(global)"))?;
        // SAFETY: global_class is a live class created immediately above.
        let ctx = unsafe { JSGlobalContextCreate(global_class.as_ptr()) };
        let Some(ctx) = NonNull::new(ctx) else {
            // SAFETY: paired release for the successfully created class.
            unsafe { JSClassRelease(global_class.as_ptr()) };
            return Err(Error::Null("JSGlobalContextCreate"));
        };
        let state = match State::new(gpu) {
            Ok(state) => Box::new(state),
            Err(error) => {
                // SAFETY: both handles are live and no state was installed.
                unsafe {
                    JSGlobalContextRelease(ctx.as_ptr());
                    JSClassRelease(global_class.as_ptr());
                }
                return Err(error);
            }
        };
        let global = unsafe { JSContextGetGlobalObject(ctx.as_ptr()) };
        let state_ptr = (&*state as *const State).cast_mut().cast();
        // SAFETY: the global object was created with a non-null class and the
        // boxed State remains address-stable until after context release.
        if !unsafe { JSObjectSetPrivate(global, state_ptr) } {
            // SAFETY: handles are live and state has not escaped Rust ownership.
            unsafe {
                JSGlobalContextRelease(ctx.as_ptr());
                JSClassRelease(state.method_class);
                JSClassRelease(global_class.as_ptr());
            }
            return Err(Error::Null("JSObjectSetPrivate(global)"));
        }
        let runtime = Self {
            ctx,
            global_class,
            state,
        };
        let trampoline = runtime.eval_unprotected(
            "(function(fns, values) { for (let i = 0; i < fns.length; i++) fns[i](values[i]); })",
            "webgpu-native-js-settle-trampoline.js",
        )?;
        runtime
            .state
            .finalizer
            .protect(runtime.raw_context(), trampoline);
        runtime.state.set_trampoline(trampoline);
        Ok(runtime)
    }

    /// Returns the raw JavaScriptCore context.
    #[must_use]
    pub fn raw_context(&self) -> JSContextRef {
        self.ctx.as_ptr()
    }

    /// Wraps an adopted WebGPU device.
    ///
    /// # Safety
    ///
    /// `device` must be a live non-null handle from this runtime's backend.
    /// Core takes its own native reference during this call.
    pub unsafe fn wrap_device(&self, device: ffi_wgpu::WGPUDevice) -> Result<JSValueRef> {
        let value = with_scope(self.raw_context(), |cx| unsafe {
            core::wrap_device::<Engine>(cx, device)
        })
        .map_err(|error| Error::Exception(value_to_string(self.raw_context(), error)))?;
        self.state.finalizer.protect(self.raw_context(), value);
        Ok(value)
    }

    /// Wraps a WebGPU instance as a JavaScript `GPU` object.
    pub fn wrap_gpu(&self, instance: ffi_wgpu::WGPUInstance) -> Result<JSValueRef> {
        let value = with_scope(self.raw_context(), |cx| {
            core::wrap_gpu::<Engine>(cx, instance)
        })
        .map_err(|error| Error::Exception(value_to_string(self.raw_context(), error)))?;
        self.state.finalizer.protect(self.raw_context(), value);
        Ok(value)
    }

    /// Sets a global property and adopts any adapter protection on `value`.
    // SAFETY: `JSValueRef` is the engine's required associated value type, and
    // this API mirrors the safe engine-handle surface used by core. Values
    // accepted here must originate from this Runtime; JSC treats the opaque
    // token as a handle, while Rust cannot express that provenance in the raw
    // C typedef. The lint cannot see this engine-level invariant.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_global_value(&self, name: &str, value: JSValueRef) -> Result<()> {
        let name = JsString::new(name)?;
        let global = unsafe { JSContextGetGlobalObject(self.raw_context()) };
        let mut exception = ptr::null();
        // SAFETY: all handles belong to this live context.
        unsafe {
            JSObjectSetProperty(
                self.raw_context(),
                global,
                name.as_raw(),
                value,
                PROPERTY_NONE,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(Error::Exception(value_to_string(
                self.raw_context(),
                exception,
            )));
        }
        self.state.finalizer.unprotect(self.raw_context(), value);
        Ok(())
    }

    /// Clears a global property by assigning JavaScript `undefined`.
    pub fn clear_global(&self, name: &str) -> Result<()> {
        with_scope(self.raw_context(), |cx| {
            self.set_global_value(name, Engine::undefined(cx))
        })
    }

    /// Evaluates a script and protects its completion value until runtime teardown.
    pub fn eval(&self, source: &str, name: &str) -> Result<JSValueRef> {
        let value = self.eval_unprotected(source, name)?;
        self.state.finalizer.protect(self.raw_context(), value);
        Ok(value)
    }

    fn eval_unprotected(&self, source: &str, name: &str) -> Result<JSValueRef> {
        let source = JsString::new(source)?;
        let name = JsString::new(name)?;
        let mut exception = ptr::null();
        // SAFETY: strings and context are live for the evaluation.
        let value = unsafe {
            JSEvaluateScript(
                self.raw_context(),
                source.as_raw(),
                ptr::null_mut(),
                name.as_raw(),
                1,
                &mut exception,
            )
        };
        if !exception.is_null() {
            Err(Error::Exception(value_to_string(
                self.raw_context(),
                exception,
            )))
        } else if value.is_null() {
            Err(Error::Null("JSEvaluateScript"))
        } else {
            Ok(value)
        }
    }

    /// Drains adapter-deferred unprotects and the native release queue.
    pub fn drain_releases(&self) -> std::result::Result<usize, core::QueueError> {
        let _ = self
            .state
            .finalizer
            .drain_deferred_unprotects(self.raw_context());
        self.state.finalizer.env.queue().drain()
    }

    /// Runs the engine-neutral tick and then drains deferred JSC unprotects.
    ///
    /// JavaScriptCore's public C API has no unhandled-rejection tracker (J20),
    /// so this method never reports an unhandled-rejection diagnostic.
    ///
    /// # Safety
    ///
    /// `instance` must remain live for the call and the caller must invoke this
    /// on the JavaScriptCore/tick thread.
    pub unsafe fn tick(&self, instance: ffi_wgpu::WGPUInstance) -> Result<usize> {
        let result = with_scope(self.raw_context(), |cx| unsafe {
            core::tick::<Engine>(cx, instance)
        });
        let _ = self
            .state
            .finalizer
            .drain_deferred_unprotects(self.raw_context());
        match result {
            Ok(drained) => Ok(drained),
            Err(core::TickError::Queue(error)) => {
                Err(Error::Exception(format!("tick queue error: {error:?}")))
            }
            Err(core::TickError::Engine(error)) => {
                Err(Error::Exception(value_to_string(self.raw_context(), error)))
            }
            Err(_) => Err(Error::Exception("unknown tick failure".to_owned())),
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.state.release_outstanding_deferreds(self.raw_context());
        with_scope(self.raw_context(), |cx| {
            self.state
                .finalizer
                .env
                .settlements()
                .release_pending::<Engine>(cx);
        });
        if let Some(trampoline) = self.state.take_trampoline() {
            self.state
                .finalizer
                .unprotect(self.raw_context(), trampoline);
        }
        self.state.finalizer.begin_teardown(self.raw_context());
        // SAFETY: context release runs after all protected engine values and
        // outstanding deferreds have been released on this engine thread.
        unsafe { JSGlobalContextRelease(self.raw_context()) };
        let _ = self.state.finalizer.env.queue().drain();
        if let Ok(mut classes) = self.state.classes.lock() {
            for (_, entry) in std::mem::take(&mut *classes) {
                // SAFETY: each registered class has one retained create reference.
                unsafe { JSClassRelease(entry.class) };
            }
        }
        // SAFETY: these are the create references retained by Runtime/State.
        unsafe {
            JSClassRelease(self.state.method_class);
            JSClassRelease(self.global_class.as_ptr());
        }
    }
}

// SAFETY: JsEngine's safe methods receive opaque C handles whose provenance is
// established by the live Context and by values produced from that context.
// The trait cannot mark individual implementations unsafe, so Clippy mistakes
// forwarding those handles to JSC for arbitrary raw-pointer dereferences.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl core::JsEngine for Engine {
    type Value = JSValueRef;
    type Context<'a> = Context<'a>;
    type Error = JSValueRef;
    type DeferredRegistration = DeferredRegistration;

    const MAPPED_RANGE_STRATEGY: core::MappedRangeStrategy =
        core::MappedRangeStrategy::CopyInCopyOut;

    fn environment<'a>(cx: Self::Context<'a>) -> &'a core::Environment {
        &state_from_context(cx.ctx).finalizer.env
    }

    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> core::Result<Self::Value, Self::Error> {
        let key = JsString::new(key).map_err(|_| Self::type_error(cx, "invalid property name"))?;
        let object = value_to_object(cx, obj)?;
        let mut exception = ptr::null();
        // SAFETY: all handles belong to this live context.
        let value = unsafe { JSObjectGetProperty(cx.ctx, object, key.as_raw(), &mut exception) };
        if exception.is_null() {
            cx.scope.track(value);
            Ok(value)
        } else {
            Err(exception)
        }
    }

    fn global(cx: Self::Context<'_>) -> Self::Value {
        // SAFETY: cx carries a live global context.
        let value = unsafe { JSContextGetGlobalObject(cx.ctx) }.cast_const();
        cx.scope.track(value);
        value
    }

    fn get_property_value(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: Self::Value,
    ) -> core::Result<Self::Value, Self::Error> {
        let object = value_to_object(cx, obj)?;
        let mut exception = ptr::null();
        // SAFETY: all handles belong to this live context.
        let value = unsafe { JSObjectGetPropertyForKey(cx.ctx, object, key, &mut exception) };
        if exception.is_null() {
            cx.scope.track(value);
            Ok(value)
        } else {
            Err(exception)
        }
    }

    fn call(
        cx: Self::Context<'_>,
        f: Self::Value,
        this: Self::Value,
        args: &[Self::Value],
    ) -> core::Result<Self::Value, Self::Error> {
        let function = value_to_object(cx, f)?;
        let this = value_to_object(cx, this)?;
        let mut exception = ptr::null();
        // SAFETY: the function, receiver, and arguments belong to this context.
        let value = unsafe {
            JSObjectCallAsFunction(
                cx.ctx,
                function,
                this,
                args.len(),
                args.as_ptr(),
                &mut exception,
            )
        };
        if exception.is_null() && !value.is_null() {
            cx.scope.track(value);
            Ok(value)
        } else if !exception.is_null() {
            Err(exception)
        } else {
            Err(Self::operation_error(cx, "JSObjectCallAsFunction failed"))
        }
    }

    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool {
        // SAFETY: value belongs to cx.
        unsafe { JSValueIsUndefined(cx.ctx, value) }
    }

    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool {
        // SAFETY: value belongs to cx.
        unsafe { JSValueIsNull(cx.ctx, value) }
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> core::Result<f64, Self::Error> {
        // JSValueToNumber follows the explicit `Number(value)` operation, which
        // accepts BigInt. WebIDL ToNumber instead rejects BigInt, so preserve
        // the boundary's WebIDL contract with the SDK predicate first.
        if unsafe { JSValueIsBigInt(cx.ctx, value) } {
            return Err(Self::type_error(
                cx,
                "BigInt cannot be converted to a number",
            ));
        }
        let mut exception = ptr::null();
        // SAFETY: value belongs to cx; exception is initialized to null.
        let number = unsafe { JSValueToNumber(cx.ctx, value, &mut exception) };
        if exception.is_null() {
            Ok(number)
        } else {
            Err(exception)
        }
    }

    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool {
        // SAFETY: value belongs to cx.
        unsafe { JSValueToBoolean(cx.ctx, value) }
    }

    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a core::Arena,
    ) -> core::Result<&'a str, Self::Error> {
        let mut exception = ptr::null();
        // SAFETY: value belongs to cx; the returned string follows Create Rule.
        let string = unsafe { JSValueToStringCopy(cx.ctx, value, &mut exception) };
        if !exception.is_null() {
            return Err(exception);
        }
        let Some(string) = NonNull::new(string) else {
            return Err(Self::operation_error(cx, "JSValueToStringCopy failed"));
        };
        let string = JsString(string);
        Ok(arena.alloc_str(&string.to_string_lossy()))
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
        let name = CString::new(spec.name)
            .map_err(|_| Self::type_error(cx, "class name contains a nul byte"))?;
        let mut definition = JSClassDefinition::empty();
        definition.class_name = name.as_ptr();
        definition.finalize = Some(wrapper_finalize);
        definition.get_property = Some(wrapper_get_property);
        definition.set_property = Some(wrapper_set_property);
        // JSC has no gc_mark callback (J5/F4). Values retained by core are
        // protected when duplicated, so the class intentionally has no trace
        // hook and finalization defers their matching unprotects.
        // SAFETY: definition matches the SDK layout. The class name storage is
        // retained in ClassEntry for at least the lifetime of the JSClassRef.
        let class = unsafe { JSClassCreate(&definition) };
        if class.is_null() {
            return Err(Self::operation_error(cx, "JSClassCreate failed"));
        }
        state
            .classes
            .lock()
            .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
            .insert(
                spec.id,
                ClassEntry {
                    class,
                    spec,
                    _name: name,
                },
            );
        Ok(spec.id)
    }

    fn new_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
    ) -> core::Result<Self::Value, Self::Error> {
        let state = state_from_context(cx.ctx);
        let (js_class, spec) = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?;
            let Some(entry) = classes.get(&class) else {
                return Err(Self::operation_error(cx, "class is not registered"));
            };
            (entry.class, entry.spec)
        };
        let holder = Box::new(ObjectPayload {
            spec,
            payload,
            finalizer: Arc::clone(&state.finalizer),
        });
        let raw = Box::into_raw(holder).cast();
        // SAFETY: class is registered and raw is owned by the resulting object.
        let object = unsafe { JSObjectMake(cx.ctx, js_class, raw) };
        if object.is_null() {
            // SAFETY: JSObjectMake did not adopt private data on failure.
            unsafe { drop(Box::from_raw(raw.cast::<ObjectPayload>())) };
            Err(Self::operation_error(cx, "JSObjectMake failed"))
        } else {
            let value = object.cast_const();
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn payload<'a>(
        _cx: Self::Context<'a>,
        obj: Self::Value,
        class: core::ClassId,
    ) -> Option<&'a (dyn Any + Send)> {
        let raw = unsafe { JSObjectGetPrivate(obj.cast_mut()) }.cast::<ObjectPayload>();
        let holder = unsafe { raw.as_ref() }?;
        (holder.spec.id == class).then_some(holder.payload.as_ref())
    }

    fn undefined(cx: Self::Context<'_>) -> Self::Value {
        // SAFETY: cx is live.
        unsafe { JSValueMakeUndefined(cx.ctx) }
    }

    fn number(cx: Self::Context<'_>, value: f64) -> core::Result<Self::Value, Self::Error> {
        // SAFETY: cx is live.
        let value = unsafe { JSValueMakeNumber(cx.ctx, value) };
        cx.scope.track(value);
        Ok(value)
    }

    fn string(cx: Self::Context<'_>, value: &str) -> core::Result<Self::Value, Self::Error> {
        let value =
            JsString::new(value).map_err(|_| Self::type_error(cx, "string contains a nul byte"))?;
        // SAFETY: cx and value are live; JSC retains the JSString for the value.
        let value = unsafe { JSValueMakeString(cx.ctx, value.as_raw()) };
        cx.scope.track(value);
        Ok(value)
    }

    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        make_error(cx, message, true)
    }

    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        make_error(cx, message, false)
    }

    fn async_error_value(cx: Self::Context<'_>, message: &str) -> Self::Value {
        Self::string(cx, message).unwrap_or_else(|_| Self::undefined(cx))
    }

    fn error_value_from_error(_cx: Self::Context<'_>, error: Self::Error) -> Self::Value {
        error
    }

    fn new_promise(
        cx: Self::Context<'_>,
    ) -> core::Result<(Self::Value, core::Deferred<Self>), Self::Error> {
        let mut resolve = ptr::null_mut();
        let mut reject = ptr::null_mut();
        let mut exception = ptr::null();
        // SAFETY: all output pointers are initialized and cx is live.
        let promise = unsafe {
            JSObjectMakeDeferredPromise(cx.ctx, &mut resolve, &mut reject, &mut exception)
        };
        if !exception.is_null() {
            return Err(exception);
        }
        if promise.is_null() || resolve.is_null() || reject.is_null() {
            return Err(Self::operation_error(
                cx,
                "JSObjectMakeDeferredPromise failed",
            ));
        }
        let state = state_from_context(cx.ctx);
        state.finalizer.protect(cx.ctx, resolve.cast_const());
        state.finalizer.protect(cx.ctx, reject.cast_const());
        let promise = promise.cast_const();
        cx.scope.track(promise);
        Ok((
            promise,
            core::Deferred::new(resolve.cast_const(), reject.cast_const()),
        ))
    }

    fn settle_deferreds(cx: Self::Context<'_>, settlements: Vec<core::DeferredSettlement<Self>>) {
        if settlements.is_empty() {
            return;
        }
        let state = state_from_context(cx.ctx);
        let Some(trampoline) = state.trampoline() else {
            for (deferred, _) in settlements {
                state.finalizer.unprotect(cx.ctx, deferred.resolve());
                state.finalizer.unprotect(cx.ctx, deferred.reject());
            }
            return;
        };
        let mut functions = Vec::with_capacity(settlements.len());
        let mut values = Vec::with_capacity(settlements.len());
        let mut selected = Vec::with_capacity(settlements.len());
        for (deferred, result) in settlements {
            let (function, unused, value) = match result {
                Ok(value) => (deferred.resolve(), deferred.reject(), value),
                Err(value) => (deferred.reject(), deferred.resolve(), value),
            };
            state.finalizer.unprotect(cx.ctx, unused);
            functions.push(function);
            values.push(value);
            selected.push(function);
        }
        let mut exception = ptr::null();
        // SAFETY: vector elements are live values in cx.
        let function_array = unsafe {
            JSObjectMakeArray(cx.ctx, functions.len(), functions.as_ptr(), &mut exception)
        };
        let value_array = if exception.is_null() {
            // SAFETY: vector elements are live values in cx.
            unsafe { JSObjectMakeArray(cx.ctx, values.len(), values.as_ptr(), &mut exception) }
        } else {
            ptr::null_mut()
        };
        if exception.is_null() && !function_array.is_null() && !value_array.is_null() {
            let arguments = [function_array.cast_const(), value_array.cast_const()];
            // SAFETY: this is the single JavaScript entry for the entire batch,
            // producing one JSC microtask checkpoint on return (J2/F3).
            let _ = unsafe {
                JSObjectCallAsFunction(
                    cx.ctx,
                    trampoline.cast_mut(),
                    ptr::null_mut(),
                    arguments.len(),
                    arguments.as_ptr(),
                    &mut exception,
                )
            };
        }
        for function in selected {
            state.finalizer.unprotect(cx.ctx, function);
        }
    }

    fn drain_microtasks(_cx: Self::Context<'_>) -> core::Result<(), Self::Error> {
        // JSC exposes no public microtask pump (F1); microtasks drain when the
        // settlement trampoline's JavaScript frame returns (F2).
        Ok(())
    }

    unsafe fn new_external_arraybuffer(
        cx: Self::Context<'_>,
        _ptr: *mut u8,
        _len: usize,
        _owner: core::WGPUBuffer,
    ) -> core::Result<Self::Value, Self::Error> {
        Err(Self::operation_error(cx, "JSC mapping lands in part 2"))
    }

    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        _bytes: &[u8],
    ) -> core::Result<Self::Value, Self::Error> {
        Err(Self::operation_error(cx, "JSC mapping lands in part 2"))
    }

    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        _value: Self::Value,
        _out: Option<&mut [u8]>,
    ) -> core::Result<(), Self::Error> {
        Err(Self::operation_error(cx, "JSC mapping lands in part 2"))
    }

    fn arraybuffer_len(_cx: Self::Context<'_>, _value: Self::Value) -> Option<usize> {
        None
    }

    fn arraybuffer_copy(_cx: Self::Context<'_>, _value: Self::Value) -> Option<Vec<u8>> {
        None
    }

    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value {
        state_from_context(cx.ctx).finalizer.protect(cx.ctx, value);
        value
    }

    fn return_held_value(_cx: Self::Context<'_>, held: Self::Value) -> Self::Value {
        held
    }

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        state_from_context(cx.ctx)
            .finalizer
            .unprotect(cx.ctx, value);
    }

    fn register_deferred(
        cx: Self::Context<'_>,
        slot: NonNull<Option<core::Deferred<Self>>>,
    ) -> Self::DeferredRegistration {
        state_from_context(cx.ctx).register_deferred(slot)
    }

    fn release_deferred(cx: Self::Context<'_>, deferred: core::Deferred<Self>) {
        let state = state_from_context(cx.ctx);
        state.finalizer.unprotect(cx.ctx, deferred.resolve());
        state.finalizer.unprotect(cx.ctx, deferred.reject());
    }
}

fn value_to_object(cx: Context<'_>, value: JSValueRef) -> core::Result<JSObjectRef, JSValueRef> {
    let mut exception = ptr::null();
    // SAFETY: value belongs to cx.
    let object = unsafe { JSValueToObject(cx.ctx, value, &mut exception) };
    if !exception.is_null() {
        Err(exception)
    } else if object.is_null() {
        Err(Engine::type_error(cx, "value is not an object"))
    } else {
        Ok(object)
    }
}

fn make_error(cx: Context<'_>, message: &str, type_error: bool) -> JSValueRef {
    let message = JsString::new(message).ok().map(|message| {
        // SAFETY: cx and message are live for value construction.
        unsafe { JSValueMakeString(cx.ctx, message.as_raw()) }
    });
    let arguments = message.as_slice();
    if type_error {
        if let Ok(name) = JsString::new("TypeError") {
            let global = unsafe { JSContextGetGlobalObject(cx.ctx) };
            let mut exception = ptr::null();
            // SAFETY: all handles belong to cx.
            let constructor =
                unsafe { JSObjectGetProperty(cx.ctx, global, name.as_raw(), &mut exception) };
            if exception.is_null() && !constructor.is_null() {
                let constructor = unsafe { JSValueToObject(cx.ctx, constructor, &mut exception) };
                if exception.is_null() && !constructor.is_null() {
                    let error = unsafe {
                        JSObjectCallAsConstructor(
                            cx.ctx,
                            constructor,
                            arguments.len(),
                            arguments.as_ptr(),
                            &mut exception,
                        )
                    };
                    if exception.is_null() && !error.is_null() {
                        return error.cast_const();
                    }
                }
            }
        }
    }
    let mut exception = ptr::null();
    // SAFETY: arguments are live values in cx.
    let error =
        unsafe { JSObjectMakeError(cx.ctx, arguments.len(), arguments.as_ptr(), &mut exception) };
    if !exception.is_null() {
        exception
    } else if error.is_null() {
        Engine::undefined(cx)
    } else {
        error.cast_const()
    }
}

unsafe extern "C" fn wrapper_get_property(
    ctx: JSContextRef,
    object: JSObjectRef,
    property_name: JSStringRef,
    exception: *mut JSValueRef,
) -> JSValueRef {
    let scope = Scope::new();
    let cx = Context { ctx, scope: &scope };
    match catch_unwind(AssertUnwindSafe(|| {
        let name = js_string_to_rust(property_name);
        let raw = unsafe { JSObjectGetPrivate(object) }.cast::<ObjectPayload>();
        let Some(holder) = (unsafe { raw.as_ref() }) else {
            return Ok(ptr::null());
        };
        if let Some(method) = holder
            .spec
            .methods
            .iter()
            .find(|method| method.name == name)
        {
            return make_method(cx, method.call);
        }
        let Some(getter) = holder
            .spec
            .properties
            .iter()
            .find(|property| property.name == name)
            .and_then(|property| property.get)
        else {
            return Ok(ptr::null());
        };
        getter(cx, object.cast_const())
    })) {
        Ok(Ok(value)) => {
            scope.escape(value);
            value
        }
        Ok(Err(error)) => {
            write_exception(exception, error);
            ptr::null()
        }
        Err(_) => {
            write_exception(
                exception,
                Engine::operation_error(cx, "Rust callback panicked"),
            );
            ptr::null()
        }
    }
}

unsafe extern "C" fn wrapper_set_property(
    ctx: JSContextRef,
    object: JSObjectRef,
    property_name: JSStringRef,
    value: JSValueRef,
    exception: *mut JSValueRef,
) -> bool {
    let scope = Scope::new();
    let cx = Context { ctx, scope: &scope };
    match catch_unwind(AssertUnwindSafe(|| {
        let name = js_string_to_rust(property_name);
        let raw = unsafe { JSObjectGetPrivate(object) }.cast::<ObjectPayload>();
        let Some(holder) = (unsafe { raw.as_ref() }) else {
            return Ok(false);
        };
        let Some(property) = holder
            .spec
            .properties
            .iter()
            .find(|property| property.name == name)
        else {
            return Ok(false);
        };
        let Some(setter) = property.set else {
            return Ok(true);
        };
        setter(cx, object.cast_const(), value)?;
        Ok(true)
    })) {
        Ok(Ok(set)) => set,
        Ok(Err(error)) => {
            write_exception(exception, error);
            false
        }
        Err(_) => {
            write_exception(
                exception,
                Engine::operation_error(cx, "Rust callback panicked"),
            );
            false
        }
    }
}

unsafe extern "C" fn method_call(
    ctx: JSContextRef,
    function: JSObjectRef,
    this_object: JSObjectRef,
    argument_count: usize,
    arguments: *const JSValueRef,
    exception: *mut JSValueRef,
) -> JSValueRef {
    let scope = Scope::new();
    let cx = Context { ctx, scope: &scope };
    match catch_unwind(AssertUnwindSafe(|| {
        let raw = unsafe { JSObjectGetPrivate(function) }.cast::<MethodTarget>();
        let Some(target) = (unsafe { raw.as_ref() }) else {
            return Err(Engine::operation_error(cx, "method target is missing"));
        };
        let args = if argument_count == 0 || arguments.is_null() {
            &[]
        } else {
            // SAFETY: JSC provides argument_count live values for this callback.
            unsafe { std::slice::from_raw_parts(arguments, argument_count) }
        };
        (target.call)(cx, this_object.cast_const(), args)
    })) {
        Ok(Ok(value)) => {
            scope.escape(value);
            value
        }
        Ok(Err(error)) => {
            write_exception(exception, error);
            ptr::null()
        }
        Err(_) => {
            write_exception(
                exception,
                Engine::operation_error(cx, "Rust callback panicked"),
            );
            ptr::null()
        }
    }
}

unsafe extern "C" fn wrapper_finalize(object: JSObjectRef) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // J21: JSObjectGetPrivate takes no JSContextRef and does not allocate.
        let raw = unsafe { JSObjectGetPrivate(object) }.cast::<ObjectPayload>();
        let Some(raw) = NonNull::new(raw) else {
            return;
        };
        // SAFETY: this finalizer is the sole owner of the object's private Box.
        let holder = unsafe { Box::from_raw(raw.as_ptr()) };
        core::release_payload_values::<Engine>(holder.payload.as_ref(), &mut |value| {
            holder.finalizer.defer_unprotect(value);
        });
        // Core finalizers only move native handles into Environment's release
        // queue. They call neither a JSC context API nor webgpu.h (J6/J15/J21).
        (holder.spec.finalizer)(holder.payload, &holder.finalizer.env);
    }));
}

unsafe extern "C" fn method_finalize(object: JSObjectRef) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // This callback has no context and only drops Rust-owned dispatch data.
        let raw = unsafe { JSObjectGetPrivate(object) }.cast::<MethodTarget>();
        if let Some(raw) = NonNull::new(raw) {
            // SAFETY: the callable object owns exactly one MethodTarget Box.
            unsafe { drop(Box::from_raw(raw.as_ptr())) };
        }
    }));
}

fn make_method(
    cx: Context<'_>,
    call: core::MethodFn<Engine>,
) -> core::Result<JSValueRef, JSValueRef> {
    let target = Box::new(MethodTarget { call });
    let raw = Box::into_raw(target).cast();
    let state = state_from_context(cx.ctx);
    // SAFETY: method_class is live and adopts raw as private data.
    let object = unsafe { JSObjectMake(cx.ctx, state.method_class, raw) };
    if object.is_null() {
        // SAFETY: JSObjectMake did not adopt private data on failure.
        unsafe { drop(Box::from_raw(raw.cast::<MethodTarget>())) };
        Err(Engine::operation_error(cx, "JSObjectMake(method) failed"))
    } else {
        let value = object.cast_const();
        cx.scope.track(value);
        Ok(value)
    }
}

fn write_exception(out: *mut JSValueRef, error: JSValueRef) {
    if let Some(out) = unsafe { out.as_mut() } {
        *out = error;
    }
}

fn state_from_context(ctx: JSContextRef) -> &'static State {
    // SAFETY: Runtime installs its address-stable boxed State on the global
    // object before any adapter callback can run and retains it past context
    // release.
    let global = unsafe { JSContextGetGlobalObject(ctx) };
    let raw = unsafe { JSObjectGetPrivate(global) }.cast::<State>();
    unsafe { &*raw }
}

fn js_string_to_rust(string: JSStringRef) -> String {
    // SAFETY: callback property-name strings are live for the callback.
    let size = unsafe { JSStringGetMaximumUTF8CStringSize(string) };
    let mut bytes = vec![0_u8; size];
    // SAFETY: bytes has the maximum capacity requested by JSC.
    let written = unsafe { JSStringGetUTF8CString(string, bytes.as_mut_ptr().cast(), bytes.len()) };
    if written == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(&bytes[..written.saturating_sub(1)]).into_owned()
    }
}

fn value_to_string(ctx: JSContextRef, value: JSValueRef) -> String {
    let mut exception = ptr::null();
    // SAFETY: value belongs to ctx and the result follows Create Rule.
    let string = unsafe { JSValueToStringCopy(ctx, value, &mut exception) };
    if !exception.is_null() || string.is_null() {
        return "JavaScriptCore exception".to_owned();
    }
    let Some(string) = NonNull::new(string) else {
        return "JavaScriptCore exception".to_owned();
    };
    JsString(string).to_string_lossy()
}

fn gpu_dispatch() -> core::GpuDispatch {
    core::GpuDispatch {
        instance_process_events,
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

unsafe fn instance_process_events(instance: core::WGPUInstance) {
    unsafe { ffi_wgpu::wgpuInstanceProcessEvents(instance) };
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use super::{core, ffi_wgpu as wgpu, Context, Engine, JSValueRef, Runtime, Scope};
    use webgpu_native_js_core::JsEngine;

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
            // SAFETY: NativeSetup owns one live reference to each handle.
            unsafe {
                wgpu::wgpuDeviceRelease(self.device);
                wgpu::wgpuAdapterRelease(self.adapter);
                wgpu::wgpuInstanceRelease(self.instance);
            }
        }
    }

    fn native_setup() -> NativeSetup {
        // SAFETY: a null descriptor requests backend defaults, including Noop.
        let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        assert!(!instance.is_null());
        let adapter_state = Rc::new(AdapterRequestState {
            status: Cell::new(
                wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_CallbackCancelled,
            ),
            handle: Cell::new(ptr::null_mut()),
        });
        let userdata = Rc::into_raw(Rc::clone(&adapter_state)).cast_mut().cast();
        let info = wgpu::WGPURequestAdapterCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(adapter_callback),
            userdata1: userdata,
            userdata2: ptr::null_mut(),
        };
        // SAFETY: userdata owns an Rc clone until the callback reclaims it.
        unsafe {
            wgpu::wgpuInstanceRequestAdapter(instance, ptr::null(), info);
            wgpu::wgpuInstanceProcessEvents(instance);
        }
        assert_eq!(
            adapter_state.status.get(),
            wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success
        );
        let adapter = adapter_state.handle.get();
        assert!(!adapter.is_null());

        let device_state = Rc::new(DeviceRequestState {
            status: Cell::new(
                wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_CallbackCancelled,
            ),
            handle: Cell::new(ptr::null_mut()),
        });
        let userdata = Rc::into_raw(Rc::clone(&device_state)).cast_mut().cast();
        let info = wgpu::WGPURequestDeviceCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(device_callback),
            userdata1: userdata,
            userdata2: ptr::null_mut(),
        };
        // SAFETY: userdata owns an Rc clone until the callback reclaims it.
        unsafe {
            wgpu::wgpuAdapterRequestDevice(adapter, ptr::null(), info);
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
            // SAFETY: native_setup leaked exactly one Rc clone into userdata1.
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
            // SAFETY: native_setup leaked exactly one Rc clone into userdata1.
            let state = unsafe { Rc::from_raw(userdata1.cast::<DeviceRequestState>()) };
            state.status.set(status);
            state.handle.set(device);
        }));
    }

    fn eval(runtime: &Runtime, source: &str, name: &str) {
        runtime.eval(source, name).unwrap_or_else(|error| {
            panic!("{name}: {error:?}");
        });
    }

    #[test]
    fn scope_is_non_optional_and_records_values() {
        let runtime = Runtime::new().expect("JSC runtime");
        let scope = Scope::new();
        let cx = Context {
            ctx: runtime.raw_context(),
            scope: &scope,
        };
        let global = Engine::global(cx);
        assert!(!Engine::is_undefined(cx, global));
        assert_eq!(scope.values.borrow().as_slice(), &[global]);
    }

    #[test]
    fn mapping_primitives_are_honest_part_two_stubs() {
        let runtime = Runtime::new().expect("JSC runtime");
        super::with_scope(runtime.raw_context(), |cx| {
            assert_eq!(
                Engine::MAPPED_RANGE_STRATEGY,
                core::MappedRangeStrategy::CopyInCopyOut
            );
            let copy_error = Engine::new_arraybuffer_copy(cx, &[1, 2, 3])
                .expect_err("copy creation must be deferred");
            assert!(super::value_to_string(runtime.raw_context(), copy_error)
                .contains("JSC mapping lands in part 2"));
            let external_error = unsafe {
                Engine::new_external_arraybuffer(cx, ptr::null_mut(), 0, ptr::null_mut())
            }
            .expect_err("external creation must be deferred");
            assert!(
                super::value_to_string(runtime.raw_context(), external_error)
                    .contains("JSC mapping lands in part 2")
            );
            let value = Engine::undefined(cx);
            assert!(Engine::detach_arraybuffer(cx, value, None).is_err());
            assert_eq!(Engine::arraybuffer_len(cx, value), None);
            assert_eq!(Engine::arraybuffer_copy(cx, value), None);
        });
    }

    #[test]
    fn wrap_device_buffer_label_destroy_vertical_slice() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                const buffer = device.createBuffer({ size: 16, usage: 8, label: 'initial' });
                if (buffer.label !== 'initial') throw new Error('initial label mismatch');
                buffer.label = 'round-trip';
                if (buffer.label !== 'round-trip') throw new Error('label round-trip mismatch');
                buffer.destroy();
            "#,
            "buffer-vertical-slice.js",
        );
        runtime.clear_global("device").expect("clear device");
    }

    #[test]
    fn protections_balance_at_teardown_without_forcing_gc() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let counters = Arc::clone(&runtime.state.finalizer);
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            "void device.queue; void device.createBuffer({size: 4, usage: 8});",
            "protection-balance.js",
        );
        drop(runtime);
        assert_eq!(
            counters.protect_count.load(Ordering::Relaxed),
            counters.unprotect_count.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn deferred_unprotect_queue_simulates_any_thread_finalizer_without_gc() {
        let runtime = Runtime::new().expect("JSC runtime");
        let value = runtime
            .eval("({held: true})", "deferred-unprotect.js")
            .expect("held value");
        let finalizer = Arc::clone(&runtime.state.finalizer);
        let moved = super::ProtectedValue(value);
        std::thread::spawn(move || {
            moved.defer_into(&finalizer);
        })
        .join()
        .expect("simulated finalizer thread");
        assert_eq!(runtime.drain_releases().expect("drain releases"), 0);
        assert_eq!(
            runtime
                .state
                .finalizer
                .deferred_unprotects
                .lock()
                .expect("deferred queue")
                .len(),
            0
        );
    }

    #[test]
    fn two_settlements_share_one_frame_and_exact_order() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let gpu = runtime.wrap_gpu(setup.instance).expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval(
            &runtime,
            "var firstAdapter; gpu.requestAdapter().then(a => { firstAdapter = a; });",
            "settle-prototype.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("prototype tick");
        eval(
            &runtime,
            r#"
                var order = [];
                var settleIndex = 0;
                Object.defineProperty(Object.getPrototypeOf(firstAdapter), 'then', {
                    configurable: true,
                    get() { order.push('settle' + (++settleIndex)); return undefined; }
                });
                gpu.requestAdapter().then(() => order.push('then1'));
                gpu.requestAdapter().then(() => order.push('then2'));
            "#,
            "settle-order.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("ordered tick");
        eval(
            &runtime,
            r#"
                const actual = order.join(',');
                if (actual !== 'settle1,settle2,then1,then2') {
                    throw new Error('settlement order was ' + actual);
                }
            "#,
            "settle-order-check.js",
        );
    }

    #[test]
    fn sequence_array_like_is_rejected_and_set_is_accepted() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                const e1 = device.createCommandEncoder();
                const c1 = e1.finish();
                let rejected = false;
                try { device.queue.submit({length: 1, 0: c1}); }
                catch (e) { rejected = e instanceof TypeError; }
                if (!rejected) throw new Error('array-like sequence was accepted');
                const e2 = device.createCommandEncoder();
                const c2 = e2.finish();
                device.queue.submit(new Set([c2]));
            "#,
            "sequence-conformance.js",
        );
    }

    #[test]
    fn bigint_size_throws_a_catchable_type_error() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                let caught = false;
                try { device.createBuffer({size: 10n, usage: 8}); }
                catch (e) { caught = e instanceof TypeError; }
                if (!caught) throw new Error('BigInt did not throw TypeError');
            "#,
            "bigint-type-error.js",
        );
    }

    #[test]
    fn promise_continuation_can_reenter_device_method() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let gpu = runtime.wrap_gpu(setup.instance).expect("wrap gpu");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            "var reentered = false; gpu.requestAdapter().then(() => { device.createBuffer({size: 4, usage: 8}).destroy(); reentered = true; });",
            "promise-reentry.js",
        );
        unsafe { runtime.tick(setup.instance) }.expect("reentrant tick");
        eval(
            &runtime,
            "if (!reentered) throw new Error('promise continuation did not re-enter');",
            "promise-reentry-check.js",
        );
    }

    const PANIC_CLASS: core::ClassId = core::ClassId(20_000);

    fn panicking_method(
        _cx: Context<'_>,
        _this: JSValueRef,
        _args: &[JSValueRef],
    ) -> core::Result<JSValueRef, JSValueRef> {
        panic!("intentional callback panic");
    }

    fn no_op_finalizer(_payload: Box<dyn std::any::Any + Send>, _env: &core::Environment) {}

    fn panic_spec() -> &'static core::ClassSpec<Engine> {
        Box::leak(Box::new(core::ClassSpec::<Engine> {
            name: "RuntimeProvidedPanicClass",
            id: PANIC_CLASS,
            properties: &[],
            methods: Box::leak(Box::new([core::MethodSpec::<Engine> {
                name: "runtimeProvidedPanicMethod",
                length: 0,
                call: panicking_method,
            }])),
            finalizer: no_op_finalizer,
        }))
    }

    #[test]
    fn panic_in_method_callback_is_a_js_exception() {
        let runtime = Runtime::new().expect("JSC runtime");
        let instance = super::with_scope(runtime.raw_context(), |cx| {
            Engine::register_class(cx, panic_spec()).expect("register class");
            Engine::new_instance(cx, PANIC_CLASS, Box::new(())).expect("instance")
        });
        runtime
            .set_global_value("panicObject", instance)
            .expect("set panic object");
        eval(
            &runtime,
            r#"
                let caught = false;
                try { panicObject.runtimeProvidedPanicMethod(); }
                catch (e) { caught = String(e).includes('Rust callback panicked'); }
                if (!caught) throw new Error('panic was not surfaced as JS exception');
            "#,
            "panic-containment.js",
        );
    }

    #[test]
    fn outstanding_request_adapter_is_released_before_context_teardown() {
        let setup = native_setup();
        {
            let runtime = Runtime::new().expect("JSC runtime");
            let gpu = runtime.wrap_gpu(setup.instance).expect("wrap gpu");
            runtime.set_global_value("gpu", gpu).expect("set gpu");
            eval(
                &runtime,
                "gpu.requestAdapter().then(() => { throw new Error('must not run'); });",
                "outstanding-adapter.js",
            );
        }
        // The backend may deliver callback cancellation only when the instance
        // is later released. Reaching here proves runtime teardown neither
        // dereferenced freed resolver values nor required a live release queue.
    }
}
