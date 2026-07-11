# Compute example

This headless example creates the native WebGPU instance in Rust, exposes the
binding's `GPU` wrapper to QuickJS, and requests the adapter and device from
JavaScript. The script runs a WGSL compute kernel that doubles eight `u32`
values, copies them to a readable buffer, maps it asynchronously, and reports
the result through a host-registered `print` function.

Set the backend library directory and run the workspace package:

```sh
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib cargo run -p example-compute
```

The default feature selects yawgpu. Use `--no-default-features` with
`--features backend-wgpu-native` or `--features backend-dawn` for another
supported backend. If the environment variable is absent and pkg-config cannot
locate the selected backend, the existing FFI build error explains which
dynamic library and variable are required.

A real compute backend prints:

```text
result: 2, 4, 6, 8, 10, 12, 14, 16
```

yawgpu's Noop backend validates the command stream but does not execute the
compute or copy commands, so its expected mapped result is eight zeros.
