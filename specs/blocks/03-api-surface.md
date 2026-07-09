# Block 03 — filling the API surface

Phase 2, part 2. `GPUQueue`, shader modules, bind groups, pipelines, command
encoders, compute passes — enough of the IDL to answer one question.

Rules are numbered **B1–B18**. Blocks 01 (R1–R25) and 02 (A1–A23) still bind.

Every claim below about `webgpu.h` was checked against the pinned file while
writing. Reopen it; do not restate from memory.

---

## 1. The question this block exists to answer

The project owner's directive (2026-07-09): *fill out the API on Windows and
macOS, to see whether the design is feasible.* Mobile bring-up is deferred; there
is no simulator and no device.

So this block is not about breadth for its own sake. It is about the one thing
breadth reveals and a two-object slice cannot:

> **Does descriptor conversion stay written-once when the descriptors get hard?**

Blocks 01 and 02 converted flat structs of scalars and one string. The interfaces
below bring **struct chaining**, **`count + pointer` arrays of dictionaries**,
**string enums**, **nullable versus non-null strings**, and **object handles
stored inside descriptors**. That is where a binding either scales or starts
sprouting per-engine special cases.

If it scales, Phase 4's codegen has a target worth generating. If it does not,
better to learn it across ten interfaces than across forty.

**A per-descriptor conversion function in `core/` is expected and fine** — that
is exactly what Phase 4 will generate. What must not appear is a conversion that
differs per engine, or a `core/` line that exists because of QuickJS.

---

## 2. Scope

**In.**
`GPUDevice.queue` / `wgpuDeviceGetQueue`; `GPUQueue.writeBuffer`, `.submit`,
`.onSubmittedWorkDone`; `GPUDevice.createShaderModule` (WGSL);
`createBindGroupLayout`, `createPipelineLayout`, `createBindGroup`;
`createComputePipeline`; `createCommandEncoder`;
`GPUCommandEncoder.beginComputePass`, `.copyBufferToBuffer`, `.finish`;
`GPUComputePassEncoder.setPipeline`, `.setBindGroup`, `.dispatchWorkgroups`, `.end`;
`GPUCommandBuffer`.

**Out.** Render pipelines, textures, samplers, surfaces, query sets, error scopes,
`uncapturederror`, device-lost, codegen, the JSC adapter, and mobile.

**Windows.** In scope as a *build* target. There is no Windows host in this
environment, so `cargo check --target x86_64-pc-windows-msvc` is the most that
can be claimed here, and only if the toolchain is available offline. **Say so
plainly rather than implying a Windows test run happened.**

---

## 3. What the headless backend can and cannot show

Measured, not assumed. yawgpu's Noop HAL:

- **executes buffer copies eagerly** — `submit_copies` performs `HalCopy::Buffer`
  immediately, "so that subsequent map-reads on the destination buffer observe
  the written bytes (mirrors the real-GPU semantics)".
- **does not execute compute.** There is no dispatch implementation.

**B1 — the end-to-end test is a copy round-trip, not a compute round-trip.**
`writeBuffer` → `copyBufferToBuffer` → `mapAsync` → read back, and assert the
bytes. That is observable headless and it exercises the whole chain: queue,
encoder, command buffer, submit, and the Phase 2 mapping path.

**B2 — compute is tested for *creation*, not for *effect*.** `createShaderModule`
(WGSL through Tint), `createBindGroupLayout`, `createPipelineLayout`,
`createBindGroup`, `createComputePipeline`, `beginComputePass` / `setPipeline` /
`setBindGroup` / `dispatchWorkgroups` / `end` / `finish` / `submit` must all
succeed and raise no validation error. **Do not assert dispatch results
headless**, and do not write a test whose passing implies the shader ran.

A real-GPU compute assertion is welcome but **gated and never required for CI**
(`CLAUDE.md` principle 7). If it is added, it must be `#[ignore]`d or
feature-gated, and its absence must not weaken the headless suite.

---

## 4. Rules

### The new conversion machinery

**B3 — struct chaining.** `WGPUChainedStruct { struct WGPUChainedStruct *next;
WGPUSType sType; }`. Descriptors carry `WGPUChainedStruct *nextInChain` — a
**mutable** pointer. WGSL source arrives as
`WGPUShaderSourceWGSL { WGPUChainedStruct chain; WGPUStringView code; }` with
`chain.sType = WGPUSType_ShaderSourceWGSL`.

- The chained node is **arena-allocated** and must outlive the FFI call (block 01
  → R10). It must not outlive the JS values it was built from.
- `sType` is always set. A descriptor we build with a chain we did not populate
  is a use of uninitialised `sType`, and the backend will branch on it.
- An **unknown or unsupported chain** from script is an error, not a silent drop.

**B4 — `NullableInputString` and `NonNullInputString` are different, and both
appear.** From `webgpu-headers/doc/articles/Strings.md`:

| Kind | Null means |
|---|---|
| **Non-Null Input String** — e.g. `label` | the **empty string** |
| **Nullable Input String** — e.g. `WGPUComputeState.entryPoint` | **absent**; run the defaulting algorithm |

`entryPoint` absent means "use the module's only entry point". `entryPoint = ""`
is a *named* entry point that does not exist. Encode
`{NULL, WGPU_STRLEN}` for absent, and a real view for present. **Getting these
backwards changes behaviour silently**, which is why block 01 → R9 forbids
branching on `data.is_null()` alone.

**B5 — `count + pointer` arrays.** `entryCount`/`entries`,
`bindGroupLayoutCount`/`bindGroupLayouts`, `commandCount`/`commands`. All counts
are **`size_t`**.

- The array and every dictionary inside it live in the arena.
- WebIDL `sequence<T>` conversion: reject a non-iterable, and reject a member
  that fails its own conversion — do not skip it.
- An empty sequence is `count == 0`; the pointer may then be null. Do not pass a
  dangling non-null pointer, and do not pass `count > 0` with a null pointer —
  `webgpu.h` forbids that shape for strings and the same discipline applies here.

**B6 — string enums.** `GPUBufferBindingType`, `GPUShaderStage` flags,
`GPUComputePassTimestampWrites`… WebIDL enum conversion: an unrecognised string
is a **`TypeError`**, thrown synchronously. An absent member with a default takes
the default. Do not accept a number where the IDL says enum.

**B7 — mixed integer widths in one call, again.**
`wgpuQueueWriteBuffer(queue, buffer, uint64_t bufferOffset, void const *data, size_t size)`
takes a **`uint64_t`** offset and a **`size_t`** size. `wgpuQueueSubmit` takes a
`size_t` count. `dispatchWorkgroups` takes three `uint32_t`.

Block 02 → A21 applies per *argument*, not per function: reject before narrowing,
reject before widening, and make the guard's behaviour identical on a 64-bit and
a 32-bit host. **Test `size = 2^32` on this 64-bit host**, where the `size_t`
guard must still fire, because no 32-bit target will be built until block 05.

### Lifetime

**B8 — a wrapper takes a native reference on *every* handle it stores, not only
on its parent.** Block 01 → R5 said "child takes a native `AddRef` on its
parent". That was the two-object case. The general rule:

> `GPUBindGroup` stores buffers. `GPUComputePipeline` stores a shader module and
> a pipeline layout. `GPUCommandBuffer` is stored in a submit array. Every
> `WGPUxxx` handle a wrapper keeps alive is `AddRef`'d when stored and released
> with the wrapper, through the queue.

The bind group's buffers are **not** its parents — the graph is not a tree.
Finalizer order is unspecified in both engines (`release-queue.md` → Q2/R5), so
the native reference is what keeps the object alive, exactly as before. The
release queue stays a plain FIFO and never sorts.

**B9 — `wgpuQueueSubmit` does not consume the command buffers.** The caller still
owns each `WGPUCommandBuffer` and must release it. A `GPUCommandBuffer` wrapper
released by the queue after submit is a use-after-free if the backend still holds
it, and a leak if nobody does. Decide by reading the header and yawgpu, and write
down which.

**B10 — encoders are single-use and not thread-safe.**
`doc/articles/Multithreading.md` names `WGPUCommandEncoder`,
`WGPUComputePassEncoder`, `WGPURenderPassEncoder` and `WGPURenderBundleEncoder`
as the API's **only** non-thread-safe objects. We are single-threaded, so this
costs nothing — but `finish()` invalidates the encoder, and `end()` invalidates
the pass. Using either afterwards is a validation error, not a crash. Track the
state in the wrapper, as `destroy()` does (block 01 → R14).

### Boundary

**B11 — no conversion may differ per engine.** Each descriptor gets one
conversion function in `core/`, generic over `E`. Per-descriptor functions are
expected; per-engine ones are the failure.

**B12 — every new engine capability is an *addition* to `JsEngine`.** Block 02 →
A18. Sequences will want an iteration primitive (`length` + indexed get, or an
iterator); enums will want string comparison. Add methods; do not reshape `core/`.
**If `core/` must change because QuickJS is refcounted, or because of anything
JSC would not need — stop and report.** That is the headline, and it is worth
more than the slice.

**B13 — the adapter names no class and no member** (R24), holds no lock across a
call into `core/` (R25), and every `extern "C"` callback catches unwinds and
calls no `webgpu.h` function (A8).

**B14 — `#[allow]` on a correctness or soundness lint is a finding** unless a
comment says why the lint is wrong here. Phase 1 shipped one on a `pub fn` taking
a raw pointer, the gate stayed green, and three reviewers walked past it.

### Errors

**B15 — validation failures have nowhere to go yet, and that must be visible.**
WebGPU routes them to the current error scope; error scopes are Phase 6. Until
then a failed `createXxx` surfaces as a synchronous engine exception, and **the
divergence is recorded** in `specs/tracking/engine-boundary.md` alongside block
01 → R13's. Do not quietly diverge, and do not invent an error-scope subset here.

**B16 — a null handle from any `createXxx` is an error, never a wrapper around
null.** `wgpuDeviceCreateBuffer` is `WGPU_NULLABLE`; check each of the new
constructors in the header and handle whichever are.

---

## 5. Tests

**B17 — the mock carries the load, and it is the strictest engine** (R23, A20).
Every conversion rule B3–B7 gets a direct `core/` unit test against the mock, with
no engine, no backend and no GPU: chained WGSL source; an entries array of three
bind-group entries; an unrecognised enum string; an absent `entryPoint` versus an
empty one; `size = 2^32` on `writeBuffer`.

Where a sequence conversion can fail halfway, assert the arena is reset and no
handle was `AddRef`'d — a partially-converted descriptor must leak nothing.

**Real-engine tests (QuickJS + yawgpu Noop, headless):**

- **B1's copy round-trip**, asserting the bytes.
- Creation of shader module, bind group layout, pipeline layout, bind group,
  compute pipeline, encoder, compute pass — no validation errors, all released
  through the queue.
- `finish()` twice, `end()` twice, and use-after-`finish()` — each an error, not a
  crash (B10).
- A bind group outliving the buffers' JS wrappers: drop every JS reference to the
  buffers, `tick()` to drain, and confirm the bind group is still usable. This is
  B8's whole point, and finalizer order will not save you.

**B18 — negative demonstrations, on the ordinary `cargo test` gate.** R19. Break
it, see it red, restore it, see it green, and report both. A guard whose red state
cannot be reproduced from the tree is on the honour system.

- B8: drop the native `AddRef` on a bind group's buffer; show use-after-free
  (poison the memory rather than relying on ASan if you can).
- B3: leave `sType` unset; show the descriptor is rejected or misinterpreted.
- B4: treat `entryPoint`'s null as the empty string; show a real behaviour change.
- B7: remove the `size_t` guard; show `size = 2^32` truncating silently.

---

## 6. Exit criteria

1. B1's copy round-trip passes headless under QuickJS against yawgpu.
2. Every conversion rule B3–B7 has a direct mock test, and every negative
   demonstration in B18 has been seen red.
3. `cargo test -p webgpu-native-js-core` still passes with **no engine, no
   backend feature, and the backend env var unset**.
4. **No conversion differs per engine; no `core/` line exists because of QuickJS.**
   Any exception is this block's headline finding.
5. Windows: `cargo check` for the MSVC target, or an explicit statement that no
   Windows toolchain was available. Do not imply a run that did not happen.
6. Full workspace gate green; clippy clean with no new `#[allow]`; Phase Review
   clean of CRITICAL and MAJOR.

## 7. Answers this block produced

- **B9 — `wgpuQueueSubmit` does not take ownership.** The header says nothing, and
  yawgpu's implementation neither `AddRef`s nor `Release`s the command buffers.
  The caller keeps each `WGPUCommandBuffer` and releases it through the queue like
  any other wrapper.

  > **Retraction.** This entry originally continued: *"A `GPUCommandBuffer` is
  > nonetheless consumed in WebGPU's sense — resubmitting it is a validation
  > error — which is a wrapper-state question, not a refcount one. See B10."*
  > That sentence reads as though single-use were handled. **It is not.**
  > `CommandBufferPayload` carries only a handle, no `consumed` flag, and
  > `queue_submit` marks nothing, so a script can submit the same
  > `GPUCommandBuffer` twice and reach `wgpuQueueSubmit` with the same handle.
  > B10's state tracking was implemented for the encoder and the pass and not for
  > the command buffer. I wrote the sentence; a reviewer caught that it described
  > work nobody had done. **B19** now requires it.

**B19 — `GPUCommandBuffer` is single-use.** `finish()` produces it; `submit()`
consumes it. A second `submit()` of the same command buffer is a validation error
raised by the binding, in the same way `finish()` twice and `end()` twice are
(B10). Track it in the wrapper. The handle is still released by the finalizer
through the queue — consumption and release are different, exactly as `destroy()`
and release are (block 01 → R14).

- **`getMappedRange()` is one JS method and two C functions** — see block 02 → A29.

- **`JsEngine` needed no sequence primitive — but the shortcut is not WebIDL.**
  `sequence_len` reads `.length`; `sequence_item` reads the stringified index. That
  is **array-like** access. WebIDL's `sequence<T>` conversion is **iterator-based**
  (`Symbol.iterator`), and the two accept different things:

  | Value | WebIDL | us |
  |---|---|---|
  | `{ length: 2, 0: a, 1: b }` | rejected | **accepted** |
  | `new Set([a, b])`, a generator | accepted | **rejected** |

  Both directions are wrong, and **a green suite cannot see either** — nothing
  constructs an array-like or a non-array iterable. It would surface only against
  the upstream TypeScript CTS, which this project names as the binding layer's
  natural oracle.

  **B20 — record the divergence, and pin it with a test.** Add tests asserting
  today's behaviour (array-like accepted, `Set` rejected) and mark them as
  documenting a **known deviation**, not a desired one. Then decide the primitive
  when Phase 4's codegen emits sequence conversions and JavaScriptCore has a vote
  on its shape — the same reasoning that deferred `associate_value` (block 02 →
  A27). A trait method is a permanent tax on every engine; choosing it with one
  engine in the tree is the error that produced P2-C3.

  The honest form of the original claim: *"a non-conforming `length`+index
  shortcut avoided adding a primitive,"* not *"no primitive was needed."*

- **`getMappedRange()` is one JS method and two C functions** — the writable
  pointer returns `NULL` on a read mapping. This block's copy round-trip found it.
  Recorded as block 02 → **A29**, because it belongs with the mapping rules.

- **Roughly 80–85% of a descriptor conversion is mechanical**, by the implementing
  agent's count. What a human had to decide, and WebIDL did not say: which chained
  `sType`s to accept and which to reject; that bind-group resources are
  buffer-only for now; nullable versus non-null string treatment (B4); dynamic
  offsets and pipeline constants deferred; and how a null handle from a
  `createXxx` maps to a synchronous error while error scopes do not exist (B15).

  **That list is Phase 4's real difficulty**, and it is short. Every item on it is
  a *policy* decision that a generator can be told once, not a per-descriptor
  judgement. That is the strongest evidence so far that codegen is tractable.

- **B8's use-after-free cannot be demonstrated against yawgpu**, and that is not a
  refutation of B8. `yawgpu-core/src/bind_group.rs` stores `buffer: Arc<Buffer>` —
  **yawgpu retains bind-group resources internally**, so removing our `AddRef`
  changes nothing there. `webgpu.h` guarantees no such thing, and this project
  links three backends. Same reasoning as `specs/tracking/backend-deltas.md`:
  never depend on what one implementation happens to do.

  The substituted guard, `b8_bind_group_addrefs_each_stored_buffer`, asserts *we
  call `AddRef` once per stored buffer*. It does **not** prove the backend needs
  it. The C ABI exposes no refcount introspection, so that is the strongest
  available check — the same limit `release-queue.md` records for its own
  exactly-once assertion. The test's doc comment says so.

- **`unsafe impl Send` is necessary, and was undocumented.** A payload must be
  `Send` because a JSC finalizer may run on any thread, and a `WGPUxxx` handle is
  a raw pointer. The justification is that handles are **moved** across threads
  and never **dereferenced** off the `tick()` thread — `webgpu.h` makes off-thread
  *use* undefined, and moving is not use. Phase 2 shipped four bare `unsafe impl
  Send` in `core/`; three reviewers and every gate walked past them. `CLAUDE.md`
  now requires a `// SAFETY:` comment on each, and forbids dodging the obligation
  by storing a handle as `usize`.

## 8. Open questions still open

- **Does `JsEngine` need a sequence/iteration primitive**, or can `core/` read
  `length` and index with the property accessor it already has? The second costs
  one engine call per element; the first costs an abstraction. Measure before
  choosing.
- **Does `wgpuQueueSubmit` take ownership of its command buffers?** Read the
  header and yawgpu, and write down the answer (B9).
- **How much of a descriptor conversion is mechanical enough for Phase 4 to
  generate?** Count what a human had to decide, per descriptor, that WebIDL did
  not say. That number is the codegen's real difficulty.
