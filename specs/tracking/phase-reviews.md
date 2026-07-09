# Tracking: Phase Reviews

Per `specs/reference/workflow.md` → "Phase Review (mandatory — Clean Review Then
Fix)". Findings, triage decisions, fixes, and gate results live here.

---

## Phase 2 Review — 2026-07-09

Three fresh no-context reviewers. The third lens was pointed at a single
question — **"what does green not mean here?"** — and given this project's own
history as evidence about its blind spots: two mocks that were mirrors, and an
`#[allow]` that hid a soundness defect for a whole phase.

It answered by **experiment**. It deleted the A12 detach-verification guard
(`if !detached` → `if !detached && false`) and re-ran `cargo test -p
webgpu-native-js-core`: **22 of 22 passed.** Not an argument. A measurement.

Three CRITICALs. Every one of them reachable from code an honest author writes,
and none of them visible to a green suite.

### Findings and triage

| ID | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| **P2-C1** | CRITICAL | `core/src/lib.rs:980` | `finalize_buffer` enqueues `wgpuBufferRelease` **without detaching outstanding zero-copy ranges**, and nothing roots the buffer from the range. `const r = buf.getMappedRange(); buf = null;` then a GC frees the mapping while `r` still aliases it. Use-after-free from an honest script. | **Confirmed.** Block 02 → **A25**: `getMappedRange` `AddRef`s the buffer and stores it as the `ArrayBuffer`'s `opaque`; the `free_func`'s `NULL` call enqueues the release. A finalizer cannot fix this — JSC's may run on any thread and must not call into the engine. |
| **P2-C2** | CRITICAL | `adapters/quickjs/src/lib.rs:224` | `Runtime::drop` calls `JS_SetRuntimeOpaque(rt, null)` **before** `JS_FreeContext`/`JS_FreeRuntime`, whose sweep runs finalizers. `qjs_finalizer` → `state_from_runtime` → `&*null`. `catch_unwind` does not catch SIGSEGV. Tests pass only because each one manually clears globals, GCs, and drains before dropping. | **Confirmed by reading.** Move the null-out after `JS_FreeRuntime`. |
| **P2-C3** | CRITICAL | `core/src/lib.rs:1266` | `core/`'s `CopyInCopyOut` arm **reads a detached buffer**: `detach_arraybuffer(v)` then `arraybuffer_copy_to(v, dst)`. **JavaScriptCore cannot implement this.** `transfer()` moves the bytes into a new private product; the original is unreadable. The arm exists *for* JSC and cannot be implemented by it. | **Confirmed.** This is the JSC exit gate firing one phase early, which is exactly what block 02 §1 said would be worth more than the slice. Block 02 → **A13** rewritten: one primitive, `detach_arraybuffer(cx, value, out: Option<&mut [u8]>)`, which each engine implements end-to-end. `arraybuffer_copy_to` is deleted from the trait. |
| **P2-M1** | MAJOR | `adapters/quickjs/src/lib.rs:641` | The A12 guard **cannot fire**. `JS_GetArrayBuffer` throws `TypeErrorDetachedArrayBuffer` on a detached buffer and sets `*psize = 0` on *every* failure path, so `arraybuffer_len` returns `Some(0)` for detached, for non-buffers, and for exceptions alike. The throw is swallowed with `let _ =` and never cleared, leaving a stale exception on the context after every successful `unmap()`. | **Confirmed at source** (`quickjs.c`). Two reviewers converged: one proved the guard dead by deleting it, the other found the mechanism. Block 02 → **A26**. |
| **P2-M2** | MAJOR | `adapters/quickjs/src/lib.rs:219` | Nothing drains the release queue after the final GC. Every buffer alive at teardown leaks its native handle and its parent-device reference. A host that cycles script VMs leaks per buffer. | **Confirmed.** Drain between `JS_FreeRuntime` and dropping `State`. |
| **P2-M3** | MAJOR | `core/src/mock.rs` | **Three new mirrors, all in the capabilities Phase 2 added.** (a) `new_external_arraybuffer` *copies* into a `Vec`, so `ZeroCopyDetach` is not zero-copy and **nowhere is it asserted that a write through a mapped range reaches backend memory** — A17 was reported met and is not. (b) `duplicate_value`/`release_value` are identity and no-op with no balance check, so a held-value leak is invisible; this is R22's class, one capability up, and it bites JSC too (`JSValueProtect` needs its `Unprotect`). (c) `detach` and `arraybuffer_len` are coupled, so A12's hazard is inexpressible. | **Confirmed by reading.** Block 02 → **A24**. |
| **P2-M4** | MAJOR | `core/src/mock.rs:890` | `r23_heap_property_values_are_reclaimed_by_scope` asserts `rt.reclaimed_values() == 4` — the **mock's own counter**, incremented by the mock's `Scope::drop`. To see it red you break the mock, not `core/`. **The third tautology**, and the same shape as the retracted P0-M3 and P1-M3. | **Confirmed.** The project has now caught itself doing this three times. |
| **P2-M5** | MAJOR | tests | Claimed-or-required, absent: two concurrent async ops (**A7 says "Test it"**, and this is Phase 0's CRITICAL); two ranges detached by one `unmap` on a **real** engine; `Deferred` settled twice; runtime dropped with a pending `mapAsync`; and the R19 red demonstrations for A12, A15, A11 — which the implementing agent reported red-then-green but which **exist nowhere in the tree**. | **Confirmed.** A guard whose red state cannot be reproduced from the tree is on the honour system. |
| **P2-M6** | MAJOR | several | `unsafe fn` `# Safety` docs restate the signature. Worst: `new_external_arraybuffer` says the memory must outlive the `ArrayBuffer`, while the code relies on the opposite (detach precedes `wgpuBufferUnmap`). A precondition nobody can check is decoration. | Handed over. |
| P2-m1 | MINOR | `core/src/lib.rs` | Reviewers disagreed on whether `_Error` and `_Aborted` collapse. Adjudicate against the code; A9 forbids collapsing. The rejection value is a bare string, not a `GPUError` — in-spec until Phase 6, but record it. | Handed over. |
| P2-m2 | MINOR | `core/src/lib.rs:708` | `destroy()` routes through `detach_all_ranges`, which for the copy arm **flushes possibly-stale script bytes back** into a buffer being destroyed. `destroy()` should discard. | Handed over. |
| P2-m3 | MINOR | `core/src/lib.rs:1262` | `detach_all_ranges`' error paths drop duplicated range values without releasing them. | Handed over. |
| P2-m4 | MINOR | `adapters/quickjs/src/lib.rs:1225` | Uncommented `#[allow(clippy::type_complexity)]`. Not a soundness lint, but `CLAUDE.md` asks for the *why*. | Handed over. |
| P2-m5 | MINOR | `core/src/lib.rs:793` | Dead guard: `mode` is already bounded by `enforce_u32`, so `if mode > WEBIDL_U32_MAX` cannot fire. Misleading about where the widening guard lives. | Handed over. |

### What Phase 2 actually earned

Recorded because a review that only lists defects misrepresents the phase.
Independently verified by two lenses:

- **The boundary is additive.** Every Phase 2 capability is a trait addition;
  `core/`'s Phase 1 logic is unchanged. `duplicate_value`/`release_value` map to
  `Protect`/`Unprotect` on JSC, so they are not QuickJS-shaped. **P2-C3 is not a
  counterexample to this** — it is a badly-chosen primitive pair, and the fix is
  still an addition.
- **`with_async_scope` is a real fix to a real leak**, and `Context`'s scope is no
  longer `Option`.
- **The `tick()` three-queue contract is strongly tested.**
  `process_events_without_microtasks_does_not_resume_await`,
  `tick_reports_unhandled_rejection_after_microtasks_drain` and
  `tick_ignores_rejection_handled_before_drain_finishes` would each fail against a
  no-op. A22's late-`.catch()` case is genuinely covered.
- **The numeric guards are right.** `enforce_u64` uses `>= 2^64` after the Phase 1
  correction; `optional_gpu_size_to_usize` rejects before narrowing; `offset = 2^32`
  is tested on this 64-bit host.
- **No `#[allow]` sits on a soundness lint in hand-written code** — the Phase 1
  defect stayed fixed.

### The pattern worth naming

Every CRITICAL this phase came from the same place: **a primitive whose contract
was set by what QuickJS happened to allow, and a mock built to match `core/`
rather than to match the engines.**

P2-C3 is the clearest. `core/` asked "detach, then read the detached buffer",
QuickJS shrugged, and the mock — written by the same hand, in the same session —
said yes. JSC would have said no, and would have said it in Phase 3, after forty
generated interfaces were resting on the answer.

The rule that follows, now A13's closing paragraph: **for every trait primitive,
ask what JavaScriptCore does — before the mock answers for it.**

And the rule that keeps catching us, now three times over: **a test that asserts
the code's own bookkeeping is not a test.** P0-M3 asserted our release-call log.
P1-M3 asserted our `add_ref` counter. P2-M4 asserts the mock's reclaim counter.
Each was written by someone who had just read the retraction of the last one.

### Two more CRITICALs, found by writing the tests the rules demanded

**P2-C4 — `core/` holds engine values; QuickJS traced none of them.**
`duplicate_value(cx, range.value)` keeps each mapped range's `ArrayBuffer` alive in
the buffer payload, while the class declared `gc_mark: None`. The value lives by
refcount and is invisible to the collector, so `JS_FreeRuntime` finds live objects
and **aborts**. A second mapped range surviving to teardown kills the process.

Block 01's **R7** required exactly this tracing. Phase 1's review marked it
**"satisfied (vacuous)"** — correctly, at the time, because no wrapper held a JS
value. Phase 2 created one, and nobody re-asked.

> **A rule can be written, correctly discharged as vacuous, and silently come back
> into force.** When a phase adds a capability, re-ask which "not applicable" rules
> just became applicable. Nothing in the process does this for you.

**P2-C5 — a pending async request survives into teardown.** A `Deferred` owns two
resolving functions inside a `Box` the WebGPU callback owns via `into_raw`. If the
callback never fires, those values are never freed and `JS_FreeRuntime` aborts.
Freeing them *after* is worse: they point into a dead runtime. Block 02 → **A28**.

Both were found because the agent **wrote the tests the rules demanded instead of
asserting they would pass** — and then reported the aborts instead of hiding them.
It did, however, delete the tests. **A test that finds a bug is not deleted; the
bug is fixed.** Both now land.

**Red demonstrations.** I ran the ones the agent could not: reintroducing the
premature `JS_SetRuntimeOpaque(null)` aborts the teardown test with `SIGABRT`
(P2-C2); neutering `qjs_gc_mark`'s body aborts the adapter binary and restoring it
returns 18 passing tests (P2-C4). The agent stated plainly that it had not observed
the aborts before fixing them, rather than claiming a quote it did not capture.
That is the right answer to give.

### The one place `core/` bent toward an engine

Wiring `trace_payload` forced a change in `core/`'s **data layout**, not its logic:
the mapped-range values sat behind `BufferState`'s `Mutex`, and `gc_mark` runs
inside the collector and cannot take a lock the mutator may hold, so `core/` grew a
lock-free side list.

`trace_payload` is a QuickJS concept — JSC needs no tracing callback, only
`protect`/`unprotect`. The cleaner primitive is for `core/` never to hold an
`E::Value`: ask the engine to *associate* the `ArrayBuffer` with the wrapper
(`associate_value` / `take_associated`), which QuickJS puts in a traced slot and
JSC protects. Then the hook, the side list, and the whole leak class disappear.

**Deliberately not redesigned now.** With one engine in the tree, shaping the
abstraction around it is exactly the error that produced P2-C3 and two mirrored
mocks. `trace_payload` is implementable by both — a no-op on JSC, wasteful but
correct. **Phase 3 decides, when JavaScriptCore has a vote.** The debt is recorded
in block 02 → A27 rather than left to be rediscovered.

### Gate — PASSED. Phase 2 is COMPLETE.

Gates re-run directly: `core` with the backend env var **unset** EXIT=0 (28 tests);
`cargo test --workspace` EXIT=0; `cargo clippy --workspace --all-targets -D
warnings` EXIT=0; both detach spikes EXIT=0.

Verified fixed: P2-C1 … P2-C5, P2-M1 … P2-M6, P2-m1 … P2-m5.
Block 02 gained A13 (rewritten), A24, A25, A26, A27, A28; A11 and A26 were revised
after the implementing agent showed each was wrong as first written.

### What this phase taught, in one line each

- **The mock must be shaped to the engines, not to `core/`.** P2-C3 shipped because
  `core/` asked "detach, then read the detached buffer", QuickJS shrugged, and the
  mock — written by the same hand in the same session — said yes.
- **A predicate that returns the same value for "zero" and "no answer" cannot guard
  anything.** `arraybuffer_len` returned `Some(0)` for detached, for non-buffers,
  and for exceptions alike.
- **Vacuous rules come back.** R7 was correctly not-applicable, then silently was.
- **Write the test, then run it red.** Two CRITICALs existed only because someone
  finally wrote a test that the rules had demanded for a phase and a half.
- **A test that asserts your own bookkeeping is not a test.** Three times now.

---

## Phase 1 Review — 2026-07-09

Three fresh no-context reviewers again, lenses tuned to the phase: correctness /
FFI soundness; block-spec compliance rule-by-rule (R1–R21); and **"is the mock a
real test, or a mirror?"**

The third lens was pointed at the one question Phase 1 exists to answer, and it
earned its place: it found the CRITICAL, and neither of the others came near it.

### Findings and triage

| ID | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| **P1-C1** | CRITICAL | `core/src/lib.rs` conversion path; `adapters/quickjs/src/lib.rs:198` | QuickJS's `JS_GetPropertyStr` returns an **owned** value. `convert_buffer_descriptor` reads four properties per call and frees none. The trait has no value-release primitive. `createBuffer({size, usage, label: "x"})` leaks a JSString **every call**. The mock is GC-backed and structurally cannot see it; `buffer_slice.js` sets `label` through the setter, so the one real-engine test never reads a heap-valued property. | **Confirmed at source** (`quickjs.c: JS_GetPropertyStr → JS_GetProperty`, which dups). Block 01 → **R22**, **R23**. Handed to the coding agent. |
| **P1-M1** | MAJOR | `adapters/quickjs/src/lib.rs:363, 411` | The adapter dispatches on hardcoded `("GPUDevice","createBuffer")` string pairs; the generic magic-indexed path is unreachable. `ClassSpec`'s data-driven half is therefore **unexercised**, and Phase 4's ~40 interfaces would each demand new match arms in every adapter. | **Confirmed.** Block 01 → **R24**. Traced to the unverified `magic == 0` claim. Handed over. |
| **P1-M2** | MAJOR | `core/src/lib.rs:641` | `enforce_u64`'s guard is `n > u64::MAX as f64`, but `u64::MAX as f64` rounds **up** to `2^64`. `size: 2**64` — valid JS, exactly f64-representable — passes the check and `n as u64` saturates silently to `2^64-1`. R8 requires `TypeError`. | **Confirmed by direct measurement** (see adjudication below). Handed over. |
| **P1-M3** | MAJOR | `core/src/mock.rs` | The `AddRef` **acquire** side of R4/R5 is never asserted. The mock counts `device_add_refs`, but no test compares it. Drop `createBuffer`'s second `wgpuDeviceAddRef` and every existing test still passes — while a real backend under-refcounts the host's device. | **Confirmed.** Two lenses found it independently. Handed over. |
| **P1-M4** | MAJOR | `core/src/mock.rs:632` | The R5 guard does `let _ = ptr::read_volatile(...)` and discards the value, so on the default gate a reversed release order **passes**. It only bites under ASan. R19 says a guard never seen red is a test of nothing. | **Confirmed, with a correction to the proposed fix** — see below. Handed over. |
| **P1-M5** | MAJOR | `adapters/quickjs/src/lib.rs` | The `catch_unwind` seam — safety-critical, principle 8 — is exercised by **no test**. R19 applies to it too. | **Confirmed.** Handed over. |
| P1-m1 | MINOR | `core/src/lib.rs:678–720` | `Box::leak`s a fresh `ClassSpec` on **every** `wrap_device`, before the adapter's idempotency check discards it. Unbounded across device re-adoption (device loss / recreation). | Handed over. Use a `OnceLock`. |
| P1-m2 | MINOR | `core/src/mock.rs` | `NaN` / `Infinity` rejection is implemented but untested. | Handed over. |
| P1-m3 | MINOR | `adapters/quickjs/tests/scripts/buffer_slice.js` | The one real-engine test would pass with a no-op `destroy()`, and asserts nothing about `mappedAtCreation`, default `label`, `usage` widening, or any rejection case. | Handed over. |
| P1-m4 | MINOR | `engine-boundary.md` Q4 | "16 tests, one per rule R8–R15" overstates: `r15_..._getters_are_synchronous` tests neither `TypeError` propagation nor the unwind catch, which is R15's content. `wrap_device`'s null guard is untested. | Fixed (Claude): Q4 rewritten. |

### Where the reviewers disagreed, and who was right

**P1-M2, the `2^64` boundary.** The correctness lens called it MAJOR. The
compliance lens called it MINOR, reasoning that it sits "inside R11's documented
`Number` precision limit." **The compliance lens is wrong**, and the disagreement
is settled by one measurement:

```
u64::MAX as f64          = 18446744073709551616.0   // == 2^64, rounded UP
(2^64 > u64::MAX as f64) = false                    // guard slips
2^64 as u64              = 18446744073709551615     // silent saturation
u32::MAX as f64 exact?   = true                     // usage guard is correct by luck
```

`2^64` is **exactly** f64-representable, so this is not a precision limit — it is
a plain off-by-one in a range check, on a value WebIDL requires be rejected. R11
covers values that lose exactness *before* the binding sees them; this one does
not. The block spec now forbids the `> u64::MAX as f64` idiom by name.

### P1-M4: the reviewer's fix would not have worked

The correctness lens proposed asserting on the `marker` value. That is not
enough: freed memory usually still holds its old bytes, so a reversed order can
read `0xfeed_face` and pass anyway. **Make the guard deterministic instead:** have
`parent_release` overwrite the marker with a poison value *before* freeing. Then
child-after-parent reads poison and the assertion fails on the ordinary
`cargo test` gate, with no sanitizer. A guard that needs ASan to bite is a guard
that does not run in CI.

Note the guard's *subject* is sound — it enqueues a real
`ReleaseRequest::BufferWithDeviceRef` and drains it, so `core/`'s ordering logic
is genuinely under test. Only the detection was weak. And the Phase 1
implementation agent **did** demonstrate it red under ASan before trusting it, as
R19 demands; what is missing is that the ordinary gate cannot.

### The finding that matters

P1-C1 is the most important result of Phase 1, and it inverts the phase's
headline.

The design bet is "one `core/`, many engines." Phase 1 tested `core/` against a
mock whose values are garbage-collected — the **same model JavaScriptCore uses**,
and **not** the model of QuickJS, the Tier-1 engine we actually ship. So `core/`
was written to a value model only one of the two engines has, and the test that
was supposed to prove engine-agnosticism was the very thing that concealed the
engine-specific obligation.

The exit gate we designed for Phase 3 — "wiring JSC must require zero `core/`
logic changes" — would **not** have caught this, because JSC is the engine the
mock resembles. The gate was pointed at the wrong engine.

**The bet survives, and the mechanism that saves it is the GAT this project had
just dismissed as ceremony.** `E::Context<'a>` is engine-defined and already
threaded through every conversion, so QuickJS can carry a per-call handle scope
there: `get_property` registers each owned value; the scope frees them on drop.
`core/` does not change. `E::Value: Copy` survives. No `free_value` on the trait.

The reviewer concluded the fix "is not additive to `core/`" and was wrong about
that — but was right about everything that mattered, and would not have found it
at all had the lens been "check the rules" rather than "is the mock a mirror."

**R23 is the durable lesson: a mock more forgiving than production is not a test.
When engines disagree about an obligation, the mock takes the union.**

### Gate — PASSED. Phase 1 is COMPLETE.

Gates re-run directly by Claude, not taken on the agent's word:

```
core, backend env var UNSET, no backend feature   EXIT=0   19 tests
cargo test  --workspace                            EXIT=0   10 suites
cargo clippy --workspace --all-targets -D warnings EXIT=0
spikes/jsc-detach, spikes/quickjs-detach           EXIT=0
R5 ASan guard                                      EXIT=0
```

**P1-C1 is fixed the way the boundary demanded.** QuickJS's `Context<'a>` now
carries a per-call `Scope`; `get_property`, `new_instance`, `number` and `string`
register their owned values there, the callback's return value is `escape`d, and
`Drop` frees the rest. **`core/`'s signatures and logic did not change.**
`type Value: Copy` and `type Context<'a>: Copy` are intact and the trait has no
`free_value` — verified. The GAT earned its keep.

**P1-M1's root cause was found by opening the file.** QuickJS stores a C
function's magic as an **`int16_t`** (`quickjs.c:1101`). Phase 1's encoding packed
a class id into the high bits, which a 16-bit field silently truncates. `magic`
was never broken; the encoding did not fit. Refusing to record "QuickJS delivers
`magic == 0`" as fact was right — it was false. **Failing to demand the
root-cause was the actual mistake**, and it cost a hardcoded dispatch table that
would have metastasised across forty interfaces in Phase 4.

**A deadlock surfaced the moment the generic path went live**, and it is a
property of the boundary rather than of QuickJS: the method callback held the
class-registry mutex while `core/` re-entered the adapter through `payload`. The
call graph is re-entrant *through* the boundary. Now block 01 → **R25**. It will
bite JavaScriptCore identically.

**The negative demonstrations were run, and reported.** Reversing the R5 release
order fails on the plain `cargo test` gate now (`marker` reads the `0xdead_beef`
poison written before the free — assertion, not sanitizer). Making the mock's
scope leaky fails `r23_heap_property_values_are_reclaimed_by_scope` with
`left: 0, right: 4` — the four descriptor properties. Both guards have been seen
red.

Verified fixed: P1-C1, P1-M1 through P1-M5, P1-m1 through P1-m4.

### One accepted deviation, recorded

`mappedAtCreation: "false"` — the case that proves `ToBoolean` — is exercised in
`core/` against the mock, but **not** in the real-engine script. `ToBoolean("false")`
is `true`, which creates a mapped buffer, and this slice has no `unmap()` to
release it. `buffer_slice.js` uses `mappedAtCreation: ""` instead, which still
drives QuickJS's real property read and `ToBoolean` while producing an unmapped
buffer. Consequence: **no real-engine test creates a mapped buffer.** That gap
closes when `mapAsync`/`unmap` land in Phase 2.

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
- ~~**Renaming the bare `ffi` crate.**~~ **CLOSED in Phase 1**, as planned, and
  for a sharper reason than anticipated: the first slice named its crate `core`,
  which shadows the **sysroot `core` crate** for every dependent. Both are now
  `webgpu-native-js-{core,ffi}` (block 01 → R21). The gate commands in
  `workflow.md` moved with them.
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
