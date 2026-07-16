# Block 19 — the skeleton/data split: `examples/bounce` as its smallest PoC

Owner directive (2026-07-16): evolve `examples/bounce` into the smallest
proof-of-concept of the skeleton/data rendering split. Rules **K1–K12**.
Blocks 11 (X1–X12) and 15 (F1–F18) still bind; this block is example-only.

## 1. What this block exists to prove

The frame contract (block 15) fixed the crossing count per frame but left a
question open: what happens when the *scene* changes — objects appearing,
disappearing, changing appearance? A naive answer re-records the render bundle,
which under JavaScriptCore leaks every superseded bundle until context teardown
(invariant 7: finalizers effectively never run; `GPURenderBundle` has no
`destroy()`). The design answer this block makes executable:

- **Rule: dynamics expressible as buffer contents never re-record commands.**
  Visibility and instance-count changes are `writeBuffer` calls into an
  indirect-draw argument buffer.
- **Rule: commands re-record only on pipeline-composition change, as an
  explicit, host-observable event.** Never implicitly, never per frame.

Prior art, cited for the shape of the API and the performance thesis, not as
dependencies: Babylon.js Snapshot Rendering (record one frame, replay via
render bundles; <https://doc.babylonjs.com/setup/support/webGPU/webGPUOptimization/webGPUSnapshotRendering>)
and three.js `BundleGroup` (explicit static marking + `needsUpdate`;
<https://threejs.org/docs/pages/BundleGroup.html>,
<https://github.com/mrdoob/three.js/pull/28719>). The explicit three.js shape
is the one adopted: implicit snapshot invalidation hides re-record frequency,
and re-record frequency is exactly what invariant 7 prices.

The prerequisites are already on `main`, verified while authoring this spec:
`GPURenderBundleEncoder.drawIndirect`/`drawIndexedIndirect` are in the codegen
subset (`codegen/policy.toml`, GPURenderBundleEncoder members), implemented as
the shared `GPURenderCommandsMixin` body in `core/`, and exercised at
validation level in the parity suite on both engines against yawgpu and Dawn
(`tests/parity/parity.js`, `indirect:bundle.drawIndirect`).
`GPUBufferUsage.INDIRECT` is exposed. No codegen work is required.

## 2. Scope

**In.** `examples/bounce` only: the indirect-draw skeleton, the deterministic
despawn/respawn schedule, one explicit re-record + host swap event, the
extended `--verify` mode, the README rewrite, and two small hardening items
recorded during the block-15 example's review (K9). A tracking entry at exit
(K11).

**Out.** Any new `Runtime` API; any change to `core/`, `codegen/`, `ffi/`, or
either adapter (K7). A general scene-graph or multi-bundle library — this PoC
exists to inform that block, not to be it. A JSC-linked example (examples link
`boa_adapter`; recorded in block 15 §5). Host `dt` clamping (timestep policy is
the host's business, block 15 §2; the stuck-outside-walls behaviour under a
multi-second wall-clock `dt` remains a recorded cosmetic limitation of the
non-verify mode).

## 3. Rules

**K1 — the PoC's claim.** Scene dynamics (which bodies are drawn, how many)
flow through buffer writes; the command skeleton is recorded at init and
re-recorded exactly once, at a fixed frame, as an explicit event the host
observes and the golden asserts. No JS draw-call issuance per frame; block 15's
crossing invariant holds every frame.

**K2 — the skeleton records one `drawIndirect`.** The bundle contains
`setPipeline`, `setBindGroup(0, …)`, `drawIndirect(indirectBuffer, 0)`. The
indirect buffer is 16 bytes, usage `INDIRECT | COPY_DST`, holding
`[vertexCount=6, instanceCount, firstVertex=0, firstInstance=0]` as four u32.
`instanceCount` is the only word that ever changes. `firstInstance` stays 0:
core WebGPU requires the `indirect-first-instance` feature for nonzero values,
and `requiredFeatures` is unplumbed in `requestDevice` (recorded gap, plan-A
entry in `specs/tracking/engine-boundary.md`).

**K3 — the crossing budget, amortized.** Steady-state frames cost one
`Runtime::frame` call plus two `queue.writeBuffer` calls (body storage,
indirect args) — constant in body count and in visibility changes. The one
frame containing the re-record event additionally spends O(bundle commands)
crossings; the golden proves that happens exactly once in the run. State this
budget in the README (K12).

**K4 — the schedule is deterministic and the simulation never stops.** All
`N = 8` bodies integrate every frame regardless of visibility (block 15 F2
arithmetic discipline continues to bind: `+`, `-`, `*`, comparison on f64
only). Drawn instances are the first `alive` bodies; despawn removes from the
highest index, so no compaction exists. Normative schedule, chosen so the
golden pins every leg:

| Frames | `alive` | Event |
|---|---|---|
| 1–30 | 8 | steady state, generation 1 |
| 31–60 | `8 - floor((frame-30)/6)`, reaching 3 | despawn via indirect-args writes only |
| 45 | — | the re-record event (K5/K6), generation 2 |
| 61–90 | `3 + floor((frame-60)/6)`, capped at 8 | respawn via indirect-args writes only |

**K5 — resources at init, commands at re-record.** Both render pipelines — the
base pipeline and a visibly distinct variant (e.g. a second fragment entry
point dimming the color) — plus layouts, bind group, and all buffers are
created at init. The frame-45 event creates exactly one
`GPURenderBundleEncoder`, re-records the K2 command sequence against the
variant pipeline, and calls `finish()`. If implementing the event turns out to
require creating any resource, the design claim has failed — stop and report
rather than absorbing it.

**K6 — the swap contract, from existing primitives only.** In frame 45's
`update()`: record the new bundle, assign `globalThis.bounceBundle`, increment
`globalThis.bundleGeneration`, call the registered host function
`signalBundleSwap()`. The host, after that `frame()` returns and before
encoding the pass: re-borrows via `native_render_bundle`, replaces the
retention global (`__hostBorrowedBounceBundle`), and swaps its stored handle;
the superseded handle is never touched again. The superseded wrapper's release
rides Boa GC and the release queue (frame step 6). Under JSC it would live to
context teardown — one more reason re-record is rare by design; record this in
the README's caveat (K12), and note the example is Boa-linked.

**K7 — example-only, or stop.** Zero diffs outside `examples/bounce`, the
READMEs it cross-references, and `specs/tracking/`. If the example cannot be
written against the existing `Runtime` surface, stop and report (the block-15
F12 discipline applied one level up). The probe-global and retention-global
gymnastics this forces are expected — they are the measurement, not a defect
to engineer around (K11 records them).

**K8 — `--verify` extensions.** Fixed `dt = 1.0/60.0`; `VERIFY_FRAMES = 90`,
defined once in the host and injected as a JS global (the block-15 example
duplicated the constant in both languages; this block removes the hazard).
`update()` prints checkpoints at frames 30, 45, 60, and 90: `alive` and
`bundleGeneration`; at frame 90 it additionally prints every body's `x,y`,
host-formatted (`{x:.6}`, the existing convention). The host asserts: all 90
frames presented, exactly one swap signal, final generation 2, and the
swapped-in native handle differs from the original (assert inequality; never
print pointer values). The corrupt-one-golden-line check is re-run once to
prove the new golden still gates.

**K9 — hardening items from the block-15 example's review.** (a) The
init-timeout failure message includes the captured `print` lines — in verify
mode `bounce.js`'s `.catch` diagnostics currently go to `captured_output` and
are discarded, leaving only "did not become ready". (b) The
`Promise.resolve().then(...)` init wrapper gains a one-line comment stating its
intent (route init through the microtask pump; collect failures in `.catch`).
Both are inside this block's files; neither expands scope.

**K10 — gated real-GPU verification, per the oracle protocol.** A gated Dawn
run and a gated yawgpu run (Metal on macOS, Vulkan on Windows) of both modes.
Expected visual behaviour: bodies vanish and reappear on the K4 schedule; the
color variant takes effect at frame 45. If a real yawgpu backend rejects or
ignores indirect draws, that is a backend finding — catalogue it in
`specs/tracking/backend-deltas.md` and hand off upstream, never work around it
in the example or the binding; Dawn arbitrates the example's correctness in
the meantime. Never CI (principle 7: the workspace gate stays headless).

**K11 — the exit recording feeds the scene-graph block.** A
`specs/tracking/engine-boundary.md` entry stating: (a) what the swap contract
actually required end to end, (b) every `Runtime`-surface ergonomics gap hit
(value drop, eval-for-effect, retention-while-borrowed — the block-15 example
already surfaced these three), (c) any cost observations, as measurements or
not at all. This entry is the block's real deliverable to the future
scene-graph design; the example is its proof.

**K12 — documentation.** The README states, in one paragraph each: the
steady-state crossing budget (K3); the skeleton/data rule pair from §1; the
swap event and its JSC caveat (K6). Cross-references updated: triangle (zero
per-frame JS) → bounce (per-frame data, and now: explicit structural events).
Plan §2.7's frame-contract text gains one sentence naming the amortized
budget; no other plan edits.

## 4. Exit criteria

1. `cargo run -p example-bounce -- --verify` matches the new golden on gated
   Dawn and yawgpu real-backend runs (or the yawgpu indirect delta is
   catalogued per K10 and the Dawn run passes); the windowed run shows the K4
   schedule and the frame-45 color change. Planner- or owner-verified, never CI.
2. The K8 assertions all hold; the corrupted-golden check exits 1.
3. `git diff` is confined to `examples/bounce/`, cross-referenced READMEs,
   plan §2.7's one sentence, and `specs/tracking/` (K7).
4. The K11 tracking entry exists.
5. A review pass over the block's diff before closing; any CRITICAL/MAJOR
   finding blocks completion (standing workflow rule).
