# Tracking: the release queue

**Historical engine note (2026-07-12):** QuickJS rows and mechanisms below are
past measurements. The release queue itself remains required and tested; only
the removed engine's harness/tests were deleted.

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

**Status: ANSWERED (2026-07-09). No. The queue stays a plain FIFO. Ordering is
made irrelevant by holding a *native* reference to the parent, not by sorting
release requests and not by relying on GC order.**
Spike: `spikes/release-queue/`, 10 tests, headless, yawgpu Noop.

### R1 — the four observations

An `Instance` (parent) and an `Adapter` (child) were each wrapped in a JS object
whose finalizer enqueues a release request. Both JS references were dropped, GC
was requested four times, and the finalizer log was snapshotted **before** the
context was torn down, then again after.

| Engine | Parent ref | Finalizers during GC | Finalizers at teardown | Drain order |
|---|---|---|---|---|
| QuickJS | with | `["Adapter", "Instance"]` | — | Adapter, Instance |
| QuickJS | without | `["Adapter", "Instance"]` | — | Adapter, Instance |
| JSC | with | **none (0 of 2)** | `["Instance", "Adapter"]` | Instance, Adapter |
| JSC | without | **none (0 of 2)** | `["Instance", "Adapter"]` | Instance, Adapter |

### R2 — the first reading of this experiment was wrong, and the fix mattered

The spike's first version read the finalizer log *after* releasing the
`JSGlobalContext`, and reported `["Instance", "Adapter"]` as JSC's ordering. That
number was real; the interpretation was not. Releasing a `JSGlobalContext`
finalizes every surviving object in unspecified order, so the measurement could
not distinguish a **GC-phase** ordering (which would be a finding about JSC's
collector) from a **teardown** ordering (which says nothing).

Snapshotting before teardown separated them, and the answer turned out to be more
interesting than either hypothesis: under JSC **no finalizer ran during GC at
all.**

### R3 — `JSGarbageCollect` does not run finalizers (independently reproduced)

Confirmed outside the spike, with a minimal program linking the system framework:
a `JSObjectRef` of a class with a `finalize` callback, its only reference
dropped, is **not** finalized after four `JSGarbageCollect` calls. It is
finalized at `JSGlobalContextRelease`. The result is unchanged when the object is
created in a `noinline` frame that has returned and the machine stack is
overwritten, so conservative stack rooting is not a sufficient explanation.

`JSBase.h` documents `JSGarbageCollect` as performing a collection, and notes
that values are destroyed when the last reference to the context group is
released. It is silent on whether finalizers run promptly. The **public C API
offers no other GC entry point**, and no synchronous-collect function.

We do not claim to know the mechanism — deferred sweeping and conservative
rooting are both plausible. The observable fact is enough:

> **Finalizer timing is not controllable through JSC's public C API.**

### R4 — consequence for invariant 7 ("GC is a backstop")

This is now evidence, not prudence. Under JSC, a script that forgets `destroy()`
may hold GPU memory **until the context is torn down**, and neither the host nor
the binding can force the finalizer to run. On iOS that is a memory-pressure
crash waiting to happen.

`destroy()` is therefore not "good practice". Under JSC it is the **only bounded
path**. This belongs in the user-facing docs in exactly those words.

It also means: **no test may depend on provoking a JSC finalizer via GC.** The
spike's JSC finalizer test is honestly named `..._is_simulated_from_foreign_thread`
for this reason.

### R5 — the design: a native parent reference makes ordering a non-question

QuickJS is refcounted, so dropping the child decrements the parent and the child
is finalized first — deterministically. JSC is a tracing collector, and at
teardown the order is unspecified; here it ran **parent first**. So finalizer
order differs by engine, and under JSC it is unspecified by construction.
Depending on it is depending on sand — in *either* engine, since teardown has no
ordering guarantee anywhere.

Rather than teach the queue to topologically sort, **each child wrapper takes a
native reference on its parent handle**: wrapping an `Adapter` calls
`wgpuInstanceAddRef(instance)` and stores the handle in the child's release
payload. The child's release request releases the adapter *and then* drops that
parent reference. The parent's native object cannot be destroyed while a child's
native reference exists — **whatever order finalizers ran in, whatever order the
queue drains in.**

Verified by draining **parent-first on purpose**, the worst case. Observed native
release sequence: `["Instance", "Adapter", "AdapterParentInstanceRef"]` — the
wrapper's own instance release runs first, and the child-held parent reference
drops last. ASan reported no double-free and no use-after-free.

**Retraction (Phase 0 review, `phase-reviews.md` → P0-M3).** An earlier version of
this paragraph said "macOS ships no LeakSanitizer, so that run does not speak to
leaks; **the exactly-once assertions do**." The second half was false, and it is
exactly the tautology this project has been catching elsewhere. The assertions
compare `native_release_order` against an expected vector — but that vector is
pushed by `record_native_release`, which *our own release functions call
unconditionally, immediately after* invoking `wgpuXxxRelease`. **If
`wgpuAdapterRelease` were a no-op, or leaked, every assertion would still pass.**
The log proves the queue invoked each release function once. It says nothing about
what the backend did with the call.

What the run therefore supports, precisely: the queue drains each request exactly
once, in FIFO order, and parent-first draining triggers no double-free or
use-after-free. Leak-freedom is **not** established. `event-loop.md` → E6 already
states the honest form of this limit — the C ABI exposes no refcount
introspection, so the strongest available check is a liveness probe on an extra
reference. **That probe now exists** (`assert_drain_does_not_over_release`): an
extra native reference is held across the drain, the handles are exercised through
ordinary C ABI calls afterwards, and the probe references are then released. It
establishes **no over-release**. It does not establish leak-freedom, and its doc
comment says so.

Also stated: this claims "the adapter is still valid when its release runs" only
insofar as ASan flagged no use-after-free in the `AddRef`/`Release` sequence. That
is a UAF check, not a validity proof.

**The queue remains a plain FIFO. No ordering logic was added.**

Note what this separates. The JS-level parent reference is about keeping the
parent's *wrapper identity* alive for `.parent`-style accessors. The native
`AddRef` is about keeping the parent's *native object* alive. Rev 2 conflated
these two jobs into one mechanism, and the mechanism it picked only works under a
refcounting engine.

Caveat, stated: this was verified against yawgpu, whose handles are `Arc`-based
and therefore tolerant. It demonstrates that the native-reference design is
*sufficient*; it cannot demonstrate that a less tolerant backend would have
failed without it. That is the point of holding the reference rather than
assuming the backend retains its parents internally — the specification says
nothing either way.

### Superseded framing

**Status when opened: REFRAMED. Probably not — and if it does, the queue is the wrong place.**

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

**Status: ANSWERED (2026-07-09). The thread that calls `tick()`. This project
spins up no thread of its own.**

`wgpuInstanceProcessEvents` and `wgpuInstanceWaitAny` are explicitly thread-safe
(Q1), and `WGPUInstance` is the one object the specification guarantees is usable
from any thread. The pump already runs once per frame on the host's thread
(`specs/tracking/event-loop.md`). Draining there costs nothing extra and keeps
every `wgpuXxxRelease` on a single, known thread.

The spike asserts this by thread id: every release executes on the thread that
called `drain()`, never on a finalizer's thread. A finalizer that panics is
contained, does not unwind across the engine's C boundary, does not poison the
queue, and does not leak its handle — the queue still drains afterwards.

This closes plan §6 Q2. Revisit only if a host appears whose frame thread must
not block on GPU teardown.
