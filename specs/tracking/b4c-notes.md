# B-4c QuickJS GC investigation — encoder-retention hypothesis disproved

Date: 2026-07-11. The deterministic CTS abort remains open. This audit did not
find an encoder-retention defect, and no adapter/core fix was made because the
proposed mechanism is absent from the current tree and its counterfactual stays
red. The frontier below is intentionally explicit rather than calling a masking
reference increment a fix.

## Reproduction

The debug command from the handoff reproduces consistently:

```sh
CTS_PATH=$HOME/Documents/workspace/Rust/webgpu-cts/out \
  ./target/debug/cts-runner \
  --query 'webgpu:api,validation,encoding,cmds,render,draw:buffer_binding_overlap:drawType="drawIndexed"' \
  --timeout-secs 200
```

Observed exit: 134 at QuickJS `gc_decref_child` (`p->ref_count > 0`). Other
runs of the same command reached exit 139 or the previously recorded
`TypeError: not a function` symptom.

The existing diagnostic runner knob remains in the tree: a positive
`CTS_RUNNER_GC_EVERY` calls `Runtime::run_gc()` after each multiple of that many
reported cases, after `Runtime::tick` and outside the `__report` callback. It is
documented in `tools/cts-runner/README.md` and inert when unset.

## Encoder-retention audit

There is no encoder-side `JSValue` retention in the current implementation.
`RenderPassEncoderPayload`, `RenderPassState`, and `RenderBundleEncoderState`
contain native handles/state only. The shared `render_pass_set_pipeline`,
`render_pass_set_vertex_buffer`, and `render_pass_set_index_buffer` bodies
convert borrowed arguments to native handles and call the backend; they do not
insert, overwrite, duplicate, release, or trace engine values.

`TracedValues` is used only by `BufferPayload` mapped-range tracking. Each
mapped range takes exactly one `duplicate_value`; `BufferState::ranges` owns
that reference and `TracedValues` is its non-owning, one-entry-per-range trace
mirror. Detach releases each owned range reference once and then clears the
mirror. Finalization clears the mirror before releasing the owned references.

The other traced payload values are the adapter/device caches, device event
handler/lost promise, and mapped ArrayBuffers. Their insertion paths duplicate
once per traced slot; replacement paths duplicate before releasing the old
value.

## Decisive counterfactuals

Three deletion/reproduction experiments falsify the proposed encoder
bookkeeping mechanism:

1. A QuickJS+yawgpu adapter test created a pipeline and shared buffer, bound
   that same buffer to vertex slots 1 and 7 and as the index buffer, with
   `setPipeline` after the buffer calls, finished the encoder, dropped all JS
   references, and ran GC twice. It passed.
2. The same test repeated the shape 500 times for both render-bundle and
   render-pass encoders, finished/submitted each encoder, dropped all JS
   references, and ran GC twice. It also passed.
3. Temporarily replacing both core native calls in `setVertexBuffer` and
   `setIndexBuffer` with no-ops did not change the deterministic CTS result:
   the query still exited 134 in `gc_decref_child`.

The temporary test and no-op edits were removed after the experiments because
the test was green before any fix and therefore was not a regression test.

The QuickJS class marker was also temporarily changed to report zero binding
payload edges. The CTS query still exited 134. Diagnostic output showed a GC
child already at refcount zero before its first recorded decrement in that
collection. Therefore the handoff claim that this assertion identifies a
binding class marker reporting too many encoder edges does not hold for this
reproduction.

## Earlier narrowing retained

With `CTS_RUNNER_GC_EVERY=1`, the 467-case validation seed, all 1,901
`createBindGroup` cases (apart from the known 82 external-texture failures),
and the combined 2,932-case unittest/createBindGroup run completed without the
assertion. The broad buffer and compute-pipeline attempts remain unsuitable
bisection inputs because existing backend/unsupported-shape failures stop them
first.

## Frontier

The next investigation should focus on an owned-value transfer that leaves a
dangling QuickJS-internal edge during the CTS promise/module/timer workload,
not on encoder slot retention. In particular:

1. Reduce the CTS framework execution around `validateFinishAndSubmit` and its
   repeated error-scope promise settlement; the synchronous 500-encoder adapter
   stress test is clean.
2. Add ownership diagnostics at QuickJS C-API transfer sites (`JS_SetProperty*`,
   promise settlement arrays, and module evaluation/loader values), recording
   the operation that first leaves a zero-refcount object reachable.
3. Turn that reduced async/module shape into a red adapter test before changing
   ownership code.

No standard gates were claimed after this audit because no coherent fix exists
yet. The original runner knob and pre-existing QuickJS diagnostic changes were
preserved; no commits were made.

## Planner narrowing (2026-07-11, evening — after the encoder hypothesis fell)

Empirical facts gathered after the deletion experiments, all on debug builds
(the abort is invisible in release: `assert()` compiles out under NDEBUG,
which is why every large release run "passed"):

1. **Single-case deterministic repro**:
   `webgpu:api,validation,encoding,cmds,render,draw:buffer_binding_overlap:drawType="drawIndexed"`
   → exit 134; the `drawType="draw"` variant → 139; `vertex_buffer_OOB:*`
   also aborts. Symptom sometimes mutates to
   `Unhandled promise rejection: TypeError: not a function`.
2. **The GC diagnostic identifies a stale edge, not a mark overcount in our
   class**: across runs the underflowing child was (a) memory whose class_id
   read as garbage (already reclaimed), (b) a runtime-registered class id 83
   (out of the 66 built-ins — i.e. one of our binding classes), (c) the
   JSContext header (type 5) — while the parent holding the edge was a plain
   `Object` (class 1) or a `VAR_REF` (captured closure variable), and NONE of
   the three objects had ever passed through `free_object` (the reclamation
   ring buffer had no record). Conclusion: an ordinary JS object/closure slot
   still holds a JSValue whose referent's refcount was driven to zero by an
   extra native release — the varying identity is whatever the stale slot
   points at by the time the cycle collector walks it.
3. **The crashing families share one construction the clean families lack**:
   CTS `makeTestPipeline` (draw.spec.ts:104) builds the vertex `buffers`
   array as `bufferLayouts[b.slot] = b` with slots 1 and 7 — a SPARSE array
   (length 8, holes at 0, 2–6). Dense-array families at equal or larger
   scale are clean: `render_pipeline,vertex_state:*` 679/679,
   `createBindGroup:*` 1,819/1,901 (all fails the known external-texture
   family), unittests+createBindGroup 2,932 in one process with GC forced
   per case. Our own `renderPipeline:nullable-holes:ok` parity line only
   ever exercised an explicit single-element `[null]` — holes (element reads
   yielding `undefined` through the iterator) and multi-element sparseness
   were never covered.
4. The generated `convert_vertex_state` buffers branch and core
   `convert_sequence`/`convert_sequence_from_method` were reviewed by the
   planner; no imbalance is visible at that level, which points the audit at
   the adapter's value-scope implementation (per-call scope tracking of
   owned JSValues: double-insertion, reallocation during reentrant
   `E::call`, escape handling) and at native paths that WRITE into JS
   objects (settlement arrays, promise capabilities, set_global paths).

Next session's first move: a red adapter test looping
`createRenderPipeline` with `var a = []; a[1] = layout; a[7] = layout;`
(holes in between) plus `run_gc()`, per the dispatch already drafted.

## Coding-agent sparse-array follow-up (2026-07-11)

The planner's proposed sparse-array adapter repro is unexpectedly **green**, so
the sparse construction is not sufficient to reproduce the imbalance outside
the CTS framework. A DEBUG QuickJS+yawgpu test created 500 pipelines followed
by two explicit GCs and exited 0. The first version used holes at 0 and 2--6,
two valid layouts at slots 1 and 7, and nested attribute arrays. The exact CTS
descriptor shape also stayed green: vertex/instance step modes, shader
locations 2 and 6, inline separately-created vertex and fragment modules,
`layout: 'auto'`, fragment target/writeMask, and triangle-list primitive. The
temporary green test was removed because it cannot guard a fix.

The original CTS query remains nondeterministically red. Consecutive runs of
the unchanged binary produced an expected-operation rejection (exit 1), then
the GC assertion (exit 134). The latter diagnostic was more concrete than the
earlier samples: a VAR_REF retained the stale edge, and the child had been
reclaimed as binding class id 82 while `JS_ExecutePendingJob` released it.
Another run exited 139. This keeps the async CTS framework/promise-job portion
as the live differentiator, not the pipeline descriptor conversion itself.

Two settlement counterfactuals were tried and fully reverted:

1. Replacing the QuickJS settlement arrays/trampoline with direct calls to each
   promise resolver stopped the assertion in two attempts, but changed the
   required batching/timing and made the runner report the CTS's temporarily
   unhandled `GPURenderPassEncoder is ended` rejection (exit 1). This is not a
   valid fix.
2. Keeping the trampoline but making its `fns` and `values` arrays explicitly
   owned and freed immediately after `JS_Call`, instead of scope-owned through
   microtask draining, still produced `TypeError: not a function` and then exit
   139. Array lifetime alone is therefore not the defect.

The QuickJS VALUE-SCOPE implementation and all adapter consuming-write sites
were audited. `get_property`, computed property access, `call`, constructors,
new instances, strings/numbers, and exceptions each insert their owned result
once. `JS_SetPropertyUint32` settlement inputs are escaped before the consuming
write; resolver functions are owned by `Deferred` and consumed once. Class
prototype/global installation likewise follows the consuming API contracts.
No coherent double insertion or missing escape was found on the sparse
pipeline path.

Frontier: reduce the CTS promise/error-scope job sequence, including
`validateFinishAndSubmit`, into the adapter test. The red shape must include the
framework's expected rejection handling and ticking; descriptor-only stress is
now disproved. Instrument binding class registration so diagnostic class id 82
has a stable name, then record every native decrement of that object's address
before its final `JS_ExecutePendingJob` decrement. No adapter/core fix is
claimed, and standard gates were not run because the tree has no fix. The
uncommitted `quickjs.c` GC diagnostics remain in place.

## Coding-agent decrement follow-up (2026-07-11, later)

The single CTS query remains immediately red on the current debug binary: one
run exited 139 after the shims, and the next exited 134 in `gc_decref_child`.
After extending the uncommitted QuickJS diagnostic to retain the registered
class name at reclamation, another pair produced the existing
`TypeError: not a function` symptom and then an assertion. In that assertion,
the stale child had been reclaimed as QuickJS's built-in `Function` class.
`JS_ExecutePendingJob` performed the final decrement which reclaimed it. A
plain Object still retained the stale edge. The diagnostic's earlier-zero
parent was also a Function, reclaimed from `Scope::drop` inside `qjs_method`
during a promise-reaction/async-function resume.

Temporary adapter logging (fully removed after use) recorded each value-scope
insertion with its source line and each native method entered. It showed no
scope mutation during a live `RefCell` borrow and no duplicate insertion of the
same owned result in the failing settlement frame. The outer `Runtime::tick`
scope contained exactly the expected values: values constructed while
converting settlements, followed by the `fns` and `values` arrays created at
`adapters/quickjs/src/lib.rs`'s `settle_deferreds`. Reentrant WebGPU methods used
distinct callback scopes. This rules out scope-vector reallocation and shared
outer/callback scope identity as mechanisms.

The audit also noticed that `DeviceEventJs::lost_deferred` resolving functions
are not visited by `trace_payload_values`. That is not a coherent explanation
for this under-count: omitting a native-held edge from QuickJS's cycle marker
leaves an extra apparent external reference (a leak/non-collection), whereas
the observed referent is short by one. No speculative marker change was kept.

The remaining high-value diagnostic is an ownership ledger keyed by object
address in the uncommitted QuickJS C diagnostics: record every `JS_DupValue`
and `JS_FreeValue` caller for runtime-registered binding objects and Functions,
then print that ring when `gc_decref_child` finds the address at zero. The
current reclamation backtrace proves only the final legitimate decrement; the
ledger must identify the earlier extra native release. The reduced adapter test
still needs the CTS expected-rejection/error-scope sequence; descriptor-only
tests remain green. No production fix or standard gates are claimed.

That external-API ledger was then added to the uncommitted QuickJS diagnostic
and the runner rebuilt. It records the last 16 `JS_DupValue`/`JS_FreeValue`
events for Functions and runtime binding classes. The next run exited 134. Its
surviving ledger contained only QuickJS-internal promise-finally/`JS_ToBoolFree`
decrements at refcount 2 and the terminal `free_var_ref` decrement at refcount
1; none of the retained events had an adapter/Rust frame. The child address had
already been reused (its class/type were garbage and its free record had
collided), so this does not exonerate an earlier native release: the 16-event
ring was overwritten by later internal promise activity. Next pass should
either increase the per-address history or tag object allocation generations,
and should distinguish calls entering `JS_FreeValue` from inside QuickJS from
calls whose first non-C frame is the adapter. The ledger remains only in the
uncommitted `quickjs.c` diagnostics.

## Decisive per-address transition history (2026-07-11)

The diagnostic ledger now records the central object transitions rather than
only exported API entry points: every `js_dup` increment and every
`JS_FreeValue`/`JS_FreeValueRT` decrement records operation, resulting
refcount, and 16 frames in a 4096-slot table with eight events per address.
Because the 4096-slot live table was repeatedly claimed by another allocation
after reclamation but before the stale edge was walked, `free_object` also
snapshots that object's eight events into the existing uncommitted reclamation
record. This is diagnostic-only and remains confined to `quickjs.c`.

The rebuilt single-case runner produced exits 139, 139, then 134. The decisive
underflow parent was a QuickJS Array. The stale child had been reclaimed as
runtime class 80, whose registered name was `GPUTexture`. Its last eight
transitions were:

1. release -> 1: `JS_ToBoolFree -> JS_CallInternal -> JS_Call ->
   js_promise_then_finally_func -> promise_reaction_job`;
2. duplicate -> 2: `js_dup -> JS_CallInternal -> JS_Call ->
   js_promise_then_finally_func -> promise_reaction_job`;
3. release -> 1: `JS_CallInternal -> JS_Call ->
   js_promise_then_finally_func -> promise_reaction_job`;
4. duplicate -> 2: the same `js_dup`/`JS_CallInternal` promise-finally path;
5. release -> 1: the same `JS_ToBoolFree` promise-finally path;
6. duplicate -> 2: the same `js_dup`/`JS_CallInternal` promise-finally path;
7. release -> 1: the same `JS_CallInternal` promise-finally path;
8. release -> 0: `JS_FreeValueRT -> free_var_ref ->
   js_bytecode_function_finalizer -> free_object -> free_zero_refcount ->
   js_free_value_rt -> JS_FreeValue -> JS_ExecutePendingJob`.

There is no adapter, core, or CTS frame in any of these transition call sites;
the first Rust frame is only `Engine::drain_microtasks`, above
`JS_ExecutePendingJob`. In particular, the terminal release is QuickJS freeing
the value owned by a detached captured-variable reference while finalizing a
bytecode function. The preceding promise-finally transitions pair perfectly at
1 -> 2 -> 1. The final `free_var_ref` 1 -> 0 is locally balanced with the
`js_dup(*pvalue)` performed when QuickJS creates a detached var-ref, yet an
Array still retains the value afterward. This points to QuickJS engine
bookkeeping (an earlier missing edge increment or an engine-internal extra
release), not a binding release call.

The vendored engine is quickjs-ng v0.15.1 (`fd0a021`). The locally available
`origin/master` (`3c8f3d6`) keeps the same `Promise.prototype.finally`
ownership protocol: `JS_NewCFunctionData` duplicates captured data and its
finalizer releases it once; the finally value thunk returns `js_dup`; and
`js_promise_then_finally_func` passes borrowed arguments, consumes only the
promise through `JS_InvokeFree`, then releases `then_func` once. Master also
retains the same `free_var_ref` decrement/release, apart from refcount-access
macro changes. The observed transition sequence therefore matches upstream's
intended local pairs but still reaches zero with a live Array edge. Treat this
as a suspected quickjs-ng engine defect; do not add a binding workaround and do
not file it externally without a standalone engine-only reduction.

No production fix or verification ladder is claimed. The next frontier is an
engine-only reproduction of a captured object passing through nested
`Promise.prototype.finally` while also stored in an Array, followed by forced
GC. It should remove WebGPU entirely and determine whether the missing edge is
introduced by promise-finally/var-ref handling or only after prior heap
corruption in the CTS workload.

## B-4c full release-path history (2026-07-11, final coding pass)

The temporary, uncommitted `quickjs.c` ledger was extended again. `js_dup` is
the single ordinary increment hook; both `JS_FreeValue` and `JS_FreeValueRT`
now delegate to one `js_decref_value` decrement hook, so engine-internal calls
through either exported form cannot bypass recording. Cycle-collector
decrements and both restore-increment paths remain recorded in
`gc_decref_child`, `gc_scan_incref_child`, and `gc_scan_incref_child2`.
This quickjs-ng revision has no separate `__JS_FreeValue` implementation.

Each address still retains the requested last 16 events with 16 frames per
event. The live-history hash now probes its entire 16,384-entry static table,
instead of giving up after four colliding entries, and never evicts a live
record. At reclamation the history is copied into a 65,536-entry open-addressed
free-record table; an unprinted reclamation record cannot be overwritten by a
collision and becomes reusable only after its history is printed. These are
diagnostic-only changes and must not be committed.

After rebuilding, the single-case retry results were exit 1, exit 134 with a
live history but a collided older reclamation snapshot, then exit 134 with the
real reclamation history. The decisive child was address `0xb3a625e00`, built-in
class 13 (`Function`, `JS_CLASS_BYTECODE_FUNCTION`). A plain `Object` at
`0xb3a627b10` still marked it as a child after it had been reclaimed.

The retained events 394--407 are seven locally balanced pairs, each
`duplicate: 1 -> 2` followed by `release: 2 -> 1`. The duplicate reads a
captured value (`OP_get_var_ref_check`, `quickjs.c:18839`); the releases
alternate between boolean conversion and exception-stack unwinding inside the
same `Promise.prototype.finally` reaction. The final pair's interesting stacks
were (verbatim symbol names, addresses omitted):

```text
event=406 operation=duplicate resulting_ref_count=2
gc_debug_record_transition
js_dup
JS_CallInternal                         (quickjs.c:18839, OP_get_var_ref_check)
JS_Call
js_promise_then_finally_func
js_call_c_function_data
JS_CallInternal
JS_Call
promise_reaction_job
JS_ExecutePendingJob

event=407 operation=release resulting_ref_count=1
gc_debug_record_transition
js_decref_value
JS_FreeValue
JS_CallInternal                         (quickjs.c:20639, exception stack unwind)
JS_Call
js_promise_then_finally_func
js_call_c_function_data
JS_CallInternal
JS_Call
promise_reaction_job
JS_ExecutePendingJob
```

The only release in the retained ring without a corresponding duplicate in
that ring is event 408, the known final end-of-job cleanup. Its call site is
`free_var_ref` (called by `js_bytecode_function_finalizer`):

```text
event=408 operation=release resulting_ref_count=0
gc_debug_record_transition
js_decref_value
JS_FreeValueRT
free_var_ref
js_bytecode_function_finalizer
free_object
free_gc_object
free_zero_refcount
js_free_value_rt
js_decref_value
JS_FreeValue
JS_ExecutePendingJob
```

The corresponding ownership increment is necessarily older than this hot
16-event window: QuickJS duplicates the captured stack value when detaching
the reference in `close_var_ref` (`var_ref->value = js_dup(*var_ref->pvalue)`,
`quickjs.c:17624`), and `free_var_ref` releases that one owned value when the
last closure reference disappears (`quickjs.c:6507`). Thus event 408 is the
locally expected release, not evidence of a binding `Scope` double-free.
Events 406/407 are also balanced. Every frame below the job pump is QuickJS
engine code; the first Rust frame is only `Engine::drain_microtasks` above
`JS_ExecutePendingJob`. The incidental parent reclamation still shows
`Scope::drop -> catch_callback -> qjs_method`, but it is a different object and
does not occur in the failing child's transition history.

The exact observed engine sequence is therefore: a promise-finally reaction
loads the captured closure (duplicate), an exception path unwinds that stack
slot (release), the pending job releases its result and thereby finalizes a
bytecode function, that finalizer drops its last detached `JSVarRef`, and
`free_var_ref` releases the captured closure to zero. Later cycle marking finds
the already-reclaimed closure still referenced by a live plain Object. The
local promise/stack pairs and detached-var-ref pair are balanced, so the
remaining defect is an engine-internal missing edge increment or earlier
engine-internal bookkeeping corruption, not a named adapter/core/runner
release. Record `free_var_ref` as the unmatched-ring call site and
`JS_CallInternal`'s exception unwind as the immediately preceding release
site. Treat this as a suspected quickjs-ng defect. No workaround and no
external filing were made.

Because the decisive path is entirely engine-internal, no production code or
regression test was changed and the requested adapter-fix verification ladder
does not apply. The frontier remains an engine-only reduction of the sequence
above, ideally retaining enough generation-aware history to connect the plain
Object property insertion to the closure's detached-var-ref lifetime.

## Binding-free reproduction attempt (2026-07-11, planner)

A plain `qjs` was built from the vendored tree (stub repl/standalone arrays;
assert live; the temporary ledger included). A pure-JS workload modeled on the
recorded sequence — async fns capturing locals across `await`; chains ending
in `.finally()` whose closure callback throws; pre-rejected promises through
`finally`; late `.catch` attachment; `Promise.race`; involved closures stored
in long-lived plain objects; periodic `gc()` — ran 5 × 20,000 cases: **no
assertion, no underflow**. The scratch script is preserved verbatim below so
the attempt is reproducible; it is NOT committed anywhere else.

Status therefore remains: **suspected quickjs-ng defect, not yet confirmed
binding-free.** What the pure-JS shape lacks vs the real workload: the
adapter's job pump interleaving (`tick` = ProcessEvents → settlement
trampoline → `JS_ExecutePendingJob` loop → release queue) and host-function
re-entry — the corruption may need that interleave even if no adapter frame
ever touches the miscounted value.

Owner decision points (in order of information value):
1. Test the pin question: build the same single CTS case against quickjs-ng
   master (submodule fetch = owner-run) — if master is quiet, bisect upstream
   fixes; if master also aborts, the reduction must go deeper.
2. Suite-level mitigation meanwhile: shard broad CTS suites across processes
   (each process well under the ~1.3k-case floor) so Phase B suite-broadening
   is unblocked without resolving the engine question first.
3. No upstream filing (standing owner rule).

<details><summary>scratch reproduction script (not reproduced with it)</summary>

See the session scratchpad `b4c_repro.js`; shape: 400 rounds × 50 cases,
4 finally-modes rotated, gc() every 8 rounds, closures kept in a 64-entry
ring of plain objects.
</details>

## Pin-vs-master experiment (2026-07-11, planner; owner ran the fetch)

quickjs-ng `master` (3c8f3d6, "Fix reference leak in Iterator.prototype.filter",
2026-07-04) was checked out locally in the submodule working tree (the
committed pin was not moved) and the single CTS case was rerun 8 times:
**8/8 aborted** — plain runs die with SIGSEGV in `get_shape_prop(sh=NULL)`
during the GC walk (with a one-frame, unwind-hostile stack); under macOS
Guard Malloc (`libgmalloc`) the run instead reaches the SAME
`gc_decref_child` `JS_REF_COUNT(p) > 0` assertion (master line 7323).

Two conclusions:

1. **Not fixed upstream.** The behavior is alive on current master; a pin
   bump is not a fix. (No upstream filing, per the standing owner rule.)
2. **No foreign memory write.** Guard Malloc places every allocation on its
   own guarded page and faults on any touch of freed memory; the run reached
   the refcount-underflow assertion with no guard fault first. So the heap is
   not being stomped — the reference COUNTS themselves go inconsistent while
   every access touches live memory. Combined with the full engine-internal
   transition history (previous section), this narrows the defect to
   refcount/edge bookkeeping logic, engine-internal or engine-adjacent, not
   wild writes from any code.

Frontier (next session): run the CTS framework itself under the plain
vendored `qjs` with WebGPU fully stubbed in pure JS (the harness's async
machinery is the remaining un-exercised ingredient of the failing workload —
the shaped 100k-case synthetic stayed quiet, but the real harness code may
carry the exact trigger). If that aborts binding-free, the engine defect is
confirmed and reducible from a pure-JS artifact; if it stays quiet, the
adapter's pump interleave (tick: ProcessEvents → settlements → pending jobs →
release queue) becomes the prime ingredient and gets instrumented next.
Instrumentation patch preserved at the session scratchpad
(`b4c-instrumentation.patch`) and re-applied to the restored pin checkout.

## Plain-qjs CTS harness attempt (2026-07-11, coding pass)

The requested binding-free CTS port was built in the supplied scratchpad as
`qjs-cts-driver.mjs` plus `qjs-cts-glue.mjs`. It runs the real
`DefaultTestFileLoader -> parseQuery -> Logger -> testcase.run` path under
`qjs --std -m`; the four runner host functions are JavaScript functions,
`performance.now()` uses `os.now()`, and timers use qjs's `os.setTimeout` /
`os.setInterval`. The existing runner shims provide the text, event, console,
and DOM pieces. `navigator.gpu` is entirely JavaScript: one fake adapter and
device expose empty `Set` features, plain limits/info, a queue, error scopes,
buffers, textures/views, shader modules, render/compute pipelines, command and
render-bundle encoders, render/compute passes, draw/drawIndexed, finish,
submit, cleanup, and the other no-op methods reached by this case. No Rust,
WebGPU binding, wgpu-native object, native GPU resource, ProcessEvents call,
settlement trampoline, or release queue participates.

The exact query expands to one CTS case with 375 subcases (5 × 5 × 5 × 3).
The fake was sufficient to reach the real concurrent subcase execution and
its chained `Promise.prototype.finally` finalizers, but the case did **not**
reach `__report`. At the stock `maxSubcasesInFlight = 100` boundary, identical
fresh processes had three different outcomes. Runs were bounded by SIGALRM at
10 seconds so a tight loop could not consume the session indefinitely:

```text
run       exit   output
fresh-1   142    none (10-second SIGALRM; qjs at ~100% CPU)
fresh-2   139    none
fresh-3   1      397 x "Possibly unhandled promise rejection: TypeError: not a function"
fresh-4   139    none
fresh-5   139    none
fresh-6   139    none
fresh-7   142    none (10-second SIGALRM; qjs at ~100% CPU)
fresh-8   1      the same 397 TypeErrors
repeat-3  142    none (15-second SIGALRM before the first report)
```

The TypeError stack was consistently:

```text
Possibly unhandled promise rejection: TypeError: not a function
    at subcaseFinishedCallback (.../common/internal/test_group.js:610:15)
    at <anonymous> (native)
```

Line 610 calls the promise resolver stored by the CTS's 100-in-flight
backpressure gate. In those runs it was truthy but no longer callable. Raising
the gate above all 375 subcases avoided that particular callback but instead
entered a sustained 100%-CPU loop, also before a report. The captured logs are
`bounded-fresh-{1..8}.log` and `bounded-repeat-3.log` in the same scratchpad.
All exit-139 and timeout logs were empty. Across every captured run there were
zero lines matching `gc_decref_child` or `GC UNDERFLOW`.

This is a binding-free abnormal reproduction of the broader runtime-stability
problem (including four spontaneous SIGSEGV exits), so the Rust pump
interleave is **not** required for runtime failure. However, the requested
marker-specific result is not claimed as **CONFIRMED without the binding**:
no underflow diagnostic appeared, and the query never completed, so the
normal-completion repetition ladder could not be run. The next frontier is to
capture a stack for the silent exit 139 or let a bounded run continue under a
debugger until it reaches the existing GC ledger assertion. Preserve the
native `Promise.prototype.finally` path while reducing the CTS
`subcaseFinishedCallback`/backpressure sequence; replacing that machinery to
force completion would remove the sequence under investigation.

## Harness-under-plain-qjs attempt (2026-07-11, planner; incomplete)

A scratch driver (`b4c_harness.mjs` in the session scratchpad, alongside the
plain `qjs`) loads the REAL CTS framework modules directly from the CTS out/
tree (same six entry points as glue.mjs), installs a minimal environment
(os-based timers, performance, console, DOMException, EventTarget,
MessageEvent, TextEncoder/Decoder) and a Proxy-based pure-JS WebGPU fake
(never thenable; popErrorScope/*Async/request* return resolved Promises;
features is a real Set; device.lost is a forever-pending Promise), then
drives parseQuery → DefaultTestFileLoader → testcase.run for the single
failing query.

Progress: imports OK, listing/loadCases OK, `case-start` reached — then
`testcase.run` hangs with ZERO accesses to the WebGPU fake (an
instrumentation counter on every Proxy get stayed at 0), i.e. the fixture
machinery stalls before it ever asks for an adapter. Next probe: instrument
the fixture init path (common/framework fixture + webgpu/gpu_test.ts
equivalents in out/) to find the pending await — candidates: the case
timeout race (os-timer semantics vs the shims' host-pumped heap),
DevicePool's device.lost interaction (forever-pending here), or an
EventTarget wait. The binding-free question therefore remains OPEN; nothing
in this attempt contradicts the engine-defect suspicion.

## Session 2 (2026-07-11 evening, planner): four deletion experiments, one new technique, one methodology trap

**Methodology trap, recorded because it invalidated three earlier conclusions:**
a failing `cargo build` leaves the PREVIOUS binary in place, and the runner
still runs. Two "results" in this investigation (and at least one from an
earlier coding session) were produced by stale binaries. **Every deletion
experiment must now assert a fresh build (`cargo build` exit 0) before its
runs count.**

**New technique — parent scan.** At the underflow, walk `rt->gc_obj_list` and
`rt->tmp_obj_list`, run `mark_children` with a read-only probe, and print
every parent that holds an edge to the victim (plus, for plain objects, the
property atom that holds it). This finally shows the shape of the corruption
directly instead of inferring it.

**What the parent scan shows.** The victim's refcount is short by exactly
one, and it is held by exactly TWO parents of the same kind:
- run A: two detached `JSVarRef`s (gc type 3) → victim a plain Object;
- run B: two `Array`s → victim an `Error`;
- earlier runs: a plain Object → victim a bytecode function.
So an edge exists whose reference was never counted: one insertion took
ownership without a duplicate, or one release ran twice. The victim's own
transition history (now 256 first + 256 last events, full backtraces) is
regular: dup/release pairs from `js_promise_then_finally_func` and
`JS_ToBoolFree`, ending in `free_var_ref` at zero. No adapter frame appears
anywhere in it, in ANY of the captured victims.

**Deletion experiments (all on verified-fresh builds):**
1. **Unhandled-rejection tracker disabled** (`JS_SetHostPromiseRejectionTracker`
   → NULL): still aborts (134/139). The tracker is the only place the binding
   holds and frees Errors/promises — **exonerated**.
2. **Binding `gc_mark` reports nothing** (our class's tracing disabled
   entirely): still aborts. Our payload tracing — **exonerated**. (This also
   kills the "our mark over-reports edges" theory that the assertion's
   semantics first suggest.)
3. **Settlement path instrumented** (every settled object value: tracked in
   scope? refcount?): only 4 object settlements in a failing run, all
   correctly tracked and owned. **Not the hot path here.**
4. **`Scope::escape` miss counter**: 1,247 misses per run, ALL from
   `catch_callback`'s return path — and all benign: they are values duplicated
   by `return_held_value` (owned, never tracked). A red herring, recorded so
   the next session does not re-chase it. (A real invariant does hang off
   this: a returned value must be owned. It is.)
5. **Fake-GPU run** (runner installs a pure-JS Proxy instead of `wrap_gpu`, so
   zero WebGPU wrappers exist): does not abort — but the harness also does not
   complete the same work (it stalls/times out in fixture init, exactly as the
   plain-qjs harness did), so this is **inconclusive**, not exculpatory.

**Where this leaves the fork decision.** The binding's three plausible
mechanisms (rejection tracker, payload tracing, settlement ownership) are now
individually excluded by experiment, and no adapter frame appears in any
victim's full transition history. That is a strong — not yet conclusive —
case for an engine defect, and it is alive on quickjs-ng master. The one
un-excluded binding surface is the tick pump's *interleave* (ProcessEvents →
settlements → JS_ExecutePendingJob loop → release queue) and the host-function
call path, neither of which touches the victims directly.

**Next session's first three moves:**
1. Reduce inside the engine: make the parent scan run at EVERY `free_var_ref`
   / array-element release of a value whose refcount is about to hit zero, and
   report any live parent — that catches the corruption at creation time, not
   at GC time.
2. Bisect quickjs-ng between v0.15.0 and v0.15.1 (and against Bellard's
   quickjs) with the single CTS case, to date the defect.
3. If (1) names an engine line: fork, fix, regression-test in the fork, point
   the submodule at it (owner runs the network ops). If it names a binding
   line after all, fix it here.

## Session 3 (2026-07-11 late, planner): a fast reproduction, and the corruption's true shape

**1. The reproduction is now fast and near-deterministic.** quickjs ships its
own knob, `FORCE_GC_AT_MALLOC` (see `js_trigger_gc`). Building the adapter's
C compile with `-DFORCE_GC_AT_MALLOC` (a one-line `build.rs` `.define(...)`,
never committed) makes the single CTS case abort on the FIRST run, in one
pass, instead of once every few runs. **This is the tool the next session
should start from** — it shrinks the window between the corrupting operation
and the assertion to almost nothing.

**2. The defect predates our pin.** v0.15.0 reproduces the abort exactly like
v0.15.1 (3/6 runs, same assertion). The six commits between the two tags touch
nothing related. Bisecting that range is pointless; if a bisection is ever
wanted it must span a much longer history.

**3. Under forced GC the victim is one of OUR wrappers — but its history is
still engine-only.** A clean, generation-safe ledger (the reclamation path now
retires the live record even when the free table is full — the earlier mixed
histories were two objects sharing one recycled address) gives, for the
underflowing object, exactly two transitions:

```
event 0  duplicate  rc 1 -> 2   close_var_ref < close_var_refs < async_func_free < js_async_function_terminate
event 1  release    rc 2 -> 1   async_func_free < js_async_function_terminate < js_async_function_free0
```

i.e. `async_func_free()` closes the frame's captured variables (each
`close_var_ref` duplicates the stack value) and then frees the frame's stack
slots — locally balanced, ending with the detached `JSVarRef` holding the one
remaining reference. Yet at the next GC the object's refcount is already 0
while that same `JSVarRef` still points at it, and the memory has been
recycled (its gc type reads as garbage). **The var_ref outlives a value whose
count reached zero** — a use-after-free, with the extra release happening
outside this object's recorded transitions (i.e. through a path that does not
pass the refcount hooks, or on a stale copy of the value).

Victims observed under forced GC include `GPUTextureView` (one of our class
instances) as well as plain Errors, closures, and promise-resolve functions —
consistent with "whatever the async frame happened to capture", not with a
specific binding object.

**4. Still not reproduced binding-free.** The synthetic pure-JS workload
(async + finally + captured closures + rejections), re-run under a plain qjs
built with `FORCE_GC_AT_MALLOC`, stays quiet. The real CTS harness under plain
qjs still stalls in fixture init (see session 1) — finishing that port remains
the cleanest path to a binding-free proof.

**Next session, in order:**
1. Rebuild with `FORCE_GC_AT_MALLOC` (fast repro) and instrument
   `async_func_free` / `close_var_refs` / `js_async_function_resume` to dump
   the frame's var_refs, their `is_detached` flags, `pvalue` targets, and the
   stack slot range being freed — the imbalance must be visible there.
2. Specifically test the hypothesis that a captured slot is freed twice, or
   that a var_ref's `pvalue` still points into `arg_buf` after the buffer is
   released (a var_ref that was created but not closed, or closed against a
   stale frame).
3. If confirmed as an engine defect: fork quickjs-ng, fix, add the reduction as
   a regression test in the fork, repoint the submodule (owner runs the network
   ops). The fork decision is already approved in principle.
