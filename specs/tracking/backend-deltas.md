# Tracking: backend deltas

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

**Status: OPEN (upstream, blocks Phase 4 label support). Found 2026-07-09.**

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

This is the first concrete case of the rule in `CLAUDE.md` biting the way it was
designed to: the fix is **upstream in yawgpu**, not a `cfg(backend)` in our
codegen. Fortunately the same organisation owns yawgpu.

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

**`wgpuGetProcAddress` — deliberately last, and worth a decision rather than a
reflex.** Nothing in this project's design needs it, since we link directly. It
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

Note the irony worth remembering when choosing what to trust: **the Tier 1
backend has the symbol gap and the Tier 2 backend does not.** Tiers express
where development effort goes, not which implementation is most finished today.

---

## D5 — both backends' dylibs carry an absolute `install_name` (macOS/iOS)

**Status: OPEN. Found 2026-07-09 during Phase 0.2. Affects shipping, not
correctness.**

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

Two things follow, and the second is the one that matters.

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

**Status: OPEN (upstream). Found 2026-07-09 during Phase 0.2.**

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
