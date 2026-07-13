# Windowed bouncing bodies

This example renders eight bodies with one draw call. JavaScript runs one
synchronous `update(dt)` call per frame, rewrites one storage buffer, and leaves
the native host to execute the render bundle recorded once during
initialization. The bundle is never re-recorded. The one `queue.writeBuffer`
call is the only JavaScript-to-native WebGPU crossing per frame. Raising `N`
raises JavaScript simulation cost and the size of that buffer copy, but it does
not raise binding cost because each frame crosses the binding once.
`examples/triangle` shows the static case with zero per-frame JavaScript; this
example shows the dynamic case with one per-frame JavaScript call and one draw
call.

Build the selected real backend first and point the loader at its library
directory:

```sh
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib YAWGPU_BACKEND=metal cargo run -p example-bounce
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib YAWGPU_BACKEND=metal cargo run -p example-bounce -- --verify
```

On Windows, MSVC has no rpath, so the backend DLL's directory must also be on
`PATH` at runtime:

```powershell
$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR = 'C:\path\to\backend\lib'
$env:PATH = "$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR;$env:PATH"
$env:YAWGPU_BACKEND = 'vulkan'
cargo run -p example-bounce -- --verify
```

With `--verify`, the host supplies `1.0 / 60.0` as `dt`, requires 60 successful
presents, captures the state printed by the sixtieth update, and compares it
with `expected.txt`. A mismatch or unsuccessful present exits non-zero. Without
`--verify`, the host supplies wall-clock `dt` and runs until the window closes.
No pixel readback is performed because `examples/triangle` verifies surface
pixels.

A callback error exits this example with the frame error. A shipping host can
instead rate-limit its log and keep rendering; that decision is host policy,
not binding behavior.

The default feature selects yawgpu. `--no-default-features --features
backend-wgpu-native` and `backend-dawn` select the experimental alternatives. A
real GPU-capable backend is required because yawgpu's Noop backend cannot
display a window. yawgpu selects Noop when `YAWGPU_BACKEND` is absent;
`YAWGPU_BACKEND=metal` selects Metal on macOS and `YAWGPU_BACKEND=vulkan`
selects Vulkan on Windows. Surface creation supports macOS and Windows.
