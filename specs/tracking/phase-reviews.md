# Tracking: Phase Reviews

Per `specs/reference/workflow.md` → "Phase Review (mandatory — Clean Review Then
Fix)". Findings, triage decisions, fixes, and gate results live here.

---

## Block 03 Review — 2026-07-10

Three fresh no-context reviewers. **No CRITICAL.** Six MAJOR, and two of them are
mine.

The central bet held under real pressure: ten interfaces with struct chaining,
dictionary arrays, string enums, nullable-vs-non-null strings and stored handles,
and **block 03 added zero methods to `JsEngine` and zero `core/` logic for
QuickJS**. Two reviewers verified that independently. The arena is address-stable
(`Box<[T]>` behind a `Vec`, so `Vec` reallocation moves the boxes, not the data).
Partial sequence failure leaks nothing. All 19 `unsafe impl Send`/`Sync` carry
accurate `// SAFETY:` comments, checked type by type.

**And the mock was not a mirror this time.** The conversion tests inspect the real
produced C structs — `chain.sType`, `native.entries`, `entryPoint.data/length` —
not a Rust shadow the conversion also wrote.

### Findings and triage

| ID | Sev | Where | Finding | Disposition |
|---|---|---|---|---|
| **B3-M1** | MAJOR | `core/src/lib.rs:1499` | `wgpuDeviceGetQueue` is documented `@ref ReturnedWithOwnership` — it **already returns an owned reference**. `device_queue_get` takes it and calls `queue_add_ref` on top; `finalize_queue` releases once. **One native ref leaks per `device.queue` read.** The mock's queue add/release are counter-less no-ops, so no test sees it. | **Confirmed at source** (`webgpu.h:6360`, `doc/articles/Ownership.md:12`). Handed over. |
| **B3-M2** | MAJOR | `adapters/quickjs/src/lib.rs:1564` | `arraybuffer_free` early-returns unless `ptr` is null. On the **unmap** path QuickJS calls it twice (real pointer, then `NULL`) and the release fires. On the **GC-only** path — script drops a mapped range and never calls `unmap()` — it is called **once, with a real pointer**, and the `buffer_add_ref` and owner `Box` leak **forever**. `CLAUDE.md` principle 5 promises *leak-until-GC, not leak-forever*. `mapped_range_survives_after_buffer_wrapper_is_collected` walks that path and asserts nothing about releases. | **Confirmed.** Handed over. |
| **B3-M3** | MAJOR | `core/src/mock.rs:1415` | **The fifth tautology.** `b7_write_buffer_rejects_size_that_would_truncate_on_32_bit_hosts` passes with its guard removed: `end > data.len()` then throws the same `TypeError: size` the test asserts. The named B18 demonstration is dead; the real coverage is incidental, via `a21`. | **Reproduced by Claude**: guard removed → `b7` **passes**, `a21` **fails**. Unlike `b8`, this one was **undisclosed**. Handed over. |
| **B3-M4** | MAJOR | block 03 §7 | §7 claimed command-buffer single-use is *"a wrapper-state question, not a refcount one. See B10."* **`CommandBufferPayload` has no `consumed` flag** and `queue_submit` marks nothing. A script can submit the same `GPUCommandBuffer` twice. | **My overclaim.** I wrote a sentence describing work nobody had done. Retracted in §7; **B19** now requires it. Handed over. |
| **B3-M5** | MAJOR | `core/src/lib.rs:2649` | `sequence_len`/`sequence_item` read `.length` and stringified indices. WebIDL's `sequence<T>` conversion is **iterator-based**. So `{length:2, 0:a, 1:b}` is accepted where WebIDL rejects, and a `Set` or generator is rejected where WebIDL accepts. **Both directions wrong; a green suite sees neither.** §7's *"no primitive was needed"* was an overclaim — mine. | **Confirmed.** §7 rewritten; **B20** records the deviation and pins it with tests. The primitive is chosen in Phase 4, with JSC voting — the same reasoning that deferred `associate_value`. |
| **B3-M6** | MAJOR | `core/src/lib.rs:309` | `.expect("just-pushed WGSL source")` in library code. Unreachable today and caught by the callback's `catch_unwind` before any C boundary — but `CLAUDE.md` principle 8 admits one exception and this is not it. Its sibling arena allocators return `map_or(&[][..], …)`. | Handed over. |
| B3-m1 | MINOR | `core/src/lib.rs:2685, 2772` | `binding` is read with `get_property(..).ok()`, substituting `0` when the getter **throws**. WebIDL propagates. Uniform across engines, so not a boundary defect. | Handed over. |
| B3-m2 | MINOR | `core/src/lib.rs` (`detach_all_ranges`) | The `CopyInCopyOut` arm copies back on **read** mappings too — it never checks `map_mode` — contradicting its own SAFETY comment. Dead for QuickJS. **Live the day JSC lands.** | Handed over. Fix now, while it is free. |
| B3-m3 | MINOR | `core/src/lib.rs` (`TracedValues`) | `unsafe impl Sync` is sound only under QuickJS's on-thread GC. A JSC any-thread finalizer breaks it. | Recorded as a **Phase 3 landmine**; the SAFETY comment must say so. |
| B3-m4 | MINOR | `engine-boundary.md` | B15 asks that the synchronous-exception divergence be recorded "alongside R13's". Only `createBuffer` is. The eight new constructors are not — and for the seven **non-nullable** ones a backend validation failure returns an *invalid* handle the binding wraps silently, surfacing nowhere until Phase 6. | Fixed by Claude below. |
| B3-m5 | MINOR | every `device_create_*` | On the `E::new_instance` error path (engine OOM) the `AddRef`'d handles and the fresh native object leak — the payload `Box` drops without its finalizer. | Handed over. |

### The "flake" that was another reviewer's experiment

The compliance lens reported a **non-deterministic failure**: on its first run,
`a21_rejects_offsets_that_would_truncate_on_32_bit_hosts` failed with *"mapAsync
offset=2^32 must be rejected"*, then passed fifty-plus subsequent runs. It could
not root-cause it and, correctly, refused to wave it away.

It is not a flake, and the guard is fine. **I ran the suite forty times: zero
failures.** The explanation is that the three lenses shared one working tree, and
the adversarial lens was proving a guard **by deleting it** — and the assertion it
observed is *exactly and only* the one that fails when that guard is gone. I
reproduced that myself.

**This is a defect in my review process, not in the product.** Deleting a guard to
see it go red is the single most valuable thing a Clean Review does — it is how the
fifth tautology was found. It must not be paid for with a phantom defect in another
reviewer's report. `workflow.md` now requires an isolated `git worktree` for any
reviewer licensed to edit the tree.

### Which vacuous rules came back

The question that caught Phase 2's two aborts, asked again. **R7 did not** become
applicable: all nine new payloads hold `WGPU*` handles only, no `E::Value`. But
**R25 did** — block 03 introduced `Mutex<CommandEncoderState>` and
`Mutex<ComputePassState>`, and `live_compute_pass` takes nested locks. A reviewer
checked that every guard is dropped before re-entering `core/` and that the lock
order is consistent, so R25 is **satisfied — and now newly applicable, recorded
rather than assumed**.

### Two of the six are mine

B3-M4 and B3-M5 are both **overclaims in a document I wrote**, in the section
titled *"Answers this block produced"*. One described work nobody had done; the
other reported a shortcut as a design conclusion. Neither was caught by the two
lenses reading the code — only by the lens told to hunt for claims the evidence
does not support.

That is the third phase running in which the most valuable finding came from
asking *"what does green not mean?"* rather than *"is the code correct?"*

### Gate — PASSED. Block 03 is COMPLETE.

Gates re-run directly by Claude: `core` with the backend env var **unset** EXIT=0
(36 tests); `cargo test --workspace` EXIT=0 (`quickjs-adapter` 26); `cargo clippy
--workspace --all-targets -- -D warnings` EXIT=0; both detach spikes EXIT=0. No
`#[ignore]` anywhere.

**Both red demonstrations were re-run, one of them by me.** Removing the
`WEBIDL_U32_MAX` guard now makes `b7` **fail** — it passed before the rewrite, and
that is the difference between a test and a decoration. The mapped-range leak test
showed `left: 1, right: 2` before the fix.

**The `ReturnedWithOwnership` audit found exactly one wrong site.** `device.queue`
double-counted; `requestAdapter`, `requestDevice`, and all eight `createXxx`
returns were already correct. The mock now **counts** queue references and asserts
the balance, so the class of bug that hid behind counter-less no-op stubs cannot
hide again.

The `arraybuffer_free` fix carries explicit state rather than guessing from the
pointer: a flag set around our own `JS_DetachArrayBuffer`, plus a `released` flag
on the owner. The detach path releases on the first call and frees the box on the
`NULL` one; the GC-only path releases and frees on its single call. Both of
QuickJS's sequences are covered without inferring intent from `ptr`.

B19 (command-buffer single-use) is implemented and tested on a real engine. B20 is
**tests only, deliberately**: an array-like is accepted, a `Set` is rejected, both
documented as known deviations from WebIDL. Fixing them means adding an iteration
primitive to `JsEngine`, and choosing its shape with one engine in the tree is the
error that produced P2-C3. **Phase 4 decides, when codegen emits sequence
conversions and JavaScriptCore has a vote.**

Verified fixed: B3-M1 … B3-M6, B3-m1 … B3-m5.

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

### The fourth tautology, caught by clippy

The A7 "red demonstration" that landed with the fixes was
`a7_red_demo_overwritten_userdata_loses_first_async_request`. It built a
single-slot design **in the test's own local `Option`**, overwrote it, and asserted
the local behaved as written. It never called into `core/`. It would have passed
whatever `core/` did — and it tested the wrong hazard besides: A7's failure mode is
a **use-after-free**, not an unresolved promise.

`cargo clippy -D warnings` found it, as `value assigned to
single_userdata_slot is never read`. **The lint was pointing at the test being
wrong, not at a style nit.** It is now deleted and replaced by
`a7_two_concurrent_map_async_operations_settle_independently`, which drives two
outstanding `mapAsync` calls through `core/` and shows each settling its own
promise.

That is the fourth: P0-M3 asserted our release-call log, P1-M3 our `add_ref`
counter, P2-M4 the mock's reclaim counter, P2-M7 the test's own local variable.

**A7's red demonstration is not re-run, and that is deliberate.** A deterministic
assertion is impossible — the hazard is a dangling `userdata1`. It was already seen
red, under ASan, in **Phase 0** (`phase-reviews.md` → Phase 0, P0-C2:
`heap-use-after-free … in core::cell::Cell::get`), for exactly this pattern and for
exactly this reason. Citing that demonstration is honest; manufacturing a second
one that passes for the wrong reason is not.

### Gate — PASSED. Phase 2 is COMPLETE.

Gates re-run directly by Claude: `core` with the backend env var **unset** EXIT=0
(27 tests); `cargo test --workspace` EXIT=0; `cargo clippy --workspace --all-targets
-- -D warnings` EXIT=0; both detach spikes EXIT=0.

**Correction to the record.** The commit that declared Phase 2 complete stated
`clippy -D warnings EXIT=0`. It did not. Clippy was failing on the tautological test
above, and the claim was written from the previous run rather than from the one
being reported. The gate is green now, and this note stays because a project that
retracts its own tautologies should also retract its own unverified gate claims.

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

---

## Design Review — 2026-07-10 (whole-tree, between Phase 3 parts 1 and 2)

Occasion: the project owner switched the orchestrating model and asked for a
review of the overall design and codebase before dispatching the JavaScriptCore
adapter. Not a Phase Review — Phase 3 is mid-flight — but run with the same
discipline: four no-context lenses over the whole tree at `0fdfd98`, one of them
(the deletion experimenter) in an isolated worktree per the workflow rule its own
predecessor caused.

Lenses: **soundness** (unsafe/FFI/refcounts), **architecture** (every `JsEngine`
method walked against JSC's public C API), **deletion experiments** (16 run, 12
red), **spec-vs-code** (77 rules audited, 58 fully covered).

### Findings — accepted

| ID | Sev | Finding | Where |
|---|---|---|---|
| DR-C1 | CRITICAL | B8 half-implemented: compute pipeline retains neither module nor layout; bind group does not retain its layout. The rule's own list has three items; one shipped, and only that one had a test. | core/src/lib.rs:1988, 2035 |
| DR-M1 | MAJOR | Two incompatible error conventions in one adapter: `get_property` returns the `JS_EXCEPTION` sentinel (pending set); `to_f64`/`to_str` return the exception object (pending cleared). `catch_callback` understands only the first, so a coercion failure is **returned to script as a value** — `createBuffer({size: 10n})` hands back a `TypeError` object instead of throwing. On the settlement path the sentinel becomes a rejection reason. Now R26. | adapters/quickjs/src/lib.rs:563–590, 1211 |
| DR-M2 | MAJOR | `core/` violates its own R25: `buffer_get_mapped_range` converts `size` (`to_f64` → user `valueOf` → arbitrary script) inside the payload mutex — self-deadlock reachable from an honest script. Now R27. | core/src/lib.rs:1541–1551 |
| DR-M3 | MAJOR | `visibility` read failure swallowed by `.ok()` → default 0; `binding`/`visibility` are *required* IDL members and must TypeError when absent. The mock's infallible `get_property` made this invisible (R23 union unmet). | core/src/lib.rs:2923–2928 |
| DR-M4 | MAJOR | `label: null` converts to `""` via `optional_non_null_string` but to `"null"` in `convert_buffer_descriptor` — two behaviours, one tree; the `""` arm misapplies the C-side B4 rule one layer up and a test pins the wrong behaviour. B4 clarified. | core/src/lib.rs:2830–2860 |
| DR-M5 | MAJOR | The barred `usize`-smuggling pattern (CLAUDE.md, block 01 → R18) reappears: deferred-slot pointers stored as `usize` to make the container `Send`, no SAFETY comment. | adapters/quickjs/src/lib.rs:410–422 |
| DR-M6 | MAJOR | `ReleaseQueue` FIFO order has no failing test (deletion experiment E7: LIFO inversion, 63 tests green). The spike tests its own private queue, not core's. | core/src/lib.rs:879 |
| DR-M7 | MAJOR | The one-frame settlement batching is unfalsifiable under QuickJS (E8: unbatching left everything green, including the test named for it). The property must be assertable in core against the mock (call-count), and the ordering must be written once — now A30 (core-owned tick skeleton, `drain_microtasks` becomes the trait method A18 already promised). | core/src/lib.rs:589–594; adapters/quickjs/src/lib.rs:217–250 |
| DR-M8 | MAJOR | Mock gaps that hid real defects (R23): `get_property` cannot fail (hid DR-M3); `new_external_arraybuffer` ignores `owner` (A25 pairing unverifiable in core); coercions run no script (R27 deadlock unreachable); `settle_deferreds` call count unrecorded (DR-M7 unassertable). | core/src/mock.rs:309–325, 511–522 |
| DR-M9 | MAJOR | R23 scope-drop assertions are vacuous: `Scope::drop` counts but never asserts, and no test asserts the counters. | core/src/mock.rs:242–254 |
| DR-M10 | MAJOR | Real-engine A17 assertion missing: the mappedAtCreation test asserts detachment, never that the bytes reached the buffer; A14's error cases (getMappedRange unmapped/destroyed) have zero tests anywhere. | adapters/quickjs/src/lib.rs:2219 |
| DR-M11 | MAJOR | Block 04 was silent about three JSC traps found by walking the trait: `arraybuffer_copy` is an invariant-10 pinning trap on the `writeBuffer` path (now J19); unhandled rejections have no JSC hook, so J17 as written was unachievable (now J20); JSC finalizers may not call `JSValueUnprotect`, breaking the mapped-range value-release protocol (now J21). | specs/blocks/04-javascriptcore.md |
| DR-m1 | MINOR | `settle_deferreds` cold fallback branches free settlement values without `escape` → double-free via scope drop (trampoline-`None` and OOM arms only). | adapters/quickjs/src/lib.rs:751–779 |
| DR-m2 | MINOR | `SettlementQueue::release_pending` releases the deferred but leaks the native `WGPUAdapter`/`WGPUDevice` inside undrained settlements at teardown; ditto the `UnexpectedSettlementType` arm. | core/src/lib.rs:599–617 |
| DR-m3 | MINOR | Every WebGPU callback discards the backend's `WGPUStringView message`, substituting a fixed string — driver diagnostics never reach script. Folded into A9's non-deferred half. | core/src/lib.rs:2533–2626 |
| DR-m4 | MINOR | `Runtime::drop` never frees entries left in `unhandled_rejections` — currently unreachable (every tick drains) but one refactor from a teardown abort (found by E13's failure mode being a heap-leak SIGABRT, not the named test). | adapters/quickjs/src/lib.rs:268–292 |
| DR-m5 | MINOR | `Arena` is per-type pools, one `alloc_*` per array type — Phase 4 would edit core per descriptor. Generic `alloc_slice<T: Copy>` instead. | core/src/lib.rs:238–246 |
| DR-m6 | MINOR | `trace_payload`/finalizer both enumerate payload types by name in every adapter — multiplies engines × payloads under codegen. Core exports `trace_payload_values::<E>`/release twin; adapters call blind. | adapters/quickjs/src/lib.rs:685–693, 1225–1232 |
| DR-m7 | MINOR | Coverage: `unmap()` idempotence untested (A16); `arraybuffer_len` `None` arm untested (A26); `writeBuffer` 2^32 tested against the helper, not the method (B17); `r15_…` test mislabelled (tests getters, not R15); consumed-guard and mapped-range AddRef only caught downstream of core (principle 1). | various |
| DR-m8 | MINOR | `device.queue` mints a fresh wrapper + native ref per read — `[SameObject]` violated. Now B21. | core/src/lib.rs:1651 |
| DR-m9 | MINOR | `writeBuffer` rejects `ArrayBufferView`s (IDL: `BufferSource`). Now B22, deliberately deferred to a slice designed against JSC pinning. | adapters/quickjs/src/lib.rs:892 |
| DR-m10 | MINOR | Stale spec text: block 03 header cited A1–A23; block 02 preamble said A1–A20; A23 lacked its J1 supersession note; A18 listed `drain_microtasks` as landed when it never became a trait method; A11's protocol text predated the leak-forever fix. All corrected in this batch. | specs/blocks |

### Findings — dropped or deferred, with reasons

- **A9 `GPUError`-shaped reason** (spec-audit M2): deferred to Phase 6, where the
  error taxonomy lives; building a half-shaped object now would be rebuilt then.
  The non-deferred half (backend message passthrough, DR-m3) is fixed now.
  Recorded in A9 itself.
- **`transfer()` defeats ZeroCopyDetach keep-alive** (soundness): real, verified
  against `quickjs.c`, and unguardable without prototype patching. Documented as
  A31 under invariant 8 (trusted scripts), not fixed.
- **`ReleaseRequest` copies the ~45-pointer `GpuDispatch` by value** (design
  E-4): true; deferred until Phase 4 reshapes the release enum wholesale.
- **A22 multiplicity** (spec m7): only the first unhandled rejection surfaces
  per tick; the rest are freed. Acceptable diagnostic loss; noted here, not fixed.
- **Mock `Error = String` lacks pending-state modelling** (design C-3): dissolved
  by R26 — the contract is now "error is a value", which `String` models
  exactly. The mock was accidentally right.
- **E13/E14/E16 fail via SIGABRT/SIGTRAP rather than named assertions**
  (deletion-lens observation): the guards are covered; converting crashes to
  named assertions is folded into DR-m7's coverage work where cheap, otherwise
  accepted — a crash in CI is still red.

### Disposition

Spec fixes (this commit): R26, R27, A9 note, A11 rewrite, A18 correction, A23
supersession, A30, A31, B4 clarification, B8 record, B21, B22, J2 amendment,
J19–J21, header renumberings.

Production fixes: two handoffs to the coding agent — **DR-F1** (correctness:
DR-C1, DR-M1..M5, DR-m1..m4) and **DR-F2** (structure + tests: DR-M6..M10,
DR-m5..m8) — gates re-run and results appended below after each lands.
