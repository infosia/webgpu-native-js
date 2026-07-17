# Pre-spec — generalizing the skeleton/data split into a scene-graph library

**Status: design discussion, not a block.** Nothing here is normative and no
implementation is implied. This document records the 2026-07-18 discussion so
a future block spec can start from it. Inputs: blocks 15 (frame contract), 19
(skeleton/data split), 20 (extension `destroy()`); the K11 exit entry in
`specs/tracking/engine-boundary.md`; design invariant 7.

## 1. Why the Three.js execution model is unavailable

Three.js's `renderer.render(scene, camera)` walks a retained JS tree every
frame and issues draw commands from JS. Three constraints rule that out here:

- The scope invariant: JS is not the render hot path; per-frame command
  issuance stays in the native host.
- Both shipping engines are JIT-less interpreters; per-frame per-object JS
  work is priced accordingly, and bounce's measured lesson is that GC pressure
  (not arithmetic) is the dominant per-frame cost.
- Invariant 7: command re-recording is priced by O(commands) crossings and by
  requiring explicit `destroy()` discipline on superseded objects (block 20
  bounds the release, but only when the script calls it). Re-record frequency
  must therefore be a visible, controlled quantity, never an implicit one.

Consequence: the API shape of Three.js (retained tree, `add`/`remove`,
materials) can be kept; the execution model must be inverted. In Three.js the
tree is the truth and is translated to draws every frame. Here the typed
arrays and recorded bundles are the truth, and the tree is an authoring view
over them.

## 2. The generalization: two planes, three layers

Block 19's rule pair, lifted from one bundle to a scene:

- **Data plane.** Everything that changes per frame — transforms, colors,
  visibility, instance counts — is buffer contents. Steady-state crossings are
  one `frame()` plus a bounded number of `writeBuffer` calls, independent of
  object count.
- **Structure plane.** The command skeleton (a set of render bundles)
  re-records only on pipeline-composition change, as an explicit, counted,
  host-observable event.

Three layers implement the split:

```
JS scene tree (authoring view)
    ↓ flatten — on structural events only
batch table (SoA typed arrays = scene truth)
    ↓ per frame: writeBuffer / on structural events: bundle re-record
frame plan (ordered (bundle, target) list the host reads)
```

**Layer 1** is the user-facing Three.js-shaped API. Nodes hold no state — only
indices into layer 2. This follows from bounce's GC-pressure lesson applied at
scene scale, and converges on the bitECS layout (SoA typed arrays as truth, JS
objects as views).

**Layer 2** is the core. Objects group into batches keyed by
(pipeline, bind group, geometry); each batch has the exact shape of bounce —
a region of a storage buffer, an indirect-args slot, one `drawIndirect`.
Object add/remove is slot allocation plus an `instanceCount` write (data
plane). Only batch creation/destruction is structural.

**Layer 3** is the host boundary. The host keeps ownership of the frame loop,
surface, pass encoding, submit, and present. The library emits a frame plan —
an ordered list of (bundle, render target) — which the host re-reads only
after a structural event. Multi-pass rendering (shadows, post-processing) adds
plan entries without changing the frame loop's shape.

## 3. Concept mapping

| Three.js | This design | Plane |
|---|---|---|
| `Object3D` tree | authoring node (index into SoA) | — |
| `Mesh` | instance slot in a batch | data |
| `Material` | batch key component + per-instance params | both (§4) |
| `scene.add`/`remove` | slot allocation + indirect-args write | data |
| `material.needsUpdate` | explicit structural event | structure |
| `Camera` | uniform buffer write | data |
| `renderer.render()` | host `frame()` + plan execution | — |
| `dispose()` | extension `destroy()` (block 20) | — |

## 4. Design positions

**Materials stay off the structure plane.** Per-material pipeline permutation
multiplies batch count, and batch count multiplies re-record frequency — the
quantity invariant 7 prices. The opposite bet: an uber-shader with
per-instance material parameters in the storage buffer, so color/texture
changes (via atlases or texture arrays indexed per instance) are data-plane
writes. This is the skeleton/data rule applied to materials.

**Matrix propagation is the one place JIT-less arithmetic may surface.**
Options: (a) JS recomputes dirty subtrees only; (b) upload local TRS + parent
indices and build world matrices in a compute pass, making JS cost
proportional to changed nodes. K11(c) records zero cost observations, so this
is the first thing to measure before any block spec (§6). The extension of
(b) is GPU-driven culling — a compute pass writing indirect args — which
removes JS from visibility entirely.

**The swap contract must be re-judged at N bundles.** K11(b) records three
`Runtime`-surface gaps (eval-for-effect, retention-while-borrowed,
signalling by registered function + globals) that were adequate at one
bundle by convention. Maintaining N retention-global pairs and N generation
counters by convention is the predicted breaking point; that concrete pain —
not anticipation of it — is the justification threshold for extending the
`Runtime` surface (e.g. a handle table tying borrows to retentions, a typed
structural-event channel). Until then the F12/K7 discipline stands.

**Lifetime is `destroy()` discipline.** Superseded bundles are destroyed at
the swap (as bounce does post-block-20); `scene.dispose()` destroys a batch's
resources explicitly. GC remains a backstop only (invariant 7).

## 5. Where it lives

A JS library on top of the binding plus a documented host-integration
contract — not new `core/` code. It ships inside the single game bundle
(block 12 M4). A thin host-side helper crate for reading the frame plan is
possible; the binding itself stays a binding (the K7 discipline one level up).

## 6. Open questions to resolve before a block spec

1. **Matrix propagation cost.** Microbenchmark thousands of nodes under both
   engines: JS dirty-subtree vs compute-pass composition. Measure first;
   nothing beyond block 19's crossing counts is currently known (K11(c)).
2. **Transcendentals vs the golden discipline.** Rotation needs sin/cos;
   `Math.sin` precision is implementation-defined, which is why block 15's F2
   restricts verify arithmetic to f64 `+ - *` and comparison. Options: keep
   verify scenes transcendental-free, or ship a deterministic polynomial
   sin/cos for verify mode.
3. **Batch-key granularity.** How far toward one uber-shader — pipeline count
   vs shader-branch cost. A measurement question.
4. **`Runtime` API extension threshold.** Decided by the N-bundle pain in §4,
   not in advance.

## 7. Prior art: the components exist; the combination was not found

A 2026-07-18 web search (including the curated ecosystem lists
[awesome-webgpu](https://github.com/mikbry/awesome-webgpu) and
[dmnsgn's frameworks collection](https://gist.github.com/dmnsgn/76878ba6903cf15789b712464875cfdc))
found no library combining the four properties in §7.1. Recorded as absence
of evidence, not proof of absence.

Component-wise precedents. Rows marked *(docs)* summarize public architecture
documentation and were not executed or measured here; the Babylon.js and
three.js rows were verified against primary sources during block 19's
authoring:

| System | Matches | Does not match |
|---|---|---|
| [Qt Quick scene graph](https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph.html) *(docs)* | authoring tree (QML/JS) separated from rendering; structural changes cross via a sync phase; render thread is native | skeleton is a C++ scene graph, not recorded commands; native walks the tree every frame; no data/structure split |
| [Chromium compositor](https://www.chromium.org/developers/design-documents/gpu-accelerated-compositing-in-chrome/) *(docs)* | the split itself: display lists (structure, rarely re-recorded) vs compositor properties (data, per frame, no JS) | UI domain; not a script-facing contract |
| [React Native Reanimated](https://docs.swmansion.com/react-native-reanimated/) on Fabric *(docs)* | structure = reconciliation (rare, batched); data = shared values updated per frame without bridge crossings; crossings treated as the budget | UI domain; no GPU command skeleton |
| [Unity BatchRendererGroup](https://docs.unity3d.com/Manual/batch-renderer-group.html) *(docs)* | persistent GPU buffers + per-instance data + batches; batch rebuild only on structural change | single-language world; no script/native boundary |
| [Babylon.js Snapshot Rendering](https://doc.babylonjs.com/setup/support/webGPU/webGPUOptimization/webGPUSnapshotRendering) | render bundles as a recorded, replayed skeleton | entirely JS-hosted; invalidation implicit |
| [three.js BundleGroup](https://github.com/mrdoob/three.js/pull/28719) | explicit `needsUpdate` structural event | JS owns the frame loop |
| [bitECS](https://github.com/NateTheGreatt/bitECS) | SoA typed arrays as truth, JS objects as views | no rendering involvement |
| [dawn.node](https://dawn.googlesource.com/dawn) | same JS↔`webgpu.h` boundary placement | no split — every API call is forwarded (correct for its tooling use case) |

### 7.1 The four properties whose combination was not found

1. The skeleton/data split straddles an in-process JS↔C ABI boundary, where
   crossing count is the budget (compositor and Unity split within one world;
   Qt has no split).
2. Crossing count independent of object count is a public contract.
3. The skeleton unit is a WebGPU render bundle with generation-counted,
   explicit re-record events.
4. The design pressure includes engine lifetime semantics (invariant 7:
   re-record frequency interacts with JSC finalization), not performance
   alone. Browser-hosted libraries control their own GC; native engines have
   none.

Implication: each individual bet is production-proven elsewhere, so design
risk concentrates in the integration — exactly the parts (N-bundle swap
contract, JIT-less matrix propagation) where prior-art benchmarks do not
transfer and §6's measurements are required.
