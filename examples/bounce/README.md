# Windowed bouncing bodies

This example is the smallest proof of the skeleton/data rendering split
(`specs/blocks/19-skeleton-data-split.md`). Two rules govern it: dynamics
expressible as buffer contents never re-record commands, and commands re-record
only on pipeline-composition change, as an explicit host-observable event —
never implicitly, never per frame.

Eight bodies render through one `drawIndirect` recorded in a render bundle. The
16-byte indirect buffer holds `[vertexCount=6, instanceCount, firstVertex=0,
firstInstance=0]`; `instanceCount` is the only word that ever changes. On a
fixed schedule bodies despawn from 8 down to 3 and respawn back to 8 — every
visibility change is a `writeBuffer` into the indirect buffer, and the command
skeleton does not change. All 8 bodies integrate every frame regardless of
visibility; drawn instances are the first `alive` bodies, so no compaction
exists.

The steady-state cost per frame is one `Runtime::frame` call plus two
`queue.writeBuffer` calls (body storage, indirect args) — constant in body
count and in visibility changes. Raising `N` raises JavaScript simulation cost
and the size of one buffer copy, but not the number of binding crossings.

At frame 45 the example demonstrates the one legitimate re-record: the script
records a new bundle against a second pipeline (a dimmed fragment variant,
created at init like every other resource), assigns it to
`globalThis.bounceBundle`, increments `globalThis.bundleGeneration`, and calls
the registered host function `signalBundleSwap()`. The host, after that
`frame()` returns and before encoding the pass, re-borrows the native handle,
replaces its retention global, and swaps its stored handle. The superseded
handle is never touched again: its release rides GC and the release queue.
Under JavaScriptCore a superseded bundle would live until context teardown
(finalizers effectively never run earlier, and `GPURenderBundle` has no
`destroy()`), which is one reason re-record is rare by design; this example
links the Boa engine. That one frame spends O(bundle commands) crossings, and
the verify golden proves it happens exactly once in the run.

`examples/triangle` shows the static case with zero per-frame JavaScript; this
example shows the dynamic case — per-frame data through buffer writes, and
structural change as an explicit, counted event.

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

With `--verify`, the host supplies `1.0 / 60.0` as `dt` and requires 90
successful presents. The script prints checkpoints at frames 30, 45, 60, and 90
(`alive` count and bundle generation) and every body's position at frame 90;
the host compares the capture with `expected.txt` and additionally asserts
exactly one swap signal and that the swapped-in native handle differs from the
original. A mismatch, a missing swap, or an unsuccessful present exits
non-zero. Without `--verify`, the host supplies wall-clock `dt` and runs until
the window closes; the schedule is frame-based, so the despawn/respawn cycle
and the frame-45 color change still occur once, early in the run. No pixel
readback is performed because `examples/triangle` verifies surface pixels.

A callback error exits this example with the frame error. A shipping host can
instead rate-limit its log and keep rendering; that decision is host policy,
not binding behavior.

The default feature selects yawgpu. `--no-default-features --features
backend-wgpu-native` and `backend-dawn` select the experimental alternatives. A
real GPU-capable backend is required because yawgpu's Noop backend cannot
display a window. yawgpu selects Noop when `YAWGPU_BACKEND` is absent;
`YAWGPU_BACKEND=metal` selects Metal on macOS and `YAWGPU_BACKEND=vulkan`
selects Vulkan on Windows. Surface creation supports macOS and Windows.
