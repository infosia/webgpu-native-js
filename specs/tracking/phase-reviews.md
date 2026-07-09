# Tracking: Phase Reviews

Per `specs/reference/workflow.md` → "Phase Review (mandatory — Clean Review Then
Fix)". Findings, triage decisions, fixes, and gate results live here.

---

## Phase 0 Review — 2026-07-09

Three **fresh, no-context** reviewers were run in parallel over the cumulative
diff (`initial..HEAD`, 30 files, ~6000 insertions), each with a distinct lens:

1. correctness / memory safety / FFI soundness,
2. `CLAUDE.md` conventions compliance,
3. **evidence integrity** — do the tests support the claims, and do the documents
   agree with each other and with upstream?

Running three rather than one is a deliberate strengthening of the workflow. They
found almost disjoint sets, which retrospectively justifies it: the correctness
lens found the only code defect, the conventions lens found the only doc-coverage
gap, and the evidence lens found every documentary defect — including one the
other two, and the author, walked past.

**The most consequential defects were documentary, not in the code.** `CLAUDE.md`
is the document that wins on conflict, so a stale invariant there is more
dangerous than a wrong spike: everything downstream is told to trust it first.

### Findings and triage

| ID | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| **P0-C1** | CRITICAL | `CLAUDE.md` invariant 2; plan §2.6 | "The device-lost and uncaptured-error callbacks have no configurable mode." **False.** `WGPUDeviceLostCallbackInfo` has a `mode` field ("Controls when the callback may be called"); only `WGPUUncapturedErrorCallbackInfo` lacks one. | **Confirmed against the pinned header. Fixed** (Claude). |
| **P0-C2** | CRITICAL | `spikes/event-loop-pump/src/lib.rs:567–582` | Use-after-free: `js_request_adapter` registers `request.state_ptr()` as `userdata1` and calls `wgpuInstanceRequestAdapter` **before** storing the request, then drops the previously-stored one. `requestAdapter(); requestAdapter();` before a pump frees the first `RequestState` while its callback is still pending. | **Confirmed by reading. Handed to the coding agent.** Kept CRITICAL — see below. |
| **P0-M1** | MAJOR | `CLAUDE.md` core principle 4 | Still asserts "`webgpu.h` guarantees no thread-safety for `wgpuXxxRelease`" and "Release ordering must respect child-before-parent" — both overturned by `release-queue.md` Q1/Q2. Design invariant 4 was fixed; the parallel principle was not. | **Confirmed. Fixed** (Claude). |
| **P0-M2** | MAJOR | plan §7 row 4 | The correction log itself carried the refuted "Real reason: no thread-safety". `CLAUDE.md` tells readers to read §7 *first*, so the wrong reason was the first thing they'd absorb. | **Confirmed. Fixed** (Claude): row struck through, Rev 3 added. |
| **P0-M3** | MAJOR | `release-queue.md` R5 | "the exactly-once assertions [speak to leaks]" — tautological. `native_release_order` is pushed by `record_native_release`, which our own release fns call unconditionally after `wgpuXxxRelease`. **A no-op or leaking release would pass identically.** | **Confirmed. Doc retracted** (Claude). Code fix (liveness probe) handed to the coding agent. |
| **P0-M4** | MAJOR | `spikes/quickjs-detach/src/lib.rs` | No `#![warn(missing_docs)]` at crate root, and five `pub` items undocumented (`Error`, `FreeEvent`, `take_free_events`, `QuickJs`, `MappedRange`). Found independently by two lenses. | **Confirmed. Handed to the coding agent.** |
| P0-m1 | MINOR | `specs/reference/workflow.md` | "The spike crates are outside the workspace" — true of only two of four. | Fixed (Claude). |
| P0-m2 | MINOR | plan §2.5 | "JSC finalizers may fire on any thread (documented engine behavior)" — uncited, and Phase 0 could not observe it (R3). | Fixed (Claude): downgraded to an explicitly unverified premise, and noted it is no longer load-bearing. |
| P0-m3 | MINOR | `spikes/release-queue/src/lib.rs:585` | `qjs_gc_mark` is the one production `extern "C"` fn without `catch_unwind`. Both lenses agree no panic path exists today. | Handed to the coding agent — principle 8 admits no exceptions, and the cost is one wrapper. |
| P0-m4 | MINOR | `spikes/release-queue/src/lib.rs:686–696` | `JscContext::wrapper` leaks the payload `Box` and leaves `parent` protected if `JSObjectMake` returns null. | Handed to the coding agent. |
| P0-m5 | MINOR | `spikes/jsc-detach/src/lib.rs` | No `#![warn(missing_docs)]` at crate root (its items *are* documented). | Handed to the coding agent. |
| P0-m6 | MINOR | four spikes | Dead code: `Error::RequestAdapterFailed`, `Error::CallbackPanicked`, `Error::InteriorNul`, `MappedRange::object`. | Handed to the coding agent. |

### Deferred MINORs, with rationale

Per the workflow gate, MINOR may be deferred only with a written reason.

- **`#[non_exhaustive]` on spike enums.** The convention targets *public* API that
  downstream code will match on. These enums never leave their spike crate, and
  every spike is scheduled for deletion once `core/` subsumes it. Applying it here
  buys nothing and adds noise. Revisit when `core/` defines its first public enum.
- **Renaming the bare `ffi` crate.** Real hazard, wrong moment: the rename touches
  the workspace root, two path dependencies, and every gate command in
  `workflow.md`. Do it as the first change of Phase 1, when `core/` lands beside
  it and the naming scheme is decided once.
- **Edition drift (2021 vs 2024 across spikes).** Harmless for standalone crates;
  unifying now would churn four manifests for no behavioural change. Fold into the
  Phase 1 workspace cleanup.
- **`QUICKJS_CLASS_ID` as a process-global, and JS handles smuggled as `usize`
  through a `static Mutex`.** Both are sound only because tests serialize on
  `TEST_LOCK`. Correct for a spike, unacceptable for `core/`. Not fixed here;
  recorded as an explicit **anti-pattern that must not be carried into Phase 1**.

### Verdict on P0-C2's severity

The reviewer offered a downgrade, on the grounds that the spike's harness is
contractually single-request and no test exercises the double call. **Declined.**
It is a use-after-free reachable from ordinary, valid script; `CLAUDE.md`'s own
severity table names "a dangling pointer handed to script" as CRITICAL; and
WebGPU permits concurrent `requestAdapter()` calls, so **the pattern would be
copied verbatim into the Phase 2 binding.** Being throwaway code lowers the
urgency, not the severity. The fix — let the callback own a reference to the
state rather than borrowing into a `Box` the slot may drop — is the shape the
production binding needs anyway.

### What the review confirms as genuinely earned

Recorded because a review that only lists defects misleads about the state of the
phase. Independently verified by the evidence lens against the pinned sources:

- The two-queue pump contract, including that the crux test asserts the *engine's*
  `JS_PromiseState`, and that "callback did not fire yet" is paired with an
  exactly-once test so it cannot pass for the wrong reason.
- QuickJS `ZeroCopyDetach`, including the `free_func` two-call sequence read off
  the vendored `quickjs.c`.
- The JSC pinning hazard (E5) and its regression test, and the 1 MiB copy protocol.
- `backend-deltas` D1–D3: the 202-function count, the `a11ef44` pin, and the
  178 → 200 export arithmetic all reconcile.
- No `unsafe impl Send/Sync`; no `unwrap`/`expect`/`panic!` in non-test library
  code; `AllowSpontaneous` absent everywhere; no local paths in any tracked file;
  no generated code committed.
- The ASan/LeakSanitizer caveat and Miri's FFI limitation are stated honestly
  throughout — *except* at R5, which is P0-M3.

### Gate — PASSED. Phase 0 is COMPLETE.

All CRITICAL and MAJOR findings are closed. Gates re-run directly by Claude, not
taken on the agent's word:

```
cargo test  --workspace --features ffi/backend-yawgpu   EXIT=0   (ffi 2, event-loop-pump 9, release-queue 10)
cargo clippy --workspace --all-targets -- -D warnings   EXIT=0
spikes/jsc-detach                                        EXIT=0
spikes/quickjs-detach                                    EXIT=0
```

**P0-C2 was reproduced before it was fixed.** The coding agent archived the
pre-fix tree, added the minimal `requestAdapter(); requestAdapter();` repro, and
ran it under ASan: `heap-use-after-free … in core::cell::Cell::get`, on the
callback path. This matters more than the fix. A regression test that has never
been seen to fail is a test of nothing, and the reviewer's own triage note had
observed that no existing test exercised the double call.

The fix is the shape the production binding needs: the callback owns an
`Arc<RequestState>` clone leaked through `userdata1` and reclaimed with
`Arc::from_raw` at the end, so replacing or dropping the slot can never free state
a pending callback still points at. No `unsafe impl Send`/`Sync` was added —
verified. `RequestState` stays `!Send`, which is correct, because
`AllowProcessEvents` guarantees the callback fires on the pumping thread
(`event-loop.md` → E3).

**P0-M3's code half landed with the right claim.** The new probe's doc comment
reads: *"The C ABI exposes no refcount introspection, so this does not prove
leak-freedom."* It keeps an extra native reference across the drain, calls
ordinary C ABI functions on the handles afterwards, then releases the probe
references — establishing **no over-release**, and saying so in those words rather
than restating the tautology it replaced.

Verified fixed: P0-C1, P0-C2, P0-M1, P0-M2, P0-M3 (doc + code), P0-M4, P0-m1
through P0-m6. Deferred MINORs are listed above with rationale.

### Reflection worth keeping

Three of the four documentary defects share one cause: **asserting what a header
says without opening it.** P0-C1 (device-lost mode), and the two thread-safety
errors it echoes, were all produced by reasoning from a header's *silence* or from
memory. Every one was refutable in under a minute by reading the pinned file that
sits in this repository. The rule this phase should hand to Phase 1 is not "read
the spec" — everyone believes they do — but: **a claim about an upstream artifact
is not written down until the artifact has been opened in the same session.**
