# Block 20 — extension `destroy()`: a bounded release path for every retained wrapper

Owner decision (2026-07-17): extend the JS surface with a non-standard
`destroy()` method on retained wrapper types that the WebGPU spec leaves
without one. Rules **L1–L11**.

## 1. Why the surface is extended

`release-queue.md` → R3/R4: under JavaScriptCore, finalizer timing is not
controllable through the public C API, and a quiet heap may not finalize until
context teardown (upstream-corroborated: `JSGarbageCollect` is a scheduling
hint by design). WebGPU gives `destroy()` only to GPUBuffer, GPUTexture,
GPUQuerySet, and GPUDevice — the browser's judgement of which objects hold
enough GPU memory to warrant it, made for engines whose GC runs finalizers
promptly. Every other retained wrapper type here has **no bounded release
path**: a churned bind group, pipeline, or render bundle waits for a finalizer
that under JSC may never come before teardown (block 19 → K6 recorded this for
bundles and mitigated by making re-record rare; the mitigation stands, this
block removes the unboundedness).

The deviation is deliberate and recorded, not hidden. The WebGPU API is
respected everywhere it can be; where its assumptions (prompt finalizers) do
not hold in this embedding, the API is extended in its own idiom.

Evidence gathered before this spec (2026-07-17):

- No CTS family in the curated suites checks interface member sets for
  *absence* of extras. `idl,exposed` is a browser-only `.html.ts` harness and
  not runnable here; `idl_test.ts`, `javascript.spec.ts`, and
  `reflection.spec.ts` contain no such enumeration assertions.
- codegen's subset⋈IDL join is checked in both directions, so a non-IDL member
  requires an explicit extension mechanism (L7), not a subset-list entry.
- GPUCommandBuffer is consumed by `submit` and has its own release path;
  encoders are transient (`finish()`/`end()` consumes them). Neither needs
  `destroy()`.

## 2. Scope

**In.** `destroy()` on the nine retained types listed in L1; the codegen
extension mechanism (L7); core semantics + unit tests; parity coverage; CTS
re-runs; `examples/bounce` adopting `destroy()` for the superseded bundle
(L10); documentation (L11).

**Out.** `destroy()` on encoders and GPUCommandBuffer (transient/consumed —
reasons above; revisit only on evidence of abandoned-encoder churn).
Any change to the four spec-`destroy()` types' behaviour. A general
`FinalizationRegistry`/weak-callback surface. Host-side (Rust) retire APIs —
the JS surface is the chosen shape.

## 3. Rules

**L1 — the extension set.** `destroy()` is added to: GPURenderBundle,
GPUBindGroup, GPUComputePipeline, GPURenderPipeline, GPUSampler,
GPUShaderModule, GPUTextureView, GPUBindGroupLayout, GPUPipelineLayout.
(GPUBuffer, GPUTexture, GPUQuerySet, GPUDevice already carry spec
`destroy()`.)

**L2 — semantics.** `destroy()` retires the wrapper: its native reference and
every native parent AddRef it holds (invariant 4 pairing) are pushed onto the
release queue, and the wrapper's handle slot is emptied. Idempotent: a second
`destroy()` is a no-op. Runs only on the JS thread by construction (it is a JS
method), so queueing is for path-unity with finalizers, not thread-safety.

**L3 — use-after-destroy is a synchronous OperationError.** Any member or
conversion that would reach the native handle of a retired wrapper throws
OperationError naming the interface (honest-mistake catching, invariant 8).
This includes passing a retired wrapper inside a descriptor or command call
(`setBindGroup`, `executeBundles`, pipeline descriptors, …): the conversion
path checks before the ABI is reached — a dangling handle is never handed to
the backend. Cached JS-side state that needs no native call (`label` if
binding-cached) may keep working; nothing native-reaching may.

**L4 — the finalizer must not double-release.** After `destroy()`, the
eventual finalizer of the same wrapper finds the handle slot empty and pushes
nothing. Pin with a test.

**L5 — in-flight GPU work is the backend's concern.** `wgpuXxxRelease` is a
refcount drop; work submitted to a queue retains what it needs (webgpu.h
ownership model). `destroy()` therefore needs no host synchronization — but
the host-borrow hazard stands: a native handle the host borrowed
(`native_render_bundle`) is not protected from script `destroy()`. Scripts are
trusted (invariant 8); the ordering obligation is documented where the borrow
pattern is documented (L10, L11).

**L6 — release is observable this frame.** A `destroy()` called inside a
`frame()` callback has its native release executed by that same frame's step-6
drain (block 15 ordering). Pin with a test.

**L7 — codegen extension mechanism.** `policy.toml` gains an explicit
extension-members section; every entry carries a required `reason`. The
subset⋈IDL join treats extension members as: **must not exist in the pinned
IDL** — if a future `webgpu.idl` pin adds a same-named member to that
interface, codegen fails the build and forces a review (collision detection,
not silent shadowing). Generated output marks extension members as
non-standard in the generated doc comment.

**L8 — tests.** Per-type inline unit tests (destroy → queued release observed;
double-destroy no-op; use-after-destroy OperationError including
inside-descriptor use; finalizer no-double-release), mock-engine first.
`parity.js` gains a destroy section exercising at least: bundle destroy +
use-after-destroy error identity, bind group destroy inside a frame, and
double-destroy — byte-identical on both engines.

**L9 — CTS re-runs.** After implementation, re-run at minimum:
`idl,javascript`, `idl,constants,flags`, `api,operation,reflection`,
`idl,constructable`, and the validation families whose objects gained the
method (`createBindGroup`, `render_pipeline`, `compute_pipeline`,
`createView`, `createSampler`, `shader_module`, `render_bundle` families).
Expected: zero new failures (per the §1 evidence). Any new failure is triaged
as a finding, never expectation'd without a recorded reason.

**L10 — `examples/bounce` adopts it.** The frame-45 update destroys the
superseded bundle immediately after reassigning `globalThis.bounceBundle` and
signalling: the host has already stopped using the old handle after the
previous frame's submit, in-flight work is backend-retained (L5), and the
host's swap happens after `frame()` returns — so the destroy is safe in the
same update. Block 19's K6 text and the bounce README are updated with a dated
note: the superseded bundle's release is now bounded on both engines; the
"rides GC" fallback remains true only for scripts that do not call
`destroy()`. The golden is unchanged (destroy prints nothing); the verify run
re-gates on both engines and both backends.

**L11 — documentation.** The main README's script-author rules extend "call
`destroy()`" to name the extension: every retained object here has `destroy()`,
the spec types by the spec, the rest as a recorded extension, and under JSC it
is the only bounded path for all of them. `codegen-deltas.md` records the
deviation class once (extension members, the L7 collision rule, and the list).
The host-borrow ordering note rides the same paragraph that documents borrows.

## 4. Exit criteria

1. All L8 tests green in the standard headless workspace gate; parity
   byte-identical on both engines.
2. L9 CTS re-runs: zero new failures on yawgpu; gated Dawn spot-check on the
   reflection/idl families.
3. `examples/bounce` `--verify` passes on both engines against yawgpu Metal
   and Dawn with the unchanged golden (L10).
4. codegen collision detection demonstrably fires: a temporary IDL-name
   collision (test-only) fails the generator (seen red, then removed).
5. Review pass over the block diff; CRITICAL/MAJOR block completion.

## 5. Review notes (2026-07-17, block review pass: 0 CRITICAL / 0 MAJOR / 2 MINOR)

Two MINOR hardening notes recorded, not fixed — both are unreachable or latent
under current invariants:

- `destroy()` returns silently on a poisoned handle-slot Mutex while
  `handle_or_throw` reports the same state as OperationError. Unreachable while
  principle 8 holds (no code panics holding the slot lock); inconsistent
  reporting only.
- The codegen `[[extension.methods]]` validation does not cross-check that
  core's `extension_destroy` dispatcher covers the interface; an entry outside
  the nine compiles and fails at runtime with a TypeError. The nine are pinned
  by the `EXTENSION_KINDS` tests; revisit if the extension list grows.
