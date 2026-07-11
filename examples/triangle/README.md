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
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR=/path/to/backend/lib cargo run -p example-triangle -- --frames 120
```

With `--frames N`, a successful run exits after N presented frames and prints
`rendered N frames`. Without it, the window runs until closed. The default
feature selects yawgpu; `--no-default-features --features backend-wgpu-native`
and `backend-dawn` select the experimental alternatives. A real Metal-capable
backend is required—yawgpu's Noop backend cannot display the triangle.

Surface creation is implemented with an AppKit `NSView` backed by a
`CAMetalLayer`, so macOS is the supported path for this example. Other targets
currently return a clear unsupported-surface error after creating the window.
