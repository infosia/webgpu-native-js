# Codegen deltas — IDL-vs-header divergences, catalogued

Block 05 → G1: where the pinned `webgpu.idl` and the pinned `webgpu.h` disagree,
the header wins, and the divergence is **catalogued here, never approximated**.
Policy skips carry their reasons in `codegen/policy.toml` and surface in the
generator's report; this file is the committed, reviewable index.

## Skipped IDL surface (policy entries with reasons)

| IDL item | Disposition | Reason |
|---|---|---|
| `GPUBindGroupLayoutEntry.externalTexture` | reject-if-present | external textures out of scope |
| ~~`GPUBindGroupLayoutEntry.sampler` / `.texture` / `.storageTexture`~~ | **shipped in block 09 slice 2 (2026-07-11)** — the "buffer-only" rejections converted to positive tests | historical: rejected 2026-07-10 (slice 2b) until the texture surface existed |
| `GPUShaderModuleDescriptor.compilationHints` | reject-if-present | recorded deferral (block 03 §7) |
| `GPUProgrammableStage.constants` | reject-if-present | pipeline constants deferred (block 03 §7); silent drop retired by the Phase 4 review |
| `GPUComputePassDescriptor.timestampWrites` | reject-if-present | query sets out of scope |
| `GPUDevice.importExternalTexture`; `GPUQueue.copyExternalImageToTexture` | not in subset | external-texture surface out of scope; join-report mismatch entries |
| `GPUDevice.lost`, `.onuncapturederror` | ~~not in subset~~ **shipped in Phase 6 (P6b)** | see the Phase 6 additions below |
| `GPUAutoLayoutMode.auto` (as an enum value) | enum_value_skip | the C ABI represents auto layout as a null pipeline-layout handle |

## C-only surface (expected non-findings per G1)

`wgpuBufferGetConstMappedRange` (block 02 → A29), `wgpuBufferRead/WriteMappedRange`,
`wgpuDeviceHasFeature`, `wgpuDeviceGetLostFuture`, `wgpuCommandEncoderWriteTimestamp`;
enum sentinels `Undefined` / `BindingNotUsed` (emitted only for absent optionals);
`WGPUShaderSourceSPIRV`; `WGPUPipelineLayoutDescriptor.immediateSize` (emitted 0).

## Recorded behavioural divergences from strict WebIDL (deferred, with rationale)

- **`enforce_u64` accepts integral values up to 2^64−1** where WebIDL
  `[EnforceRange] unsigned long long` caps at 2^53−1, and rejects fractional
  values where the spec truncates. Inherited block 01 → R8 semantics with named
  tests; the C ABI accepts the full width, and a stricter cap would reject
  sizes the backend supports. Revisit if the upstream CTS is ever run.
  (Found by the Phase 4 review, emission lens item 7.)
- **Dictionary property read order** is required-first, then declaration order —
  not WebIDL's lexicographic order. Observable only via getter side effects;
  trusted scripts (invariant 8). (Emission lens item 8.)
- **`sequence<GPUBindGroupLayout?>` element nullability is dropped**: a null
  element (valid WebGPU: an empty bind group slot) raises a TypeError instead.
  Clear-early-error until null-slot support is actually needed. (Compliance
  lens m3.)
- **Attribute-setter join (`label` → `set_label`) is a convention hard-coded in
  the generator**, not policy — it mirrors `webgpu.yml`'s own naming; a wrongly
  guessed mapping surfaces as a join mismatch. (Compliance m5; accepted.)
- **Interface-level (method) mismatches are report-only** while methods are
  hand-written; they gate nothing. Revisit when method emission lands.
  (Compliance m6.)

## Phase 6 additions (2026-07-10)

- **No DOMException hierarchy (block 07 → S8).** WebGPU rejections are spec'd
  as DOMExceptions (`OperationError`, `AbortError`); this binding rejects with
  plain error objects carrying `name`/`message`. Cited at each rejection
  construction site.
- **`onuncapturederror` receives the bare `GPUError`, not a
  `GPUUncapturedErrorEvent`.** The IDL defines an `Event` subclass with an
  `.error` attribute and full EventTarget semantics; this binding has no
  EventTarget and calls the handler with the error object directly. Revisit if
  event plumbing ever lands.
- **`WGPUErrorType_Unknown` folds into `GPUInternalError`.** The IDL has no
  fourth error class, so a fold is forced; Internal is the closest semantic.
  Direct test pins it.
- **Non-callable `onuncapturederror` assignments coerce to `null`** (Web
  EventHandler semantics), tested.
- **Constructor emission is not in the generator** (block 07 → S3): the
  `ConstructorSpec` slot is hand-wired for the four error classes; the
  generator learns constructors when a second constructible family appears.

## Block 09 addition (2026-07-11)

- **RETIRED 2026-07-11 (B-4b): render attachments now accept
  `(GPUTexture or GPUTextureView)` per the pinned IDL.** The original entry
  (below, kept for the record) declined a hidden temporary view as an unowned
  lifecycle. B-4b built exactly that machinery for `GPUBindingResource`'s
  direct-`GPUTexture` arm — a conversion-created implicit default view
  (`wgpuTextureCreateView(texture, NULL)`, null descriptor header-blessed)
  owned by the converted wrapper and released through the release queue,
  failure paths symmetric — and the same acceptance now covers render-pass
  color/resolve/depth attachments. The parity line that pinned the TypeError
  is now a positive `renderPass:texture-as-view:ok` line on both engines.
  - *Original entry:* Render attachments accept `GPUTextureView` only; the C
    descriptor takes only `WGPUTextureView`; synthesizing a hidden temporary
    view would create an unowned lifecycle behind the script's back, so a
    direct `GPUTexture` raised the converter's TypeError. *(Corrected
    2026-07-11 by the block 09 review: this entry originally claimed the
    message "tells the author to call createView()" — an embellishment the
    planner wrote beyond the agent's report.)*

## Block 13 / B-4b additions (2026-07-11)

- **Async pipeline rejections use named `OperationError`, not
  `GPUPipelineError`.** The pinned IDL defines `GPUPipelineError` (a
  `DOMException` subclass with a `reason` attribute: `"validation"` |
  `"internal"`) as the rejection type for
  `createComputePipelineAsync`/`createRenderPipelineAsync`. Implementing the
  DOMException-subclass machinery was out of B-4b's slice; the rejection is a
  named `OperationError` whose message carries the `validation`/`internal`
  distinction. Deviation is visible to CTS cases that assert the class;
  revisit when a second DOMException-subclass consumer appears or a CTS
  family blocks on it.

## Block 10 additions (2026-07-11)

- **`features` is a real (mutable) JS `Set`** where the IDL's setlike is
  read-only (block 10 → I2). Every read behavior is conformant; a script CAN
  `.add()` to it, which the spec's interface would forbid. Trusted scripts
  (invariant 8); freezing Sets does not exist in JS. Names are sorted before
  insertion for cross-backend determinism.
- **`isFallbackAdapter` derives from `adapterType == CPU`.** The C ABI has no
  direct fallback field; CPU adapters are what the fallback request yields.
  A conformant-enough proxy, recorded as a derivation rather than a fact.
- **`requiredFeatures`/`requiredLimits` are unplumbed in `requestDevice`**
  (`requiredFeatureCount` is hard-coded 0). Consequences, all recorded at
  their sites: `timestamp` query sets cannot be created (untested),
  `timestampWrites` stays policy-skipped for this reason (both policy twins
  now carry the same reason). The plumbing is a known, deliberate gap.
- **Compute-pass `timestampWrites` reason corrected** — it read "out of scope
  until query sets" after query sets shipped; both twins now cite the
  requiredFeatures gap.
