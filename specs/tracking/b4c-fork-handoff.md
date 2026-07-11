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
