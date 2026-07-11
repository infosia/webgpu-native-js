use std::cell::RefCell;
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant};

use cts_runner::{format_summary, load_expectations, load_suite, summarize, Status, TestResult};
use quickjs_adapter::{HostValue, ModuleEvaluationStatus, Runtime};
use webgpu_native_js_ffi::native as wgpu;

const DEFAULT_TIMEOUT_SECS: u64 = 300;
const GLUE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/glue.mjs");

// Phase A-2 replaces these placeholders with aliases for the discovered CTS
// build layout. Values are paths relative to --cts-path/CTS_PATH.
const CTS_MODULE_ALIASES: &[(&str, &str)] = &[("cts", "")];

#[derive(Debug)]
struct Config {
    cts_path: PathBuf,
    queries: Vec<String>,
    expectations: Option<PathBuf>,
    list: bool,
    timeout: Duration,
}

fn usage() -> &'static str {
    "usage: cts-runner [--cts-path <dir>] (--query <query> | --suite <file>)... \
     [--expectations <file>] [--list] [--timeout-secs <seconds>]"
}

fn parse_args() -> Result<Config, String> {
    let mut args = env::args().skip(1);
    let mut cts_path = None;
    let mut queries = Vec::new();
    let mut expectations = None;
    let mut list = false;
    let mut timeout_secs = DEFAULT_TIMEOUT_SECS;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cts-path" => cts_path = Some(PathBuf::from(next_value(&mut args, &arg)?)),
            "--query" => queries.push(next_value(&mut args, &arg)?),
            "--suite" => {
                let path = PathBuf::from(next_value(&mut args, &arg)?);
                queries.extend(load_suite(&path)?);
            }
            "--expectations" => {
                expectations = Some(PathBuf::from(next_value(&mut args, &arg)?));
            }
            "--list" => list = true,
            "--timeout-secs" => {
                let value = next_value(&mut args, &arg)?;
                timeout_secs = value
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --timeout-secs value {value:?}"))?;
                if timeout_secs == 0 {
                    return Err("--timeout-secs must be greater than zero".to_owned());
                }
            }
            "--help" | "-h" => return Err(usage().to_owned()),
            _ => return Err(format!("unknown argument {arg:?}\n{}", usage())),
        }
    }

    let cts_path = cts_path
        .or_else(|| env::var_os("CTS_PATH").map(PathBuf::from))
        .ok_or_else(|| format!("--cts-path or CTS_PATH is required\n{}", usage()))?;
    if queries.is_empty() {
        return Err(format!(
            "at least one --query or --suite is required\n{}",
            usage()
        ));
    }
    Ok(Config {
        cts_path,
        queries,
        expectations,
        list,
        timeout: Duration::from_secs(timeout_secs),
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

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
                    "unknown YAWGPU_BACKEND value {other:?}; accepted values are noop, metal, vulkan, and gles"
                ));
            }
        };
        if backend == YAWGPU_BACKEND_NOOP {
            let instance = unsafe { wgpu::wgpuCreateInstance(ptr::null()) };
            return non_null_instance(instance);
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
        non_null_instance(unsafe { wgpu::wgpuCreateInstance(&descriptor) })
    }

    fn non_null_instance(instance: wgpu::WGPUInstance) -> Result<wgpu::WGPUInstance, String> {
        if instance.is_null() {
            Err("wgpuCreateInstance returned null".to_owned())
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

fn js_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => encoded.push_str("\\\\"),
            '"' => encoded.push_str("\\\""),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            c if c.is_control() => encoded.push_str(&format!("\\u{:04x}", c as u32)),
            c => encoded.push(c),
        }
    }
    encoded.push('"');
    encoded
}

fn install_config(runtime: &Runtime, config: &Config) -> quickjs_adapter::Result<()> {
    let queries = config
        .queries
        .iter()
        .map(|query| js_string(query))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "globalThis.__ctsRunnerConfig = {{ queries: [{queries}], list: {} }};",
        config.list
    );
    let value = runtime.eval(&source, "cts-runner-config.js")?;
    runtime.set_global_value("__cts_runner_config_eval", value)?;
    runtime.clear_global("__cts_runner_config_eval")
}

fn run(config: &Config, instance: wgpu::WGPUInstance) -> Result<Vec<TestResult>, String> {
    let runtime = Runtime::new().map_err(|error| format!("runtime creation failed: {error:?}"))?;
    let results = Rc::new(RefCell::new(Vec::new()));

    runtime
        .register_host_function("print", |args| {
            println!(
                "{}",
                args.iter()
                    .map(host_value_text)
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            Ok(())
        })
        .map_err(|error| format!("could not register print: {error:?}"))?;
    runtime
        .register_host_function("__perf_now", |_| Ok(()))
        .map_err(|error| format!("could not register __perf_now: {error:?}"))?;
    let report_results = Rc::clone(&results);
    runtime
        .register_host_function("__report", move |args| {
            let [HostValue::String(query), HostValue::String(status), HostValue::String(message)] =
                args
            else {
                return Err("__report expects (query, status, message) strings".to_owned());
            };
            report_results.borrow_mut().push(TestResult {
                query: query.clone(),
                status: Status::parse(status)?,
                message: message.clone(),
            });
            Ok(())
        })
        .map_err(|error| format!("could not register __report: {error:?}"))?;
    runtime
        .register_host_function("__log_shim", |args| {
            let [HostValue::String(name)] = args else {
                return Err("__log_shim expects one string".to_owned());
            };
            eprintln!("shim: {name}");
            Ok(())
        })
        .map_err(|error| format!("could not register __log_shim: {error:?}"))?;

    for (alias, relative_path) in CTS_MODULE_ALIASES {
        runtime
            .set_module_alias(alias, &config.cts_path.join(relative_path))
            .map_err(|error| format!("could not set module alias {alias:?}: {error:?}"))?;
    }
    let gpu = unsafe { runtime.wrap_gpu(instance) }
        .map_err(|error| format!("could not wrap GPU instance: {error:?}"))?;
    runtime
        .set_global_value("gpu", gpu)
        .map_err(|error| format!("could not install gpu global: {error:?}"))?;
    install_config(&runtime, config)
        .map_err(|error| format!("could not install runner configuration: {error:?}"))?;

    let evaluation = runtime
        .eval_module(Path::new(GLUE_PATH))
        .map_err(|error| format!("glue module failed: {error:?}"))?;
    let deadline = Instant::now() + config.timeout;
    loop {
        match evaluation
            .status()
            .map_err(|error| format!("glue module failed: {error:?}"))?
        {
            ModuleEvaluationStatus::Fulfilled => break,
            ModuleEvaluationStatus::Pending => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "CTS run timed out after {} seconds",
                        config.timeout.as_secs()
                    ));
                }
                unsafe { runtime.tick(instance) }
                    .map_err(|error| format!("runtime tick failed: {error:?}"))?;
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    drop(evaluation);
    let collected = results.borrow().clone();
    Ok(collected)
}

fn main() -> ExitCode {
    let config = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let expectations = match config.expectations.as_deref() {
        Some(path) => match load_expectations(path) {
            Ok(expectations) => expectations,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        },
        None => Vec::new(),
    };
    let instance = match create_instance() {
        Ok(instance) => instance,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let run_result = run(&config, instance);
    unsafe { wgpu::wgpuInstanceRelease(instance) };

    let results = match run_result {
        Ok(results) => results,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if config.list {
        return ExitCode::SUCCESS;
    }
    if results.is_empty() {
        eprintln!("CTS glue completed without reporting any selected cases");
        return ExitCode::FAILURE;
    }
    let (summary, failure_lines) = summarize(&results, &expectations);
    println!("{}", format_summary(summary));
    if !failure_lines.is_empty() {
        eprint!("{failure_lines}");
    }
    if summary.exit_success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
