# Block 10 — introspection and pipeline-derived handles

Owner-approved (2026-07-11, "proceed with A"). Rules **I1–I8**. Verified while
writing: `GPUSupportedFeatures` is `readonly setlike<DOMString>` (webgpu.idl:63);
`GPUSupportedLimits` is an interface of readonly numeric attributes;
`wgpuDeviceGetFeatures` fills a caller-owned `WGPUSupportedFeatures`
(ReturnedWithOwnership; freed via `wgpuSupportedFeaturesFreeMembers`);
`wgpuDeviceGetLimits`/`wgpuAdapterGetInfo` return `WGPUStatus` into out-structs
(info strings freed via `wgpuAdapterInfoFreeMembers`);
`wgpu{Compute,Render}PipelineGetBindGroupLayout` is **ReturnedWithOwnership**
(the device.queue lesson: no extra AddRef).

## Rules

**I1 — `JsEngine` gains `construct` (additive).** `fn construct(cx, ctor,
args) -> Result<Value, Error>` — QuickJS `JS_CallConstructor`, JSC
`JSObjectCallAsConstructor`, mock analog. Needed to build real JS built-ins
from core; first consumer is I2.

**I2 — `features` is a real JS `Set` of feature-name strings,
[SameObject]-cached.** Core reads `wgpuDeviceGetFeatures` /
`wgpuAdapterGetFeatures` ONCE per wrapper, maps each `WGPUFeatureName` through
the generated enum join (IDL-listed names only; C-only values skipped with a
report line), builds `new Set([...])` via I1 + the existing global/call
primitives, caches it like B21's queue, frees the native list via
`FreeMembers` immediately after copying. **Recorded deviation:** a real `Set`
is mutable where the IDL's setlike is read-only — trusted scripts (invariant
8), recorded in codegen-deltas, revisit never (freezing Sets does not exist).
All read behavior (`has`, `size`, iteration, `forEach`) is conformant for
free.

**I3 — `limits` is a generated value-backed wrapper.** New lifecycle pattern:
a wrapper whose payload is a **copied C struct**, not a native handle — no
release queue involvement, no AddRef, finalizer just drops the copy.
`GPUSupportedLimits`' readonly attributes generate from the IDL⋈`WGPULimits`
join (numeric widths per G11; `u64` limits surface as JS numbers — values
beyond 2^53 are not expected from real limits, but convert through the
existing checked paths and error loudly rather than rounding).
`wgpuDeviceGetLimits` status != Success → OperationError. [SameObject]-cached.

**I4 — `adapterInfo` follows I3's pattern with strings.** Copy every
`WGPUStringView` into owned Strings inside the fetch, `wgpuAdapterInfoFreeMembers`
immediately, cache the wrapper. Attributes per the pinned IDL
(`vendor`/`architecture`/`device`/`description` + whatever the pin lists —
read it, do not assume).

**I5 — `GPUAdapter` gains `features`/`limits`/`info`; `GPUDevice` gains
`features`/`limits`/`adapterInfo`** — exactly the attribute names the pinned
IDL gives each interface (verify; do not mirror blindly), all
[SameObject]-cached, all through I2–I4's machinery.

**I6 — `getBindGroupLayout(index)` wraps a pipeline-derived handle.** The
recorded design item: a **non-creator lifecycle** — the wrapper adopts the
ReturnedWithOwnership handle (no extra AddRef), releases through the queue
like any wrapper, retains its PIPELINE (B8: the layout's validity may depend
on it — verify against the header's lifetime notes; if the header guarantees
independence, record and skip the retention). Per WebIDL default this returns
a NEW wrapper each call (verify the pin for [SameObject]-ness; encode what it
says). Out-of-range index: the C call's error behavior decides (null →
OperationError per R13/B16).

**I7 — tests.** Mock: feature list mapping (incl. a C-only value skipped),
FreeMembers called exactly once (counter), limits struct copied and every
generated attribute read back, status-failure path, adapterInfo strings copied
+ freed, [SameObject] identity for all cached values, getBindGroupLayout
release balance + pipeline retention (per I6's verified answer). Script (both
engines): `device.features.has(...)`, `[...device.features]` iteration,
`device.limits.maxBindGroups` (or a limit the pin names) numeric, identity
lines. Parity: features-iteration line — *(scoped honestly, 2026-07-11, after a
deletion experiment proved the line order-blind: a default-requested device
carries ONE feature on yawgpu, and a one-element join cannot observe order.
Core's sort-before-insert is pinned by the mock's deliberately-unsorted
two-name list; parity will observe ordering only when `requiredFeatures`
plumbing lands and devices can carry several features — the recorded gap.)*, one limits line with a backend-stable limit (verify yawgpu/Dawn
agree on at least one value — if none agree, log only `typeof`), identity
lines, a getBindGroupLayout create/release line.

**I8 — the standing boundaries hold.** I1 is the block's only trait addition;
adapters otherwise unchanged (G10/G16); emitted code engine-free; every
FreeMembers pairing is a MAJOR finding if unbalanced.

## Exit criteria

1. All five surfaces work headless under both engines; parity extended,
   byte-identical (backend-stable lines only).
2. FreeMembers pairings counter-asserted; no new unsafe without SAFETY.
3. Deviations recorded (I2 mutability; anything I5/I6 verification turns up).
4. Review clean of CRITICAL/MAJOR.
