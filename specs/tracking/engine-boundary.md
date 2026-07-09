# Tracking: engine boundary (`trait JsEngine`)

Topic owner: the core/adapter boundary — `CLAUDE.md` invariant 1, plan §2.4.

---

## Q1 — Does JSC's public C API expose ArrayBuffer detach?

**Status: ANSWERED (2026-07-09). No. Design absorbed it as a capability; the
boundary survives.**

This was the highest-priority Phase 0 spike (plan §4, Phase 0.1) because
`getMappedRange()` must return an `ArrayBuffer` that `unmap()` **detaches**, and
a failure here was the most likely way the "one core, two engines" bet breaks.

### Environment

macOS 26.5.1 (build 25F80), Xcode SDK 26.5, system
`JavaScriptCore.framework` (bundle version 21624). Probes were Objective-C
programs linking the system framework and driving the public C API.

### Evidence

**E1 — the public C API has no detach.** The exported surface of
`JSTypedArray.h` is exactly:

```
JSObjectMakeTypedArray                    JSObjectGetTypedArrayBytesPtr
JSObjectMakeTypedArrayWithBytesNoCopy     JSObjectGetTypedArrayLength
JSObjectMakeTypedArrayWithArrayBuffer     JSObjectGetTypedArrayByteLength
JSObjectMakeTypedArrayWithArrayBufferAndOffset
                                          JSObjectGetTypedArrayByteOffset
JSObjectMakeArrayBufferWithBytesNoCopy    JSObjectGetTypedArrayBuffer
                                          JSObjectGetArrayBufferBytesPtr
                                          JSObjectGetArrayBufferByteLength
```

A case-insensitive search for `detach` or `transfer` across **all** public
JavaScriptCore headers (`JSBase.h`, `JSObjectRef.h`, `JSValueRef.h`,
`JSTypedArray.h`, `JSStringRef.h`, `JSContextRef.h`, and the Obj-C headers)
returns **no match**. There is no C-level detach, and no C-level way to observe
detachment.

**E2 — the JS-level `ArrayBuffer.prototype.transfer()` exists and works on
*normal* buffers.** `typeof ArrayBuffer.prototype.transfer === "function"`, and
for a script-allocated `new ArrayBuffer(8)`, `b.transfer()` yields
`b.detached === true`. The `detached` getter is present (`'detached' in ab`).
This is reachable from the C API via `JSObjectGetProperty` +
`JSObjectCallAsFunction`.

**E3 — on an *external* (`…WithBytesNoCopy`) buffer, `transfer()` is not
dependable.** Across probes:

| Probe | Result |
|---|---|
| `ab.transfer()` on a fresh external buffer | once returned a new 8-byte buffer with **`ab.detached === false`** and the original still readable; once threw `TypeError: Buffer is already detached` on a *freshly created* buffer |
| `ab.transfer(8)` (explicit length) on a fresh external buffer | `detached === true` |
| `ab.transferToFixedLength()` | `TypeError` |
| `structuredClone` / `Worker` (transfer-list routes) | `ReferenceError` — absent from a bare `JSContext` |

**E4 — the `bytesDeallocator` never fired** in any probe, including after a
successful `transfer(8)`. So even where detach appears to succeed, there is no
public signal that JSC has released the external memory.

### Interpretation

E1 alone rules out the obvious design. E3's inconsistency — a no-arg
`transfer()` that silently leaves the source attached in one trial and reports
an already-detached buffer in another — means the semantics of `transfer()` over
externally-backed storage are, at minimum, unspecified for our purposes. E4
means we could not safely reclaim the memory even if detach were reliable.

The consequence is not a spec deviation, it is **memory unsafety**: if
`getMappedRange()` hands script a `…WithBytesNoCopy` view over GPU-mapped memory
and `unmap()` cannot detach it, a script that retains the `ArrayBuffer` holds a
dangling pointer. That is a CRITICAL-class defect, not a conformance footnote.

### Decision

**`getMappedRange()` never hands JSC a NoCopy view over GPU memory.**

`trait JsEngine` gains a capability, declared by each adapter:

```rust
enum MappedRangeStrategy {
    /// Wrap the GPU mapping directly; `unmap()` detaches the external buffer.
    ZeroCopyDetach,
    /// Hand script a normal engine-owned ArrayBuffer.
    /// MAP_READ:  copy GPU -> JS at getMappedRange().
    /// MAP_WRITE: copy JS -> GPU at unmap(), then detach.
    CopyInCopyOut,
}
```

- **QuickJS → `ZeroCopyDetach`** (`JS_DetachArrayBuffer` on an external buffer).
  **Not yet verified locally** — see Q1a.
- **JSC → `CopyInCopyOut`**, detaching the engine-owned buffer via E2's
  `transfer()` (the reliable path), invoked through `JSObjectCallAsFunction`.

**This is spec-conformant, not a deviation.** WebGPU defines the contents of a
mapped range as becoming visible to the GPU at `unmap()`, so copying at `unmap()`
is exactly the contract. The cost is a bounded number of `memcpy`s per mapped
range per map cycle on the JSC tier — a **performance** difference, not a
behavioural one. JSC is Tier 2 / experimental (`CLAUDE.md` → Engine support
tiers), so this is an acceptable price.

> **Amended 2026-07-09 after the Q1b spike.** The original wording here said
> "one `memcpy`". That was wrong, and the reason it was wrong is important
> enough to have its own section — see **Q1b, the pinning hazard**. The copy
> cannot be done through the obvious C pointer at all. The corrected protocol
> is in Q1b → "Decided JSC mapping protocol"; it costs two copies per
> direction, still O(n) `memcpy` and not O(n) engine calls.

### Why this validates the boundary rather than breaking it

The fix is a **capability enum plus an additive trait method**
(`detach_arraybuffer` returning `Result<(), Unsupported>`, and a
`MAPPED_RANGE_STRATEGY` associated const). `core/` implements *both* strategies
once, generic over `E`, and selects on the capability. No engine-specific branch
enters `core/`, and no conversion logic is duplicated.

Per `specs/reference/workflow.md` → "The JSC phase carries an extra exit gate",
adding a trait method or capability variant is **additive** and does not trip the
gate. `CLAUDE.md` invariant 1 stands.

Had we discovered this in Phase 3 instead of Phase 0, the copy path would have
been retrofitted into ~40 generated interfaces. This is what Phase 0 is for.

---

## Q1b — The pinning hazard (found by the JSC spike, 2026-07-09)

**Status: ANSWERED. This is the most dangerous thing found so far.**

The Q1a JSC-arm spike (`spikes/jsc-detach/`) came back with an unexpected
report: taking the buffer's C bytes pointer appeared to prevent later detach.
Independently reproduced, and it is **worse than reported**.

### E5 — taking a C bytes pointer permanently and *silently* disables detach

| Sequence on an engine-owned `new ArrayBuffer(8)` | `transfer()` result |
|---|---|
| no C pointer taken | `detached = true` ✅ |
| `JSObjectGetArrayBufferBytesPtr` taken first | `detached = **false**`, no exception |
| `JSObjectGetTypedArrayBytesPtr` taken first | `detached = **false**`, no exception |

This is WebKit's `ArrayBuffer::pinAndLock()`: once a C client takes the pointer,
the buffer is non-detachable for the rest of its life. `transfer()` does not
throw — it **silently degrades to a copy** and leaves the original attached.

**Why this is the worst possible failure mode.** The natural, obvious
implementation of `CopyInCopyOut` is: allocate an engine-owned `ArrayBuffer`,
take its bytes pointer, `memcpy`, hand it to script; at `unmap()`, `memcpy` back
and `transfer()` to detach. **Every step succeeds. No error is raised. And the
buffer is never detached** — script keeps a live, readable, writable view after
`unmap()`. The exact hazard `CopyInCopyOut` exists to prevent is reintroduced
through the back door, with no diagnostic.

A test suite that never calls `bytes_ptr()` and `unmap()` on the *same* mapping
will not catch this. The spike's own tests do not: `read_mapping_…` calls
`bytes_ptr()` but never unmaps.

### E6 — the staging + `transfer()` protocol restores a fast, safe path

Verified directly. `transfer()` on a *pinned* buffer still returns a **new,
unpinned, correctly-populated** buffer; and the product of `transfer()` is itself
detachable.

```
staging (pinned, memcpy'd from foreign) --transfer()--> visible  [bytes intact]
visible.transfer() -> detached = true                            [detachable ✅]

v2 (script-visible, never pinned) --transfer()--> out            [v2 detached ✅]
C bytes pointer of `out` -> memcpy to foreign                    [safe: out is private]
```

### Decided JSC mapping protocol

**Rule: the C bytes pointer of any buffer script can see must never be taken.**

- **`getMappedRange()` (copy-in).** Allocate a *staging* `ArrayBuffer`; take its
  C pointer (pinning it — it is private); `memcpy` foreign → staging; then
  `visible = staging.transfer()`. Hand `visible` to script and drop `staging`.
  `visible` is unpinned, populated, and detachable.
- **`unmap()` (copy-out).** `out = visible.transfer()` — this detaches the
  script-visible buffer (the required semantics) and yields a private copy.
  *Then* take `out`'s C pointer and `memcpy` out → foreign.

Cost: two copies per direction (one `memcpy`, one engine-internal transfer copy),
both O(n) bytes. **Not** O(n) engine calls. Acceptable for a Tier 2 engine.

This protocol also removes the ordering trap: detach happens *before* we ever
touch a pointer, so a pinned buffer can never reach script.

### Review of the spike — VERDICT: accepted as evidence, revision required

Gates re-run directly (not via the agent): `cargo test` → 6 passed, EXIT=0;
`cargo clippy --all-targets -- -D warnings` → EXIT=0; zero external crates
(`Cargo.lock` has one package); agent reports ASan EXIT=0.

The spike **does** prove the JSC arm's core claim: after `unmap()`, a script that
stashed the buffer observes `stash.byteLength === 0` and cannot read through it.
That result stands.

Findings:

- **MAJOR-1 — `MappedRange::bytes_ptr()` is a public footgun.** It pins the
  script-visible buffer, silently disabling the detach that `unmap()` depends on.
  It exists only so one test can assert the pointer differs from the foreign
  pointer. No API that can permanently break `unmap()` may be reachable that way.
  The invariant is currently unenforced *and* untested.
- **MAJOR-2 — the copy is O(n) engine calls, not O(n) bytes.**
  `copy_from_foreign`/`copy_to_foreign` walk the range one byte at a time through
  `JSObjectSetPropertyAtIndex` / `JSObjectGetPropertyAtIndex`. The agent chose
  this *because* of E5, which was the right instinct, but the cost is
  unacceptable: a 4 MiB mapped range becomes ~4 million engine calls. E6's
  protocol gets it back to `memcpy`.
- **MINOR-1** — `copy_to_foreign` does `number as u8`; a `NaN` (from an
  `undefined` slot) silently becomes `0` rather than erroring.
- **MINOR-2** — `Error::Exception(&'static str)` discards the JavaScript
  exception's message, which will make every future JSC failure hard to diagnose.
- **MINOR-3** — `temporary_uint8_view` sets and deletes a global named
  `__mapped_range_buffer`, which is observable from script.

MAJOR-1 and MAJOR-2 must be fixed before this informs adapter design.

### Revision handoff → coding agent

```
## Task: engine-boundary — revise the JSC spike to the staging/transfer protocol

Phase: 0 (spike revision). Read specs/tracking/engine-boundary.md -> Q1b first.

Fix, in order:
- MAJOR-2: replace the per-byte JSObjectSet/GetPropertyAtIndex copies with the
  E6 protocol. copy-in: staging ArrayBuffer -> take C ptr -> memcpy ->
  visible = staging.transfer(). copy-out: out = visible.transfer() (detaches
  the script-visible buffer) -> take out's C ptr -> memcpy to foreign.
  The copies must be std::ptr::copy_nonoverlapping, not loops over JS indices.
- MAJOR-1: delete the public bytes_ptr(). Replace the "pointer is not the
  foreign pointer" assertion with a stronger, behavioural test: mutate foreign
  memory from Rust while the range is mapped and prove script does NOT observe
  the change (for a Read mapping). That tests the property we actually care
  about without pinning anything.
- Add a regression test that would have caught E5: take a C bytes pointer of a
  script-visible buffer, then unmap, and assert that detach FAILS loudly.
  Encode E5 as a documented, tested invariant rather than folklore.
- MINOR-1: make out-of-range / non-numeric slots an error, not a silent 0.
- MINOR-2: carry the JS exception message in Error::Exception.
- MINOR-3: avoid the observable global; keep the Uint8Array view in C.

Out of scope: QuickJS arm, real GPU, core/ changes, commits, specs/ edits.

Acceptance criteria:
- [ ] no C bytes pointer is ever taken from a buffer script can reach
- [ ] copies are memcpy, not per-element engine calls
- [ ] the E5 regression test exists and passes
- [ ] post-unmap: stash.byteLength === 0 from script
- [ ] Read mapping: foreign mutation while mapped is NOT visible to script
- [ ] zero external crates; builds offline; clippy clean with -D warnings
- [ ] ASan clean, or a plain statement of why it could not run

Report back: files changed, gate output, and whether E6 held under memcpy-sized
ranges (try >= 1 MiB) rather than the 4-16 byte toys used so far.
```

---

## Q1a — Does QuickJS `JS_DetachArrayBuffer` work on an external buffer?

**Status: ANSWERED AT SOURCE LEVEL (2026-07-09), runtime proof handed off.**

quickjs-ng is now pinned (Q3), so the implementation is readable directly.
Reading it changed the design *and* corrected acceptance criteria this document
had previously stated wrongly.

### E7 — detach works on external buffers, and calls `free_func` **at detach**

`JS_DetachArrayBuffer` (quickjs-ng `quickjs.c`):

```c
void JS_DetachArrayBuffer(JSContext *ctx, JSValueConst obj) {
    JSArrayBuffer *abuf = JS_GetOpaque(obj, JS_CLASS_ARRAY_BUFFER);
    if (!abuf || abuf->detached) return;          /* silent no-op */
    if (abuf->free_func)
        abuf->free_func(ctx->rt, abuf->opaque, abuf->data);
    abuf->data = NULL; abuf->byte_length = 0; abuf->detached = true;
    /* ...and every typed-array view over it gets count = 0, ptr = NULL */
}
```

So the `ZeroCopyDetach` arm is real: `JS_NewArrayBuffer(ctx, ptr, len, free_func,
opaque, /*is_shared=*/false)` over foreign memory, detached at `unmap()`. The
buffer's views are neutered in the same call.

**But `free_func` fires at `unmap()` time, synchronously, on the JS thread** —
not at GC. For a zero-copy view over a GPU mapping this is the wrong place to
free anything: the mapping is owned by the backend and released by
`wgpuBufferUnmap`. **Pass a null `free_func`**, or a no-op. This is a design
input, not a detail.

### E8 — `free_func` is called a **second time**, with `ptr == NULL`

`js_array_buffer_finalizer` calls `abuf->free_func(rt, abuf->opaque, abuf->data)`
**unconditionally — it does not check `abuf->detached`.** After a detach,
`abuf->data` is `NULL`, so the sequence over a buffer's life is:

| Event | `free_func` called with |
|---|---|
| `JS_DetachArrayBuffer` | the real pointer |
| later GC / finalize | **`NULL`** |

**Any `free_func` that does `Box::from_raw(ptr)`, `wgpuBufferUnmap`, or a plain
`free(ptr)` without a null guard is a double-free or a null deref.** This is the
QuickJS-side twin of the JSC pinning hazard (Q1b): an unguarded path with no
diagnostic.

**This corrects an acceptance criterion this document previously stated.** The
earlier handoff demanded "`free_func` fires exactly once". That is false and,
taken literally, would have produced a broken implementation. The correct
requirement is: **`free_func` fires exactly once with a non-null pointer, and
must tolerate a subsequent call with `NULL`.**

### E9 — `JS_DetachArrayBuffer` returns `void` and silently no-ops

It returns nothing, and does nothing at all if the value is not an `ArrayBuffer`
or is already detached. Like JSC's `transfer()` (Q1b/E5), **it cannot fail
loudly.** The adapter must *verify* detachment after the call — e.g.
`JS_GetArrayBuffer` yielding a null pointer / zero length — rather than trusting
that the call did anything.

### Consequence for the boundary

Both engines can detach, and **both can silently fail to.** Verification after
detach is therefore not engine-specific defensive coding; it belongs in `core/`,
once, as part of the `unmap()` contract. This is a *good* outcome for
`CLAUDE.md` invariant 1 — the shared logic grew, the engine-specific surface did
not.

Note also that quickjs-ng ships resizable `ArrayBuffer` (`max_len`) and
`JS_IsImmutableArrayBuffer`, neither of which exists in Bellard's. Neither is
exercised by our path, but a resizable buffer reaching `JS_DetachArrayBuffer` is
worth one negative test.

### Handoff → coding agent (runtime proof)

### Handoff → coding agent

```
## Task: engine-boundary — prove both MappedRangeStrategy arms

Phase: 0
Goal: A headless Rust harness that demonstrates, for each engine, that a mapped
      range handed to script is unreachable after unmap().

Inputs to read:
- specs/tracking/engine-boundary.md  (this file: E1..E4, the Decision)
- specs/reference/workflow.md, CLAUDE.md

Dependencies (per Q2, already decided — do not re-litigate):
- quickjs-ng, pinned git submodule at third_party/quickjs, built from our own
  build.rs. Bindings via bindgen. Do NOT add rquickjs or rquickjs-sys.
- JSC: link the system JavaScriptCore.framework (macOS). No submodule.

Produce:
- A throwaway spike crate (NOT core/): for each engine, allocate a page,
  expose it to script as an ArrayBuffer per that engine's strategy, run a
  script that stashes a reference to it, "unmap", then prove from script that
  the stashed reference is detached (byteLength === 0 / throws on access).
- Spike crate spikes/quickjs-detach/ (standalone, NOT a workspace member,
  NOT core/). Build quickjs-ng from the pinned submodule via build.rs (cc),
  bindings via bindgen. No rquickjs, no rquickjs-sys.
- Prove ZeroCopyDetach: JS_NewArrayBuffer(ctx, ptr, len, free_func, opaque,
  /*is_shared=*/false) over Rust-owned "foreign" memory; script stashes the
  buffer; JS_DetachArrayBuffer at "unmap"; prove from script that the stash
  is detached (byteLength === 0, typed-array views neutered).

Read E7/E8/E9 above BEFORE writing free_func. Specifically:
- free_func is invoked BY JS_DetachArrayBuffer with the real pointer, and
  AGAIN by the finalizer with ptr == NULL. It must be null-tolerant. Test
  both calls explicitly; a free_func that frees unconditionally is a
  double-free and ASan will say so.
- JS_DetachArrayBuffer returns void and no-ops silently. Do not trust it:
  verify detachment afterwards and surface a hard error if it did not happen.

Out of scope: real GPU, webgpu.h calls, core/ changes, commits, specs/ edits,
the JSC arm (already landed in spikes/jsc-detach).

Acceptance criteria:
- [ ] post-unmap access from script fails observably (byteLength === 0)
- [ ] free_func called exactly once with a non-null ptr, and tolerates the
      later NULL call; assert on the observed call sequence
- [ ] detach is verified, not assumed; a failed detach raises a hard error
- [ ] negative test: a resizable ArrayBuffer (maxByteLength) through the same
      path — document what happens, do not paper over it
- [ ] ASan clean: no double free, no use-after-free, no leak
- [ ] headless: no GPU, no window
- [ ] no local or sibling filesystem paths in any committed file
- [ ] clippy clean with -D warnings

Report back: files changed, the observed free_func call sequence, what the
resizable-buffer case did, and gate output. If detach does NOT work on an
external buffer, STOP and report — the ZeroCopyDetach arm depends on it.
```

---

## Q2 — Which QuickJS fork, and `rquickjs` vs raw `bindgen`?

**Status: DECIDED (2026-07-09).**
**Fork: [quickjs-ng](https://github.com/quickjs-ng/quickjs), pinned as a git
submodule. Bindings: raw `bindgen` from our own `build.rs`. We depend on neither
`rquickjs` nor `rquickjs-sys`.**

### Q2a — the fork

**Decision: quickjs-ng.** The decisive criterion is **MSVC support**, and it is
not a close call.

Windows is a *development target*, and `CLAUDE.md` → "Target platforms" makes
behavioral parity across all four platforms a first-class concern precisely
because dev/test results must predict production behavior. An engine that does
not build under the platform's native toolchain undermines that.

| | Bellard's `quickjs` | `quickjs-ng` |
|---|---|---|
| MSVC | **No official support**; long-standing open issues requesting it | **"Windows as a first class citizen"**, MSVC supported |
| Build system | Makefile, Unix-oriented | CMake, explicitly for cross-platform |
| Android / iOS | not called out | called out as supported platforms |
| CI | — | every PR tested in **50+ configurations** across OSes, build types, and **sanitizers**; test262 on every change |
| Cadence | occasional | ~a release every 2 months; latest v0.15.1 (2026-06-04), 20 releases |
| Community | Bellard + Gordon | 40+ contributors, 400+ PRs reviewed in the open |
| License | MIT | **MIT** (Bellard, Gordon, Noordhuis, Ibarra Corretgé) |

The sanitizer CI matters concretely: the Q1a harness below must run under ASan,
and an engine already tested under sanitizers upstream is far likelier to give a
clean signal about *our* bug rather than *its* bug.

**Do not rest this decision on "the original went dormant."** That is
quickjs-ng's own framing of its motivation, and the original has continued to
ship occasional releases. The decision rests on MSVC, CMake, and CI breadth,
which are directly verifiable.

**Divergence accepted.** quickjs-ng drops Bellard's `libbf`-based BigDecimal /
BigFloat and adds Resizable ArrayBuffer, `Float16Array`, iterator helpers, and
V8-compatible stack-trace APIs. None of the removals touch WebGPU binding work,
and bytecode incompatibility with Bellard's is irrelevant to us. **Resizable
ArrayBuffer is worth a note** — it changes ArrayBuffer internals, so Q1a's
detach-on-external-buffer test is a genuine test, not a formality.

The APIs the design depends on are present: `JS_NewArrayBuffer(ctx, buf, len,
free_func, opaque, is_shared)` accepts **external memory with a custom
`JSFreeArrayBufferDataFunc`**, and `JS_DetachArrayBuffer(ctx, obj)` exists. This
is the API-level basis for the `ZeroCopyDetach` arm; Q1a proves it at runtime.

### Q2b — `rquickjs` vs raw `bindgen`

**Decision: raw `bindgen`. Depend on neither crate.** Three reasons, in order of
weight.

**1. `rquickjs` is the wrong layer.** It is a *high-level safe binding* with its
own context/lifetime model and its own class-registration system. Our entire
thesis (`CLAUDE.md` invariant 1) is that `trait JsEngine` is the one abstraction
over engines. Putting rquickjs beneath it stacks two abstractions doing the same
job, and the lower one would dictate the shape of the upper. The adapter needs
raw, precise control over class opaque pointers, finalizer timing,
`JS_DetachArrayBuffer`, and `JS_ExecutePendingJob` — exactly the primitives a
safe wrapper exists to hide.

**2. `rquickjs-sys` ships no bindings for our production platforms.** Its
pre-generated bindings cover Linux, Windows (gnu + msvc), macOS, and wasm32-wasi.
**Neither Android nor iOS.** For our two ship targets we would enable its
`bindgen` feature anyway — i.e. run bindgen ourselves, through someone else's
`build.rs`, against a quickjs-ng revision they pin rather than we do.

**3. Symmetry with what we already do.** Phase 0.2/0.6 pins `webgpu-headers` and
runs `bindgen` over `webgpu.h` in our own `build.rs`. Doing the same for
`quickjs.h` from a pinned quickjs-ng submodule is the *same machinery*, one fewer
third-party `build.rs` between us and the engine we are abstracting, and it
honours `CLAUDE.md` principle 9 (generated code is a build artifact, never
committed, fix the generator not the output).

**Cost, stated honestly:** we own the NDK / iOS / MSVC build wiring for the C
sources. We own it under either choice, because of reason 2.

**`rquickjs-sys` remains useful as a reference** (MIT): its `build.rs` shows a
working `cc`-based vendored quickjs-ng compile. Read it; do not depend on it.

**Named fallback.** If our own `build.rs` for the four platforms exceeds roughly
a week of effort, adopt `rquickjs-sys` with the `bindgen` feature and revisit.
Record the switch here. This is a build-plumbing decision, not an architectural
one — it must not be allowed to become one.

---

## Q3 — Which QuickJS revision is pinned?

**Status: PINNED (2026-07-09).**

| | |
|---|---|
| Submodule path | `third_party/quickjs` |
| Upstream | https://github.com/quickjs-ng/quickjs |
| Tag | `v0.15.1` |
| Commit | `fd0a0210b7be00957751871e7e01b8291268fc29` |
| License | MIT |

The `webgpu-headers` pin (Phase 0.6) is still open and is tracked in
`backend-deltas.md` → D1's caveat. Two independently-versioned upstreams; both
pins live in tracking docs, never in prose.
