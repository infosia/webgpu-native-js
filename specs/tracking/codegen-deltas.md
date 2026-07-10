# Codegen deltas — IDL-vs-header divergences, catalogued

Block 05 → G1: where the pinned `webgpu.idl` and the pinned `webgpu.h` disagree,
the header wins, and the divergence is **catalogued here, never approximated**.
Policy skips carry their reasons in `codegen/policy.toml` and surface in the
generator's report; this file is the committed, reviewable index.

## Skipped IDL surface (policy entries with reasons)

| IDL item | Disposition | Reason |
|---|---|---|
| `GPUBindGroupLayoutEntry.externalTexture` | reject-if-present | external textures out of scope |
| `GPUBindGroupLayoutEntry.sampler` / `.texture` / `.storageTexture` (present) | member-named TypeError | bind-group resources are buffer-only for now (block 03 §7); silence was retired 2026-07-10 (slice 2b) |
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

- **Render attachments accept `GPUTextureView` only.** The pinned IDL permits
  `(GPUTexture or GPUTextureView)` for color/depth attachments; the C
  descriptor takes only `WGPUTextureView`. Synthesizing a hidden temporary
  view would create an unowned lifecycle behind the script's back, so a direct
  `GPUTexture` raises a transparent TypeError telling the author to call
  `createView()`. Revisit if implicit-view semantics are ever demanded.
