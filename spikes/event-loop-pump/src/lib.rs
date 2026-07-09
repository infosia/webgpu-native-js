#![warn(missing_docs)]

//! Spike proving that `wgpuInstanceProcessEvents` and QuickJS microtask
//! draining are separate queues that must be pumped in order.

use std::cell::Cell;
use std::ffi::{c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread::ThreadId;

use webgpu_native_js_ffi::native as wgpu;

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
    PendingJobFailed(String),
    /// QuickJS boolean conversion failed.
    ToBoolFailed,
    /// Installing a JavaScript global failed.
    SetGlobalFailed(&'static str),
    /// Calling a JavaScript function failed.
    CallFailed(&'static str),
    /// The global request-adapter test slot was poisoned.
    SlotPoisoned,
}

/// State reported by QuickJS for a Promise.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromiseState {
    /// The value passed to `JS_PromiseState` was not a Promise.
    NotAPromise,
    /// The Promise has not settled.
    Pending,
    /// The Promise has been fulfilled.
    Fulfilled,
    /// The Promise has been rejected.
    Rejected,
}

impl PromiseState {
    fn from_raw(raw: qjs::JSPromiseStateEnum) -> Self {
        match raw {
            qjs::JSPromiseStateEnum_JS_PROMISE_PENDING => Self::Pending,
            qjs::JSPromiseStateEnum_JS_PROMISE_FULFILLED => Self::Fulfilled,
            qjs::JSPromiseStateEnum_JS_PROMISE_REJECTED => Self::Rejected,
            _ => Self::NotAPromise,
        }
    }
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
                _ => {
                    let message = exception_message(job_ctx, "JS_ExecutePendingJob");
                    return Err(Error::PendingJobFailed(message));
                }
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
        Err(Error::Exception(exception_message(self.ctx(), fallback)))
    }
}

fn exception_message(ctx: *mut qjs::JSContext, fallback: &'static str) -> String {
    if ctx.is_null() {
        return fallback.to_owned();
    }

    let exception = unsafe { qjs::JS_GetException(ctx) };
    let raw = unsafe { qjs::JS_ToCString(ctx, exception) };
    let message = if raw.is_null() {
        fallback.to_owned()
    } else {
        let text = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { qjs::JS_FreeCString(ctx, raw) };
        text
    };
    unsafe { qjs::JS_FreeValue(ctx, exception) };
    message
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
    callback_count: Cell<usize>,
    resolver_called: Cell<bool>,
    adapter_release_count: Cell<usize>,
    retain_callback_adapter: Cell<bool>,
    retained_adapter: Cell<usize>,
    panicked: Cell<bool>,
    thread_id: Cell<Option<ThreadId>>,
}

impl RequestState {
    fn new(ctx: *mut qjs::JSContext, resolve: qjs::JSValue, reject: qjs::JSValue) -> Self {
        Self {
            ctx,
            resolve,
            reject,
            callback_count: Cell::new(0),
            resolver_called: Cell::new(false),
            adapter_release_count: Cell::new(0),
            retain_callback_adapter: Cell::new(false),
            retained_adapter: Cell::new(0),
            panicked: Cell::new(false),
            thread_id: Cell::new(None),
        }
    }
}

impl Drop for RequestState {
    fn drop(&mut self) {
        unsafe {
            qjs::JS_FreeValue(self.ctx, self.resolve);
            qjs::JS_FreeValue(self.ctx, self.reject);
        }
        let adapter = self.retained_adapter.replace(0) as wgpu::WGPUAdapter;
        if !adapter.is_null() {
            unsafe { wgpu::wgpuAdapterRelease(adapter) };
        }
    }
}

/// A pending request-adapter promise plus callback observations.
pub struct AdapterRequest {
    promise: qjs::JSValue,
    state: Arc<RequestState>,
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
        self.state.callback_count.get()
    }

    /// Returns whether the Promise resolver has been called.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        self.state.resolver_called.get()
    }

    /// Returns the Promise state reported by QuickJS.
    #[must_use]
    pub fn promise_state(&self) -> PromiseState {
        let raw = unsafe { qjs::JS_PromiseState(self.state.ctx, self.promise) };
        PromiseState::from_raw(raw)
    }

    /// Returns how many successful adapter handles the callback released.
    #[must_use]
    pub fn adapter_release_count(&self) -> usize {
        self.state.adapter_release_count.get()
    }

    #[cfg(test)]
    fn retain_callback_adapter_for_test(&self) {
        self.state.retain_callback_adapter.set(true);
    }

    #[cfg(test)]
    fn retained_callback_adapter(&self) -> Option<wgpu::WGPUAdapter> {
        let adapter = self.state.retained_adapter.get() as wgpu::WGPUAdapter;
        NonNull::new(adapter).map(NonNull::as_ptr)
    }

    /// Returns whether the extern callback caught a panic.
    #[must_use]
    pub fn callback_panicked(&self) -> bool {
        self.state.panicked.get()
    }

    /// Returns the thread that executed the WebGPU callback.
    #[must_use]
    pub fn callback_thread_id(&self) -> Option<ThreadId> {
        self.state.thread_id.get()
    }
}

impl Drop for AdapterRequest {
    fn drop(&mut self) {
        unsafe { qjs::JS_FreeValue(self.state.ctx, self.promise) };
    }
}

/// Calls `wgpuInstanceRequestAdapter` and bridges completion to a QuickJS Promise.
#[allow(clippy::arc_with_non_send_sync)]
pub fn request_adapter_promise(instance: &Instance, js: &QuickJs) -> Result<AdapterRequest> {
    let mut resolving_funcs = [js_undefined(), js_undefined()];
    let promise = unsafe { qjs::JS_NewPromiseCapability(js.ctx(), resolving_funcs.as_mut_ptr()) };
    if unsafe { qjs::JS_IsException(promise) } {
        return js.take_exception("JS_NewPromiseCapability");
    }

    let state = Arc::new(RequestState::new(
        js.ctx(),
        resolving_funcs[0],
        resolving_funcs[1],
    ));
    let request = AdapterRequest {
        promise,
        state: Arc::clone(&state),
    };
    // RequestState is intentionally !Send because it holds a JSContext and JSValues.
    // AllowProcessEvents runs this callback on the pumping thread, so this Arc is
    // only shared for lifetime ownership and is never sent to another thread.
    let callback_state = Arc::into_raw(state).cast::<c_void>().cast_mut();

    let callback_info = wgpu::WGPURequestAdapterCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(request_adapter_callback),
        userdata1: callback_state,
        userdata2: ptr::null_mut(),
    };

    unsafe {
        wgpu::wgpuInstanceRequestAdapter(instance.raw(), ptr::null(), callback_info);
    }

    Ok(request)
}

extern "C" fn request_adapter_callback(
    status: wgpu::WGPURequestAdapterStatus,
    adapter: wgpu::WGPUAdapter,
    _message: wgpu::WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    if userdata1.is_null() {
        return;
    }

    let state = unsafe { Arc::from_raw(userdata1.cast::<RequestState>()) };
    let result = catch_unwind(AssertUnwindSafe(|| {
        state.callback_count.set(state.callback_count.get() + 1);
        state.thread_id.set(Some(std::thread::current().id()));

        if status == wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success {
            retain_callback_adapter_for_test(adapter, &state);
            release_callback_adapter(adapter, &state);

            let mut value = js_undefined();
            let ret =
                unsafe { qjs::JS_Call(state.ctx, state.resolve, js_undefined(), 1, &mut value) };
            if unsafe { !qjs::JS_IsException(ret) } {
                state.resolver_called.set(true);
            }
            unsafe { qjs::JS_FreeValue(state.ctx, ret) };
        } else {
            let mut value = js_undefined();
            let ret =
                unsafe { qjs::JS_Call(state.ctx, state.reject, js_undefined(), 1, &mut value) };
            unsafe { qjs::JS_FreeValue(state.ctx, ret) };
        }
    }));

    if result.is_err() {
        state.panicked.set(true);
    }
}

/// Releases the owned adapter reference delivered to a request-adapter callback.
///
/// Releasing here is legal: `webgpu.h` prohibits re-entrant API calls only from
/// spontaneous callbacks and explicitly allows nested calls from
/// `wgpuInstanceProcessEvents` and `wgpuInstanceWaitAny` callback stacks. A real
/// binding will not release inline; it will enqueue a release request for Phase
/// 0.5's release queue. This callback is the first point where a `webgpu.h`
/// handle crosses a callback boundary into our ownership, which is exactly what
/// that queue exists to manage.
fn release_callback_adapter(adapter: wgpu::WGPUAdapter, state: &RequestState) {
    if adapter.is_null() {
        return;
    }

    unsafe { wgpu::wgpuAdapterRelease(adapter) };
    state
        .adapter_release_count
        .set(state.adapter_release_count.get() + 1);
}

fn retain_callback_adapter_for_test(adapter: wgpu::WGPUAdapter, state: &RequestState) {
    if !state.retain_callback_adapter.get() || adapter.is_null() {
        return;
    }

    unsafe { wgpu::wgpuAdapterAddRef(adapter) };
    state.retained_adapter.set(adapter as usize);
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
        for request in state.requests.drain(..) {
            let ptr = request as *mut AdapterRequest;
            if !ptr.is_null() {
                drop(unsafe { Box::from_raw(ptr) });
            }
        }
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
    requests: Vec<usize>,
}

impl RequestAdapterSlot {
    /// Creates an empty JavaScript request-adapter slot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(RequestAdapterSlotState {
                instance: 0,
                requests: Vec::new(),
            }),
        }
    }

    /// Takes ownership of the most recent adapter request started from JavaScript.
    pub fn take_request(&self) -> Result<Option<Box<AdapterRequest>>> {
        let mut state = self.state.lock().map_err(|_| Error::SlotPoisoned)?;
        let ptr = state.requests.pop().unwrap_or_default() as *mut AdapterRequest;
        Ok(NonNull::new(ptr).map(|ptr| unsafe { Box::from_raw(ptr.as_ptr()) }))
    }

    #[cfg(test)]
    fn take_requests_for_test(&self) -> Result<Vec<AdapterRequest>> {
        let mut state = self.state.lock().map_err(|_| Error::SlotPoisoned)?;
        let requests = std::mem::take(&mut state.requests)
            .into_iter()
            .filter_map(|ptr| NonNull::new(ptr as *mut AdapterRequest))
            .map(|ptr| unsafe { *Box::from_raw(ptr.as_ptr()) })
            .collect();
        Ok(requests)
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
    #[allow(clippy::arc_with_non_send_sync)]
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

        let state = Arc::new(RequestState::new(
            ctx,
            resolving_funcs[0],
            resolving_funcs[1],
        ));
        let request = Box::new(AdapterRequest {
            promise,
            state: Arc::clone(&state),
        });
        // RequestState is intentionally !Send because it holds a JSContext and JSValues.
        // AllowProcessEvents runs this callback on the pumping thread, so this Arc is
        // only shared for lifetime ownership and is never sent to another thread.
        let callback_state = Arc::into_raw(state).cast::<c_void>().cast_mut();
        let callback_info = wgpu::WGPURequestAdapterCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(request_adapter_callback),
            userdata1: callback_state,
            userdata2: ptr::null_mut(),
        };

        unsafe {
            wgpu::wgpuInstanceRequestAdapter(instance, ptr::null(), callback_info);
        }

        let promise = unsafe { qjs::JS_DupValue(ctx, request.promise) };
        match slot.state.lock() {
            Ok(mut state) => state.requests.push(Box::into_raw(request) as usize),
            Err(_) => return js_exception(),
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
        process_events(&instance);
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

        assert_eq!(request.promise_state(), PromiseState::Pending);
        process_events(&instance);
        assert!(request.is_resolved());
        assert_eq!(request.promise_state(), PromiseState::Fulfilled);
        assert!(js.is_job_pending());
        assert!(!js.eval_bool("globalThis.ran")?);

        js.drain_microtasks()?;
        assert!(js.eval_bool("globalThis.ran")?);
        assert!(!js.is_job_pending());
        Ok(())
    }

    #[test]
    fn double_request_adapter_keeps_both_pending_states_alive() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        install_request_adapter_function(&instance, &js, &REQUEST_ADAPTER_SLOT)?;

        js.eval(
            "globalThis.ran1 = false;
             globalThis.ran2 = false;
             globalThis.p1 = requestAdapter();
             globalThis.p2 = requestAdapter();
             globalThis.p1.then(() => { globalThis.ran1 = true; });
             globalThis.p2.then(() => { globalThis.ran2 = true; });",
        )?;
        let requests = REQUEST_ADAPTER_SLOT.take_requests_for_test()?;

        assert_eq!(requests.len(), 2);
        assert!(requests
            .iter()
            .all(|request| request.promise_state() == PromiseState::Pending));
        process_events(&instance);
        assert!(requests.iter().all(|request| request.callback_count() == 1));
        assert!(requests.iter().all(|request| request.is_resolved()));
        assert!(requests
            .iter()
            .all(|request| request.promise_state() == PromiseState::Fulfilled));
        assert!(js.is_job_pending());

        js.drain_microtasks()?;
        assert!(js.eval_bool("globalThis.ran1")?);
        assert!(js.eval_bool("globalThis.ran2")?);
        js.eval("globalThis.p1 = undefined; globalThis.p2 = undefined;")?;
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

    #[test]
    fn successful_request_releases_callback_adapter_exactly_once() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let instance = Instance::new_headless()?;
        let request = request_adapter_promise(&instance, &js)?;
        request.retain_callback_adapter_for_test();

        process_events(&instance);

        assert_eq!(request.callback_count(), 1);
        assert_eq!(request.adapter_release_count(), 1);
        let adapter = request
            .retained_callback_adapter()
            .ok_or(Error::Null("retained callback adapter"))?;
        let _ = unsafe {
            wgpu::wgpuAdapterHasFeature(
                adapter,
                wgpu::WGPUFeatureName_WGPUFeatureName_CoreFeaturesAndLimits,
            )
        };

        process_events(&instance);
        assert_eq!(request.callback_count(), 1);
        assert_eq!(request.adapter_release_count(), 1);
        Ok(())
    }

    #[test]
    fn drain_microtasks_reports_pending_job_exception_message() -> Result<()> {
        let _guard = TEST_LOCK.lock().expect("test lock is not poisoned");
        let js = QuickJs::new()?;
        let rc = unsafe { qjs::JS_EnqueueJob(js.ctx(), Some(throwing_job), 0, ptr::null_mut()) };
        assert_eq!(rc, 0);

        let err = js.drain_microtasks().expect_err("microtask should throw");

        match err {
            Error::PendingJobFailed(message) => {
                assert!(message.contains("microtask boom"), "{message}");
            }
            other => panic!("unexpected error: {other:?}"),
        }
        Ok(())
    }

    unsafe extern "C" fn throwing_job(
        ctx: *mut qjs::JSContext,
        _argc: i32,
        _argv: *mut qjs::JSValue,
    ) -> qjs::JSValue {
        let message = qjs::JS_NewString(ctx, c"microtask boom".as_ptr());
        qjs::JS_Throw(ctx, message)
    }
}
