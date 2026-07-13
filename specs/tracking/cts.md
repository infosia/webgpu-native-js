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

**Indirect draw/dispatch implemented (2026-07-12) — the first bug the QuickJS
crash had been hiding.** Under QuickJS these cases aborted the process, so their
failures were never observable. Boa runs them and reported
`TypeError: not a callable function` on every `drawIndirect` /
`drawIndexedIndirect` subcase. Cause: the indirect methods were simply absent
from the surface. Added, from the pins:

| Interface | Members | IDL | C ABI |
|---|---|---|---|
| `GPURenderPassEncoder` | `drawIndirect`, `drawIndexedIndirect` | `webgpu.idl:1159-1160` (`GPURenderCommandsMixin`) | `wgpuRenderPassEncoderDraw{,Indexed}Indirect` |
| `GPURenderBundleEncoder` | `drawIndirect`, `drawIndexedIndirect` | same mixin | `wgpuRenderBundleEncoderDraw{,Indexed}Indirect` |
| `GPUComputePassEncoder` | `dispatchWorkgroupsIndirect` | `webgpu.idl:1046` | `wgpuComputePassEncoderDispatchWorkgroupsIndirect` |

Result: `buffer_binding_overlap:*` **2 pass/2 fail → 4/0**;
`vertex_buffer_OOB:*` **30/30 → 60/0**. Parity 127 → 133 lines, byte-identical
under Boa and JSC.

**A premise of the handoff was wrong, and the agent said so** rather than
building on it: `setVertexBuffer`/`setIndexBuffer` do *not* capture buffers in
Rust-side encoder state — native command recording retains them. The indirect
methods follow that same established behaviour; no new wrapper-side retention
scheme was invented, and the mock tests verify native ownership transfers from
encoder to command buffer / render bundle and survives wrapper release.

**Still open in the same family:** `draw:*` as a whole is now **82 pass / 16
fail / 1 skip**. The 16 are unrelated to the indirect gap and are not yet
triaged.

## Phase B-5 (2026-07-12): the crash blocker is gone, and the suite more than doubles

Dropping QuickJS for Boa (block 14) removed the B-4c engine crash that had
forced the earlier curation. The families that used to abort the process now
*report* — and what they reported was a run of genuine, previously invisible
binding gaps. All are now fixed:

| Gap | Was | Now |
|---|---|---|
| indirect draw/dispatch (5 methods) | `TypeError: not a callable function` | implemented |
| `GPURenderPassDescriptor.maxDrawCount` | reject-if-present (expired reason) | emitted via `WGPURenderPassMaxDrawCount` chain |
| `GPUCommandEncoder.clearBuffer` | absent | implemented |
| `GPUCommandEncoder.resolveQuerySet` | absent | implemented |
| `setBlendConstant`, `setStencilReference` | absent | implemented |
| `setBindGroup` dynamic offsets (both overloads) | ignored | implemented (+ `JsEngine::is_uint32array`) |
| `GPUDebugCommandsMixin` (12 C fns) | absent | implemented |
| encoder-state violations | threw `OperationError` | **validation error to the device sink** (spec + principle 8) |

**The suite grew 1,312 → 2,822 cases**, 3/3 stable, exit 0, **0 fail**, 178
skip, 2 expected-fail. Parity grew 127 → 154 lines, byte-identical under Boa and
JSC throughout.

The two expected-fails are honest and reasoned, not swept under the rug:
- `setBindGroup:u32array_start_and_length` — **Boa engine gap**: no
  `Error.prototype.stack` (block 14 → B7, verified in Boa's own CLI). Not a
  binding defect.
- `createView:texture_view_usage_of_multiple_usages` — **yawgpu backend
  validation gap**: the binding delivers the requested view usage to the C
  descriptor (pinned by a mock regression test); the backend does not validate a
  usage subset. Parked for Dawn arbitration, never worked around in the binding.

**The earlier engine-crash exclusion list is retired.** `buffer_binding_overlap`,
`vertex_buffer_OOB`, and `createView` — the three families excluded as crashers —
are all in the suite now and all green.

**timestampWrites landed (2026-07-12) — and the record that justified skipping it
was false.** `timestampWrites` was policy-skipped in both twins with
`reject_if_present`, reasoned "timestamp-query feature not yet requested in
tests". That reason had expired: `requiredFeatures`/`requiredLimits` are fully
plumbed through `requestDevice`, and the parity suite already proved it
(`features:requested:core-features-and-limits,timestamp-query`). Worse,
`codegen-deltas.md` still asserted "`requiredFeatures`/`requiredLimits` are
unplumbed in `requestDevice` (`requiredFeatureCount` is hard-coded 0)" — **flatly
false against the code**. Both the skip and the stale record are now retired
(annotated, not erased).

Both IDL dictionaries (`GPUComputePassTimestampWrites`,
`GPURenderPassTimestampWrites`) emit through the single shared C struct
`WGPUPassTimestampWrites`; the optional write indices use the header's sentinel
when absent.

Unblocked three families that had been gated behind the skip:
`encoding,beginRenderPass:*` 3 pass/1 fail → **4/0**; `encoding,queries,*`
**22/0**; `query_set,*` **4/0**. All three added to the suite, which grows
**2,822 → 2,852 cases**, 3/3 stable, exit 0, 0 fail.

Lesson worth keeping: a policy skip's *reason* is a claim with an expiry date.
Two of them (`maxDrawCount`, `timestampWrites`) turned out to be stale within a
day of each other, and one tracking record was simply wrong. Re-read skip
reasons against the code before trusting them.

**GPUDevice becomes an EventTarget; GPUUncapturedErrorEvent lands (2026-07-12) —
retiring a Phase-6 deviation.** The recorded deviation read: "`onuncapturederror`
receives the bare `GPUError`, not a `GPUUncapturedErrorEvent` … this binding has
no EventTarget … Revisit if event plumbing ever lands." The CTS forced the issue:
`expectUncapturedError` (webgpu/error_test.js) uses
`device.addEventListener('uncapturederror', …)`, calls `event.preventDefault()`,
and reads `event.error`. Every `errorType !== errorFilter` case failed on it.

Implemented from the pins (`webgpu.idl:150` `GPUDevice : EventTarget`; `:1323`
`GPUUncapturedErrorEvent : Event` with `[SameObject] readonly error`; `:1331`
`GPUUncapturedErrorEventInit`): a minimal `Event` base (type, preventDefault,
defaultPrevented — no browser DOM invented beyond the pins), the event class, and
`addEventListener`/`removeEventListener`/`dispatchEvent` on the device. An
uncaptured error now constructs the event and dispatches it to registered
listeners *and* to `onuncapturederror` (the handler is one of the listeners, per
Web EventHandler semantics). Invariant 2 is untouched — the uncaptured-error C
callback is still the one with no mode and still marshals to the JS thread.

`error_scope:*` 35 pass / 14 fail → **37 / 12**, and **zero of the remaining 12
are binding failures**.

**All 12 are a backend limitation, recorded as expectations with the reason.**
The CTS provokes out-of-memory by creating a **256 GiB texture** (rgba32float at
max dimensions). yawgpu's Noop backend allocates it without failing, so no
`GPUOutOfMemoryError` is ever raised. The binding's OOM path itself is pinned by
the parity suite (`error:GPUOutOfMemoryError`). Verify on a real backend (Dawn)
— never worked around in the binding.

Suite grows **2,852 → 2,889 cases**, 3/3 stable, exit 0, 0 fail, 14
expected-fail (all the Noop OOM cases).

## Phase B-6 (2026-07-13) — the unscreened families, and what running them found

Every remaining `api,validation` family was screened. Suite grows **2,889 →
23,178 cases**, 3/3 stable, exit 0, 0 fail, 16 expected-fail. Three findings,
in ascending order of how much they had been hiding.

**Finding 1 — most families were already green; they had simply never been
run.** `image_copy,*` (6,557), `encoding,cmds,copyTextureToTexture:*` (3,194),
`layout_shader_compat:*` (110), the four `texture,*` families, `buffer,destroy`,
`getBindGroupLayout`, `debugMarker`, `dispatch`, `encoding,cmds,compute_pass`,
`beginComputePass` — all passed on first contact, no binding change. Coverage was
bounded by what we had thought to run, not by what worked.

**Finding 2 — the binding installed no WebIDL interface objects.** The adapters'
`register_class` put a global on the object graph **only when the class had a
constructor**, and most classes registered lazily on first instance creation. So
`GPURenderPassEncoder`, `GPUComputePassEncoder`, `GPUComputePipeline` and the
rest simply did not exist as globals. Per WebIDL every exposed interface gets an
interface object whose `prototype` is the interface prototype object; a
non-constructible one throws `TypeError: Illegal constructor` when called.

The CTS leans on this hard — it feature-detects with
`'setImmediates' in GPURenderPassEncoder.prototype` (`supportsImmediateData`, in
the CTS's `common/util/util.js`) and asserts `instanceof`. Failures read
`ReferenceError: GPURenderPassEncoder is not defined`.

Fixed in both adapters plus the mock, with eager registration of every class
(the generator now emits a `register_generated_classes` inventory; `wrap_gpu` and
`wrap_device` both call it, so host adoption — invariant 6 — reaches the same
complete surface). Results: `resource_usages,*` 3,286 pass/280 fail → **3,566/0**;
`encoding,encoder_open_state:*` → **47/0**; `createPipelineLayout:*` 3/11 →
**11/3** (the 3 are the recorded null-BGL nullability deviation).

*A cross-engine parity bug surfaced inside this fix, and it is the reason the
parity suite exists.* JSC's `JSObjectMakeConstructor` defines
`prototype.constructor` as **enumerable**; Boa defines it non-enumerable (ES
semantics: writable, non-enumerable, configurable). So `Object.keys(X.prototype)`
would have differed by engine. JSC's public C API has no `JSObjectDefineProperty`
and `JSObjectSetProperty` follows *assignment* semantics — it honours the
inherited `constructor` and recreates the own property with default attributes.
The adapter therefore detaches the prototype chain, defines the property with
`DontEnum`, and restores the chain. Ugly, documented, and correct.

**Finding 3 — `GPUPipelineError` was the single thing standing between us and
2,000 cases.** After findings 1–2, **the sole remaining failure message** in four
whole families was `THREW OperationError, instead of GPUPipelineError`. The B-4b
deviation had said to revisit "when a second DOMException-subclass consumer
appears **or a CTS family blocks on it**" — four did.

Implemented from the pins (`webgpu.idl` `GPUPipelineError : DOMException` with
`constructor(optional DOMString message = "", GPUPipelineErrorInit options)` and
readonly `reason`): a minimal `DOMException` base — the same
build-only-what-the-pins-need approach the `Event` base took — and
`GPUPipelineError` emitted through the existing policy-driven constructor
machinery (`codegen/policy.toml`, not hand-written). Async pipeline creation now
rejects with a real `GPUPipelineError` carrying `name` and `reason`; the
synchronous paths still raise device validation errors.

The engine boundary held: the only core change was an additive
`ClassParent::IntrinsicError` and a `new_error_instance` trait method, so a class
prototype can inherit the engine's intrinsic `Error.prototype` (the CTS's
`shouldReject` requires `ex instanceof Error`). No engine types in `core/`.

Results — all four families go **fully green**: `render_pipeline,*` 4,467
pass/1,772 fail → **6,239/0**; `compute_pipeline:*` 128/146 → **274/0**;
`shader_module,*` 26/12 → **38/0**; `non_filterable_texture:*` 96/64 →
**160/0**. `webgpu:idl,constructable:*` (which constructs a `GPUPipelineError`
directly) is now **8/0** and joins the suite.

### Two runner shims, and one honest limit

Boa has no `queueMicrotask` and no `Error.prototype.stack` (the latter is block
14 → B7). The CTS asserts `typeof ex.stack === "string"` on every expected
throw/rejection, which was costing thousands of otherwise-passing subcases. Both
are **runner** shims (`tools/cts-runner/shims.js`), not binding behaviour: the
stack shim is guarded (installed only if the engine has no stack) and returns a
synthetic string that says so.

Two CTS self-tests assert real stack *contents* (`.spec.js` frame locations),
which no synthetic string can satisfy. They stay expected-fail, with that reason,
until the engine gap closes — as do the two `determinantInterval` numeric
failures B7 recorded but never characterized.

### Families still red, and why (all catalogued, none worked around)

- `capability_checks,*` (10,954 cases) — blocked by a **Boa engine bug**, now
  isolated (below). Not a binding gap.
- `encoding,programmable,*` — `pipeline_immediate` (immediate data / push
  constants, unimplemented) and `TypeError: GPUBindGroupLayout is required` (the
  null-BGL nullability deviation).
- `createBindGroup:external_texture,*` (82) — external textures are out of scope.
- `createTexture:texture_usage` (42), `render_pass_descriptor:loadOp_storeOp`,
  `queue,submit` command-buffer reuse, `buffer,mapping` — a mix of yawgpu Noop
  backend gaps and suspected binding bugs; **untriaged**, and the honest state is
  that they are not yet separated. Next slice.

### The Boa bug blocking `capability_checks` — characterized, not guessed

The CTS builds its limit fixtures with (`capability_checks/limits/limit_utils.js`,
`makeLimitTestFixture`):

```js
function makeLimitTestFixture(limit, params) {
  class LimitTests extends LimitTestsImpl {
    limit = limit;                      // class field initializer
    limitTestParams = params ?? {};
  }
  return LimitTests;
}
```

Under Boa 0.21.1 this throws `ReferenceError: access of uninitialized binding`.
A minimal repro run directly against the adapter isolates it, and the shape is
**not** what the CTS source suggests:

| Case | Result |
|---|---|
| field initializer reads an enclosing **function parameter** | **throws** |
| field initializer reads an enclosing **function-local `let`** | **throws** |
| field initializer reads an enclosing **function-local `var`** | **throws** |
| **static** field initializer reads an enclosing function param | **throws** |
| field initializer reads a **global** binding | OK |
| field initializer that is a **constant** | OK |
| field initializer reads the param **through an arrow function** — `x = (() => v)()` | **OK** |
| an ordinary **method** reads the same enclosing binding | OK |

So it is *not* about the field name shadowing the outer name (`limit = limit`) —
a differently-named binding fails identically. **A class field initializer cannot
read any binding from an enclosing function scope**, while a method in the same
class can, and an arrow *inside* the initializer can. The closure machinery
works; the field-initializer's own synthetic function is being given the wrong
outer environment.

This is an engine defect, catalogued here per the operational rule. It gates
`capability_checks,*` entirely (10,954 cases — the single largest remaining
family) and cannot be fixed in the binding: the offending code is the CTS's, the
JS is valid, and no shim can reach into a class body. Boa is pure Rust and
MIT/Unlicense, so unlike the QuickJS defect this one is *fixable by us* — but
doing so means pinning a patched Boa, which contradicts the standing "crates.io,
exact version pin, never a filesystem path" dependency rule (block 14).

**Owner decision (2026-07-13): catalogue and move on — do not fork Boa.** The
dependency rule stands; `capability_checks,*` stays out of the suite with this
entry as its reason. Revisit if the family becomes load-bearing, or if a Boa
release fixes it (re-test the repro above against each pin bump — the repro is
the acceptance test).

## Phase B-7 (2026-07-13) — error routing, and a crash that was not what it looked like

### Four spec-shaped error-routing bugs, all the same mistake

Commit 45d18fc established the rule (principle 8): *a spec-level validation
failure routes to the device error sink, not to a JS exception.* Screening found
four sites that rule had not reached.

1. **`queue.submit()` of a consumed command buffer threw `OperationError`.** The
   spec makes it a validation error. The throw also escaped the CTS's
   `expectValidationError`, leaving its pushed scope on the stack — the
   "extra error scope" failure was a *consequence*, not a second bug. A failed
   submit must also invalidate every command buffer passed to it.
2. **A lost device still surfaced binding-originated validation errors.** Spec
   §22: *"No errors are generated from a device which is lost."* Our client-side
   encoder/command-buffer state checks fired regardless. The CTS's
   `executeAfterDestroy` runs the same operation twice — once live (must be
   clean), once after `destroy()` (must produce **no uncaptured error**) — and we
   failed the second. Lost is now recorded synchronously at `destroy()`, at the
   native lost callback, and on the adopted-device path; suppression covers error
   scopes, the uncaptured-error queue, and the settlement race. Live-device
   behaviour is unchanged and pinned by regression tests (`encoding,encoder_state`
   stays green).
   - Same rule, async arm: on an already-lost device
     `createRenderPipelineAsync`/`createComputePipelineAsync` must **resolve**,
     not reject — even for a descriptor that is invalid on a live device.
3. **`mapAsync` threw synchronously and never validated buffer state.** It must
   *never* throw — it always returns a promise. The buffer-state checks (already
   mapped / mappedAtCreation / mapping pending) belong on the content timeline:
   reject the returned promise **immediately** with `OperationError` *and* raise a
   validation error. Descriptor failures (usage, alignment, OOB, invalid,
   destroyed) reject deferred; a pending map cancelled by `unmap()`/`destroy()`
   rejects with `AbortError` and no validation error. We had handed all of it to
   the backend, which variously succeeded, rejected late, or rejected with the
   wrong name.
4. **`TypeError` vs `OperationError` was conflated in range checks**
   (`queue.writeBuffer`, `getMappedRange`). The rule that fell out: `[EnforceRange]`
   coercion failure of a *supplied argument* is a `TypeError`; failure of the
   resulting *range* against the resource is an `OperationError`. `getMappedRange`
   made this vivid — with `size` omitted its default is `max(0, buffer.size -
   offset)`, and we underflowed and reported a `TypeError` naming an argument the
   script never passed.

Results: `queue,*` 37 pass/3 fail → **40/0**; `buffer,mapping:*` 22/13 → **35/0**;
`state,device_lost,*` 3,288/44 → **3,332/0**. The first two join the suite, which
grows **23,178 → 23,253 cases**, 3/3 stable, exit 0, 0 fail.

### `device_lost` passes but is NOT in the suite — a Boa crash, and a correction

`state,device_lost,*` reaches 3,332/0, but the family **intermittently aborts the
process** (`exit 134`): Boa panics inside its own Map builtin during garbage
collection —

```
boa_engine-0.21.1/src/builtins/map/ordered_map.rs:225
  Object already borrowed: BorrowMutError
  <MapLock as Finalize>::finalize
  ...GcBox<VTableObject<MapIterator>> drop...
panic in a destructor during cleanup — thread caused non-unwinding panic. aborting.
```

A GC that runs while a JS `Map` is being iterated re-enters `MapLock`'s finalizer,
which `unwrap()`s a `borrow_mut()`. It aborts rather than unwinds, so no `catch`
at any layer can contain it.

**Correction, recorded because the reasoning error is the lesson.** The crash
first appeared immediately after a slice landed, and the obvious inference — "the
new code introduced it" — was wrong. Re-running the *unmodified* HEAD binary
aborted **2 of 3 times**. The earlier clean run of that family had simply been a
lucky one. The crash is **pre-existing and non-deterministic**; the new code only
changed how often it is hit. *Assert a fresh build and re-run the baseline before
attributing a nondeterministic failure to the change in front of you* — the same
trap as the stale-binary incident, wearing different clothes.

Naive repros (iterating a large `Map` while allocating hard, forcing GC between
rounds) do **not** reproduce it; the trigger needs a collection during Map
iteration *inside a native call*, which is what the CTS's promise-heavy
device-lost path produces. Not chased further.

Consequence: the family stays out of the curated suite. A flaky abort in the gate
is worth more damage than the coverage is worth, and the curated suite is 3/3
stable precisely because it excludes it. This is the second Boa engine defect
catalogued (with the class-field scope bug), and unlike that one it is a **crash**
— relevant to Boa's production suitability, not just to CTS coverage.

### Backend gaps confirmed, with the blame isolated before it was assigned

`createTexture:texture_usage` (42) and `render_pass,render_pass_descriptor`
(121 subcases) fail as *"Validation succeeded unexpectedly"* on
**transient-attachment** rules: the CTS gates these behind
`'TRANSIENT_ATTACHMENT' in GPUTextureUsage`, which is true here because the
pinned `webgpu.h` has `WGPUTextureUsage_TransientAttachment` (0x20).

A dropped usage bit in the *binding* would produce an identical symptom, so per
the D11 lesson the paths were separated before blame was assigned: a texture
created with `usage: RENDER_ATTACHMENT | TRANSIENT_ATTACHMENT` and `dimension:
"3d"` reads **`texture.usage === 48`** back through `wgpuTextureGetUsage`. The
binding forwards the bit; the backend creates the texture without complaint.
**Backend validation gap**, parked for Dawn arbitration — same class as the
recorded `createView` view-usage gap. Never worked around in the binding.

## Phase C (2026-07-13) — the Dawn oracle runs

Backend: Dawn, built locally, real GPU (Metal), gated and unsandboxed — never CI.
The oracle's precondition holds: our `third_party/webgpu-headers` pin is
`a11ef44…`, byte-identical to Dawn's `DEPS` pin, so both sides speak the same ABI.

### C1 — the curated validation suite on Dawn

**23,247 pass / 6 fail / 13 unexpected-pass**, against a suite that is 23,253 / 0
on yawgpu Noop. Every number in that line is a finding.

*(One methodology note: the first C1 attempt died at the runner's default 300 s
timeout, which reports as a plain failure. Dawn is a real GPU and the suite is
23 k cases; `--timeout-secs` is not optional at this size.)*

**The 13 unexpected-passes are the arbitration.** Each is an expectation we had
recorded against yawgpu's Noop backend, and Dawn passes all of them — confirming
every one as a **backend** limitation, not a binding bug:

- `error_scope:*` — 12 cases. Dawn is **49/0** where yawgpu Noop is 37/12. Dawn
  can actually run out of memory (the CTS provokes it with a 256 GiB texture);
  Noop allocates it without complaint. The binding's OOM path was right all along.
- `createView:texture_view_usage_of_multiple_usages` — Dawn is **1192/0** where
  yawgpu is 1191/1. Dawn validates the view-usage subset; yawgpu does not.

Two further catalogued gaps arbitrate the same way, outside the curated suite:

- **D14 (transient attachments) — CONFIRMED a yawgpu gap.** `createTexture:*` is
  **3143/0** on Dawn vs 3097/46 on yawgpu; `render_pass,*` is **262/1** vs
  212/51. Dawn enforces every transient-attachment rule.

**The 6 Dawn-only failures are presumed binding bugs** (oracle protocol), and both
families are the same shape: *a binding gap that yawgpu's stricter behaviour had
been hiding.*

- `encoding,encoder_state:pass_end_invalid_order` (4) — "Validation succeeded
  unexpectedly". This family is 16/0 on yawgpu, so the error we were relying on
  was **yawgpu's, not ours**. Dawn does not raise it, and neither do we.
- `buffer,mapping:getMappedRange,disjointRanges{,_many}` (2) — "unexpectedly did
  not throw". Overlapping mapped ranges must be rejected on the content timeline
  by the binding, which tracks them. We delegate to
  `wgpuBufferGetMappedRange`, which returns NULL on yawgpu (so we threw by
  accident) and succeeds on Dawn (so we did not).

**Both are now fixed, and the fix was confirmed on Dawn** (it cannot be confirmed
on yawgpu — neither bug is observable there):

- `getMappedRange` now tracks the ranges it has handed out for the current mapping
  and throws `OperationError` on overlap, per the IDL (this one *is* a synchronous
  throw, not a device error — the distinction this project keeps getting wrong).
  Non-overlap is `a.start >= b.end || b.start >= a.end`, so ranges that merely touch
  are fine and an empty range inside a non-empty one is not. `unmap()` clears the
  bookkeeping; a re-`mapAsync` starts clean.
- Beginning a second pass while one is open now invalidates the command encoder and
  surfaces a `GPUValidationError` at `finish()` (which returns an invalid command
  buffer) — extending the encoder-state machinery from 45d18fc rather than inventing
  a parallel one.

**C1 re-run on Dawn: 23,305 pass / 0 fail.** The suite is now zero-fail on *both*
backends. (The run still exits nonzero on Dawn, and correctly so: the 13
unexpected-passes are the Noop-based expectations that Dawn passes. That is the
arbitration signal, not a regression — the runner has no backend-conditional
expectation syntax, and adding one would hide exactly the thing we want shouted.)

### C2 — the first `api,operation` families ever run, and what they cost us

`api,operation` had never been run against anything. Four families are green on
both backends on first contact (`queue`, `onSubmittedWorkDone`, `device`, and —
after the fixes below — `reflection` and `labels`). Getting there exposed **four
WebIDL conformance bugs that no validation family reaches**, three of which fail
identically on yawgpu *and* Dawn — so they were never Dawn-specific, only
never-tested.

1. **Prototype properties were non-enumerable; WebIDL requires them enumerable.**
   `reflection:*` was 2/4. The CTS reflects with a plain **`for...in`**
   (`extractValuePropertyKeys`), which sees only enumerable properties — and our
   accessors were installed CONFIGURABLE-only (Boa) and our methods with
   `DontEnum` (JSC). WebIDL puts operations at
   `{writable, enumerable, configurable}` and attributes at
   `{enumerable, configurable}`; only `constructor` is non-enumerable. **We had
   it exactly backwards**, in both adapters. Now **6/0**.
2. **`label` existed on 3 of 19 interfaces.** `labels:*` was 4/16. The IDL puts
   `label` on `GPUObjectBase` — every WebGPU object — as a *writable* attribute;
   policy listed it only for `GPUBuffer`, `GPUSampler`, `GPUQuerySet`. It must
   round-trip the descriptor's label, survive `destroy()`, and carry embedded NULs
   and non-BMP text (`WGPUStringView` has an explicit length, so it can). Now
   **20/0**.
3. **`GPUBuffer.mapState` did not exist.** In the IDL, absent from the subset.
   Added; the state machine was already there.
4. **`depthSlice`: a C sentinel collided with a legal script value.** See below —
   it is the most interesting bug of the phase.

### The depthSlice sentinel collision — a hazard class, not a one-off

`WGPU_DEPTH_SLICE_UNDEFINED` is **0xFFFFFFFF**. `depthSlice` is a
`GPUIntegerCoordinate` — an unsigned long — so **0xFFFFFFFF is a value a script
may legally pass**. At the C ABI, "the script passed 0xFFFFFFFF" and "the script
omitted it" are the same 32 bits. The CTS knows this and says so in its own test
description: *"The special value '0xFFFFFFFF' is not treated as 'undefined'."*

The binding was forwarding the value faithfully — that was never the bug. The bug
is that **no backend can enforce a distinction the ABI cannot express**, so the
binding must decide presence on the JS side (`is_undefined`) and raise the
validation error itself. It now does, for all six definedness rows and the mip-
level bound check, which required view wrappers to retain their effective
dimension and per-mip depth.

Recorded as a **codegen/ABI delta** (`codegen-deltas.md`): wherever a C "undefined"
sentinel lies inside the range of its IDL type, the binding — not the backend —
owns that validation. `depthSlice` is unlikely to be the only such member.

### Does the Boa GC crash reach the curated suite? Measured: no, in 6/6 runs

The coding agent reported one `exit 134` on the curated suite, which would have
changed the crash's severity from "a CTS coverage problem" to "the gate is flaky".
Measured with a **pinned binary copy** (see the trap below): **6 consecutive clean
runs**, 23,305 / 0 every time.

State it precisely, because the two claims are different:

- The crash **is** confirmed in `state,device_lost,*` — 2 aborts in 3 runs of the
  *unmodified HEAD* binary. That family stays out of the suite.
- The curated suite is **not proven crash-free**; it is *unreproduced* in 6 clean
  runs. The one contrary report came from an agent that rebuilds
  `target/release/cts-runner` constantly, so it is exactly as vulnerable to the
  binary-contamination trap below as the planner turned out to be. Treat it as
  unexplained, not as refuted, and re-measure if it recurs.

### A third instance of the same trap — and it is about tooling, not luck

While measuring how often the Boa GC crash hits the curated suite, the planner
launched a five-run loop against `target/release/cts-runner` and then, *while the
loop was still running*, rebuilt that same path with `--features backend-dawn` to
do an oracle run. Runs 2–5 therefore executed the **Dawn** binary against yawgpu's
library directory and produced a confident, stable-looking "16 failures, 1
unexpected-pass" — a number that measured nothing.

This is the **third** appearance of the same trap in this project:

1. A failing `cargo build` left the previous binary in place, and three B-4c
   conclusions were drawn from it.
2. A nondeterministic crash was attributed to the slice that had just landed,
   before the baseline was re-run (it aborted 2-of-3 at HEAD).
3. This one: a *successful* build silently replaced the binary a running
   measurement depended on.

The common shape is that **`target/release/<bin>` is shared mutable state**, and a
measurement that spans time does not own it. The rule that follows is mechanical,
not a matter of care: **copy the binary you are measuring to a fixed path and run
that copy.** Every multi-run or backgrounded measurement in this file from here on
does so.

### Acceptance

Block 13's Phase C acceptance asked for *"at least one binding bug found-and-fixed
via the oracle, to prove the loop works (if literally none surface, say so — do not
manufacture)."* Four surfaced and were fixed; two more are open with the oracle
pointing straight at them. The loop works.

Suite grows **23,253 → 23,305 cases** (the green `api,operation` families join),
exit 0, 0 fail on yawgpu.
