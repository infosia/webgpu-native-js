# Project Plan: webgpu-native-js

**Status:** Working draft, Rev 2 (2026-07-09). Not a contract — see §0.
**Rev 1 → Rev 2:** three load-bearing claims in Rev 1 were checked against the
canonical `webgpu.h` and against `dawn.node`, and found wrong. They are
corrected in place; the full list and its evidence is in §7 (Revision history),
which exists so the corrections are not silently re-litigated back to Rev 1.

---

## 0. How to read this document

This plan is revised as evidence arrives; it is not a decision record to be
defended. Two rules:

1. **Claims in this document that can be checked against a header, a spec, or
   an existing implementation must be checked before being built on.** Rev 1's
   most expensive errors were all of this kind — each was refutable in minutes
   by reading `webgpu.h`.
2. **`CLAUDE.md` holds the invariants** (roles, boundaries, conventions). This
   plan holds the design and the phasing. Where they disagree, `CLAUDE.md`
   wins; where evidence disagrees with either, fix both.

Open questions live in §6 and are marked as genuinely undecided. Do not let
them harden into assumptions by being restated confidently in a later document.

---

## 1. Background

### 1.1 Related prior projects (already built, do not re-implement)

- **[yawgpu](https://github.com/infosia/yawgpu)** — "Yet Another WebGPU C API
  implementation in Rust." Implements the `webgpu.h` C ABI (the same header
  standard implemented by Dawn and wgpu-native), plus SPIR-V/MSL passthrough,
  mobile TBDR extensions, and other extras. This is the WebGPU backend this
  project primarily targets, but the design must not hard-couple to it.
- **[webgpu-native-cts](https://github.com/infosia/webgpu-native-cts)** — A
  native (C++, no JS engine) port of the WebGPU Conformance Test Suite that
  links against `webgpu.h` and is backend-swappable between yawgpu, Dawn, and
  wgpu-native at build/link time. Reuse its backend-swap build patterns (CMake
  `CTS_BACKEND`-style selection) as a reference, translated to Cargo feature
  flags.

### 1.2 Two different conformance questions

Rev 1 said "this project is not for conformance testing — that concern is fully
owned by `webgpu-native-cts`." That is half right, and the half that is wrong
matters.

| Question | Owner | Oracle |
|---|---|---|
| Does *yawgpu* correctly implement WebGPU? | `webgpu-native-cts` | Dawn, via the C ABI |
| Does the *JS binding* faithfully present that C ABI as WebGPU-shaped JS? | **this project** | see §5.4 |

`webgpu-native-cts` structurally cannot answer the second question: the bug
class here is "the binding mis-converted a `GPUBufferDescriptor`," which either
never reaches the C ABI or reaches it as a well-formed but wrong call. This
project therefore needs its own test strategy (§5.4). It does **not** need to
re-implement backend conformance.

### 1.3 What this project is for

Enable JavaScript as a **scripting/authoring layer inside native game engines**
(and other native apps) that use `webgpu.h`-based WebGPU implementations
(yawgpu, Dawn, wgpu-native — swappable) as their rendering backend.

**Explicitly not the goal:** using JS as the mechanism to issue per-frame draw
calls / the render hot path. That stays in native code. JS is for
initialization, resource/pipeline definition, scripting logic, and similar
non-hot-path work. This scoping decision is what makes the JS engine choice
tractable — see §3.

**Corollary Rev 1 missed (§2.8):** if the host is a game engine, the host has
already created the instance, adapter, and device before any script runs. JS
adopting an existing `WGPUDevice` is the *primary* entry point; `requestAdapter`
is secondary.

### 1.4 Target platforms

- **Production (execution):** iOS, Android
- **Development / testing:** Windows, macOS

Behavioral parity across all four is a first-class concern, because dev/test
results on Windows/macOS are only useful if they predict behavior on iOS/Android.

---

## 2. Architecture

### 2.1 Layered design

```
┌───────────────────────────────────────────────┐
│  Per-engine adapter (thin)                     │  adapters/quickjs, adapters/jsc
│  - impl JsEngine (value ops, class registry)   │
│  - Promise primitive + microtask pump          │
│  - finalizer → release queue hookup            │
├───────────────────────────────────────────────┤
│  Engine-agnostic core, generic over E: JsEngine│  core/
│  - descriptor conversion (the bulk of the work)│
│  - object model, lifetime rules                │
│  - thread-safe release queue                   │
├───────────────────────────────────────────────┤
│  webgpu.h FFI (bindgen-generated)              │  Rust extern "C" declarations
├───────────────────────────────────────────────┤
│  Swappable backend .so/.dylib/.a               │  yawgpu / Dawn / wgpu-native
└───────────────────────────────────────────────┘
```

If the core/adapter boundary is drawn correctly, adding a second engine requires
zero changes to `core/`'s *logic* — only additive `JsEngine` trait methods.
Treat the second engine as the validation checkpoint for the boundary.

**Rev 1's rationale for this layering was right; its estimate was wrong.** Rev 1
said "~90% of binding work (object shape, method dispatch, struct↔JS conversion,
lifetime rules) is identical regardless of JS engine." Method dispatch is indeed
mostly shared, but method dispatch is not the bulk of the work — struct↔JS
conversion is, and it touches engine-native value types on every line. See §2.4.

### 2.2 FFI boundary: must go through `webgpu.h`, not yawgpu's Rust internals

Even though yawgpu is Rust (same language as most of this project's adapter
code), binding directly to yawgpu's internal Rust API would break
backend-swappability with Dawn (C++) and wgpu-native. **All GPU calls must cross
the `webgpu.h` C ABI**, generated via `bindgen` from the canonical
[`webgpu-headers`](https://github.com/webgpu-native/webgpu-headers) `webgpu.h`.
A convenience shortcut through yawgpu's Rust internals is a design violation,
not an optimization.

### 2.3 Codegen: the source of truth is WebIDL joined with `webgpu.yml`

Do not hand-write bindings for all ~40 WebGPU IDL interfaces per engine. But
**Rev 1's proposed generator input was wrong.**

`webgpu.yml` is the machine-readable description of the **C ABI**. It does not
carry the JS-facing surface:

- dictionary member **defaults** and **required**-ness
- string enums (`"read-only"`, not an integer)
- flag namespaces (`GPUBufferUsage.MAP_READ`)
- `Promise<T>` return types
- exception-vs-error-scope routing
- `[EnforceRange]` / `[Clamp]` numeric coercion rules

Generating from `webgpu.yml` alone means hand-encoding all of WebIDL's semantics
anyway, ~40 times.

The precedent Rev 1 should have cited is **`dawn.node`**, which generates its
`src/dawn/node/interop/` layer from **WebIDL** (`webgpu.idl` + `Browser.idl` +
`DawnExtensions.idl`) via `tools/src/cmd/idlgen` and `WebGPU.cpp.tmpl`. The
correct design is a **join**:

- **WebIDL** → the JS-facing shape, defaults, and coercion rules.
- **`webgpu.yml` / `webgpu.h`** → the C ABI to lower onto.

Neither alone is sufficient. Note that yawgpu vendors only `webgpu.h`, not
`webgpu.yml` — sourcing and pinning both inputs is itself a Phase 0 task (§6).

Still worth reading before designing the generator:
[xgpu](https://github.com/PyryM/xgpu) (Python) — not only for struct-chaining
and count+pointer array conventions, but for its **lifetime discipline** around
temporaries (§2.4);
[webgpu-headers' Go generator](https://pkg.go.dev/github.com/webgpu-native/webgpu-headers/gen)
— the canonical schema→code transform.

### 2.4 Core data model: `trait JsEngine` with associated types

**Rev 1's §2.4 sketch cannot be implemented.** It proposed:

```rust
trait PromiseBridge { fn resolve(&self, value: JsValueHandle); }  // WRONG
```

There is no engine-agnostic `JsValueHandle`. QuickJS's `JSValue` is a 16-byte
NaN-boxed, reference-counted tagged union. JSC's `JSValueRef` is an opaque
GC-traced pointer that requires a `JSContextRef` for every single operation.
Any type that erases both is either a `dyn` call per field access — on the
descriptor-conversion path, which is the hot path of the *binding* even if not
of the renderer — or a lie.

The boundary is instead a trait with **associated types, monomorphized per
engine**:

```rust
trait JsEngine {
    type Value: Copy;
    type Context<'a>;

    fn get_property(cx: Self::Context<'_>, obj: Self::Value, key: &str) -> Self::Value;
    fn to_u32(cx: Self::Context<'_>, v: Self::Value) -> Result<u32, TypeError>;
    fn to_f64(cx: Self::Context<'_>, v: Self::Value) -> Result<f64, TypeError>;
    fn as_str(cx: Self::Context<'_>, v: Self::Value) -> Result<&str, TypeError>;

    fn new_arraybuffer(cx: Self::Context<'_>, ptr: *mut u8, len: usize) -> Self::Value;
    fn detach_arraybuffer(cx: Self::Context<'_>, v: Self::Value) -> Result<(), Unsupported>;

    fn register_class(cx: Self::Context<'_>, spec: &ClassSpec<Self>);
    fn new_promise(cx: Self::Context<'_>) -> (Self::Value, Deferred<Self>);
    fn pump_microtasks(cx: Self::Context<'_>);
}
```

`core/` is generic over `E: JsEngine` and compiles to zero-cost concrete code per
engine. **The descriptor conversions — the actual bulk of the work — are written
once, in `core/`, against this trait.** This is the project's central design
bet. Get it wrong and every conversion is written twice.

`ClassSpec<E>` (properties, methods, finalizer) survives from Rev 1's
`NativeClassSpec`, but parameterized by `E` rather than erased.

**Per-call arena.** Descriptor conversion additionally needs scratch memory:
`WGPUStringView` is `{data, length}` and **not null-terminated**, and
`count + pointer` arrays must outlive the FFI call without outliving the JS
values they were read from. Allocate temporaries in a bump allocator reset after
each call. Rev 1 omitted this entirely.

### 2.5 Object lifetime: release queue, and why

QuickJS finalizers fire deterministically on a known thread (refcounting GC).
JSC finalizers **may fire on any thread** (documented engine behavior).

**Rev 1 justified the release queue by that JSC fact and called it "the
highest-risk piece of the whole design." Both halves need correcting.**

The firmer justification: **`webgpu.h` documents no general thread-safety
guarantee for `wgpuXxxRelease`.** (The header's only "any thread" statement
concerns the uncaptured-error callback.) Every current implementation happens to
be thread-safe by construction — yawgpu and wgpu-native use `Arc`, Dawn uses
atomic refcounts — but that is an *implementation accident*, and depending on it
across three swappable backends is exactly the coupling this project exists to
avoid.

Two further reasons Rev 1 did not give:

- It is the natural place to enforce **child-released-before-parent** ordering.
  Rev 1 deferred that to Phase 6 as a robustness pass; it is a **design input**.
- It keeps release calls out of any `wgpuInstanceProcessEvents` callstack, which
  `webgpu.h` warns about (§2.6).

So: all finalizers, regardless of engine, push a release request onto a shared
thread-safe queue rather than calling `wgpuXxxRelease` directly. A designated
GPU-owning thread drains it.

But it is a bounded, ordinary MPSC queue. **It is not the highest-risk piece of
the design** — §2.4 is, and within §2.4 the specific unknown in §6 (JSC
ArrayBuffer detach) is the thing most likely to break the whole bet.

**GC is a backstop, not a resource-management strategy.** WebGPU has explicit
`destroy()` on buffers, textures, and devices for a reason. On mobile, waiting
for a finalizer to free GPU memory is a bug. Scripts are expected to call
`destroy()`; the finalizer exists so that forgetting is a leak-until-GC rather
than a leak-forever. This must be said in the user-facing docs.

### 2.6 Async: a contract you choose, not an unknown to discover

**Rev 1 listed as a "blocking unknown": which thread yawgpu's `webgpu.h`
callbacks fire on. This is not an unknown.** `webgpu.h` defines `WGPUFuture` and
`WGPUCallbackMode`; the caller selects the mode per callback:

| Mode | Fires |
|---|---|
| `WGPUCallbackMode_WaitAnyOnly` | only inside `wgpuInstanceWaitAny` |
| `WGPUCallbackMode_AllowProcessEvents` | also inside `wgpuInstanceProcessEvents` |
| `WGPUCallbackMode_AllowSpontaneous` | any time, **on an arbitrary thread** |

yawgpu implements all three modes, plus `wgpuInstanceProcessEvents` and
`wgpuInstanceWaitAny` (see `yawgpu/src/ffi/instance.rs` in the yawgpu
repository).

**Decision: every JS-facing async operation uses
`WGPUCallbackMode_AllowProcessEvents`** — `requestAdapter`, `requestDevice`,
`mapAsync`, `popErrorScope`, `onSubmittedWorkDone`. Callbacks then fire only on
the thread that pumps, so **Promise resolution needs no cross-thread signaling
at all.** `AllowSpontaneous` is forbidden on JS-facing paths: `webgpu.h`
explicitly documents that re-entrantly calling the API from inside a spontaneous
callback is undefined behaviour.

`WGPUDeviceLostCallback` / uncaptured-error are the exception — the header states
they have no configurable mode and may fire at any time. Those must marshal to
the JS thread through a queue and must not call back into `webgpu.h`.

### 2.7 The host event-loop contract (Rev 1 omitted this entirely)

This is the consequence of §2.6 that Rev 1 never drew. **This project has an
event-loop contract with its host.** Once per frame the host must pump *two*
queues, in order:

1. `wgpuInstanceProcessEvents(instance)` — fires WebGPU callbacks, which resolve
   the corresponding JS `Promise`s.
2. The engine's **microtask queue** — QuickJS `JS_ExecutePendingJob` in a loop,
   or JSC's equivalent — which actually runs the `.then()` continuations.

**Resolving a Promise does not run its callbacks.** A binding that does step 1
and not step 2 produces `await`s that hang forever, and it will pass every unit
test that does not involve `await`. Specify, expose, and test this pump before
building anything on top of Promises. It belongs in the public API
(`WebGpuJs::tick()`), not buried in an adapter.

### 2.8 Handle import: the primary entry point

Rev 1's Phase 1 vertical slice was `GPU.requestAdapter()` →
`GPUAdapter.requestDevice()` → `GPUDevice.createBuffer()` → `mapAsync()`. That is
the shape a **browser** has. It is not the shape a **game engine** has.

Per §1.3, the host already owns a `WGPUDevice` before the script VM starts. So
the primary entry point is adopting an existing handle:

```rust
fn wrap_device<E: JsEngine>(cx: E::Context<'_>, dev: WGPUDevice) -> E::Value;
// calls wgpuDeviceAddRef; JS GPUDevice finalizer pushes a release onto the queue
```

`requestAdapter`/`requestDevice` remain necessary for Web-source-compatibility
and standalone tools, but they are the **secondary** path. Building the import
path first also lets the first vertical slice skip async entirely, which
decouples §2.4 (the risky bet) from §2.6/§2.7 (the merely fiddly ones).

### 2.9 Surface / windowing (non-spec extension)

There is no `<canvas>` equivalent in a native host. `WGPUSurface` can be created
directly from a native window handle (HWND / NSView·CAMetalLayer / ANativeWindow
/ UIView / X11 / Wayland) — a native `webgpu.h` concept independent of the
browser `<canvas>`; Dawn's own native examples (including Android via
`ANativeWindow`) demonstrate the pattern. Expose a
`createSurfaceFromNativeHandle()`-equivalent in a later phase (§4, Phase 5). It
is additive to the WebGPU IDL, not part of it, and does not block core Phase 1–3
work, which proceeds against compute/offscreen use cases only.

---

## 3. Key decisions and their rationale

Do not relitigate these without new information. "New information" means a
header, a spec text, or a working prototype — not a recollection.

### 3.1 Rejected: Node.js / N-API as the (only) target

N-API was seriously considered because of the existing reference implementation
(`dawn.node`) and because Node-API is implemented by Node.js, Bun, and Deno (one
build, three hosts). Deprioritized as the primary target because:

- Per-call marshaling cost across the N-API boundary is a poor fit if JS ever
  touches the render hot path.
- V8's GC model doesn't suit a tight frame loop.
- Node's libuv event loop isn't designed around a fixed-timestep render loop.
- **It does not serve the actual target platforms (iOS/Android) at all** —
  N-API is a Node.js-family-runtime concept, irrelevant on mobile.

May be revisited purely as a **desktop tooling/editor** target — a separate
concern from the mobile runtime this plan covers.

### 3.2 Engine selection: QuickJS (primary) + JavaScriptCore (secondary)

Given production targets are iOS + Android with Windows/macOS for dev/test:

- **iOS forbids JIT for any in-process JS engine**, including JavaScriptCore via
  `JSContext` — JIT is only available to WKWebView's out-of-process engine. This
  neutralizes JSC's usual "has JIT" advantage on the actual production platform.
- **JavaScriptCore has no system-provided build on Android** and no officially
  supported path; using it there requires building a WebKit fork from source.
- **QuickJS** builds cleanly via NDK on Android, has no JIT to lose on iOS (so no
  dev/prod performance-characteristic mismatch), is small, and starts fast. Its
  deterministic (refcounted) finalization set the direction for §2.5.

**Decision: QuickJS is the primary/first-class engine.** JavaScriptCore is a
secondary target pursued for multi-engine support and as a validation check on
the core/adapter boundary (§2.1).

**Rev 2 additions:**

- **"QuickJS" is a fork choice, not a dependency.** ✅ **DECIDED 2026-07-09:
  [quickjs-ng](https://github.com/quickjs-ng/quickjs)** (MIT), pinned as a git
  submodule, bindings via raw `bindgen` in our own `build.rs`; we depend on
  neither `rquickjs` nor `rquickjs-sys`. Decisive criterion: Bellard's original
  has no official MSVC support, and Windows is a dev target whose results must
  predict production. quickjs-ng additionally ships CMake, sanitizer CI across
  50+ configurations, and calls out Android/iOS. `rquickjs` is the wrong layer
  (a safe wrapper beneath our own `JsEngine` abstraction), and `rquickjs-sys`
  ships no pre-generated bindings for Android or iOS — our two ship targets — so
  it buys nothing we would not run ourselves. Full rationale and the named
  fallback: `specs/tracking/engine-boundary.md` → Q2.
- **JSC on Windows is a licensing and build problem, not just a build problem.**
  JavaScriptCore is LGPL-2.1. On iOS/macOS this is fine: dynamically link the
  system `JavaScriptCore.framework`. On Windows there is no system JSC, so
  supporting it means shipping a WebKit build *and* honouring LGPL
  dynamic-linking obligations inside a proprietary game engine. Since JSC's
  purpose here is boundary validation, **macOS alone is sufficient.** Windows+JSC
  joins Android+JSC as explicitly unsupported.
- **Hermes was never considered, and should have been.** This plan cites React
  Native shipping Hermes on iOS as precedent for bundling an engine, without
  noticing Hermes is itself a candidate: no JIT (so the same iOS/Android parity
  argument that selected QuickJS applies), AOT bytecode for fast startup, and the
  most battle-tested mobile deployment story of any engine here. Rejected because
  its native class-binding API is markedly less ergonomic than QuickJS's and
  Static Hermes is still churning — but it is rejected *for reasons*, not by
  omission. Revisit if QuickJS startup or throughput disappoints on device.
- **Caveat to monitor:** Apple tightened App Store guideline 4.7 in November 2025
  regarding custom JS engines, but in the context of third-party "mini app"
  hosting (remotely-delivered executable content), not ordinary engine-bundled
  scripting. This should not block bundling QuickJS for internal game logic
  (precedent: React Native ships Hermes on iOS), but **verify against the current
  App Store Review Guidelines text before shipping.**

### 3.3 Why `webgpu.h`, not a JS-engine-specific WebGPU implementation

The alternative of binding QuickJS/JSC directly to `wgpu-core` (as Deno's
`deno_webgpu` does) was rejected because it would hard-couple the binding to
wgpu-native/wgpu-core specifically and lose the ability to swap in yawgpu or
Dawn. Going through `webgpu.h` is what makes this project's output usable with
any of the three implementations, consistent with `webgpu-native-cts`.

### 3.4 Scripts are trusted

This is first-party game logic, not a browser sandbox. Do not spend effort
hardening against adversarial JS. Do spend it on catching honest mistakes with
clear, early errors.

---

## 4. Implementation Phases

Rev 1's phase *ordering* was right and is preserved: hand-write a slice →
validate the boundary with a second engine → generate. Generating ~40 interfaces
against an unvalidated boundary is the expensive failure mode. Only the
*contents* of Phase 0 and Phase 1 change.

### Phase 0 — Foundation spikes (de-risking, no full binding yet)

Answer the unknowns the rest of the plan depends on, cheaply. **Reordered by
risk:** the JSC capability spike now comes first, because it is the one that can
invalidate §2.4.

1. **JSC ArrayBuffer detach capability.** ✅ **ANSWERED 2026-07-09** —
   `specs/tracking/engine-boundary.md` → Q1. JSC's public C API exposes **no**
   detach; the JS-level `ArrayBuffer.prototype.transfer()` is dependable on
   engine-owned buffers but not on external (`…WithBytesNoCopy`) ones, and the
   `bytesDeallocator` never fires. Handing script a zero-copy view over GPU
   memory would therefore leave a dangling pointer after `unmap()` — memory
   unsafety, not a conformance footnote. **Decision:** `JsEngine` carries a
   `MappedRangeStrategy` capability (`ZeroCopyDetach` for QuickJS,
   `CopyInCopyOut` for JSC); `core/` implements both once, generic over `E`.
   Copying at `unmap()` is spec-conformant, so this is a perf cost on the Tier 2
   engine, not a behavioural deviation. **The boundary survived** — the fix was
   additive (a capability + a trait method), which is exactly the outcome §2.4
   predicted a correct boundary would produce. **Q1a closed 2026-07-09**: both
   arms now have running spikes. QuickJS `ZeroCopyDetach` is proven
   (`spikes/quickjs-detach/`), and along the way the source showed that
   `free_func` fires at *detach* and then again at finalize with a **null**
   pointer (E7/E8), and that `JS_DetachArrayBuffer` cannot fail loudly (E9) —
   just as JSC's `transfer()` cannot (Q1b/E5). **Both engines can silently fail
   to detach**, so post-detach verification belongs in `core/` once.
2. Set up the `bindgen`-generated Rust FFI crate from `webgpu.h`; verify a
   trivial program (`wgpuCreateInstance` → `wgpuInstanceRelease`) links and runs
   against yawgpu and wgpu-native. ✅ **DONE 2026-07-09.** `ffi/` generates from
   the pinned canonical header; the backend is a Cargo feature; the library
   directory comes from an env var or `pkg-config`, never a path in a committed
   file. Both backends pass headless. Two packaging defects surfaced —
   `backend-deltas.md` → D5 (absolute `install_name`, so the *artifact* embeds a
   developer path even though the source does not) and D6 (yawgpu's Tint shim is
   not colocated). Dawn deferred to Phase 7 CI; it has no local build.
3. Confirm yawgpu's `webgpu.h` surface matches (or document deltas from) the
   canonical `webgpu-headers` version. ✅ **ANSWERED 2026-07-09** —
   `specs/tracking/backend-deltas.md` → D1–D3. The *header* surfaces match (202
   functions), bar one enumerator. **The libraries do not.** yawgpu exports only
   **178 of the 202** canonical functions — the whole `SetLabel` family,
   `wgpuGetProcAddress`, `wgpuDeviceGetAdapterInfo` and others are unimplemented
   in its source (D2). wgpu-native exports a complete superset (D3). Reading a
   vendored header tells you what a backend *intends* to implement, not what it
   *does*: compare shipped libraries. Blocks nothing in Phase 0, but the
   `SetLabel` gap blocks Phase 4, because WebIDL gives every `GPUObjectBase` a
   writable `label`. Fix upstream in yawgpu, per `CLAUDE.md`.
4. **Prototype the event-loop pump (§2.7)** end-to-end with no GPU. ✅ **DONE
   2026-07-09** — `specs/tracking/event-loop.md`. The contract holds, and yawgpu
   genuinely defers an `AllowProcessEvents` callback rather than firing it
   inline, which was the load-bearing unknown. The sequence is now an executable
   invariant: after `ProcessEvents` the Promise is resolved, a job is pending,
   and `globalThis.ran` is **still false**; only draining
   `JS_ExecutePendingJob` runs the continuation. A regression test ticks
   `ProcessEvents` eight times without draining and shows an `await` continuation
   never runs. The callback fires on the pumping thread, so `AllowProcessEvents`
   removes cross-thread signalling entirely. Residual: two review findings
   (the crux test asserts our own flag rather than `JS_PromiseState`; the
   callback leaks its `WGPUAdapter`) — handoff issued.
5. Prototype the release queue (§2.5) standalone: allocate one buffer handle,
   trigger release from a QuickJS finalizer and separately from a JSC finalizer,
   confirm the queue delivers exactly one `wgpuBufferRelease` from the designated
   thread in both cases.
6. **Source and pin `webgpu.idl`** (§2.3) alongside `webgpu.h`; establish how the
   two versions are kept in sync.

**Exit criteria:** all six answered; the pump and release-queue prototypes pass
under both engines; the JSC detach question has a written answer and a decision.

~~Rev 1's Phase 0 item "determine yawgpu's async callback threading model"~~ —
resolved by reading the header (§2.6). Removed.

### Phase 1 — Core abstractions + the import slice

1. Implement `trait JsEngine`, `ClassSpec<E>`, `PropertySpec`, `MethodSpec`,
   `FinalizerFn` (§2.4).
2. Implement the per-call bump arena for descriptor temporaries (§2.4).
3. Promote the release queue from Phase 0 into a reusable `core/` module.
4. **Vertical slice target (changed from Rev 1):** `wrap_device(WGPUDevice)` →
   `GPUDevice.createBuffer()` → `buffer.destroy()`, expressed purely against
   `JsEngine`. **Synchronous, no Promises.** This isolates the §2.4 bet from the
   §2.6/§2.7 machinery.
5. Async slice (`requestAdapter` → `requestDevice` → `mapAsync`) lands in Phase 2
   with the QuickJS Promise implementation, not here.

### Phase 2 — QuickJS adapter (primary)

1. `impl JsEngine for QuickJs` — value ops, `ClassSpec` → `JSClassDef`.
2. Promise via `JS_NewPromiseCapability`; microtask pump via `JS_ExecutePendingJob`.
3. Expose `tick()` (§2.7) as public API; wire the async vertical slice through it.
4. Build integration: NDK (Android), clang cross-compile (iOS, interpreter-only),
   Windows/macOS for dev/test.
5. Validate on Android emulator + physical device, iOS simulator + physical
   device, Windows, macOS. Confirm behavioral parity across all four — this is
   the point of choosing QuickJS; verify it actually holds.

**Exit criteria:** sync + async slices pass on all four platforms, consistently.

### Phase 3 — JavaScriptCore adapter (boundary validation)

1. `impl JsEngine for Jsc` — `ClassSpec` → `JSClassDefinition`.
2. Promise via `JSObjectMakeDeferredPromise`; microtask pump via the
   version-appropriate equivalent.
3. **macOS only** (link the system framework). iOS follows for free but is not
   the point. Android and Windows are explicitly unsupported (§3.2).
4. Wire and validate the same slices as Phase 2.
5. **Design check (the real deliverable):** wiring JSC must require **zero
   changes to `core/`'s logic** — only additive `JsEngine` trait methods. Any
   non-trivial core churn means §2.4's boundary was drawn wrong. **Stop and
   revisit before Phase 4.** Absorbing the churn here means generating ~40
   interfaces against a broken abstraction.

**Exit criteria:** slices pass under JSC on macOS; `core/` logic unchanged;
the Phase 0 detach finding is reflected in a documented capability model.

### Phase 4 — Codegen from WebIDL ⋈ `webgpu.yml`

1. Build the generator (§2.3): WebIDL supplies JS shape/defaults/coercion,
   `webgpu.yml` supplies the C ABI to lower onto.
2. Regenerate the Phase 1–3 hand-written slice and confirm the output matches
   behaviorally. **Then delete the hand-written version** — it is a bootstrap
   artifact, not a permanent parallel path.
3. Expand coverage incrementally: textures, samplers, bind groups, pipelines,
   shader modules, command encoders, render/compute pass encoders — roughly in
   the order a minimal renderer needs them.

### Phase 5 — Native surface / windowing integration

1. Design and implement `createSurfaceFromNativeHandle()`-equivalent per platform
   (HWND, NSView/CAMetalLayer, ANativeWindow, UIView, X11/Wayland).
2. Minimal demo: render a triangle to an actual native window (not offscreen),
   through the JS binding, on at least one desktop and one mobile platform,
   backend-swappable per §2.2/§3.3.

### Phase 6 — Robustness pass

1. Error scopes (`pushErrorScope`/`popErrorScope`) and `uncapturederror` through
   both engines, including the no-configurable-mode callbacks of §2.6.
2. Device-lost / adapter-lost lifecycle edge cases.
3. Parent-keeps-child-alive lifetime rules (a `GPUBuffer` must not outlive its
   `GPUDevice`). Note the *ordering* requirement was already a design input to
   §2.5; this phase verifies it, it does not discover it.

### Phase 7 — CI and documentation

1. CI matrix: {yawgpu, wgpu-native, Dawn} × {QuickJS, JavaScriptCore} × {iOS,
   Android, Windows, macOS}, pruned where documented as unsupported (JSC ×
   {Android, Windows}).
2. Example programs per platform, including one showing the host `tick()` contract.
3. Document the codegen workflow for contributors adding IDL coverage, and the
   `destroy()`-not-GC guidance of §2.5.

### Phase 8 — Aspirational: run the upstream WebGPU CTS

Not near-term scope. See §5.4 for why it is the end state worth steering toward.

---

## 5. Cross-cutting concerns

### 5.1 Repository layout

```
webgpu-native-js/
  core/                    # generic over E: JsEngine — the bulk of the code
    engine.rs              #   trait JsEngine, ClassSpec<E>, Deferred<E>
    convert/               #   descriptor conversion, written once
    arena.rs               #   per-call bump allocator for FFI temporaries
    release_queue.rs       #   thread-safe release queue (§2.5)
    import.rs              #   wrap_device / wrap_* handle adoption (§2.8)
  ffi/
    webgpu_bindgen/        # bindgen output + build.rs backend selection
    conv.rs                # C↔Rust, macro-driven (wgpu-native style)
  codegen/
    idl/                   # WebIDL reader
    yml/                   # webgpu.yml reader
    join.rs                # WebIDL ⋈ yml -> ClassSpec emitters
  adapters/
    quickjs/               # impl JsEngine for QuickJs
    javascriptcore/        # impl JsEngine for Jsc
  examples/
    triangle-native-surface/
  Cargo.toml               # features: quickjs, jsc; yawgpu, wgpu-native, dawn
```

A Cargo **workspace** with separate crates, not one crate with modules — the
feature-gating across engine × backend is cleaner that way.

### 5.2 Threading model

- The JS engine instance is **single-threaded** and thread-confined.
- `webgpu.h` handles may be shared with the host's render thread; the binding
  must not assume it owns them.
- The release queue's drain thread is the GPU-owning thread (§6: whose?).
- No `AllowSpontaneous` on JS-facing paths (§2.6).

### 5.3 Mobile-specific

- **Binary size** matters; it is a selection criterion, not an afterthought.
- **Startup time**: QuickJS bytecode precompilation (`qjsc`-equivalent) should be
  evaluated during Phase 2, not bolted on later.
- **Memory pressure**: see the `destroy()`-not-GC rule in §2.5.

### 5.4 Testing the binding layer

Per §1.2, `webgpu-native-cts` cannot validate this project. The binding layer's
natural oracle is the **upstream WebGPU CTS itself**, which is written in
TypeScript and therefore runs *inside* the engine under test. This is exactly
what `dawn.node` does (`src/dawn/node/cts.cjs`, `tools/src/cmd/run-cts`) — the
same trick, one engine down.

Running it under QuickJS needs a module loader and a set of Web shims. That is a
real lift and explicitly **not** Phase 0–4 scope. But it is the end state worth
steering toward, and it is precisely why §2.3 matters: **only an IDL-faithful
binding can pass an IDL-derived suite.** A binding generated from `webgpu.yml`
alone would fail the CTS on coercion and default-value cases that the C ABI
never expresses.

Until Phase 8, the test layers are:

1. Inline `#[cfg(test)]` unit tests on every public fn (the primary coverage).
2. Per-conversion unit tests in `core/`, generic over a **mock `JsEngine`** — no
   real engine, no GPU.
3. One `.js` conformance script executed under **both** engines with identical
   expected output.
4. Headless-first throughout: every test passes with no GPU and no window
   (yawgpu's Noop backend, or a compute/offscreen path).

---

## 6. Open questions

Genuinely undecided. Answer with evidence. Do not let them harden into
assumptions by being restated confidently in a later document.

1. ~~**Does JSC's public C API expose ArrayBuffer detach?**~~ **CLOSED
   2026-07-09: no.** Resolved into the `MappedRangeStrategy` capability; see
   `specs/tracking/engine-boundary.md` → Q1 and Phase 0.1 above. Residual **Q1a**
   (QuickJS detach on an external buffer) is open and handed off.
2. **Does the host game engine own the GPU-release thread, or does this project
   spin up its own?** (Affects §2.5, §5.2.) Now the highest-priority open
   question, since Q1 is closed.
3. ~~**Which QuickJS fork, and `rquickjs` vs. raw `bindgen`?**~~ **CLOSED
   2026-07-09: quickjs-ng, pinned submodule, raw `bindgen`.** (§3.2;
   `specs/tracking/engine-boundary.md` → Q2.) Residual: **which revision is
   pinned**, recorded at submodule-add time alongside the `webgpu-headers` pin.
4. ~~**Where does `webgpu.idl` come from?**~~ **CLOSED 2026-07-09:** the W3C
   [gpuweb](https://github.com/gpuweb/gpuweb) repository, not `webgpu-headers`.
   Dawn's `src/dawn/node/BUILD.gn` feeds `third_party/gpuweb/webgpu.idl` to
   `idlgen` alongside `interop/Browser.idl` and `interop/DawnExtensions.idl`.
   See `specs/reference/dependencies.md`. **Residual, and still open: how the
   `webgpu.idl` and `webgpu.h` pins are kept consistent**, since they version
   independently upstream. Default policy adopted: pin what Dawn pins, and
   record any divergence with its reason.
5. **Full WebIDL coverage vs. a deliberately trimmed engine-oriented subset**
   (Web-source-compatibility vs. minimal-surface tradeoff). Revisit after Phase
   4's first codegen pass shows the real effort delta. Note that Phase 8 (running
   the upstream CTS) pushes toward full coverage.
6. **Multithreaded script execution** (multiple `JSContextGroup`/`JSRuntime`) is
   out of scope; §5.2 assumes one engine instance per game instance. Revisit only
   if a concrete requirement appears.
7. **Current App Store Review Guidelines text** re: bundled custom JS engines
   should be re-checked immediately before any iOS release (§3.2).

---

## 7. Revision history

### Rev 2 (2026-07-09) — corrections applied

Each item below was verified against a primary source, not against recollection:
the canonical [`webgpu-headers`](https://github.com/webgpu-native/webgpu-headers)
`webgpu.h`; the [yawgpu](https://github.com/infosia/yawgpu) implementation
(`yawgpu/src/ffi/instance.rs` within that repository); or
[Dawn](https://dawn.googlesource.com/dawn)'s `src/dawn/node/` tree. They are
recorded here so Rev 1's claims are not reintroduced from memory.

| # | Rev 1 claim | Status | Where |
|---|---|---|---|
| 1 | "Blocking unknown: which thread do yawgpu's callbacks fire on" | **Wrong — not an unknown.** `WGPUCallbackMode` makes it a caller-chosen contract; yawgpu implements all three modes plus `ProcessEvents`/`WaitAny`. | §2.6 |
| 2 | `PromiseBridge::resolve(value: JsValueHandle)`; one `NativeClassSpec` "consumed identically by every engine" | **Wrong — cannot be implemented.** No engine-agnostic value handle exists. Replaced with `trait JsEngine` + associated types. | §2.4 |
| 3 | "Generate `NativeClassSpec` from `webgpu.yml`" | **Wrong input.** `webgpu.yml` is the C ABI; it carries no defaults, string enums, flag namespaces, `Promise` types, or coercion rules. `dawn.node` generates from **WebIDL**. Corrected to a WebIDL ⋈ yml join. | §2.3 |
| 4 | Release queue justified by JSC's any-thread finalizers; "the highest-risk piece of the whole design" | **Right conclusion, wrong reason, wrong risk ranking.** Real reason: `webgpu.h` guarantees no thread-safety for `wgpuXxxRelease` at all. Risk: it is an ordinary MPSC queue; §2.4 is the risky part. | §2.5 |
| 5 | Phase 1 slice = `requestAdapter` → `requestDevice` → `createBuffer` → `mapAsync` | **Browser-shaped, not host-shaped.** The host already owns the device. Primary entry is handle adoption; the first slice is now synchronous. | §2.8, Phase 1 |
| 6 | (absent) | **Omission: the host event-loop contract.** Resolving a Promise does not run `.then()`. The host must pump `ProcessEvents` *and* the microtask queue each frame. | §2.7 |
| 7 | (absent) | **Omission: per-call arena.** `WGPUStringView` is non-null-terminated; `count+pointer` arrays must outlive the call. | §2.4 |
| 8 | "QuickJS" | **Under-specified.** Bellard vs. quickjs-ng is an unmade decision. | §3.2, §6.3 |
| 9 | "JSC secondary target… Windows/macOS" | **Licensing omission.** JSC is LGPL-2.1; no system build on Windows. macOS alone suffices for boundary validation. Windows+JSC now unsupported. | §3.2 |
| 10 | Cites React Native/Hermes as precedent | **Omission: Hermes as a candidate.** Now an explicit rejected alternative with reasons. | §3.2 |
| 11 | "This project is not for conformance testing" | **Half wrong.** Backend conformance is `webgpu-native-cts`'s. *Binding* conformance is this project's, and the upstream TS CTS is its natural oracle. | §1.2, §5.4 |
| 12 | Phase 6: parent-keeps-child-alive ordering | **Mis-phased.** Ordering is a design input to the release queue, not a late robustness discovery. | §2.5, Phase 6 |

Unchanged from Rev 1 and explicitly endorsed: the layered design (§2.1), the
`webgpu.h`-not-Rust-internals rule (§2.2), the N-API rejection (§3.1), the
QuickJS-primary decision (§3.2), the `webgpu.h`-not-`wgpu-core` decision (§3.3),
and the overall phase ordering (hand-write → validate boundary with a second
engine → generate).

### Rev 1 — original draft

Derived from a design discussion between the project owner and an assistant.
Written so another engineer or agent could pick it up without the full discussion
history. Superseded in the twelve respects above.
