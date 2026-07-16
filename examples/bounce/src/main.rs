use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[cfg(not(feature = "engine-jsc"))]
use boa_adapter::{HostValue, Runtime};
#[cfg(feature = "engine-jsc")]
use javascriptcore_adapter::{HostValue, Runtime};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use webgpu_native_js_ffi::native as wgpu;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const BOUNCE_SOURCE: &str = include_str!("../bounce.js");
const EXPECTED_OUTPUT: &str = include_str!("../expected.txt");
const INIT_DEADLINE: Duration = Duration::from_secs(10);
const VERIFY_FRAMES: u64 = 90;

fn host_value_text(value: &HostValue) -> String {
    match value {
        HostValue::String(value) => value.clone(),
        HostValue::Number(value) => value.to_string(),
        HostValue::Bool(value) => value.to_string(),
        HostValue::Null => "null".to_owned(),
        HostValue::Undefined => "undefined".to_owned(),
    }
}

fn host_print_line(args: &[HostValue], fixed_numbers: bool) -> String {
    if fixed_numbers {
        if let [HostValue::Number(x), HostValue::Number(y)] = args {
            return format!("{x:.6},{y:.6}");
        }
    }
    args.iter()
        .map(host_value_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn string_view_lossy(view: wgpu::WGPUStringView) -> String {
    if view.data.is_null() || view.length == usize::MAX {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length) };
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(feature = "backend-yawgpu")]
mod yawgpu_backend {
    use std::ptr;

    use webgpu_native_js_ffi::native as wgpu;

    // Mirrored from yawgpu's vendor header `yawgpu/ffi/webgpu-headers/yawgpu.h`
    // (https://github.com/infosia/yawgpu); the canonical webgpu-headers
    // bindings stay vendor-free.
    const YAWGPU_STYPE_INSTANCE_BACKEND_SELECT: wgpu::WGPUSType = 0x70000001;
    const YAWGPU_BACKEND_NOOP: u32 = 0;
    const YAWGPU_BACKEND_METAL: u32 = 1;
    const YAWGPU_BACKEND_VULKAN: u32 = 2;
    const YAWGPU_BACKEND_GLES: u32 = 3;

    #[repr(C)]
    struct YaWGPUInstanceBackendSelect {
        chain: wgpu::WGPUChainedStruct,
        backend: u32,
    }

    pub fn create_instance() -> Result<wgpu::WGPUInstance, String> {
        let requested = match std::env::var("YAWGPU_BACKEND") {
            Ok(value) => value,
            Err(std::env::VarError::NotPresent) => String::new(),
            Err(error) => return Err(format!("YAWGPU_BACKEND is not readable: {error}")),
        };
        let backend = match requested.as_str() {
            "" | "noop" => YAWGPU_BACKEND_NOOP,
            "metal" => YAWGPU_BACKEND_METAL,
            "vulkan" => YAWGPU_BACKEND_VULKAN,
            "gles" => YAWGPU_BACKEND_GLES,
            other => {
                return Err(format!(
                    "unknown YAWGPU_BACKEND value {other:?}; accepted values are \
                     noop, metal, vulkan, and gles"
                ));
            }
        };
        if backend == YAWGPU_BACKEND_NOOP {
            let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
            return if instance.is_null() {
                Err("wgpuCreateInstance returned null".to_owned())
            } else {
                Ok(instance)
            };
        }
        let mut select = YaWGPUInstanceBackendSelect {
            chain: wgpu::WGPUChainedStruct {
                next: ptr::null_mut(),
                sType: YAWGPU_STYPE_INSTANCE_BACKEND_SELECT,
            },
            backend,
        };
        let descriptor = wgpu::WGPUInstanceDescriptor {
            nextInChain: ptr::from_mut(&mut select.chain),
            requiredFeatureCount: 0,
            requiredFeatures: ptr::null(),
            requiredLimits: ptr::null(),
        };
        let instance = unsafe { wgpu::wgpuCreateInstance(&descriptor) };
        if instance.is_null() {
            Err(format!(
                "wgpuCreateInstance returned null (YAWGPU_BACKEND={requested})"
            ))
        } else {
            Ok(instance)
        }
    }
}

#[cfg(feature = "backend-yawgpu")]
fn create_instance() -> Result<wgpu::WGPUInstance, String> {
    yawgpu_backend::create_instance()
}

#[cfg(not(feature = "backend-yawgpu"))]
fn create_instance() -> Result<wgpu::WGPUInstance, String> {
    let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
    if instance.is_null() {
        Err("wgpuCreateInstance returned null".to_owned())
    } else {
        Ok(instance)
    }
}

struct AdapterRequest {
    done: Cell<bool>,
    status: Cell<wgpu::WGPURequestAdapterStatus>,
    adapter: Cell<wgpu::WGPUAdapter>,
    message: RefCell<String>,
}

unsafe extern "C" fn adapter_callback(
    status: wgpu::WGPURequestAdapterStatus,
    adapter: wgpu::WGPUAdapter,
    message: wgpu::WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    if userdata1.is_null() {
        return;
    }
    let state = unsafe { Rc::from_raw(userdata1.cast::<AdapterRequest>()) };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        state.status.set(status);
        state.adapter.set(adapter);
        *state.message.borrow_mut() = string_view_lossy(message);
        state.done.set(true);
    }));
}

struct DeviceRequest {
    done: Cell<bool>,
    status: Cell<wgpu::WGPURequestDeviceStatus>,
    device: Cell<wgpu::WGPUDevice>,
    message: RefCell<String>,
}

unsafe extern "C" fn device_callback(
    status: wgpu::WGPURequestDeviceStatus,
    device: wgpu::WGPUDevice,
    message: wgpu::WGPUStringView,
    userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    if userdata1.is_null() {
        return;
    }
    let state = unsafe { Rc::from_raw(userdata1.cast::<DeviceRequest>()) };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        state.status.set(status);
        state.device.set(device);
        *state.message.borrow_mut() = string_view_lossy(message);
        state.done.set(true);
    }));
}

fn wait_for(instance: wgpu::WGPUInstance, done: impl Fn() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + INIT_DEADLINE;
    while !done() {
        unsafe { wgpu::wgpuInstanceProcessEvents(instance) };
        if Instant::now() >= deadline {
            return Err(format!(
                "WebGPU request timed out after {} seconds",
                INIT_DEADLINE.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn request_adapter(
    instance: wgpu::WGPUInstance,
    surface: wgpu::WGPUSurface,
) -> Result<wgpu::WGPUAdapter, String> {
    let state = Rc::new(AdapterRequest {
        done: Cell::new(false),
        status: Cell::new(wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Error),
        adapter: Cell::new(ptr::null_mut()),
        message: RefCell::new(String::new()),
    });
    let options = wgpu::WGPURequestAdapterOptions {
        nextInChain: ptr::null_mut(),
        featureLevel: wgpu::WGPUFeatureLevel_WGPUFeatureLevel_Undefined,
        powerPreference: wgpu::WGPUPowerPreference_WGPUPowerPreference_Undefined,
        forceFallbackAdapter: wgpu::WGPU_FALSE,
        backendType: wgpu::WGPUBackendType_WGPUBackendType_Undefined,
        compatibleSurface: surface,
    };
    let callback = wgpu::WGPURequestAdapterCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(adapter_callback),
        userdata1: Rc::into_raw(Rc::clone(&state)).cast_mut().cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe { wgpu::wgpuInstanceRequestAdapter(instance, &options, callback) };
    wait_for(instance, || state.done.get())?;
    let adapter = state.adapter.get();
    if state.status.get() != wgpu::WGPURequestAdapterStatus_WGPURequestAdapterStatus_Success
        || adapter.is_null()
    {
        return Err(format!("requestAdapter failed: {}", state.message.borrow()));
    }
    Ok(adapter)
}

fn request_device(
    instance: wgpu::WGPUInstance,
    adapter: wgpu::WGPUAdapter,
) -> Result<wgpu::WGPUDevice, String> {
    let state = Rc::new(DeviceRequest {
        done: Cell::new(false),
        status: Cell::new(wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Error),
        device: Cell::new(ptr::null_mut()),
        message: RefCell::new(String::new()),
    });
    let callback = wgpu::WGPURequestDeviceCallbackInfo {
        nextInChain: ptr::null_mut(),
        mode: wgpu::WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
        callback: Some(device_callback),
        userdata1: Rc::into_raw(Rc::clone(&state)).cast_mut().cast(),
        userdata2: ptr::null_mut(),
    };
    unsafe { wgpu::wgpuAdapterRequestDevice(adapter, ptr::null(), callback) };
    wait_for(instance, || state.done.get())?;
    let device = state.device.get();
    if state.status.get() != wgpu::WGPURequestDeviceStatus_WGPURequestDeviceStatus_Success
        || device.is_null()
    {
        return Err(format!("requestDevice failed: {}", state.message.borrow()));
    }
    Ok(device)
}

#[cfg(target_os = "macos")]
struct PlatformSurface {
    surface: wgpu::WGPUSurface,
    _metal_layer: raw_window_metal::Layer,
}

#[cfg(target_os = "windows")]
struct PlatformSurface {
    surface: wgpu::WGPUSurface,
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct PlatformSurface {
    surface: wgpu::WGPUSurface,
}

#[cfg(target_os = "macos")]
fn create_surface(
    instance: wgpu::WGPUInstance,
    window: &Window,
) -> Result<PlatformSurface, String> {
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("winit did not provide an AppKit window handle".to_owned());
    };
    let layer = unsafe { raw_window_metal::Layer::from_ns_view(handle.ns_view) };
    let mut source = wgpu::WGPUSurfaceSourceMetalLayer {
        chain: wgpu::WGPUChainedStruct {
            next: ptr::null_mut(),
            sType: wgpu::WGPUSType_WGPUSType_SurfaceSourceMetalLayer,
        },
        layer: layer.as_ptr().as_ptr(),
    };
    let descriptor = wgpu::WGPUSurfaceDescriptor {
        nextInChain: ptr::from_mut(&mut source.chain),
        label: wgpu::WGPUStringView {
            data: ptr::null(),
            length: usize::MAX,
        },
    };
    let surface = unsafe { wgpu::wgpuInstanceCreateSurface(instance, &descriptor) };
    if surface.is_null() {
        Err("wgpuInstanceCreateSurface returned null".to_owned())
    } else {
        Ok(PlatformSurface {
            surface,
            _metal_layer: layer,
        })
    }
}

#[cfg(target_os = "windows")]
fn create_surface(
    instance: wgpu::WGPUInstance,
    window: &Window,
) -> Result<PlatformSurface, String> {
    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("winit did not provide a Win32 window handle".to_owned());
    };
    let mut source = wgpu::WGPUSurfaceSourceWindowsHWND {
        chain: wgpu::WGPUChainedStruct {
            next: ptr::null_mut(),
            sType: wgpu::WGPUSType_WGPUSType_SurfaceSourceWindowsHWND,
        },
        hinstance: handle
            .hinstance
            .map_or(ptr::null_mut(), |hinstance| hinstance.get() as *mut c_void),
        hwnd: handle.hwnd.get() as *mut c_void,
    };
    let descriptor = wgpu::WGPUSurfaceDescriptor {
        nextInChain: ptr::from_mut(&mut source.chain),
        label: wgpu::WGPUStringView {
            data: ptr::null(),
            length: usize::MAX,
        },
    };
    let surface = unsafe { wgpu::wgpuInstanceCreateSurface(instance, &descriptor) };
    if surface.is_null() {
        Err("wgpuInstanceCreateSurface returned null".to_owned())
    } else {
        Ok(PlatformSurface { surface })
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn create_surface(
    _instance: wgpu::WGPUInstance,
    window: &Window,
) -> Result<PlatformSurface, String> {
    let _ = window.window_handle().map_err(|error| error.to_string())?;
    Err("bounce surface creation is currently implemented for macOS and Windows only".to_owned())
}

fn idl_texture_format(format: wgpu::WGPUTextureFormat) -> Option<&'static str> {
    match format {
        wgpu::WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm => Some("bgra8unorm"),
        wgpu::WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb => Some("bgra8unorm-srgb"),
        wgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm => Some("rgba8unorm"),
        wgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8UnormSrgb => Some("rgba8unorm-srgb"),
        wgpu::WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm => Some("rgb10a2unorm"),
        wgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Float => Some("rgba16float"),
        _ => None,
    }
}

struct SurfaceSelection {
    format: wgpu::WGPUTextureFormat,
    format_name: &'static str,
    alpha_mode: wgpu::WGPUCompositeAlphaMode,
}

fn surface_selection(
    surface: wgpu::WGPUSurface,
    adapter: wgpu::WGPUAdapter,
) -> Result<SurfaceSelection, String> {
    let mut capabilities = wgpu::WGPUSurfaceCapabilities {
        nextInChain: ptr::null_mut(),
        usages: wgpu::WGPUTextureUsage_None,
        formatCount: 0,
        formats: ptr::null(),
        presentModeCount: 0,
        presentModes: ptr::null(),
        alphaModeCount: 0,
        alphaModes: ptr::null(),
    };
    let status = unsafe { wgpu::wgpuSurfaceGetCapabilities(surface, adapter, &mut capabilities) };
    if status != wgpu::WGPUStatus_WGPUStatus_Success {
        return Err("wgpuSurfaceGetCapabilities failed".to_owned());
    }
    let formats = if capabilities.formatCount == 0 || capabilities.formats.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(capabilities.formats, capabilities.formatCount) }
    };
    let preferred = formats
        .first()
        .copied()
        .and_then(|format| idl_texture_format(format).map(|name| (format, name)));
    let alpha_modes = if capabilities.alphaModeCount == 0 || capabilities.alphaModes.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(capabilities.alphaModes, capabilities.alphaModeCount) }
    };
    let alpha_mode = alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::WGPUCompositeAlphaMode_WGPUCompositeAlphaMode_Auto);
    unsafe { wgpu::wgpuSurfaceCapabilitiesFreeMembers(capabilities) };
    let (format, format_name) = preferred
        .ok_or_else(|| "surface exposes no texture format mapped by this example".to_owned())?;
    Ok(SurfaceSelection {
        format,
        format_name,
        alpha_mode,
    })
}

fn eval_discard(runtime: &Runtime, source: &str, name: &str) -> Result<(), String> {
    let value = runtime
        .eval(source, name)
        .map_err(|error| format!("{error:?}"))?;
    runtime
        .set_global_value("__bounce_eval_result", value)
        .map_err(|error| format!("{error:?}"))?;
    runtime
        .clear_global("__bounce_eval_result")
        .map_err(|error| format!("{error:?}"))
}

struct Renderer {
    runtime: Runtime,
    _platform_surface: PlatformSurface,
    instance: wgpu::WGPUInstance,
    surface: wgpu::WGPUSurface,
    adapter: wgpu::WGPUAdapter,
    device: wgpu::WGPUDevice,
    queue: wgpu::WGPUQueue,
    bundle: wgpu::WGPURenderBundle,
    format: wgpu::WGPUTextureFormat,
    alpha_mode: wgpu::WGPUCompositeAlphaMode,
    size: PhysicalSize<u32>,
    verify: bool,
    last_frame: Instant,
    captured_output: Rc<RefCell<Vec<String>>>,
    swap_signals: Rc<Cell<u64>>,
    handled_swaps: u64,
}

struct InitialNativeHandles {
    surface: wgpu::WGPUSurface,
    adapter: wgpu::WGPUAdapter,
    device: wgpu::WGPUDevice,
    queue: wgpu::WGPUQueue,
}

impl Drop for InitialNativeHandles {
    fn drop(&mut self) {
        unsafe {
            if !self.queue.is_null() {
                wgpu::wgpuQueueRelease(self.queue);
            }
            if !self.device.is_null() {
                wgpu::wgpuDeviceRelease(self.device);
            }
            if !self.adapter.is_null() {
                wgpu::wgpuAdapterRelease(self.adapter);
            }
            if !self.surface.is_null() {
                wgpu::wgpuSurfaceRelease(self.surface);
            }
        }
    }
}

impl Renderer {
    fn new(window: &Window, verify: bool) -> Result<Self, String> {
        let instance = create_instance()?;
        let runtime = match Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                unsafe { wgpu::wgpuInstanceRelease(instance) };
                return Err(format!("{error:?}"));
            }
        };
        let captured_output = Rc::new(RefCell::new(Vec::new()));
        let print_output = Rc::clone(&captured_output);
        if let Err(error) = runtime.register_host_function("print", move |args| {
            let line = host_print_line(args, verify);
            if verify {
                print_output.borrow_mut().push(line);
            } else {
                println!("{line}");
            }
            Ok(())
        }) {
            unsafe { wgpu::wgpuInstanceRelease(instance) };
            return Err(format!("{error:?}"));
        }
        let swap_signals = Rc::new(Cell::new(0_u64));
        let host_swap_signals = Rc::clone(&swap_signals);
        if let Err(error) = runtime.register_host_function("signalBundleSwap", move |_| {
            let next = host_swap_signals
                .get()
                .checked_add(1)
                .ok_or_else(|| "bundle swap signal counter overflowed".to_owned())?;
            host_swap_signals.set(next);
            Ok(())
        }) {
            unsafe { wgpu::wgpuInstanceRelease(instance) };
            return Err(format!("{error:?}"));
        }
        let result = Self::new_with_instance(
            instance,
            window,
            runtime,
            verify,
            captured_output,
            swap_signals,
        );
        if result.is_err() {
            unsafe { wgpu::wgpuInstanceRelease(instance) };
        }
        result
    }

    fn new_with_instance(
        instance: wgpu::WGPUInstance,
        window: &Window,
        runtime: Runtime,
        verify: bool,
        captured_output: Rc<RefCell<Vec<String>>>,
        swap_signals: Rc<Cell<u64>>,
    ) -> Result<Self, String> {
        let platform_surface = create_surface(instance, window)?;
        let surface = platform_surface.surface;
        let mut handles = InitialNativeHandles {
            surface,
            adapter: ptr::null_mut(),
            device: ptr::null_mut(),
            queue: ptr::null_mut(),
        };
        let adapter = request_adapter(instance, surface)?;
        handles.adapter = adapter;
        let device = request_device(instance, adapter)?;
        handles.device = device;
        let queue = unsafe { wgpu::wgpuDeviceGetQueue(device) };
        if queue.is_null() {
            return Err("wgpuDeviceGetQueue returned null".to_owned());
        }
        handles.queue = queue;
        let selection = surface_selection(surface, adapter)?;
        eval_discard(
            &runtime,
            "globalThis.console = { log: (...args) => print(...args) };",
            "console-shim.js",
        )?;
        let js_format = runtime
            .eval(&format!("{:?}", selection.format_name), "surface-format.js")
            .map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("surfaceFormat", js_format)
            .map_err(|error| format!("{error:?}"))?;
        let js_device =
            unsafe { runtime.wrap_device(device) }.map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("device", js_device)
            .map_err(|error| format!("{error:?}"))?;
        let js_verify = runtime
            .eval(if verify { "true" } else { "false" }, "verify-mode.js")
            .map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("verify", js_verify)
            .map_err(|error| format!("{error:?}"))?;
        let js_frames = runtime
            .eval(&format!("{VERIFY_FRAMES}"), "verify-frames.js")
            .map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("VERIFY_FRAMES", js_frames)
            .map_err(|error| format!("{error:?}"))?;
        eval_discard(&runtime, BOUNCE_SOURCE, "bounce.js")?;

        let deadline = Instant::now() + INIT_DEADLINE;
        loop {
            unsafe { runtime.tick(instance) }.map_err(|error| format!("{error:?}"))?;
            let ready = runtime
                .eval(
                    "globalThis.ready === true ? globalThis.bounceBundle : undefined",
                    "bounce-ready.js",
                )
                .map_err(|error| format!("{error:?}"))?;
            if let Some(bundle) = runtime.native_render_bundle(ready) {
                runtime
                    .set_global_value("__hostBorrowedBounceBundle", ready)
                    .map_err(|error| format!("{error:?}"))?;
                let size = window.inner_size();
                let mut renderer = Self {
                    runtime,
                    _platform_surface: platform_surface,
                    instance,
                    surface,
                    adapter,
                    device,
                    queue,
                    bundle,
                    format: selection.format,
                    alpha_mode: selection.alpha_mode,
                    size,
                    verify,
                    last_frame: Instant::now(),
                    captured_output,
                    swap_signals,
                    handled_swaps: 0,
                };
                renderer.configure();
                std::mem::forget(handles);
                return Ok(renderer);
            }
            runtime
                .set_global_value("__bounce_ready_probe", ready)
                .map_err(|error| format!("{error:?}"))?;
            runtime
                .clear_global("__bounce_ready_probe")
                .map_err(|error| format!("{error:?}"))?;
            if Instant::now() >= deadline {
                let mut message = format!(
                    "bounce.js did not become ready within {} seconds",
                    INIT_DEADLINE.as_secs()
                );
                let output = captured_output.borrow();
                if !output.is_empty() {
                    message.push_str("; captured script output:\n");
                    message.push_str(&output.join("\n"));
                }
                return Err(message);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn configure(&mut self) {
        if self.size.width == 0 || self.size.height == 0 {
            return;
        }
        let configuration = wgpu::WGPUSurfaceConfiguration {
            nextInChain: ptr::null_mut(),
            device: self.device,
            format: self.format,
            usage: wgpu::WGPUTextureUsage_RenderAttachment,
            width: self.size.width,
            height: self.size.height,
            viewFormatCount: 0,
            viewFormats: ptr::null(),
            alphaMode: self.alpha_mode,
            presentMode: wgpu::WGPUPresentMode_WGPUPresentMode_Fifo,
        };
        unsafe { wgpu::wgpuSurfaceConfigure(self.surface, &configuration) };
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
        self.configure();
    }

    fn render(&mut self) -> Result<bool, String> {
        if self.size.width == 0 || self.size.height == 0 {
            return Ok(false);
        }
        let now = Instant::now();
        let dt = if self.verify {
            1.0 / 60.0
        } else {
            now.duration_since(self.last_frame).as_secs_f64()
        };
        self.last_frame = now;
        unsafe {
            self.runtime
                .frame(self.instance, "update", &[HostValue::Number(dt)])
        }
        .map_err(|error| format!("update frame failed: {error:?}"))?;

        let signalled_swaps = self.swap_signals.get();
        if signalled_swaps < self.handled_swaps {
            return Err("bundle swap signal counter moved backwards".to_owned());
        }
        let pending_swaps = signalled_swaps - self.handled_swaps;
        if pending_swaps > 1 {
            return Err(format!(
                "multiple bundle swaps are pending: observed {pending_swaps}"
            ));
        }
        if pending_swaps == 1 {
            let value = self
                .runtime
                .eval("globalThis.bounceBundle", "bounce-swap.js")
                .map_err(|error| format!("{error:?}"))?;
            let new_bundle = self.runtime.native_render_bundle(value).ok_or_else(|| {
                "bundle swap signalled but globalThis.bounceBundle is not a render bundle"
                    .to_owned()
            })?;
            if new_bundle == self.bundle {
                return Err("bundle swap signalled but the native handle did not change".to_owned());
            }
            self.runtime
                .set_global_value("__hostBorrowedBounceBundle", value)
                .map_err(|error| format!("{error:?}"))?;
            self.bundle = new_bundle;
            self.handled_swaps += 1;
        }

        let mut current = wgpu::WGPUSurfaceTexture {
            nextInChain: ptr::null_mut(),
            texture: ptr::null_mut(),
            status: 0,
        };
        unsafe { wgpu::wgpuSurfaceGetCurrentTexture(self.surface, &mut current) };
        match current.status {
            wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_Outdated
            | wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_Lost => {
                self.configure();
                if self.verify {
                    return Err("a verification frame did not acquire a surface texture".to_owned());
                }
                return Ok(false);
            }
            wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_Timeout => {
                if self.verify {
                    return Err("a verification frame timed out before presentation".to_owned());
                }
                return Ok(false);
            }
            wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_SuccessOptimal
            | wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_SuccessSuboptimal => {}
            status => return Err(format!("surface texture acquisition failed with status {status}")),
        }
        if current.texture.is_null() {
            return Err("surface returned a null texture".to_owned());
        }

        let view = unsafe { wgpu::wgpuTextureCreateView(current.texture, ptr::null()) };
        if view.is_null() {
            unsafe { wgpu::wgpuTextureRelease(current.texture) };
            return Err("wgpuTextureCreateView returned null".to_owned());
        }
        let encoder = unsafe { wgpu::wgpuDeviceCreateCommandEncoder(self.device, ptr::null()) };
        if encoder.is_null() {
            unsafe {
                wgpu::wgpuTextureViewRelease(view);
                wgpu::wgpuTextureRelease(current.texture);
            }
            return Err("wgpuDeviceCreateCommandEncoder returned null".to_owned());
        }
        let color_attachment = wgpu::WGPURenderPassColorAttachment {
            nextInChain: ptr::null_mut(),
            view,
            depthSlice: wgpu::WGPU_DEPTH_SLICE_UNDEFINED,
            resolveTarget: ptr::null_mut(),
            loadOp: wgpu::WGPULoadOp_WGPULoadOp_Clear,
            storeOp: wgpu::WGPUStoreOp_WGPUStoreOp_Store,
            clearValue: wgpu::WGPUColor {
                r: 0.015,
                g: 0.025,
                b: 0.08,
                a: 1.0,
            },
        };
        let pass_descriptor = wgpu::WGPURenderPassDescriptor {
            nextInChain: ptr::null_mut(),
            label: wgpu::WGPUStringView {
                data: ptr::null(),
                length: usize::MAX,
            },
            colorAttachmentCount: 1,
            colorAttachments: ptr::from_ref(&color_attachment),
            depthStencilAttachment: ptr::null(),
            occlusionQuerySet: ptr::null_mut(),
            timestampWrites: ptr::null(),
        };
        let pass = unsafe { wgpu::wgpuCommandEncoderBeginRenderPass(encoder, &pass_descriptor) };
        if pass.is_null() {
            unsafe {
                wgpu::wgpuCommandEncoderRelease(encoder);
                wgpu::wgpuTextureViewRelease(view);
                wgpu::wgpuTextureRelease(current.texture);
            }
            return Err("wgpuCommandEncoderBeginRenderPass returned null".to_owned());
        }
        unsafe {
            wgpu::wgpuRenderPassEncoderExecuteBundles(pass, 1, ptr::from_ref(&self.bundle));
            wgpu::wgpuRenderPassEncoderEnd(pass);
            wgpu::wgpuRenderPassEncoderRelease(pass);
        }
        let command = unsafe { wgpu::wgpuCommandEncoderFinish(encoder, ptr::null()) };
        unsafe { wgpu::wgpuCommandEncoderRelease(encoder) };
        if command.is_null() {
            unsafe {
                wgpu::wgpuTextureViewRelease(view);
                wgpu::wgpuTextureRelease(current.texture);
            }
            return Err("wgpuCommandEncoderFinish returned null".to_owned());
        }
        unsafe {
            wgpu::wgpuQueueSubmit(self.queue, 1, ptr::from_ref(&command));
            wgpu::wgpuCommandBufferRelease(command);
        }
        let present_status = unsafe { wgpu::wgpuSurfacePresent(self.surface) };
        unsafe {
            wgpu::wgpuTextureViewRelease(view);
            wgpu::wgpuTextureRelease(current.texture);
        }
        if present_status != wgpu::WGPUStatus_WGPUStatus_Success {
            return Err("wgpuSurfacePresent failed".to_owned());
        }
        Ok(true)
    }

    fn verify_output(&self) -> Result<(), String> {
        let observed_swaps = self.swap_signals.get();
        if observed_swaps != 1 {
            return Err(format!(
                "expected exactly one bundle swap signal, observed {observed_swaps}"
            ));
        }
        let lines = self.captured_output.borrow();
        let actual = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        if actual != EXPECTED_OUTPUT {
            return Err(format!(
                "state output did not match expected.txt\nexpected:\n{EXPECTED_OUTPUT}actual:\n{actual}"
            ));
        }
        println!(
            "bounce verification passed: {VERIFY_FRAMES} frames presented; state matched expected.txt; one bundle swap observed"
        );
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // The runtime owns the bundle wrapper/global and drains its release
        // queue at teardown. `bundle` is only a borrowed handle.
        let _ = self.runtime.drain_releases();
        unsafe {
            wgpu::wgpuQueueRelease(self.queue);
            wgpu::wgpuDeviceRelease(self.device);
            wgpu::wgpuAdapterRelease(self.adapter);
            wgpu::wgpuSurfaceRelease(self.surface);
            wgpu::wgpuInstanceRelease(self.instance);
        }
    }
}

struct App {
    verify: bool,
    frames: u64,
    window: Option<Window>,
    renderer: Option<Renderer>,
    failure: Option<String>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("bounce example failed: {error}");
        self.failure = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("webgpu-native-js bounce")
            .with_inner_size(PhysicalSize::new(800, 600));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(error) => {
                self.fail(event_loop, error.to_string());
                return;
            }
        };
        match Renderer::new(&window, self.verify) {
            Ok(renderer) => {
                window.request_redraw();
                self.renderer = Some(renderer);
                self.window = Some(window);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(Window::id) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.verify && self.frames != VERIFY_FRAMES {
                    self.fail(
                        event_loop,
                        format!(
                            "verification stopped after {} of {VERIFY_FRAMES} successful presents",
                            self.frames
                        ),
                    );
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                let render_result = match &mut self.renderer {
                    Some(renderer) => renderer.render(),
                    None => return,
                };
                match render_result {
                    Ok(true) => {
                        self.frames += 1;
                        if self.verify && self.frames == VERIFY_FRAMES {
                            let verification = match &self.renderer {
                                Some(renderer) => renderer.verify_output(),
                                None => {
                                    Err("renderer disappeared after a successful frame".to_owned())
                                }
                            };
                            if let Err(error) = verification {
                                self.fail(event_loop, error);
                                return;
                            }
                            event_loop.exit();
                            return;
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.fail(event_loop, error);
                        return;
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn parse_verify() -> Result<bool, String> {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        return Ok(false);
    };
    if flag != "--verify" {
        return Err(format!("unknown argument: {flag}"));
    }
    if args.next().is_some() {
        return Err("unexpected arguments after --verify".to_owned());
    }
    Ok(true)
}

fn main() -> ExitCode {
    let verify = match parse_verify() {
        Ok(verify) => verify,
        Err(error) => {
            eprintln!("bounce example failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("bounce example failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut app = App {
        verify,
        frames: 0,
        window: None,
        renderer: None,
        failure: None,
    };
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("bounce example failed: {error}");
        return ExitCode::FAILURE;
    }
    if app.failure.is_some() {
        return ExitCode::FAILURE;
    }
    if app.verify && app.frames != VERIFY_FRAMES {
        eprintln!(
            "bounce example failed: verification ended after {} of {VERIFY_FRAMES} successful presents",
            app.frames
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
