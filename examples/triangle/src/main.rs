use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use quickjs_adapter::{HostValue, Runtime};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use webgpu_native_js_ffi::native as wgpu;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const TRIANGLE_SOURCE: &str = include_str!("../triangle.js");
const INIT_DEADLINE: Duration = Duration::from_secs(10);

fn host_value_text(value: &HostValue) -> String {
    match value {
        HostValue::String(value) => value.clone(),
        HostValue::Number(value) => value.to_string(),
        HostValue::Bool(value) => value.to_string(),
        HostValue::Null => "null".to_owned(),
        HostValue::Undefined => "undefined".to_owned(),
    }
}

fn string_view_lossy(view: wgpu::WGPUStringView) -> String {
    if view.data.is_null() || view.length == usize::MAX {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.length) };
    String::from_utf8_lossy(bytes).into_owned()
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

#[cfg(not(target_os = "macos"))]
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

#[cfg(not(target_os = "macos"))]
fn create_surface(
    _instance: wgpu::WGPUInstance,
    window: &Window,
) -> Result<PlatformSurface, String> {
    let _ = window.window_handle().map_err(|error| error.to_string())?;
    Err("triangle surface creation is currently implemented for macOS only".to_owned())
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

fn preferred_format(
    surface: wgpu::WGPUSurface,
    adapter: wgpu::WGPUAdapter,
) -> Result<(wgpu::WGPUTextureFormat, &'static str), String> {
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
    unsafe { wgpu::wgpuSurfaceCapabilitiesFreeMembers(capabilities) };
    preferred.ok_or_else(|| "surface exposes no texture format mapped by this example".to_owned())
}

fn eval_discard(runtime: &Runtime, source: &str, name: &str) -> Result<(), String> {
    let value = runtime
        .eval(source, name)
        .map_err(|error| format!("{error:?}"))?;
    runtime
        .set_global_value("__triangle_eval_result", value)
        .map_err(|error| format!("{error:?}"))?;
    runtime
        .clear_global("__triangle_eval_result")
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
    size: PhysicalSize<u32>,
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
    fn new(window: &Window) -> Result<Self, String> {
        let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
        if instance.is_null() {
            return Err("wgpuCreateInstance returned null".to_owned());
        }
        let runtime = match Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => {
                unsafe { wgpu::wgpuInstanceRelease(instance) };
                return Err(format!("{error:?}"));
            }
        };
        if let Err(error) = runtime.register_host_function("print", |args| {
            println!(
                "{}",
                args.iter()
                    .map(host_value_text)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            Ok(())
        }) {
            unsafe { wgpu::wgpuInstanceRelease(instance) };
            return Err(format!("{error:?}"));
        }
        let result = Self::new_with_instance(instance, window, runtime);
        if result.is_err() {
            unsafe { wgpu::wgpuInstanceRelease(instance) };
        }
        result
    }

    fn new_with_instance(
        instance: wgpu::WGPUInstance,
        window: &Window,
        runtime: Runtime,
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
        let (format, format_name) = preferred_format(surface, adapter)?;
        eval_discard(
            &runtime,
            "globalThis.console = { log: (...args) => print(...args) };",
            "console-shim.js",
        )?;
        let js_format = runtime
            .eval(&format!("{format_name:?}"), "surface-format.js")
            .map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("surfaceFormat", js_format)
            .map_err(|error| format!("{error:?}"))?;
        let js_device =
            unsafe { runtime.wrap_device(device) }.map_err(|error| format!("{error:?}"))?;
        runtime
            .set_global_value("device", js_device)
            .map_err(|error| format!("{error:?}"))?;
        eval_discard(&runtime, TRIANGLE_SOURCE, "triangle.js")?;

        let deadline = Instant::now() + INIT_DEADLINE;
        loop {
            unsafe { runtime.tick(instance) }.map_err(|error| format!("{error:?}"))?;
            let ready = runtime
                .eval(
                    "globalThis.ready === true ? globalThis.triangleBundle : undefined",
                    "triangle-ready.js",
                )
                .map_err(|error| format!("{error:?}"))?;
            if let Some(bundle) = runtime.native_render_bundle(ready) {
                runtime
                    .set_global_value("__hostBorrowedTriangleBundle", ready)
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
                    format,
                    size,
                };
                renderer.configure();
                std::mem::forget(handles);
                return Ok(renderer);
            }
            runtime
                .set_global_value("__triangle_ready_probe", ready)
                .map_err(|error| format!("{error:?}"))?;
            runtime
                .clear_global("__triangle_ready_probe")
                .map_err(|error| format!("{error:?}"))?;
            if Instant::now() >= deadline {
                return Err(format!(
                    "triangle.js did not become ready within {} seconds",
                    INIT_DEADLINE.as_secs()
                ));
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
            alphaMode: wgpu::WGPUCompositeAlphaMode_WGPUCompositeAlphaMode_Auto,
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
                return Ok(false);
            }
            wgpu::WGPUSurfaceGetCurrentTextureStatus_WGPUSurfaceGetCurrentTextureStatus_Timeout => {
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
    frame_limit: Option<u64>,
    frames: u64,
    window: Option<Window>,
    renderer: Option<Renderer>,
    failure: Option<String>,
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("triangle example failed: {error}");
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
            .with_title("webgpu-native-js triangle")
            .with_inner_size(PhysicalSize::new(800, 600));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(error) => {
                self.fail(event_loop, error.to_string());
                return;
            }
        };
        match Renderer::new(&window) {
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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size);
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(renderer) = &mut self.renderer else {
                    return;
                };
                match renderer.render() {
                    Ok(true) => {
                        self.frames += 1;
                        if self.frame_limit == Some(self.frames) {
                            println!("rendered {} frames", self.frames);
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

fn parse_frame_limit() -> Result<Option<u64>, String> {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    if flag != "--frames" {
        return Err(format!("unknown argument: {flag}"));
    }
    let value = args
        .next()
        .ok_or_else(|| "--frames requires a positive integer".to_owned())?;
    if args.next().is_some() {
        return Err("unexpected arguments after --frames N".to_owned());
    }
    let frames = value
        .parse::<u64>()
        .map_err(|_| "--frames requires a positive integer".to_owned())?;
    if frames == 0 {
        return Err("--frames requires a positive integer".to_owned());
    }
    Ok(Some(frames))
}

fn main() -> ExitCode {
    let frame_limit = match parse_frame_limit() {
        Ok(limit) => limit,
        Err(error) => {
            eprintln!("triangle example failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!("triangle example failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut app = App {
        frame_limit,
        frames: 0,
        window: None,
        renderer: None,
        failure: None,
    };
    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!("triangle example failed: {error}");
        return ExitCode::FAILURE;
    }
    if app.failure.is_some() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
