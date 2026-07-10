# webgpu-native-js

A **JavaScript scripting layer for native game engines**, exposing the standard
**WebGPU JavaScript API** (`GPUDevice`, `GPUBuffer`, `GPUQueue`, …) inside a
native application — no browser, no Node.js — over any GPU backend that speaks
the standard **WebGPU C ABI** ([`webgpu.h`](https://github.com/webgpu-native/webgpu-headers)).

The host engine owns the GPU. Scripts author resources, pipelines, and game
logic in the same WebGPU API they would use on the web; the engine hands the
binding an already-created `WGPUDevice` and pumps one `tick()` per frame.

## What makes it different

- **JS is the scripting layer, not the render hot path.** Initialization,
  resource and pipeline definition, and game logic run in JS. Per-frame draw
  submission stays in the native host. This scoping is permanent and is what
  keeps a JIT-less embedded engine viable.
- **The host owns the GPU.** The primary entry point is *handle adoption* —
  `wrap_device(WGPUDevice)` — not `navigator.gpu.requestAdapter()`. The engine
  has already chosen its instance, adapter, and device before any script runs.
  (`requestAdapter`/`requestDevice` exist too, so the async path is real.)
- **One conversion layer, N engines.** All descriptor conversion, validation,
  promise plumbing, and lifetime management is written **once** in an
  engine-agnostic core against a `trait JsEngine` with associated types, and
  monomorphized per engine — no `dyn` dispatch on the conversion path, and no
  per-engine conversion code. Wiring the second engine (JavaScriptCore)
  required **zero changes to core logic**; that gate is enforced per phase.
- **Backend-swappable by construction.** Every GPU call crosses the canonical
  `webgpu.h` C ABI through `bindgen`-generated bindings. The binding never
  touches a backend's native API, so yawgpu, wgpu-native, and Dawn are
  link-time choices, not code paths.
- **Engine-parity is a tested claim, not a goal.** The same conformance script
  ([`tests/parity/parity.js`](tests/parity/parity.js)) runs under QuickJS and
  JavaScriptCore and must produce **byte-identical output**
  ([`tests/parity/expected.txt`](tests/parity/expected.txt)), asserted by one
  test in each adapter. Promise settlement ordering, label conversion, mapping
  round-trips, sequence conversion, and error shapes are all in that script.

## Architecture

```
        game scripts (JavaScript, WebGPU API)
                │
                ▼
┌─────────────────────────────────────────────────┐
│ adapters/      engine adapters                    │
│   quickjs/       Tier 1 — quickjs-ng (pinned)     │
│   javascriptcore/ Tier 1 — system JSC, macOS/iOS  │
│                JsEngine impl, class glue,         │
│                microtask pump, finalizer→queue    │
├─────────────────────────────────────────────────┤
│ core/          engine-agnostic binding            │
│                trait JsEngine (associated types), │
│                descriptor conversion (WebIDL),    │
│                Promise bridge + settlement queue, │
│                tick() skeleton, release queue,    │
│                buffer mapping (both strategies)   │
├─────────────────────────────────────────────────┤
│ ffi/           webgpu.h C ABI                     │
│                bindgen from pinned webgpu-headers,│
│                backend selected by cargo feature  │
└─────────────────────────────────────────────────┘
                │  webgpu.h (standard WebGPU C ABI)
                ▼
     yawgpu · wgpu-native · Dawn   (dynamic library)
```

- **`core/`** — the heart. Contains zero references to any JS engine and zero
  backend-specific branches; it is tested against a mock engine with no GPU,
  no engine, and no backend library present. The mock deliberately models the
  *strictest* union of both engines' obligations (value ownership, detach
  semantics, mapped-range overlap rules), so core bugs fail core tests.
- **`ffi/`** — types-only unless a backend feature is enabled; `bindgen` runs
  against the pinned canonical header. Generated code is never committed and
  never edited.
- **`adapters/*`** — one crate per engine. An adapter may not know the name of
  any WebGPU class or member; everything is driven by core's class specs.

## Engines

| Tier | Engine | Notes |
|---|---|---|
| **1 — Supported** | [quickjs-ng](https://github.com/quickjs-ng/quickjs) (MIT, pinned submodule at v0.15.1) | Primary engine. JIT-less and portable — dev results on desktop predict behavior on mobile. Built from source with raw `bindgen`. |
| **1 — Supported (Apple platforms)** | JavaScriptCore | Default-on (`jsc` feature; compiles to an empty crate off Apple platforms). **macOS and iOS** — dynamically linked system framework, so there is no bundled engine and no binary-size cost, and the App Store bundled-engine question does not arise. macOS is fully tested on every run; iOS compiles, with on-device verification deferred to mobile bring-up. Born as the engine-boundary validator — it earned the promotion by finding five core defects before code generation could multiply them. |

Cross-engine parity is not assumed, it is asserted: one conformance script
runs under both engines on every test run and must produce **byte-identical
output** — the suite has already caught and retired real divergences (engine
error-class names, method identity, lone-surrogate string handling).

Two engine facts worth knowing when targeting JavaScriptCore (both measured, both
recorded in `specs/`): JavaScriptCore's public C API offers **no way to force
garbage collection to run finalizers** — an unreferenced object is typically
finalized only at context teardown — and no microtask pump; the binding
compensates for the latter, but not the former (see the `destroy()` rule
below).

## Backends

| Tier | Backend | Notes |
|---|---|---|
| **1 — Supported** | [yawgpu](https://github.com/infosia/yawgpu) | Primary development and CI backend. Its Noop backend runs the full suite headless — no GPU, no window. |
| **2 — Experimental** | [wgpu-native](https://github.com/gfx-rs/wgpu-native), [Dawn](https://dawn.googlesource.com/dawn) | Selected by cargo feature. Divergences from canonical `webgpu.h` are catalogued in `specs/tracking/backend-deltas.md`, never worked around above the FFI layer. |

Backend conformance itself is out of scope here — it is owned by
[webgpu-native-cts](https://github.com/infosia/webgpu-native-cts), which
validates backends against the WebGPU CTS with Dawn as the oracle. This
project's job is the layer above: whether the *JS binding* faithfully presents
that C ABI as WebGPU-shaped JavaScript.

## Target platforms

| Tier | Platforms |
|---|---|
| **Production (execution)** | iOS, Android |
| **Development / testing** | Windows, macOS |

Behavioral parity across all four is a first-class concern: desktop test
results are only useful if they predict mobile behavior, which is the entire
reason a JIT-less engine was chosen. Mobile bring-up is deliberately deferred
until the API surface is filled out on desktop.

## The host contract

The host pumps the binding **once per frame**:

```
tick():
  1. wgpuInstanceProcessEvents(instance)   — WebGPU callbacks record results
  2. settle all recorded promises          — in ONE JS frame (parity-critical)
  3. drain the engine's microtask queue    — .then() continuations actually run
  4. drain the native release queue        — GC'd wrappers release GPU handles
```

Resolving a promise does not run its continuations — a host that pumps only
`ProcessEvents` passes every test that avoids `await` and hangs on the first
one that uses it. The four-step order lives in `core/` once; adapters delegate.

Three rules for script authors:

- **Call `destroy()`. GC is a backstop, not a resource-management strategy.**
  On QuickJS, forgetting `destroy()` means GPU memory waits for a finalizer.
  **Under JavaScriptCore, `destroy()` is the only bounded path** — the engine
  may not finalize a dropped wrapper until the context itself dies, and
  neither the host nor the binding can force it.
- **Do not call `transfer()` on a mapped-range `ArrayBuffer`.** It moves the
  live mapping into a new buffer the binding cannot see or revoke, leaving a
  dangling view after `unmap()`. Recorded limitation; no guard is practical.
- **Scripts are trusted.** This is first-party game logic, not a browser
  sandbox. The binding spends its effort on catching honest mistakes with
  clear, early errors, not on hardening against adversarial JS.

## Current API surface

Filled so far (headless-tested end-to-end under both engines):

- `wrap_device` → `GPUDevice`; `GPU.requestAdapter` → `GPUAdapter.requestDevice`
- `GPUBuffer`: `createBuffer` (incl. `mappedAtCreation`), `mapAsync`,
  `getMappedRange`, `unmap`, `destroy`, `size`/`usage`/`label`
- `GPUQueue`: `writeBuffer`, `submit`, `onSubmittedWorkDone` (`device.queue`
  is `[SameObject]`)
- `createShaderModule` (WGSL), `createBindGroupLayout`, `createPipelineLayout`,
  `createBindGroup`, `createComputePipeline`, `createCommandEncoder`
- `GPUCommandEncoder`: `beginComputePass`, `copyBufferToBuffer`, `finish`;
  compute pass: `setPipeline`, `setBindGroup`, `dispatchWorkgroups`, `end`;
  single-use command buffers

WebIDL semantics are taken seriously: iterator-based `sequence<T>` conversion
(a `Set` or generator is accepted, an array-like is rejected), `[EnforceRange]`
width checks on both the 64→32 and 64→`size_t` edges (tested with 2^32 on
64-bit hosts), nullable vs non-null string distinctions, and required-member
enforcement. Known deviations (e.g. validation errors surface as synchronous
exceptions until error scopes land) are recorded in `specs/`, never silent.

Buffer mapping is strategy-selected per engine: **zero-copy detach** on QuickJS
(the `ArrayBuffer` aliases the mapping and is detached at `unmap()`), and
**copy-in/copy-out** on JavaScriptCore, whose public C API cannot detach
external memory — and where taking a C pointer to a script-visible buffer would
silently and permanently disable detaching it. Both strategies are implemented
once, in `core/`. This is a performance difference, not a behavioral one.

## Building and testing

Prerequisites: Rust (stable), a C toolchain, and the two pinned submodules:

```sh
git submodule update --init third_party/quickjs third_party/webgpu-headers
```

The binding builds **types-only** with no backend. Anything that actually
calls the GPU needs one backend feature and a directory containing the
backend's dynamic library, via the `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR`
environment variable — point it at a yawgpu `target/release`, for example.
Wire it in a **gitignored** `.cargo/config.toml`; committed files never carry
machine-specific paths:

```toml
[env]
WEBGPU_NATIVE_JS_BACKEND_LIB_DIR = "/path/to/backend/lib"
```

```sh
# engine-agnostic core: no engine, no backend, no GPU
cargo test -p webgpu-native-js-core

# the full workspace against yawgpu (QuickJS engine), headless
cargo test --workspace --features webgpu-native-js-ffi/backend-yawgpu

# the JavaScriptCore adapter (macOS; default-on)
cargo test -p javascriptcore-adapter
```

Every gate is **headless-first**: the entire suite passes with no GPU and no
window against yawgpu's Noop backend. Real-GPU and windowed tests are gated
separately and never required.

## Quality

- **Every public function has a direct unit test**, in the same commit that
  adds it.
- **The mock engine is the strictest engine.** Core is tested against a mock
  that takes the union of both real engines' obligations — property reads can
  throw, coercions can re-enter, detach can silently no-op, mapped-range
  overlaps are rejected — so the default `cargo test` catches bug classes that
  a forgiving mock would wave through.
- **Negative demonstrations.** Guards for memory hazards are required to be
  *seen red* — break the guard, watch the named test fail, restore it — and
  review phases run deletion experiments against the tree to find tests that
  cannot fail.
- **Dual-engine parity is asserted byte-for-byte** by the shared conformance
  script, on every test run.
- The long-term binding oracle is the upstream
  [WebGPU CTS](https://github.com/gpuweb/cts) itself, which is written in
  TypeScript and can eventually run inside the engine under test — the same
  approach `dawn.node` uses, one engine down. Out of scope until codegen
  lands; the per-conversion unit tests and the parity script carry the load
  until then.

## Status

Working through a phased plan (see `specs/`): the engine boundary, the async
and mapping machinery, and the API surface above are in place; the
JavaScriptCore adapter validates the central design bet end-to-end. Next:
code generation from WebIDL joined with `webgpu.yml` (the same input pairing
`dawn.node` generates from), then error scopes, then mobile bring-up.

## License

Not yet decided — this repository currently ships no license, so all rights
are reserved in the interim. Third-party components keep their own licenses:
quickjs-ng is MIT, `webgpu-headers` is BSD-3-Clause, and JavaScriptCore
(LGPL-2.1) is only ever **dynamically linked** as an Apple system framework
(the `jsc` feature, default-on, compiles to an empty crate off Apple
platforms).
