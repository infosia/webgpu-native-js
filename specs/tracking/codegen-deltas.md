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
| ~~`GPUComputePassDescriptor.timestampWrites` / `GPURenderPassDescriptor.timestampWrites`~~ | **RETIRED 2026-07-12 — both IDL dictionaries emit through the shared `WGPUPassTimestampWrites` C struct** | historical: rejected until `requiredFeatures` plumbing made timestamp-query devices testable |
| ~~`GPURenderPassDescriptor.maxDrawCount`~~ | **RETIRED 2026-07-12 — emitted through `WGPURenderPassMaxDrawCount` only when present** | historical: rejected while optional extension-chain emission was unavailable |
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
  construction site. **Narrowed 2026-07-13 (B-6):** a minimal `DOMException`
  base now exists — built only to what the pins and CTS need, as the `Event`
  base was — because `GPUPipelineError : DOMException` required it. It is a real
  `Error` subclass (the CTS's `shouldReject` demands `ex instanceof Error`).
  `GPUPipelineError` is its one subclass; the other rejection sites still use
  plain named error objects, so the general deviation stands.
- **RETIRED 2026-07-12 — `GPUDevice` now inherits the binding's minimal
  `EventTarget`, and uncaptured errors dispatch a `GPUUncapturedErrorEvent`.**
  The event carries its `[SameObject]` `.error`, supports the pinned Event
  surface (`type`, `cancelable`, `preventDefault()`, `defaultPrevented`), and is
  delivered through the ordered listener list shared by `addEventListener` and
  `onuncapturederror`. Historical deviation: the handler received the bare
  `GPUError` because no event plumbing existed.
- **`WGPUErrorType_Unknown` folds into `GPUInternalError`.** The IDL has no
  fourth error class, so a fold is forced; Internal is the closest semantic.
  Direct test pins it.
- **Non-callable `onuncapturederror` assignments coerce to `null`** (Web
  EventHandler semantics), tested.
- **RETIRED 2026-07-12 — constructor emission is now policy-driven.** The
  anticipated second WebGPU constructible family arrived with
  `GPUUncapturedErrorEvent`; lifecycle policy now emits its `ConstructorSpec`
  (and the illegal `GPUDevice` interface constructor used to establish
  EventTarget prototype inheritance). The original four GPU error-class
  constructor specs remain hand-written historical first-family plumbing.

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

- ~~**Async pipeline rejections use named `OperationError`, not
  `GPUPipelineError`.**~~ **RETIRED 2026-07-13 (B-6).** The exit condition this
  entry named — "revisit when a second DOMException-subclass consumer appears
  **or a CTS family blocks on it**" — was met by four families at once
  (`render_pipeline`, `compute_pipeline`, `shader_module`,
  `non_filterable_texture`), where `THREW OperationError, instead of
  GPUPipelineError` was the *sole* remaining failure. `GPUPipelineError` is now
  implemented from the pins and emitted through the policy-driven constructor
  machinery; async pipeline creation rejects with it, carrying `name` and
  `reason`. All four families are green. *Historical:* the rejection was a named
  `OperationError` whose message carried the validation/internal distinction,
  because the DOMException-subclass machinery was out of B-4b's slice.

## Block 10 additions (2026-07-11)

- **`features` is a real (mutable) JS `Set`** where the IDL's setlike is
  read-only (block 10 → I2). Every read behavior is conformant; a script CAN
  `.add()` to it, which the spec's interface would forbid. Trusted scripts
  (invariant 8); freezing Sets does not exist in JS. Names are sorted before
  insertion for cross-backend determinism.
- **`isFallbackAdapter` derives from `adapterType == CPU`.** The C ABI has no
  direct fallback field; CPU adapters are what the fallback request yields.
  A conformant-enough proxy, recorded as a derivation rather than a fact.
- ~~**`requiredFeatures`/`requiredLimits` are unplumbed in `requestDevice`**
  (`requiredFeatureCount` is hard-coded 0).~~ **RETIRED 2026-07-12:**
  `requestDevice` now converts both fields, passes the feature slice and its
  real length, and parity requests a `timestamp-query` device. The historical
  consequence was that timestamp query sets and `timestampWrites` could not be
  exercised; both timestamp-write policy skips are now retired.
- ~~**Compute-pass `timestampWrites` reason corrected** — it read "out of scope
  until query sets" after query sets shipped; both twins cited the
  requiredFeatures gap.~~ **RETIRED 2026-07-12:** the gap and both skips are
  gone; the two IDL dictionaries map to the shared C timestamp-write struct.

## Block 13 / B-6 additions (2026-07-13)

- **WebIDL interface objects are now installed for every registered class**, not
  only constructible ones, and every class registers eagerly at install
  (`wrap_gpu` / `wrap_device`); the generator emits the inventory. A
  non-constructible interface object throws `TypeError: Illegal constructor` on
  call and construct, and its `prototype` is the interface prototype object.
  Previously the adapters installed a global only for constructible classes and
  registered most classes lazily, so `GPURenderPassEncoder` and friends did not
  exist as globals at all. Recorded here because it is an IDL-conformance
  property the join report cannot see: interface *objects* are not descriptor
  conversion.
- **`prototype.constructor` attribute parity is engine-specific work.** ES says
  `{ writable: true, enumerable: false, configurable: true }`. Boa complies;
  JSC's `JSObjectMakeConstructor` makes it **enumerable**, its public C API has
  no `JSObjectDefineProperty`, and `JSObjectSetProperty` follows assignment
  semantics (so an inherited `constructor` defeats the attribute). The JSC
  adapter detaches the prototype chain, defines with `DontEnum`, and restores.
  Contained in the adapter; `core/` is untouched by it.

## Phase C additions (2026-07-13) — found by the Dawn oracle

- **A C "undefined" sentinel that lies inside its IDL type's range is a binding
  obligation, not a backend one.** `WGPU_DEPTH_SLICE_UNDEFINED` is `0xFFFFFFFF`,
  and `GPURenderPassColorAttachment.depthSlice` is a `GPUIntegerCoordinate`
  (unsigned long) — so `0xFFFFFFFF` is a value a script may legally pass, and at
  the C ABI it is indistinguishable from omission. The CTS tests this on purpose
  (*"The special value '0xFFFFFFFF' is not treated as 'undefined'"*). The binding
  forwards the value correctly; that was never the bug. **No backend can enforce a
  distinction the ABI cannot express**, so the binding decides presence on the JS
  side (`is_undefined`) and raises the validation error itself — all six
  definedness rows plus the mip-level bound check, which required view wrappers to
  retain their effective dimension and per-mip depth.

  Generalize: **wherever a header sentinel falls inside the value range of the IDL
  type it stands for, presence must be decided in the binding.** `depthSlice` is
  unlikely to be the only instance; audit the other `*_UNDEFINED` constants when a
  family blocks on one.

- **WebIDL property attributes are part of conformance.** On the interface
  prototype object, operations are `{writable, enumerable, configurable}` and
  attributes are `{enumerable, configurable}`; only `constructor` is
  non-enumerable. We had shipped the inverse (accessors CONFIGURABLE-only in Boa,
  methods `DontEnum` in JSC), which is invisible to every validation test and
  fatal to any reflection that uses `for...in` — as the CTS's
  `api,operation,reflection` does. Fixed in both adapters and the mock.

- **`label` is on `GPUObjectBase`, i.e. on every object.** The policy subset had it
  on three interfaces. It is a *writable* attribute, must round-trip the
  descriptor's label, must survive `destroy()`, and must carry embedded NULs and
  non-BMP text (`WGPUStringView` carries an explicit length, so no truncation is
  needed or acceptable).

- **`GPUBuffer.mapState`** was absent from the subset; the state machine already
  existed internally. Added.
