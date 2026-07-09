# Block 02 — `tick()`, Promises, and buffer mapping

Phase 2, part 1. The public host contract, the Promise bridge, and
`mapAsync`/`getMappedRange`/`unmap`.

Rules in this block are numbered **A1–A20** to keep them distinct from block 01's
R-rules, which all still apply.

Every claim below about `webgpu.h` or `quickjs.h` was checked against the pinned
files in `third_party/` while writing. Reopen them; do not restate from memory.

---

## 1. Why this block is the real exam

Block 01 proved that *descriptor conversion* can be written once. It did not
prove the boundary, because the only engine primitives it needed were property
reads and object creation. Phase 2 adds four capabilities where the two engines
genuinely differ:

| Capability | QuickJS | JavaScriptCore |
|---|---|---|
| Promise creation | `JS_NewPromiseCapability`, two owned resolving functions | `JSObjectMakeDeferredPromise` |
| Microtask pump | `JS_ExecutePendingJob` returns `>0` / `0` / `<0` | version-appropriate equivalent |
| Value ownership | refcounted; owned values must be freed | GC-traced; nothing to free |
| Mapped range | `JS_DetachArrayBuffer` works on external memory | **cannot detach external memory at all** |

That last row is the one that matters. Phase 0 measured it
(`engine-boundary.md` → Q1, Q1b): JSC's public C API has no ArrayBuffer detach,
and taking a C bytes pointer *silently and permanently* disables `transfer()`.
The two engines therefore need **different mapping strategies**, and `core/` must
implement both, once, behind a capability.

**If Phase 2 forces a change to `core/`'s logic rather than an addition to
`JsEngine`, the bet has failed.** That is the finding, and it is worth more than
the slice. Report it; do not route around it.

---

## 2. Scope

**In.** `tick()`; the Promise bridge; `GPU.requestAdapter` → `GPUAdapter.requestDevice`
(so the async path exists end-to-end, even though `wrap_device` stays the primary
entry point); `GPUBuffer.mapAsync` / `getMappedRange` / `unmap`;
`MappedRangeStrategy` with **both** arms implemented in `core/`; the release queue
promoted to drain from `tick()`.

**Out.** Error scopes, `uncapturederror`, device-lost, `GPUQueue`, textures,
pipelines, codegen, and the JSC adapter.

**Mobile bring-up is deferred, by decision (2026-07-09).** The project owner has
ruled out iOS/Android **simulators and emulators** entirely, and deferred real
devices. The near-term goal is to **fill out the API surface on Windows and
macOS** and find out whether the design is feasible before paying for
cross-compilation.

This is the right order. Four-platform parity is a *verification* of a design; it
teaches nothing while the design is still moving, and Phase 2 has already shown
the design still moves (A23). What it costs is that **A21's 32-bit truncation
hazard is unexercised until an armv7 build exists** — which is precisely why A21
demands the guard be tested with `offset = 2^32` on a 64-bit host, where it must
fire anyway.

Windows is still a first-class target of this block: it is where a second
word-size and a second linker are exercised. What is deferred is **iOS and
Android only**, and the old "block 03 = four-platform bring-up" is now
**block 05**, after the API surface is filled.

---

## 3. Rules

### The host contract

**A1 — `tick()` is public API and pumps three queues, in this order.**

1. `wgpuInstanceProcessEvents(instance)` — fires the WebGPU callbacks, which
   resolve JS `Promise`s.
2. The engine's microtask queue, drained until no job is pending — this is what
   actually runs `.then()` continuations.
3. The release queue.

Measured in `specs/tracking/event-loop.md`: after step 1 the Promise is
**fulfilled** and its continuation has **not run**. A binding that stops there
passes every test that avoids `await` and hangs forever on the first one that
uses it. Step 3 is last because QuickJS finalizers run *during* step 2, when the
last reference to a wrapper is dropped.

**A2 — `JS_ExecutePendingJob`'s `<0` return must surface the exception**, and
`tick()` returns a `Result` carrying the message.

**Corrected 2026-07-09, and the original premise was wrong.** A2 first claimed
that "a loop which only checks `JS_IsJobPending` makes a throwing `.then()`
vanish". It does not. `quickjs.c`'s `promise_reaction_job` calls the handler,
and:

```c
is_reject = JS_IsException(res);
if (is_reject) {
    if (unlikely(JS_IsUncatchableError(ctx->rt->current_exception)))
        return JS_EXCEPTION;          /* only interrupts / stack overflow */
    res = JS_GetException(ctx);
}
func = argv[is_reject];               /* reject the derived promise */
```

A throw inside `.then()` is **captured and turned into a rejection of the derived
promise**. `JS_ExecutePendingJob` returns `<0` only for an **uncatchable** error.
So `<0` must still be surfaced — it means the runtime is unwinding — but it is
not the mechanism that catches a throwing continuation.

*(Found by the implementing agent, who was invited to say when the spec is wrong.
It was. Verified at source before being written down.)*

**A22 — the real vanishing hazard is an unhandled rejection, and it needs
`JS_SetHostPromiseRejectionTracker`.** A `.then()` that throws with no `.catch()`
rejects a promise nobody observes, and nothing in `tick()`'s three queues reports
it. Install the tracker; surface unhandled rejections through `tick()`'s result
or a host callback. Do not let a script's async failure disappear.

`quickjs.h` exposes:

```c
typedef void JSHostPromiseRejectionTracker(JSContext *ctx, JSValueConst promise,
                                           JSValueConst reason, bool is_handled, void *opaque);
void JS_SetHostPromiseRejectionTracker(JSRuntime *rt, JSHostPromiseRejectionTracker *cb, void *opaque);
```

Note `is_handled`: a rejection can be handled *later*, so the tracker fires twice
for a promise that gets a `.catch()` after the fact. Report only what remains
unhandled once the microtask queue is drained.

**A3 — every JS-facing async op uses `WGPUCallbackMode_AllowProcessEvents`.**
`requestAdapter`, `requestDevice`, `mapAsync`. `WGPUBufferMapCallbackInfo` has a
`mode` field — verify against `webgpu.h`. `AllowSpontaneous` is forbidden
(`CLAUDE.md` invariant 2). Because callbacks then fire only on the pumping
thread, **no cross-thread signalling is needed anywhere in this block.**

**A4 — `tick()` runs on the thread that owns the engine, and it is the drain
thread.** `WGPUInstance` is the one object the specification guarantees is usable
from any thread; `wgpuInstanceProcessEvents` is explicitly thread-safe. This
project spins up no thread of its own (`release-queue.md` → Q3).

### The Promise bridge

**A5 — a `Deferred` is owned, escapes the call scope, and is settled exactly
once.** QuickJS's `JS_NewPromiseCapability` yields two **owned** resolving
functions that must outlive the call and be freed after settling. They therefore
must **not** be registered in the per-call handle scope (block 01 → R22) — that
scope frees at callback exit, and the deferred has to survive until the WebGPU
callback fires.

Model it as an owned Rust type whose `settle` consumes it. `E::Deferred` need not
be `Copy`.

**A6 — the WebGPU callback owns what its `userdata` points at.** This is Phase 0's
CRITICAL, restated because it will be re-invented: leak an `Arc` (or `Box`) into
`userdata1` with `into_raw`, reclaim it with `from_raw` in the callback. A raw
pointer into something the caller may drop is a use-after-free the moment two
requests are outstanding — and `requestAdapter(); requestAdapter();` is valid JS.

**A7 — concurrent async operations are supported.** Two `mapAsync` calls on
different buffers, or two `requestAdapter` calls, must each settle their own
promise. Test it. The failure mode is a single-slot "pending request" field.

**A8 — every `extern "C"` WebGPU callback catches unwinds** and calls **no**
`webgpu.h` function. Releasing a handle inside a `ProcessEvents` callback is
*legal* — the header exempts that callstack from its re-entrancy prohibition —
but the release queue exists so we never depend on that; enqueue instead.

**A9 — a rejected `Promise` carries a `GPUError`-shaped reason.** `mapAsync`
failing yields `WGPUMapAsyncStatus_Error` / `_Aborted` / `_CallbackCancelled`.
Map each to a rejection; do not collapse them. `_CallbackCancelled` is not an
error the script caused.

### Mapping

**A10 — `MappedRangeStrategy` is a `JsEngine` capability, and `core/` implements
both arms.**

```rust
enum MappedRangeStrategy { ZeroCopyDetach, CopyInCopyOut }
```

- **QuickJS → `ZeroCopyDetach`.** `JS_NewArrayBuffer(ctx, ptr, len, free_func,
  opaque, /*is_shared=*/false)` over the pointer from `wgpuBufferGetMappedRange`,
  detached at `unmap()`.
- **JSC → `CopyInCopyOut`** (block 03 / Phase 3; not shipped here).

**Both arms get `core/` unit tests against the mock now**, by parameterising the
mock's strategy. This is exactly what R23 asks for: the mock takes the union of
engine obligations, and it is the only place `CopyInCopyOut` can be exercised
before JSC exists.

**A11 — `free_func` must be null, or null-tolerant.** Measured
(`engine-boundary.md` → E7/E8): `JS_DetachArrayBuffer` invokes `free_func`
**synchronously at detach** with the real pointer, and
`js_array_buffer_finalizer` invokes it **again** with `ptr == NULL`, because it
does not check `abuf->detached`. A `free_func` that frees unconditionally is a
double-free.

The mapping is owned by the backend and released by `wgpuBufferUnmap`. **Pass a
null `free_func`.** If a hook is ever wanted, it must tolerate the null call.

**A12 — detach cannot fail loudly, so `unmap()` must verify.** `JS_DetachArrayBuffer`
returns `void` and silently no-ops on a non-buffer or an already-detached buffer
(E9). JSC's `transfer()` silently no-ops on a pinned buffer (E5). **Neither engine
reports a failed detach.** After detaching, confirm it — a zero byte length read
back through the C API — and raise a hard error otherwise.

This check is shared behaviour: it lives in `core/` **once**, not in each adapter
(`CLAUDE.md` invariant 11).

**A13 — under JSC, never take the C bytes pointer of a buffer script can see.**
Not exercised in this block, but `core/`'s `CopyInCopyOut` arm must be written to
the protocol now, because the mock will test it: copy in through a *private*
pinned staging buffer and `transfer()` it to script; on `unmap()`, `transfer()`
**first** (which detaches the script-visible buffer) and only then take the
private product's pointer (`engine-boundary.md` → Q1b/E6). Detach before any
pointer is taken; a pinned buffer can then never reach script.

**A14 — `getMappedRange` semantics.** WebIDL:
`getMappedRange(optional GPUSize64 offset = 0, optional GPUSize64 size)`. Absent
`size` means "to the end", which is `WGPU_WHOLE_MAP_SIZE` (`SIZE_MAX`) in C.
Calling it before the map promise settles, or after `unmap()`/`destroy()`, is an
`OperationError`.

**A15 — a buffer may have several mapped ranges, and `unmap()` detaches all of
them.** Track every `ArrayBuffer` handed out. Detaching only the most recent one
leaves script holding a live view over memory `wgpuBufferUnmap` has reclaimed —
the exact hazard `ZeroCopyDetach` exists to close. Test with two ranges.

**A16 — `unmap()` is idempotent, and distinct from `destroy()` and from release.**
Three different operations, in the vocabulary of block 01 → R14: `unmap()`
releases the mapping; `destroy()` frees GPU memory; release frees the handle, via
the queue, later. `destroy()` on a mapped buffer must also detach its ranges.

**A17 — `mappedAtCreation: true` finally gets a real-engine test.** Block 01
recorded that no real-engine test creates a mapped buffer, because the slice had
no `unmap()`. It does now. Create with `mappedAtCreation: true`, write through
`getMappedRange`, `unmap()`, and assert the bytes reached the buffer. Note the C
API needs no `MapWrite` usage for this (`webgpu.h` says so explicitly).

**A21 — `size_t` narrowing is a 32-bit-platform hazard, and it is on the ship
path.** Opened `webgpu.h` to answer §6's third question:

```c
WGPUFuture wgpuBufferMapAsync(WGPUBuffer, WGPUMapMode, size_t offset, size_t size, WGPUBufferMapCallbackInfo);
void *     wgpuBufferGetMappedRange(WGPUBuffer, size_t offset, size_t size);
```

`offset` and `size` are **`size_t`**, not `uint64_t`. WebIDL types them
`GPUSize64`. On **armv7 Android and i686 Windows** — both dev or production
targets — `size_t` is 32 bits, so a `GPUSize64` above `2^32 - 1` **truncates
silently**. `createBuffer`'s `size` is `uint64_t` and does not truncate, so a
buffer can legally be larger than any range you can map on those targets.

Reject before narrowing. `usize::try_from(value)` and raise the WebIDL error, on
every platform, so a 64-bit host and a 32-bit host behave identically — which is
the whole reason a JIT-less engine was chosen (`CLAUDE.md` → Target platforms).
**Test with `offset = 2^32` on a 64-bit host**; the check must fire there too, or
the parity claim is untested until someone builds for armv7.

The mirror image, from block 01 → R8: `WGPUMapMode` is `WGPUFlags` = `uint64_t`
while WebIDL's `GPUMapModeFlags` is a 32-bit `unsigned long`. **Do not let the C
type widen the accepted range.** Both narrowing and widening appear in this one
function; write the guards so neither depends on the host's word size.

Absent `size` means "to the end", which is `WGPU_WHOLE_MAP_SIZE` (`SIZE_MAX`) —
itself word-sized. Do not confuse it with `WGPU_WHOLE_SIZE` (`UINT64_MAX`).

### Boundary

**A18 — every capability this block needs is an *addition* to `JsEngine`.** New
associated types (`Deferred`), new methods (`new_promise`, `settle`,
`drain_microtasks`, `new_external_arraybuffer`, `new_arraybuffer`,
`detach_arraybuffer`, `arraybuffer_len`), and one associated const
(`MAPPED_RANGE_STRATEGY`). **No `core/` logic may change to accommodate an
engine.** If it must, stop and report: that is the JSC exit gate firing one phase
early, and it is a better time to learn it.

**A19 — the adapter names no class and no member** (block 01 → R24), and **holds
no lock across a call into `core/`** (R25). The boundary is re-entrant: `core/`
calls back through `E::payload` while servicing a method.

**A23 — the WebGPU callback runs inside a handle scope, and there is no way to
opt out.** *(Added after the first Phase 2 attempt, which reintroduced Phase 1's
CRITICAL through a hole in the trait.)*

The callback fires from `wgpuInstanceProcessEvents`, outside any JS call, so it
has no per-call scope to inherit. The first attempt solved this with
`type AsyncContext` plus `fn context_from_async(cx) -> Context<'static>`, and
QuickJS implemented the latter as `Context { ctx, scope: None }`. **Every owned
value the callback then created — the settled promise's plumbing included — was
owned by nobody.** P1-C1 verbatim, one layer down. It is why the real-engine
`mapAsync` test leaked a promise graph at runtime teardown.

Two rules follow:

- **`Context`'s scope is not `Option`.** An engine that can silently decline the
  scope will, under pressure, decline it. The type must make the obligation
  unavoidable.
- **The callback opens its own scope.** Give the trait
  `fn with_async_scope<R>(cx: Self::AsyncContext, f: impl FnOnce(Self::Context<'_>) -> R) -> R`
  (or equivalent), so the callback body runs with a real `Context<'_>` whose
  scope drops on the way out. `Context<'static>` must not exist.

This is still an **addition** to `JsEngine`, not a change to `core/`'s logic — so
A18 holds. But it is the second time this project has had to learn that value
ownership is the boundary's hardest edge, and the first was three days of review
ago.

**A20 — the mock is at least as strict as the strictest engine** (R23). It models
value ownership, and now also: promise settlement (settled exactly once), detach
verification, and the `CopyInCopyOut` arm. Where QuickJS and JSC disagree, the
mock takes the **union** of their obligations, so a `core/` bug fails a `core/`
test on the default gate, with no sanitizer and no engine.

---

## 4. Tests

- **`core/` against the mock**, no engine, no backend, no GPU: every rule A1–A17
  that does not require a real engine. **Both** `MappedRangeStrategy` arms.
- **QuickJS + yawgpu Noop**, headless: `wrap_device` → `createBuffer` →
  `mapAsync` → `getMappedRange` → write → `unmap` → assert the bytes landed;
  `mappedAtCreation: true` round-trip; two concurrent `mapAsync`; two ranges
  detached by one `unmap`; a `.then()` that throws surfacing through `tick()`.
  yawgpu's Noop backend allocates real host memory (`NoopBuffer::new(size)`), so
  the zero-copy path is genuinely exercised.
- **The `await` regression test.** Pump `ProcessEvents` without draining
  microtasks and assert an `await` continuation never runs. This is the bug that
  would otherwise ship (`event-loop.md` → E4).
- **Negative demonstrations (R19).** Each guard for a memory hazard must be seen
  red before it is trusted, **on the ordinary `cargo test` gate where possible**:
  - A15: hand out two ranges, detach only one, show the second is still readable.
  - A12: make detach a no-op, show the verification fires.
  - A11: make `free_func` free unconditionally, show the double-free.
  - A6/A7: two outstanding `mapAsync`s where the second overwrites the first's
    userdata, under ASan.

  A guard that only bites under a sanitizer does not run in CI. Poison the memory
  rather than relying on ASan wherever a deterministic assertion is possible
  (block 01 → the P1-M4 correction).

---

## 5. Exit criteria

1. The QuickJS async slice runs headless on macOS against yawgpu.
2. Both `MappedRangeStrategy` arms are implemented in `core/` and unit-tested.
3. `cargo test -p webgpu-native-js-core` still passes with **no engine, no
   backend feature, and the backend env var unset**.
4. Every new engine capability is an *addition* to `JsEngine`; `core/`'s existing
   logic is unchanged. **Any exception is the headline finding of this phase.**
5. `tick()` is public, documented, and its three-queue order is tested.
6. Full workspace gate green; Phase Review clean of CRITICAL and MAJOR.

Platform bring-up and four-platform parity are **block 03**, and Phase 2 is not
COMPLETE without it.

## 6. Open questions this block will answer

- **Where does `tick()` live?** `core/`, generic over `E`, taking the
  `WGPUInstance` from the host — or a host-facing crate above the adapter?
  Deferred from block 01 §6. Decide with the code.
- **Does `Deferred` need to be an associated type, or can it be a `core/` struct
  parameterised by two `E::Value`s?** The latter would be one fewer thing an
  engine must supply. Try it; QuickJS's resolving functions are just values.
- ~~**Is `mapAsync`'s `size` argument `size_t` or `uint64_t`?**~~ **ANSWERED:
  `size_t`.** Which makes it a silent-truncation hazard on 32-bit targets, and
  those are ship targets. Promoted to **A21** rather than left as a question.
