# Tracking: backend deltas

**Historical engine note (2026-07-12):** tables labelled QuickJS record past
backend measurements and are retained for provenance. Current dual-engine
verification is Boa and JSC.

Divergences between a backend and the canonical
[`webgpu-headers`](https://github.com/webgpu-native/webgpu-headers).

Per `CLAUDE.md` → "Operational rule (backend-independent core)", a divergence is
fixed **upstream in that backend**, or catalogued here. It is never papered over
with a `cfg(backend)` check above the FFI layer.

Reference point throughout: the `webgpu.h` pinned by
[Dawn](https://dawn.googlesource.com/dawn) (`third_party/webgpu-headers/src`),
which declares **202 `WGPU_EXPORT` functions**. Both Dawn checkouts available
locally — the upstream one and a fork — pin the identical header, so the
reference is unambiguous.

---

## D1 — yawgpu's vendored header is one enumerator behind

**Status: OPEN (upstream, low severity). Found 2026-07-09.**

Canonical declares an enumerator yawgpu's vendored copy lacks:

```
WGPUFeatureName_SubgroupSizeControl = 0x00000017
```

Everything else in the *header* matches: same 202 function declarations, same
type surface.

**Action.** Report upstream; re-vendor. Does not block, because this project
generates bindings from canonical `webgpu-headers`, not from a backend's
vendored copy (`CLAUDE.md` principle 2).

> **Correction, 2026-07-09.** The first version of this entry concluded from the
> above that yawgpu was "ABI-compatible today". **That inference was wrong.** It
> compared *headers*. A header declares; a library exports. Comparing the
> shipped libraries produced D2, below. Reading a vendored header tells you what
> a backend *intends* to implement, not what it *does*.

---

## D2 — yawgpu implements 178 of the 202 canonical functions

**Status: CLOSED 2026-07-09. Fixed upstream in yawgpu.**

Re-measured after the upstream fix: `libyawgpu.dylib` now exports **200** `wgpu*`
symbols — **199 of 202 canonical**, plus the `wgpuCommandEncoderWriteBuffer`
extension. The three still absent are exactly the three named as deliberate
non-goals: `wgpuGetProcAddress`, `wgpuBufferReadMappedRange`,
`wgpuBufferWriteMappedRange`. The predicted counts matched exactly.

All 16 `SetLabel` functions landed (21 total, with the 5 that pre-existed), as
did `wgpuDeviceGetAdapterInfo`, `wgpuTextureGetTextureBindingViewDimension`, and
the instance capability quartet.

**The `WGPUStringView` trap was handled correctly.** yawgpu added a
`label_from_string_view` distinct from
`string_view_to_str`, and it discriminates on **length before nullness**:

| Input | Spec | yawgpu |
|---|---|---|
| `{NULL, WGPU_STRLEN}` | the null value | `None` ✓ |
| `{non_null, WGPU_STRLEN}` | null-terminated | `CStr` ✓ |
| `{any, 0}` | the **empty string** | `Some("")` ✓ |
| `{NULL, non_zero}` | not allowed | `None`, no dereference ✓ |

A minor observation, not raised as a defect: `string_view_to_str` (the non-label
path, used for e.g. entry points) returns `None` for `{NULL, 0}` but `Some("")`
for `{non_null, 0}`, though the spec calls both the empty string. That is a
deliberate, documented choice for treating an unset entry point as absent. It
does not affect labels, and it is pre-existing.

The original entry follows, for provenance.

---

**Status when opened: OPEN (upstream, blocks Phase 4 label support). Found 2026-07-09.**

Symbols exported by the release `libyawgpu.dylib`, diffed against the 202
canonical `WGPU_EXPORT` functions: **25 missing.** Confirmed to be genuinely
unimplemented — the corresponding `pub extern "C" fn`s are absent from yawgpu's
source, so this is not a stale build artifact.

Mind the arithmetic: yawgpu exports **178** `wgpu*` symbols, but that is not 178
*canonical* symbols. One export — `wgpuCommandEncoderWriteBuffer`, a Dawn
extension — is not declared by canonical at all. So canonical coverage is
**177 / 202**, and 177 + 25 = 202 reconciles. Any future symbol-count gate must
account for the extension, or it will be off by one.

| Group | Missing symbols |
|---|---|
| **`SetLabel` (16)** | `wgpuBindGroupSetLabel`, `wgpuBindGroupLayoutSetLabel`, `wgpuBufferSetLabel`, `wgpuCommandBufferSetLabel`, `wgpuCommandEncoderSetLabel`, `wgpuComputePassEncoderSetLabel`, `wgpuComputePipelineSetLabel`, `wgpuPipelineLayoutSetLabel`, `wgpuRenderBundleSetLabel`, `wgpuRenderBundleEncoderSetLabel`, `wgpuRenderPassEncoderSetLabel`, `wgpuRenderPipelineSetLabel`, `wgpuSamplerSetLabel`, `wgpuShaderModuleSetLabel`, `wgpuTextureSetLabel`, `wgpuTextureViewSetLabel` |
| Mapped-range accessors (2) | `wgpuBufferReadMappedRange`, `wgpuBufferWriteMappedRange` |
| Instance capability query (4) | `wgpuGetInstanceFeatures`, `wgpuGetInstanceLimits`, `wgpuHasInstanceFeature`, `wgpuSupportedInstanceFeaturesFreeMembers` |
| Misc (3) | `wgpuGetProcAddress`, `wgpuDeviceGetAdapterInfo`, `wgpuTextureGetTextureBindingViewDimension` |

`SetLabel` is **partially** implemented: yawgpu already exports
`wgpuDeviceSetLabel`, `wgpuQueueSetLabel`, `wgpuQuerySetSetLabel`,
`wgpuExternalTextureSetLabel`, and `wgpuSurfaceSetLabel`. The 16 above are the
remainder. All share one signature shape:

```c
void wgpuXxxSetLabel(WGPUXxx xxx, WGPUStringView label);
```

### Why this matters to *this* project specifically

**The `SetLabel` family is the sharp edge.** WebGPU's WebIDL gives every
`GPUObjectBase` a writable `label` attribute. A binding generated from WebIDL
(plan §2.3) will emit a setter for `label` on **every** object, and it must lower
to `wgpuXxxSetLabel`. So the moment Phase 4 generates from IDL, the binding will
reference 14 symbols yawgpu does not export, and **static linking will fail.**

Per `CLAUDE.md`'s backend-independent-core rule, the fix is **upstream in yawgpu**,
not a `cfg(backend)` in our codegen. The same organisation owns yawgpu.

**`wgpuGetProcAddress` being absent** also forecloses one design escape: we
cannot fall back on runtime proc-table resolution to paper over per-backend
symbol gaps, because the Tier 1 backend does not export the entry point that
would make it possible. Direct linking is the only strategy, which means the
symbol set must actually match.

**Not blocking Phase 0.2.** The exit criterion there is
`wgpuCreateInstance` → `wgpuInstanceRelease`, and Rust emits an undefined-symbol
reference only for `extern "C"` functions that are actually called. Unused
bindgen declarations cost nothing at link time. The gap surfaces at Phase 4.

**Not blocking `getMappedRange()` either.** `wgpuBufferGetMappedRange` *is*
exported; only the newer `Read`/`Write` variants are missing.

### Action — upstream work in yawgpu, prioritised

yawgpu is maintained by the same organisation as this project, so these are
implementable rather than merely reportable. Ordered by what unblocks this
project soonest. Each lands in yawgpu with its own inline unit test, per that
repository's principle 1.

**P0 — the 16 `SetLabel` functions.** Blocks Phase 4. Mechanical: one signature
shape, and five siblings already exist in yawgpu to copy from. Note the label is
a `WGPUStringView` (`{data, length}`, **not** null-terminated), so the
implementation must not assume a C string.

**P1 — `wgpuDeviceGetAdapterInfo`.** WebIDL exposes `GPUAdapterInfo`; needed
once adapter/device introspection is bound.
```c
WGPUStatus wgpuDeviceGetAdapterInfo(WGPUDevice device, WGPUAdapterInfo *adapterInfo);
```

**P2 — `wgpuTextureGetTextureBindingViewDimension`.** Needed for texture binding
once textures are bound.

**P3 — the instance capability quartet.** `wgpuGetInstanceFeatures`,
`wgpuGetInstanceLimits`, `wgpuHasInstanceFeature`,
`wgpuSupportedInstanceFeaturesFreeMembers`. Not IDL-facing; low urgency for this
project, but part of canonical conformance.

**P4 — `wgpuBufferReadMappedRange` / `wgpuBufferWriteMappedRange`.** *Not needed
by this project.* `wgpuBufferGetMappedRange` is exported and is what
`getMappedRange()` lowers to. Listed for canonical completeness only.

**`wgpuGetProcAddress` — last.** Nothing in this project's design needs it,
since we link directly. It
is only interesting if a future consumer wants runtime backend selection from a
single binary. Do not implement it just to close the diff.

---

## D3 — wgpu-native's vendored header is stale, but its library is complete

**Status: NOT A PROBLEM. Recorded to prevent a future false alarm.**

`libwgpu_native.dylib` exports **227** `wgpu*` functions — a strict superset of
the canonical 202, the extras being wgpu-native's own `wgpu*` extensions.
**Zero canonical symbols are missing.**

Its *vendored* `webgpu.h`, however, declares only 199 functions: it lacks
`wgpuComputePassEncoderSetImmediates`, `wgpuRenderBundleEncoderSetImmediates`,
and `wgpuRenderPassEncoderSetImmediates` — which the library nonetheless
exports. The header is simply pinned older than the source.

Irrelevant to us for the same reason as D1: we generate from canonical
`webgpu-headers`, never from a backend's vendored copy. Noted only so that a
future reader who diffs wgpu-native's header does not conclude the backend is
incomplete. It is the most complete of the three.

Note: the Tier 1 backend has the symbol gap and the Tier 2 backend does not.
Tiers express where development effort goes, not which implementation is most
finished today.

---

## D5 — both backends' dylibs carry an absolute `install_name` (macOS/iOS)

**Status: CLOSED for yawgpu (2026-07-09). OPEN for wgpu-native, low priority.**

yawgpu now sets `install_name` to `@rpath/libyawgpu.dylib`. Verified: the Phase
0.2 test binary's load command changed from an absolute path to
`@rpath/libyawgpu.dylib`, and our `LC_RPATH` now resolves it. The rpath emission
that D5 called "dead code, do not delete" is now load-bearing.

wgpu-native still exports an absolute `install_name` into its build tree. It is
Tier 2, upstream is not ours, and iOS ships yawgpu — so this stays open at low
priority rather than being worked around. Do not add an `install_name_tool`
rewrite to `ffi/build.rs` for it without a concrete need.

The original analysis follows.

**Status when opened: OPEN. Found 2026-07-09 during Phase 0.2. Affects shipping,
not correctness.**

Cargo's default for a `cdylib` on macOS leaves the `install_name` as an absolute
path into the build tree:

```
libyawgpu.dylib      -> <build-tree>/target/release/deps/libyawgpu.dylib
libwgpu_native.dylib -> <build-tree>/target/release/deps/libwgpu_native.dylib
```

Consequence, verified by inspecting the Phase 0.2 test binary's load commands:
the binary records that **absolute path** as the library to load. Our `build.rs`
emits an `LC_RPATH` entry, and dyld **never consults it**, because the load
command is absolute rather than `@rpath/…`.

Two things follow.

1. **Our rpath is currently dead code.** The Phase 0.2 tests pass only because
   the absolute path happens to exist on the machine that built the backend.
   Move or rename that directory and the binary breaks, with no fallback.
2. **The artifact violates the no-local-paths rule even though the source does
   not.** `build.rs` is clean — a hygiene grep over the tree finds nothing — yet
   the linked binary embeds a developer's home directory. The rule is about what
   we *ship*, and a check that only greps source misses this.

**This is not a yawgpu bug.** wgpu-native does the same; it is what Rust emits
for a `cdylib` unless told otherwise. It is nonetheless ours to solve, because
iOS requires `@rpath`-relative install names for embedded frameworks.

Fixes, in preference order:

1. **Upstream, cheap:** each backend sets
   `cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libX.dylib` in its own
   `build.rs`. One line. Then our existing `LC_RPATH` starts doing its job.
2. **Ours, defensive:** `ffi/build.rs` copies the dylib into `$OUT_DIR` and runs
   `install_name_tool -id @rpath/...`. Works for backends we do not control, but
   it is a build-time rewrite of someone else's artifact and should be a last
   resort.
3. Link the staticlib. **Rejected**: a Rust `staticlib` bundles its own `std`;
   linking it into another Rust target duplicates those symbols.

Until fixed, `ffi/build.rs` keeps emitting the rpath. It is harmless, and it is
exactly what becomes load-bearing the moment (1) lands. Do not delete it as
"unused".

**Android is unaffected** — Rust sets a plain `SONAME` for `.so` targets.
**iOS is affected** and will fail at packaging time, not build time.

---

## D6 — yawgpu's dylib has a transitive `@rpath/libtint_shim.dylib` not colocated

**Status: CLOSED 2026-07-09. Fixed upstream in yawgpu.**

yawgpu now colocates `libtint_shim.dylib` beside `libyawgpu.dylib` and references
it as `@loader_path/libtint_shim.dylib`, so it resolves relative to the library
that needs it rather than relative to the consumer's binary — fix (2) below.

Verified three ways:

1. `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` pointed at yawgpu's own `target/release`
   now passes (previously: `dyld: Library not loaded: @rpath/libtint_shim.dylib`).
2. The test binary's load command is `@rpath/libyawgpu.dylib`, resolved through
   the `LC_RPATH` we emit — so D5's fix is actually in force, not merely present.
3. **Relocation test.** Both dylibs copied into a scratch directory unrelated to
   yawgpu's build tree; tests pass from there — a property a compile check does
   not exercise.

The original analysis follows.

**Status when opened: OPEN (upstream). Found 2026-07-09 during Phase 0.2.**

```
libyawgpu.dylib
  └── @rpath/libtint_shim.dylib
```

`libtint_shim.dylib` is yawgpu's C++ shim over Tint. It is **not** placed next to
`libyawgpu.dylib`; it is built into
`target/release/build/yawgpu-tint-<hash>/out/build/`, under a directory whose
name embeds a Cargo build hash.

So a consumer that points `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` at yawgpu's release
directory gets a **dyld failure at test run time**, not a link error. Pointing it
at a hand-curated directory containing both dylibs works, which is how Phase 0.2
was verified against yawgpu.

The hash in the path makes this unfixable from the consumer side: there is no
stable location to add to the rpath.

**Fix upstream in yawgpu**, one of:

1. Static-link the Tint shim into `libyawgpu.dylib`, so there is one artifact.
2. Copy `libtint_shim.dylib` next to `libyawgpu.dylib` in `build.rs`, and set its
   install name to `@loader_path/libtint_shim.dylib` so it resolves relative to
   the library that needs it.

(1) is simpler for consumers and is what a distributable backend should do.
Contrast wgpu-native, which has no non-system dylib dependency at all.

Recorded in yawgpu's `HANDOFF.md` alongside D2.

---

## D4 — Dawn

**Status: DEFERRED.** No build artifacts exist locally, and a Dawn build needs
GN + depot_tools. Per plan §4 Phase 0.2, Dawn linkage is deferred to Phase 7 CI
unless it turns out to be cheap. Its header is the canonical reference in the
meantime (see above).

---

## Tier 2 backend verification — 2026-07-11 (gated real-GPU runs, Metal)

The backend-swap claim is now empirical. Same binding, same test suites, same
94-line parity script; backends selected only by cargo feature +
`WEBGPU_NATIVE_JS_BACKEND_LIB_DIR`. Runs are **gated real-GPU** (not CI): the
sandbox blocks Metal (adapter enumeration returns Unavailable inside it), so
these runs execute unsandboxed by design.

**Results:**

| Backend | QuickJS suite | JSC suite | Parity (94 lines) |
|---|---|---|---|
| yawgpu (Noop, headless) | 53/53 | 24/24 (+1 ignored) | byte-identical |
| Dawn (Metal, real GPU) | **53/53** | **24/24 (+1)** | **byte-identical** |
| wgpu-native (Metal, real GPU) | 50/50 (3 skips, all traced below) | 20/20 (4 skips, same causes) | blocked by D8 |

**The parity script is byte-identical across two engines × two backends**
(yawgpu, Dawn) — including real-GPU mapping byte round-trips under Dawn.

### New deltas (wgpu-native, catalogued not worked around)

- **D7 — request callbacks fire synchronously inside the call and
  `callback_info.mode` is unread** (wgpu-native `src/lib.rs`,
  `wgpuInstanceRequestAdapter`). Benign for this binding (J1: pure-Rust
  callbacks), recorded because invariant 2 reasons from the mode contract.
- **D8 — `wgpuDevicePopErrorScope` on an empty stack panics**
  (`error_sink.scopes.pop().unwrap()`) — a **process abort** where the header
  defines a status. Upstream-report candidate. Blocks any test that pops an
  empty scope (the parity script does, deliberately).
- **D9 — the `wgpu*SetLabel` family, `wgpuDeviceGetLostFuture`,
  `wgpuInstanceWaitAny`, `wgpuShaderModuleGetCompilationInfo` are
  `unimplemented!()`** (non-unwinding panic → SIGABRT across the C boundary).
  A script assigning `.label` aborts the process on wgpu-native. Two adapter
  tests skip on this backend for exactly this reason.
- **D10 — bounded-tick async tests were Noop-tuned** — a binding-side test
  assumption, fixed for all backends with deadline-based `tick_until` helpers
  (5s ceiling; Noop still completes on the first round).
- **D11 — sampler validation divergence, and yawgpu is the outlier**:
  `maxAnisotropy > 1` with any non-linear filter must fail createSampler per
  the pinned spec (gpuweb `index.html`, createSampler validation); wgpu-native
  enforces it; **yawgpu accepts the invalid descriptor**. Test inputs were made
  spec-valid everywhere (that is input correctness, not a workaround). yawgpu
  upstream finding for the project owner.
- **D5 (standing, re-confirmed):** wgpu-native's dylib install_name is an
  absolute path into its own build tree — non-relocatable; runs resolve only
  on the machine that built it. Dawn's is `@rpath`-clean.
- **Dawn message texts differ from yawgpu's** (e.g. the empty-pop
  diagnostic) — never a contract; the one parity line that had pinned backend
  prose now pins the binding-owned prefix and asserts the backend detail's
  presence without pinning its text.

**Owner decision (2026-07-11): no upstream reports will be filed — for any
external project.** The deltas above (D7–D9, D11) and any future ones are
catalogued for this project's own reference: they explain skipped tests,
bound what a Tier 2 backend can be expected to do, and record which side
diverges from the pinned spec/header. Nothing here implies an intent to file
issues against wgpu-native, quickjs-ng, or anything else. (This also closes
the standing "report quickjs-ng maxByteLength upstream" item as
won't-do.)

**D11 handed off to yawgpu (2026-07-11)** via the established yawgpu handoff
flow (spec citation, the wgpu-native rejection message verbatim, suggested fix
shape + tests, and the question of why a fail=0 CTS run missed it). Nothing on
the binding side waits on it.

**D11 RETRACTED (2026-07-11) — yawgpu's reply (REPORT.md) is correct and the
defect was the planner's diagnosis.** yawgpu has enforced the anisotropy/
filter rule since 2026-05-21 (their `validate_sampler_descriptor`, verified by
their C-level Noop repro: scope active → validation error fires, error sampler
returned). What actually happened on our side: createSampler returns a
NON-NULL error object on validation failure (webgpu.h-correct), our B16 check
observes only null, and the parity sampler line ran with NO error scope — so
yawgpu's rejection routed to the uncaptured path, which its default silently
drops. wgpu-native "rejected visibly" only because its default uncaptured
handler PANICS. The real deltas, restated:

- **D11′ — default uncaptured-error disposition differs**: yawgpu drops
  silently (no scope, no callback); wgpu-native aborts the process. Neither
  is contract; hosts should install the uncaptured callback / use the S6
  forwarder, and the host-contract docs already say so.
- **Error-object creation semantics**: a failed createXxx yields a live
  non-null error handle — "creation returned non-null" proves nothing about
  validity. Any test line that means "this descriptor is valid" must observe
  it under an error scope (the parity render-pipeline line already does; the
  old sampler line did not — the B2 lesson, one layer up).

The spec-valid sampler inputs stay (correct regardless). The yawgpu handoff is
closed with a retraction note. The diagnosis inferred "backend A accepts, backend
B rejects" from "suite green on A, loud on B" without isolating where the paths
diverge.
Rule: a green line proves only what it observes; isolate where the paths diverge
before assigning blame.

---

## D12 — yawgpu surface configure rejects `WGPUCompositeAlphaMode_Auto`

**Status: OPEN (upstream candidate, catalogued per the no-reports owner
decision). Found 2026-07-11 during Windows example bring-up.**

`webgpu.h` defines `WGPUCompositeAlphaMode_Auto` as "Lets the WebGPU
implementation choose the best mode (supported, and with the best performance)
between Opaque or Inherit" — i.e. always satisfiable. yawgpu's
`wgpuSurfaceConfigure` validates `alphaMode` against its advertised list
(`[Opaque]` only), which does not contain `Auto`, so a configuration passing
`Auto` is rejected as a validation error and the surface stays unconfigured.
Dawn accepts `Auto` (the triangle example's gated Dawn run configured with it).

The failure mode is quiet: the validation error goes to the device error sink,
and every later `wgpuSurfaceGetCurrentTexture` returns `Error` (status 6) with
no message anywhere unless the host installed an error callback.

**Host-side consequence (correct regardless of the delta):** the triangle
example now selects `alphaMode` from `wgpuSurfaceGetCapabilities` instead of
hard-coding `Auto` — the same handshake it already did for the format. That is
proper host behaviour, not a workaround; the delta stands on the `Auto`
semantics alone.

## D13 — yawgpu surface capabilities advertise `RenderAttachment` usage only

**Status: RECORDED (capability floor, not a header divergence). Found
2026-07-11.**

`wgpuSurfaceGetCapabilities` reports `usages = RenderAttachment` and configure
enforces it, so a surface texture can never carry `CopySrc` against yawgpu —
on any platform. Consequence: the triangle example's `--verify` center-pixel
readback (which copies from the surface texture) cannot run against yawgpu;
the gated `--verify` evidence was produced against Dawn (commit `e7e112b`).
Capabilities are allowed to vary per backend, so this is catalogued as a floor,
not a divergence. The example now checks the advertised usages up front and
fails `--verify` with a clear message instead of the silent status-6 loop.

**Dawn promoted to Oracle (2026-07-12, owner decision).** The classification
catches up with practice: Dawn has been the arbiter since the backend-swap
verification (byte-identical parity on both engines, the D11 arbitration, the
CTS plan's fail-on-Dawn-is-a-binding-bug rule). The oracle protocol lives in
CLAUDE.md: presumption-not-axiom (isolate the divergence point first — the
D11 lesson), pins-win-over-implementations, pin lockstep with Dawn's DEPS,
gated Dawn parity runs required for surface-extending slices, and
yawgpu-vs-Dawn disagreements remain owner-handoff findings. wgpu-native stays
Tier 2 Experimental.

---

## D14 — yawgpu does not validate transient-attachment rules

**Status: CLOSED 2026-07-15. Fixed upstream in yawgpu (`1a2a879`).**

**Fix verified (2026-07-15).** yawgpu implemented every transient-attachment rule
and the transient view-usage-subset rule (its `REPORT.md`, tests in
`yawgpu-core/src/{texture,texture_view,command_encoder}.rs`). Re-measured against the
rebuilt library on the current CTS pin: each family is 0 fail on yawgpu and matches
Dawn. The five families moved from Dawn-only arbitration into the curated yawgpu
suite (`validation-core.txt`); the `createView` expectation was removed.

| CTS query | yawgpu before | yawgpu after | Dawn |
|---|---|---|---|
| `createView:texture_view_usage_of_multiple_usages` | 15 / 1 | 16 / 0 | 16 / 0 |
| `createTexture:texture_usage` | 288 / 42 | 330 / 0 | 330 / 0 |
| `createTexture:depthOrArrayLayers_and_mipLevelCount_for_transient_attachments` | 0 / 2 | 2 / 0 | 2 / 0 |
| `createTexture:transient_viewFormats` | 0 / 2 | 2 / 0 | 2 / 0 |
| `render_pass,render_pass_descriptor:color_attachments,loadOp_storeOp` | 0 / 39 | 39 / 0 | 39 / 0 |

The arbitration and original analysis follow, for provenance.

**Arbitration (2026-07-15).** The affected families were run side by side on the
current CTS pin: yawgpu Noop (headless) vs Dawn (Metal, oracle). Dawn passes all to
0 fail; yawgpu accepts the descriptors the CTS expects it to reject
(`Validation succeeded unexpectedly`). Confirmed a yawgpu validation gap, binding
cleared. Filed to yawgpu's `HANDOFF.md` (Finding 1) with the four sub-rules and
reproduction. The `createView` view-usage-subset gap (the
`texture_view_usage_of_multiple_usages:usage1=16;usage2=32` expectation) went in the
same handoff as Finding 2.

The original analysis follows.

**Status when opened: OPEN (Noop backend; needs Dawn arbitration). Found 2026-07-13 (CTS B-6/B-7).**

The pinned header declares `WGPUTextureUsage_TransientAttachment = 0x20`, and the
CTS gates its transient-attachment cases on that usage being exposed — so they
run here, and they fail as *"Validation succeeded unexpectedly"*:

- `api,validation,createTexture:texture_usage` (42 cases) — a texture with
  `RENDER_ATTACHMENT | TRANSIENT_ATTACHMENT` and a dimension other than `2d` must
  fail validation; yawgpu creates it.
- `api,validation,createTexture:depthOrArrayLayers_and_mipLevelCount_for_transient_attachments`
  (2), `:transient_viewFormats` (2) — `depthOrArrayLayers` and `mipLevelCount`
  must be 1 for transient attachments; not enforced.
- `api,validation,render_pass,render_pass_descriptor:color_attachments,loadOp_storeOp`
  and the depth/stencil twin (121 subcases, all `transientTexture=true`) — a
  transient attachment may not be stored (`storeOp: "store"` / `loadOp: "load"`);
  not enforced.

**The binding was cleared first** (per D11: isolate *where* the paths diverge
before assigning blame — a binding that silently dropped the 0x20 bit would produce
an identical symptom). A texture created through the
binding with `usage: RENDER_ATTACHMENT | TRANSIENT_ATTACHMENT`, `dimension: "3d"`
reads back **`texture.usage === 48`** via `wgpuTextureGetUsage`. The bit reaches
the C ABI intact and the backend echoes it; the backend simply does not validate
the rules that go with it.

Same class as D13 and the recorded `createView` view-usage gap. Never worked
around in the binding. The affected families stay out of the curated suite until a
real-backend (Dawn) run arbitrates.

---

## D15 — Dawn on Metal does not aggregate timestamp query sets past Metal's limit

**Status: OPEN (Dawn/Metal). Found 2026-07-14 (CTS B-10).**

`api,operation,command_buffer,queries,timestampQuery:many_query_sets` — 6 cases fail
on Dawn:

```
[Invalid QuerySet (unlabeled)] is invalid due to a previous error.
```

**The boundary is exact: `numQuerySets` of 8, 16 and 32 pass; 64, 256 and 65536 fail.**

The CTS test names the cause in its own description:

> *This test is because there is a **Metal limit of 32 MTLCounterSampleBuffers**.
> Implementations are **supposed to work around this limit** by internally allocating
> larger MTLCounterSampleBuffers and having the WebGPU sets be subsets of those larger
> buffers.*

32 is exactly where it stops passing.

**The binding is cleared without a probe, by the shape of the test.** The test requires
64k query sets to be **simultaneously live** — that is what it is testing. Whether the
binding releases promptly or leaks is therefore irrelevant: 64 live query sets is the
premise, and Metal's per-process limit of 32 counter-sample buffers is hit unless the
implementation aggregates them. Aggregation is entirely Dawn's internal business; the
binding only calls `wgpuDeviceCreateQuerySet`.

Recorded as a backend delta. The six cases are carried in `expectations.txt` with this
reason; they stay out of the Dawn suite's green count rather than being hidden.

---

## D16 — yawgpu does not validate a transient resolve target

**Status: OPEN (yawgpu). Found 2026-07-15 (CTS B-11). Dawn-arbitrated, handed off.**

`api,validation,render_pass,resolve:resolve_attachment:resolveTargetUsage=48` — a
resolve target created with `RENDER_ATTACHMENT | TRANSIENT_ATTACHMENT` (48) must fail
validation: a resolve target is written (stored) by the resolve, and a transient
attachment's contents cannot persist. yawgpu accepts it
(`Validation succeeded unexpectedly`); Dawn rejects it. Measured on the current CTS
pin: yawgpu 22/1, Dawn 23/0.

Same class as D14 (transient-attachment rules), surfaced by a different family after
D14 landed. Binding cleared by the same evidence as D14 — the 0x20 bit crosses the C
ABI intact (`wgpuTextureGetUsage` reads back 48). The one case is carried in
`expectations.txt`; `render_pass,resolve:*` is otherwise in the curated suite. Filed
to yawgpu's `HANDOFF.md` (Finding 3).
