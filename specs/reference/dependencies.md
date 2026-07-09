# External dependencies and how they are located

Two rules govern everything below, both from `CLAUDE.md`:

1. **Nothing committed may reference a path outside this repository.** A
   developer having a sibling checkout is a convenience of their machine, not a
   fact the build may rely on.
2. **All GPU calls cross the `webgpu.h` C ABI**, generated from the *canonical*
   `webgpu-headers` — never from a backend's vendored copy.

## Pinned sources (git submodules)

| Path | Upstream | Pin | License |
|---|---|---|---|
| `third_party/quickjs` | [quickjs-ng](https://github.com/quickjs-ng/quickjs) | `v0.15.1` (`fd0a021`) | MIT |
| `third_party/webgpu-headers` | [webgpu-headers](https://github.com/webgpu-native/webgpu-headers) | `a11ef44` | BSD-3 |
| `third_party/gpuweb` | [gpuweb](https://github.com/gpuweb/gpuweb) (for `webgpu.idl`) | *to add* — Phase 4 | W3C |

The `webgpu-headers` checkout supplies **both** codegen inputs named by plan
§2.3's C-ABI half: `webgpu.h` (for `bindgen`) and `webgpu.yml` (plus
`webgpu.json` and `schema.json`). Its `webgpu.h` is byte-identical to the copy
Dawn pins, so "canonical" is unambiguous. The pinned commit is, fittingly,
*"Add `subgroup-size-control` feature"* — the very enumerator yawgpu's vendored
header lacks (`backend-deltas.md` → D1); yawgpu is pinned one commit behind it.

**Alignment policy.** Dawn is the conformance oracle for the backend layer, so
we pin the same revisions Dawn pins unless we have a reason not to. As of
2026-07-09 Dawn's `DEPS` pins:

- `webgpu-headers` → `a11ef4462405c4506ad7284e5b1edeff2750bb54`
- `gpuweb` → `acaf809d9323e72429d2252e372ee4d917fc40eb`

Record any divergence here, with the reason.

## Where `webgpu.idl` comes from

Answers plan §6 Q4. It is **not** part of `webgpu-headers`. It lives in the W3C
**gpuweb** repository and is the normative WebIDL of the WebGPU specification.

Dawn's `src/dawn/node/BUILD.gn` sets
`dawn_webgpu_idl_path = "$dawn_root/third_party/gpuweb/webgpu.idl"` and feeds it
to `idlgen` together with `interop/Browser.idl` and `interop/DawnExtensions.idl`
to generate `WebGPU.h` / `WebGPU.cpp`. That is the precedent this project's
codegen follows (plan §2.3): **WebIDL supplies the JS-facing shape, `webgpu.yml`
/ `webgpu.h` supplies the C ABI to lower onto.**

`webgpu.idl` and `webgpu.h` are versioned **independently upstream**. Pinning
both, and recording how they are kept consistent, is a standing obligation of
this document — not something to rediscover at Phase 4.

## Locating a backend at build time

The FFI crate links one backend, selected by Cargo feature (`yawgpu`,
`wgpu-native`, `dawn`). The **library itself is never assumed to sit at a
relative path**. `build.rs` resolves it, in order:

1. An explicit environment variable — `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR`.
2. `pkg-config`, where the backend installs a `.pc` file.
3. Failure, with an error message naming the variable.

There is no fallback to a sibling directory, and a developer's local checkout is
wired up through the environment or a **gitignored** `.cargo/config.toml`, never
through a committed path. See `CLAUDE.md` → "No local or sibling paths in
committed files" for why: a filesystem path to a backend in `build.rs`
re-couples this project to that backend through the filesystem, undoing the
whole point of going through the C ABI.

## Backend availability, as of 2026-07-09

| Backend | Prebuilt locally | Notes |
|---|---|---|
| yawgpu | `.a` + `.dylib` (release) | Tier 1. Exports 199 of the canonical 202; the three absent are deliberate non-goals (D2, closed). `target/release` is self-contained and **relocatable**: `@rpath` install name, `libtint_shim.dylib` colocated via `@loader_path` (D5, D6, closed). |
| wgpu-native | `.a` + `.dylib` (release) | Tier 2. Exports 227 — a superset; no canonical symbol missing, no non-system dylib dependency. Still has an absolute `install_name` (D5, open, low priority). |
| Dawn | none | Tier 2. Heavy build (GN + depot_tools). Deferred to Phase 7 CI per plan §4. |

Point `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` at the backend's `target/release`. For
yawgpu that now works from any location the two dylibs are copied to, which is
what makes iOS packaging tractable.

Dawn exists locally as two checkouts, an upstream one and a fork. They pin
**identical** `webgpu-headers`, so either serves as a header reference. Prefer
upstream when citing behaviour, since the fork carries local changes.
