# Tracking: the release queue

Topic owner: `CLAUDE.md` invariant 4; plan §2.5.

---

## Q1 — Is `wgpuXxxRelease` thread-safe?

**Status: ANSWERED (2026-07-09). Conditionally — and the condition is not
queryable. This corrects a claim this project had already written down twice.**

### The claim we had made, and why it was wrong

`CLAUDE.md` invariant 4 and plan §2.5 both asserted:

> `webgpu.h` guarantees **no** thread-safety for `wgpuXxxRelease`.

That was inferred from the *header*, where `wgpuAdapterAddRef` and
`wgpuAdapterRelease` carry no documentation at all — no thread-safety statement,
no lifetime statement, nothing. **Absence of a guarantee in the header is not a
guarantee of absence.** The normative prose lives in
`webgpu-headers/doc/articles/`, which we had not read.

### What the specification actually says

`doc/articles/Multithreading.md`:

> The `webgpu.h` API is thread-safe (when multithreading is supported). That is,
> its functions are reentrant and may be called during the execution of other
> functions, with the following exceptions:
> - Encoder objects … are not thread-safe.
> - API calls may not be made during `WGPUCallbackMode_AllowSpontaneous`
>   callbacks.

So `Release` *is* thread-safe — **where multithreading is supported.** And:

> `webgpu.h` implementations are **allowed to require** that all returned
> objects, except for `WGPUInstance`, can only be used from the thread they were
> created on, **causing undefined behavior otherwise**.
>
> Native (non-Wasm) implementations **should** support multithreading.

"Should", not "must".

### The finding that matters

**There is no way to ask an implementation whether it supports multithreading.**
The token `multithread` does not appear anywhere in `webgpu.h`, and the instance
feature enum offers only `TimedWaitAny`, `ShaderSourceSPIRV`, and
`MultipleDevicesPerAdapter`.

So a conformant `webgpu.h` implementation may legally declare every object except
`WGPUInstance` thread-confined, make off-thread use undefined behaviour, and
provide no way for a caller to detect this.

### Consequence — the release queue survives, on firmer ground

A JavaScriptCore finalizer fires on **an arbitrary GC thread**. If it calls
`wgpuBufferRelease` directly, that is undefined behaviour against any conformant
implementation that has not opted into multithreading — and **the binding cannot
detect at runtime whether it is talking to one.**

That is a stronger justification than the one we had. The old reasoning — "the
header promises nothing, and the fact that all three backends happen to be
`Arc`/atomic-refcounted is an implementation accident" — reached the right
conclusion by the wrong route. The accident framing was right about the risk and
wrong about the spec.

So: **finalizers never call `webgpu.h` directly.** They enqueue. A designated
thread drains. This holds regardless of which backend is linked, and it is the
only portable choice given an unqueryable capability.

Note the exception is exactly the one that helps us: `WGPUInstance` is explicitly
usable from any thread, and `wgpuInstanceProcessEvents` / `wgpuInstanceWaitAny`
are explicitly listed as thread-safe. That is what lets the pump thread be the
drain thread.

Also explicitly thread-safe, per the same article: `wgpuDeviceDestroy`,
`wgpuBufferDestroy`, `wgpuTextureDestroy`, `wgpuQuerySetDestroy`. Since scripts
are expected to call `destroy()` (invariant 7), the common path never needs the
queue at all — the queue is the backstop for the uncommon one.

---

## Q2 — Does the queue need to enforce child-before-parent ordering?

**Status: REFRAMED. Probably not — and if it does, the queue is the wrong place.**

`CLAUDE.md` invariant 4 says "the queue also enforces child-released-before-parent
ordering". The specification is **silent** on whether a child object keeps its
parent alive; the header documents neither `AddRef` nor `Release`.

But an ordering-aware queue means modelling the parent/child graph in `core/` and
topologically sorting release requests — real complexity, for a rule nobody has
verified is necessary.

**There is a cheaper mechanism that makes the ordering question disappear.** Let
each JS wrapper hold a reference to its **parent's JS wrapper** — `GPUBuffer`
keeps `GPUDevice` reachable, `GPUDevice` keeps `GPUAdapter` reachable. Then the
engine's own GC cannot collect a parent while a child is alive, finalizers run in
a valid order by construction, and the queue stays a plain FIFO. This is the
standard trick and is what `dawn.node` does.

It costs one slot per wrapper and requires the engine to trace it: QuickJS via
`JS_DupValue` plus the class `gc_mark` callback; JSC by protecting the value or
storing it as a property. Both are additive `JsEngine` capabilities, not `core/`
churn.

**Decide this in Phase 0.5 with a test, not by assertion.** The spike must
demonstrate what actually happens when a parent's finalizer runs before a
child's, under each engine.

---

## Q3 — Who owns the drain thread?

**Status: OPEN.** Plan §6 Q2. Given that `wgpuInstanceProcessEvents` is
explicitly thread-safe and the pump already runs once per frame on the host's
thread (`specs/tracking/event-loop.md`), the obvious answer is "the thread that
calls `tick()`". Confirm in Phase 0.5; do not spin up a thread this project owns
without a reason.
