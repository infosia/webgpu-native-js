# Compute example

This headless example creates the native WebGPU instance in Rust, exposes the
binding's `GPU` wrapper to Boa, and requests the adapter and device from
JavaScript. The script runs a WGSL compute kernel that doubles eight `u32`
values, copies them to a readable buffer, maps it asynchronously, and reports
the result through a host-registered `print` function.

## The engine thread

The example spawns a thread with an explicit `stack_size` and runs the engine on
it. This is a host obligation, not example scaffolding: Boa's interpreter
recurses over the JS call graph, and in a debug build its frames exhaust the
platform default stack — 1 MiB for the MSVC main thread, 512 KiB for iOS
secondary threads, 1 MiB for Android native threads. `compute.js` overflows a
default-sized stack in debug builds. A host must therefore run the engine on a
thread whose stack size it chose; the binding cannot do this for it, because the
host owns its threads.

The instance is created, used, and released inside that same thread, so no
WebGPU handle crosses a thread boundary.

Set the backend library directory and run the workspace package:

```sh
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib cargo run -p example-compute
```

On Windows, MSVC has no rpath, so the backend DLL's directory must also be on
`PATH` at runtime:

```powershell
$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR = 'C:\path\to\backend\lib'
$env:PATH = "$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR;$env:PATH"
cargo run -p example-compute
```

The default feature selects yawgpu. Use `--no-default-features` with
`--features backend-wgpu-native` or `--features backend-dawn` for another
supported backend. If the environment variable is absent and pkg-config cannot
locate the selected backend, the existing FFI build error explains which
dynamic library and variable are required.

The engine is a link-time choice: Boa by default, and `--features engine-jsc`
selects the system JavaScriptCore instead (Apple platforms only; the adapter is
an empty crate elsewhere, so the feature does not compile off them). The engine
thread and its explicit stack size stay in place under both engines — the
obligation is the host's regardless of engine. Both engines print the same
result line.

yawgpu never auto-selects a real backend: with `YAWGPU_BACKEND` absent (or set
to `noop`) the instance is Noop. `YAWGPU_BACKEND=vulkan` (Windows/Linux,
requires a yawgpu build with the `vulkan` feature) or `YAWGPU_BACKEND=metal`
(macOS) selects real execution; any other value fails with an early error
naming the accepted ones.

A real compute backend prints:

```text
result: 2, 4, 6, 8, 10, 12, 14, 16
```

yawgpu's Noop backend validates the command stream but does not execute the
compute pass, so its mapped result is the un-doubled input:

```text
result: 1, 2, 3, 4, 5, 6, 7, 8
```
