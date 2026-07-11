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

**X10 — Windows joins the supported example platforms** (owner directive,
2026-07-11). The compute example already runs unmodified (verified: real
yawgpu backend prints the doubled sequence). The triangle example gains a
Windows surface branch: winit's `RawWindowHandle::Win32` provides the `HWND`
and `HINSTANCE`, chained as `WGPUSurfaceSourceWindowsHWND` onto the surface
descriptor — the header documents both fields, and yawgpu recognizes the
chain (its own Windows example framework uses it). The macOS Metal-layer
path is untouched; platforms with neither branch keep the clear
unsupported-surface error. READMEs must say how to run on Windows: MSVC has
no rpath, so the backend DLL's directory must be on `PATH` at runtime in
addition to `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` at build time.

Bring-up findings that reshape this rule (2026-07-11, planner-measured):

- **The X6 handshake was too narrow.** The example consulted capabilities
  for the format but hard-coded `alphaMode: Auto` and requested `CopySrc`
  unconditionally under `--verify`. yawgpu rejects both at configure
  (advertised alpha modes `[Opaque]`, usages `RenderAttachment` only — see
  `../tracking/backend-deltas.md` → D12/D13), and the failure surfaces only
  as a status-6 `GetCurrentTexture` loop. X6 therefore now covers **format,
  alphaMode, and usages**: pick alphaMode from capabilities, and fail
  `--verify` early with a clear message when the surface does not advertise
  `CopySrc`.
- **`--verify`'s readback is Dawn-verified, not yawgpu-verified** (commit
  `bf6d7db` says so; D13 explains why it cannot pass on yawgpu on any
  platform today).
- **Real rendering on Windows needs yawgpu built with its `vulkan` feature**
  (yawgpu block 85: Vulkan is the one real Windows backend; the default
  feature set is Noop-only, which configures but returns `Lost` from
  acquire).

- **yawgpu never auto-selects a real backend.** `wgpuCreateInstance(NULL)`
  returns a Noop instance even in a Vulkan-enabled build; a real backend is
  requested by chaining the vendor extension `YaWGPUInstanceBackendSelect`
  (sType `0x70000001`) onto the instance descriptor — see
  [yawgpu](https://github.com/infosia/yawgpu), header
  `yawgpu/ffi/webgpu-headers/yawgpu.h`. yawgpu's own examples read a
  `YAWGPU_BACKEND` environment variable and chain the struct; ours follow
  the same convention. **X11:** both examples, under
  `#[cfg(feature = "backend-yawgpu")]` (examples are backend-aware hosts —
  X8's boundary applies to core/adapters/ffi, which stay untouched), read
  `YAWGPU_BACKEND` (`noop`/absent, `metal`, `vulkan`, `gles`) and chain the
  hand-declared `#[repr(C)]` vendor struct; an unrecognised value is a
  clear early error, not a silent Noop. The struct and constants are
  mirrored from the vendor header with a citation comment — canonical
  bindings stay vendor-free.

Exit (amended): `cargo run -p example-triangle` with `YAWGPU_BACKEND=vulkan`
renders on Windows against yawgpu+Vulkan (planner-verified); the compute
example under the same selection prints the doubled sequence on a real GPU;
`--verify` passes wherever the backend advertises `CopySrc` and fails with
the clear early error where it does not.

**Verified on Windows, 2026-07-11 (planner, real GPU via Vulkan):** compute
prints `result: 2, 4, 6, 8, 10, 12, 14, 16` (exit 0); the triangle window's
center pixel screen-captures as `145,120,113` — byte-identical to the macOS
Dawn `--verify` readback in the X9 commit — over the `4,6,20` clear color,
so the same bundle rasterizes identically on both platforms. Regressions
held: default (Noop) compute prints the un-doubled input and exits 0;
`--verify` against yawgpu fails with the CopySrc message; an unknown
`YAWGPU_BACKEND` value fails naming the accepted ones; the full workspace
test suite stays green.
