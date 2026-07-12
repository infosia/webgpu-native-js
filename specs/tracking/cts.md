# CTS runner — tracking (block 13)

**Engine correction (2026-07-12):** Phase A and early Phase B entries below
record QuickJS measurements from before the owner decision. The runner is now
Boa-only; the authoritative retained gates are 1,312/1,312 curated CTS cases
and byte-identical Boa/JSC parity.

## Phase A — bootstrap: COMPLETE (2026-07-12)

**Result: `unittests:*` — 1,031/1,031 pass, exit 0, ~10s warm (~102
cases/second).** The CTS framework's own self-test suite runs entirely inside
QuickJS on this binding's module loader and shims.

### The §5 questions, answered empirically

1. **Build output layout**: the standalone build (`npm run standalone`)
   emits clean ESM with explicit `.js` import extensions under `out/`.
   Framework entries used by the glue: `common/internal/file_loader.js`
   (DefaultTestFileLoader), `common/internal/query/parseQuery.js`,
   `common/internal/logging/logger.js`, `common/framework/test_config.js`.
   Dynamic imports (listings/specs) are importer-relative
   (`../../<suite>/listing.js`) — **zero loader changes needed**; block 12's
   machinery (aliases exact-match for the five glue entries, everything else
   importer-relative) just worked. The transform hook was NOT needed —
   Babel's output runs untransformed on QuickJS-ng.
2. **JS feature gaps**: **none at the language level.** Missing were Web
   GLOBALS only: EventTarget, MessageEvent (minimal shims), plus the planned
   timers/performance/console/TextEncoder/TextDecoder/DOMException set.
   Shims actually exercised by `unittests:*`: performance.now,
   MessageEvent, setTimeout, TextEncoder.encode, EventTarget, DOMException.
3. **Device acquisition/pooling**: not yet exercised (unittests need no
   WebGPU) — Phase B question.
4. **Directory listing**: not needed — the CTS's generated `listing.js`
   modules carry the enumeration; the loader reads files only, and that
   suffices.
5. **Throughput**: ~102 cases/second on the unittests suite (interpreter,
   debug build). Sizing note for Phase B: a 10k-case validation suite ≈
   ~2 minutes at this rate — CI-viable; measure again on the real suite.

### Pins

CTS checkout at Dawn's DEPS pin (lockstep with the oracle protocol):
`e8389d86` (local short: e8389d86fc5). Built with `npm ci && npm run
standalone` by the owner. Recorded in tools/cts-runner/README.md (verified present after the review fixes).

### Runner shape (as landed)

A-1: crate `tools/cts-runner` — CLI (--query/--suite/--expectations/--list/
--timeout-secs), host fns (__report/__list/print/__perf_now/__log_shim),
eval_module + tick until completion, exit-code table (unexpected-pass = fail).
A-2: real glue (parseQuery → loadCases → Logger → per-case run → __report),
shims per C3, five exact aliases. Planner decision: `call_global_function`
was NOT restored — JS→host reporting suffices (recorded against the block's
inventory note).

### Phase A review (one focused lens) — closed 2026-07-12

0 CRITICAL / 3 MAJOR / 8 MINOR. The MAJORs: the README pin was missing while
this file claimed it existed (the recurring record-honesty class — fixed, and
the false claim above is annotated rather than erased); the acceptance suite
file was not committed (now `suites/unittests.txt` — the 1,031-case run is
reproducible from the tree); and `setInterval(0)` looped forever inside one
eval, unreachable by `--timeout-secs` (repeating timers now re-arm after the
drain with a fresh now — a bare-Runtime regression test pins it). Selected
minors fixed: clearTimeout no-op semantics + cancellation-set hygiene, the
exit-code deviation from C2's letter documented as deliberate
(skip-under-expected-fail = mismatch, stale-expectation hygiene), all eight
summary counters printed, empty `--list` exits nonzero, the Bool/String/Null
host-return paths tested. Recorded approximations (from the review, kept):
expectations are Rust-side query-prefix matching, NOT the framework's
subcase-level expectations — a case failing 1 of 100 subcases can only be
expected wholesale (revisit if Phase B needs finer grain); glue/shims are
covered by the live CTS run plus targeted shim unit tests, not by a full
offline harness — acceptable for the spike, stated here. expectations.txt
deliberately does not exist yet (unittests needed zero entries); Phase B
creates it with the codegen-deltas-derived initial population per C5.

## Phase B — headless validation subset

**B-1 landed (2026-07-12): requiredFeatures/requiredLimits plumbed (C7).**
The block-10 recorded gap closes: requestDevice converts the feature-name
sequence through the generated enum join and the requiredLimits RECORD type
(a new WebIDL shape — string-keyed open dictionary) through a new additive
`JsEngine::own_property_names` primitive (both engines + mock, per J13).
Unknown feature → TypeError; unknown limit key → OperationError (spec wording
quoted from the pin); undefined values → the header's UINT32/64_MAX
sentinels; compatibility chain mirrored from block 10 in reverse. Timestamp
query sets now creatable under a requesting device (tested); the parity
features line finally observes ordering with two features (block 10's
rescoped I7 claim can un-rescope) — 123 lines, byte-identical on yawgpu AND
Dawn (gated run: Dawn's Metal adapter advertises timestamp-query, confirmed).
timestampWrites conversion itself stays skipped with an updated reason —
both IDL timestamp dicts join one shared C struct, a name-map shape deferred
to its own slice. Suites: core 138, JSC 29+1.

**B-2/B-3 landed (2026-07-12): 467 real validation cases green, and the
oracle starts earning.** The CTS's DevicePool drives our binding through the
navigator.gpu shim unchanged; constants namespaces come from the CTS's own
canonical module; a non-constructible GPUDevice global satisfies fixture
cleanup. `suites/validation-core.txt` (467 cases: buffer create/mapping,
texture create, bind group family, pipeline, sampler) runs green at ~111
cases/s with `expectations.txt` carrying three reasoned entries (the
nullable-layout-element delta). **The triage found six suspected binding
bugs and one scale blocker** — planner-confirmed against the pins:

1. createBuffer mappedAtCreation size%4 must throw RangeError synchronously
   (spec) — missing. FIX.
2. mapAsync on a destroyed buffer throws synchronously where a
   promise-returning method must reject — FIX + audit every promise-returning
   method for the same class.
3. `GPUDevice.destroy()` — in the IDL, never implemented. FIX.
4. `GPUBindingResource` in the pinned IDL includes **direct GPUBuffer and
   GPUTexture** (the modern shorthand; verified at webgpu.idl:588) — our
   union rejects both. Texture-direct needs implicit-default-view machinery
   (which could also retire the render-attachment view-only delta). FIX.
5. `createComputePipelineAsync`/`createRenderPipelineAsync` — missing;
   implementable on the standard settlement machinery. FIX.
6. Pipeline `constants` — the deferral reason expired the moment
   own_property_names landed (B-1); record<USVString, double> →
   WGPUConstantEntry array. FIX.
7. Transient-attachment validation: yawgpu passes cases it should reject —
   needs Dawn arbitration (Phase C material), catalogued not fixed.
8. **Scale blocker: QuickJS `gc_decref_child` assertions (exit 134/139) at
   ~1.3k–3.2k cases in one process** — either a binding refcount imbalance
   that only manifests at scale or an engine bug; dedicated investigation
   slice before suites broaden.

**B-4a landed (2026-07-12): four CTS-found bugs fixed.** (1) mappedAtCreation
size%4 → synchronous RangeError (spec quoted from the pinned gpuweb build;
the error rides the block-08 name mechanism, name="RangeError"). (2) The
promise-uniformity audit: requestAdapter, requestDevice, mapAsync,
onSubmittedWorkDone, popErrorScope now convert EVERY post-dispatch
synchronous error into a rejection (the WebIDL rule) — five methods changed,
five existing sync-throw tests flipped and listed. (3) GPUDevice.destroy()
exists at last (idempotent, R14 split, destroy-then-new-requestDevice tested
— the CTS DevicePool recovery path). (4) Pipeline constants: the deferral
died; a shape-driven record emitter (own_property_names) fills
WGPUConstantEntry arrays for compute/vertex/fragment. Parity 123 → 124,
byte-identical on yawgpu AND Dawn (gated). The 467-case seed stays exit 0.
Remaining from the triage: B-4b (direct buffer/texture binding-resource arms
+ async pipeline methods), B-4c (the gc_decref_child scale investigation),
Phase C material (transient-attachment arbitration).

**B-4b landed (2026-07-11): the union grows its direct arms; pipelines go
async.** (1) `GPUBindingResource` accepts a direct `GPUBuffer` (flattened to
`{buffer, offset: 0, size: WHOLE_SIZE}`) and a direct `GPUTexture` — the
latter creates an **implicit default view** (`wgpuTextureCreateView(texture,
NULL)`; the header marks the descriptor `WGPU_NULLABLE` and the result
`ReturnedWithOwnership`), which the bind-group wrapper owns without an extra
AddRef and releases through the release queue, failure paths symmetric.
(2) The same machinery retired the render-attachment view-only delta:
color/resolve/depth attachments accept `(GPUTexture or GPUTextureView)` per
the IDL; the delta entry in codegen-deltas.md is annotated RETIRED and the
TypeError parity pin became a positive line. (3)
`createComputePipelineAsync`/`createRenderPipelineAsync` ride the standard
settlement machinery (pure-Rust callback, AllowProcessEvents, retention
matching the sync paths). Rejections are named `OperationError` carrying
validation/internal in the message — `GPUPipelineError` is a recorded
deviation (codegen-deltas.md, Block 13 section). Parity 124 → 127,
byte-identical on yawgpu AND Dawn (gated). The 467-case seed stays exit 0.

The Dawn parity run caught one divergence — in the *suite*, not a backend:
`validationScope` discarded its body's returned promise, so the
`pipelineAsync` settlement line was fire-and-forget and printed wherever each
backend's CreatePipelineAsync callback happened to land (yawgpu after
`renderPass:chain-ok`, Dawn after `scope:querySet:null`; both drifted past
their own section's scope line). Async completion latency across ticks is
unspecified, so neither backend was wrong — the script was nondeterministic.
Fix: `validationScope` now chains `Promise.resolve(action())` before popping
the scope, making every section's async work settle before its scope line.
Suite-design rule going forward: **a parity line that depends on a settlement
must be sequenced by an await/then the section chain actually waits on;
fire-and-forget printing is a determinism bug even while both backends agree.**

CTS shape after B-4b (yawgpu, informational — not yet suite-gated):
`createBindGroup:*` 1,819/1,901 (all 82 fails are the unsupported
external-texture binding family); `compute_pipeline:*` lists 274 but aborts
on the unplumbed `wgslLanguageFeatures` before summarizing
(`compute_pipeline:basic,*` passes 2/2 incl. async); negative async cases
also surface the GPUPipelineError deviation. Remaining: B-4c (the
gc_decref_child scale investigation), Phase C material
(transient-attachment arbitration), `wgslLanguageFeatures` as a small
follow-up gap.

**Phase B-4 suite growth + engine-crash exclusion policy (2026-07-12).** The
validation-core suite grew from 467 to **1,312 cases** by adding three families
verified crash-free over >=6 isolated runs and passing under yawgpu Noop:
`encoding,cmds,copyBufferToBuffer:*` (+16), `render_pipeline,vertex_state:*`
(+679), `encoding,createRenderBundleEncoder:*` (+150). The grown suite runs 3/3
byte-stable, exit 0, 1,312 pass / 0 fail; the workspace gate stays green (331).

**Why growth is curated, not wholesale.** The B-4c engine defect
(specs/tracking/b4c-fork-handoff.md) makes for-of-heavy validation cases crash
QuickJS *probabilistically* — measured in isolation: `draw:buffer_binding_overlap`
~8/10, `createView` ~1/3 even for a single case, `draw:vertex_buffer_OOB` ~1/4.
The crash aborts the whole one-process suite run, so a suite that includes any
such family is a flaky gate. Until the fork fix lands, families are added to
the curated (reliable) suite only after passing a multi-run crash screen;
known-crashing families are deliberately excluded and listed here:
- `webgpu:api,validation,encoding,cmds,render,draw:buffer_binding_overlap:*`
- `webgpu:api,validation,encoding,cmds,render,draw:vertex_buffer_OOB:*`
- `webgpu:api,validation,createView:*`
(others likely exist; screen before adding.) These are excluded for an ENGINE
crash, distinct from binding-bug exclusions (which go in expectations.txt as
`fail`) and from `clearBuffer:*`/`beginRenderPass:*`/`error_scope:*` which have
real test *failures* (not crashes) to be triaged separately.

**Robust broad coverage remains future work.** Running the excluded families
needs per-case process isolation with retry (the crash is retry-recoverable:
the same case often passes on a re-run), so a crash aborts only one unit
instead of the gate. That driver is deferred until the fork fix is attempted;
if the fix lands first it is moot. Recorded so the next agent does not mistake
the curated suite's exclusions for missing coverage.
