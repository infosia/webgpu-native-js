# Block 14 — Boa engine adapter (spike)

**Status: SPIKE.** Goal is a decision, not a shipped tier. Boa
([boa-dev/boa](https://github.com/boa-dev/boa), MIT/Unlicense, v0.21.1) is a
pure-Rust JavaScript engine. This block adds `adapters/boa/` implementing
`trait JsEngine`, then *measures*. Tier assignment and QuickJS's fate are owner
decisions taken **after** Phase 2's numbers, not assumptions baked in here.

## Why (the motivation, stated honestly)

1. **A whole bug class disappears.** Phase B is currently blocked by a QuickJS
   GC defect (`specs/tracking/b4c-fork-handoff.md`): eight sessions of C-level
   forensics localized it to the engine's `for...of` loop-const var_ref
   lifecycle vs async frame teardown, a fork was prepared, and the exact fix is
   still open. A pure-Rust engine removes both the defect class and the C
   debugging burden.
2. **The build loses C.** No C toolchain, no `bindgen` for the engine, and
   iOS/Android cross-compilation becomes ordinary `cargo build --target`.
3. **The boundary gets its third proof.** Wiring JSC required zero changes to
   `core/` logic (the J13 gate). A third, structurally *different* engine
   (tracing GC, non-`Copy` values, `&mut Context`) is the strongest test yet of
   whether `trait JsEngine` was drawn correctly.

## Engine facts (verified against the v0.21.1 source, not assumed)

| Concern | Boa's API | Consequence for us |
|---|---|---|
| ArrayBuffer detach | `JsArrayBuffer::detach(&self, key) -> JsResult<AlignedVec<u8>>` | **A checked detach that returns the bytes.** QuickJS's `JS_DetachArrayBuffer` returns `void` and silently no-ops; JSC's `transfer()` silently no-ops on a pinned buffer — which is exactly why invariant 11 makes `core/` verify detachment itself. Boa reports failure natively. |
| External memory | only `from_byte_block(AlignedVec<u8>)` — Boa owns its buffer allocation | **No zero-copy over GPU memory.** Boa declares `MAPPED_RANGE_STRATEGY = CopyInCopyOut` (same as JSC), so `core/` never calls `new_external_arraybuffer`; the adapter implements it as an error. Spec-conformant (invariant 9): a performance difference, not a behavioural one. |
| Microtasks | `trait JobExecutor { fn run_jobs(&self, &mut Context) }` | A first-class embedder hook; `drain_microtasks` maps directly (invariant 3). |
| Modules | `trait ModuleLoader` | Block 12's loader maps directly. |
| Native classes | `trait Class: NativeObject` | Class registration + payload storage. |
| GC | tracing, **thread-local** (`thread_local!`, `Rc`, `NonNull`); `Trace`/`Finalize` | Finalizers run on the **owning thread**, so JSC's "finalizer on an arbitrary GC thread" hazard is gone, and collection can be forced, so JSC's "never finalizes before context teardown" hazard (invariant 7) is gone too. The release queue stays anyway — it is engine-generic and costs nothing. |
| Build | `default = ["float16", "xsum"]`; `intl` is **opt-in** | No ICU pulled in by default. |

## The central design problem, and the answer

Three `JsEngine` bounds do not fit Boa's types as-is:

- `type Value: Copy` — Boa's `JsValue` is an enum holding `JsObject` (a `Gc`
  pointer). It is `Clone`, **not `Copy`**.
- `type Context<'a>: Copy` — Boa needs `&mut Context` for nearly every
  operation; a `Copy` context cannot be a `&mut`.
- `type DeferredRegistration: Send + 'static` — Boa's GC types are not `Send`.

**Do not relax the bounds.** Relaxing `Copy` to `Clone` would ripple through
every conversion in `core/` and would forfeit the J13 gate — and the gate is
the point of the spike. Instead:

**B1 — Values are `Copy` handles into an adapter-owned rooting arena.**
`type Value = BoaValue(u32)`: an index into a slotmap the adapter owns, whose
slots hold `JsValue`s and a refcount. This is `Copy`, it is `Send`, and it
solves GC rooting at the same time — a Boa value held in a Rust struct that the
GC does not trace can be collected, and the arena *is* the root set. Crucially,
`duplicate_value` / `release_value` already exist in the trait (they are
QuickJS's refcount ops) and become **root / unroot**, so nothing in `core/`
changes. `DeferredRegistration` holds `u32`s, so `Send` is satisfied honestly
rather than by an `unsafe impl`.

**B2 — `Context<'a>` is a `Copy` struct carrying a raw `*mut boa::Context` plus
`&'a Arena`.** The `&mut` aliasing discipline lives entirely inside the
adapter: single-threaded, one active borrow at a time, every `unsafe` carrying
a `// SAFETY:` comment saying what must be true when it runs (per the code
conventions). This is the adapter's own unsafe, not a leak into `core/`.

**B3 — The J13 gate applies unchanged.** Wiring Boa must require **zero changes
to `core/`'s logic**. Additive `JsEngine` trait methods are permitted; changed
bounds, changed semantics, or `cfg`-on-engine in `core/` are not. If Boa cannot
be wired without core churn, that is a finding about the boundary and it is
reported, not absorbed.

## Phase 1 — the spike (this block)

Deliverable: `adapters/boa/` implementing all 38 `JsEngine` methods, with the
B1/B2 design above.

**Exit criteria (both required):**

- **B4 — Parity.** `tests/parity/parity.js` runs under Boa and produces output
  **byte-identical** to `tests/parity/expected.txt` (currently 127 lines) — the
  same file QuickJS and JSC already agree on. This is the correctness verdict;
  it needs no new test infrastructure.
- **B5 — The J13 gate.** `git diff` on `core/` shows no logic changes (additive
  trait methods only). Reported explicitly in the handoff.

Standard gates additionally apply: workspace `cargo test` green, clippy
`-D warnings`, `cargo fmt`, and every new `pub fn` carries an inline unit test
(principle 1).

## Phase 2 — measure (the decision material)

Nothing here is a code deliverable; it is the evidence the tier decision needs.

- **B6 — Performance. DEFERRED (owner, 2026-07-12).** Boa publishes its own
  benchmarks; re-measuring them here buys nothing the decision needs. Revisit
  only if a concrete workload turns out to be too slow in practice.
- **B7 — Language gaps.** Phase A found *zero* ES gaps under QuickJS for the
  CTS harness (Babel output runs untransformed). Re-run that check under Boa;
  catalogue any gap rather than working around it.
- **B8 — The decisive check.** Run the families that currently crash QuickJS —
  `encoding,cmds,render,draw:buffer_binding_overlap:*` (~8/10 crash),
  `createView:*` (~1/3), `draw:vertex_buffer_OOB:*` (~1/4) — under Boa. If they
  are clean, B-4c stops blocking Phase B and the fork fix becomes unnecessary.
- **B9 — Mobile. DEFERRED (owner, 2026-07-12)** to mobile bring-up, as with the
  other engines. One data point already exists and cost nothing:
  `cargo check -p boa-adapter --target aarch64-apple-ios` passes. (Android's
  `cargo check` fails in the *FFI* crate's bindgen, which needs the NDK
  headers — a pre-existing backend/toolchain matter, not a Boa one.) Whether
  the `jsvalue-enum` feature is needed (Boa's NaN-boxing assumes a pointer
  alignment some platforms break) is a bring-up question.

## Phase 3 — the owner's decision (not this block's)

With B7 and B8 in hand: Boa's tier, and whether QuickJS is kept, demoted, or
dropped. Dropping it would remove the C toolchain, `bindgen`-for-the-engine, and
the B-4c fork workstream — but only the numbers can say whether the performance
cost is acceptable. **This block does not pre-judge that.**

## Dependency and pinning

`boa_engine` is taken from **crates.io, version-pinned** — never by filesystem
path. A path to a sibling checkout would violate the repository's
no-local-paths rule and would make the build machine-specific. Declaring the
dependency and fetching it must land together (the lesson from the `winit`
incident: a declared-but-unfetched dependency breaks the offline gate).

Boa is pre-1.0 and self-describes as experimental; the pin is exact and moves
deliberately.

## Out of scope for the spike

- Module loading (block 12) under Boa — the `ModuleLoader` trait makes it
  straightforward, but the spike's verdict does not depend on it.
- The CTS runner under Boa beyond B7/B8's checks.
- Any tier or `CLAUDE.md` change. Those follow Phase 3.

## Phase 2 results (2026-07-12) — measured, and independently re-run by the planner

### B8 — the decisive check: **Boa does not crash. 0/15.**

The three families that crash QuickJS were run five times each under Boa. Every
exit was **1** (an ordinary test failure); **no exit was ever >= 132**, and
nothing hung.

| Family | QuickJS (measured earlier) | Boa (5 runs) |
|---|---|---|
| `encoding,cmds,render,draw:buffer_binding_overlap:*` | crashes ~8/10 | `1 1 1 1 1` — no crash |
| `createView:*` | crashes ~1/3 | `1 1 1 1 1` — no crash |
| `encoding,cmds,render,draw:vertex_buffer_OOB:*` | crashes ~1/4 | `1 1 1 1 1` — no crash |

**The B-4c crash class does not occur under Boa.** Two consequences, both real:

1. **The crash was hiding results.** Under QuickJS these families aborted the
   process, so we never saw their pass/fail at all. Under Boa they *report*:
   `createView` 1,191 pass / 1 fail / 175 skip; `buffer_binding_overlap` 2/2;
   `vertex_buffer_OOB` 30/30. Those failures are ordinary and actionable —
   several indirect-draw cases raise `TypeError: not a callable function`
   (a binding gap the crash had been masking), and one `createView` case is an
   `Expected validation error` mismatch. **These are new findings, not
   regressions**, and they are triage material, not blockers.
2. The Phase-B suite-growth blocker and the quickjs fork fix both become
   unnecessary *if* Boa is adopted — that call is Phase 3's, not this block's.

### B7 — language gaps: **one real gap — Boa has no `Error.prototype.stack`.**

CTS self-tests under Boa: **1,002 pass / 29 fail** (QuickJS: 1,031/1,031).
Twenty-three of the 29 say verbatim `EXPECTATION FAILED: threw as expected, but
missing stack` / `rejected as expected, but missing stack`.

Confirmed **at the source, not inferred from our binding** — Boa's own CLI:

```
typeof e.stack = undefined
own props: ["message"]
```

`Error.prototype.stack` is a de-facto standard (every other engine has it; it is
a proposal, not ES-spec text), and the CTS self-tests assert on it. Note the
scope: this hits the CTS's *own* unittests, **not** the WebGPU validation suites
— B8's `createView` run passed 1,191 cases with the gap present. The remaining
handful of failures include two `determinantInterval` cases raising
`TypeError: cannot convert 'null' or 'undefined' to object`, not yet
characterized.

Catalogued, not worked around. Boa is pure Rust and MIT/Unlicense, so this is a
gap we *could* close upstream — unlike the QuickJS defect, which took eight
sessions of C forensics and is still unfixed.

### Engine wiring

`tools/cts-runner` is now engine-selectable by cargo feature: `engine-quickjs`
(default, unchanged) and `engine-boa`. `cfg`-on-engine is forbidden in `core/`;
this is a tool, so a thin wiring module is fine. The Boa adapter gained host
functions, module loading (with block 12's lexical-normalization rule and its
diamond-import regression test), `clear_global`, and `run_gc`; 23 tests, up
from 15.

**Gates:** workspace 354 pass, clippy `-D warnings`, fmt, `git diff core/`
**still empty**, and the QuickJS curated suite still exits 0 at **1,312 pass /
0 fail** — the incumbent path is bit-for-bit intact.
