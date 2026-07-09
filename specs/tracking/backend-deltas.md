# Tracking: backend deltas

Divergences between a backend's `webgpu.h` and the canonical
[`webgpu-headers`](https://github.com/webgpu-native/webgpu-headers).

Per `CLAUDE.md` → "Operational rule (backend-independent core)", a divergence is
fixed **upstream in that backend**, or catalogued here. It is never papered over
with a `cfg(backend)` check above the FFI layer.

---

## D1 — yawgpu's vendored `webgpu.h` is one enum value behind

**Status: OPEN (upstream, low severity). Found 2026-07-09, plan §4 Phase 0.3.**

Compared the `webgpu.h` vendored by [yawgpu](https://github.com/infosia/yawgpu)
against the `webgpu-headers` copy pinned by
[Dawn](https://dawn.googlesource.com/dawn), taken as the canonical reference.

**Result: the C ABI surfaces are identical.**

| Surface | yawgpu | Dawn-pinned canonical |
|---|---|---|
| `wgpu*` functions | 202 | 202 — **no difference** |
| `WGPU*` types/enums/constants | — | — **no difference** |

The files differ by exactly one line. Canonical declares an enumerator that
yawgpu's copy lacks:

```
WGPUFeatureName_SubgroupSizeControl = 0x00000017
```

**Assessment.** No function signature, struct layout, or existing enumerator
differs, so `bindgen` output is ABI-compatible today and backend-swappability is
not at risk. yawgpu's vendored header is simply pinned slightly older than
Dawn's.

**Action.** Report upstream to yawgpu; re-vendor the header. This project must
generate its bindings from the **canonical** `webgpu-headers`, not from a
backend's vendored copy (`CLAUDE.md` principle 2), so this delta does not block
Phase 0.2. It is recorded because a project that generated from yawgpu's copy
would silently lack `SubgroupSizeControl`.

**Caveat on the word "canonical".** Dawn pins a specific `webgpu-headers`
revision; it is not necessarily upstream `HEAD`. Pinning our own
`webgpu-headers` revision — and deciding how it tracks upstream — is Phase 0.6.

---

## D2 — wgpu-native, Dawn

**Status: NOT STARTED.** Phase 0.2 links a trivial program
(`wgpuCreateInstance` → `wgpuInstanceRelease`) against each backend. Deltas found
there land here.
