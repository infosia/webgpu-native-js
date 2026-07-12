#![warn(missing_docs)]

//! Boa engine adapter (block 14 spike).
//!
//! Values are `Copy` indices into a per-runtime rooting arena and contexts are
//! `Copy` handles containing a raw Boa context pointer plus that arena.

use std::any::Any;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use boa_engine::builtins::object::OrdinaryObject as BoaObjectBuiltin;
use boa_engine::builtins::promise::PromiseState;
use boa_engine::module::{ModuleLoader, Referrer};
use boa_engine::object::builtins::{AlignedVec, JsArray, JsArrayBuffer, JsPromise, JsUint32Array};
use boa_engine::object::{FunctionObjectBuilder, JsObject, ObjectInitializer};
use boa_engine::property::{Attribute, PropertyDescriptor};
use boa_engine::{
    Context as BoaContext, JsData, JsError, JsNativeError, JsResult, JsString, JsValue, Module,
    NativeFunction, Source,
};
use boa_gc::{Finalize, Trace};
use webgpu_native_js_core as core;
use webgpu_native_js_core::__gpu_dispatch_from_ffi;
use webgpu_native_js_ffi::native as ffi_wgpu;

pub use core::HostValue;

/// Adapter result type.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors returned by the Boa adapter's host-facing API.
#[derive(Debug)]
pub enum Error {
    /// Boa raised a JavaScript exception.
    Exception(String),
    /// An engine-neutral queue operation failed.
    Queue(core::QueueError),
    /// A module source file could not be read.
    Io {
        /// The module path that failed.
        path: PathBuf,
        /// The underlying filesystem error.
        source: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exception(message) => formatter.write_str(message),
            Self::Queue(error) => write!(formatter, "{error:?}"),
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "could not read module '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// A `Copy` handle to a value rooted by the adapter arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoaValue(u32);

struct Slot {
    value: JsValue,
    refs: u32,
}

struct ClassEntry {
    spec: &'static core::ClassSpec<Engine>,
    prototype: BoaValue,
}

struct Arena {
    env: core::Environment,
    slots: RefCell<Vec<Option<Slot>>>,
    free: RefCell<Vec<u32>>,
    scopes: RefCell<Vec<Vec<BoaValue>>>,
    classes: RefCell<BTreeMap<core::ClassId, ClassEntry>>,
    next_registration: Cell<u32>,
    outstanding_deferreds: RefCell<BTreeMap<u32, NonNull<Option<core::Deferred<Engine>>>>>,
    registration_removals: Arc<Mutex<Vec<u32>>>,
    settle_trampoline: RefCell<Option<BoaValue>>,
    host_functions: RefCell<Vec<Rc<HostFunction>>>,
}

impl Arena {
    fn new(gpu: core::GpuDispatch) -> Self {
        Self {
            env: core::Environment::new(gpu, Arc::new(core::ReleaseQueue::new())),
            slots: RefCell::new(Vec::new()),
            free: RefCell::new(Vec::new()),
            scopes: RefCell::new(Vec::new()),
            classes: RefCell::new(BTreeMap::new()),
            next_registration: Cell::new(0),
            outstanding_deferreds: RefCell::new(BTreeMap::new()),
            registration_removals: Arc::new(Mutex::new(Vec::new())),
            settle_trampoline: RefCell::new(None),
            host_functions: RefCell::new(Vec::new()),
        }
    }

    fn insert(&self, value: JsValue) -> BoaValue {
        let index = if let Some(index) = self.free.borrow_mut().pop() {
            self.slots.borrow_mut()[index as usize] = Some(Slot { value, refs: 1 });
            index
        } else {
            let mut slots = self.slots.borrow_mut();
            let index = u32::try_from(slots.len()).expect("Boa value arena exhausted");
            slots.push(Some(Slot { value, refs: 1 }));
            index
        };
        let handle = BoaValue(index);
        if let Some(scope) = self.scopes.borrow_mut().last_mut() {
            scope.push(handle);
        }
        handle
    }

    fn get(&self, handle: BoaValue) -> JsValue {
        self.slots
            .borrow()
            .get(handle.0 as usize)
            .and_then(Option::as_ref)
            .expect("stale Boa value handle")
            .value
            .clone()
    }

    fn duplicate(&self, handle: BoaValue) {
        let mut slots = self.slots.borrow_mut();
        let slot = slots
            .get_mut(handle.0 as usize)
            .and_then(Option::as_mut)
            .expect("stale Boa value handle");
        slot.refs = slot
            .refs
            .checked_add(1)
            .expect("Boa value refcount overflow");
    }

    fn release(&self, handle: BoaValue) {
        let mut slots = self.slots.borrow_mut();
        let Some(slot) = slots.get_mut(handle.0 as usize).and_then(Option::as_mut) else {
            return;
        };
        slot.refs -= 1;
        if slot.refs == 0 {
            slots[handle.0 as usize] = None;
            self.free.borrow_mut().push(handle.0);
        }
    }

    fn begin_scope(&self) {
        self.scopes.borrow_mut().push(Vec::new());
    }

    fn end_scope(&self) {
        let values = self
            .scopes
            .borrow_mut()
            .pop()
            .expect("unbalanced Boa scope");
        for value in values {
            self.release(value);
        }
    }

    fn retain_from_scope(&self, value: BoaValue) {
        self.duplicate(value);
    }

    fn drain_registration_removals(&self) {
        let removals = self
            .registration_removals
            .lock()
            .map(|mut removals| std::mem::take(&mut *removals))
            .unwrap_or_default();
        let mut outstanding = self.outstanding_deferreds.borrow_mut();
        for id in removals {
            outstanding.remove(&id);
        }
    }

    fn release_outstanding_deferreds(&self) {
        self.drain_registration_removals();
        let outstanding = std::mem::take(&mut *self.outstanding_deferreds.borrow_mut());
        for (_, mut slot) in outstanding {
            // SAFETY: each pointer names the deferred field of a still-live
            // callback request Box; registration drop removes it before that
            // Box is freed, and teardown runs on the engine/process-events thread.
            let deferred = unsafe { slot.as_mut() }.take();
            if let Some(deferred) = deferred {
                self.release(deferred.resolve());
                self.release(deferred.reject());
            }
        }
    }
}

type HostFunction = dyn Fn(&[HostValue]) -> std::result::Result<HostValue, String>;
type ModuleTransform = dyn Fn(&str, &Path) -> std::result::Result<String, String>;

#[derive(Default)]
struct FileModuleLoader {
    aliases: RefCell<BTreeMap<String, PathBuf>>,
    transform: RefCell<Option<Rc<ModuleTransform>>>,
    modules: RefCell<BTreeMap<PathBuf, Module>>,
}

impl FileModuleLoader {
    fn resolve(&self, specifier: &str, importer: &Path) -> std::result::Result<PathBuf, String> {
        let base = self
            .aliases
            .borrow()
            .get(specifier)
            .cloned()
            .unwrap_or_else(|| {
                importer
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(specifier)
            });
        let probes = module_resolution_probes(&base);
        probes
            .iter()
            .find(|path| path.is_file())
            .map(|path| lexical_normalize_path(path))
            .ok_or_else(|| {
                let probed = probes
                    .iter()
                    .map(|path| format!("'{}'", path.display()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "could not resolve module '{specifier}' imported from '{}': probed paths: {probed}",
                    importer.display()
                )
            })
    }

    fn parse(&self, path: &Path, context: &mut BoaContext) -> JsResult<Module> {
        if let Some(module) = self.modules.borrow().get(path) {
            return Ok(module.clone());
        }
        let source = std::fs::read_to_string(path).map_err(|error| {
            JsNativeError::error().with_message(format!(
                "could not load module '{}': {error}",
                path.display()
            ))
        })?;
        let transform = self.transform.borrow().clone();
        let source = match transform {
            Some(transform) => transform(&source, path).map_err(|message| {
                JsNativeError::error().with_message(format!(
                    "could not load module '{}': transform failed: {message}",
                    path.display()
                ))
            })?,
            None => source,
        };
        let module = Module::parse(
            Source::from_reader(Cursor::new(source), Some(path)),
            None,
            context,
        )?;
        self.modules
            .borrow_mut()
            .insert(path.to_path_buf(), module.clone());
        Ok(module)
    }
}

impl ModuleLoader for FileModuleLoader {
    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut BoaContext>,
    ) -> JsResult<Module> {
        let importer = referrer.path().unwrap_or_else(|| Path::new(""));
        let specifier = specifier.to_std_string_lossy();
        let path = self
            .resolve(&specifier, importer)
            .map_err(|message| JsNativeError::error().with_message(message))?;
        self.parse(&path, &mut context.borrow_mut())
    }
}

struct Scope<'a>(&'a Arena);

impl<'a> Scope<'a> {
    fn new(arena: &'a Arena) -> Self {
        arena.begin_scope();
        Self(arena)
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.0.end_scope();
    }
}

#[derive(Clone, Copy)]
struct ArenaPointer(*const Arena);

impl Finalize for ArenaPointer {}

impl JsData for ArenaPointer {}

// SAFETY: `ArenaPointer` contains no Boa GC pointer. The pointed-to arena is
// adapter-owned root storage whose `JsValue` handles root themselves in Boa.
unsafe impl Trace for ArenaPointer {
    boa_gc::empty_trace!();
}

struct WrapperData {
    class: core::ClassId,
    payload: RefCell<Option<Box<dyn Any + Send>>>,
    finalizer: core::FinalizerFn,
    arena: *const Arena,
}

impl Finalize for WrapperData {
    fn finalize(&self) {
        let Some(payload) = self.payload.borrow_mut().take() else {
            return;
        };
        // SAFETY: every WrapperData is created from the live runtime arena and
        // Boa finalizes its objects before that arena is dropped.
        let arena = unsafe { &*self.arena };
        core::release_payload_values::<Engine>(payload.as_ref(), &mut |value| {
            arena.release(value);
        });
        (self.finalizer)(payload, &arena.env);
    }
}

impl JsData for WrapperData {}

// SAFETY: wrapper payloads contain native handles and `BoaValue` indices, not
// direct Boa GC pointers; all JavaScript values are traced by the arena slots.
unsafe impl Trace for WrapperData {
    boa_gc::empty_trace!();
}

#[derive(Clone, Copy)]
struct MethodCapture(core::MethodFn<Engine>);

impl Finalize for MethodCapture {}

// SAFETY: a static function pointer contains no GC-managed data.
unsafe impl Trace for MethodCapture {
    boa_gc::empty_trace!();
}

#[derive(Clone, Copy)]
struct GetterCapture(core::GetterFn<Engine>);

impl Finalize for GetterCapture {}

// SAFETY: a static function pointer contains no GC-managed data.
unsafe impl Trace for GetterCapture {
    boa_gc::empty_trace!();
}

#[derive(Clone, Copy)]
struct SetterCapture(core::SetterFn<Engine>);

impl Finalize for SetterCapture {}

// SAFETY: a static function pointer contains no GC-managed data.
unsafe impl Trace for SetterCapture {
    boa_gc::empty_trace!();
}

#[derive(Clone, Copy)]
struct ConstructorCapture(core::ConstructorFn<Engine>);

impl Finalize for ConstructorCapture {}

// SAFETY: a static function pointer contains no GC-managed data.
unsafe impl Trace for ConstructorCapture {
    boa_gc::empty_trace!();
}

/// Boa engine marker type.
pub struct Engine;

/// Per-call Boa context handle.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    ctx: *mut BoaContext,
    arena: &'a Arena,
}

impl<'a> Context<'a> {
    fn boa(self) -> &'a mut BoaContext {
        // SAFETY: the runtime is live, execution is confined to its owning
        // thread, and adapter entry points maintain one active mutable borrow.
        unsafe { &mut *self.ctx }
    }
}

/// Engine-owned registration token for an asynchronous deferred.
#[derive(Debug)]
pub struct DeferredRegistration {
    id: u32,
    removals: Arc<Mutex<Vec<u32>>>,
}

impl Drop for DeferredRegistration {
    fn drop(&mut self) {
        if let Ok(mut removals) = self.removals.lock() {
            removals.push(self.id);
        }
    }
}

/// A Boa runtime configured with the WebGPU binding environment.
pub struct Runtime {
    context: UnsafeCell<Option<BoaContext>>,
    arena: Box<Arena>,
    module_loader: Rc<FileModuleLoader>,
}

/// Observable completion state of an ES module evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleEvaluationStatus {
    /// Evaluation is suspended, normally at top-level `await`.
    Pending,
    /// Evaluation completed successfully.
    Fulfilled,
}

/// An owned Boa module-evaluation promise.
pub struct ModuleEvaluation<'runtime> {
    runtime: &'runtime Runtime,
    promise: JsPromise,
}

impl fmt::Debug for ModuleEvaluation<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleEvaluation")
            .finish_non_exhaustive()
    }
}

impl ModuleEvaluation<'_> {
    /// Returns the current evaluation state, surfacing rejection as an adapter exception.
    pub fn status(&self) -> Result<ModuleEvaluationStatus> {
        match self.promise.state() {
            PromiseState::Pending => Ok(ModuleEvaluationStatus::Pending),
            PromiseState::Fulfilled(_) => Ok(ModuleEvaluationStatus::Fulfilled),
            PromiseState::Rejected(reason) => {
                // SAFETY: the evaluation borrows the live runtime and status calls
                // do not overlap any other Boa operation.
                let context = unsafe { &mut *self.runtime.raw_context() };
                let message = reason.to_string(context).map_or_else(
                    |_| "Boa module evaluation rejected".to_owned(),
                    |value| value.to_std_string_lossy(),
                );
                Err(Error::Exception(message))
            }
        }
    }
}

impl Runtime {
    /// Creates a Boa runtime configured with the process WebGPU dispatch table.
    pub fn new() -> Result<Self> {
        Self::new_with_dispatch(gpu_dispatch())
    }

    fn new_with_dispatch(gpu: core::GpuDispatch) -> Result<Self> {
        let arena = Box::new(Arena::new(gpu));
        let module_loader = Rc::new(FileModuleLoader::default());
        let mut context = BoaContext::builder()
            .module_loader(module_loader.clone())
            .build()
            .map_err(|error| Error::Exception(error.to_string()))?;
        context.insert_data(ArenaPointer((&*arena) as *const Arena));
        let runtime = Self {
            context: UnsafeCell::new(Some(context)),
            arena,
            module_loader,
        };
        let trampoline = runtime.eval(
            "(function(fns, values) { for (let i = 0; i < fns.length; i++) fns[i](values[i]); })",
            "webgpu-native-js-settle-trampoline.js",
        )?;
        *runtime.arena.settle_trampoline.borrow_mut() = Some(trampoline);
        Ok(runtime)
    }

    fn raw_context(&self) -> *mut BoaContext {
        // SAFETY: the context Option remains `Some` until Runtime::drop, and
        // host-facing operations cannot execute after drop begins.
        unsafe { (*self.context.get()).as_mut().expect("Boa context is live") as *mut BoaContext }
    }

    fn with_scope<R>(&self, operation: impl FnOnce(Context<'_>) -> R) -> R {
        let _scope = Scope::new(&self.arena);
        operation(Context {
            ctx: self.raw_context(),
            arena: &self.arena,
        })
    }

    /// Evaluates a script and returns its rooted completion value.
    pub fn eval(&self, source: &str, _name: &str) -> Result<BoaValue> {
        // SAFETY: runtime methods are single-threaded and do not overlap Boa
        // calls; native callbacks finish before this borrow is used again.
        let context = unsafe { &mut *self.raw_context() };
        context
            .eval(Source::from_bytes(source))
            .map(|value| self.arena.insert(value))
            .map_err(|error| Error::Exception(js_error_string(error, context)))
    }

    /// Sets a global property and releases the adapter hold on `value`.
    pub fn set_global_value(&self, name: &str, value: BoaValue) -> Result<()> {
        let js_value = self.arena.get(value);
        // SAFETY: this runtime is single-threaded and no other Boa call is
        // active while a host-facing method executes.
        let context = unsafe { &mut *self.raw_context() };
        let result = context
            .global_object()
            .set(JsString::from(name), js_value, true, context)
            .map_err(|error| Error::Exception(js_error_string(error, context)));
        self.arena.release(value);
        result.map(|_| ())
    }

    /// Registers a global JavaScript function backed by a side-effect-only Rust callback.
    pub fn register_host_function<F>(&self, name: &str, f: F) -> Result<()>
    where
        F: Fn(&[HostValue]) -> std::result::Result<(), String> + 'static,
    {
        self.register_host_function_with_result(name, move |args| {
            f(args)?;
            Ok(HostValue::Undefined)
        })
    }

    /// Registers a global JavaScript function backed by a primitive-valued Rust callback.
    pub fn register_host_function_with_result<F>(&self, name: &str, f: F) -> Result<()>
    where
        F: Fn(&[HostValue]) -> std::result::Result<HostValue, String> + 'static,
    {
        let index = {
            let mut functions = self.arena.host_functions.borrow_mut();
            functions.push(Rc::new(f));
            functions.len() - 1
        };
        let capture = HostFunctionCapture {
            arena: (&*self.arena) as *const Arena,
            index,
        };
        // SAFETY: no other Boa operation overlaps this host-facing call.
        let context = unsafe { &mut *self.raw_context() };
        context
            .register_global_builtin_callable(
                JsString::from(name),
                0,
                NativeFunction::from_copy_closure_with_captures(host_function_callback, capture),
            )
            .map_err(|error| Error::Exception(js_error_string(error, context)))
    }

    /// Clears a global property by assigning `undefined`.
    pub fn clear_global(&self, name: &str) -> Result<()> {
        let value = self.arena.insert(JsValue::undefined());
        self.set_global_value(name, value)
    }

    /// Maps an exact ES module specifier to a host-owned source file.
    pub fn set_module_alias(&self, specifier: &str, path: &Path) -> Result<()> {
        let path = lexical_normalize_path(&absolute_path(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?);
        self.module_loader
            .aliases
            .borrow_mut()
            .insert(specifier.to_owned(), path);
        Ok(())
    }

    /// Sets the source transform applied before every module is parsed.
    pub fn set_module_transform<F>(&self, transform: F)
    where
        F: Fn(&str, &Path) -> std::result::Result<String, String> + 'static,
    {
        self.module_loader
            .transform
            .replace(Some(Rc::new(transform)));
    }

    /// Reads and evaluates a file as an ES module.
    pub fn eval_module(&self, path: &Path) -> Result<ModuleEvaluation<'_>> {
        let path = lexical_normalize_path(&absolute_path(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?);
        std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        // SAFETY: no other Boa operation overlaps this host-facing call.
        let context = unsafe { &mut *self.raw_context() };
        let module = self
            .module_loader
            .parse(&path, context)
            .map_err(|error| Error::Exception(js_error_string(error, context)))?;
        let promise = module.load_link_evaluate(context);
        context
            .run_jobs()
            .map_err(|error| Error::Exception(js_error_string(error, context)))?;
        let evaluation = ModuleEvaluation {
            runtime: self,
            promise,
        };
        evaluation.status()?;
        Ok(evaluation)
    }

    /// Wraps an adopted WebGPU device.
    ///
    /// # Safety
    /// `device` must be a live non-null device from this adapter's backend.
    pub unsafe fn wrap_device(&self, device: ffi_wgpu::WGPUDevice) -> Result<BoaValue> {
        self.with_scope(|cx| {
            // SAFETY: the caller guarantees the native device is live and from
            // the configured backend for the duration of this call.
            let value = unsafe { core::wrap_device::<Engine>(cx, device) }
                .map_err(|error| Error::Exception(value_string(cx, error)))?;
            cx.arena.retain_from_scope(value);
            Ok(value)
        })
    }

    /// Wraps a WebGPU instance as a JavaScript `GPU` object.
    ///
    /// # Safety
    /// `instance` must remain live while the returned wrapper is reachable.
    pub unsafe fn wrap_gpu(&self, instance: ffi_wgpu::WGPUInstance) -> Result<BoaValue> {
        self.with_scope(|cx| {
            let value = core::wrap_gpu::<Engine>(cx, instance)
                .map_err(|error| Error::Exception(value_string(cx, error)))?;
            cx.arena.retain_from_scope(value);
            Ok(value)
        })
    }

    /// Returns the native render-bundle handle carried by `value`, if it has that class.
    ///
    /// The returned handle is borrowed. The host must keep `value` reachable for
    /// the entire native use or take its own native reference.
    #[must_use]
    pub fn native_render_bundle(&self, value: BoaValue) -> Option<ffi_wgpu::WGPURenderBundle> {
        self.with_scope(|cx| core::native_render_bundle::<Engine>(cx, value))
    }

    /// Drains the core release queue.
    pub fn drain_releases(&self) -> std::result::Result<usize, core::QueueError> {
        self.arena.env.queue().drain()
    }

    /// Returns a thread-safe adopted-device event producer.
    #[must_use]
    pub fn device_event_forwarder(&self) -> DeviceEventForwarder {
        DeviceEventForwarder {
            inner: self.arena.env.device_event_forwarder(),
        }
    }

    /// Enqueues adopted-device loss without touching Boa.
    pub fn forward_device_lost(
        &self,
        device: ffi_wgpu::WGPUDevice,
        reason: ffi_wgpu::WGPUDeviceLostReason,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.device_event_forwarder()
            .forward_device_lost(device, reason, message)
    }

    /// Enqueues an adopted-device uncaptured error without touching Boa.
    pub fn forward_uncaptured_error(
        &self,
        device: ffi_wgpu::WGPUDevice,
        type_: ffi_wgpu::WGPUErrorType,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.device_event_forwarder()
            .forward_uncaptured_error(device, type_, message)
    }

    /// Runs one engine-neutral WebGPU tick and Boa's job executor.
    ///
    /// # Safety
    /// `instance` must be live and this must run on the runtime's owning thread.
    pub unsafe fn tick(&self, instance: ffi_wgpu::WGPUInstance) -> Result<usize> {
        self.with_scope(|cx| {
            // SAFETY: the caller guarantees a live instance and engine-thread
            // execution, which are the requirements of the core tick.
            unsafe { core::tick::<Engine>(cx, instance) }.map_err(|error| match error {
                core::TickError::Queue(error) => Error::Queue(error),
                core::TickError::Engine(error) => Error::Exception(value_string(cx, error)),
                _ => Error::Exception("unknown tick failure".to_owned()),
            })
        })
    }

    /// Runs the engine garbage collector.
    pub fn run_gc(&self) {
        boa_gc::force_collect();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        let arena = &self.arena;
        {
            let _scope = Scope::new(arena);
            let cx = Context {
                ctx: self.raw_context(),
                arena,
            };
            arena.env.settlements().release_pending::<Engine>(cx);
            arena.env.release_device_event_values::<Engine>(cx);
        }
        arena.release_outstanding_deferreds();
        let _ = arena.env.queue().drain();
        arena.slots.borrow_mut().clear();
        arena.free.borrow_mut().clear();
        arena.classes.borrow_mut().clear();
        arena.settle_trampoline.borrow_mut().take();
        // SAFETY: drop has exclusive access to Runtime and no adapter context
        // handle survives this teardown point.
        let context = unsafe { &mut *self.context.get() }.take();
        drop(context);
        // Finalize wrappers while their WrapperData arena pointer and binding
        // environment are still live; Boa's collector is thread-local.
        boa_gc::force_collect();
        let _ = arena.env.queue().drain();
    }
}

/// Send + Sync producer handle for adopted-device events.
#[derive(Clone)]
pub struct DeviceEventForwarder {
    inner: core::DeviceEventForwarder,
}

impl DeviceEventForwarder {
    /// Enqueues an adopted-device uncaptured error without touching Boa.
    pub fn forward_uncaptured_error(
        &self,
        device: ffi_wgpu::WGPUDevice,
        type_: ffi_wgpu::WGPUErrorType,
        message: impl Into<String>,
    ) -> std::result::Result<(), core::QueueError> {
        self.inner
            .forward_uncaptured_error::<Engine>(device, type_, message)
    }

    /// Enqueues adopted-device loss without touching Boa.
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

fn arena_pointer(context: &BoaContext) -> *const Arena {
    context
        .get_data::<ArenaPointer>()
        .expect("Boa adapter arena is installed")
        .0
}

fn callback_context(context: &mut BoaContext) -> Context<'_> {
    let pointer = arena_pointer(context);
    // SAFETY: Runtime keeps the arena allocation alive and address-stable for
    // the entire lifetime of its Boa context.
    let arena = unsafe { &*pointer };
    Context {
        ctx: context as *mut BoaContext,
        arena,
    }
}

fn callback_args(arena: &Arena, args: &[JsValue]) -> Vec<BoaValue> {
    args.iter()
        .cloned()
        .map(|value| arena.insert(value))
        .collect()
}

#[derive(Clone, Copy)]
struct HostFunctionCapture {
    arena: *const Arena,
    index: usize,
}

impl Finalize for HostFunctionCapture {}

// SAFETY: the capture contains no Boa GC pointer and Runtime keeps the arena live.
unsafe impl Trace for HostFunctionCapture {
    boa_gc::empty_trace!();
}

fn host_value(value: &JsValue, context: &mut BoaContext) -> JsResult<HostValue> {
    if value.is_undefined() {
        return Ok(HostValue::Undefined);
    }
    if value.is_null() {
        return Ok(HostValue::Null);
    }
    if let Some(value) = value.as_boolean() {
        return Ok(HostValue::Bool(value));
    }
    if let Some(value) = value.as_number() {
        return Ok(HostValue::Number(value));
    }
    value
        .to_string(context)
        .map(|value| HostValue::String(value.to_std_string_lossy()))
}

fn host_function_callback(
    _this: &JsValue,
    args: &[JsValue],
    capture: &HostFunctionCapture,
    context: &mut BoaContext,
) -> JsResult<JsValue> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: Runtime keeps the address-stable arena alive for every callback.
        let arena = unsafe { &*capture.arena };
        let function = arena
            .host_functions
            .borrow()
            .get(capture.index)
            .cloned()
            .ok_or_else(|| {
                JsNativeError::error().with_message("host function is not registered")
            })?;
        let args = args
            .iter()
            .map(|value| host_value(value, context))
            .collect::<JsResult<Vec<_>>>()?;
        let result =
            function(&args).map_err(|message| JsNativeError::typ().with_message(message))?;
        Ok(match result {
            HostValue::String(value) => JsString::from(value).into(),
            HostValue::Number(value) => JsValue::from(value),
            HostValue::Bool(value) => JsValue::from(value),
            HostValue::Null => JsValue::null(),
            HostValue::Undefined => JsValue::undefined(),
        })
    }));
    match result {
        Ok(result) => result,
        Err(_) => Err(JsNativeError::error()
            .with_message("Rust callback panicked")
            .into()),
    }
}

fn module_resolution_probes(base: &Path) -> [PathBuf; 4] {
    [
        base.to_path_buf(),
        path_with_suffix(base, ".js"),
        path_with_suffix(base, ".mjs"),
        base.join("index.js"),
    ]
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                );
                if can_pop {
                    normalized.pop();
                } else if !normalized.has_root() {
                    normalized.push(component.as_os_str());
                }
            }
        }
    }
    normalized
}

fn absolute_path(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn callback_result(cx: Context<'_>, result: core::Result<BoaValue, BoaValue>) -> JsResult<JsValue> {
    match result {
        Ok(value) => Ok(cx.arena.get(value)),
        Err(error) => Err(JsError::from_opaque(cx.arena.get(error))),
    }
}

fn method_callback(
    this: &JsValue,
    args: &[JsValue],
    capture: &MethodCapture,
    context: &mut BoaContext,
) -> JsResult<JsValue> {
    let pointer = arena_pointer(context);
    // SAFETY: callback execution occurs while Runtime owns the stable arena Box.
    let arena = unsafe { &*pointer };
    let _scope = Scope::new(arena);
    let cx = callback_context(context);
    let this = arena.insert(this.clone());
    let args = callback_args(arena, args);
    callback_result(cx, (capture.0)(cx, this, &args))
}

fn getter_callback(
    this: &JsValue,
    _args: &[JsValue],
    capture: &GetterCapture,
    context: &mut BoaContext,
) -> JsResult<JsValue> {
    let pointer = arena_pointer(context);
    // SAFETY: callback execution occurs while Runtime owns the stable arena Box.
    let arena = unsafe { &*pointer };
    let _scope = Scope::new(arena);
    let cx = callback_context(context);
    let this = arena.insert(this.clone());
    callback_result(cx, (capture.0)(cx, this))
}

fn setter_callback(
    this: &JsValue,
    args: &[JsValue],
    capture: &SetterCapture,
    context: &mut BoaContext,
) -> JsResult<JsValue> {
    let pointer = arena_pointer(context);
    // SAFETY: callback execution occurs while Runtime owns the stable arena Box.
    let arena = unsafe { &*pointer };
    let _scope = Scope::new(arena);
    let cx = callback_context(context);
    let this = arena.insert(this.clone());
    let value = arena.insert(args.first().cloned().unwrap_or_else(JsValue::undefined));
    (capture.0)(cx, this, value)
        .map(|()| JsValue::undefined())
        .map_err(|error| JsError::from_opaque(arena.get(error)))
}

fn constructor_callback(
    new_target: &JsValue,
    args: &[JsValue],
    capture: &ConstructorCapture,
    context: &mut BoaContext,
) -> JsResult<JsValue> {
    let pointer = arena_pointer(context);
    // SAFETY: callback execution occurs while Runtime owns the stable arena Box.
    let arena = unsafe { &*pointer };
    let _scope = Scope::new(arena);
    let cx = callback_context(context);
    let args = callback_args(arena, args);
    let value = match (capture.0)(cx, &args) {
        Ok(value) => arena.get(value),
        Err(error) => return Err(JsError::from_opaque(arena.get(error))),
    };
    if let (Some(object), Some(new_target)) = (value.as_object(), new_target.as_object()) {
        let prototype = new_target.get(JsString::from("prototype"), context)?;
        if let Some(prototype) = prototype.as_object() {
            object.set_prototype(Some(prototype));
        }
    }
    Ok(value)
}

fn illegal_constructor_callback(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut BoaContext,
) -> JsResult<JsValue> {
    Err(JsNativeError::typ()
        .with_message("Illegal constructor")
        .into())
}

fn object(cx: Context<'_>, value: BoaValue) -> core::Result<JsObject, BoaValue> {
    cx.arena
        .get(value)
        .to_object(cx.boa())
        .map_err(|error| insert_error(cx, error))
}

fn insert_error(cx: Context<'_>, error: JsError) -> BoaValue {
    let value = error.to_opaque(cx.boa());
    cx.arena.insert(value)
}

fn named_error(cx: Context<'_>, kind: ErrorKind, name: &str, message: &str) -> BoaValue {
    let native = match kind {
        ErrorKind::Type => JsNativeError::typ().with_message(message.to_owned()),
        ErrorKind::Range => JsNativeError::range().with_message(message.to_owned()),
        ErrorKind::Error => JsNativeError::error().with_message(message.to_owned()),
    };
    let value = native.to_opaque(cx.boa());
    let _ = value.set(JsString::from("name"), JsString::from(name), true, cx.boa());
    cx.arena.insert(value.into())
}

enum ErrorKind {
    Type,
    Range,
    Error,
}

impl core::JsEngine for Engine {
    type Value = BoaValue;
    type Context<'a> = Context<'a>;
    type Error = BoaValue;
    type DeferredRegistration = DeferredRegistration;

    fn environment<'a>(cx: Self::Context<'a>) -> &'a core::Environment {
        &cx.arena.env
    }

    fn get_property(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: &str,
    ) -> core::Result<Self::Value, Self::Error> {
        let object = object(cx, obj)?;
        object
            .get(JsString::from(key), cx.boa())
            .map(|value| cx.arena.insert(value))
            .map_err(|error| insert_error(cx, error))
    }

    fn own_property_names(
        cx: Self::Context<'_>,
        obj: Self::Value,
    ) -> core::Result<Vec<String>, Self::Error> {
        let value = cx.arena.get(obj);
        let keys = BoaObjectBuiltin::keys(&JsValue::undefined(), &[value], cx.boa())
            .map_err(|error| insert_error(cx, error))?;
        let array = JsArray::from_object(
            keys.as_object()
                .ok_or_else(|| Self::operation_error(cx, "Object.keys did not return an array"))?,
        )
        .map_err(|error| insert_error(cx, error))?;
        let len = array
            .length(cx.boa())
            .map_err(|error| insert_error(cx, error))?;
        (0..len)
            .map(|index| {
                array
                    .at(index as i64, cx.boa())
                    .and_then(|value| value.to_string(cx.boa()))
                    .map(|value| value.to_std_string_lossy())
                    .map_err(|error| insert_error(cx, error))
            })
            .collect()
    }

    fn global(cx: Self::Context<'_>) -> Self::Value {
        cx.arena.insert(cx.boa().global_object().into())
    }

    fn get_property_value(
        cx: Self::Context<'_>,
        obj: Self::Value,
        key: Self::Value,
    ) -> core::Result<Self::Value, Self::Error> {
        let object = object(cx, obj)?;
        let key = cx
            .arena
            .get(key)
            .to_property_key(cx.boa())
            .map_err(|error| insert_error(cx, error))?;
        object
            .get(key, cx.boa())
            .map(|value| cx.arena.insert(value))
            .map_err(|error| insert_error(cx, error))
    }

    fn call(
        cx: Self::Context<'_>,
        f: Self::Value,
        this: Self::Value,
        args: &[Self::Value],
    ) -> core::Result<Self::Value, Self::Error> {
        let function = object(cx, f)?;
        let this = cx.arena.get(this);
        let args = args
            .iter()
            .map(|value| cx.arena.get(*value))
            .collect::<Vec<_>>();
        function
            .call(&this, &args, cx.boa())
            .map(|value| cx.arena.insert(value))
            .map_err(|error| insert_error(cx, error))
    }

    fn construct(
        cx: Self::Context<'_>,
        ctor: Self::Value,
        args: &[Self::Value],
    ) -> core::Result<Self::Value, Self::Error> {
        let constructor = object(cx, ctor)?;
        let args = args
            .iter()
            .map(|value| cx.arena.get(*value))
            .collect::<Vec<_>>();
        constructor
            .construct(&args, None, cx.boa())
            .map(|value| cx.arena.insert(value.into()))
            .map_err(|error| insert_error(cx, error))
    }

    fn is_undefined(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena.get(value).is_undefined()
    }

    fn is_null(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena.get(value).is_null()
    }

    fn is_object(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena.get(value).is_object()
    }

    fn is_callable(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena.get(value).is_callable()
    }

    fn same_value(cx: Self::Context<'_>, left: Self::Value, right: Self::Value) -> bool {
        JsValue::same_value(&cx.arena.get(left), &cx.arena.get(right))
    }

    fn is_uint32array(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena
            .get(value)
            .as_object()
            .is_some_and(|object| JsUint32Array::from_object(object.clone()).is_ok())
    }

    fn to_f64(cx: Self::Context<'_>, value: Self::Value) -> core::Result<f64, Self::Error> {
        cx.arena
            .get(value)
            .to_number(cx.boa())
            .map_err(|error| insert_error(cx, error))
    }

    fn to_bool(cx: Self::Context<'_>, value: Self::Value) -> bool {
        cx.arena.get(value).to_boolean()
    }

    fn to_str<'a>(
        cx: Self::Context<'_>,
        value: Self::Value,
        arena: &'a core::Arena,
    ) -> core::Result<&'a str, Self::Error> {
        cx.arena
            .get(value)
            .to_string(cx.boa())
            .map(|value| arena.alloc_str(&value.to_std_string_lossy()))
            .map_err(|error| insert_error(cx, error))
    }

    fn register_class(
        cx: Self::Context<'_>,
        spec: &'static core::ClassSpec<Self>,
    ) -> core::Result<core::ClassId, Self::Error> {
        if cx.arena.classes.borrow().contains_key(&spec.id) {
            return Ok(spec.id);
        }
        let mut initializer = ObjectInitializer::new(cx.boa());
        for property in spec.properties {
            let getter = property.get.map(|callback| {
                NativeFunction::from_copy_closure_with_captures(
                    getter_callback,
                    GetterCapture(callback),
                )
                .to_js_function(initializer.context().realm())
            });
            let setter = property.set.map(|callback| {
                NativeFunction::from_copy_closure_with_captures(
                    setter_callback,
                    SetterCapture(callback),
                )
                .to_js_function(initializer.context().realm())
            });
            initializer.accessor(
                JsString::from(property.name),
                getter,
                setter,
                Attribute::CONFIGURABLE,
            );
        }
        for method in spec.methods {
            initializer.function(
                NativeFunction::from_copy_closure_with_captures(
                    method_callback,
                    MethodCapture(method.call),
                ),
                JsString::from(method.name),
                usize::from(method.length),
            );
        }
        let prototype = initializer.build();
        drop(initializer);
        let prototype_handle = cx.arena.insert(prototype.clone().into());
        cx.arena.duplicate(prototype_handle);

        let constructor = if let Some(constructor_spec) = &spec.constructor {
            FunctionObjectBuilder::new(
                cx.boa().realm(),
                NativeFunction::from_copy_closure_with_captures(
                    constructor_callback,
                    ConstructorCapture(constructor_spec.call),
                ),
            )
            .name(spec.name)
            .length(usize::from(constructor_spec.length))
            .constructor(true)
            .build()
        } else {
            FunctionObjectBuilder::new(
                cx.boa().realm(),
                NativeFunction::from_fn_ptr(illegal_constructor_callback),
            )
            .name(spec.name)
            .length(0)
            .constructor(true)
            .build()
        };
        constructor
            .set(
                JsString::from("prototype"),
                prototype.clone(),
                true,
                cx.boa(),
            )
            .map_err(|error| insert_error(cx, error))?;
        prototype
            .define_property_or_throw(
                JsString::from("constructor"),
                PropertyDescriptor::builder()
                    .value(constructor.clone())
                    .writable(true)
                    .enumerable(false)
                    .configurable(true)
                    .build(),
                cx.boa(),
            )
            .map_err(|error| insert_error(cx, error))?;
        if let Some(parent) = spec
            .constructor
            .as_ref()
            .and_then(|constructor| constructor.parent)
        {
            let parent_prototype = match parent {
                core::ClassParent::Class(parent) => cx
                    .arena
                    .classes
                    .borrow()
                    .get(&parent)
                    .map(|entry| cx.arena.get(entry.prototype))
                    .and_then(|value| value.as_object())
                    .ok_or_else(|| Self::operation_error(cx, "parent class is not registered"))?,
                core::ClassParent::IntrinsicError => cx
                    .boa()
                    .realm()
                    .intrinsics()
                    .constructors()
                    .error()
                    .prototype(),
            };
            if !prototype.set_prototype(Some(parent_prototype)) {
                return Err(Self::operation_error(cx, "failed to set parent prototype"));
            }
        }
        cx.boa()
            .global_object()
            .set(JsString::from(spec.name), constructor, true, cx.boa())
            .map_err(|error| insert_error(cx, error))?;

        cx.arena.classes.borrow_mut().insert(
            spec.id,
            ClassEntry {
                spec,
                prototype: prototype_handle,
            },
        );
        Ok(spec.id)
    }

    fn new_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
    ) -> core::Result<Self::Value, Self::Error> {
        let classes = cx.arena.classes.borrow();
        let entry = classes
            .get(&class)
            .ok_or_else(|| Self::operation_error(cx, "class is not registered"))?;
        let prototype = cx
            .arena
            .get(entry.prototype)
            .as_object()
            .expect("registered prototype is an object");
        let object = ObjectInitializer::with_native_data_and_proto(
            WrapperData {
                class,
                payload: RefCell::new(Some(payload)),
                finalizer: entry.spec.finalizer,
                arena: cx.arena as *const Arena,
            },
            prototype,
            cx.boa(),
        )
        .build();
        Ok(cx.arena.insert(object.into()))
    }

    fn new_error_instance(
        cx: Self::Context<'_>,
        class: core::ClassId,
        payload: Box<dyn Any + Send>,
        name: &str,
        message: &str,
    ) -> core::Result<Self::Value, Self::Error> {
        let value = Self::new_instance(cx, class, payload)?;
        let object = cx
            .arena
            .get(value)
            .as_object()
            .ok_or_else(|| Self::operation_error(cx, "Error instance is not an object"))?;
        let native = named_error(cx, ErrorKind::Error, name, message);
        let native = cx
            .arena
            .get(native)
            .as_object()
            .ok_or_else(|| Self::operation_error(cx, "native Error is not an object"))?;
        let stack = native
            .get(JsString::from("stack"), cx.boa())
            .map_err(|error| insert_error(cx, error))?;
        object
            .define_property_or_throw(
                JsString::from("stack"),
                PropertyDescriptor::builder()
                    .value(stack)
                    .writable(true)
                    .enumerable(false)
                    .configurable(true)
                    .build(),
                cx.boa(),
            )
            .map_err(|error| insert_error(cx, error))?;
        Ok(value)
    }

    fn payload<'a>(
        cx: Self::Context<'a>,
        obj: Self::Value,
        class: core::ClassId,
    ) -> Option<&'a (dyn Any + Send)> {
        let object = cx.arena.get(obj).as_object()?.clone();
        let data = object.downcast_ref::<WrapperData>()?;
        if data.class != class {
            return None;
        }
        let payload = data.payload.borrow();
        let pointer = payload.as_deref()? as *const (dyn Any + Send);
        drop(payload);
        drop(data);
        // SAFETY: the object is rooted by `obj` for the context lifetime and
        // its boxed payload is address-stable until Boa invokes its finalizer.
        Some(unsafe { &*pointer })
    }

    fn undefined(cx: Self::Context<'_>) -> Self::Value {
        cx.arena.insert(JsValue::undefined())
    }

    fn null(cx: Self::Context<'_>) -> Self::Value {
        cx.arena.insert(JsValue::null())
    }

    fn number(cx: Self::Context<'_>, value: f64) -> core::Result<Self::Value, Self::Error> {
        Ok(cx.arena.insert(value.into()))
    }

    fn boolean(cx: Self::Context<'_>, value: bool) -> Self::Value {
        cx.arena.insert(value.into())
    }

    fn string(cx: Self::Context<'_>, value: &str) -> core::Result<Self::Value, Self::Error> {
        Ok(cx.arena.insert(JsString::from(value).into()))
    }

    fn type_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        named_error(cx, ErrorKind::Type, "TypeError", message)
    }

    fn operation_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        named_error(cx, ErrorKind::Error, "OperationError", message)
    }

    fn range_error(cx: Self::Context<'_>, message: &str) -> Self::Error {
        named_error(cx, ErrorKind::Range, "RangeError", message)
    }

    fn async_error_value(cx: Self::Context<'_>, name: &str, message: &str) -> Self::Value {
        named_error(cx, ErrorKind::Error, name, message)
    }

    fn error_value_from_error(_cx: Self::Context<'_>, error: Self::Error) -> Self::Value {
        error
    }

    fn new_promise(
        cx: Self::Context<'_>,
    ) -> core::Result<(Self::Value, core::Deferred<Self>), Self::Error> {
        let (promise, resolvers) = JsPromise::new_pending(cx.boa());
        let promise = cx.arena.insert(promise.into());
        let resolve = cx.arena.insert(resolvers.resolve.into());
        let reject = cx.arena.insert(resolvers.reject.into());
        cx.arena.retain_from_scope(resolve);
        cx.arena.retain_from_scope(reject);
        Ok((promise, core::Deferred::new(resolve, reject)))
    }

    fn settle_deferreds(
        cx: Self::Context<'_>,
        settlements: Vec<core::DeferredSettlement<Self>>,
    ) -> core::Result<(), Self::Error> {
        if settlements.is_empty() {
            return Ok(());
        }
        let Some(trampoline) = *cx.arena.settle_trampoline.borrow() else {
            for (deferred, _) in settlements {
                cx.arena.release(deferred.resolve());
                cx.arena.release(deferred.reject());
            }
            return Err(Self::operation_error(
                cx,
                "settlement trampoline is unavailable",
            ));
        };
        let mut functions = Vec::with_capacity(settlements.len());
        let mut values = Vec::with_capacity(settlements.len());
        let mut selected_functions = Vec::with_capacity(settlements.len());
        for (deferred, result) in settlements {
            let (selected, unused, value) = match result {
                Ok(value) => (deferred.resolve(), deferred.reject(), value),
                Err(value) => (deferred.reject(), deferred.resolve(), value),
            };
            cx.arena.release(unused);
            functions.push(cx.arena.get(selected));
            values.push(cx.arena.get(value));
            selected_functions.push(selected);
        }
        let function_array = JsArray::from_iter(functions, cx.boa());
        let value_array = JsArray::from_iter(values, cx.boa());
        let result = cx
            .arena
            .get(trampoline)
            .as_object()
            .expect("settlement trampoline is callable")
            .call(
                &JsValue::undefined(),
                &[function_array.into(), value_array.into()],
                cx.boa(),
            )
            .map(|_| ())
            .map_err(|error| insert_error(cx, error));
        for function in selected_functions {
            cx.arena.release(function);
        }
        result
    }

    fn drain_microtasks(cx: Self::Context<'_>) -> core::Result<(), Self::Error> {
        cx.boa().run_jobs().map_err(|error| insert_error(cx, error))
    }

    fn new_arraybuffer_copy(
        cx: Self::Context<'_>,
        bytes: &[u8],
    ) -> core::Result<Self::Value, Self::Error> {
        let block = AlignedVec::from_iter(0, bytes.iter().copied());
        JsArrayBuffer::from_byte_block(block, cx.boa())
            .map(|buffer| cx.arena.insert(buffer.into()))
            .map_err(|error| insert_error(cx, error))
    }

    fn detach_arraybuffer(
        cx: Self::Context<'_>,
        value: Self::Value,
        out: Option<&mut [u8]>,
    ) -> core::Result<(), Self::Error> {
        let buffer = cx
            .arena
            .get(value)
            .as_object()
            .ok_or_else(|| Self::type_error(cx, "value is not an ArrayBuffer"))
            .and_then(|object| {
                JsArrayBuffer::from_object(object).map_err(|error| insert_error(cx, error))
            })?;
        let bytes = buffer
            .detach(&JsValue::undefined())
            .map_err(|error| insert_error(cx, error))?;
        if let Some(out) = out {
            if out.len() != bytes.len() {
                return Err(Self::type_error(cx, "ArrayBuffer length mismatch"));
            }
            out.copy_from_slice(&bytes);
        }
        Ok(())
    }

    fn arraybuffer_len(cx: Self::Context<'_>, value: Self::Value) -> Option<usize> {
        let object = cx.arena.get(value).as_object()?.clone();
        JsArrayBuffer::from_object(object)
            .ok()
            .map(|buffer| buffer.byte_length())
    }

    fn arraybuffer_copy(cx: Self::Context<'_>, value: Self::Value) -> Option<Vec<u8>> {
        let object = cx.arena.get(value).as_object()?.clone();
        JsArrayBuffer::from_object(object)
            .ok()?
            .data()
            .map(|data| data.to_vec())
    }

    fn duplicate_value(cx: Self::Context<'_>, value: Self::Value) -> Self::Value {
        cx.arena.duplicate(value);
        value
    }

    fn return_held_value(_cx: Self::Context<'_>, held: Self::Value) -> Self::Value {
        held
    }

    fn release_value(cx: Self::Context<'_>, value: Self::Value) {
        cx.arena.release(value);
    }

    fn register_deferred(
        cx: Self::Context<'_>,
        slot: NonNull<Option<core::Deferred<Self>>>,
    ) -> Self::DeferredRegistration {
        cx.arena.drain_registration_removals();
        let id = cx.arena.next_registration.get();
        cx.arena.next_registration.set(id.wrapping_add(1));
        cx.arena.outstanding_deferreds.borrow_mut().insert(id, slot);
        DeferredRegistration {
            id,
            removals: Arc::clone(&cx.arena.registration_removals),
        }
    }

    fn release_deferred(cx: Self::Context<'_>, deferred: core::Deferred<Self>) {
        cx.arena.release(deferred.resolve());
        cx.arena.release(deferred.reject());
    }
}

fn value_string(cx: Context<'_>, value: BoaValue) -> String {
    cx.arena.get(value).to_string(cx.boa()).map_or_else(
        |_| "Boa exception".to_owned(),
        |value| value.to_std_string_lossy(),
    )
}

fn js_error_string(error: JsError, context: &mut BoaContext) -> String {
    error.to_opaque(context).to_string(context).map_or_else(
        |_| "Boa exception".to_owned(),
        |value| value.to_std_string_lossy(),
    )
}

fn gpu_dispatch() -> core::GpuDispatch {
    core::for_each_gpu_dispatch_entry!(__gpu_dispatch_from_ffi, ffi_wgpu)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::fs;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::{Path, PathBuf};
    use std::ptr;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use boa_engine::object::ObjectInitializer;
    use boa_engine::{JsData, JsValue};
    use boa_gc::{Finalize, Trace};

    use super::{
        core, ffi_wgpu as wgpu, BoaValue, Engine, HostValue, ModuleEvaluationStatus, Runtime,
    };
    use webgpu_native_js_core::JsEngine;

    struct FinalizeProbe {
        finalized: Arc<AtomicUsize>,
    }

    impl Finalize for FinalizeProbe {
        fn finalize(&self) {
            self.finalized.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl JsData for FinalizeProbe {}

    // SAFETY: FinalizeProbe contains only an Arc to an atomic counter and no
    // Boa GC pointers.
    unsafe impl Trace for FinalizeProbe {
        boa_gc::empty_trace!();
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
            // SAFETY: these are the three create/request references owned by
            // NativeSetup and are released in child-before-parent order.
            unsafe {
                wgpu::wgpuDeviceRelease(self.device);
                wgpu::wgpuAdapterRelease(self.adapter);
                wgpu::wgpuInstanceRelease(self.instance);
            }
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
            // SAFETY: userdata1 is the one Rc raw clone created for this
            // one-shot callback and has not been reclaimed elsewhere.
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
            // SAFETY: userdata1 is the one Rc raw clone created for this
            // one-shot callback and has not been reclaimed elsewhere.
            let state = unsafe { Rc::from_raw(userdata1.cast::<DeviceRequestState>()) };
            state.status.set(status);
            state.handle.set(device);
        }));
    }

    fn native_setup() -> NativeSetup {
        // SAFETY: a null descriptor requests the backend's default instance.
        let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        assert!(!instance.is_null());

        let adapter_state = Rc::new(AdapterRequestState::new());
        let adapter_userdata = Rc::into_raw(Rc::clone(&adapter_state)).cast_mut().cast();
        let adapter_info = wgpu::WGPURequestAdapterCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(adapter_callback),
            userdata1: adapter_userdata,
            userdata2: ptr::null_mut(),
        };
        // SAFETY: instance is live and adapter_userdata stays allocated until
        // the AllowProcessEvents callback consumes its Rc clone.
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

        let device_state = Rc::new(DeviceRequestState::new());
        let device_userdata = Rc::into_raw(Rc::clone(&device_state)).cast_mut().cast();
        let device_info = wgpu::WGPURequestDeviceCallbackInfo {
            nextInChain: ptr::null_mut(),
            mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
            callback: Some(device_callback),
            userdata1: device_userdata,
            userdata2: ptr::null_mut(),
        };
        // SAFETY: adapter is live and device_userdata stays allocated until
        // the AllowProcessEvents callback consumes its Rc clone.
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

    fn retained_value(runtime: &Runtime, value: JsValue) -> BoaValue {
        runtime.with_scope(|cx| {
            let value = cx.arena.insert(value);
            Engine::duplicate_value(cx, value)
        })
    }

    fn eval_js(runtime: &Runtime, source: &str, name: &str) -> JsValue {
        let value = runtime.eval(source, name).expect(name);
        let js_value = runtime.arena.get(value);
        runtime.arena.release(value);
        js_value
    }

    fn eval_bool(runtime: &Runtime, source: &str, name: &str) -> bool {
        eval_js(runtime, source, name).to_boolean()
    }

    fn eval_number(runtime: &Runtime, source: &str, name: &str) -> Option<f64> {
        eval_js(runtime, source, name).as_number()
    }

    fn eval_string(runtime: &Runtime, source: &str, name: &str) -> String {
        // SAFETY: the test is single-threaded and no other Boa call overlaps conversion.
        let context = unsafe { &mut *runtime.raw_context() };
        eval_js(runtime, source, name)
            .to_string(context)
            .expect("stringify evaluation")
            .to_std_string_lossy()
    }

    struct TempModules(PathBuf);

    impl TempModules {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "webgpu-native-js-boa-modules-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).expect("create module temp directory");
            Self(path)
        }

        fn write(&self, name: &str, source: &str) -> PathBuf {
            let path = self.0.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create module fixture parent");
            }
            fs::write(&path, source).expect("write module fixture");
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempModules {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn tick_until(runtime: &Runtime, instance: wgpu::WGPUInstance, expression: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: the caller's NativeSetup keeps instance live, and tests
            // invoke this helper only on the runtime's owning thread.
            unsafe { runtime.tick(instance) }.expect("tick while waiting");
            if eval_bool(runtime, expression, "wait-condition.js") {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "condition timed out: {expression}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn public_runtime_and_copy_handles_work() {
        let runtime = Runtime::new().expect("Boa runtime");
        assert_eq!(eval_number(&runtime, "1 + 2", "smoke.js"), Some(3.0));
    }

    #[test]
    fn runtime_new_creates_independent_contexts() {
        let first = Runtime::new().expect("first Boa runtime");
        let second = Runtime::new().expect("second Boa runtime");
        first
            .eval("globalThis.onlyFirst = 7", "first.js")
            .expect("set first global");
        assert!(eval_bool(
            &second,
            "typeof globalThis.onlyFirst === 'undefined'",
            "second.js"
        ));
    }

    #[test]
    fn runtime_eval_returns_values_and_surfaces_exceptions() {
        let runtime = Runtime::new().expect("Boa runtime");
        assert_eq!(eval_number(&runtime, "6 * 7", "value.js"), Some(42.0));
        let error = runtime
            .eval("throw new TypeError('eval boom')", "throw.js")
            .expect_err("evaluation must fail");
        assert!(error.to_string().contains("eval boom"));
    }

    #[test]
    fn register_host_function_converts_arguments_and_surfaces_errors() {
        let runtime = Runtime::new().expect("Boa runtime");
        let recorded = Rc::new(std::cell::RefCell::new(Vec::new()));
        let captured = Rc::clone(&recorded);
        runtime
            .register_host_function("record", move |args| {
                captured.borrow_mut().extend_from_slice(args);
                Ok(())
            })
            .expect("register record");
        runtime
            .register_host_function("reject", |_| Err("host rejected call".to_owned()))
            .expect("register reject");
        eval_js(
            &runtime,
            "record('text', 3.5, true, null, undefined, { toString() { return 'object'; } });",
            "host-arguments.js",
        );
        assert_eq!(
            *recorded.borrow(),
            [
                HostValue::String("text".to_owned()),
                HostValue::Number(3.5),
                HostValue::Bool(true),
                HostValue::Null,
                HostValue::Undefined,
                HostValue::String("object".to_owned()),
            ]
        );
        let error = runtime
            .eval("reject()", "host-error.js")
            .expect_err("host error must throw");
        assert!(error.to_string().contains("host rejected call"), "{error}");
    }

    #[test]
    fn register_host_function_with_result_returns_all_primitives() {
        let runtime = Runtime::new().expect("Boa runtime");
        for (name, value) in [
            ("hostString", HostValue::String("text".to_owned())),
            ("hostNumber", HostValue::Number(3.5)),
            ("hostTrue", HostValue::Bool(true)),
            ("hostNull", HostValue::Null),
            ("hostUndefined", HostValue::Undefined),
        ] {
            runtime
                .register_host_function_with_result(name, move |_| Ok(value.clone()))
                .expect("register primitive result");
        }
        assert!(eval_bool(
            &runtime,
            "hostString() === 'text' && hostNumber() === 3.5 && hostTrue() === true && hostNull() === null && hostUndefined() === undefined",
            "host-results.js",
        ));
    }

    #[test]
    fn clear_global_assigns_undefined_and_releases_its_handle() {
        let runtime = Runtime::new().expect("Boa runtime");
        let value = runtime.eval("41", "clear-value.js").expect("value");
        runtime
            .set_global_value("clearTarget", value)
            .expect("set clear target");
        runtime.clear_global("clearTarget").expect("clear target");
        assert!(eval_bool(
            &runtime,
            "globalThis.clearTarget === undefined",
            "clear-result.js",
        ));
    }

    #[test]
    fn eval_module_loads_a_relative_import_chain() {
        let files = TempModules::new();
        files.write("value.mjs", "export const answer = 42;");
        let entry = files.write(
            "entry.mjs",
            "import { answer } from './value.mjs'; globalThis.moduleAnswer = answer;",
        );
        let runtime = Runtime::new().expect("Boa runtime");
        let evaluation = runtime.eval_module(&entry).expect("evaluate module");
        assert_eq!(
            evaluation.status().expect("module status"),
            ModuleEvaluationStatus::Fulfilled
        );
        assert_eq!(
            eval_number(&runtime, "globalThis.moduleAnswer", "module-result.js"),
            Some(42.0)
        );
    }

    #[test]
    fn set_module_alias_resolves_an_exact_specifier() {
        let files = TempModules::new();
        let aliased = files.write("owner.mjs", "export const revision = 'owner';");
        let entry = files.write(
            "alias-entry.mjs",
            "import { revision } from 'three'; globalThis.aliasRevision = revision;",
        );
        let runtime = Runtime::new().expect("Boa runtime");
        runtime
            .set_module_alias("three", &aliased)
            .expect("set alias");
        runtime.eval_module(&entry).expect("evaluate alias module");
        assert!(eval_bool(
            &runtime,
            "globalThis.aliasRevision === 'owner'",
            "alias-result.js",
        ));
    }

    #[test]
    fn set_module_transform_applies_to_root_and_imported_modules() {
        let files = TempModules::new();
        files.write("value.mjs", "export const value = __IMPORTED_MARKER__;");
        let entry = files.write(
            "transform-entry.mjs",
            "import { value } from './value.mjs'; globalThis.transformValue = value + __ROOT_MARKER__;",
        );
        let runtime = Runtime::new().expect("Boa runtime");
        runtime.set_module_transform(|source, _| {
            Ok(source
                .replace("__IMPORTED_MARKER__", "29")
                .replace("__ROOT_MARKER__", "31"))
        });
        runtime
            .eval_module(&entry)
            .expect("evaluate transformed module");
        assert_eq!(
            eval_number(&runtime, "globalThis.transformValue", "transform-result.js"),
            Some(60.0)
        );
    }

    #[test]
    fn module_identity_lexically_normalizes_dot_and_parent_diamonds() {
        let files = TempModules::new();
        files.write(
            "sub/x.mjs",
            "globalThis.diamondEvaluations = (globalThis.diamondEvaluations ?? 0) + 1; export const shared = {};",
        );
        files.write(
            "sub/nested/y.mjs",
            "import { shared } from '.././x.mjs'; export { shared };",
        );
        let entry = files.write(
            "diamond-entry.mjs",
            "import { shared as direct } from './sub/./x.mjs'; import { shared as indirect } from './sub/nested/../nested/y.mjs'; globalThis.diamondSame = direct === indirect;",
        );
        let runtime = Runtime::new().expect("Boa runtime");
        runtime.eval_module(&entry).expect("evaluate diamond");
        assert!(eval_bool(
            &runtime,
            "globalThis.diamondEvaluations === 1 && globalThis.diamondSame",
            "diamond-result.js",
        ));
        assert!(runtime
            .module_loader
            .modules
            .borrow()
            .contains_key(&files.path().join("sub/x.mjs")));
    }

    #[test]
    fn run_gc_collects_an_unrooted_native_object() {
        let runtime = Runtime::new().expect("Boa runtime");
        let finalized = Arc::new(AtomicUsize::new(0));
        runtime.with_scope(|cx| {
            ObjectInitializer::with_native_data(
                FinalizeProbe {
                    finalized: Arc::clone(&finalized),
                },
                cx.boa(),
            )
            .build();
        });
        runtime.run_gc();
        assert_eq!(finalized.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn set_global_value_adopts_the_handle_and_sets_the_property() {
        let runtime = Runtime::new().expect("Boa runtime");
        let value = retained_value(&runtime, JsValue::from(37));
        runtime
            .set_global_value("hostValue", value)
            .expect("set global value");
        assert!(eval_bool(
            &runtime,
            "globalThis.hostValue === 37",
            "global-value.js"
        ));
        assert!(runtime.arena.slots.borrow()[value.0 as usize].is_none());

        runtime
            .eval(
                "Object.defineProperty(globalThis, 'lockedValue', { value: 1, writable: false, configurable: false })",
                "locked-global.js",
            )
            .expect("define locked global");
        let rejected = retained_value(&runtime, JsValue::from(99));
        assert!(
            runtime.set_global_value("lockedValue", rejected).is_err(),
            "non-writable global assignment must fail"
        );
        assert!(
            runtime.arena.slots.borrow()[rejected.0 as usize].is_none(),
            "failed assignment must still adopt and release its input handle"
        );
    }

    #[test]
    fn wrap_device_returns_a_working_gpu_device_wrapper() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("wrappedDevice", device)
            .expect("set wrapped device");
        assert!(eval_bool(
            &runtime,
                "typeof wrappedDevice.createBuffer === 'function' && wrappedDevice.queue === wrappedDevice.queue",
                "wrapped-device.js",
        ));
    }

    #[test]
    fn install_exposes_eager_non_constructible_interface_objects() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("interfaceDevice", device)
            .expect("set device");
        assert!(eval_bool(
            &runtime,
            r#"
                const passTypeReady = typeof GPURenderPassEncoder === "function";
                const passMethodReady = "setBindGroup" in GPURenderPassEncoder.prototype;
                const descriptorIsWebIdl = (prototype, interfaceObject) => {
                    const descriptor = Object.getOwnPropertyDescriptor(prototype, "constructor");
                    return descriptor.value === interfaceObject && descriptor.writable === true &&
                        descriptor.enumerable === false && descriptor.configurable === true;
                };
                const constructibleConstructorDescriptor = descriptorIsWebIdl(
                    GPURenderPassEncoder.prototype,
                    GPURenderPassEncoder
                );
                const nonConstructibleConstructorDescriptor = descriptorIsWebIdl(
                    GPUSupportedLimits.prototype,
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
                passTypeReady && passMethodReady && constructibleConstructorDescriptor &&
                    nonConstructibleConstructorDescriptor && instanceReady &&
                    callError instanceof TypeError && callError.message.includes("Illegal constructor") &&
                    constructError instanceof TypeError && constructError.message.includes("Illegal constructor")
            "#,
            "eager-interface-objects.js",
        ));
    }

    #[test]
    fn gpu_pipeline_error_is_a_constructible_error_subclass() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let _device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        assert!(eval_bool(
            &runtime,
            r#"
                (() => {
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
                    return validation.name === "GPUPipelineError" &&
                        validation.message === "pipeline failed" &&
                        validation.reason === "validation" &&
                        internal.message === "" && internal.reason === "internal" &&
                        validation instanceof GPUPipelineError &&
                        validation instanceof DOMException && validation instanceof Error &&
                        typeof validation.stack === typeof new Error("pipeline failed").stack &&
                        missingIsTypeError && invalidIsTypeError;
                })()
            "#,
            "gpu-pipeline-error.js",
        ));
    }

    #[test]
    fn wrap_gpu_returns_a_working_gpu_wrapper() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live instance until after runtime teardown.
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap GPU");
        runtime
            .set_global_value("wrappedGpu", gpu)
            .expect("set wrapped GPU");
        assert!(eval_bool(
            &runtime,
            "typeof wrappedGpu.requestAdapter === 'function'",
            "wrapped-gpu.js",
        ));
    }

    #[test]
    fn device_event_forwarder_handles_registered_and_unknown_devices() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("eventDevice", device)
            .expect("set event device");
        let forwarder = runtime.device_event_forwarder();
        assert_eq!(
            forwarder.forward_device_lost(
                ptr::null_mut(),
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "unknown",
            ),
            Err(core::QueueError::UnknownDevice)
        );
        forwarder
            .forward_device_lost(
                setup.device,
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "direct forwarder loss",
            )
            .expect("forward registered loss");
    }

    #[test]
    fn shared_device_event_script_survives_gc_and_tick() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        runtime
            .eval(
                include_str!("../../../tests/device-events.js"),
                "device-events.js",
            )
            .expect("install listeners");
        runtime.run_gc();
        runtime
            .forward_uncaptured_error(
                setup.device,
                wgpu::WGPUErrorType_WGPUErrorType_Validation,
                "script uncaptured",
            )
            .expect("forward uncaptured");
        runtime
            .forward_device_lost(
                setup.device,
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "script lost",
            )
            .expect("forward lost");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: setup keeps the instance live on the runtime thread.
            unsafe { runtime.tick(setup.instance) }.expect("device-event tick");
            if eval_bool(
                &runtime,
                "uncapturedEventPassed && uncapturedListenerPassed && deviceLostPassed",
                "device-events-check.js",
            ) {
                break;
            }
            assert!(Instant::now() < deadline, "device event script timed out");
        }
    }

    #[test]
    fn runtime_forward_device_lost_enqueues_for_a_registered_wrapper() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("lostDevice", device)
            .expect("set lost device");
        assert_eq!(
            runtime.forward_device_lost(
                ptr::null_mut(),
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "unknown runtime device",
            ),
            Err(core::QueueError::UnknownDevice)
        );
        runtime
            .forward_device_lost(
                setup.device,
                wgpu::WGPUDeviceLostReason_WGPUDeviceLostReason_Destroyed,
                "runtime forward loss",
            )
            .expect("forward device loss");
        // SAFETY: setup keeps the instance live and this runs on the runtime thread.
        unsafe { runtime.tick(setup.instance) }.expect("settle lost promise");
        runtime
            .eval(
                "lostDevice.lost.then(info => { globalThis.forwardedReason = info.reason; })",
                "observe-lost.js",
            )
            .expect("observe lost promise");
        // SAFETY: setup keeps the instance live and this runs on the runtime thread.
        unsafe { runtime.tick(setup.instance) }.expect("drain lost observer");
        assert!(eval_bool(
            &runtime,
            "globalThis.forwardedReason === 'destroyed'",
            "lost-result.js",
        ));
    }

    #[test]
    fn runtime_tick_processes_an_empty_tick() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup keeps the instance live and this runs on the runtime thread.
        let drained = unsafe { runtime.tick(setup.instance) }.expect("empty tick");
        assert_eq!(drained, 0);
    }

    #[test]
    fn drain_releases_reports_an_empty_queue() {
        let runtime = Runtime::new().expect("Boa runtime");
        assert_eq!(runtime.drain_releases().expect("drain releases"), 0);
    }

    #[test]
    fn native_render_bundle_is_class_checked_and_borrows_the_native_handle() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("bundleDevice", device)
            .expect("set wrapped device");
        let bundle = runtime
            .eval(
                "bundleDevice.createRenderBundleEncoder({ colorFormats: ['rgba8unorm'] }).finish()",
                "native-render-bundle.js",
            )
            .expect("create render bundle");
        let wrong = runtime
            .eval("bundleDevice", "wrong-render-bundle.js")
            .expect("get wrong wrapper class");

        let native = runtime
            .native_render_bundle(bundle)
            .expect("render bundle native handle");
        assert!(!native.is_null());
        assert_eq!(runtime.native_render_bundle(wrong), None);

        runtime.arena.release(wrong);
        runtime.arena.release(bundle);
    }

    #[test]
    fn arena_duplicate_release_and_slot_reuse_are_exact() {
        let runtime = Runtime::new().expect("Boa runtime");
        let first = runtime.with_scope(|cx| {
            let value = Engine::string(cx, "rooted").expect("string");
            assert_eq!(
                cx.arena.slots.borrow()[value.0 as usize]
                    .as_ref()
                    .unwrap()
                    .refs,
                1
            );
            Engine::duplicate_value(cx, value);
            assert_eq!(
                cx.arena.slots.borrow()[value.0 as usize]
                    .as_ref()
                    .unwrap()
                    .refs,
                2
            );
            Engine::release_value(cx, value);
            assert_eq!(
                cx.arena.slots.borrow()[value.0 as usize]
                    .as_ref()
                    .unwrap()
                    .refs,
                1
            );
            value
        });
        assert!(runtime.arena.slots.borrow()[first.0 as usize].is_none());

        let reused = runtime.with_scope(|cx| Engine::number(cx, 9.0).expect("number"));
        assert_eq!(reused.0, first.0, "released arena slot must be reused");
        assert!(runtime.arena.slots.borrow()[reused.0 as usize].is_none());
    }

    #[test]
    fn arena_root_survives_gc_and_full_release_allows_collection() {
        let runtime = Runtime::new().expect("Boa runtime");
        let finalized = Arc::new(AtomicUsize::new(0));
        let held = runtime.with_scope(|cx| {
            let object = ObjectInitializer::with_native_data(
                FinalizeProbe {
                    finalized: Arc::clone(&finalized),
                },
                cx.boa(),
            )
            .build();
            let value = cx.arena.insert(object.into());
            Engine::duplicate_value(cx, value)
        });

        boa_gc::force_collect();
        assert_eq!(finalized.load(Ordering::Relaxed), 0, "arena root was lost");
        runtime.with_scope(|cx| Engine::release_value(cx, held));
        boa_gc::force_collect();
        assert_eq!(
            finalized.load(Ordering::Relaxed),
            1,
            "fully released object remained rooted"
        );
    }

    #[test]
    fn checked_arraybuffer_detach_returns_bytes_and_rejects_second_detach() {
        let runtime = Runtime::new().expect("Boa runtime");
        runtime.with_scope(|cx| {
            let buffer = Engine::new_arraybuffer_copy(cx, &[1, 2, 3, 4]).expect("buffer");
            let mut copied = [0; 4];
            Engine::detach_arraybuffer(cx, buffer, Some(&mut copied)).expect("checked detach");
            assert_eq!(copied, [1, 2, 3, 4]);
            assert_eq!(Engine::arraybuffer_len(cx, buffer), Some(0));
            assert!(
                Engine::detach_arraybuffer(cx, buffer, None).is_err(),
                "a failed checked detach must be a hard error"
            );
        });
    }

    #[test]
    fn copy_in_copy_out_writes_reach_native_bytes_at_unmap() {
        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns the live device until after runtime teardown.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        runtime
            .set_global_value("copyDevice", device)
            .expect("set copy device");
        runtime
            .eval(
                r#"
                globalThis.copyBackDone = false;
                globalThis.copyBackBytes = [];
                const source = copyDevice.createBuffer({ size: 4, usage: 4, mappedAtCreation: true });
                new Uint8Array(source.getMappedRange()).set([11, 22, 33, 44]);
                source.unmap();
                const readback = copyDevice.createBuffer({ size: 4, usage: 9 });
                const encoder = copyDevice.createCommandEncoder();
                encoder.copyBufferToBuffer(source, 0, readback, 0, 4);
                copyDevice.queue.submit([encoder.finish()]);
                copyDevice.queue.onSubmittedWorkDone()
                    .then(() => readback.mapAsync(1, 0, 4))
                    .then(() => {
                        globalThis.copyBackBytes = Array.from(new Uint8Array(readback.getMappedRange()));
                        readback.unmap();
                        source.destroy();
                        readback.destroy();
                        globalThis.copyBackDone = true;
                    });
                "#,
                "copy-back.js",
            )
            .expect("run copy-back script");
        tick_until(&runtime, setup.instance, "globalThis.copyBackDone");
        assert!(eval_bool(
            &runtime,
            "globalThis.copyBackBytes.join(',') === '11,22,33,44'",
            "copy-back-result.js",
        ));
    }

    #[test]
    fn promise_settlement_is_batched_and_microtasks_require_explicit_drain() {
        let runtime = Runtime::new().expect("Boa runtime");
        let (first_promise, first_deferred, second_promise, second_deferred) =
            runtime.with_scope(|cx| {
                let (first, first_deferred) = Engine::new_promise(cx).expect("first promise");
                let (second, second_deferred) = Engine::new_promise(cx).expect("second promise");
                (
                    Engine::duplicate_value(cx, first),
                    first_deferred,
                    Engine::duplicate_value(cx, second),
                    second_deferred,
                )
            });
        runtime
            .set_global_value("firstPromise", first_promise)
            .expect("set first promise");
        runtime
            .set_global_value("secondPromise", second_promise)
            .expect("set second promise");
        runtime
            .eval(
                "globalThis.settlementOrder = []; firstPromise.then(v => settlementOrder.push('first:' + v)); secondPromise.then(v => settlementOrder.push('unexpected:' + v), e => settlementOrder.push('second-error:' + e));",
                "promise-observers.js",
            )
            .expect("install promise observers");

        runtime.with_scope(|cx| {
            let first = Engine::string(cx, "one").expect("first value");
            let second = Engine::string(cx, "two").expect("second value");
            Engine::settle_deferreds(
                cx,
                vec![(first_deferred, Ok(first)), (second_deferred, Err(second))],
            )
            .expect("batched settlement");
        });
        assert!(eval_bool(
            &runtime,
            "globalThis.settlementOrder.length === 0",
            "before-microtasks.js",
        ));
        runtime.with_scope(|cx| Engine::drain_microtasks(cx).expect("drain microtasks"));
        assert!(eval_bool(
            &runtime,
            "globalThis.settlementOrder.join(',') === 'first:one,second-error:two'",
            "after-microtasks.js",
        ));
    }

    #[test]
    fn shared_j17_parity_script_matches_expected_output() {
        const SCRIPT: &str = include_str!("../../../tests/parity/parity.js");
        const EXPECTED: &str = include_str!("../../../tests/parity/expected.txt");

        let setup = native_setup();
        let runtime = Runtime::new().expect("Boa runtime");
        // SAFETY: setup owns live device and instance references for the full
        // lifetime of the runtime and its wrappers.
        let device = unsafe { runtime.wrap_device(setup.device) }.expect("wrap device");
        // SAFETY: setup owns the live instance for the wrapper's lifetime.
        let gpu = unsafe { runtime.wrap_gpu(setup.instance) }.expect("wrap gpu");
        runtime
            .set_global_value("device", device)
            .expect("set device");
        runtime.set_global_value("gpu", gpu).expect("set gpu");
        runtime
            .eval(SCRIPT, "tests/parity/parity.js")
            .expect("evaluate parity script");
        runtime
            .device_event_forwarder()
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

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: setup keeps the instance live and this loop runs on the
            // runtime's owning thread.
            unsafe { runtime.tick(setup.instance) }.expect("parity tick");
            let done = eval_bool(
                &runtime,
                "Boolean(globalThis.parityDone)",
                "tests/parity/done.js",
            );
            if done {
                break;
            }
            assert!(Instant::now() < deadline, "parity script timed out");
            std::thread::sleep(Duration::from_millis(1));
        }

        let actual = format!(
            "{}\n",
            eval_string(
                &runtime,
                "globalThis.parityLog.join('\\n')",
                "tests/parity/join.js",
            )
        );
        assert_eq!(actual, EXPECTED);
    }
}
