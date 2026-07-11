use std::cell::Cell;
use std::process::ExitCode;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use quickjs_adapter::{HostValue, Runtime};
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

fn eval_discard(runtime: &Runtime, source: &str, name: &str) -> quickjs_adapter::Result<()> {
    let value = runtime.eval(source, name)?;
    runtime.set_global_value("__example_eval_result", value)?;
    runtime.clear_global("__example_eval_result")
}

fn run(instance: wgpu::WGPUInstance) -> quickjs_adapter::Result<bool> {
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
    let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
    if instance.is_null() {
        eprintln!("wgpuCreateInstance returned null");
        return ExitCode::FAILURE;
    }

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
