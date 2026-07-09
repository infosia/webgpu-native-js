# Tracking: the host event-loop contract

Topic owner: `CLAUDE.md` invariants 2 and 3; plan §2.6 / §2.7.

---

## Q1 — Does the two-queue pump contract actually hold?

**Status: ANSWERED (2026-07-09). Yes, and it is now an executable invariant.**
Spike: `spikes/event-loop-pump/`, against yawgpu's Noop backend, headless.

The claim: `wgpuInstanceProcessEvents()` fires the WebGPU callback, which
resolves the JS `Promise`. **Resolving a Promise does not run its `.then()`.**
The host must additionally drain the engine's microtask queue. A binding that
pumps only the first queue passes every test that avoids `await` and hangs
forever on the first one that uses it.

### E1 — yawgpu genuinely defers the callback to `ProcessEvents`

This was the load-bearing unknown. If yawgpu had fired the `requestAdapter`
callback inline despite `WGPUCallbackMode_AllowProcessEvents`, the Promise would
resolve during the call, the pump would never be exercised, and everything
downstream would rest on a coincidence.

It does not. Immediately after `wgpuInstanceRequestAdapter` returns, the callback
count is `0`. After one `wgpuInstanceProcessEvents` it is `1`. After a second, it
is still `1`.

Note the two tests are only meaningful together: "did not fire yet" would also
pass if the callback were never registered at all. The exactly-once test is what
gives the first one its content.

### E2 — the observed sequence

```
requestAdapter()                    callback_count == 0
wgpuInstanceProcessEvents(instance) callback_count == 1
                                    promise resolved
                                    JS_IsJobPending() == true
                                    globalThis.ran    == false   <-- the whole point
JS_ExecutePendingJob() until !pending
                                    globalThis.ran    == true
                                    JS_IsJobPending() == false
```

The middle block is the finding. The Promise is resolved and the continuation has
**not run**. Anyone who pumps only `ProcessEvents` sees a resolved Promise and
concludes the work is done.

### E3 — `AllowProcessEvents` removes the need for cross-thread signalling

The thread id recorded inside the callback equals the thread that called
`wgpuInstanceProcessEvents`. This is precisely what the mode buys, and it is why
plan §2.6's original "blocking unknown" (which thread do callbacks fire on?) was
never a discovery problem but a choice.

`AllowSpontaneous` appears nowhere in the spike, and the callback calls no
`webgpu.h` function.

### E4 — the failure mode is now a regression test

`omitting_microtask_drain_never_runs_await_style_continuation` runs a real
`async`/`await` continuation and ticks `ProcessEvents` **eight times** without
draining microtasks. `globalThis.ran` stays `false` throughout, and a job remains
pending. `microtasks_before_process_events_defers_continuation_until_next_tick`
pins the *ordering* — draining before `ProcessEvents` in the same tick defers the
continuation by a full tick.

Together these make the bug impossible to reintroduce silently, which is the only
reason this spike exists.

### Consequence for the public API

`tick()` is public API, not an adapter detail (plan §2.7). Its contract:

1. `wgpuInstanceProcessEvents(instance)`
2. drain the engine's microtask queue until no job is pending

`JS_ExecutePendingJob` returns `>0` (a job ran), `0` (nothing pending), or `<0`
(the job threw). **The error case is easy to swallow**: a loop that only checks
`JS_IsJobPending` will spin or exit silently on `<0`. The spike returns an error.
`core/`'s eventual `tick()` must too — an exception thrown inside a `.then()` is
otherwise invisible.

---

## Revision landed (2026-07-09) — Q1 CLOSED

Gates re-run directly: `cargo test --offline -p event-loop-pump --features
ffi/backend-yawgpu` → **8 passed**, EXIT=0; clippy `-D warnings` → EXIT=0.

### E5 — the engine agrees with our bookkeeping, and now we assert the engine

`JS_PromiseState` reports `JS_PROMISE_FULFILLED` at exactly the point the old
Rust flag flipped. **They do not diverge.** So E2's conclusion was right all
along — but it is now first-hand evidence rather than a proxy for it.

The crux test closes the full loop:

```rust
assert_eq!(request.promise_state(), PromiseState::Pending);   // before
process_events(&instance);
assert_eq!(request.promise_state(), PromiseState::Fulfilled); // engine says so
assert!(js.is_job_pending());
assert!(!js.eval_bool("globalThis.ran")?);                    // continuation unrun
js.drain_microtasks()?;
assert!(js.eval_bool("globalThis.ran")?);
```

`callback_count()` is retained, because "did the C callback run?" is a genuinely
different question from "is the Promise fulfilled?", and E1's pre-pump test
depends on the former.

### E6 — the adapter is released, and the C ABI limits how well we can prove it

`request_adapter_callback` now calls `wgpuAdapterRelease` on success, with a doc
comment recording that this is legal (`webgpu.h` forbids re-entrant calls only
from **spontaneous** callbacks and exempts the `ProcessEvents` / `WaitAny`
callstacks) and that a real binding will enqueue a release request instead.

**Stated limitation, not papered over.** `webgpu.h` exposes no refcount
introspection, so "released exactly once" cannot be observed directly. The test
asserts our own release-call count is `1`, holds an extra reference taken with
`wgpuAdapterAddRef`, probes that the handle is still usable via
`wgpuAdapterHasFeature`, and confirms a second `ProcessEvents` does not release
again. That is a bookkeeping assertion plus a liveness probe — strictly weaker
than a refcount check, and it is the best the C ABI allows. macOS ships no
LeakSanitizer, so ASan adds nothing here either.

The test-only `AddRef` is gated behind a `Cell<bool>` that is off by default, so
the other tests are unaffected.

### E7 — the aliasing fix is by construction, not by tooling

No `&mut` is derived from `userdata1` any more: the callback takes `&*` and the
counters are `Cell`s. **Miri could not verify this.** `nightly` and `miri` are
installed, but Miri aborts on the first foreign function call
(`JS_NewRuntime`) — it does not support FFI on macOS. So the fix rests on
inspection, not on a checker. Recorded so nobody later assumes it was
machine-verified.

`drain_microtasks` now carries the failing job's exception message out on the
`<0` path, with a test for it.

---

## E8 — a throwing `.then()` does **not** surface via `JS_ExecutePendingJob`

**Found in Phase 2, by the implementing agent, and verified at source.** It
corrects the reasoning behind block 02's original A2.

`quickjs.c`'s `promise_reaction_job`:

```c
res = JS_Call(ctx, handler, JS_UNDEFINED, 1, &arg);
is_reject = JS_IsException(res);
if (is_reject) {
    if (unlikely(JS_IsUncatchableError(ctx->rt->current_exception)))
        return JS_EXCEPTION;          /* interrupts, stack overflow */
    res = JS_GetException(ctx);
}
func = argv[is_reject];               /* reject the derived promise */
```

A throw inside `.then()` is **captured and converted into a rejection of the
derived promise**. `JS_ExecutePendingJob` returns `<0` only for an **uncatchable**
error. So the `<0` case must still be surfaced — the runtime is unwinding — but
it is not what catches a throwing continuation.

**The real vanishing hazard is an unhandled rejection**, and the only way to see
it is `JS_SetHostPromiseRejectionTracker` (block 02 → A22). Its `is_handled`
argument fires again when a `.catch()` attaches late, so report only what remains
unhandled once the microtask queue has drained.

E1–E4 are unaffected: they concern *whether the continuation runs at all*, which
the two-queue pump still governs. E8 concerns what happens when it runs and
throws.

That is the second time a claim about `webgpu.h` or `quickjs.h` written from
reasoning rather than from the file turned out to be wrong. The agent was invited
to say when the spec is mistaken, and did.

---

## Original review of the spike — VERDICT: result accepted, revision required

Gates re-run directly: `cargo test --offline -p event-loop-pump --features
ffi/backend-yawgpu` → 6 passed, EXIT=0; clippy `-D warnings` → EXIT=0. `ffi` and
both detach spikes still build.

E1–E4 stand. The findings below do not threaten the conclusion; they weaken the
*evidence* for it, and one of them is a leak.

- **MAJOR-1 — the crux assertion observes our bookkeeping, not the engine.**
  `AdapterRequest::is_resolved()` returns a Rust `bool` set by the callback: it
  means "we invoked the resolve function", not "the Promise is fulfilled". The
  claim under test is precisely *"the Promise **is resolved**, yet the
  continuation has not run"*. quickjs-ng exposes `JS_PromiseState` and
  `JS_PROMISE_FULFILLED` (`quickjs.h`). Assert against the engine's own state.
  The distinction is not academic: if a future refactor resolved the Promise
  through a path that sets the flag but does not fulfil it, the crux test would
  still pass.

- **MAJOR-2 — the adapter handle is dropped on the floor.**
  `request_adapter_callback` binds the `WGPUAdapter` as `_adapter` and never
  calls `wgpuAdapterRelease`. WebGPU futures hand the callback an **owned**
  reference; every test leaks one adapter. macOS ships no LeakSanitizer, so ASan
  would not have caught it.

  Worth stating why this is more than a spike-hygiene nit: this is the **first
  place a `webgpu.h` handle is handed to us across a callback boundary**, which
  is exactly the shape Phase 0.5's release queue exists to manage. Note also that
  releasing here is *legal* — `webgpu.h` forbids re-entrant API calls only from
  **spontaneous** callbacks, explicitly exempting the `ProcessEvents` and
  `WaitAny` callstacks. The right fix is to release it, and to notice that a real
  binding would enqueue a release request instead.

- **MINOR-1 — `&mut` through a pointer aliasing an owned `Box`.**
  `AdapterRequest` owns `state: Box<RequestState>` and passes a raw pointer to it
  as `userdata1`; the callback does `&mut *userdata1.cast::<RequestState>()`
  while the `Box` is still live and later read through `self.state`. Under
  Stacked Borrows the raw pointer is invalidated by uses of the `Box`. ASan does
  not see this; Miri would. Same class as the finding already fixed in
  `quickjs-detach`. Prefer shared `&` plus `Cell`/atomics for the counters, so no
  `&mut` is ever created from the raw pointer.

- **MINOR-2 — the failing-job exception is discarded.** `drain_microtasks`
  correctly maps `<0` to an error, but drops the exception pending on `job_ctx`.
  Carry the message; a `.then()` that throws will otherwise be undebuggable.

### Revision handoff → coding agent

```
## Task: event-loop-pump — assert the engine's promise state; stop leaking adapters

Phase: 0 (spike revision). Read specs/tracking/event-loop.md first.

Fix, in order:
- MAJOR-1: replace AdapterRequest::is_resolved()'s Rust flag with a query of
  the engine: JS_PromiseState(ctx, promise) == JS_PROMISE_FULFILLED. Keep the
  callback counter, which answers a different question (did the C callback
  run?). Test 3 must assert BOTH: the promise is fulfilled per the engine, and
  globalThis.ran is still false.
- MAJOR-2: release the WGPUAdapter handed to request_adapter_callback. Add a
  test asserting the adapter is released exactly once. Add a doc comment saying
  that a real binding enqueues a release request rather than calling release
  inline, and that release inside a ProcessEvents callback is legal because
  webgpu.h exempts the ProcessEvents and WaitAny callstacks from its
  re-entrancy prohibition.
- MINOR-1: do not create &mut from userdata1 while AdapterRequest's Box is
  live. Use Cell/atomics and a shared reference.
- MINOR-2: carry the pending exception's message out of drain_microtasks on
  the <0 path.

Out of scope: core/ changes, real GPU, the other spikes, commits, specs/ edits.

Acceptance criteria:
- [ ] test 3 asserts JS_PROMISE_FULFILLED from the engine AND !globalThis.ran
- [ ] adapter released exactly once; asserted
- [ ] no &mut derived from userdata1 while the Box is live
- [ ] drain_microtasks surfaces the JS exception message
- [ ] all six existing tests still pass; clippy clean with -D warnings
- [ ] no local or sibling filesystem paths

Report back: whether JS_PromiseState reported FULFILLED at exactly the point
is_resolved() did, or whether the two ever diverge. If they diverge, that is a
finding and I want the details.
```
