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
