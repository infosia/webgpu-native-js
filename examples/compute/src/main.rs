use std::cell::Cell;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use boa_adapter::{HostValue, Runtime};
use webgpu_native_js_ffi::native as wgpu;

const COMPUTE_SOURCE: &str = include_str!("../compute.js");
const DEADLINE: Duration = Duration::from_secs(10);

fn host_value_text(value: &HostValue) -> String {
    match value {
        HostValue::String(value) => value.clone(),
        HostValue::Number(value) => value.to_string(),
        HostValue::Bool(value) => value.to_string(),
        HostValue::Null => "null".to_owned(),
        HostValue::Undefined => "undefined".to_owned(),
    }
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
    let instance = unsafe { wgpu::wgpuCreateInstance(std::ptr::null()) };
    if instance.is_null() {
        Err("wgpuCreateInstance returned null".to_owned())
    } else {
        Ok(instance)
    }
}

fn eval_discard(runtime: &Runtime, source: &str, name: &str) -> boa_adapter::Result<()> {
    let value = runtime.eval(source, name)?;
    runtime.set_global_value("__example_eval_result", value)?;
    runtime.clear_global("__example_eval_result")
}

fn run(instance: wgpu::WGPUInstance) -> boa_adapter::Result<bool> {
    let runtime = Runtime::new()?;
    let done = Rc::new(Cell::new(false));
    let ok = Rc::new(Cell::new(false));
    let print_done = Rc::clone(&done);
    let print_ok = Rc::clone(&ok);
    runtime.register_host_function("print", move |args| {
        if let [HostValue::String(marker), HostValue::Bool(result)] = args {
            if marker == "__example_status__" {
                print_ok.set(*result);
                print_done.set(true);
                return Ok(());
            }
        }
        println!(
            "{}",
            args.iter()
                .map(host_value_text)
                .collect::<Vec<_>>()
                .join(" ")
        );
        Ok(())
    })?;

    eval_discard(
        &runtime,
        "globalThis.console = { log: (...args) => print(...args) };",
        "console-shim.js",
    )?;
    let gpu = unsafe { runtime.wrap_gpu(instance) }?;
    runtime.set_global_value("gpu", gpu)?;
    eval_discard(&runtime, COMPUTE_SOURCE, "compute.js")?;

    let deadline = Instant::now() + DEADLINE;
    while !done.get() {
        unsafe { runtime.tick(instance) }?;
        eval_discard(
            &runtime,
            "if (globalThis.done === true) print('__example_status__', globalThis.ok === true);",
            "compute-status.js",
        )?;
        if Instant::now() >= deadline {
            eprintln!(
                "compute example timed out after {} seconds",
                DEADLINE.as_secs()
            );
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(ok.get())
}

fn main() -> ExitCode {
    let instance = match create_instance() {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let result = run(instance);
    unsafe { wgpu::wgpuInstanceRelease(instance) };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("compute example failed: {error:?}");
            ExitCode::FAILURE
        }
    }
}
