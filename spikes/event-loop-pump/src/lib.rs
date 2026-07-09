#![warn(missing_docs)]

//! Spike proving that `wgpuInstanceProcessEvents` and QuickJS microtask
//! draining are separate queues that must be pumped in order.

use std::ffi::{c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::Mutex;
use std::thread::ThreadId;

use ffi::native as wgpu;

#[allow(
    dead_code,
    clippy::upper_case_acronyms,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod qjs {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

fn js_undefined() -> qjs::JSValue {
    qjs::JSValue {
        u: qjs::JSValueUnion { int32: 0 },
        tag: qjs::JS_TAG_UNDEFINED.into(),
    }
}

fn js_exception() -> qjs::JSValue {
    qjs::JSValue {
        u: qjs::JSValueUnion { int32: 0 },
        tag: qjs::JS_TAG_EXCEPTION.into(),
    }
}

/// Crate-local result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the event-loop pump spike.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// A string passed to C contained an interior NUL byte.
    InteriorNul,
    /// A C API returned null where a live handle was required.
    Null(&'static str),
    /// QuickJS raised an exception with the contained message.
    Exception(String),
    /// QuickJS reported a pending-job execution failure.
    PendingJobFailed,
    /// QuickJS boolean conversion failed.
    ToBoolFailed,
    /// Installing a JavaScript global failed.
    SetGlobalFailed(&'static str),
    /// Calling a JavaScript function failed.
    CallFailed(&'static str),
    /// A WebGPU request completed with a non-success status.
    RequestAdapterFailed(wgpu::WGPURequestAdapterStatus),
    /// The callback panicked and the extern boundary caught it.
    CallbackPanicked,
    /// The global request-adapter test slot was poisoned.
    SlotPoisoned,
}

/// A minimal QuickJS runtime/context pair.
pub struct QuickJs {
    rt: NonNull<qjs::JSRuntime>,
    ctx: NonNull<qjs::JSContext>,
}

impl QuickJs {
    /// Creates a QuickJS runtime and context.
    pub fn new() -> Result<Self> {
        let rt = unsafe { qjs::JS_NewRuntime() };
        let rt = NonNull::new(rt).ok_or(Error::Null("JS_NewRuntime"))?;
        let ctx = unsafe { qjs::JS_NewContext(rt.as_ptr()) };
        let ctx = NonNull::new(ctx).ok_or(Error::Null("JS_NewContext"))?;
        Ok(Self { rt, ctx })
    }

    /// Returns the raw QuickJS context.
    #[must_use]
    pub fn ctx(&self) -> *mut qjs::JSContext {
        self.ctx.as_ptr()
    }

    /// Returns whether QuickJS has a pending microtask/job.
    #[must_use]
    pub fn is_job_pending(&self) -> bool {
        unsafe { qjs::JS_IsJobPending(self.rt.as_ptr()) }
    }

    /// Drains the QuickJS microtask queue.
    pub fn drain_microtasks(&self) -> Result<usize> {
        let mut ran = 0;
        while self.is_job_pending() {
            let mut job_ctx = ptr::null_mut();
            let rc = unsafe { qjs::JS_ExecutePendingJob(self.rt.as_ptr(), &mut job_ctx) };
            match rc {
                1.. => ran += 1,
                0 => break,
                _ => return Err(Error::PendingJobFailed),
            }
        }
        Ok(ran)
    }

    /// Evaluates JavaScript and frees the returned value.
    pub fn eval(&self, script: &str) -> Result<()> {
        let value = self.eval_value(script)?;
        unsafe { qjs::JS_FreeValue(self.ctx(), value) };
        Ok(())
    }

    /// Evaluates JavaScript and converts the result to a boolean.
    pub fn eval_bool(&self, script: &str) -> Result<bool> {
        let value = self.eval_value(script)?;
        let result = unsafe { qjs::JS_ToBool(self.ctx(), value) };
        unsafe { qjs::JS_FreeValue(self.ctx(), value) };
        match result {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::ToBoolFailed),
        }
    }

    fn eval_value(&self, script: &str) -> Result<qjs::JSValue> {
        let script = CString::new(script).map_err(|_| Error::InteriorNul)?;
        let value = unsafe {
            qjs::JS_Eval(
                self.ctx(),
                script.as_ptr(),
                script.as_bytes().len(),
                c"event-loop-pump".as_ptr(),
                qjs::JS_EVAL_TYPE_GLOBAL as i32,
            )
        };
        if unsafe { qjs::JS_IsException(value) } {
            return self.take_exception("JS_Eval");
        }
        Ok(value)
    }

    fn set_global_value(&self, name: &'static CStr, value: qjs::JSValue) -> Result<()> {
        let global = unsafe { qjs::JS_GetGlobalObject(self.ctx()) };
        let rc = unsafe { qjs::JS_SetPropertyStr(self.ctx(), global, name.as_ptr(), value) };
        unsafe { qjs::JS_FreeValue(self.ctx(), global) };
        if rc < 0 {
            return Err(Error::SetGlobalFailed("JS_SetPropertyStr"));
        }
        Ok(())
    }

    fn take_exception<T>(&self, fallback: &'static str) -> Result<T> {
        let exception = unsafe { qjs::JS_GetException(self.ctx()) };
        let raw = unsafe { qjs::JS_ToCString(self.ctx(), exception) };
        let message = if raw.is_null() {
            fallback.to_owned()
        } else {
            let text = unsafe { CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned();
            unsafe { qjs::JS_FreeCString(self.ctx(), raw) };
            text
        };
        unsafe { qjs::JS_FreeValue(self.ctx(), exception) };
        Err(Error::Exception(message))
    }
}

impl Drop for QuickJs {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeContext(self.ctx.as_ptr());
            qjs::JS_FreeRuntime(self.rt.as_ptr());
        }
    }
}

/// A headless WebGPU instance.
pub struct Instance {
    raw: NonNull<wgpu::WGPUInstanceImpl>,
}

impl Instance {
    /// Creates a headless instance using backend defaults.
    pub fn new_headless() -> Result<Self> {
        let raw = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        let raw = NonNull::new(raw).ok_or(Error::Null("wgpuCreateInstance"))?;
        Ok(Self { raw })
    }

    /// Returns the raw WebGPU instance handle.
    #[must_use]
    pub fn raw(&self) -> wgpu::WGPUInstance {
        self.raw.as_ptr()
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { wgpu::wgpuInstanceRelease(self.raw()) };
    }
}

/// Processes WebGPU events and then drains QuickJS microtasks.
pub fn tick(instance: &Instance, js: &QuickJs) -> Result<usize> {
    process_events(instance);
    js.drain_microtasks()
}

/// Processes only the WebGPU event queue, omitting QuickJS microtasks.
pub fn tick_without_microtask_drain(instance: &Instance) {
    process_events(instance);
}

fn process_events(instance: &Instance) {
    unsafe { wgpu::wgpuInstanceProcessEvents(instance.raw()) };
}

struct RequestState {
    ctx: *mut qjs::JSContext,
    resolve: qjs::JSValue,
    reject: qjs::JSValue,
    callback_count: usize,
    resolved: bool,
    panicked: bool,
    thread_id: Option<ThreadId>,
}

impl RequestState {
    fn new(ctx: *mut qjs::JSContext, resolve: qjs::JSValue, reject: qjs::JSValue) -> Self {
        Self {
            ctx,
            resolve,
            reject,
            callback_count: 0,
            resolved: false,
            panicked: false,
            thread_id: None,
        }
    }
}

impl Drop for RequestState {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeValue(self.ctx, self.resolve);
            qjs::JS_FreeValue(self.ctx, self.reject);
        }
    }
}

/// A pending request-adapter promise plus callback observations.
pub struct AdapterRequest {
    promise: qjs::JSValue,
    state: Box<RequestState>,
}

impl AdapterRequest {
    /// Installs the promise as a JavaScript global.
    pub fn install_promise_global(&self, js: &QuickJs, name: &'static CStr) -> Result<()> {
        let promise = unsafe { qjs::JS_DupValue(js.ctx(), self.promise) };
        js.set_global_value(name, promise)
    }

    /// Returns how many times the WebGPU callback has run.
    #[must_use]
    pub fn callback_count(&self) -> usize {
        self.state.callback_count
    }

    /// Returns whether the Promise resolver has been called.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.state.resolved
    }

    /// Returns whether the extern callback caught a panic.
    #[must_use]
    pub fn callback_panicked(&self) -> bool {
        self.state.panicked
    }

    /// Returns the thread that executed the WebGPU callback.
    #[must_use]
    pub fn callback_thread_id(&self) -> Option<ThreadId> {
        self.state.thread_id
    }

    fn state_ptr(&mut self) -> *mut RequestState {
        self.state.as_mut()
    }
}

impl Drop for AdapterRequest {
    fn drop(&mut self) {
        unsafe { qjs::JS_FreeValue(self.state.ctx, self.promise) };
    }
}

/// Calls `wgpuInstanceRequestAdapter` and bridges completion to a QuickJS Promise.
pub fn request_adapter_promise(instance: &Instance, js: &QuickJs) -> Result<AdapterRequest> {
    let mut resolving_funcs = [js_undefined(), js_undefined()];
    let promise = unsafe { qjs::JS_NewPromiseCapability(js.ctx(), resolving_funcs.as_mut_ptr()) };
    if unsafe { qjs::JS_IsException(promise) } {
        return js.take_exception("JS_NewPromiseCapability");
    }

    let state = Box::new(RequestState::new(
        js.ctx(),
        resolving_funcs[0],
        resolving_funcs[1],
    ));
    let mut request = AdapterRequest { promise, state };

    let callback_info = wgpu::WGPURequestAdapterCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(request_adapter_callback),
        userdata1: request.state_ptr().cast::<c_void>(),
        userdata2: ptr::null_mut(),
    };

    unsafe {
        wgpu::wgpuInstanceRequestAdapter(instance.raw(), ptr::null(), callback_info);
    }

    Ok(request)
}

extern "C" fn request_adapter_callback(
    status: wgpu::WGPURequestAdapterStatus,
    _adapter: wgpu::WGPUAdapter,
    _message: wgpu::WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        if userdata1.is_null() {
            return;
        }

        let state = unsafe { &mut *userdata1.cast::<RequestState>() };
        state.callback_count += 1;
        state.thread_id = Some(std::thread::current().id());

        if status == wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success {
            let mut value = js_undefined();
            let ret =
                unsafe { qjs::JS_Call(state.ctx, state.resolve, js_undefined(), 1, &mut value) };
            if unsafe { !qjs::JS_IsException(ret) } {
                state.resolved = true;
            }
            unsafe { qjs::JS_FreeValue(state.ctx, ret) };
        } else {
            let mut value = js_undefined();
            let ret =
                unsafe { qjs::JS_Call(state.ctx, state.reject, js_undefined(), 1, &mut value) };
            unsafe { qjs::JS_FreeValue(state.ctx, ret) };
        }
    }));

    if result.is_err() && !userdata1.is_null() {
        let state = unsafe { &mut *userdata1.cast::<RequestState>() };
        state.panicked = true;
    }
}

/// Installs the `requestAdapter` JavaScript function used by the tests.
pub fn install_request_adapter_function(
    instance: &Instance,
    js: &QuickJs,
    slot: &'static RequestAdapterSlot,
) -> Result<()> {
    {
        let mut state = slot.state.lock().map_err(|_| Error::SlotPoisoned)?;
        state.instance = instance.raw() as usize;
    }

    let function = unsafe {
        qjs::JS_NewCFunction(
            js.ctx(),
            Some(js_request_adapter),
            c"requestAdapter".as_ptr(),
            0,
        )
    };
    if unsafe { qjs::JS_IsException(function) } {
        return js.take_exception("JS_NewCFunction");
    }
    js.set_global_value(c"requestAdapter", function)
}

/// Storage used by the test-only JavaScript `requestAdapter` function.
pub struct RequestAdapterSlot {
    state: Mutex<RequestAdapterSlotState>,
}

struct RequestAdapterSlotState {
    instance: usize,
    request: usize,
}

impl RequestAdapterSlot {
    /// Creates an empty JavaScript request-adapter slot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(RequestAdapterSlotState {
                instance: 0,
                request: 0,
            }),
        }
    }

    /// Takes ownership of the most recent adapter request started from JavaScript.
    pub fn take_request(&self) -> Result<Option<Box<AdapterRequest>>> {
        let mut state = self.state.lock().map_err(|_| Error::SlotPoisoned)?;
        let ptr = std::mem::replace(&mut state.request, 0) as *mut AdapterRequest;
        Ok(NonNull::new(ptr).map(|ptr| unsafe { Box::from_raw(ptr.as_ptr()) }))
    }
}

impl Default for RequestAdapterSlot {
    fn default() -> Self {
        Self::new()
    }
}

extern "C" fn js_request_adapter(
    ctx: *mut qjs::JSContext,
    _this_val: qjs::JSValue,
    _argc: i32,
    _argv: *mut qjs::JSValue,
) -> qjs::JSValue {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let slot = &REQUEST_ADAPTER_SLOT;
        let instance = match slot.state.lock() {
            Ok(state) => state.instance as wgpu::WGPUInstance,
            Err(_) => return js_exception(),
        };
        if instance.is_null() {
            return js_exception();
        }

        let mut resolving_funcs = [js_undefined(), js_undefined()];
        let promise = unsafe { qjs::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr()) };
        if unsafe { qjs::JS_IsException(promise) } {
            return promise;
        }

        let state = Box::new(RequestState::new(
            ctx,
            resolving_funcs[0],
            resolving_funcs[1],
        ));
        let mut request = Box::new(AdapterRequest { promise, state });
        let callback_info = wgpu::WGPURequestAdapterCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(request_adapter_callback),
            userdata1: request.state_ptr().cast::<c_void>(),
            userdata2: ptr::null_mut(),
        };

        unsafe {
            wgpu::wgpuInstanceRequestAdapter(instance, ptr::null(), callback_info);
        }

        let promise = unsafe { qjs::JS_DupValue(ctx, request.promise) };
        let old = match slot.state.lock() {
            Ok(mut state) => std::mem::replace(&mut state.request, Box::into_raw(request) as usize),
            Err(_) => return js_exception(),
        } as *mut AdapterRequest;
        if !old.is_null() {
            drop(unsafe { Box::from_raw(old) });
        }
        promise
    }));

    result.unwrap_or_else(|_| js_exception())
}

static REQUEST_ADAPTER_SLOT: RequestAdapterSlot = RequestAdapterSlot::new();

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn callback_does_not_fire_before_pump() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        let request = request_adapter_promise(&instance, &js)?;

        assert_eq!(request.callback_count(), 0);
        assert!(!request.callback_panicked());
        Ok(())
    }

    #[test]
    fn process_events_fires_callback_exactly_once() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        let request = request_adapter_promise(&instance, &js)?;

        process_events(&instance);
        assert_eq!(request.callback_count(), 1);

        process_events(&instance);
        assert_eq!(request.callback_count(), 1);
        Ok(())
    }

    #[test]
    fn resolving_promise_does_not_run_continuation_until_microtasks_drain() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        install_request_adapter_function(&instance, &js, &REQUEST_ADAPTER_SLOT)?;

        js.eval(
            "globalThis.ran = false;
             requestAdapter().then(() => { globalThis.ran = true; });",
        )?;
        let request = REQUEST_ADAPTER_SLOT
            .take_request()?
            .ok_or(Error::Null("requestAdapter request"))?;

        process_events(&instance);
        assert!(request.is_resolved());
        assert!(js.is_job_pending());
        assert!(!js.eval_bool("globalThis.ran")?);

        js.drain_microtasks()?;
        assert!(js.eval_bool("globalThis.ran")?);
        assert!(!js.is_job_pending());
        Ok(())
    }

    #[test]
    fn omitting_microtask_drain_never_runs_await_style_continuation() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        install_request_adapter_function(&instance, &js, &REQUEST_ADAPTER_SLOT)?;

        js.eval(
            "globalThis.ran = false;
             (async () => {
               await requestAdapter();
               globalThis.ran = true;
             })();",
        )?;
        let _request = REQUEST_ADAPTER_SLOT
            .take_request()?
            .ok_or(Error::Null("requestAdapter request"))?;

        for _ in 0..8 {
            tick_without_microtask_drain(&instance);
            assert!(!js.eval_bool("globalThis.ran")?);
        }
        assert!(js.is_job_pending());
        Ok(())
    }

    #[test]
    fn microtasks_before_process_events_defers_continuation_until_next_tick() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        install_request_adapter_function(&instance, &js, &REQUEST_ADAPTER_SLOT)?;

        js.eval(
            "globalThis.ran = false;
             requestAdapter().then(() => { globalThis.ran = true; });",
        )?;
        let _request = REQUEST_ADAPTER_SLOT
            .take_request()?
            .ok_or(Error::Null("requestAdapter request"))?;

        js.drain_microtasks()?;
        process_events(&instance);
        assert!(js.is_job_pending());
        assert!(!js.eval_bool("globalThis.ran")?);

        tick(&instance, &js)?;
        assert!(js.eval_bool("globalThis.ran")?);
        assert!(!js.is_job_pending());
        Ok(())
    }

    #[test]
    fn callback_runs_on_process_events_thread() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        let request = request_adapter_promise(&instance, &js)?;
        let pumping_thread = std::thread::current().id();

        process_events(&instance);

        assert_eq!(request.callback_count(), 1);
        assert_eq!(request.callback_thread_id(), Some(pumping_thread));
        Ok(())
    }
}
