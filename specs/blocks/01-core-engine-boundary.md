# Block 01 — `core/`: the engine boundary and the import slice

Phase 1. Public API and behaviour contract for `trait JsEngine`, the object
model built on it, and the first vertical slice.

This block establishes **the project's central design bet** (`CLAUDE.md`
invariant 1): descriptor conversion is written **once**, in `core/`, generic over
`E: JsEngine`, and monomorphized per engine. If that bet is wrong, every
conversion gets written twice and Phase 4's codegen doubles in size. Phase 3
(JavaScriptCore) is the exam; this block is the answer sheet.

Every claim below about `webgpu.h`, `quickjs.h`, or WebIDL was checked against
the pinned files in `third_party/` while writing this document. Per the Phase 0
review's closing rule, do not restate any of it from memory — reopen the file.

---

## 1. Scope

**In.** `trait JsEngine`; `ClassSpec<E>` and its property/method/finalizer specs;
a per-call bump arena; the release queue promoted from
`spikes/release-queue/`; a mock engine for `core/` unit tests; and the vertical
slice:

```js
// the host has already created the WGPUDevice; JS adopts it
const device = /* wrap_device(WGPUDevice) */;
const buf = device.createBuffer({ size: 256, usage: GPUBufferUsage.COPY_DST });
buf.label = "staging";
buf.destroy();
```

**Out.** Promises, `mapAsync`, `getMappedRange`/`unmap`, `MappedRangeStrategy`,
`requestAdapter`/`requestDevice`, error scopes, `uncapturederror`, the JSC
adapter, and codegen. The slice is **synchronous on purpose** — it isolates the
§2.4 bet from the §2.6/§2.7 pump machinery, which Phase 0 already proved
separately.

---

## 2. Public API

Sketch, not prescription. The rules in §3 are the contract; a different shape
that satisfies them is acceptable and should be argued for.

```rust
pub trait JsEngine: Sized {
    type Value: Copy;
    type Context<'a>: Copy;
    type Error;

    // value inspection / conversion
    fn get_property(cx: Self::Context<'_>, obj: Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn is_undefined(cx: Self::Context<'_>, v: Self::Value) -> bool;
    fn to_f64(cx: Self::Context<'_>, v: Self::Value) -> Result<f64, Self::Error>;
    fn to_bool(cx: Self::Context<'_>, v: Self::Value) -> bool;          // ToBoolean, infallible
    fn to_str<'a>(cx: Self::Context<'a>, v: Self::Value, arena: &'a Arena) -> Result<&'a str, Self::Error>;

    // object model
    fn register_class(cx: Self::Context<'_>, spec: &ClassSpec<Self>) -> Result<ClassId, Self::Error>;
    fn new_instance(cx: Self::Context<'_>, class: ClassId, payload: Box<dyn Any>) -> Result<Self::Value, Self::Error>;
    fn payload<'a>(cx: Self::Context<'a>, obj: Self::Value, class: ClassId) -> Option<&'a dyn Any>;

    // errors
    fn throw_type_error(cx: Self::Context<'_>, msg: &str) -> Self::Value;
}
```

`ClassSpec<E>` carries `name`, `properties: &[PropertySpec<E>]`,
`methods: &[MethodSpec<E>]`, and a `FinalizerFn`.

Slice surface:

```rust
pub fn wrap_device<E: JsEngine>(cx: E::Context<'_>, device: WGPUDevice) -> Result<E::Value, E::Error>;
```

JS-visible: `GPUDevice.createBuffer(descriptor) -> GPUBuffer`;
`GPUBuffer.destroy()`; `GPUBuffer.size`, `.usage`, `.label` (read/write).

---

## 3. Rules

### Boundary

**R1.** `core/` contains **zero** references to QuickJS or JSC types — only
`E: JsEngine`. Enforced by inspection and by the fact that `core/` must compile
with **only** the mock engine, no engine crate in its dependency graph.

**R2.** No `dyn` on the descriptor-conversion path. `E::Value` is `Copy` and
conversion functions are generic and monomorphized. `Box<dyn Any>` is permitted
for the *payload* an object carries, because it is touched once per finalizer,
not once per field.

**R3.** Adding an engine may add `JsEngine` methods or associated consts. It may
**not** change `core/`'s logic. Any pressure to do so is a boundary defect —
stop and report (`CLAUDE.md`, JSC-phase exit gate).

### Handles and lifetime

**R4 — adoption, not acquisition.** `wrap_device(dev)` calls `wgpuDeviceAddRef`.
The host keeps its own reference and remains free to release it. This is the
primary entry point (invariant 6); `requestAdapter` is out of scope.

**R5 — child wrappers take a native reference on their parent.**
`createBuffer` calls `wgpuDeviceAddRef` and stores the `WGPUDevice` in the
buffer's payload. The buffer's release request releases the buffer **and then**
drops that device reference.

This is the mechanism, and it is not negotiable: finalizer order is
**unspecified**. QuickJS is refcounted and orders child-first; JSC gives no
ordering during GC and, at context teardown, ran **parent first**. Measured —
`specs/tracking/release-queue.md` → Q2/R5.

**R6 — the release queue is a plain FIFO and never sorts.** Finalizers **only**
enqueue. No `webgpu.h` call may occur inside any finalizer, on any engine. The
`tick()` thread drains. (`release-queue.md` → Q1, Q3.)

**R7 — the parent slot must be traced.** If the wrapper also holds a JS-level
reference to its parent's wrapper (for a future `.parent`-style accessor), the
engine must trace it: QuickJS via `JSClassDef::gc_mark`, JSC by protecting it.
Note this is **not** a lifetime mechanism for native handles — R5 is. Conflating
them is the Rev 2 error.

### Conversion

**R8 — `GPUBufferDescriptor`, faithfully.** The WebIDL is
`{ required GPUSize64 size; required GPUBufferUsageFlags usage; boolean mappedAtCreation = false; }`
plus `GPUObjectDescriptorBase { USVString label = ""; }`.

| Member | WebIDL | C ABI | Rule |
|---|---|---|---|
| `size` | `required GPUSize64` (`[EnforceRange] unsigned long long`) | `uint64_t` | Missing → `TypeError`. Non-finite, non-integral, or outside `[0, 2^64-1]` → `TypeError`. |
| `usage` | `required GPUBufferUsageFlags` (`[EnforceRange] unsigned long`) | `WGPUBufferUsage` = `WGPUFlags` = **`uint64_t`** | Missing → `TypeError`. Outside `[0, 2^32-1]` → `TypeError`, *then* widen to 64-bit. The C type is 64-bit but the IDL type is 32-bit; do not let the C type widen the accepted range. |
| `mappedAtCreation` | `boolean = false` | `WGPUBool` | `ToBoolean`, infallible. Absent → `false`. Note `"false"` is `true` — that is IDL-correct, do not "fix" it. |
| `label` | `USVString = ""` | `WGPUStringView` | Absent → the empty string. |

**R9 — `WGPUStringView` is not a C string.** `{NULL, WGPU_STRLEN}` is the null
value; `{any, 0}` is the empty string; `{NULL, non_zero}` is forbidden. `label`
is documented as a **Non-Null Input String**
(`webgpu-headers/doc/articles/Strings.md`): "If the null value is passed, it is
treated as the empty string." So the empty case may be encoded either way — pick
one and test both encodings are accepted where we *read* string views.

Never `strlen` a view. Never assume NUL termination.

**R10 — the per-call arena.** Label bytes and any `count + pointer` array must
outlive the FFI call and must not outlive the JS values they were read from.
Allocate them in a bump allocator that is reset after each call. `E::to_str`
borrows from the arena, which is why it takes one.

**R11 — precision is a documented limit, not a silent truncation.** A JS `Number`
represents integers exactly only up to `2^53 - 1`. `[EnforceRange] unsigned long
long` accepts a much larger range, and values above `2^53` that are not exactly
representable have already lost information before we see them. Follow the IDL
(accept the integral value that arrives), and **record this in the tracking doc**
as an inherent limit of `Number`-typed `GPUSize64`. Do not invent a stricter
rule; do not pretend the limit does not exist.

### Behaviour

**R12 — `label` has no C getter.** Canonical `webgpu.h` exports
`wgpuBufferSetLabel` but **no `wgpuBufferGetLabel`**; verify against
`third_party/webgpu-headers/webgpu.h`. WebIDL exposes `label` as a read/write
attribute, so the **getter must read the wrapper's own copy**. The setter writes
the wrapper's copy *and* calls `wgpuBufferSetLabel`. (yawgpu implements the
setter as of `backend-deltas.md` → D2.)

**R13 — `createBuffer` may return NULL.** `wgpuDeviceCreateBuffer` is declared
`WGPU_NULLABLE`. A null result must surface as an error, never as a wrapper
around a null handle. Phase 1 raises an engine exception and **records the
deviation**: WebGPU's IDL says `createBuffer` returns an invalid `GPUBuffer` and
routes the failure to an error scope. Error scopes are Phase 6. Write the
deviation down in `specs/tracking/engine-boundary.md`; do not quietly diverge.

**R14 — `destroy()` is explicit, idempotent, and not release.** It calls
`wgpuBufferDestroy` (explicitly thread-safe per
`doc/articles/Multithreading.md`). Calling it twice is a no-op. Using the buffer
afterwards is a validation error, not a crash. The handle is still **released**
later, by the finalizer, through the queue. `destroy()` frees GPU memory;
release frees the handle. They are different, and under JSC `destroy()` is the
**only bounded path** (`CLAUDE.md` principle 7 — `JSGarbageCollect` does not run
finalizers).

**R15 — errors.** Conversion failures throw `TypeError` synchronously, per
WebIDL. Nothing in this slice routes to an error sink; that arrives with error
scopes. No `unwrap`/`expect`/`panic!` in `core/`. Every `extern "C"` callback
catches unwinds.

---

## 4. Tests

**R16 — `core/` is tested against a mock engine, with no GPU and no engine.**
The mock implements `JsEngine` over a plain Rust value tree. Every rule R8–R15
gets at least one direct unit test there. This is what proves `core/` is
engine-agnostic: if it needs QuickJS to be tested, it is not.

**R17 — the QuickJS adapter is tested against the real slice.** One `.js` script
runs `wrap_device` → `createBuffer` → `label` round-trip → `destroy`, headless,
against yawgpu's Noop backend. Dual-engine comes in Phase 3; this block ships one
engine.

**R18 — anti-patterns barred**, carried forward from the Phase 0 review
(`phase-reviews.md` → deferred MINORs). None of these may appear in `core/` or the
adapter:

- a process-global class id shared across engine instances;
- JS handles or native handles smuggled through a `static` as `usize`;
- `&mut` derived from an opaque/userdata pointer that aliases a live `Box`/`Arc`;
- a raw pointer into a `Box` handed to a C callback whose lifetime the callback
  does not control (this was Phase 0's CRITICAL).

**R19 — a regression test must be seen to fail.** For any rule whose violation is
a memory error, the test that guards it must be demonstrated failing against a
deliberately broken version before it is trusted. Phase 0's UAF regression test
was written *after* the bug and would have been worthless otherwise.

---

## 5. Exit criteria

1. `wrap_device` → `createBuffer` → `label` set/get → `destroy` runs headless
   under QuickJS against yawgpu.
2. Every rule R1–R15 exercised by at least one test; R8's table by one test per
   row, including the rejection cases.
3. `core/` compiles and its unit tests pass with **only** the mock engine in the
   dependency graph.
4. Release queue promoted into `core/`, still FIFO, still drained on the `tick()`
   thread, finalizers still calling no `webgpu.h`.
5. Full workspace gate green: `cargo test --workspace --features
   backend-yawgpu`, `cargo clippy --workspace --all-targets -- -D warnings`.
6. Phase Review clean of CRITICAL and MAJOR.

## 6. Open questions this block will answer

- Does `E::Context<'a>` need a lifetime, or does an owning `Context` handle work
  for both engines? JSC needs a `JSContextRef` per operation; QuickJS needs a
  `*mut JSContext`. Both are `Copy` pointers, so the GAT may be unnecessary
  ceremony — decide with the mock plus QuickJS, revisit in Phase 3.
- Is `Box<dyn Any>` the right payload type, or should `ClassSpec` be generic over
  the payload? `dyn Any` costs a downcast per finalizer, which is cheap; a second
  type parameter costs ergonomics everywhere. Prefer `dyn Any` until measured.
- Where does the `tick()` API live once `core/` owns the queue — `core/`, or a
  host-facing crate? Deferred to Phase 2, when the pump moves out of its spike.
