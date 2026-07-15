# webgpu-native-js

A **JavaScript scripting layer for native applications**, exposing the standard
**WebGPU JavaScript API** (`GPUDevice`, `GPUBuffer`, `GPUQueue`, …) inside a
native host — no browser, no Node.js — over any GPU backend that speaks
the standard **WebGPU C ABI** ([`webgpu.h`](https://github.com/webgpu-native/webgpu-headers)).

Any native host that owns a GPU can embed it: game engines, renderers and
DCC/creative tools, simulation and visualization apps, GPU-compute pipelines.

The host owns the GPU. Scripts author resources, pipelines, and application
logic in the same WebGPU API they would use on the web; the host hands the
binding an already-created `WGPUDevice` and pumps one `tick()` per frame.

If your team already knows WebGPU from the web and needs scriptable, moddable
logic inside a native app — without embedding a browser, running Node, or
inventing a scripting language — this is that layer. It presents the standard
WebGPU JavaScript API in-process, over whatever GPU backend and JS engine you
pick, and holds their behavior identical across the combinations you ship.

This is pre-1.0 and under active development. The design bets below are in place
and tested; the API surface is still filling out (see
[Current API surface](#current-api-surface)) and mobile bring-up is ahead.

## What makes it different

- **Write standard WebGPU, in the API your team already knows.** Scripts use
  the same `GPUDevice`/`GPUBuffer`/`GPUQueue` objects and WGSL shaders as the web
  platform, so shaders, pipeline setup, and prototypes carry across. The one
  deliberate change is scoping: the host owns the GPU and drives the frame, so
  the surface is WebGPU as an authoring API, not a browser sandbox.
- **The host owns the GPU.** The primary entry point is *handle adoption* —
  `wrap_device(WGPUDevice)` — not `navigator.gpu.requestAdapter()`. The host
  has already chosen its instance, adapter, and device before any script runs.
  (`requestAdapter`/`requestDevice` exist too, so the async path is real.)
- **Mix and match engine and backend.** The JS engine (Boa or JavaScriptCore)
  and the GPU backend (yawgpu, wgpu-native, or Dawn) are independent, link-time
  choices — the same script and the same binding run on any of them, because the
  binding has no per-engine or per-backend code paths (one engine-agnostic
  conversion core; every GPU call crosses the canonical `webgpu.h` C ABI). The
  freedom is safe to use because the behavior is *verified* identical, not
  assumed: the parity script
  ([`tests/parity/parity.js`](tests/parity/parity.js)) asserts **byte-identical
  output** ([`expected.txt`](tests/parity/expected.txt)) across both engines on
  every run, and reproduces it on Dawn in gated real-GPU runs. Swapping a backend
  or engine is a build-flag decision, not a re-test-everything one.
- **What you test on desktop is what ships on device.** iOS runs the same
  JavaScriptCore as macOS, Android the same Boa as Windows, and Boa↔JavaScriptCore
  parity is verified — so a result on the desktop box you can debug predicts the
  phone you cannot, and a bug cannot hide on the platform you can least reach.
  Portable, predictable behavior is the point of the engine choice, and the same
  parity run enforces it.
- **Embeds in-process as a library, not a runtime.** No browser and no Node on
  any platform — the engine links directly into the application and shares its
  address space. On Apple platforms it uses the system JavaScriptCore, so nothing
  is bundled: no binary-size cost and no App Store bundled-engine question.
  Elsewhere, Boa compiles in — a pure-Rust engine that needs no C toolchain or
  separate VM, so what ships is a static library, not a browser or a runtime
  process.
- **JS is the scripting layer, not the render hot path.** Initialization,
  resource and pipeline definition, and application logic run in JS; per-frame
  draw submission stays in the native host. This scoping is permanent: the engine
  authors and configures the frame, it does not run inside it.

## Compared to the alternatives

- **vs. embedding a browser or WebView** — no browser process and no IPC hop;
  the script calls the GPU in the host's own address space, and the host keeps
  frame control.
- **vs. Node.js and a WebGPU binding** — no Node runtime and no separate
  JavaScript VM to ship; the engine links into the application itself.
- **vs. a non-WebGPU scripting language (Lua and similar)** — the GPU API and
  WGSL your team already knows from the web, rather than a bespoke native binding
  to design, learn, and maintain.

## Architecture

```
        host scripts (JavaScript, WebGPU API)
                │
                ▼
┌─────────────────────────────────────────────────┐
│ adapters/      engine adapters                    │
│   boa/           Tier 1 — Boa (exact crates.io pin)│
│   javascriptcore/ Tier 1 — system JSC, macOS/iOS  │
│                JsEngine impl, class glue,         │
│                microtask pump, finalizer→queue    │
├─────────────────────────────────────────────────┤
│ core/          engine-agnostic binding            │
│                trait JsEngine (associated types), │
│                descriptor conversion (WebIDL),    │
│                Promise bridge + settlement queue, │
│                tick() skeleton, release queue,    │
│                buffer mapping (copy-in/copy-out)  │
├─────────────────────────────────────────────────┤
│ ffi/           webgpu.h C ABI                     │
│                bindgen from pinned webgpu-headers,│
│                backend selected by cargo feature  │
└─────────────────────────────────────────────────┘
                │  webgpu.h (standard WebGPU C ABI)
                ▼
     yawgpu · wgpu-native · Dawn   (dynamic library)
```

- **`core/`** — the engine-agnostic binding. Contains zero references to any JS
  engine and zero backend-specific branches; it is tested against a mock engine with no GPU,
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
| **1 — Supported, all platforms** | [Boa](https://github.com/boa-dev/boa) (MIT/Unlicense, exact crates.io pin) | Primary cross-platform engine. Pure Rust and portable; it needs no C toolchain or engine-specific `bindgen`, and cross-compiles with an ordinary Cargo target build. |
| **1 — Supported (Apple platforms)** | JavaScriptCore | Default-on (`jsc` feature; compiles to an empty crate off Apple platforms). **macOS and iOS** — dynamically linked system framework, so there is no bundled engine and no binary-size cost, and the App Store bundled-engine question does not arise. macOS is fully tested on every run; iOS compiles, with on-device verification deferred to mobile bring-up. Added as the engine-boundary validator; it found five core defects before code generation could multiply them. |

Cross-engine parity is asserted, not assumed: one conformance script
runs under both engines on every test run and must produce **byte-identical
output** — the suite has already caught and retired real divergences (engine
error-class names, method identity, lone-surrogate string handling).

Two engine facts worth knowing when targeting JavaScriptCore (both measured, both
recorded in `specs/`): JavaScriptCore's public C API offers **no way to force
garbage collection to run finalizers** — an unreferenced object is typically
finalized only at context teardown — and no microtask pump; the binding
compensates for the latter, but not the former (see the `destroy()` rule
below).

## JavaScript delivery

**Game JavaScript is delivered to the runtime as a single script.** Multi-file
sources must be bundled by the application's build using the ordinary JavaScript
toolchain, such as esbuild, Rollup, or SWC. Runtime ES modules are a **Boa-only
development-tooling capability** used by the CTS runner; game code must not rely
on them. JavaScriptCore's module API is not part of the public Apple SDK, and
this project does not ship on private API.

**Bundling erases top-level TDZ, so do not rely on it.** A bundler rewrites every
top-level `let`, `const` and `class` to `var`. Under real ES modules — which is what
you get while developing against the Boa module loader — reading a binding before
its initializer throws `ReferenceError`. In the bundle you ship, the same read
silently yields `undefined`. Prefer acyclic module graphs: a circular import that
appears to work in the bundle may be reading `undefined` where the module goal would
have stopped you.

## Backends

| Tier | Backend | Notes |
|---|---|---|
| **1 — Supported** | [yawgpu](https://github.com/infosia/yawgpu) | Primary development and CI backend. Its Noop backend runs the full suite headless — no GPU, no window. |
| **Oracle** | [Dawn](https://dawn.googlesource.com/dawn) | The reference arbiter: this project's `webgpu-headers` pin is Dawn's own `DEPS` pin, Dawn passes both engines' full suites with byte-identical parity, and disagreements with Dawn are presumed binding bugs (investigated, not assumed — the pins win over any implementation). Gated real-GPU runs, not CI. |
| **2 — Experimental** | [wgpu-native](https://github.com/gfx-rs/wgpu-native) | Selected by cargo feature. Divergences from canonical `webgpu.h` are catalogued in `specs/tracking/backend-deltas.md`, never worked around above the FFI layer. |

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
results are only useful if they predict mobile behavior, which is why
portability drove the engine choice. Mobile bring-up is deliberately deferred
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
  On Boa, forgetting `destroy()` means GPU memory waits for a collection.
  **Under JavaScriptCore, `destroy()` is the only bounded path** — the engine
  may not finalize a dropped wrapper until the context itself dies, and
  neither the host nor the binding can force it.
- **Do not call `transfer()` on a mapped-range `ArrayBuffer`.** It moves the
  live mapping into a new buffer the binding cannot see or revoke, leaving a
  dangling view after `unmap()`. Recorded limitation; no guard is practical.
- **Scripts are trusted.** This is first-party application logic, not a browser
  sandbox. The binding spends its effort on catching honest mistakes with
  clear, early errors, not on hardening against adversarial JS.

## Current API surface

The JS-visible surface is generated from WebIDL joined with `webgpu.yml`, and
headless-tested end-to-end. Present so far:

- `wrap_device` → `GPUDevice`; `GPU.requestAdapter` → `GPUAdapter.requestDevice`
- `GPUBuffer`: `createBuffer` (incl. `mappedAtCreation`), `mapAsync`,
  `getMappedRange`, `unmap`, `destroy`, `size`/`usage`/`label`
- `GPUTexture`: `createTexture`, `createView`, `destroy`, and readonly
  dimensions/counts/dimension/format/usage; `GPUTextureView` creation retains
  its parent texture
- `GPUQueue`: `writeBuffer`, `writeTexture`, `submit`, `onSubmittedWorkDone` (`device.queue`
  is `[SameObject]`)
- `createShaderModule` (WGSL), `createBindGroupLayout`, `createPipelineLayout`,
  `createBindGroup` (buffer, sampler, and texture-view resources),
  `createComputePipeline`, `createRenderPipeline` (full descriptor: vertex
  buffers with holes, depth-stencil, multisample, fragment targets with
  blending), `createSampler`, `createTexture` / `GPUTextureView` (readonly
  attributes read through the C getters), `createCommandEncoder`
- `GPUCommandEncoder`: compute/render passes, buffer and texture copy recording,
  and `finish`; render pass: pipeline/buffer/bind-group state, viewport/scissor,
  draw/drawIndexed and their indirect forms, occlusion queries, and `end`;
  compute pass: `setPipeline`, `setBindGroup`, `dispatchWorkgroups`(`Indirect`), `end`;
  single-use command buffers
- `GPURenderBundleEncoder`: `createRenderBundleEncoder`, the render-command mixin
  (pipeline/vertex-index buffers/bind groups, draws, debug markers), `finish` →
  `GPURenderBundle`, and `executeBundles` on a render pass
- `GPUQuerySet`: `createQuerySet`, `resolveQuerySet`, occlusion queries, and
  timestamp writes on compute and render passes
- Immediate data: `setImmediates` on compute/render passes and render bundles,
  and `GPUPipelineLayoutDescriptor.immediateSize`
- Errors: `pushErrorScope`/`popErrorScope`, the `GPUError` hierarchy
  (`GPUValidationError`, `GPUOutOfMemoryError`, `GPUInternalError`),
  `device.onuncapturederror`, and `device.lost`
- Introspection: `GPUAdapter`/`GPUDevice` `info`/`features`/`limits`,
  `getBindGroupLayout`, object `label` round-trips, and
  `GPUShaderModule.getCompilationInfo`
- Async pipeline creation: `createComputePipelineAsync`,
  `createRenderPipelineAsync`

WebIDL semantics are followed: iterator-based `sequence<T>` conversion
(a `Set` or generator is accepted, an array-like is rejected), `[EnforceRange]`
width checks on both the 64→32 and 64→`size_t` edges (tested with 2^32 on
64-bit hosts), nullable vs non-null string distinctions, and required-member
enforcement. Known deviations (e.g. validation errors surface as synchronous
exceptions for null-handle catastrophes; validation errors route to error scopes) are recorded in `specs/`, never silent.

Texture tests on yawgpu's headless Noop backend cover descriptor conversion,
creation, validation, attributes, and lifecycle only. Noop texture-copy
operations do not move texel data, so the headless suite deliberately makes no
claim about texture bytes; byte-level texture tests require a separately gated
real GPU.

Buffer mapping is strategy-selected per engine. Both supported engines use
**copy-in/copy-out**: Boa owns its `ArrayBuffer` allocation, while
JavaScriptCore's public C API cannot detach external memory and taking a C
pointer to a script-visible buffer would silently and permanently disable
detaching it. The dormant zero-copy capability remains in `core/` pending an
explicit owner decision; no supported engine currently selects it. This is a
performance difference, not a behavioral one.

## Building and testing

Prerequisites: Rust (stable), the native tooling required by the WebGPU FFI,
and the two pinned specification/header submodules:

```sh
git submodule update --init third_party/webgpu-headers third_party/gpuweb
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

# the full workspace against yawgpu (Boa engine), headless
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
- **The upstream [WebGPU CTS](https://github.com/gpuweb/cts) runs against the
  binding.** A harness executes the TypeScript suite under Boa against yawgpu
  (headless) and Dawn (the oracle) — the same approach `dawn.node` uses, one
  engine down. A curated set of tens of thousands of cases passes with zero
  failures; execution-result families the headless Noop backend cannot run are
  verified on Dawn, and every catalogued expected-failure carries a reason. The
  per-conversion unit tests and the byte-identical parity script run underneath
  it.

## Status

Pre-1.0, working through a phased plan (see `specs/`). In place and tested: the
engine boundary; the async and mapping machinery; the API surface above; code
generation from WebIDL joined with `webgpu.yml` (the same input pairing
`dawn.node` generates from); error scopes and the `GPUError` hierarchy;
`onuncapturederror` and `device.lost`; the texture/render surface; and immediate
data. The JavaScriptCore adapter validates the central design bet end-to-end, and
a WebGPU-CTS harness runs the binding against yawgpu (headless) and Dawn (the
oracle). Next: mobile bring-up, and continued CTS coverage.

## License

Dual-licensed under **MIT** ([LICENSE-MIT](LICENSE-MIT)) or **Apache-2.0**
([LICENSE-APACHE](LICENSE-APACHE)), at your option — the same terms as
[yawgpu](https://github.com/infosia/yawgpu) and the Rust ecosystem norm.

Third-party components keep their own licenses, and all of them are compatible
with that choice:

- **Boa** (the Tier 1 engine) — MIT or Unlicense; vendored only as a
  crates.io dependency.
- **`webgpu-headers`** — BSD-3-Clause; the pinned canonical `webgpu.h`.
- **gpuweb** — the W3C document licenses; used as the pinned `webgpu.idl`
  codegen input.
- **JavaScriptCore** (LGPL-2.1) — never vendored. It is only ever
  **dynamically linked** as an Apple system framework (the default-on `jsc`
  feature compiles to an empty crate off Apple platforms), so no LGPL
  obligations attach to a binary that links it this way.

Contributions are accepted under the same dual license, without any additional
terms.
