use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use webgpu_native_js_core as core;
use webgpu_native_js_core::__gpu_dispatch_from_ffi;
use webgpu_native_js_core::JsEngine;
use webgpu_native_js_ffi::native as ffi_wgpu;

pub use core::HostValue;

/// Opaque JavaScriptCore context storage.
pub enum OpaqueJsContext {}
/// Opaque JavaScriptCore value storage.
pub enum OpaqueJsValue {}
/// Opaque JavaScriptCore string storage.
pub enum OpaqueJsString {}
/// Opaque JavaScriptCore class storage.
pub enum OpaqueJsClass {}
/// Opaque JavaScriptCore property-name-array storage.
pub enum OpaqueJsPropertyNameArray {}

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
/// JavaScriptCore property-name-array handle.
pub type JSPropertyNameArrayRef = *mut OpaqueJsPropertyNameArray;

type JSChar = u16;
type JSPropertyAttributes = u32;
type JSClassAttributes = u32;
type JSTypedArrayType = c_int;
const TYPED_ARRAY_TYPE_UINT32_ARRAY: JSTypedArrayType = 6;
const TYPED_ARRAY_TYPE_ARRAY_BUFFER: JSTypedArrayType = 9;
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
type BigIntPredicate = unsafe extern "C" fn(JSContextRef, JSValueRef) -> bool;

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
const PROPERTY_DONT_ENUM: JSPropertyAttributes = 1 << 2;

#[link(name = "JavaScriptCore", kind = "framework")]
unsafe extern "C" {
    /// Creates a global JavaScript context with the supplied global-object class.
    fn JSGlobalContextCreate(global_object_class: JSClassRef) -> JSGlobalContextRef;
    /// Releases a global JavaScript context.
    fn JSGlobalContextRelease(ctx: JSGlobalContextRef);
    /// Returns a context's global object.
    fn JSContextGetGlobalObject(ctx: JSContextRef) -> JSObjectRef;
    /// Copies an object's enumerable property names, including inherited names.
    fn JSObjectCopyPropertyNames(ctx: JSContextRef, object: JSObjectRef) -> JSPropertyNameArrayRef;
    /// Releases a property-name array.
    fn JSPropertyNameArrayRelease(array: JSPropertyNameArrayRef);
    /// Returns the number of names in a property-name array.
    fn JSPropertyNameArrayGetCount(array: JSPropertyNameArrayRef) -> usize;
    /// Borrows a name from a property-name array.
    fn JSPropertyNameArrayGetNameAtIndex(
        array: JSPropertyNameArrayRef,
        index: usize,
    ) -> JSStringRef;
    /// Creates a JavaScript string by copying a nul-terminated UTF-8 string.
    fn JSStringCreateWithUTF8CString(string: *const c_char) -> JSStringRef;
    /// Returns the number of UTF-16 code units in a JavaScript string.
    fn JSStringGetLength(string: JSStringRef) -> usize;
    /// Returns the UTF-16 backing store, live while the string is live.
    fn JSStringGetCharactersPtr(string: JSStringRef) -> *const JSChar;
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
    /// Retains a JavaScript string.
    fn JSStringRetain(string: JSStringRef) -> JSStringRef;
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
    /// Tests whether a value is a JavaScript boolean.
    fn JSValueIsBoolean(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Tests whether a value is a JavaScript number.
    fn JSValueIsNumber(ctx: JSContextRef, value: JSValueRef) -> bool;
    /// Tests whether a value is a JavaScript object.
    fn JSValueIsObject(ctx: JSContextRef, value: JSValueRef) -> bool;
    // F9 owner decision: JSValueIsBigInt is intentionally not hard-linked.
    // Its macOS 15 / iOS 18 availability would impose that deployment floor,
    // so `bigint_predicate` resolves it at runtime and older systems use one
    // retained `JSObjectMakeFunction` typeof helper instead.
    /// Tests whether an object was created with a specific class.
    fn JSValueIsObjectOfClass(ctx: JSContextRef, value: JSValueRef, js_class: JSClassRef) -> bool;
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
    /// Returns the typed-array kind of a value, including ArrayBuffer.
    fn JSValueGetTypedArrayType(
        ctx: JSContextRef,
        value: JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSTypedArrayType;
    /// Protects a value from garbage collection.
    fn JSValueProtect(ctx: JSContextRef, value: JSValueRef);
    /// Removes one garbage-collection protection from a value.
    fn JSValueUnprotect(ctx: JSContextRef, value: JSValueRef);
    /// Creates JavaScript `undefined`.
    fn JSValueMakeUndefined(ctx: JSContextRef) -> JSValueRef;
    /// Creates JavaScript `null`.
    fn JSValueMakeNull(ctx: JSContextRef) -> JSValueRef;
    /// Creates a JavaScript boolean.
    fn JSValueMakeBoolean(ctx: JSContextRef, boolean: bool) -> JSValueRef;
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
    /// Creates a JavaScript function from trusted parameter and body strings.
    fn JSObjectMakeFunction(
        ctx: JSContextRef,
        name: JSStringRef,
        parameter_count: u32,
        parameter_names: *const JSStringRef,
        body: JSStringRef,
        source_url: JSStringRef,
        starting_line_number: c_int,
        exception: *mut JSValueRef,
    ) -> JSObjectRef;
    /// Tests whether an object is callable as a function.
    fn JSObjectIsFunction(ctx: JSContextRef, object: JSObjectRef) -> bool;
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
    /// Deletes a named object property.
    fn JSObjectDeleteProperty(
        ctx: JSContextRef,
        object: JSObjectRef,
        property_name: JSStringRef,
        exception: *mut JSValueRef,
    ) -> bool;
    /// Gets an object's JavaScript prototype.
    fn JSObjectGetPrototype(ctx: JSContextRef, object: JSObjectRef) -> JSValueRef;
    /// Sets an object's JavaScript prototype.
    fn JSObjectSetPrototype(ctx: JSContextRef, object: JSObjectRef, value: JSValueRef);
    /// Creates a constructor function associated with a class prototype.
    fn JSObjectMakeConstructor(
        ctx: JSContextRef,
        class: JSClassRef,
        call_as_constructor: Option<CallAsConstructorCallback>,
    ) -> JSObjectRef;
    /// Calls a JavaScript object as a function.
    fn JSObjectCallAsFunction(
        ctx: JSContextRef,
        object: JSObjectRef,
        this_object: JSObjectRef,
        argument_count: usize,
        arguments: *const JSValueRef,
        exception: *mut JSValueRef,
    ) -> JSValueRef;
    /// Returns an ArrayBuffer's private backing pointer.
    fn JSObjectGetArrayBufferBytesPtr(
        ctx: JSContextRef,
        object: JSObjectRef,
        exception: *mut JSValueRef,
    ) -> *mut c_void;
    /// Returns an ArrayBuffer's byte length without pinning it.
    fn JSObjectGetArrayBufferByteLength(
        ctx: JSContextRef,
        object: JSObjectRef,
        exception: *mut JSValueRef,
    ) -> usize;
    /// Returns an object's private pointer.
    fn JSObjectGetPrivate(object: JSObjectRef) -> *mut c_void;
    /// Replaces an object's private pointer.
    fn JSObjectSetPrivate(object: JSObjectRef, data: *mut c_void) -> bool;
}

unsafe extern "C" {
    /// Looks up a symbol in the already-loaded process images (libSystem libc).
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

fn bigint_predicate() -> Option<BigIntPredicate> {
    static PREDICATE: OnceLock<Option<BigIntPredicate>> = OnceLock::new();
    *PREDICATE.get_or_init(|| {
        // Darwin defines RTLD_DEFAULT as (void *)-2. JavaScriptCore is already
        // loaded through the adapter's other framework symbols.
        let default = (-2_isize) as *mut c_void;
        // SAFETY: the nul-terminated name is static and dlsym accepts the
        // process-wide RTLD_DEFAULT pseudo-handle.
        let symbol = unsafe { dlsym(default, c"JSValueIsBigInt".as_ptr()) };
        if symbol.is_null() {
            None
        } else {
            // SAFETY: WebKit declares JSValueIsBigInt with exactly the
            // BigIntPredicate ABI; dlsym returned that symbol's entry address.
            Some(unsafe { std::mem::transmute::<*mut c_void, BigIntPredicate>(symbol) })
        }
    })
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
        // The SDK defines JSChar as a UTF-16 code unit and keeps this backing
        // store live until the JSString is released. Reading UTF-16 directly
        // preserves lone surrogates for Rust's USVString-style lossy conversion.
        let len = unsafe { JSStringGetLength(self.as_raw()) };
        if len == 0 {
            String::new()
        } else {
            // SAFETY: the string is live, and the SDK promises len UTF-16 code
            // units in the returned backing store.
            let characters = unsafe { JSStringGetCharactersPtr(self.as_raw()) };
            debug_assert!(!characters.is_null());
            let utf16 = unsafe { std::slice::from_raw_parts(characters, len) };
            String::from_utf16_lossy(utf16)
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
    class_method_protected: Mutex<Vec<ProtectedValue>>,
    tearing_down: AtomicBool,
    protect_count: AtomicUsize,
    unprotect_count: AtomicUsize,
    teardown_mop_up_unprotect_count: AtomicUsize,
    class_method_protect_count: AtomicUsize,
    class_method_teardown_unprotect_count: AtomicUsize,
}

impl FinalizerState {
    fn new(gpu: core::GpuDispatch) -> Self {
        Self {
            env: core::Environment::new(gpu, Arc::new(core::ReleaseQueue::new())),
            deferred_unprotects: Mutex::new(Vec::new()),
            protected: Mutex::new(Vec::new()),
            class_method_protected: Mutex::new(Vec::new()),
            tearing_down: AtomicBool::new(false),
            protect_count: AtomicUsize::new(0),
            unprotect_count: AtomicUsize::new(0),
            teardown_mop_up_unprotect_count: AtomicUsize::new(0),
            class_method_protect_count: AtomicUsize::new(0),
            class_method_teardown_unprotect_count: AtomicUsize::new(0),
        }
    }

    fn protect(&self, ctx: JSContextRef, value: JSValueRef) {
        if ctx.is_null() || value.is_null() {
            return;
        }
        // SAFETY: called only on the live context's engine thread.
        unsafe { JSValueProtect(ctx, value) };
        self.protect_count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut protected) = self.protected.lock() {
            protected.push(ProtectedValue(value));
        }
    }

    fn protect_class_method(&self, ctx: JSContextRef, value: JSValueRef) {
        if ctx.is_null() || value.is_null() || self.tearing_down.load(Ordering::Acquire) {
            return;
        }
        // SAFETY: retained helpers are installed on the live context's engine
        // thread before teardown.
        unsafe { JSValueProtect(ctx, value) };
        let Ok(mut protected) = self.class_method_protected.lock() else {
            // SAFETY: without a ledger entry teardown cannot balance this
            // protection, so undo it immediately on the same engine thread.
            unsafe { JSValueUnprotect(ctx, value) };
            return;
        };
        if self.tearing_down.load(Ordering::Acquire) {
            // Teardown may have started between the first guard and this lock.
            // Leave the value unprotected rather than push into a drained ledger.
            drop(protected);
            // SAFETY: this balances the temporary protection above.
            unsafe { JSValueUnprotect(ctx, value) };
            return;
        }
        protected.push(ProtectedValue(value));
        self.protect_count.fetch_add(1, Ordering::Relaxed);
        self.class_method_protect_count
            .fetch_add(1, Ordering::Relaxed);
    }

    fn unprotect(&self, ctx: JSContextRef, value: JSValueRef) {
        if ctx.is_null() || value.is_null() {
            return;
        }
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
        if value.is_null() || self.tearing_down.load(Ordering::Acquire) {
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
            self.teardown_mop_up_unprotect_count
                .fetch_add(1, Ordering::Relaxed);
        }
        // Per-class method objects intentionally remain protected until runtime
        // teardown. Account for them separately so `teardown_mop_up` continues
        // to mean an unexpectedly live owner/scope protection (the M2 oracle).
        let class_methods = self
            .class_method_protected
            .lock()
            .map(|mut values| std::mem::take(&mut *values))
            .unwrap_or_default();
        for value in class_methods {
            // SAFETY: teardown runs on the engine thread before context release.
            unsafe { JSValueUnprotect(ctx, value.0) };
            self.unprotect_count.fetch_add(1, Ordering::Relaxed);
            self.class_method_teardown_unprotect_count
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

struct ClassEntry {
    class: JSClassRef,
    spec: &'static core::ClassSpec<Engine>,
    methods: Vec<ClassMethod>,
    prototype: JSValueRef,
    _name: CString,
}

struct ObjectPayload {
    spec: &'static core::ClassSpec<Engine>,
    payload: Box<dyn Any + Send>,
    finalizer: Arc<FinalizerState>,
}

enum MethodTarget {
    Method(core::MethodFn<Engine>),
    Host(Box<HostFunction>),
}

type HostFunction = dyn Fn(&[HostValue]) -> std::result::Result<(), String>;

#[derive(Clone, Copy)]
struct ClassMethod {
    name: &'static str,
    value: ProtectedValue,
}

/// JavaScriptCore adapter state shared by callbacks for one global context.
pub struct State {
    finalizer: Arc<FinalizerState>,
    classes: Mutex<BTreeMap<core::ClassId, ClassEntry>>,
    constructors: Mutex<BTreeMap<usize, core::ClassId>>,
    method_class: JSClassRef,
    bigint_predicate: Option<BigIntPredicate>,
    bigint_fallback: Option<ProtectedValue>,
    outstanding_deferreds: Arc<Mutex<Vec<DeferredSlot>>>,
    settle_trampoline: Mutex<Option<JSValueRef>>,
}

impl State {
    fn new(gpu: core::GpuDispatch, bigint_predicate: Option<BigIntPredicate>) -> Result<Self> {
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
            constructors: Mutex::new(BTreeMap::new()),
            method_class,
            bigint_predicate,
            bigint_fallback: None,
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
/// JavaScriptCore's collector discovers conservative roots on the machine stack,
/// but values retained in Rust heap allocations are invisible to it. This scope
/// is therefore the per-call root set: tracking protects a value, escaping
/// removes that protection while the returned value is still a stack root, and
/// dropping the scope unprotects every remaining value (J4/J7/R22).
///
/// Scope traffic uses `FinalizerState`'s protection ledger rather than a second
/// accounting path. Scopes always drop on the JavaScript/tick thread before
/// teardown, so their direct unprotects are legal and clean teardown can prove
/// that no scope protection was left for forced mop-up.
pub struct Scope {
    ctx: JSContextRef,
    finalizer: Arc<FinalizerState>,
    values: RefCell<Vec<JSValueRef>>,
}

impl Scope {
    fn new(ctx: JSContextRef) -> Self {
        Self {
            ctx,
            finalizer: Arc::clone(&state_from_context(ctx).finalizer),
            values: RefCell::new(Vec::new()),
        }
    }

    fn track(&self, value: JSValueRef) {
        if self.ctx.is_null() || value.is_null() {
            return;
        }
        self.finalizer.protect(self.ctx, value);
        self.values.borrow_mut().push(value);
    }

    fn escape(&self, value: JSValueRef) {
        let mut values = self.values.borrow_mut();
        if let Some(index) = values.iter().position(|candidate| *candidate == value) {
            let value = values.swap_remove(index);
            // The callback return value is still present on this C/Rust stack,
            // which JSC scans conservatively. Remove the explicit protection
            // now so scope drop skips it without leaking a protection.
            self.finalizer.unprotect(self.ctx, value);
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        for value in self.values.get_mut().drain(..) {
            self.finalizer.unprotect(self.ctx, value);
        }
    }
}

fn with_scope<R>(ctx: JSContextRef, f: impl FnOnce(Context<'_>) -> R) -> R {
    let scope = Scope::new(ctx);
    f(Context { ctx, scope: &scope })
}

/// A JavaScriptCore global context configured for WebGPU bindings.
pub struct Runtime {
    ctx: NonNull<OpaqueJsContext>,
    global_class: NonNull<OpaqueJsClass>,
    state: Box<State>,
}

/// Send + Sync producer handle for events on adopted devices.
#[derive(Clone)]
pub struct DeviceEventForwarder {
    inner: core::DeviceEventForwarder,
}

impl DeviceEventForwarder {
    /// Enqueues an adopted device's uncaptured error without touching JSC.
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

    /// Enqueues adopted-device loss without touching JSC.
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
    /// Creates a JavaScriptCore runtime configured with the WebGPU environment.
    pub fn new() -> Result<Self> {
        Self::new_with_dispatch(gpu_dispatch())
    }

    fn new_with_dispatch(gpu: core::GpuDispatch) -> Result<Self> {
        #[cfg(test)]
        {
            Self::new_with_dispatch_and_bigint_mode(gpu, false)
        }
        #[cfg(not(test))]
        {
            Self::new_with_dispatch_and_bigint_mode(gpu)
        }
    }

    #[cfg(test)]
    fn new_forcing_bigint_fallback() -> Result<Self> {
        Self::new_with_dispatch_and_bigint_mode(gpu_dispatch(), true)
    }

    fn new_with_dispatch_and_bigint_mode(
        gpu: core::GpuDispatch,
        #[cfg(test)] force_bigint_fallback: bool,
    ) -> Result<Self> {
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
        let predicate = bigint_predicate();
        #[cfg(test)]
        let predicate = if force_bigint_fallback {
            None
        } else {
            predicate
        };
        let state = match State::new(gpu, predicate) {
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
        let mut runtime = Self {
            ctx,
            global_class,
            state,
        };
        if runtime.state.bigint_predicate.is_none() {
            let fallback = with_scope(runtime.raw_context(), make_bigint_fallback)
                .map_err(|error| Error::Exception(value_to_string(runtime.raw_context(), error)))?;
            runtime
                .state
                .finalizer
                .protect_class_method(runtime.raw_context(), fallback);
            runtime.state.bigint_fallback = Some(ProtectedValue(fallback));
        }
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

    /// Returns a thread-safe adopted-device event producer.
    #[must_use]
    pub fn device_event_forwarder(&self) -> DeviceEventForwarder {
        DeviceEventForwarder {
            inner: self.state.finalizer.env.device_event_forwarder(),
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
    ///
    /// # Safety
    ///
    /// `instance` must be a live non-null handle from this runtime's backend
    /// and must remain live while the returned wrapper can be used.
    pub unsafe fn wrap_gpu(&self, instance: ffi_wgpu::WGPUInstance) -> Result<JSValueRef> {
        let value = with_scope(self.raw_context(), |cx| {
            core::wrap_gpu::<Engine>(cx, instance)
        })
        .map_err(|error| Error::Exception(value_to_string(self.raw_context(), error)))?;
        self.state.finalizer.protect(self.raw_context(), value);
        Ok(value)
    }

    /// Returns the native handle borrowed by a `GPURenderBundle` wrapper.
    ///
    /// This is class-checked through the registered `ClassSpec`; any value of
    /// the wrong JavaScript class returns `None`.
    ///
    /// # Lifetime
    ///
    /// The returned handle is **not retained**. The host must keep `value`
    /// alive (normally in a JavaScript global) for the entire native use, or
    /// call the backend's render-bundle AddRef function and own that reference.
    #[must_use]
    pub fn native_render_bundle(&self, value: JSValueRef) -> Option<ffi_wgpu::WGPURenderBundle> {
        with_scope(self.raw_context(), |cx| {
            core::native_render_bundle::<Engine>(cx, value)
        })
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

    /// Registers a global JavaScript function backed by a Rust callback.
    ///
    /// Primitive arguments preserve their type. Every other argument is
    /// converted with JavaScript `ToString`. The v1 callback is side-effect
    /// only; returning `Err` throws a JavaScript `TypeError` with that message.
    pub fn register_host_function<F>(&self, name: &str, f: F) -> Result<()>
    where
        F: Fn(&[HostValue]) -> std::result::Result<(), String> + 'static,
    {
        let _ = JsString::new(name)?;
        let target = Box::new(MethodTarget::Host(Box::new(f)));
        let raw = Box::into_raw(target).cast();
        let object = unsafe { JSObjectMake(self.raw_context(), self.state.method_class, raw) };
        if object.is_null() {
            unsafe { drop(Box::from_raw(raw.cast::<MethodTarget>())) };
            return Err(Error::Null("JSObjectMake(host function)"));
        }
        let value = object.cast_const();
        self.state.finalizer.protect(self.raw_context(), value);
        self.set_global_value(name, value)
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
            self.state
                .finalizer
                .env
                .release_device_event_values::<Engine>(cx);
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

    fn own_property_names(
        cx: Self::Context<'_>,
        obj: Self::Value,
    ) -> core::Result<Vec<String>, Self::Error> {
        let object = value_to_object(cx, obj)?;
        let global = unsafe { JSContextGetGlobalObject(cx.ctx) };
        let object_name = JsString::new("Object")
            .map_err(|_| Self::type_error(cx, "Object property name failed"))?;
        let prototype_name = JsString::new("prototype")
            .map_err(|_| Self::type_error(cx, "prototype property name failed"))?;
        let has_own_name = JsString::new("hasOwnProperty")
            .map_err(|_| Self::type_error(cx, "hasOwnProperty name failed"))?;
        let mut exception = ptr::null();
        let object_ctor =
            unsafe { JSObjectGetProperty(cx.ctx, global, object_name.as_raw(), &mut exception) };
        if !exception.is_null() {
            return Err(exception);
        }
        let object_ctor = value_to_object(cx, object_ctor)?;
        let prototype = unsafe {
            JSObjectGetProperty(cx.ctx, object_ctor, prototype_name.as_raw(), &mut exception)
        };
        if !exception.is_null() {
            return Err(exception);
        }
        let prototype = value_to_object(cx, prototype)?;
        let has_own = unsafe {
            JSObjectGetProperty(cx.ctx, prototype, has_own_name.as_raw(), &mut exception)
        };
        if !exception.is_null() {
            return Err(exception);
        }
        let has_own = value_to_object(cx, has_own)?;
        // SAFETY: both handles belong to the live context; Copy follows the Create Rule.
        let array = unsafe { JSObjectCopyPropertyNames(cx.ctx, object) };
        if array.is_null() {
            return Err(Self::operation_error(
                cx,
                "JSObjectCopyPropertyNames failed",
            ));
        }
        let count = unsafe { JSPropertyNameArrayGetCount(array) };
        let mut names = Vec::with_capacity(count);
        for index in 0..count {
            let name = unsafe { JSPropertyNameArrayGetNameAtIndex(array, index) };
            if name.is_null() {
                unsafe { JSPropertyNameArrayRelease(array) };
                return Err(Self::operation_error(cx, "property name was null"));
            }
            let argument = unsafe { JSValueMakeString(cx.ctx, name) };
            let own = unsafe {
                JSObjectCallAsFunction(
                    cx.ctx,
                    has_own,
                    object,
                    1,
                    ptr::from_ref(&argument),
                    &mut exception,
                )
            };
            if !exception.is_null() {
                unsafe { JSPropertyNameArrayRelease(array) };
                return Err(exception);
            }
            if !unsafe { JSValueToBoolean(cx.ctx, own) } {
                continue;
            }
            // The array owns the borrowed string, so retain it for the JsString drop guard.
            unsafe { JSStringRetain(name) };
            names.push(JsString(unsafe { NonNull::new_unchecked(name) }).to_string_lossy());
        }
        unsafe { JSPropertyNameArrayRelease(array) };
        Ok(names)
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

    fn construct(
        cx: Self::Context<'_>,
        ctor: Self::Value,
        args: &[Self::Value],
    ) -> core::Result<Self::Value, Self::Error> {
        let constructor = value_to_object(cx, ctor)?;
        let mut exception = ptr::null();
        // SAFETY: the constructor and arguments belong to this live context.
        let value = unsafe {
            JSObjectCallAsConstructor(
                cx.ctx,
                constructor,
                args.len(),
                args.as_ptr(),
                &mut exception,
            )
        };
        if exception.is_null() && !value.is_null() {
            let value = value.cast_const();
            cx.scope.track(value);
            Ok(value)
        } else if !exception.is_null() {
            Err(exception)
        } else {
            Err(Self::operation_error(
                cx,
                "JSObjectCallAsConstructor failed",
            ))
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

    fn is_object(cx: Self::Context<'_>, value: Self::Value) -> bool {
        // SAFETY: value belongs to cx.
        unsafe { JSValueIsObject(cx.ctx, value) }
    }

    fn is_callable(cx: Self::Context<'_>, value: Self::Value) -> bool {
        // SAFETY: the object predicate guards the JSValueRef-to-JSObjectRef cast.
        unsafe { JSValueIsObject(cx.ctx, value) && JSObjectIsFunction(cx.ctx, value.cast_mut()) }
    }

    fn same_value(_cx: Self::Context<'_>, left: Self::Value, right: Self::Value) -> bool {
        left == right
    }

    fn is_uint32array(cx: Self::Context<'_>, value: Self::Value) -> bool {
        let mut exception = ptr::null();
        // SAFETY: value belongs to cx. This predicate reads only the view kind
        // and does not expose or pin its backing-store pointer.
        let kind = unsafe { JSValueGetTypedArrayType(cx.ctx, value, &mut exception) };
        exception.is_null() && kind == TYPED_ARRAY_TYPE_UINT32_ARRAY
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> core::Result<f64, Self::Error> {
        // JSValueToNumber follows the explicit `Number(value)` operation, which
        // accepts BigInt. WebIDL ToNumber instead rejects BigInt, so preserve
        // the boundary's WebIDL contract with the runtime predicate first.
        if is_bigint(cx, value)? {
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
        let mut methods = Vec::with_capacity(spec.methods.len());
        for method in spec.methods {
            let value = match make_method(cx, method.call) {
                Ok(value) => value,
                Err(error) => {
                    // SAFETY: the class has not entered the registry, so this
                    // is the sole create reference and must be released here.
                    unsafe { JSClassRelease(class) };
                    return Err(error);
                }
            };
            state.finalizer.protect_class_method(cx.ctx, value);
            methods.push(ClassMethod {
                name: method.name,
                value: ProtectedValue(value),
            });
        }
        // SAFETY: class is live and wrapper_construct has the verified
        // JSObjectCallAsConstructorCallback ABI from the pinned SDK.
        let native_constructor =
            unsafe { JSObjectMakeConstructor(cx.ctx, class, Some(wrapper_construct)) };
        if native_constructor.is_null() {
            return Err(Self::operation_error(cx, "JSObjectMakeConstructor failed"));
        }
        state
            .constructors
            .lock()
            .map_err(|_| Self::operation_error(cx, "constructor registry is poisoned"))?
            .insert(native_constructor as usize, spec.id);
        let prototype = JsString::new("prototype")
            .map_err(|_| Self::operation_error(cx, "prototype string failed"))?;
        let mut exception = ptr::null();
        // SAFETY: native_constructor belongs to cx and exposes the class
        // prototype associated with instances created by JSObjectMake.
        let child_prototype = unsafe {
            JSObjectGetProperty(
                cx.ctx,
                native_constructor,
                prototype.as_raw(),
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        let child_prototype = unsafe { JSValueToObject(cx.ctx, child_prototype, &mut exception) };
        if !exception.is_null() || child_prototype.is_null() {
            return Err(if exception.is_null() {
                Self::operation_error(cx, "constructor prototype is not an object")
            } else {
                exception
            });
        }
        let constructor = make_interface_function(cx, spec, native_constructor)?;
        // SAFETY: interface function and prototype belong to the live cx.
        unsafe {
            JSObjectSetProperty(
                cx.ctx,
                constructor,
                prototype.as_raw(),
                child_prototype.cast_const(),
                PROPERTY_NONE,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        for method in &methods {
            let name = JsString::new(method.name)
                .map_err(|_| Self::type_error(cx, "method name contains a nul byte"))?;
            unsafe {
                JSObjectSetProperty(
                    cx.ctx,
                    child_prototype,
                    name.as_raw(),
                    method.value.0,
                    PROPERTY_DONT_ENUM,
                    &mut exception,
                )
            };
            if !exception.is_null() {
                return Err(exception);
            }
        }
        let constructor_name = JsString::new("constructor")
            .map_err(|_| Self::operation_error(cx, "constructor string failed"))?;
        // JSObjectMakeConstructor pre-populates an enumerable `constructor`
        // property. Delete it so the WebIDL attributes below can take effect.
        // The native class prototype also inherits a `constructor`, and JSC's
        // C API otherwise follows assignment semantics and creates the new own
        // property with default attributes. Temporarily detach that inherited
        // property while defining the replacement.
        // SAFETY: child_prototype belongs to the live cx.
        let inherited_prototype = unsafe { JSObjectGetPrototype(cx.ctx, child_prototype) };
        let deleted = unsafe {
            JSObjectDeleteProperty(
                cx.ctx,
                child_prototype,
                constructor_name.as_raw(),
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        if !deleted {
            return Err(Self::operation_error(
                cx,
                "failed to replace prototype constructor",
            ));
        }
        // SAFETY: prototype, constructor, and inherited_prototype belong to the
        // live cx. Restore the prototype chain before inspecting any exception.
        unsafe {
            JSObjectSetPrototype(cx.ctx, child_prototype, JSValueMakeNull(cx.ctx));
            JSObjectSetProperty(
                cx.ctx,
                child_prototype,
                constructor_name.as_raw(),
                constructor.cast_const(),
                PROPERTY_DONT_ENUM,
                &mut exception,
            );
            JSObjectSetPrototype(cx.ctx, child_prototype, inherited_prototype);
        };
        if !exception.is_null() {
            return Err(exception);
        }
        if let Some(parent) = spec
            .constructor
            .as_ref()
            .and_then(|constructor| constructor.parent)
        {
            let parent_prototype = match parent {
                core::ClassParent::Class(parent) => {
                    let parent_prototype = state
                        .classes
                        .lock()
                        .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?
                        .get(&parent)
                        .map(|entry| entry.prototype)
                        .ok_or_else(|| {
                            Self::operation_error(cx, "parent class is not registered")
                        })?;
                    parent_prototype
                }
                core::ClassParent::IntrinsicError => {
                    let error_name = JsString::new("Error")
                        .map_err(|_| Self::operation_error(cx, "Error string failed"))?;
                    let global = unsafe { JSContextGetGlobalObject(cx.ctx) };
                    let error_constructor = unsafe {
                        JSObjectGetProperty(cx.ctx, global, error_name.as_raw(), &mut exception)
                    };
                    if !exception.is_null() {
                        return Err(exception);
                    }
                    let error_constructor =
                        unsafe { JSValueToObject(cx.ctx, error_constructor, &mut exception) };
                    if !exception.is_null() || error_constructor.is_null() {
                        return Err(if exception.is_null() {
                            Self::operation_error(cx, "intrinsic Error is not an object")
                        } else {
                            exception
                        });
                    }
                    let parent_prototype = unsafe {
                        JSObjectGetProperty(
                            cx.ctx,
                            error_constructor,
                            prototype.as_raw(),
                            &mut exception,
                        )
                    };
                    if !exception.is_null() {
                        return Err(exception);
                    }
                    parent_prototype
                }
            };
            // SAFETY: prototype values belong to the live cx. This creates
            // JS inheritance without a native JSClass finalizer chain.
            unsafe { JSObjectSetPrototype(cx.ctx, child_prototype, parent_prototype) };
        }
        let name_property =
            JsString::new("name").map_err(|_| Self::operation_error(cx, "name string failed"))?;
        let function_name = JsString::new(spec.name)
            .map_err(|_| Self::type_error(cx, "class name contains a nul byte"))?;
        // SAFETY: constructor and strings belong to the live cx.
        let function_name = unsafe { JSValueMakeString(cx.ctx, function_name.as_raw()) };
        unsafe {
            JSObjectSetProperty(
                cx.ctx,
                constructor,
                name_property.as_raw(),
                function_name,
                PROPERTY_NONE,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        let length_property = JsString::new("length")
            .map_err(|_| Self::operation_error(cx, "length string failed"))?;
        // SAFETY: constructor and property belong to the live cx.
        let length = unsafe {
            JSValueMakeNumber(
                cx.ctx,
                spec.constructor
                    .as_ref()
                    .map_or(0.0, |constructor| f64::from(constructor.length)),
            )
        };
        unsafe {
            JSObjectSetProperty(
                cx.ctx,
                constructor,
                length_property.as_raw(),
                length,
                PROPERTY_NONE,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
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
                    methods,
                    prototype: child_prototype.cast_const(),
                    _name: name,
                },
            );
        let property = JsString::new(spec.name)
            .map_err(|_| Self::type_error(cx, "class name contains a nul byte"))?;
        let global = unsafe { JSContextGetGlobalObject(cx.ctx) };
        let mut exception = ptr::null();
        // SAFETY: global, constructor, and property belong to the live cx.
        unsafe {
            JSObjectSetProperty(
                cx.ctx,
                global,
                property.as_raw(),
                constructor.cast_const(),
                PROPERTY_NONE,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        Ok(spec.id)
    }

    fn new_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
    ) -> core::Result<Self::Value, Self::Error> {
        let state = state_from_context(cx.ctx);
        let (js_class, spec, prototype) = {
            let classes = state
                .classes
                .lock()
                .map_err(|_| Self::operation_error(cx, "class registry is poisoned"))?;
            let Some(entry) = classes.get(&class) else {
                return Err(Self::operation_error(cx, "class is not registered"));
            };
            (entry.class, entry.spec, entry.prototype)
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
            // SAFETY: object and the globally rooted interface prototype belong
            // to the live cx.
            unsafe { JSObjectSetPrototype(cx.ctx, object, prototype) };
            let value = object.cast_const();
            cx.scope.track(value);
            Ok(value)
        }
    }

    fn new_error_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
        name: &str,
        message: &str,
    ) -> core::Result<Self::Value, Self::Error> {
        let value = Self::new_instance(cx, class, payload)?;
        let error = set_error_name(cx, make_error(cx, message, false), name);
        let stack_name =
            JsString::new("stack").map_err(|_| Self::operation_error(cx, "stack string failed"))?;
        let mut exception = ptr::null();
        let error = unsafe { JSValueToObject(cx.ctx, error, &mut exception) };
        if !exception.is_null() || error.is_null() {
            return Err(if exception.is_null() {
                Self::operation_error(cx, "native Error is not an object")
            } else {
                exception
            });
        }
        let stack =
            unsafe { JSObjectGetProperty(cx.ctx, error, stack_name.as_raw(), &mut exception) };
        if !exception.is_null() {
            return Err(exception);
        }
        let object = unsafe { JSValueToObject(cx.ctx, value, &mut exception) };
        if !exception.is_null() || object.is_null() {
            return Err(if exception.is_null() {
                Self::operation_error(cx, "Error instance is not an object")
            } else {
                exception
            });
        }
        unsafe {
            JSObjectSetProperty(
                cx.ctx,
                object,
                stack_name.as_raw(),
                stack,
                PROPERTY_DONT_ENUM,
                &mut exception,
            )
        };
        if !exception.is_null() {
            return Err(exception);
        }
        Ok(value)
    }

    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: core::ClassId,
    ) -> Option<&'a (dyn Any + Send)> {
        let js_class = state_from_context(cx.ctx)
            .classes
            .lock()
            .ok()
            .and_then(|classes| classes.get(&class).map(|entry| entry.class))?;
        // SAFETY: `obj` belongs to `cx` and `js_class` is retained by the
        // adapter registry. Private data is read only after JSC proves this is
        // an instance of the requested wrapper class; the global object and
        // method objects therefore cannot be type-confused with ObjectPayload.
        if obj.is_null() || !unsafe { JSValueIsObjectOfClass(cx.ctx, obj, js_class) } {
            return None;
        }
        let raw = unsafe { JSObjectGetPrivate(obj.cast_mut()) }.cast::<ObjectPayload>();
        let holder = unsafe { raw.as_ref() }?;
        (holder.spec.id == class).then_some(holder.payload.as_ref())
    }

    fn undefined(cx: Self::Context<'_>) -> Self::Value {
        // SAFETY: cx is live.
        unsafe { JSValueMakeUndefined(cx.ctx) }
    }

    fn null(cx: Self::Context<'_>) -> Self::Value {
        // SAFETY: cx is live.
        unsafe { JSValueMakeNull(cx.ctx) }
    }

    fn number(cx: Self::Context<'_>, value: f64) -> core::Result<Self::Value, Self::Error> {
        // SAFETY: cx is live.
        let value = unsafe { JSValueMakeNumber(cx.ctx, value) };
        cx.scope.track(value);
        Ok(value)
    }

    fn boolean(cx: Self::Context<'_>, value: bool) -> Self::Value {
        unsafe { JSValueMakeBoolean(cx.ctx, value) }
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
        let error = make_error(cx, message, false);
        set_error_name(cx, error, "OperationError")
    }

    fn range_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        let error = make_error(cx, message, false);
        set_error_name(cx, error, "RangeError")
    }

    fn async_error_value(cx: Self::Context<'_>, name: &str, message: &str) -> Self::Value {
        let error = make_error(cx, message, false);
        cx.scope.track(error);
        set_error_name(cx, error, name)
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

    fn settle_deferreds(
        cx: Self::Context<'_>,
        settlements: Vec<core::DeferredSettlement<Self>>,
    ) -> core::Result<(), Self::Error> {
        if settlements.is_empty() {
            return Ok(());
        }
        let state = state_from_context(cx.ctx);
        let Some(trampoline) = state.trampoline() else {
            for (deferred, _) in settlements {
                state.finalizer.unprotect(cx.ctx, deferred.resolve());
                state.finalizer.unprotect(cx.ctx, deferred.reject());
            }
            return Err(Self::operation_error(
                cx,
                "settlement trampoline is unavailable",
            ));
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
        let result = (|| {
            let mut exception = ptr::null();
            // SAFETY: vector elements are live values in cx.
            let function_array = unsafe {
                JSObjectMakeArray(cx.ctx, functions.len(), functions.as_ptr(), &mut exception)
            };
            if !exception.is_null() {
                return Err(exception);
            }
            if function_array.is_null() {
                return Err(Self::operation_error(
                    cx,
                    "settlement function-array allocation failed",
                ));
            }
            // SAFETY: vector elements are live values in cx.
            let value_array =
                unsafe { JSObjectMakeArray(cx.ctx, values.len(), values.as_ptr(), &mut exception) };
            if !exception.is_null() {
                return Err(exception);
            }
            if value_array.is_null() {
                return Err(Self::operation_error(
                    cx,
                    "settlement value-array allocation failed",
                ));
            }
            let arguments = [function_array.cast_const(), value_array.cast_const()];
            // SAFETY: this is the single JavaScript entry for the entire batch,
            // producing one JSC microtask checkpoint on return (J2/F3).
            let value = unsafe {
                JSObjectCallAsFunction(
                    cx.ctx,
                    trampoline.cast_mut(),
                    ptr::null_mut(),
                    arguments.len(),
                    arguments.as_ptr(),
                    &mut exception,
                )
            };
            if !exception.is_null() {
                Err(exception)
            } else if value.is_null() {
                Err(Self::operation_error(
                    cx,
                    "settlement trampoline call failed",
                ))
            } else {
                Ok(())
            }
        })();
        for function in selected {
            state.finalizer.unprotect(cx.ctx, function);
        }
        result
    }

    fn drain_microtasks(_cx: Self::Context<'_>) -> core::Result<(), Self::Error> {
        // JSC exposes no public microtask pump (F1); microtasks drain when the
        // settlement trampoline's JavaScript frame returns (F2).
        Ok(())
    }

    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> core::Result<Self::Value, Self::Error> {
        let staging = make_engine_arraybuffer(cx, bytes.len())?;
        let staging_object = value_to_object(cx, staging)?;
        if !bytes.is_empty() {
            let mut exception = ptr::null();
            // SAFETY: staging is a private, engine-owned ArrayBuffer that has
            // never reached script. Pinning this staging remnant is permitted
            // by J8/J9; only its unpinned transfer product is returned.
            // This assumes untampered built-ins (trusted scripts, CLAUDE.md
            // invariant 8); a tampered `slice`/`transfer` could hand back a
            // script-visible buffer.
            let staging_ptr =
                unsafe { JSObjectGetArrayBufferBytesPtr(cx.ctx, staging_object, &mut exception) };
            if !exception.is_null() {
                return Err(exception);
            }
            let Some(staging_ptr) = NonNull::new(staging_ptr.cast::<u8>()) else {
                return Err(Self::operation_error(
                    cx,
                    "private staging ArrayBuffer has no bytes",
                ));
            };
            // SAFETY: the private staging allocation has bytes.len() bytes and
            // cannot overlap the borrowed foreign source slice.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), staging_ptr.as_ptr(), bytes.len()) };
        }
        call_own_method(cx, staging, "transfer")
    }

    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        value: Self::Value,
        out: Option<&mut [u8]>,
    ) -> core::Result<(), Self::Error> {
        // transfer() runs before any bytes-pointer access. On success `value`
        // is detached and `product` is a private copy (J8/A13).
        let product = call_own_method(cx, value, "transfer")?;
        if let Some(out) = out {
            if Self::arraybuffer_len(cx, product) != Some(out.len()) {
                return Err(Self::type_error(cx, "ArrayBuffer length mismatch"));
            }
            if !out.is_empty() {
                let product_object = value_to_object(cx, product)?;
                let mut exception = ptr::null();
                // SAFETY: product is the private ArrayBuffer returned by
                // transfer(); script can only see the now-detached `value`.
                // This assumes untampered built-ins (trusted scripts, CLAUDE.md
                // invariant 8); a tampered `slice`/`transfer` could hand back a
                // script-visible buffer.
                let product_ptr = unsafe {
                    JSObjectGetArrayBufferBytesPtr(cx.ctx, product_object, &mut exception)
                };
                if !exception.is_null() {
                    return Err(exception);
                }
                let Some(product_ptr) = NonNull::new(product_ptr.cast::<u8>()) else {
                    return Err(Self::operation_error(
                        cx,
                        "private transfer product has no bytes",
                    ));
                };
                // SAFETY: product and out have the same checked length and are
                // disjoint engine-owned and Rust-owned allocations.
                unsafe {
                    ptr::copy_nonoverlapping(product_ptr.as_ptr(), out.as_mut_ptr(), out.len())
                };
            }
        }
        Ok(())
    }

    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize> {
        let mut exception = ptr::null();
        // SAFETY: value belongs to cx. This predicate does not expose a bytes
        // pointer and therefore cannot pin the buffer.
        let kind = unsafe { JSValueGetTypedArrayType(cx.ctx, value, &mut exception) };
        if !exception.is_null() || kind != TYPED_ARRAY_TYPE_ARRAY_BUFFER {
            // JSC reports exceptions through the out parameter; consuming it
            // here means it is intentionally neither propagated nor pending.
            return None;
        }
        let object = value_to_object(cx, value).ok()?;
        exception = ptr::null();
        // SAFETY: the kind check above proved this is an ArrayBuffer. The byte
        // length accessor is the non-pinning E12 operation.
        let len = unsafe { JSObjectGetArrayBufferByteLength(cx.ctx, object, &mut exception) };
        exception.is_null().then_some(len)
    }

    fn arraybuffer_copy(cx: Self::Context<'_>, value: Self::Value) -> Option<Vec<u8>> {
        Self::arraybuffer_len(cx, value)?;
        let product = call_own_method(cx, value, "slice").ok()?;
        let len = Self::arraybuffer_len(cx, product)?;
        if len == 0 {
            return Some(Vec::new());
        }
        let product_object = value_to_object(cx, product).ok()?;
        let mut exception = ptr::null();
        // SAFETY: product is the private ArrayBuffer returned by slice(); the
        // script-reachable input `value` has never had its pointer requested.
        // This assumes untampered built-ins (trusted scripts, CLAUDE.md invariant
        // 8); a tampered `slice`/`transfer` could hand back a script-visible buffer.
        let product_ptr =
            unsafe { JSObjectGetArrayBufferBytesPtr(cx.ctx, product_object, &mut exception) };
        if !exception.is_null() {
            return None;
        }
        let product_ptr = NonNull::new(product_ptr.cast::<u8>())?;
        // SAFETY: the private slice product contains len initialized bytes.
        Some(unsafe { std::slice::from_raw_parts(product_ptr.as_ptr(), len).to_vec() })
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

fn make_engine_arraybuffer(cx: Context<'_>, len: usize) -> core::Result<JSValueRef, JSValueRef> {
    let global = Engine::global(cx);
    let constructor = Engine::get_property(cx, global, "ArrayBuffer")?;
    let constructor_object = value_to_object(cx, constructor)?;
    let len = Engine::number(cx, len as f64)?;
    let arguments = [len];
    let mut exception = ptr::null();
    // SAFETY: ArrayBuffer is the live built-in constructor from this context;
    // the numeric length argument belongs to the same call scope.
    // This assumes untampered built-ins (trusted scripts, CLAUDE.md invariant 8);
    // a tampered `slice`/`transfer` could hand back a script-visible buffer.
    let object = unsafe {
        JSObjectCallAsConstructor(
            cx.ctx,
            constructor_object,
            arguments.len(),
            arguments.as_ptr(),
            &mut exception,
        )
    };
    if !exception.is_null() {
        Err(exception)
    } else if object.is_null() {
        Err(Engine::operation_error(
            cx,
            "ArrayBuffer construction failed",
        ))
    } else {
        let value = object.cast_const();
        cx.scope.track(value);
        Ok(value)
    }
}

fn call_own_method(
    cx: Context<'_>,
    receiver: JSValueRef,
    name: &str,
) -> core::Result<JSValueRef, JSValueRef> {
    // Use the J11 engine primitives: two property reads plus one call, matching
    // the measured spike and avoiding eval for transfer() and slice().
    let method = Engine::get_property(cx, receiver, name)?;
    let call = Engine::get_property(cx, method, "call")?;
    Engine::call(cx, call, method, &[receiver])
}

fn make_bigint_fallback(cx: Context<'_>) -> core::Result<JSValueRef, JSValueRef> {
    let name = JsString::new("webgpuNativeIsBigInt")
        .map_err(|_| Engine::operation_error(cx, "BigInt helper name failed"))?;
    let parameter = JsString::new("v")
        .map_err(|_| Engine::operation_error(cx, "BigInt helper parameter failed"))?;
    let body = JsString::new("return typeof v === \"bigint\";")
        .map_err(|_| Engine::operation_error(cx, "BigInt helper body failed"))?;
    let source_url = JsString::new("webgpu-native-js-bigint-predicate.js")
        .map_err(|_| Engine::operation_error(cx, "BigInt helper source URL failed"))?;
    let parameter_names = [parameter.as_raw()];
    let mut exception = ptr::null();
    // SAFETY: every string and the context are live for this call. The body is
    // fixed first-party source under CLAUDE.md invariant 8 (trusted scripts),
    // not script input, and JSObjectMakeFunction copies the source strings.
    let helper = unsafe {
        JSObjectMakeFunction(
            cx.ctx,
            name.as_raw(),
            1,
            parameter_names.as_ptr(),
            body.as_raw(),
            source_url.as_raw(),
            1,
            &mut exception,
        )
    };
    if !exception.is_null() {
        Err(exception)
    } else if helper.is_null() {
        Err(Engine::operation_error(
            cx,
            "JSObjectMakeFunction(BigInt predicate) failed",
        ))
    } else {
        let value = helper.cast_const();
        cx.scope.track(value);
        Ok(value)
    }
}

fn is_bigint(cx: Context<'_>, value: JSValueRef) -> core::Result<bool, JSValueRef> {
    let state = state_from_context(cx.ctx);
    if let Some(predicate) = state.bigint_predicate {
        // SAFETY: dlsym verified the symbol is present, and both handles belong
        // to the live context represented by cx.
        return Ok(unsafe { predicate(cx.ctx, value) });
    }
    let helper = state
        .bigint_fallback
        .ok_or_else(|| Engine::operation_error(cx, "BigInt predicate is unavailable"))?;
    let global = Engine::global(cx);
    let result = Engine::call(cx, helper.0, global, &[value])?;
    // SAFETY: the helper's result belongs to cx and is a JavaScript boolean.
    Ok(unsafe { JSValueToBoolean(cx.ctx, result) })
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

fn set_error_name(cx: Context<'_>, error: JSValueRef, name: &str) -> JSValueRef {
    let mut exception = ptr::null();
    // SAFETY: error belongs to cx and make_error returns an object unless
    // allocation failed, in which case the conversion reports an exception.
    let object = unsafe { JSValueToObject(cx.ctx, error, &mut exception) };
    if !exception.is_null() || object.is_null() {
        return if exception.is_null() {
            error
        } else {
            exception
        };
    }
    let Ok(property) = JsString::new("name") else {
        return error;
    };
    let Ok(name) = JsString::new(name) else {
        return error;
    };
    // SAFETY: cx, object, and both strings are live for this property set.
    let value = unsafe { JSValueMakeString(cx.ctx, name.as_raw()) };
    unsafe {
        JSObjectSetProperty(
            cx.ctx,
            object,
            property.as_raw(),
            value,
            PROPERTY_NONE,
            &mut exception,
        )
    };
    if exception.is_null() {
        error
    } else {
        exception
    }
}

unsafe extern "C" fn wrapper_get_property(
    ctx: JSContextRef,
    object: JSObjectRef,
    property_name: JSStringRef,
    exception: *mut JSValueRef,
) -> JSValueRef {
    let scope = Scope::new(ctx);
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
            let value = {
                let classes = state_from_context(ctx)
                    .classes
                    .lock()
                    .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?;
                classes
                    .get(&holder.spec.id)
                    .and_then(|entry| {
                        entry
                            .methods
                            .iter()
                            .find(|cached| cached.name == method.name)
                    })
                    .map(|cached| cached.value.0)
            };
            // Error construction allocates in JSC and may synchronously run
            // finalizers, so never retain the class-registry guard across it.
            return value
                .ok_or_else(|| Engine::operation_error(cx, "class method is not registered"));
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
    let scope = Scope::new(ctx);
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
    let scope = Scope::new(ctx);
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
        match target {
            MethodTarget::Method(call) => call(cx, this_object.cast_const(), args),
            MethodTarget::Host(call) => {
                let args = args
                    .iter()
                    .copied()
                    .map(|value| jsc_host_value(cx, value))
                    .collect::<core::Result<Vec<_>, _>>()?;
                call(&args).map_err(|message| Engine::type_error(cx, &message))?;
                Ok(Engine::undefined(cx))
            }
        }
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

fn jsc_host_value(cx: Context<'_>, value: JSValueRef) -> core::Result<HostValue, JSValueRef> {
    if unsafe { JSValueIsUndefined(cx.ctx, value) } {
        return Ok(HostValue::Undefined);
    }
    if unsafe { JSValueIsNull(cx.ctx, value) } {
        return Ok(HostValue::Null);
    }
    if unsafe { JSValueIsBoolean(cx.ctx, value) } {
        return Ok(HostValue::Bool(unsafe { JSValueToBoolean(cx.ctx, value) }));
    }
    if unsafe { JSValueIsNumber(cx.ctx, value) } {
        let mut exception = ptr::null();
        let number = unsafe { JSValueToNumber(cx.ctx, value, &mut exception) };
        if !exception.is_null() {
            return Err(exception);
        }
        return Ok(HostValue::Number(number));
    }

    let mut exception = ptr::null();
    let string = unsafe { JSValueToStringCopy(cx.ctx, value, &mut exception) };
    if !exception.is_null() {
        return Err(exception);
    }
    let Some(string) = NonNull::new(string) else {
        return Err(Engine::operation_error(
            cx,
            "value string conversion failed",
        ));
    };
    Ok(HostValue::String(JsString(string).to_string_lossy()))
}

unsafe extern "C" fn wrapper_construct(
    ctx: JSContextRef,
    constructor: JSObjectRef,
    argument_count: usize,
    arguments: *const JSValueRef,
    exception: *mut JSValueRef,
) -> JSObjectRef {
    let scope = Scope::new(ctx);
    let cx = Context { ctx, scope: &scope };
    match catch_unwind(AssertUnwindSafe(|| {
        let state = state_from_context(ctx);
        let class = state
            .constructors
            .lock()
            .map_err(|_| Engine::operation_error(cx, "constructor registry is poisoned"))?
            .get(&(constructor as usize))
            .copied()
            .ok_or_else(|| Engine::operation_error(cx, "constructor is not registered"))?;
        let call = state
            .classes
            .lock()
            .map_err(|_| Engine::operation_error(cx, "class registry is poisoned"))?
            .get(&class)
            .ok_or_else(|| Engine::operation_error(cx, "constructor class is not registered"))?
            .spec
            .constructor
            .as_ref()
            .map(|constructor| constructor.call)
            .ok_or_else(|| Engine::type_error(cx, "Illegal constructor"))?;
        let args = if argument_count == 0 || arguments.is_null() {
            &[]
        } else {
            // SAFETY: JSC provides argument_count live values for this callback.
            unsafe { std::slice::from_raw_parts(arguments, argument_count) }
        };
        let value = call(cx, args)?;
        let mut conversion_exception = ptr::null();
        // SAFETY: value belongs to cx; constructors in core return objects.
        let object = unsafe { JSValueToObject(ctx, value, &mut conversion_exception) };
        if !conversion_exception.is_null() {
            return Err(conversion_exception);
        }
        if object.is_null() {
            return Err(Engine::operation_error(
                cx,
                "constructor returned no object",
            ));
        }
        let prototype_name = JsString::new("prototype")
            .map_err(|_| Engine::operation_error(cx, "prototype string failed"))?;
        let mut prototype_exception = ptr::null();
        // SAFETY: the constructor argument is the active new-target constructor
        // for this call, and both it and the returned object belong to ctx.
        let prototype = unsafe {
            JSObjectGetProperty(
                ctx,
                constructor,
                prototype_name.as_raw(),
                &mut prototype_exception,
            )
        };
        if !prototype_exception.is_null() {
            return Err(prototype_exception);
        }
        // SAFETY: prototype is the live constructor.prototype value from ctx.
        unsafe { JSObjectSetPrototype(ctx, object, prototype) };
        Ok((value, object))
    })) {
        Ok(Ok((value, object))) => {
            scope.escape(value);
            object
        }
        Ok(Err(error)) => {
            write_exception(exception, error);
            ptr::null_mut()
        }
        Err(_) => {
            write_exception(
                exception,
                Engine::operation_error(cx, "Rust callback panicked"),
            );
            ptr::null_mut()
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
        let ObjectPayload {
            spec,
            payload,
            finalizer,
        } = *holder;
        core::release_payload_values::<Engine>(payload.as_ref(), &mut |value| {
            finalizer.defer_unprotect(value);
        });
        // Core finalizers only move native handles into Environment's release
        // queue. They call neither a JSC context API nor webgpu.h (J6/J15/J21).
        (spec.finalizer)(payload, &finalizer.env);
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
    let target = Box::new(MethodTarget::Method(call));
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

fn make_interface_function(
    cx: Context<'_>,
    spec: &'static core::ClassSpec<Engine>,
    native_constructor: JSObjectRef,
) -> core::Result<JSObjectRef, JSValueRef> {
    let length = spec
        .constructor
        .as_ref()
        .map_or(0, |constructor| constructor.length);
    let parameters = (0..length)
        .map(|index| format!("arg{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let body = if spec.constructor.is_some() {
        format!(
            "return function {}({parameters}) {{ \
             if (!new.target) {{ throw new TypeError('Illegal constructor'); }} \
             return Reflect.construct(nativeConstructor, \
             Array.prototype.slice.call(arguments), new.target); \
             }};",
            spec.name
        )
    } else {
        format!(
            "return function {}() {{ \
             void nativeConstructor; \
             throw new TypeError('Illegal constructor'); \
             }};",
            spec.name
        )
    };
    let factory_name = JsString::new("createWebGpuInterface")
        .map_err(|_| Engine::operation_error(cx, "interface factory name failed"))?;
    let native_parameter = JsString::new("nativeConstructor")
        .map_err(|_| Engine::operation_error(cx, "interface factory parameter failed"))?;
    let body = JsString::new(&body)
        .map_err(|_| Engine::operation_error(cx, "interface function body failed"))?;
    let source = JsString::new("webgpu-native-js-interface.js")
        .map_err(|_| Engine::operation_error(cx, "interface source URL failed"))?;
    let parameter_names = [native_parameter.as_raw()];
    // SAFETY: all strings and the parameter pointer remain live for the call,
    // and JSObjectMakeFunction copies the source into a function owned by cx.
    let mut exception = ptr::null();
    let factory = unsafe {
        JSObjectMakeFunction(
            cx.ctx,
            factory_name.as_raw(),
            1,
            parameter_names.as_ptr(),
            body.as_raw(),
            source.as_raw(),
            1,
            &mut exception,
        )
    };
    if !exception.is_null() {
        return Err(exception);
    }
    if factory.is_null() {
        return Err(Engine::operation_error(
            cx,
            "JSObjectMakeFunction(interface factory) failed",
        ));
    }
    let arguments = [native_constructor.cast_const()];
    let mut exception = ptr::null();
    // SAFETY: factory and native_constructor belong to cx.
    let function = unsafe {
        JSObjectCallAsFunction(
            cx.ctx,
            factory,
            ptr::null_mut(),
            arguments.len(),
            arguments.as_ptr(),
            &mut exception,
        )
    };
    if !exception.is_null() {
        return Err(exception);
    }
    // SAFETY: the factory returns the JavaScript function expression above.
    let function = unsafe { JSValueToObject(cx.ctx, function, &mut exception) };
    if !exception.is_null() || function.is_null() {
        return Err(if exception.is_null() {
            Engine::operation_error(cx, "interface factory returned no function")
        } else {
            exception
        });
    }
    Ok(function)
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
    core::for_each_gpu_dispatch_entry!(__gpu_dispatch_from_ffi, ffi_wgpu)
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::{
        core, ffi_wgpu as wgpu, Context, Engine, JSObjectGetArrayBufferBytesPtr, JSValueRef,
        JSValueToObject, Runtime, Scope,
    };
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

    #[test]
    fn own_property_names_returns_only_own_enumerable_string_keys() {
        let runtime = Runtime::new().expect("JSC runtime");
        let object = runtime
            .eval(
                "(() => { const inherited = { inherited: 1 }; const value = Object.create(inherited); value.second = 2; value.first = 1; Object.defineProperty(value, 'hidden', { value: 0 }); value[Symbol('ignored')] = 3; return value; })()",
                "own-property-names.js",
            )
            .expect("object");
        let names = super::with_scope(runtime.raw_context(), |cx| {
            Engine::own_property_names(cx, object).expect("own names")
        });
        assert_eq!(names, ["second", "first"]);
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

    fn global_bool(runtime: &Runtime, name: &str) -> bool {
        let name = super::JsString::new(name).expect("global name");
        let global = unsafe { super::JSContextGetGlobalObject(runtime.raw_context()) };
        let mut exception = ptr::null();
        let value = unsafe {
            super::JSObjectGetProperty(runtime.raw_context(), global, name.as_raw(), &mut exception)
        };
        assert!(exception.is_null(), "global lookup threw");
        unsafe { super::JSValueToBoolean(runtime.raw_context(), value) }
    }

    #[test]
    fn register_host_function_converts_arguments_and_surfaces_errors() {
        let runtime = Runtime::new().expect("JSC runtime");
        let calls = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&calls);
        runtime
            .register_host_function("record", move |args| {
                captured.borrow_mut().push(args.to_vec());
                Ok(())
            })
            .expect("register record");
        runtime
            .register_host_function("fail", |_| Err("host rejected call".to_owned()))
            .expect("register fail");

        eval(
            &runtime,
            r#"
                record("text", 3.5, true, null, undefined, {
                    toString() { return "coerced object"; }
                });
                let caught = false;
                try { fail(); } catch (error) {
                    caught = error instanceof TypeError && error.message === "host rejected call";
                }
                if (!caught) throw new Error("host error was not a catchable TypeError");
            "#,
            "register-host-function.js",
        );
        assert_eq!(
            calls.borrow().as_slice(),
            &[vec![
                super::HostValue::String("text".to_owned()),
                super::HostValue::Number(3.5),
                super::HostValue::Bool(true),
                super::HostValue::Null,
                super::HostValue::Undefined,
                super::HostValue::String("coerced object".to_owned()),
            ]]
        );
    }

    #[test]
    fn register_host_function_contains_panics() {
        let runtime = Runtime::new().expect("JSC runtime");
        runtime
            .register_host_function("panicHost", |_| panic!("host panic"))
            .expect("register panic host");
        eval(
            &runtime,
            r#"
                let caught = false;
                try { panicHost(); } catch (error) {
                    caught = String(error).includes("Rust callback panicked");
                }
                if (!caught) throw new Error("host panic escaped its callback boundary");
            "#,
            "register-host-function-panic.js",
        );
    }

    #[test]
    fn native_render_bundle_is_class_checked_and_borrows_the_native_handle() {
        unsafe fn finish_bundle(
            _encoder: wgpu::WGPURenderBundleEncoder,
            _descriptor: *const wgpu::WGPURenderBundleDescriptor,
        ) -> wgpu::WGPURenderBundle {
            13_001usize as wgpu::WGPURenderBundle
        }
        unsafe fn release_bundle(_bundle: wgpu::WGPURenderBundle) {}

        let setup = native_setup();
        let mut dispatch = super::gpu_dispatch();
        dispatch.render_bundle_encoder_finish = finish_bundle;
        dispatch.render_bundle_release = release_bundle;
        let runtime = Runtime::new_with_dispatch(dispatch).expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        let bundle = runtime
            .eval(
                "device.createRenderBundleEncoder({ colorFormats: ['rgba8unorm'] }).finish()",
                "native-render-bundle.js",
            )
            .expect("create render bundle");
        let wrong = runtime
            .eval("device", "wrong-render-bundle.js")
            .expect("device");

        assert_eq!(
            runtime.native_render_bundle(bundle),
            Some(13_001usize as wgpu::WGPURenderBundle)
        );
        assert_eq!(runtime.native_render_bundle(wrong), None);
    }

    #[test]
    fn install_exposes_eager_non_constructible_interface_objects() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("interfaceDevice", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                const passTypeReady = typeof GPURenderPassEncoder === "function";
                const passMethodReady = "setBindGroup" in GPURenderPassEncoder.prototype;
                const constructibleDescriptor = Object.getOwnPropertyDescriptor(
                    GPURenderPassEncoder.prototype,
                    "constructor"
                );
                const nonConstructibleDescriptor = Object.getOwnPropertyDescriptor(
                    GPUSupportedLimits.prototype,
                    "constructor"
                );
                const descriptorIsWebIdl = (descriptor, interfaceObject) =>
                    descriptor.value === interfaceObject && descriptor.writable === true &&
                    descriptor.enumerable === false && descriptor.configurable === true;
                const constructibleConstructorDescriptor = descriptorIsWebIdl(
                    constructibleDescriptor,
                    GPURenderPassEncoder
                );
                const nonConstructibleConstructorDescriptor = descriptorIsWebIdl(
                    nonConstructibleDescriptor,
                    GPUSupportedLimits
                );
                let callError;
                let constructError;
                try { GPUSupportedLimits(); } catch (error) { callError = error; }
                try { new GPUSupportedLimits(); } catch (error) { constructError = error; }
                const encoder = interfaceDevice.createCommandEncoder();
                const pass = encoder.beginComputePass();
                const instanceReady = pass instanceof GPUComputePassEncoder;
                pass.end();
                encoder.finish();
                if (!(passTypeReady && passMethodReady && constructibleConstructorDescriptor &&
                    nonConstructibleConstructorDescriptor && instanceReady &&
                    callError instanceof TypeError && callError.message.includes("Illegal constructor") &&
                    constructError instanceof TypeError && constructError.message.includes("Illegal constructor"))) {
                    throw new Error("eager interface-object invariants failed: " + [
                        passTypeReady,
                        passMethodReady,
                        JSON.stringify(constructibleDescriptor),
                        JSON.stringify(nonConstructibleDescriptor),
                        instanceReady,
                        callError && callError.name,
                        callError && callError.message,
                        constructError && constructError.name,
                        constructError && constructError.message
                    ].join("|"));
                }
            "#,
            "eager-interface-objects.js",
        );
    }

    fn tick_until<F>(
        runtime: &Runtime,
        instance: wgpu::WGPUInstance,
        deadline_ms: u64,
        mut condition: F,
    ) where
        F: FnMut() -> bool,
    {
        let deadline = Instant::now() + Duration::from_millis(deadline_ms);
        loop {
            unsafe { runtime.tick(instance) }.expect("tick while waiting for async completion");
            if condition() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "async condition was not met within {deadline_ms}ms while ticking the runtime"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn assert_shared_j17_parity_script_matches_expected_output(force_bigint_fallback: bool) {
        const SCRIPT: &str = include_str!("../../../tests/parity/parity.js");
        const EXPECTED: &str = include_str!("../../../tests/parity/expected.txt");

        let setup = native_setup();
        let runtime = if force_bigint_fallback {
            Runtime::new_forcing_bigint_fallback().expect("JSC fallback runtime")
        } else {
            Runtime::new().expect("JSC runtime")
        };
        assert_eq!(
            runtime.state.bigint_predicate.is_none(),
            force_bigint_fallback,
            "the requested BigInt detection path must be active"
        );
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval(&runtime, SCRIPT, "tests/parity/parity.js");
        runtime
            .forward_uncaptured_error(
                setup.device,
                wgpu::WGPUErrorType_WGPUErrorType_Validation,
                "parity uncaptured",
            )
            .expect("forward parity uncaptured");
        runtime
            .forward_device_lost(
                setup.device,
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "parity loss",
            )
            .expect("forward parity loss");

        tick_until(&runtime, setup.instance, 5000, || {
            global_bool(&runtime, "parityDone")
        });

        let joined = runtime
            .eval("globalThis.parityLog.join('\\n')", "tests/parity/join.js")
            .expect("join parity log");
        let actual = format!(
            "{}\n",
            super::value_to_string(runtime.raw_context(), joined)
        );
        assert_eq!(actual, EXPECTED);
    }

    #[test]
    fn shared_j17_parity_script_matches_expected_output() {
        assert!(
            super::bigint_predicate().is_some(),
            "this gate requires the modern JSC runtime symbol"
        );
        assert_shared_j17_parity_script_matches_expected_output(false);
    }

    #[test]
    fn shared_j17_parity_script_matches_expected_output_with_bigint_fallback() {
        assert_shared_j17_parity_script_matches_expected_output(true);
    }

    #[test]
    fn scope_protects_on_track_unprotects_on_drop_and_escape_skips_drop_release() {
        let runtime = Runtime::new().expect("JSC runtime");
        let counters = Arc::clone(&runtime.state.finalizer);
        let protect_start = counters.protect_count.load(Ordering::Relaxed);
        let unprotect_start = counters.unprotect_count.load(Ordering::Relaxed);
        {
            let scope = Scope::new(runtime.raw_context());
            let cx = Context {
                ctx: runtime.raw_context(),
                scope: &scope,
            };
            let global = Engine::global(cx);
            assert!(!Engine::is_undefined(cx, global));
            assert_eq!(scope.values.borrow().as_slice(), &[global]);
            assert_eq!(
                counters.protect_count.load(Ordering::Relaxed),
                protect_start + 1
            );
            assert_eq!(
                counters.unprotect_count.load(Ordering::Relaxed),
                unprotect_start
            );
        }
        assert_eq!(
            counters.unprotect_count.load(Ordering::Relaxed),
            unprotect_start + 1
        );

        let unprotect_before_escape = counters.unprotect_count.load(Ordering::Relaxed);
        let unprotect_after_escape;
        {
            let scope = Scope::new(runtime.raw_context());
            let cx = Context {
                ctx: runtime.raw_context(),
                scope: &scope,
            };
            let global = Engine::global(cx);
            scope.escape(global);
            assert!(scope.values.borrow().is_empty());
            unprotect_after_escape = counters.unprotect_count.load(Ordering::Relaxed);
            assert_eq!(unprotect_after_escape, unprotect_before_escape + 1);
        }
        assert_eq!(
            counters.unprotect_count.load(Ordering::Relaxed),
            unprotect_after_escape,
            "scope drop must skip the protection already removed by escape"
        );
    }

    #[test]
    fn mapping_primitives_stage_copy_detach_and_read_without_pinning_input() {
        let runtime = Runtime::new().expect("JSC runtime");
        super::with_scope(runtime.raw_context(), |cx| {
            let value = Engine::new_arraybuffer_copy(cx, &[1, 2, 3]).expect("staged copy");
            assert_eq!(Engine::arraybuffer_len(cx, value), Some(3));
            assert_eq!(Engine::arraybuffer_copy(cx, value), Some(vec![1, 2, 3]));
            let mut out = [0_u8; 3];
            Engine::detach_arraybuffer(cx, value, Some(&mut out)).expect("detach and copy");
            assert_eq!(out, [1, 2, 3]);
            assert_eq!(Engine::arraybuffer_len(cx, value), Some(0));
            let non_buffer = Engine::undefined(cx);
            assert_eq!(Engine::arraybuffer_len(cx, non_buffer), None);
            assert_eq!(Engine::arraybuffer_copy(cx, non_buffer), None);
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
    fn buffer_label_converts_a_lone_surrogate_to_one_replacement_character() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                const surrogateBuffer = device.createBuffer({
                    size: 4,
                    usage: 8,
                    label: "a\uD800b"
                });
                if (surrogateBuffer.label !== "a\uFFFDb" ||
                    surrogateBuffer.label.length !== 3) {
                    throw new Error("lone surrogate was not converted once");
                }
                surrogateBuffer.destroy();
            "#,
            "lone-surrogate-label.js",
        );
        runtime.clear_global("device").expect("clear device");
    }

    #[test]
    fn detached_device_method_rejects_every_incompatible_receiver_with_type_error() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                var f = device.createBuffer;
                var otherWrapperOfDifferentClass = device.queue;
                function mustThrowTypeError(call, label) {
                    var caught = false;
                    try { call(); }
                    catch (error) { caught = error instanceof TypeError; }
                    if (!caught) throw new Error(label + ' did not throw TypeError');
                }
                mustThrowTypeError(function () { f(); }, 'detached call');
                mustThrowTypeError(function () { f.call(f); }, 'method receiver');
                mustThrowTypeError(
                    function () { f.call(otherWrapperOfDifferentClass); },
                    'different wrapper receiver'
                );
            "#,
            "payload-class-identity.js",
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
        runtime
            .eval_unprotected(
                r#"
                if (device.createBuffer !== device.createBuffer) {
                    throw new Error('method identity was not stable');
                }
                void device.queue;
            "#,
                "protection-balance.js",
            )
            .expect("method identity and payload cache");
        runtime.clear_global("device").expect("clear device");
        // F5 means ordinary JSC collection cannot make this a clean teardown.
        // Invoke the real owner finalizer directly, then prevent context release
        // from finalizing the already-consumed ObjectPayload a second time.
        unsafe { super::wrapper_finalize(device.cast_mut()) };
        assert!(unsafe { super::JSObjectSetPrivate(device.cast_mut(), ptr::null_mut()) });
        let _ = runtime.drain_releases().expect("owner release drain");
        let class_method_count = counters.class_method_protect_count.load(Ordering::Relaxed);
        assert!(
            class_method_count > 0,
            "registered class methods must have their own teardown lifetime"
        );
        drop(runtime);
        assert_eq!(
            counters
                .teardown_mop_up_unprotect_count
                .load(Ordering::Relaxed),
            0,
            "clean teardown must not force-unprotect an owner's value"
        );
        assert_eq!(
            counters
                .class_method_teardown_unprotect_count
                .load(Ordering::Relaxed),
            class_method_count,
            "intentional class-level protections are released and counted separately"
        );
        assert_eq!(
            counters.protect_count.load(Ordering::Relaxed),
            counters.unprotect_count.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn real_wrapper_finalize_defers_payload_unprotects_until_tick() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        runtime
            .eval_unprotected(
                "void device.createBuffer; void device.queue;",
                "real-wrapper-finalize-setup.js",
            )
            .expect("read class method and cache payload value");
        let class_methods = runtime
            .state
            .classes
            .lock()
            .expect("class registry")
            .values()
            .flat_map(|entry| entry.methods.iter().map(|method| method.value))
            .collect::<Vec<_>>();
        runtime.clear_global("device").expect("clear device");
        assert!(runtime
            .state
            .finalizer
            .deferred_unprotects
            .lock()
            .expect("deferred queue")
            .is_empty());

        unsafe { super::wrapper_finalize(device.cast_mut()) };
        assert!(unsafe { super::JSObjectSetPrivate(device.cast_mut(), ptr::null_mut()) });
        let deferred = runtime
            .state
            .finalizer
            .deferred_unprotects
            .lock()
            .expect("deferred queue");
        assert!(
            !deferred.is_empty(),
            "the production finalizer must defer the payload value"
        );
        assert!(
            deferred.iter().all(|value| !class_methods.contains(value)),
            "class methods must never enter a wrapper finalizer's deferred queue"
        );
        drop(deferred);

        unsafe { runtime.tick(setup.instance) }.expect("tick drains deferred unprotects");
        assert!(runtime
            .state
            .finalizer
            .deferred_unprotects
            .lock()
            .expect("deferred queue")
            .is_empty());
    }

    fn deliberately_pin_script_visible_arraybuffer(runtime: &Runtime, name: &str) {
        let value = runtime
            .eval_unprotected(name, "pin-script-visible-arraybuffer.js")
            .expect("script-visible ArrayBuffer");
        let mut exception = ptr::null();
        // SAFETY: J18 deliberately violates J9 in test code only. Taking the C
        // bytes pointer of this script-visible mapped range pins it so the A12
        // detach-verification guard can be observed firing.
        let object = unsafe { JSValueToObject(runtime.raw_context(), value, &mut exception) };
        assert!(exception.is_null());
        assert!(!object.is_null());
        let bytes = unsafe {
            JSObjectGetArrayBufferBytesPtr(runtime.raw_context(), object, &mut exception)
        };
        assert!(exception.is_null());
        assert!(!bytes.is_null());
    }

    #[test]
    fn a12_red_demo_pinned_script_visible_range_fails_detach_verification() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            r#"
                var redMapped = device.createBuffer({
                    size: 8,
                    usage: 2,
                    mappedAtCreation: true
                });
                var redRange = redMapped.getMappedRange();
                new Uint8Array(redRange)[0] = 37;
            "#,
            "a12-pinning-setup.js",
        );
        deliberately_pin_script_visible_arraybuffer(&runtime, "redRange");

        let error = runtime
            .eval("redMapped.unmap();", "a12-pinning-unmap.js")
            .expect_err("pinned range must fail core detach verification");
        assert!(
            format!("{error:?}").contains("mapped range detach failed"),
            "unexpected hard error: {error:?}"
        );
        eval(
            &runtime,
            "if (redRange.byteLength !== 8) throw new Error('transfer unexpectedly detached');",
            "a12-pinning-check.js",
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
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval(
            &runtime,
            "var firstAdapter; var firstAdapterReady = false; gpu.requestAdapter().then(a => { firstAdapter = a; firstAdapterReady = true; });",
            "settle-prototype.js",
        );
        tick_until(&runtime, setup.instance, 5000, || {
            global_bool(&runtime, "firstAdapterReady")
        });
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
        tick_until(&runtime, setup.instance, 5000, || {
            let value = runtime
                .eval("order.length === 4", "settle-order-poll.js")
                .expect("poll settlement order");
            unsafe { super::JSValueToBoolean(runtime.raw_context(), value) }
        });
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
    fn tick_surfaces_settlement_trampoline_exception_without_leaking_resolvers() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let old_trampoline = runtime
            .state
            .take_trampoline()
            .expect("installed trampoline");
        runtime
            .state
            .finalizer
            .unprotect(runtime.raw_context(), old_trampoline);
        let throwing_trampoline = runtime
            .eval_unprotected(
                "(function() { throw new Error('settlement trampoline boom'); })",
                "throwing-settle-trampoline.js",
            )
            .expect("throwing trampoline");
        runtime
            .state
            .finalizer
            .protect(runtime.raw_context(), throwing_trampoline);
        runtime.state.set_trampoline(throwing_trampoline);

        let deferred = super::with_scope(runtime.raw_context(), |cx| {
            Engine::new_promise(cx).expect("promise").1
        });
        let resolve = deferred.resolve();
        let reject = deferred.reject();
        let direct_error = super::with_scope(runtime.raw_context(), |cx| {
            Engine::settle_deferreds(cx, vec![(deferred, Ok(Engine::undefined(cx)))])
        });
        assert!(direct_error.is_err(), "throwing trampoline must fail");
        let protected = runtime
            .state
            .finalizer
            .protected
            .lock()
            .expect("protection ledger");
        assert!(
            protected
                .iter()
                .all(|value| value.0 != resolve && value.0 != reject),
            "both deferred resolver protections must be released on failure"
        );
        drop(protected);

        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        eval(
            &runtime,
            "void gpu.requestAdapter();",
            "throwing-settle-request.js",
        );

        let error = unsafe { runtime.tick(setup.instance) }
            .expect_err("trampoline exception must escape tick");

        assert!(
            format!("{error:?}").contains("settlement trampoline boom"),
            "unexpected tick error: {error:?}"
        );
    }

    #[test]
    #[ignore = "manual trampoline cost measurement"]
    fn measure_batched_trampoline_against_direct_settlement_calls() {
        const TICKS: usize = 200;
        const SETTLEMENTS_PER_TICK: usize = 8;

        fn deferreds(runtime: &Runtime) -> Vec<core::Deferred<Engine>> {
            (0..TICKS * SETTLEMENTS_PER_TICK)
                .map(|_| {
                    super::with_scope(runtime.raw_context(), |cx| {
                        Engine::new_promise(cx).expect("promise").1
                    })
                })
                .collect()
        }

        let runtime = Runtime::new().expect("JSC runtime");
        let batched = deferreds(&runtime);
        let direct = deferreds(&runtime);

        let batched_start = std::time::Instant::now();
        for batch in batched.chunks(SETTLEMENTS_PER_TICK) {
            super::with_scope(runtime.raw_context(), |cx| {
                let settlements = batch
                    .iter()
                    .map(|deferred| {
                        (
                            core::Deferred::new(deferred.resolve(), deferred.reject()),
                            Ok(Engine::undefined(cx)),
                        )
                    })
                    .collect();
                Engine::settle_deferreds(cx, settlements).expect("batched settlement");
            });
        }
        let batched_elapsed = batched_start.elapsed();

        let direct_start = std::time::Instant::now();
        for deferred in direct {
            let argument = unsafe { super::JSValueMakeUndefined(runtime.raw_context()) };
            let mut exception = ptr::null();
            let result = unsafe {
                super::JSObjectCallAsFunction(
                    runtime.raw_context(),
                    deferred.resolve().cast_mut(),
                    ptr::null_mut(),
                    1,
                    &argument,
                    &mut exception,
                )
            };
            runtime
                .state
                .finalizer
                .unprotect(runtime.raw_context(), deferred.resolve());
            runtime
                .state
                .finalizer
                .unprotect(runtime.raw_context(), deferred.reject());
            assert!(exception.is_null(), "direct resolver call threw");
            assert!(!result.is_null(), "direct resolver call returned null");
        }
        let direct_elapsed = direct_start.elapsed();

        println!(
            "trampoline cost: {TICKS} ticks x {SETTLEMENTS_PER_TICK} settlements: batched={batched_elapsed:?}, direct={direct_elapsed:?}"
        );
    }

    #[test]
    fn i1_construct_tracks_success_and_exception() {
        let runtime = Runtime::new().expect("JSC runtime");
        let constructor = runtime
            .eval(
                "(class Box { constructor(value) { if (value < 0) throw new Error('construct boom'); this.value = value; } })",
                "construct-primitive.js",
            )
            .expect("constructor");
        super::with_scope(runtime.raw_context(), |cx| {
            let value = Engine::number(cx, 42.0).expect("value");
            let object = Engine::construct(cx, constructor, &[value]).expect("construct");
            let stored = Engine::get_property(cx, object, "value").expect("stored value");
            assert_eq!(Engine::to_f64(cx, stored).expect("stored number"), 42.0);
            let negative = Engine::number(cx, -1.0).expect("negative");
            let error = Engine::construct(cx, constructor, &[negative]).expect_err("must throw");
            assert!(super::value_to_string(runtime.raw_context(), error).contains("construct boom"));
        });
    }

    #[test]
    fn j18_j1_direct_resolver_runs_then_before_jsc_call_returns() {
        let runtime = Runtime::new().expect("JSC runtime");
        let (promise, deferred) = super::with_scope(runtime.raw_context(), |cx| {
            let (promise, deferred) = Engine::new_promise(cx).expect("promise");
            cx.scope.escape(promise);
            (promise, deferred)
        });
        runtime
            .set_global_value("j18Promise", promise)
            .expect("set promise");
        runtime
            .eval_unprotected(
                "var j18Ran = false; j18Promise.then(function () { j18Ran = true; });",
                "j18-j1-jsc-setup.js",
            )
            .expect("install continuation");

        let argument = unsafe { super::JSValueMakeUndefined(runtime.raw_context()) };
        let mut exception = ptr::null();
        // J18/J1 counterfactual: bypass settlement machinery and call the
        // resolver directly. JSC's F2 checkpoint runs the continuation before
        // this C call returns, which is the divergence J1 exists to neutralize.
        let result = unsafe {
            super::JSObjectCallAsFunction(
                runtime.raw_context(),
                deferred.resolve().cast_mut(),
                ptr::null_mut(),
                1,
                &argument,
                &mut exception,
            )
        };
        assert!(exception.is_null(), "direct resolver call threw");
        assert!(!result.is_null(), "direct resolver call returned null");
        assert!(
            global_bool(&runtime, "j18Ran"),
            "JSC continuation must already have run when resolver call returns"
        );
        runtime
            .state
            .finalizer
            .unprotect(runtime.raw_context(), deferred.resolve());
        runtime
            .state
            .finalizer
            .unprotect(runtime.raw_context(), deferred.reject());
        runtime.clear_global("j18Promise").expect("clear promise");
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
    fn gpu_pipeline_error_is_a_constructible_error_subclass() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let _device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        eval(
            &runtime,
            r#"
                const validation = new GPUPipelineError("pipeline failed", {
                    reason: "validation"
                });
                const internal = new GPUPipelineError(undefined, { reason: "internal" });
                let missingIsTypeError = false;
                let invalidIsTypeError = false;
                try { new GPUPipelineError("bad", {}); }
                catch (error) { missingIsTypeError = error instanceof TypeError; }
                try { new GPUPipelineError("bad", { reason: "device-lost" }); }
                catch (error) { invalidIsTypeError = error instanceof TypeError; }
                if (validation.name !== "GPUPipelineError" ||
                    validation.message !== "pipeline failed" ||
                    validation.reason !== "validation" ||
                    internal.message !== "" || internal.reason !== "internal" ||
                    !(validation instanceof GPUPipelineError) ||
                    !(validation instanceof DOMException) || !(validation instanceof Error) ||
                    typeof validation.stack !== typeof new Error("pipeline failed").stack ||
                    !missingIsTypeError || !invalidIsTypeError) {
                    throw new Error("GPUPipelineError contract mismatch");
                }
            "#,
            "gpu-pipeline-error.js",
        );
    }

    fn assert_bigint_size_throws_a_catchable_type_error(force_bigint_fallback: bool) {
        let setup = native_setup();
        let runtime = if force_bigint_fallback {
            Runtime::new_forcing_bigint_fallback().expect("JSC fallback runtime")
        } else {
            Runtime::new().expect("JSC runtime")
        };
        assert_eq!(
            runtime.state.bigint_predicate.is_none(),
            force_bigint_fallback,
            "the requested BigInt detection path must be active"
        );
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
    fn bigint_size_throws_a_catchable_type_error() {
        assert!(
            super::bigint_predicate().is_some(),
            "this gate requires the modern JSC runtime symbol"
        );
        assert_bigint_size_throws_a_catchable_type_error(false);
    }

    #[test]
    fn bigint_size_throws_a_catchable_type_error_with_fallback() {
        assert_bigint_size_throws_a_catchable_type_error(true);
    }

    #[test]
    fn shared_error_class_constructor_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            include_str!("../../../tests/error-classes.js"),
            "error-classes.js",
        );
    }

    #[test]
    fn shared_named_async_rejection_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            include_str!("../../../tests/error-rejection.js"),
            "error-rejection.js",
        );
        tick_until(&runtime, setup.instance, 5000, || {
            global_bool(&runtime, "errorRejectionDone")
        });
    }

    #[test]
    fn shared_device_event_script_passes() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
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
        tick_until(&runtime, setup.instance, 5000, || {
            global_bool(&runtime, "uncapturedEventPassed")
                && global_bool(&runtime, "uncapturedListenerPassed")
                && global_bool(&runtime, "deviceLostPassed")
        });
        eval(
            &runtime,
            "if (!uncapturedEventPassed || !uncapturedListenerPassed || !deviceLostPassed) throw new Error('device event callback did not run');",
            "device-events-check.js",
        );
    }

    #[test]
    fn bind_group_layout_and_resource_arms_follow_the_shared_script() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        eval(
            &runtime,
            include_str!("../../../tests/bind-group-resources.js"),
            "tests/bind-group-resources.js",
        );
    }

    #[test]
    fn promise_continuation_can_reenter_device_method() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("JSC runtime");
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
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
        tick_until(&runtime, setup.instance, 5000, || {
            global_bool(&runtime, "reentered")
        });
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
            constructor: None,
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
        // If the callback unwind guard is deleted, the failure mode is an
        // unattributed process abort, inherent to a panic crossing `extern "C"`.
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
            let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
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
