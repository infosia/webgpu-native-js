# Block 17 — immediate data (`GPUBindingCommandsMixin.setImmediates`)

**Status: COMPLETE (2026-07-15).** Closed the last unimplemented WebIDL operation
in the command-encoding subset. Results in `specs/tracking/cts.md` → block 17.

Outcome: three C entry points, two Rust bodies, zero new `JsEngine` trait methods,
zero changes to the engine adapters. The dispatch symbols were derived by the join
from the three `[[subset]]` member additions; no `extra_symbols` were needed.

## 1. What is missing, and what is not

`setImmediates` is the only part of the immediate-data surface the binding does
not implement. The other two parts already ship:

| Surface | State | Evidence |
|---|---|---|
| `GPUSupportedLimits.maxImmediateSize` | **shipped** | `core/src/lib.rs` limit getter `limit_max_immediate_size`; reported through `WGPULimits.maxImmediateSize` |
| `GPUPipelineLayoutDescriptor.immediateSize` | **shipped** | generated `convert_pipeline_layout_descriptor` reads the dictionary member and emits it with `enforce_u32` (default 0) |
| `GPUBindingCommandsMixin.setImmediates` | **absent** | not listed in any `[[subset]]` `members` array in `codegen/policy.toml` |

**Correction to a prior record.** `specs/tracking/codegen-deltas.md` lists
`WGPUPipelineLayoutDescriptor.immediateSize` under "C-only surface (expected
non-findings)" with the note "(emitted 0)". That is wrong: `immediateSize` is a
WebIDL dictionary member (`webgpu.idl` line 614, `GPUSize32 immediateSize = 0`)
and the generated converter reads it. The entry is stale and is removed by this
block.

## 2. The C ABI and both backends

The pinned `webgpu.h` declares three entry points, one per interface that
includes the mixin:

```c
void wgpuComputePassEncoderSetImmediates (WGPUComputePassEncoder,  uint32_t offset, void const* data, size_t size);
void wgpuRenderPassEncoderSetImmediates  (WGPURenderPassEncoder,   uint32_t offset, void const* data, size_t size);
void wgpuRenderBundleEncoderSetImmediates(WGPURenderBundleEncoder, uint32_t offset, void const* data, size_t size);
```

Both backends export all three (`nm`, in-session): yawgpu and Dawn. Dawn
additionally exports `dawn::native::ProgrammableEncoder::ValidateSetImmediates`,
so the device-timeline validation this block does **not** implement (§4) has an
oracle.

## 3. The WebIDL contract

```webidl
interface mixin GPUBindingCommandsMixin {
    undefined setImmediates(GPUSize32 rangeOffset, AllowSharedBufferSource data,
        optional GPUSize64 dataOffset = 0, optional GPUSize64 dataSize);
};
```

Included by `GPUComputePassEncoder`, `GPURenderPassEncoder`, and
`GPURenderBundleEncoder`.

`dataOffset` and `dataSize` are **in elements if `data` is a TypedArray, in
bytes otherwise** (`ArrayBuffer`, `DataView`) — the same rule as
`GPUQueue.writeBuffer`, and the same rule `ConvertedBufferSource.bytes_per_element`
already encodes.

## 4. Content timeline vs device timeline — the split that defines this block's scope

The spec algorithm (`third_party/gpuweb/index.html`, algorithm
`GPUBindingCommandsMixin.setImmediates`) has two halves. **The binding implements
the first and forwards the second.**

**Content timeline — the binding's job. Throws `OperationError`.**

1. element type is byte for `ArrayBuffer`/`DataView`, else the TypedArray's.
2. `contentsSize = dataSize ?? (dataElementCount − dataOffset)`.
3. `OperationError` unless all of:
   - `contentsSize ≥ 0` (unsigned: the observable failure is `dataOffset > dataElementCount`),
   - `dataOffset + contentsSize ≤ dataElementCount`,
   - `contentsSize` converted to bytes is a multiple of 4.
4. copy the `contentsSize` elements starting at `dataOffset` elements.

**Device timeline — the backend's job. Raises a validation error and
invalidates the encoder.**

- `rangeOffset` is a multiple of 4.
- `rangeOffset + contentsBytes ≤ device.limits.maxImmediateSize`.

**Rule: the binding never pre-empts a device-timeline check.** Neither the
`rangeOffset` alignment nor the `maxImmediateSize` bound is checked in `core/`;
they are the backend's validation and reach script through the error sink. A
binding-side check would convert a validation error into a JS exception and
break `error_scope` semantics. If yawgpu does not raise them, that is a backend
gap, catalogued in `backend-deltas.md` — never worked around here.

## 5. Rules

- **I1.** `setImmediates` is emitted from the join, not hand-registered: three
  `[[subset]]` member additions plus three `[[lifecycle.methods]]` entries in
  `codegen/policy.toml`. No hand-edited generated code.
- **I2.** The render pass and the render bundle encoder share **one** core body,
  dispatched through the existing `RenderCommands` enum — the same structure
  `setBindGroup` already uses. `GPUComputePassEncoder` gets its own body. Three
  C entry points, two Rust bodies.
- **I3.** Argument conversion reuses `convert_buffer_source` and its
  `bytes_per_element`. No second implementation of the elements-vs-bytes rule.
- **I4.** Range violations raise `OperationError`, not `TypeError`. Type
  violations (`data` is not a buffer source) raise `TypeError`. This matches
  `queue_write_buffer`.
- **I5.** A call on an ended encoder is a no-op that returns `undefined`, via the
  existing `live_compute_pass` / `live_render_commands` guards.
- **I6.** `contentsBytes == 0` is legal (the CTS exercises `elementCount: 0`).
  The call still reaches the C entry point, with size 0.
- **I7.** No new `JsEngine` trait method. The trait stays at 49 items (the F12/J13
  gate).

## 6. Acceptance

**Unit (mock, `core/src/mock.rs`) — every one of these must exist:**

1. Happy path per encoder kind (compute pass, render pass, render bundle): the
   mock records the `offset`, the bytes, and the size actually delivered to the C
   entry point.
2. `Uint32Array` with `dataOffset`/`dataSize` in **elements** — the recorded byte
   range is `dataOffset * 4 .. + dataSize * 4`.
3. `DataView` and `ArrayBuffer` with `dataOffset`/`dataSize` in **bytes**.
4. A TypedArray **view window** (non-zero `byteOffset` into its buffer) is
   honoured — the recorded bytes come from the view, not the whole buffer.
5. `dataSize` omitted ⇒ `dataElementCount − dataOffset`.
6. `contentsBytes % 4 != 0` ⇒ `OperationError`.
7. `dataOffset > dataElementCount` ⇒ `OperationError`.
8. `dataOffset + contentsSize > dataElementCount` ⇒ `OperationError`.
9. `data` not a buffer source ⇒ `TypeError`.
10. `elementCount == 0` ⇒ no error; the C entry point is called with size 0.
11. Called after `end()` ⇒ returns `undefined`, no C call.
12. `rangeOffset` unaligned and `rangeOffset` huge ⇒ **no binding error**; the
    value is forwarded verbatim to the C entry point (pins §4).

**Gates:** workspace test (yawgpu features), core with the backend env var
UNSET, both clippys `-D warnings`, `cargo fmt --all -- --check`, parity suite
byte-identical across Boa and JSC.

**Parity:** `tests/parity/parity.js` gains a `setImmediates` case whose output is
byte-identical under both engines.

**CTS (yawgpu, headless):** the four families that `supportsImmediateData` gates
must be run and their results recorded in `specs/tracking/cts.md`:

- `webgpu:api,validation,encoding,cmds,setImmediates:*`
- `webgpu:api,validation,encoding,programmable,pipeline_immediate:*`
- `webgpu:api,operation,command_buffer,programmable,immediate:*`
- `webgpu:api,validation,encoding,encoder_open_state:*` (already green; must not regress)

Any residual failure is triaged into exactly one of: binding bug (fix),
backend gap (catalogue in `backend-deltas.md`), engine gap (catalogue). An
expectation entry whose reason does not name one of the three does not land.

**Dawn oracle run (gated, real GPU):** the same four families under Dawn. A
family that passes on Dawn and fails on yawgpu is a yawgpu backend gap. A family
that fails on Dawn is a presumed binding bug.

## 7. Note on `supportsImmediateData`

The CTS enables these families when **any** of five conditions holds
(`webgpu-cts` `src/common/util/util.ts`):

```js
'setImmediates' in GPURenderPassEncoder.prototype ||
'setImmediates' in GPUComputePassEncoder.prototype ||
'setImmediates' in GPURenderBundleEncoder.prototype ||
'maxImmediateSize' in GPUSupportedLimits.prototype ||
gpu.wgslLanguageFeatures.has('immediate_address_space')
```

The fourth already holds, which is why these families run today and fail. This
block does not change *whether* they run — only whether they pass.

`GPU.wgslLanguageFeatures` is a separate, unimplemented attribute and is **out of
scope for this block**; it is recorded as an open item, not silently skipped.
