# Block 11 — examples, and the host surface they force into existence

Owner directive (2026-07-11): show how the library works — compute first,
then a windowed triangle. Rules **X1–X8**.

Examples are HOSTS. They may call `webgpu.h` directly (hosts own the GPU),
may be backend-aware in their own code, and demonstrate the architecture's
load-bearing choice: **JS authors; the host renders.**

## Rules

**X1 — examples live in `examples/<name>/` as workspace binary crates**, each
with its own deps (winit only where windowed). They are dev/demo targets:
never part of the test gates, excluded from `default-members` if needed so
plain builds stay lean. Backend selection stays the env-var discipline —
examples read `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` like everything else and say
so in a README per example. Real-GPU output requires a real backend
(Dawn / wgpu-native / a Metal-enabled yawgpu); on Noop the compute example
prints unexecuted zeros and says why.

**X2 — the host-function registration API.** `Runtime::register_host_function
(name, fn)` (shape per adapter idiom; engine-neutral contract in core if it
can ride `ClassSpec`-style machinery, adapter-level otherwise — implementer
decides, planner reviews) so hosts can expose `console.log`-style functions.
The example registers a `print` and builds a minimal `console.log` shim in JS
on top. Errors thrown by host functions follow R26. This is real product
surface, not example scaffolding — tests per principle 1 on both engines.

**X3 — the native-handle accessor.** The windowed example needs the
`WGPURenderBundle` handle out of a JS wrapper: `Runtime::native_render_bundle
(&value) -> Option<WGPURenderBundle>` (class-checked — the C2 lesson; returns
None for wrong classes; the handle's lifetime is the wrapper's, documented
loudly: the host must keep the JS value alive — via a global — while using
the handle, or AddRef it itself). Generalize only as far as the example
needs (bundle now; the pattern is documented for later types). Tests both
engines.

**X4 — `examples/compute`.** Headless. Host: creates the instance via the
ffi dispatch, requestAdapter/requestDevice THROUGH THE BINDING (shows the
async path + tick pumping), loads `compute.js`, ticks until done, prints the
result read from a global. JS: WGSL compute (double each element), buffers,
bind group, pipeline, dispatch, mapAsync readback, writes the numbers to a
global and calls `print`. Every promise `.catch`-ed (J20 discipline).

**X5 — `examples/triangle`.** Windowed (winit). Host: window + surface via
`wgpuInstanceCreateSurface` (metal-layer chain on macOS; the example is
macOS-first, other platforms best-effort), surface configure, then the
frame loop — acquire texture, create view, begin render pass (clear +
attach), `wgpuRenderPassEncoderExecuteBundles` with the bundle obtained via
X3, end, submit, present. JS (`triangle.js`): shader module (vertex+fragment
WGSL triangle), render pipeline, **records a GPURenderBundle once** at init.
**JS never runs during the frame loop** — the example IS the scoping
invariant, and its README says so in one paragraph.

**X6 — surface format handshake.** The host knows the surface format; the JS
pipeline needs it for `fragment.targets[0].format`. The host passes it as a
plain string global (`set_global_value`) before running the script. No new
API.

**X7 — examples get a README each**: what it shows, how to run (env var +
backend note), what output to expect, and — for triangle — the
JS-authors/host-renders paragraph.

**X8 — the boundaries hold.** New host APIs are adapter-level additions or
additive core; the binding still never names backends; no example code leaks
into core/adapters beyond X2/X3; no sibling/absolute paths in anything
committed.

## Exit criteria

1. `cargo run -p example-compute` prints doubled numbers against a real
   backend (planner-verified, gated).
2. `cargo run -p example-triangle` shows the triangle on macOS against a real
   backend (owner- or planner-verified) with JS absent from the frame loop.
3. X2/X3 tested on both engines; standard gates untouched-green.
4. Review pass on the new host APIs (X2/X3) before declaring done.

**X9 — argument conventions follow yawgpu's examples** (owner directive,
2026-07-11): the verification flag is `--verify` — windowed examples auto-exit
success after 60 presented frames, and (in the tiled_deferred spirit) the
final frame's center pixel is read back and printed, proving rasterization;
the default runs until the window closes. No `--frames`-style knobs.
