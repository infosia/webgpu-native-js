# Block 07 — error scopes and the error model

Phase 6 (owner-approved order item 2, after lifecycle emission and B22).
Rules **S1–S10**. All prior blocks bind; invariant 2 (callback modes) and J1/J2
(pure-Rust callbacks, one-frame settlement) are the load-bearing ones here.

Every `webgpu.h` claim below was verified against the pinned header while
writing (2026-07-10): `WGPUErrorFilter` {Validation, OutOfMemory, Internal};
`WGPUErrorType` {NoError, Validation, OutOfMemory, Internal, Unknown};
`WGPUPopErrorScopeCallbackInfo` **has a `mode` field** ("Controls when the
callback may be called", no default); the callback signature is
`(status, type, message, ud1, ud2)` and on non-Success `type` is `NoError`;
`wgpuDeviceGetLostFuture` exists; `WGPUUncapturedErrorCallbackInfo` remains
creation-time-only and mode-less (invariant 2's lone exception).

## 1. Scope

**In (slice P6a):** `GPUError` / `GPUValidationError` / `GPUOutOfMemoryError` /
`GPUInternalError` classes; `GPUDevice.pushErrorScope` / `popErrorScope`;
A9's retirement (rejection reasons become error objects). **In (slice P6b,
after P6a review):** `onuncapturederror` (host-forwarded for adopted devices);
`device.lost` via `wgpuDeviceGetLostFuture` — semantics to be verified against
the header's Doc comments before implementation. **Out:** full DOMException
hierarchy (recorded deviation, S8); error scopes on anything but the device.

## 2. Rules

**S1 — `pushErrorScope(filter)` is synchronous and thin.** WebIDL
`GPUErrorFilter` string → `WGPUErrorFilter` (generated enum conversion — the
policy subset grows by this enum); unknown string → TypeError (B6);
`wgpuDevicePushErrorScope` returns nothing. No wrapper state: the scope stack
lives in the backend.

**S2 — `popErrorScope()` is a Promise through the standard machinery, nothing
new.** `WGPUCallbackMode_AllowProcessEvents` (the `mode` field is verified
present); the callback is **pure Rust** (J1): it records
`(deferred, status, type, message)` into the settlement queue and returns.
Settlement (J2, one frame): `Success` + `NoError` → resolve `null`; `Success` +
an error type → resolve the matching `GPUError` instance carrying the
backend's message (A9's non-deferred half already threads messages);
non-Success → reject with an operation-error object whose message includes the
backend's (`webgpu.h`: empty stack and device-lost arrive here). A new
`SettlementRequest` variant carries the error type — the settle path constructs
the JS instance on the JS thread, exactly like adapters/devices.

**S3 — the error classes are real classes, hand-written this phase.**
`GPUError` exposes `message` (per IDL); the three subclasses are
script-constructible per the pinned IDL (`constructor(DOMString message)`),
and `instanceof` works. They are the first script-constructible classes in the
binding — the ClassSpec gains a constructor slot (additive; the generator does
not learn constructors this phase, recorded in §4).

**S4 — A9 retires: every async rejection reason is an error object, never a
bare string.** `mapAsync`, `requestAdapter`, `requestDevice`,
`onSubmittedWorkDone`, `popErrorScope` — the reason has `name` and `message`
properties and stringifies usefully. The existing `async_error_value` is
upgraded or replaced; block 02 A9's deferral note is closed by this rule.

**S5 — scope routing changes nothing in `createXxx`.** Validation failures are
the backend's to route into its scope stack; the binding's null-handle checks
(R13/B16) keep catching only catastrophic/misuse cases. B15's recorded
deviation ("validation failures surface as synchronous exceptions") narrows to
exactly that class and its note in `engine-boundary.md` is updated by the
implementing slice's review.

**S6 — `onuncapturederror` for adopted devices is host-forwarded, by design.**
`WGPUUncapturedErrorCallbackInfo` is settable only at device creation and has
no `mode` (it may fire like `AllowSpontaneous` — invariant 2 forbids touching
the engine or `webgpu.h` from it). For `wrap_device` (the primary entry), the
HOST owns that callback; the binding exposes a thread-safe
`forward_uncaptured_error(type, message)` that only enqueues (the J1 shape) and
dispatches to the JS `onuncapturederror` attribute during `tick()`. For
binding-created devices (`requestDevice`), the binding installs a pure-Rust
recording callback at creation that feeds the same queue. One queue, two
producers, one JS-thread consumer.

**S7 — `device.lost` waits for P6b and a header reading.** `wgpuDeviceGetLostFuture`
exists and returns a `WGPUFuture`; how it interacts with the creation-time
`WGPUDeviceLostCallbackInfo` (which HAS a mode) and with adopted devices must
be read from the header's Doc comments — not assumed — before S-rules are
written for it.

**S8 — recorded deviation: no DOMException hierarchy.** WebGPU rejections are
spec'd as DOMExceptions (`OperationError`, `AbortError`); this binding rejects
with plain error objects carrying `name`/`message`. Recorded in
`codegen-deltas.md`'s divergence section by the implementing slice; revisit if
the upstream CTS is ever run.

**S9 — tests.** Mock: push/pop round-trip per filter; pop resolving null;
pop resolving each error type with the backend message; pop on an empty stack
rejecting; settlement one-frame ordering unchanged (A30 count still 1).
Script (QuickJS): `instanceof GPUValidationError`; constructor works;
`e.message` round-trips. Parity: one deterministic
push→invalid-op→pop→classname line, byte-identical (the invalid op must be
deterministic across backends — pick one yawgpu Noop validates; verify, do not
assume). JSC runs the same suite. Negative demo: break the type→class mapping,
see the classname test red.

**S10 — the exit gate stands.** Zero core-logic changes for the JSC adapter's
benefit beyond additive capability (a constructor slot in ClassSpec is
additive). Every new `extern "C"` callback catches unwinds and calls nothing
(J15/A8).

## 3. Exit criteria (P6a)

1. push/pop/error classes work headless under both engines; parity extended,
   byte-identical.
2. A9's deferral note closed; every rejection reason has `name`/`message`.
3. All prior suites unchanged; the A30 one-batch property still asserts 1.
4. Deviations recorded (S8; B15 narrowed).
5. Phase Review clean of CRITICAL/MAJOR before P6b.
