# Windowed triangle

This example creates a native window and surface, gives JavaScript the selected
surface format and a wrapped device, and lets `triangle.js` author the shaders,
pipeline, and reusable render bundle. After initialization, JavaScript has no
part in the frame loop: the host borrows the bundle while its JS global keeps
the wrapper alive, then acquires, clears, renders, submits, and presents every
frame through `webgpu.h`. The host deliberately does not call `eval` or `tick`
between frames; queued wrapper releases are drained during teardown.

Build the selected real backend first and point the loader at its library
directory:

```sh
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib cargo run -p example-triangle -- --verify
```

On Windows, MSVC has no rpath, so the backend DLL's directory must also be on
`PATH` at runtime:

```powershell
$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR = 'C:\path\to\backend\lib'
$env:PATH = "$env:WEBGPU_NATIVE_JS_BACKEND_LIB_DIR;$env:PATH"
cargo run -p example-triangle -- --verify
```

With `--verify`, a successful run exits after 60 presented frames, reads back
the final frame's center pixel, and prints `center pixel: R,G,B,A` with decimal
8-bit channel values. Without it, the window runs until closed. `--verify`
additionally requires the backend's surface capabilities to advertise
`CopySrc` (Dawn does; yawgpu currently advertises `RenderAttachment` only, so
verify mode fails early with a clear error there). The default feature selects
yawgpu; `--no-default-features --features backend-wgpu-native` and
`backend-dawn` select the experimental alternatives. A real GPU-capable
backend is required—yawgpu's Noop backend cannot display the triangle. On
Windows, real rendering via yawgpu requires a yawgpu build with its `vulkan`
feature.

yawgpu never auto-selects a real backend: with `YAWGPU_BACKEND` absent (or set
to `noop`) the instance is Noop, which opens the window but presents nothing.
`YAWGPU_BACKEND=vulkan` renders for real on Windows against a
`vulkan`-featured yawgpu build; `YAWGPU_BACKEND=metal` does the same on macOS.
Any other value fails with an early error naming the accepted ones.

Surface creation supports macOS (an AppKit `NSView` backed by a
`CAMetalLayer`) and Windows (`WGPUSurfaceSourceWindowsHWND` via the window's
HWND). Other targets currently return a clear unsupported-surface error after
creating the window.
