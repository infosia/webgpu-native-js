# B-4c → quickjs-ng fork: handoff for the fix

**Status: engine defect confirmed and precisely localized; fork created; the
exact off-by-one line and the fix remain.** This document is self-contained so
any agent (or the owner) can resume the fix work.

## The fork

- Upstream forked to `https://github.com/infosia/quickjs.git`
  (local copy at the checkout referenced by the owner; located via env, never
  by a committed path).
- Working branch **`b4c-async-varref-fix`**, based on **v0.15.1
  (`fd0a0210b7be00957751871e7e01b8291268fc29`)** — the exact rev our submodule
  pins. Fix here, then the submodule repoints to the fork's fixed commit
  (owner runs the push; planner repoints the submodule).

## The bug (one line)

quickjs-ng under-counts, by exactly one, the reference count of a **closure
that is co-owned by a detached `JSVarRef` and an object property** when the
async function that received the closure **as an argument** tears its frame
down — a use-after-free the cycle collector trips on at the next GC.

## Reproduction (reliable)

Build the binding's quickjs C compile with **`-DFORCE_GC_AT_MALLOC`** (quickjs's
own knob; one-line `.define(...)` in `adapters/quickjs/build.rs`), then, DEBUG
build (assert must be live):

```
CTS_PATH=<cts-out> ./target/debug/cts-runner \
  --query 'webgpu:api,validation,encoding,cmds,render,draw:buffer_binding_overlap:drawType="drawIndexed"' \
  --timeout-secs 580
```

→ exit 134, `Assertion failed: JS_REF_COUNT(p) > 0, gc_decref_child`. Aborts on
the first run under forced GC. (Without forced GC: ~50% of runs; release builds
never assert because `assert()` is compiled out — this masked it for the whole
of Phase B.) No minimal pure-JS reducer has reproduced it yet (see "Open").

## What is directly confirmed (by instrumentation, patches v2–v6 in scratchpad)

1. The victim is a **class-13 `JS_CLASS_BYTECODE_FUNCTION`** (a JS closure).
2. Its COMPLETE reference history (generation-safe ledger + leak-preserved
   identity) is just two real ops:
   - `duplicate` rc 1→2 at **`OP_get_arg2`** (opcode 217): the closure is read
     as **argument #2** of a function.
   - `release` rc 2→1 labelled **`OP_return_undef`** (opcode 41): actually the
     post-completion **`async_func_free`** teardown (the flag scoped to
     `async_func_resume` logged zero such releases, so it is the teardown path,
     not the resume path — `b4c_last_op` still reads 41 because no new opcode
     dispatches between the async return and teardown).
3. At the assertion the closure has refcount **1** but **two** live gc-edges
   (parent scan): a plain `Object` holding it as a property, and a detached
   `JSVarRef`.
4. A live trap in `async_func_free`'s arg/stack free loop
   (`for sp in arg_buf..cur_sp`) fires ~94×/run: it frees closure slots that
   are **co-owned by a detached `JSVarRef`**. Some closures are pointed at by
   **two distinct** detached var_refs.
5. The victim's history contains **no `close_var_ref` dup and no property-store
   dup** — only the single `OP_get_arg2` dup. So the two persistent edges were
   established by **ownership transfer (move/consume), not duplication**.

## Leading hypothesis (needs bytecode-level confirmation in the fork)

A closure read once by `OP_get_arg2` (one dup, rc→2) is stored into **two**
destinations — a captured variable's `JSVarRef` (via `OP_put_var_ref` →
`set_value`, which *consumes* the stack reference) **and** an object property —
but only one dup backs both stores. The async frame teardown
(`async_func_free`, quickjs.c ~L21015: `close_var_refs` then the arg/stack free
loop) then releases the arg slot, leaving refcount one short of the two
surviving edges. The suspects, in order: (a) the arg/stack free loop freeing a
slot whose reference was already moved out; (b) `close_var_refs`'s
`var_ref_count != 0` guard skipping a var_ref that captured an argument; (c) a
bytecode-gen bug emitting a store-to-two-places with a single dup.

## Fix-development loop (ready)

- **Oracle:** the CTS case above through `cts-runner` (submodule build). Fast:
  `cargo build -p cts-runner` then run; ~1 min/iteration.
- **Fork standalone:** a `qjs` built from the fork with `-DFORCE_GC_AT_MALLOC`
  exists in scratchpad (`qjs_fork`); use it once a minimal JS reducer is found —
  that reducer becomes the fork's regression test.
- Instrumentation to re-apply while iterating: `b4c-instrumentation-v6.patch`
  (FORCE_GC + class-13 ledger + closure-leak + close_var_ref logger + parent
  scan + async-free co-owned trap + opcode recording).

## Open

- **No minimal pure-JS reducer yet.** Six shaped attempts (incl. the exact
  signature: closure passed as an async-fn argument, captured by an inner
  closure AND stored on an object, with try/finally + throw, under forced GC)
  all stay quiet. The trigger needs an ingredient still unidentified — likely a
  specific await/suspension + capture ordering, or the multi-await resume path.
  Reducing the ACTUAL CTS bytecode (disassemble `buffer_binding_overlap`'s
  compiled function, or delta-debug the harness) is the surest route to the
  minimal repro that will double as the regression test.
- The exact off-by-one line: pin it in the fork by editing
  `async_func_free` / `close_var_refs` / the arg-capture opcodes and re-running
  the oracle.

## Regression-test plan (for the fork)

Once a minimal reducer reproduces under the fork's `-DFORCE_GC_AT_MALLOC` qjs,
add it as a `tests/` script the fork runs under forced GC in CI; the fix must
turn it green. Mirror it as a binding-level script test here once it exists.

## Session 7 (2026-07-12, planner): CTS delta-debugging — the `for...of` iterator is central

Delta-debugged the actual `buffer_binding_overlap` test in a writable copy of
the CTS `out/` tree (the owner's checkout was never modified — sandbox blocks
writes there, so the tree was copied to scratch and edited there), forced-GC
build, crash rate measured over 3–5 processes per variant.

Findings (each a real deletion experiment):
- Removing the draw calls: **still crashes** — the draw is irrelevant.
- Removing `renderPipeline` + `setPipeline`: **still crashes**.
- Replacing `validateFinishAndSubmit(true,true)` with a plain
  `encoder.finish()`: **still crashes** — the async error-scope expectation is
  NOT the trigger.
- Removing all buffer binds (the "overlap" the test is named for): **still
  crashes** — the shared-buffer binding is NOT the trigger.
- The `calcAttributeBufferSize`/`calcSetBufferOffset` local arrow closures
  (which capture `arrayStride`): inlining them away — **still crashes** — not
  the trigger.
- **Decisive:** the trigger is the **nested `for...of` loop**. A `for (let
  i=0; i<4; i++)` count loop over `createEncoder()+finish()` does **not**
  crash; the original **`for (const encoderType of [...]) { for (const x of
  [...]) { ... } }`** does. A single (un-nested) `for...of` does not; the
  nested pair does.

`for...of` compiles to an iterator protocol: an iterator object with a `next`
closure, and a fresh per-iteration `const` binding that the loop body captures.
Nested, the inner loop body references the **outer** loop's `const`
(`encoderType`, passed to `createEncoder`) — a closure/var_ref capturing a
loop variable across an inner iterator, exactly the co-ownership the engine
mis-frees. This finally connects the engine-side finding (async_func_free
over-releases a co-owned closure) to a source-level construct.

**Caveat that blocks a clean minimal repro:** the crash is **probabilistic**
and scales with total allocation/GC volume even under `FORCE_GC_AT_MALLOC`.
Aggressively reduced variants drop below the trigger threshold (0/5) without
being true non-triggers, which makes further pure-deletion delta-debugging
unreliable, and is why every low-volume synthetic reducer (now eight attempts,
including nested `for...of` + captured loop var + closure-on-object + async +
2000 iterations in standalone qjs) stays quiet. The reliable repro remains the
full CTS case; the reduction narrowed the *construct* (nested `for...of` +
captured outer loop var + a framework closure like `createEncoder`'s `finish`)
but not to a standalone artifact.

**Updated fix-development guidance for the fork:** the bug lives in the
interaction of the **iterator/`for...of` desugaring**, **var_ref capture of a
loop `const`**, and **frame teardown** under GC. Suspect opcodes/paths:
`OP_for_of_start`/`OP_for_of_next` iterator objects, the per-iteration `const`
var_ref, and `async_func_free`/`close_var_refs`. The oracle stays the full CTS
case under `-DFORCE_GC_AT_MALLOC`; a fix must take it from 5/5 crashes to 0/N.
The reduced CTS body (nested `for...of` + `createEncoder(encoderType)` +
`finish()`, no binds/pipeline/draw/async-expectation) is the smallest
*in-framework* repro found and is a good fix-verification target because it
isolates the construct while staying above the probability threshold when the
subcase count is left intact.

Scratch: writable CTS copy at `<scratch>/cts` (disposable); staged reductions
`stage1..6.js`; standalone reducers `red{2,3,4}.js`; fork qjs `qjs_fork`.

## Session 8 (2026-07-12, planner): the co-owning var_refs are frame-detached for-of loop consts

Instrumented `async_func_free`'s arg/stack free loop to classify each co-owned
closure: is the detached `JSVarRef` that also owns it present in THIS frame's
`sf->var_refs` (i.e. one `close_var_refs` just closed) or not?

Result on the forced-GC CTS case: **94/94 co-owned closures are owned by
var_refs with `in_frame=0`** — none are in the teardown frame's `sf->var_refs`.
Zero `in_frame=1`.

This nails the `for...of` connection mechanically. `close_lexical_var`
(quickjs.c ~L17335), invoked by `OP_close_loc` at every for-of iteration
boundary to give the loop `const` fresh-per-iteration semantics, does:

```c
close_var_ref(rt, var_ref);        // dup value into var_ref->value, detach, add to gc_obj_list
sf->var_refs[var_ref_idx] = NULL;  // <-- clears the slot
```

So after a loop iteration the loop-const's var_ref is **detached and in
`gc_obj_list` but no longer in `sf->var_refs`**. At async teardown,
`close_var_refs` therefore does NOT touch it (correct — it's already closed),
but the free loop still frees the corresponding `var_buf` slot. For a single
capture this is balanced (close_var_ref's +1 vs the slot free's −1). The crash
is a rarer imbalance layered on this structure — the victim closure ends one
reference short of its two gc-edges (the frame-detached var_ref + an object
property).

**Where the fix work must focus (fork):** the fresh-per-iteration `const`
lifecycle in `close_lexical_var` / `close_var_ref` and its interaction with
`async_func_free`'s `var_buf` free loop, when the loop-const value (or a
closure co-captured in the same iteration) is ALSO referenced by an object
property. The imbalance is one extra release along that path. Candidate-fix
strategy: instrument the exact sequence for ONE victim (pin its pointer at
`close_lexical_var` time, log every subsequent refcount transition and every
gc-edge holder until the assert), or bisect a targeted edit (e.g. auditing
whether the `var_buf` slot for a closed loop-const should be cleared to
`JS_UNDEFINED` at `close_lexical_var` time rather than freed again at teardown)
against the CTS oracle (5/5 → 0/N).

## Analytical status

Every single-capture path traced analytically balances; the crash is a rare
timing/aliasing imbalance on the for-of-loop-const + async-teardown +
object-property structure. Pinning the one extra release is the remaining work
and is best done as edit-and-test in the fork against the CTS oracle, not by
further read-only tracing (which has now reached its ceiling: the mechanism,
the construct, the opcodes, and the frame-detached-var_ref fact are all
established).

## Session 8 addendum: first candidate fix tested and REJECTED

Hypothesis: after `close_lexical_var` closes a for-of loop-const's var_ref, the
`var_buf` slot still holds the (now dup'd) value, and a fresh per-iteration
binding could alias that stale value before the next store. Candidate: free the
slot immediately in `close_lexical_var`
(`set_value(ctx, &sf->var_buf[var_idx], JS_UNDEFINED)` after the close).

Result: **still 5/5 crashes** on the CTS oracle. Rejected — the imbalance is not
a stale `var_buf` alias at the close point. (Semantically the edit only moves
the slot's release earlier, which the crash rate confirms is a no-op for this
bug.) Search narrowed: the extra release is elsewhere than the loop-const
slot's own lifetime.

Remaining suspects, reordered after this negative result:
1. The **object-property edge** (the second co-owner): the store that puts the
   closure on an object may consume a reference the var_ref path also assumes it
   owns. Trace the property-store opcode for the victim, not just the var_ref.
2. The **inner-iterator** teardown (`JS_IteratorClose` / `OP_iterator_close`)
   freeing a value still referenced by a captured var_ref.
3. `get_var_ref`'s reuse path (`var_ref->header.ref_count++` without touching
   the value) interacting with a closure value that migrated slots.

Recommended next tactic (fork): a **value-pinned trap** — at the moment a
closure first becomes co-owned (detected via the `async_func_free` co-owned
trap, but pin the pointer EARLIER, at `close_lexical_var` or the property
store), log EVERY refcount transition on that exact pointer with a full
backtrace until the assert, and refuse to retire its ledger. That directly
catches the one unmatched release. This is edit-and-test territory in the fork
with the CTS oracle; read-only aggregate tracing has reached its ceiling.
