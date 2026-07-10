# Block 04 — the JavaScriptCore adapter

Phase 3. Rules **J1–J21** (J19–J21 added by the 2026-07-10 review). Blocks 01
(R1–R27), 02 (A1–A31) and 03 (B1–B22) all still bind.

Every JSC fact below was **measured on this machine, today**, against the system
`JavaScriptCore.framework`, by a program linking the public C API. Where a claim
could not be measured, it is marked as an unverified premise. Reopen the headers;
do not restate from memory.

---

## 1. This is the exam

`CLAUDE.md`'s JSC exit gate: *wiring JavaScriptCore must require **zero changes to
`core/`'s logic** — only additive `JsEngine` methods. Non-trivial core churn means
the boundary was drawn wrong.*

The gate has already fired once, a phase early. Phase 2's `CopyInCopyOut` arm read
a **detached** buffer, which JSC cannot do; `core/` had been shaped to what QuickJS
tolerates, and the mock — written by the same hand — agreed. That cost one
primitive redesign. It would have cost a boundary redesign under forty generated
interfaces.

**It fires again here, and the finding is J4.** Read it before writing code.

---

## 2. Measured facts about JavaScriptCore

| # | Fact | How it was established |
|---|---|---|
| **F1** | **The public C API has no microtask/job pump.** No `drainMicrotasks`, no `JS_ExecutePendingJob` equivalent, no `queueMicrotask`. | Exhaustive grep of every public header. |
| **F2** | **Microtasks drain when the JS stack empties.** Calling a promise's `resolve` from C runs its `.then()` **before the C call returns**. | Program: resolve from C, then read `globalThis.ran` with `JSObjectGetProperty` (no JS executed). It was already `true`. |
| **F3** | **Two separate C→JS resolve calls give two checkpoints.** Settling both inside **one** JS frame gives one, matching QuickJS. | Program: log order was `then1,then2` for separate calls, `settle1,settle2,then1,then2` through a trampoline. |
| **F4** | **`JSClassDefinition` has no `gc_mark`.** Only `initialize` and `finalize`. | `JSObjectRef.h:355–356`. |
| **F5** | **`JSGarbageCollect` does not run finalizers.** An unreferenced object with a `finalize` callback survives four calls and is finalized at `JSGlobalContextRelease`, even from a returned `noinline` frame with the machine stack overwritten. | Phase 0, `release-queue.md` → R3. |
| **F6** | **No ArrayBuffer detach in the C API.** `ArrayBuffer.prototype.transfer()` detaches an **engine-owned** buffer reliably; on an **external** one it is not dependable. Taking a C bytes pointer (`JSObjectGetArrayBufferBytesPtr` *or* `JSObjectGetTypedArrayBytesPtr`) invokes `pinAndLock()` and **permanently, silently** disables detach. `JSObjectGetArrayBufferByteLength` does **not** pin. | Phase 0, `engine-boundary.md` → Q1, Q1b (E5, E6, E12). |
| **F7** | **`Symbol.iterator` is reachable without `eval`.** Two string property reads (`globalThis.Symbol`, then `.iterator`) yield a symbol `JSValueRef`; `JSObjectGetPropertyForKey` accepts it. A `Set` has it; an array-like does not. Full iteration works from pure C. | Program: iterated `new Set([10,20])` to completion; `JSObjectHasPropertyForKey` returned 1 for the `Set` and 0 for `{length:2,0:1,1:2}`. |
| **F8** | **LGPL-2.1.** Dynamically link the system framework. macOS and iOS only; Android and Windows are unsupported (`CLAUDE.md` → Engine support tiers). | `engine-boundary.md` → Q2a. |

**The any-thread-finalizer premise is a documented contract, not folklore.**
*(Upgraded 2026-07-10 by the Phase Review, which found the F1 header sweep had
missed it.)* `JSObjectRef.h` states outright: *"An object may be finalized on
any thread."* F5 still means it **cannot be provoked** — the project has never
observed an off-main-thread finalizer — but the design obligation now rests on
the header's own words, not on an assumption. Invariant 4 and J21 are load-bearing
against a documented behaviour.

| **F9** | **`JSValueIsBigInt` is `API_AVAILABLE(macos(15.0), ios(18.0))` and the adapter hard-links it**, so macOS 15 / iOS 18 is the JSC adapter's deployment floor (older systems dyld-abort at load). Measured before accepting: `JSValueToNumber` on a BigInt does **not** raise a TypeError through the exception out-param, so the symbol cannot be dropped without losing the WebIDL BigInt rejection the parity script pins. | Deletion experiment during the Phase 3 review MINOR tier. |

---

## 3. Rules

### The parity finding

**J1 — a WebGPU callback must not touch the engine.** It records its result; the
`tick()` thread settles.

Today, `wgpuInstanceProcessEvents` invokes a callback that calls `resolve`. Under
QuickJS the continuation runs later, during `tick()`'s microtask drain. **Under
JSC (F2) it runs immediately, inside the `ProcessEvents` callstack.** So the same
script observes `.then()` bodies executing at different points relative to other
WebGPU callbacks in the same batch — and, if that body calls back into
`webgpu.h`, executing *re-entrantly inside* `ProcessEvents`.

`webgpu.h` permits that re-entrancy; **`CLAUDE.md` does not permit the
divergence.** "Behavioral parity across all four platforms is a first-class
concern" is meaningless if two engines disagree about when script runs.

The callback becomes pure Rust: it takes ownership of its `userdata`, records
`(deferred_id, Result)` into a settlement queue, and returns. It calls no engine
function, allocates no `JSValue`, opens no scope. `with_async_scope` disappears
from the callback path — and with it a class of ownership bug this project has
already paid for twice.

**J2 — `tick()` has four steps, and the third is a trampoline.**

1. `wgpuInstanceProcessEvents(instance)` — callbacks record results.
2. *(nothing; the queue is now full)*
3. **Settle every recorded result inside a single JS frame**, then drain
   microtasks.
4. Drain the release queue.

Step 3 is one JS call, not N. Measured (F3): settling two promises from two
separate C→JS calls gives JSC two microtask checkpoints; settling both inside one
JS frame gives one, which is what QuickJS does natively. **The trampoline is what
makes the two engines agree.**

It is engine-neutral: QuickJS runs it harmlessly and still needs its explicit
`JS_ExecutePendingJob` drain (step 3's second half). Under JSC that drain is a
no-op, because the trampoline's return already drained.

*(Amended 2026-07-10: the four-step ordering is now **owned by `core/`** as a
generic tick skeleton — block 02 → A30 — with `drain_microtasks` as the engine
hook. The JSC adapter delegates to it; it does not hand-write the sequence. A
deletion experiment showed the batching step is unfalsifiable from QuickJS's
side, so the only structural guarantee of the ordering is that it is written
once.)*

**J3 — `core/` changes, and that is the gate working.** J1 and J2 alter `core/`'s
async settlement policy. Per the exit gate this must be reported, not absorbed —
so: **the boundary is not wrong; `core/` was engine-shaped and nobody noticed.**
It had encoded QuickJS's checkpoint semantics ("resolving does not run
continuations") as if universal. The fix makes `core/` *more* neutral, and it is
the second time the JSC gate has caught engine-shaped `core/` logic before codegen
multiplied it.

Record it in `specs/tracking/engine-boundary.md`. Do not let it pass as "just an
addition".

### Value ownership and tracing

**J4 — `duplicate_value` is `JSValueProtect`; `release_value` is
`JSValueUnprotect`.** They must balance, and the mock already asserts it (R23).

**J5 — payload tracing is a no-op under JSC.** JSC has no `gc_mark` (F4);
protection keeps a value alive outright. QuickJS needs the hook.
**This resolves block 02 → A27's deferred question in favour of keeping
the tracing mechanism**, and against `associate_value`: the payloads that hold
engine values (mapped ranges) form no cycle back to their wrapper, so protection
is sufficient and no association primitive is needed.

*(Wording corrected 2026-07-10: after the design review's DR-m6 reshaping, the
mechanism is `core::trace_payload_values` — a blind core helper the QuickJS
adapter calls from its `gc_mark` — not a `JsEngine` trait method named
`trace_payload`. Under JSC the "no-op" is the absence of any hook wiring, which
is the correct amount of code.)*

State that in `engine-boundary.md`. A27 said "Phase 3 decides, when JavaScriptCore
has a vote." It has voted.

**J6 — finalizers only enqueue.** Unchanged, and now doubly justified: the premise
that JSC finalizers run on any thread is unverifiable (F5), so it must be assumed;
and F5 itself means a forgotten `destroy()` holds GPU memory until the context
dies. `CLAUDE.md` principle 7's "under JSC `destroy()` is the only bounded path"
is a measured statement, not a caution.

**J7 — `Context<'a>` carries a handle scope on JSC too, and it is not `Option`.**
JSC's values are GC-managed, so the scope has nothing to free — but the type must
not offer an opt-out. Block 02 → A23 exists because an `Option<&Scope>` was
declined the moment it could be.

### Mapping

**J8 — `MappedRangeStrategy::CopyInCopyOut` gets its first real implementation**,
and it is the reason both arms live in `core/`.

- **`getMappedRange` (copy-in).** Allocate a **private** staging `ArrayBuffer`;
  take its C bytes pointer (this pins it — it is private and never reaches
  script); `copy_nonoverlapping` foreign → staging; `visible = staging.transfer()`.
  Hand `visible` to script and drop `staging`. `visible` is unpinned, populated,
  and detachable.
- **`unmap` (copy-out).** `detach_arraybuffer(cx, value, out)` — one primitive
  (block 02 → A13). JSC implements it as: `product = value.transfer()`, which
  detaches the script-visible buffer; **verify** `value`'s length is 0; take **the
  product's** C pointer; `memcpy` into `out`; release the product.

**Detach happens before any pointer is taken.** A pinned buffer can then never
reach script — which is the entire safety argument (F6).

**J9 — never take the C bytes pointer of a buffer script can see.** `CLAUDE.md`
invariant 10. Treat any occurrence as CRITICAL. `JSObjectGetArrayBufferByteLength`
is safe and is what `arraybuffer_len` must use (A26).

**J10 — `arraybuffer_len` must distinguish "detached" from "failed"** on JSC too.
Return `Some(0)` for a detached buffer, `None` for a non-buffer or an engine
failure, and clear any pending exception. A26's trap was that these are three
different facts with one obvious encoding.

### Sequences

**J11 — implement WebIDL sequence conversion, and delete B20's deviation.**
Measured (F7): JSC reaches `Symbol.iterator` with two string property reads and
`JSObjectGetPropertyForKey`, without `eval`. QuickJS reaches it the same way
(`JS_GetPropertyStr` twice, then `JS_ValueToAtom` + `JS_GetProperty`).

So the primitive is engine-neutral and additive:

```rust
fn global(cx: Self::Context<'_>) -> Self::Value;
fn get_property_value(cx: Self::Context<'_>, obj: Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
fn call(cx: Self::Context<'_>, f: Self::Value, this: Self::Value, args: &[Self::Value]) -> Result<Self::Value, Self::Error>;
```

**A27's sibling question is now answered too.** B20 said "Phase 4 decides, when
JavaScriptCore has a vote." **JSC votes yes**: iteration is reachable, so the
`length`+index shortcut can go, and array-likes must be rejected while `Set`s and
generators are accepted. Replace B20's deviation tests with conformance tests.

`core/`'s `sequence_len`/`sequence_item` are deleted, not fixed.

### Boundary

**J12 — `core/` contains zero references to JSC types**, no `cfg(engine)` branch,
no `dyn` on the conversion path.

**J13 — every capability JSC needs is an addition to `JsEngine`.** J1/J2 are the
declared exception, reported under J3. **Any further `core/` logic change: stop and
report.** That decision is the planner's, and it is the most important one in the
project.

**J14 — the adapter names no class and no member** (R24) and **holds no lock
across a call into `core/`** (R25). The boundary is re-entrant through
`E::payload`; JSC will hit it exactly as QuickJS did.

**J15 — every `extern "C"` callback catches unwinds** and calls no `webgpu.h`
function. JSC finalizers run on an unknown thread; a panic crossing that boundary
is undefined and unattributable.

**J16 — `unsafe impl Send`/`Sync` needs its `// SAFETY:` comment**, and JSC breaks
one that QuickJS did not: `unsafe impl Sync for TracedValues` is sound only under
QuickJS's on-thread GC. Recheck it, and every sibling, against an any-thread
finalizer.

### Added by the 2026-07-10 review — the traps block 04 was silent about

**J19 — `arraybuffer_copy` is the pinning trap J8/J9 forgot, and its obvious JSC
implementation is the project's named CRITICAL.** `writeBuffer` copies script
bytes via `E::arraybuffer_copy`, and the QuickJS impl takes the raw bytes pointer
of the **script's own** `ArrayBuffer` — harmless there. The mirror-image JSC
implementation (`JSObjectGetArrayBufferBytesPtr` on the script's `data`) invokes
`pinAndLock()` and **permanently, silently** disables `transfer()` on a
script-visible buffer (F6, invariant 10). The safe JSC shape: call the buffer's
own `slice()` (two property reads + `call`, no `eval` — the J11 primitives
suffice), which yields a **private** product; pin *that*, `memcpy`, release. The
script's buffer is never pinned. J9's grep — any C bytes pointer taken from a
script-reachable buffer is CRITICAL — applies to this method with no exemption
for "it's just a read".

**J20 — unhandled rejections have no JSC hook, and the parity claim must be
scoped honestly.** QuickJS surfaces them via
`JS_SetHostPromiseRejectionTracker` (A22). The F1 header sweep found no JSC
equivalent in the public C API — no tracker, no job hook. Consequences, stated
before the adapter exists so nobody discovers them as a red diff:

- A22's surfacing is a **Tier 1 diagnostic**, not a portable contract. Record it
  as an engine delta in `specs/tracking/engine-boundary.md`.
- **J17's conformance script must not depend on unhandled-rejection reporting**:
  every rejection in it is explicitly `.catch()`ed and logged, so the
  byte-identical-output claim stays achievable on both engines.
- The JSC adapter's `tick()` returns no unhandled-rejection error, ever. If the
  host wants that diagnostic under JSC, the answer is a script-level
  `.catch()` discipline — trusted scripts, invariant 8 — not prototype patching.

**J21 — a JSC finalizer may not call *any* function taking a `JSContextRef`, and
that includes `JSValueUnprotect`.** The QuickJS adapter releases a payload's
duplicated range values inside the class finalizer, synchronously — legal there.
JSC's `finalize` callback documentation forbids it, and the finalizer receives
only the `JSObjectRef`, no context. So the JSC adapter needs a **deferred-release
path for engine values**, symmetric with the native release queue: the finalizer
extracts the values from the payload and pushes them onto an adapter-owned
unprotect queue; `tick()` step 4 drains it on the JS thread with a live context.
The native `ReleaseQueue` stays native-only (it is `E`-free by design); this
queue is the adapter's. The mock's teardown balance assertion (R23) must still
hold: an extracted-but-undrained value at context teardown is drained by
`Runtime::drop` **before** `JSGlobalContextRelease`, mirroring A28.

---

## 4. Tests

**J17 — the same `.js` script, both engines, identical expected output.** This is
the deliverable. `CLAUDE.md`: *"one `.js` conformance script executed under both
engines with identical expected output."*

At minimum: `wrap_device` → `createBuffer` → `label` round-trip → `mapAsync` →
`getMappedRange` → write → `unmap` → assert the bytes → `destroy`. Plus the block
03 copy round-trip. **Byte-identical output, or the parity claim is empty.**

- `core/`'s mock tests keep passing with **no engine, no backend, no GPU**.
- The JSC adapter is behind the `jsc` cargo feature, **never in `default`**, macOS
  only.
- **`tick()` ordering, both engines.** Two promises settling in one `tick()` must
  run their `.then()`s in the same order, after both settlements, on both engines.
  Assert the log. This is J2's whole point, and it is the test that would have
  caught the divergence.
- **No test may provoke a JSC finalizer via GC** (F5). Where Phase 0 simulated an
  any-thread finalizer, say so in the test name, as `spikes/release-queue` does.

**J18 — negative demonstrations, seen red, on the ordinary `cargo test` gate where
possible.** Break it, watch it fail, restore it, watch it pass, quote both.

- **J1**: call `resolve` from inside the WebGPU callback; show the `.then()` runs
  inside `ProcessEvents` under JSC and not under QuickJS.
- **J2**: settle two promises with two separate JS calls; show the checkpoint
  ordering diverges from QuickJS.
- **J8/J9**: take the C bytes pointer of a script-visible buffer before `unmap()`;
  show detach silently fails and the verification fires.
- **J11**: pass an array-like; show it is rejected. Pass a `Set`; show it is
  accepted.

This project has shipped **five tautologies**. Before every assertion: *would this
still pass if the feature were a no-op?*

---

## 5. Exit criteria

1. The same `.js` script produces byte-identical output under QuickJS and JSC.
2. `core/` contains zero JSC references; `cargo test -p webgpu-native-js-core`
   still passes with no engine, no backend feature, and the backend env var unset.
3. **Every JSC capability was an addition to `JsEngine`, except J1/J2, which are
   reported under J3 as the gate firing.** Any *other* `core/` logic change stops
   the phase.
4. `MappedRangeStrategy`'s `CopyInCopyOut` arm runs against a real engine for the
   first time, and no pinned buffer ever reaches script.
5. WebIDL sequence conversion replaces the array-like shortcut; B20's deviation
   tests become conformance tests.
6. Every negative demonstration in J18 has been seen red.
7. Full workspace gate green; clippy clean; Phase Review clean of CRITICAL and
   MAJOR.

## 6. Open questions this block will answer — ANSWERED (2026-07-10)

- ~~**Can a JSC finalizer be observed on a non-main thread at all?**~~ Not
  provokable (F5 held throughout the phase), but no longer merely a premise:
  `JSObjectRef.h` documents "An object may be finalized on any thread" (§2
  upgrade). The designs resting on it rest on the header's contract.
- ~~**Does the handle scope have anything to do under JSC?**~~ **Yes — it is
  the root set, and treating it as an empty formality was the phase's worst
  bug.** JSC's collector scans only the machine stack; a `JSValueRef` held in
  a Rust `Vec` is invisible to it. The first `Scope` was a pure recorder, and
  the Phase Review found settlement values GC-collectable mid-drain (PR3-C1,
  a use-after-free). `Scope::track` is now `JSValueProtect`; drop unprotects
  what did not escape. A23's "the scope is not `Option`" aged well; J7's "it
  has nothing to free" did not — it has nothing to *free*, and everything to
  *root*.
- ~~**What does the trampoline cost?**~~ Measured (ignored test, 200 ticks ×
  8 settlements): batched trampoline 16.50 ms vs per-deferred direct calls
  15.70 ms — about 4 µs per tick, the price of one extra JS call. The parity
  property costs nothing worth discussing.
