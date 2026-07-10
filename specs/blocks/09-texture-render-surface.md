# Block 09 — the texture and render surface

Owner directive (2026-07-10): expand the API surface. Rules **T1–T10**. All
prior blocks bind; the machinery is mature — descriptors, enums, lifecycle,
and class tables generate (block 05), so this block is mostly **policy entries
plus the genuinely new conversion kinds**, hand-written only where bodies are
non-standard.

Verified while writing: `GPUExtent3D`/`GPUOrigin3D` are
`(sequence<GPUIntegerCoordinate> or GPU*3DDict)` typedefs (webgpu.idl:1375,
1382; the dict has `required width`, defaulted height/depthOrArrayLayers);
`wgpuDeviceCreateTexture` / `wgpuTextureCreateView` / `wgpuQueueWriteTexture`
exist in the pinned header; `WGPUTextureFormat_*` has 118 C values against the
IDL's enum (the generated join + sentinel policy already handles the shape).
**yawgpu's Noop backend executes buffer↔buffer copies eagerly but texture
copies are a no-op** (`HalCopy::Buffer` handled; texture arms fall through —
verified at source in yawgpu-hal's noop module). Cite upstream, not paths.

## 1. What is headless-testable (the B1/B2 split, extended)

- **Creation and validation**: everything — textures, views, samplers in bind
  groups, render pipelines, render passes. Tested like B2 (no validation
  error; wrappers/lifecycle/release balance).
- **Texel bytes**: NOT observable headless (Noop texture copies are no-ops).
  No test may assert texture contents; a test whose green implies a texel
  round-trip happened is a lie (the B2 discipline). Byte-level texture tests
  are gated real-GPU work, out of scope.
- **Parity lines**: creation, readonly attributes, enum rejections, union
  conversions, validation-error classes via error scopes — never texel bytes.

## 2. Rules

**T1 — `GPUExtent3D`/`GPUOrigin3D` become a generator kind: dict-or-sequence
union.** Per WebIDL: a sequence (iterator protocol, existing machinery) of 1–3
`[EnforceRange] GPUIntegerCoordinate`s, or the dict with `required width` and
defaulted members. Wrong length / non-iterable-non-dict → TypeError. One kind,
policy-selected, emitted for both types; direct tests per arm and per failure.

**T2 — `GPUTexture` follows the generated lifecycle** (G14): createTexture
descriptor fully generated (size union, mipLevelCount/sampleCount defaults,
dimension + format enums via the join, usage flags 32→64, viewFormats
`sequence<enum>`); `destroy()` mirrors GPUBuffer's (R14 vocabulary: destroy ≠
release). **Readonly attributes** (width, height, depthOrArrayLayers,
mipLevelCount, sampleCount, dimension, format, usage) — verify against the
header which `wgpuTextureGet*` getters exist and read through them (the
wrapper stores no copies it could get wrong); dimension/format map C→IDL
string once, in generated code.

**T3 — `GPUTextureView` is created from a texture, retains it (B8), and is
otherwise inert.** createView's descriptor is all-optional (format/dimension
inherit per spec — the C side encodes "undefined" sentinels; pass them and let
the backend infer, do not re-implement inference). View wrappers follow the
generated lifecycle with the texture handle retained.

**T4 — the bind-group resource arms complete.** `GPUBindGroupLayoutEntry`'s
`sampler`/`texture`/`storageTexture` members become real (generated nested
dicts + enums); `GPUBindGroupEntry.resource` accepts `GPUSampler` and
`GPUTextureView` wrappers (the union-flatten policy grows two arms; retention
derivation picks up the handles per G14). The slice-2b "not supported yet"
TypeErrors for these kinds are REMOVED with their tests converted to
positive tests — `externalTexture` alone keeps its rejection.

**T5 — `createRenderPipeline` is the monster, and it is still just policy.**
Vertex state (`buffers: sequence<GPUVertexBufferLayout?>` — NULLABLE elements
this time, unlike bind group layouts: honor it, `null` → a hole/stride-0
buffer slot per the C encoding — verify against webgpu.yml), attributes
(format enum ~30 values, offset u64, shaderLocation u32), primitive state
(topology/strip-index/front-face/cull enums), depth-stencil (optional chained?
— verify: it is a plain optional dict in C, `WGPU_NULLABLE` pointer), stencil
faces, multisample, fragment (optional; targets `sequence<GPUColorTargetState?>`
nullable elements; blend components with enum defaults), and the same
`GPUProgrammableStage`⋈`WGPUVertexState`/`WGPUFragmentState` name-maps as
compute. Creation-tested headless; `getBindGroupLayout(index)` deferred with a
recorded reason unless trivially generatable.

**T6 — render passes are validation-level headless.** `beginRenderPass`
(color attachments `sequence<GPURenderPassColorAttachment?>`, loadOp/storeOp
enums, clearValue `GPUColor` — a dict-or-sequence union of 4 doubles, T1's
kind with f64 elements; depth-stencil attachment), then the encoder methods:
`setPipeline`, `setVertexBuffer` (nullable buffer!), `setIndexBuffer`,
`draw`, `drawIndexed`, `setViewport`, `setScissorRect`, `setBindGroup`,
`end`. State machine mirrors compute (B10: use-after-end is an error);
`draw` records but Noop does not execute — tests assert no validation error
and correct encoder state transitions, never pixels.

**T7 — copies and `writeTexture` land creation/validation-only.**
`copyBufferToTexture` / `copyTextureToBuffer` / `copyTextureToTexture`
(GPUTexelCopyBufferInfo / GPUTexelCopyTextureInfo dicts — verify the pinned
IDL names, they were renamed upstream at some revision), `queue.writeTexture`
(BufferSource data via the B22 machinery, layout dict, extent union). Bytes
NOT asserted (§1); the parity lines pin argument validation and error classes.

**T8 — every new enum goes through the generated join**, C sentinels policied,
IDL-only values gating generation (the Phase 4 rule). The texture-format enum
is the stress test: 118 C values vs the IDL list — the report's mismatch
output for it is planner input, attach it to the slice report.

**T9 — parity grows with every slice** (block 08 P8): creation + attribute
lines, enum rejection lines, union conversion lines (sequence and dict arms,
wrong-length errors), nested-scope validation classes for texture/render
mistakes (backend-deterministic per the block 07 precedent — verify each
against yawgpu Noop before pinning).

**T10 — the boundary rules hold unchanged.** Zero engine references in
emitted code; adapters gain nothing per-interface (G10/G16); any JsEngine
addition is additive and reported; the JSC exit-gate discipline applies to
every slice.

## 3. Slices

1. **Textures**: T1 (unions) + T2 + T3 + T8 for its enums + T9 lines.
2. **Bind-group completion**: T4 + T9 lines.
3. **Render pipeline**: T5 + T8 (vertex/blend/format enums) + T9 lines.
4. **Render pass + copies**: T6 + T7 + T9 lines.

Each slice: full gate set + parity byte-identical; per-slice review by the
planner; a phase review after slice 4.

## 4. Exit criteria

1. Everything in §3 creation/validation-tested headless under both engines.
2. The Noop texel limitation recorded in every test that would otherwise
   overclaim (the B2 discipline), and in the README's API list.
3. Parity ≥90 lines, byte-identical, no divergence unresolved.
4. Zero core-logic changes for engine reasons; adapter diff ≤ 0 lines.
5. Phase review clean of CRITICAL/MAJOR.
