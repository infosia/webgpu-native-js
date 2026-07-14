#![warn(missing_docs)]

//! Spike proving that JS engine finalizers enqueue WebGPU releases and that a
//! designated drain thread performs the actual `webgpu.h` calls.

use std::collections::VecDeque;
#[cfg(target_os = "macos")]
use std::ffi::c_char;
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;

use webgpu_native_js_ffi::native as wgpu;

#[cfg(target_os = "macos")]
mod jsc {
    use super::{c_char, c_void};

    pub enum OpaqueJSContext {}
    pub enum OpaqueJSValue {}
    pub enum OpaqueJSClass {}

    pub type JSContextRef = *mut OpaqueJSContext;
    pub type JSGlobalContextRef = *mut OpaqueJSContext;
    pub type JSObjectRef = *mut OpaqueJSValue;
    pub type JSValueRef = *const OpaqueJSValue;
    pub type JSClassRef = *const OpaqueJSClass;

    #[repr(C)]
    pub struct JSClassDefinition {
        pub version: i32,
        pub attributes: u32,
        pub class_name: *const c_char,
        pub parent_class: JSClassRef,
        pub static_values: *const c_void,
        pub static_functions: *const c_void,
        pub initialize: Option<unsafe extern "C" fn(JSContextRef, JSObjectRef)>,
        pub finalize: Option<unsafe extern "C" fn(JSObjectRef)>,
        pub has_property: *const c_void,
        pub get_property: *const c_void,
        pub set_property: *const c_void,
        pub delete_property: *const c_void,
        pub get_property_names: *const c_void,
        pub call_as_function: *const c_void,
        pub call_as_constructor: *const c_void,
        pub has_instance: *const c_void,
        pub convert_to_type: *const c_void,
    }

    #[link(name = "JavaScriptCore", kind = "framework")]
    unsafe extern "C" {
        pub fn JSGlobalContextCreate(global_object_class: JSClassRef) -> JSGlobalContextRef;
        pub fn JSGlobalContextRelease(ctx: JSGlobalContextRef);
        pub fn JSGarbageCollect(ctx: JSContextRef);

        pub fn JSClassCreate(definition: *const JSClassDefinition) -> JSClassRef;
        pub fn JSClassRelease(js_class: JSClassRef);
        pub fn JSObjectMake(
            ctx: JSContextRef,
            js_class: JSClassRef,
            data: *mut c_void,
        ) -> JSObjectRef;
        pub fn JSObjectGetPrivate(object: JSObjectRef) -> *mut c_void;
        pub fn JSValueProtect(ctx: JSContextRef, value: JSValueRef);
        pub fn JSValueUnprotect(ctx: JSContextRef, value: JSValueRef);
    }
}

/// Crate-local result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the release-queue spike.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// A C API returned null where a live handle was required.
    Null(&'static str),
    /// A mutex was poisoned.
    Poisoned(&'static str),
    /// Requesting a WebGPU adapter failed.
    RequestAdapterFailed(wgpu::WGPURequestAdapterStatus),
    /// JavaScriptCore is only available on macOS for this spike.
    UnsupportedPlatform,
}

/// Release function executed by the drain thread.
pub type ReleaseFn = fn(ReleasePayload, Arc<ReleaseLog>);

/// Opaque data carried by a release request.
#[derive(Clone, Copy)]
pub enum ReleasePayload {
    /// A single opaque WebGPU handle.
    Handle(usize),
    /// An adapter plus the native parent-instance reference held by that child wrapper.
    AdapterWithInstanceRef {
        /// Adapter handle released by this request.
        adapter: usize,
        /// Extra instance reference taken when the adapter wrapper was created.
        instance_ref: usize,
    },
}

/// A single queued release request.
#[derive(Clone)]
pub struct ReleaseRequest {
    payload: ReleasePayload,
    release: ReleaseFn,
    log: Arc<ReleaseLog>,
}

impl ReleaseRequest {
    /// Creates a release request from an opaque handle, a release function, and log.
    #[must_use]
    pub fn new(handle: usize, release: ReleaseFn, log: Arc<ReleaseLog>) -> Self {
        Self {
            payload: ReleasePayload::Handle(handle),
            release,
            log,
        }
    }

    /// Creates an adapter release request that owns a native reference to its parent instance.
    ///
    /// The JS-level parent reference preserves wrapper identity. This native AddRef is the
    /// lifetime mechanism that keeps the parent handle alive regardless of finalizer or FIFO order.
    #[must_use]
    pub fn adapter_with_parent_instance_ref(
        adapter: usize,
        instance: usize,
        log: Arc<ReleaseLog>,
    ) -> Self {
        unsafe { wgpu::wgpuInstanceAddRef(instance as wgpu::WGPUInstance) };
        Self {
            payload: ReleasePayload::AdapterWithInstanceRef {
                adapter,
                instance_ref: instance,
            },
            release: release_adapter_with_parent_instance_ref,
            log,
        }
    }
}

/// Thread-safe many-producer, one-consumer release queue.
#[derive(Default)]
pub struct ReleaseQueue {
    requests: Mutex<VecDeque<ReleaseRequest>>,
}

impl ReleaseQueue {
    /// Creates an empty release queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a release request.
    pub fn enqueue(&self, request: ReleaseRequest) -> Result<()> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| Error::Poisoned("release queue"))?;
        requests.push_back(request);
        Ok(())
    }

    /// Drains all currently queued releases on the calling thread.
    pub fn drain(&self) -> Result<usize> {
        let mut drained = 0;
        loop {
            let request = {
                let mut requests = self
                    .requests
                    .lock()
                    .map_err(|_| Error::Poisoned("release queue"))?;
                requests.pop_front()
            };
            let Some(request) = request else {
                return Ok(drained);
            };
            (request.release)(request.payload, request.log);
            drained += 1;
        }
    }
}

/// Instrumentation shared by finalizers and release functions.
#[derive(Default)]
pub struct ReleaseLog {
    release_count: AtomicUsize,
    finalizer_count: AtomicUsize,
    releases_seen_in_finalizer: AtomicUsize,
    panic_count: AtomicUsize,
    finalizer_order: Mutex<Vec<&'static str>>,
    drain_order: Mutex<Vec<&'static str>>,
    native_release_order: Mutex<Vec<&'static str>>,
    finalizer_threads: Mutex<Vec<ThreadId>>,
    release_threads: Mutex<Vec<ThreadId>>,
}

impl ReleaseLog {
    /// Creates an empty release log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of observed release calls.
    #[must_use]
    pub fn release_count(&self) -> usize {
        self.release_count.load(Ordering::SeqCst)
    }

    /// Returns the number of observed finalizer calls.
    #[must_use]
    pub fn finalizer_count(&self) -> usize {
        self.finalizer_count.load(Ordering::SeqCst)
    }

    /// Returns release count observed from inside finalizers.
    #[must_use]
    pub fn releases_seen_in_finalizers(&self) -> usize {
        self.releases_seen_in_finalizer.load(Ordering::SeqCst)
    }

    /// Returns caught finalizer panic count.
    #[must_use]
    pub fn panic_count(&self) -> usize {
        self.panic_count.load(Ordering::SeqCst)
    }

    #[cfg(target_os = "macos")]
    fn record_finalizer(&self, kind: &'static str) {
        self.finalizer_count.fetch_add(1, Ordering::SeqCst);
        self.releases_seen_in_finalizer
            .fetch_add(self.release_count(), Ordering::SeqCst);
        if let Ok(mut order) = self.finalizer_order.lock() {
            order.push(kind);
        }
        if let Ok(mut threads) = self.finalizer_threads.lock() {
            threads.push(std::thread::current().id());
        }
    }

    fn record_release(&self, kind: &'static str) {
        self.release_count.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut order) = self.drain_order.lock() {
            order.push(kind);
        }
        if let Ok(mut order) = self.native_release_order.lock() {
            order.push(kind);
        }
        if let Ok(mut threads) = self.release_threads.lock() {
            threads.push(std::thread::current().id());
        }
    }

    fn record_native_release(&self, kind: &'static str) {
        if let Ok(mut order) = self.native_release_order.lock() {
            order.push(kind);
        }
    }

    #[cfg(target_os = "macos")]
    fn record_panic(&self) {
        self.panic_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Returns the recorded finalizer order.
    pub fn finalizer_order(&self) -> Result<Vec<&'static str>> {
        self.finalizer_order
            .lock()
            .map(|order| order.clone())
            .map_err(|_| Error::Poisoned("finalizer order"))
    }

    /// Returns the recorded drain order.
    pub fn drain_order(&self) -> Result<Vec<&'static str>> {
        self.drain_order
            .lock()
            .map(|order| order.clone())
            .map_err(|_| Error::Poisoned("drain order"))
    }

    /// Returns every native release call observed while draining.
    pub fn native_release_order(&self) -> Result<Vec<&'static str>> {
        self.native_release_order
            .lock()
            .map(|order| order.clone())
            .map_err(|_| Error::Poisoned("native release order"))
    }

    /// Returns the recorded finalizer thread ids.
    pub fn finalizer_threads(&self) -> Result<Vec<ThreadId>> {
        self.finalizer_threads
            .lock()
            .map(|threads| threads.clone())
            .map_err(|_| Error::Poisoned("finalizer threads"))
    }

    /// Returns the recorded release thread ids.
    pub fn release_threads(&self) -> Result<Vec<ThreadId>> {
        self.release_threads
            .lock()
            .map(|threads| threads.clone())
            .map_err(|_| Error::Poisoned("release threads"))
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

    /// Processes the WebGPU event queue.
    pub fn process_events(&self) {
        unsafe { wgpu::wgpuInstanceProcessEvents(self.raw()) };
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { wgpu::wgpuInstanceRelease(self.raw()) };
    }
}

struct AdapterRequestState {
    adapter: AtomicUsize,
    status: AtomicUsize,
}

extern "C" fn request_adapter_callback(
    status: wgpu::WGPURequestAdapterStatus,
    adapter: wgpu::WGPUAdapter,
    _message: wgpu::WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if userdata1.is_null() {
            return;
        }
        let state = unsafe { &*userdata1.cast::<AdapterRequestState>() };
        state.status.store(status as usize, Ordering::SeqCst);
        state.adapter.store(adapter as usize, Ordering::SeqCst);
    }));
}

/// Requests a real headless adapter and returns its callback-owned handle.
pub fn request_headless_adapter(instance: &Instance) -> Result<wgpu::WGPUAdapter> {
    let state = Box::new(AdapterRequestState {
        adapter: AtomicUsize::new(0),
        status: AtomicUsize::new(0),
    });
    let state_ptr = ptr::from_ref(state.as_ref()).cast_mut().cast::<c_void>();
    let callback_info = wgpu::WGPURequestAdapterCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(request_adapter_callback),
        userdata1: state_ptr,
        userdata2: ptr::null_mut(),
    };

    unsafe {
        wgpu::wgpuInstanceRequestAdapter(instance.raw(), ptr::null(), callback_info);
    }
    instance.process_events();

    let status = state.status.load(Ordering::SeqCst) as wgpu::WGPURequestAdapterStatus;
    if status != wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success {
        return Err(Error::RequestAdapterFailed(status));
    }
    let adapter = state.adapter.load(Ordering::SeqCst) as wgpu::WGPUAdapter;
    NonNull::new(adapter)
        .map(NonNull::as_ptr)
        .ok_or(Error::Null("request adapter callback"))
}

fn release_adapter_with_parent_instance_ref(payload: ReleasePayload, log: Arc<ReleaseLog>) {
    let ReleasePayload::AdapterWithInstanceRef {
        adapter,
        instance_ref,
    } = payload
    else {
        return;
    };

    let adapter = adapter as wgpu::WGPUAdapter;
    unsafe {
        wgpu::wgpuAdapterAddRef(adapter);
        wgpu::wgpuAdapterRelease(adapter);
        wgpu::wgpuAdapterRelease(adapter);
    }
    log.record_release("Adapter");

    unsafe { wgpu::wgpuInstanceRelease(instance_ref as wgpu::WGPUInstance) };
    log.record_native_release("AdapterParentInstanceRef");
}

#[cfg(test)]
fn release_instance(payload: ReleasePayload, log: Arc<ReleaseLog>) {
    let ReleasePayload::Handle(handle) = payload else {
        return;
    };
    unsafe { wgpu::wgpuInstanceRelease(handle as wgpu::WGPUInstance) };
    log.record_release("Instance");
}

#[cfg(test)]
fn synthetic_release(payload: ReleasePayload, log: Arc<ReleaseLog>) {
    let _ = payload;
    log.record_release("Synthetic");
}

#[cfg(target_os = "macos")]
struct FinalizerPayload {
    queue: Arc<ReleaseQueue>,
    request: ReleaseRequest,
    log: Arc<ReleaseLog>,
    kind: &'static str,
    #[cfg(test)]
    panic_after_enqueue: bool,
}

#[cfg(target_os = "macos")]
impl FinalizerPayload {
    fn finalize(&self) {
        self.log.record_finalizer(self.kind);
        let _ = self.queue.enqueue(self.request.clone());
        #[cfg(test)]
        assert!(!self.panic_after_enqueue, "intentional finalizer panic");
    }
}

#[cfg(target_os = "macos")]
/// A minimal JavaScriptCore global context and wrapper class.
pub struct JscContext {
    raw: NonNull<jsc::OpaqueJSContext>,
    class: NonNull<jsc::OpaqueJSClass>,
}

#[cfg(target_os = "macos")]
impl JscContext {
    /// Creates a default JavaScriptCore global context and wrapper class.
    pub fn new() -> Result<Self> {
        let name = c"ReleaseQueueWrapper";
        let definition = jsc::JSClassDefinition {
            version: 0,
            attributes: 0,
            class_name: name.as_ptr(),
            parent_class: ptr::null(),
            static_values: ptr::null(),
            static_functions: ptr::null(),
            initialize: None,
            finalize: Some(jsc_finalizer),
            has_property: ptr::null(),
            get_property: ptr::null(),
            set_property: ptr::null(),
            delete_property: ptr::null(),
            get_property_names: ptr::null(),
            call_as_function: ptr::null(),
            call_as_constructor: ptr::null(),
            has_instance: ptr::null(),
            convert_to_type: ptr::null(),
        };
        let class = unsafe { jsc::JSClassCreate(&definition) };
        let class = NonNull::new(class.cast_mut()).ok_or(Error::Null("JSClassCreate"))?;
        let raw = unsafe { jsc::JSGlobalContextCreate(ptr::null()) };
        let raw = NonNull::new(raw).ok_or(Error::Null("JSGlobalContextCreate"))?;
        Ok(Self { raw, class })
    }

    fn ctx(&self) -> jsc::JSContextRef {
        self.raw.as_ptr()
    }

    /// Creates a wrapper object with an optional parent reference.
    pub fn wrapper(
        &self,
        queue: Arc<ReleaseQueue>,
        request: ReleaseRequest,
        log: Arc<ReleaseLog>,
        kind: &'static str,
        parent: Option<jsc::JSObjectRef>,
    ) -> Result<jsc::JSObjectRef> {
        let parent_ref = parent.map(|parent| {
            unsafe { jsc::JSValueProtect(self.ctx(), parent.cast_const()) };
            JscParentRef {
                ctx: self.ctx(),
                object: parent,
            }
        });
        let payload = Box::new(JscPayload {
            finalizer: FinalizerPayload {
                queue,
                request,
                log,
                kind,
                #[cfg(test)]
                panic_after_enqueue: false,
            },
            parent: parent_ref,
        });
        let payload = Box::into_raw(payload);
        let object = unsafe { jsc::JSObjectMake(self.ctx(), self.class.as_ptr(), payload.cast()) };
        let Some(object) = NonNull::new(object) else {
            let mut payload = unsafe { Box::from_raw(payload) };
            if let Some(parent) = payload.parent.take() {
                unsafe { jsc::JSValueUnprotect(parent.ctx, parent.object.cast_const()) };
            }
            return Err(Error::Null("JSObjectMake"));
        };
        unsafe { jsc::JSValueProtect(self.ctx(), object.as_ptr().cast_const()) };
        Ok(object.as_ptr())
    }

    /// Removes an external protection from a wrapper object.
    pub fn unprotect(&self, object: jsc::JSObjectRef) {
        unsafe { jsc::JSValueUnprotect(self.ctx(), object.cast_const()) };
    }

    /// Forces a JavaScriptCore garbage collection.
    pub fn run_gc(&self) {
        unsafe { jsc::JSGarbageCollect(self.ctx()) };
    }
}

#[cfg(target_os = "macos")]
impl Drop for JscContext {
    fn drop(&mut self) {
        unsafe {
            jsc::JSGlobalContextRelease(self.raw.as_ptr());
            jsc::JSClassRelease(self.class.as_ptr());
        }
    }
}

#[cfg(target_os = "macos")]
struct JscParentRef {
    ctx: jsc::JSContextRef,
    object: jsc::JSObjectRef,
}

#[cfg(target_os = "macos")]
struct JscPayload {
    finalizer: FinalizerPayload,
    parent: Option<JscParentRef>,
}

#[cfg(target_os = "macos")]
unsafe extern "C" fn jsc_finalizer(object: jsc::JSObjectRef) {
    let payload = unsafe { jsc::JSObjectGetPrivate(object) }.cast::<JscPayload>();
    let Some(payload) = NonNull::new(payload) else {
        return;
    };
    let mut payload = unsafe { Box::from_raw(payload.as_ptr()) };
    let log = Arc::clone(&payload.finalizer.log);
    let result = catch_unwind(AssertUnwindSafe(|| payload.finalizer.finalize()));
    if result.is_err() {
        log.record_panic();
    }
    if let Some(parent) = payload.parent.take() {
        unsafe { jsc::JSValueUnprotect(parent.ctx, parent.object.cast_const()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Probes that queue draining did not over-release the handles.
    ///
    /// The C ABI exposes no refcount introspection, so this does not prove
    /// leak-freedom. It keeps one extra native reference to each handle, drains
    /// the queue, then calls ordinary C ABI functions on the handles before
    /// releasing those extra probe references.
    fn assert_drain_does_not_over_release(
        queue: &ReleaseQueue,
        instance: wgpu::WGPUInstance,
        adapter: wgpu::WGPUAdapter,
    ) -> Result<usize> {
        unsafe {
            wgpu::wgpuInstanceAddRef(instance);
            wgpu::wgpuAdapterAddRef(adapter);
        }

        let drained = queue.drain();

        unsafe {
            wgpu::wgpuInstanceProcessEvents(instance);
            let _ = wgpu::wgpuAdapterHasFeature(
                adapter,
                wgpu::WGPUFeatureName_WGPUFeatureName_CoreFeaturesAndLimits,
            );
            wgpu::wgpuAdapterRelease(adapter);
            wgpu::wgpuInstanceRelease(instance);
        }

        drained
    }

    #[cfg(target_os = "macos")]
    #[derive(Debug)]
    struct OrderingObservation {
        gc_finalizers: Vec<&'static str>,
        teardown_finalizers: Vec<&'static str>,
        drains: Vec<&'static str>,
    }

    #[cfg(target_os = "macos")]
    impl OrderingObservation {
        fn gc_count(&self) -> usize {
            self.gc_finalizers.len()
        }

        fn total_finalizer_count(&self) -> usize {
            self.gc_finalizers.len() + self.teardown_finalizers.len()
        }
    }

    #[test]
    fn exactly_once_for_fifo_queue() -> Result<()> {
        let queue = Arc::new(ReleaseQueue::new());
        let log = Arc::new(ReleaseLog::new());
        queue.enqueue(ReleaseRequest::new(1, synthetic_release, Arc::clone(&log)))?;
        assert_eq!(queue.drain()?, 1);
        assert_eq!(queue.drain()?, 0);
        assert_eq!(log.release_count(), 1);

        queue.enqueue(ReleaseRequest::new(2, synthetic_release, Arc::clone(&log)))?;
        queue.enqueue(ReleaseRequest::new(3, synthetic_release, Arc::clone(&log)))?;
        assert_eq!(queue.drain()?, 2);
        assert_eq!(log.release_count(), 3);
        Ok(())
    }

    #[test]
    fn releases_execute_on_drain_thread() -> Result<()> {
        let _guard = test_lock();
        let queue = Arc::new(ReleaseQueue::new());
        let log = Arc::new(ReleaseLog::new());
        queue.enqueue(ReleaseRequest::new(11, synthetic_release, Arc::clone(&log)))?;
        let drain_thread = thread::current().id();
        assert_eq!(queue.drain()?, 1);
        assert_eq!(log.release_threads()?, vec![drain_thread]);
        Ok(())
    }

    #[test]
    fn parent_first_drain_is_safe_with_child_native_parent_ref() -> Result<()> {
        let _guard = test_lock();
        let instance = Instance::new_headless()?;
        let raw_instance = instance.raw();
        unsafe { wgpu::wgpuInstanceAddRef(raw_instance) };
        let adapter = request_headless_adapter(&instance)?;
        let queue = Arc::new(ReleaseQueue::new());
        let log = Arc::new(ReleaseLog::new());

        queue.enqueue(ReleaseRequest::new(
            raw_instance as usize,
            release_instance,
            Arc::clone(&log),
        ))?;
        queue.enqueue(ReleaseRequest::adapter_with_parent_instance_ref(
            adapter as usize,
            raw_instance as usize,
            Arc::clone(&log),
        ))?;

        assert_eq!(
            assert_drain_does_not_over_release(&queue, raw_instance, adapter)?,
            2
        );
        assert_eq!(log.drain_order()?, vec!["Instance", "Adapter"]);
        assert_eq!(
            log.native_release_order()?,
            vec!["Instance", "Adapter", "AdapterParentInstanceRef"]
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn jsc_finalizer_enqueue_is_simulated_from_foreign_thread() -> Result<()> {
        let queue = Arc::new(ReleaseQueue::new());
        let log = Arc::new(ReleaseLog::new());
        let finalizer_thread = {
            let queue = Arc::clone(&queue);
            let log = Arc::clone(&log);
            thread::spawn(move || {
                let payload = FinalizerPayload {
                    queue,
                    request: ReleaseRequest::new(31, synthetic_release, Arc::clone(&log)),
                    log,
                    kind: "Synthetic",
                    panic_after_enqueue: false,
                };
                payload.finalize();
            })
        };
        finalizer_thread.join().expect("foreign finalizer thread");
        assert_eq!(log.finalizer_count(), 1);
        assert_eq!(log.releases_seen_in_finalizers(), 0);
        let drain_thread = thread::current().id();
        assert_eq!(queue.drain()?, 1);
        assert_eq!(log.release_threads()?, vec![drain_thread]);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn jsc_ordering_with_parent_reference() -> Result<()> {
        let _guard = test_lock();
        let observation = jsc_ordering(true)?;
        eprintln!("JSC with parent ref: {observation:?}");
        assert_eq!(observation.total_finalizer_count(), 2);
        if observation.gc_count() == 2 {
            assert_eq!(observation.gc_finalizers, vec!["Instance", "Adapter"]);
        }
        assert_eq!(observation.drains.len(), 2);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn jsc_ordering_without_parent_reference_is_observed() -> Result<()> {
        let _guard = test_lock();
        let observation = jsc_ordering(false)?;
        eprintln!("JSC without parent ref: {observation:?}");
        assert_eq!(observation.total_finalizer_count(), 2);
        if observation.gc_count() == 2 {
            assert_eq!(observation.drains, observation.gc_finalizers);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn jsc_ordering(with_parent: bool) -> Result<OrderingObservation> {
        let instance = Instance::new_headless()?;
        let raw_instance = instance.raw();
        unsafe { wgpu::wgpuInstanceAddRef(raw_instance) };
        let adapter = request_headless_adapter(&instance)?;
        let queue = Arc::new(ReleaseQueue::new());
        let log = Arc::new(ReleaseLog::new());
        let js = JscContext::new()?;
        let parent = js.wrapper(
            Arc::clone(&queue),
            ReleaseRequest::new(raw_instance as usize, release_instance, Arc::clone(&log)),
            Arc::clone(&log),
            "Instance",
            None,
        )?;
        let child_parent = with_parent.then_some(parent);
        let child = js.wrapper(
            Arc::clone(&queue),
            ReleaseRequest::adapter_with_parent_instance_ref(
                adapter as usize,
                raw_instance as usize,
                Arc::clone(&log),
            ),
            Arc::clone(&log),
            "Adapter",
            child_parent,
        )?;
        // With JSC the parent reference is a JSValueProtect count held by the child wrapper.
        // The object also has one external protect from creation, so this only drops that
        // external root and does not cancel the child's parent reference.
        js.unprotect(parent);
        js.unprotect(child);
        for _ in 0..4 {
            js.run_gc();
        }
        let gc_finalizers = log.finalizer_order()?;
        drop(js);
        let all_finalizers = log.finalizer_order()?;
        let teardown_finalizers = all_finalizers[gc_finalizers.len()..].to_vec();
        queue.drain()?;
        let drains = log.drain_order()?;
        Ok(OrderingObservation {
            gc_finalizers,
            teardown_finalizers,
            drains,
        })
    }
}
