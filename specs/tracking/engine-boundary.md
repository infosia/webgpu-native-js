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
  **Verified** in `spikes/quickjs-detach/` — see Q1a.
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

### E12 — only the *bytes-pointer* accessors pin; `byteLength` does not

Checked directly, because invariant 10 would be actively harmful if read too
broadly. `JSObjectGetArrayBufferByteLength` does **not** pin: taking it and then
calling `transfer()` still yields `detached === true`.

So the hazard is confined to `JSObjectGetArrayBufferBytesPtr` and
`JSObjectGetTypedArrayBytesPtr`. Length and other metadata accessors are safe,
and post-detach verification may use them. This is what makes the verification in
`detach_and_take_private_copy()` sound.

### Revision landed (2026-07-09) — Q1b CLOSED

`spikes/jsc-detach/` now implements the E6 protocol. Gates re-run directly:
`cargo test --offline` → **8 passed**; `clippy --all-targets -- -D warnings` →
EXIT=0; ASan → EXIT=0. Same for `spikes/quickjs-detach/` (5 passed).

What changed, and why each matters:

- **Copies are `copy_nonoverlapping`.** The per-byte `JSObjectSetPropertyAtIndex`
  walk is gone. A **1 MiB** range now round-trips in both directions
  (`e6_protocol_holds_for_one_mib_ranges`), so E6 is confirmed at a realistic
  size rather than on 8-byte toys.
- **`bytes_ptr()` is deleted.** Removing it forced no other API change — it had
  existed solely to let one test assert a pointer inequality.
- **`unmap()` verifies.** `detach_and_take_private_copy()` calls `transfer()`,
  then checks the source buffer's length is `0`, and returns
  `DetachVerificationFailed` otherwise. Detach now happens **before** any pointer
  is taken, so a pinned buffer cannot reach script by construction.
- **E5 is now an executable invariant.** `pinned_script_visible_buffer_fails_loudly_on_unmap`
  deliberately pins a script-visible buffer, then asserts `unmap()` returns
  `DetachVerificationFailed` *and* that `buf.byteLength` is still `8` — pinning
  E5's silent-no-op behaviour in place. Independently reconfirmed by the agent:
  `transfer()` on a pinned buffer **does not throw**.
- **The suite now enforces the rule it documents.** If `make_visible_buffer` ever
  pinned the buffer it hands to script, every `unmap()` test would fail. The
  invariant no longer depends on anyone remembering it.
- The two engines' zero-copy tests are now deliberate mirror images: QuickJS
  asserts script **does** observe a foreign mutation while mapped; JSC asserts it
  **does not**. That divergence is the visible reason `MappedRangeStrategy` exists.

Remaining MINORs from the original review are resolved: `Error::Exception` now
carries the JS exception message, the observable `__mapped_range_buffer` global
is gone, and the silent `NaN → 0` path was removed with the per-byte copy. The
`quickjs-detach` aliasing finding is fixed.

**ASan caveat, stated once and for all:** Apple platforms ship no
LeakSanitizer. A clean ASan run on macOS demonstrates no double-free and no
use-after-free. It does **not** demonstrate the absence of leaks. In
`quickjs-detach`, leak coverage comes from the asserted `free_func` call
sequence, not from ASan.

### Original review of the spike — VERDICT: accepted as evidence, revision required

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

### Runtime proof — `spikes/quickjs-detach/`, VERDICT: accepted, `ZeroCopyDetach` confirmed

Gates re-run directly (not via the agent): `cargo test --offline` → 5 passed,
EXIT=0; `cargo clippy --offline --all-targets -- -D warnings` → EXIT=0.

| Claim | Result |
|---|---|
| Zero-copy is real | ✅ script observes a Rust-side mutation of foreign memory while mapped |
| `unmap()` detaches | ✅ `stash.byteLength === 0`; the stashed `Uint8Array` view is neutered (`length === 0`, `view[0] === undefined`) |
| **E7** `free_func` at detach, real pointer | ✅ confirmed |
| **E8** `free_func` again at finalize, `ptr == NULL` | ✅ confirmed; foreign allocation freed exactly once |
| **E9** silent no-op on non-buffer / already-detached | ✅ confirmed; the spike's verification layer turns it into a hard error |

**`ZeroCopyDetach` is proven for QuickJS.** Both arms of `MappedRangeStrategy`
now have running code behind them.

### E11 — quickjs-ng bug: `maxByteLength` ignores detachment (upstream)

The resizable-`ArrayBuffer` negative test reported `byteLength === 0` and
`detached === true`, but **`maxByteLength` still returned `16`**. Confirmed at
source: `js_array_buffer_get_maxByteLength` has **no detached guard** —

```c
if (array_buffer_is_resizable(abuf))
    return js_uint32(abuf->max_byte_length);   /* never zeroed by detach */
```

whereas `byteLength` only appears correct because detach sets
`abuf->byte_length = 0`. ECMA-262 `get ArrayBuffer.prototype.maxByteLength`
requires "**If IsDetachedBuffer(O) is true, return +0**". This is an upstream
conformance bug.

**Impact on us: near zero** — we never hand script a resizable buffer. Recorded
for two reasons. It should be reported upstream. And it means quickjs-ng's
`ArrayBuffer` getters do **not** uniformly reflect the detached state, so
post-detach verification (E9) must go through `JS_GetArrayBuffer` (null data
pointer), never through a JS-visible length getter.

### Review findings

- **MINOR-1 — aliasing discipline in `free_array_buffer`.** The callback does
  `let state = unsafe { &mut *opaque.cast::<ForeignAllocation>() }` *before* the
  null-pointer branch, and that branch then does `Box::from_raw(opaque)`. A live
  `&mut` derived from the same pointer is a Stacked Borrows violation; ASan does
  not see it, Miri would. Move the `&mut` into the non-null branch. This shape
  will be copied into the real adapter's finalizer, so fix it in the spike where
  it is cheap.
- **MINOR-2 — the ASan claim is narrower than it looks.** Apple platforms ship
  no LeakSanitizer, so `ASAN EXIT=0` on macOS proves **no double-free and no
  use-after-free**. It does **not** prove no leak. The report should say so; the
  "freed exactly once" assertion, not ASan, is what covers the leak here.

Neither is CRITICAL or MAJOR, so Q1a is closed. Both are folded into the
outstanding `spikes/jsc-detach` revision handoff (Q1b) rather than dispatched
separately.

### Original handoff (superseded — kept for provenance)

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

## Q4 — Did the boundary hold in Phase 1?

**Status: PARTLY — and the first version of this entry overclaimed. Corrected
after the Phase 1 review (`phase-reviews.md` → Phase 1).**

What is genuinely established: `core/` contains zero QuickJS or JSC references;
`cargo test -p webgpu-native-js-core` passes with no engine, no backend feature,
and the backend env var unset; the descriptor **conversion arithmetic** is
written once and generic over `E`; `wrap_device` → `createBuffer` → `label`
round-trip → `destroy()` runs headless under QuickJS against yawgpu.

What was **not** established, and what this entry previously implied:

- **The boundary already broke, for the Tier-1 engine, and the mock hid it.**
  QuickJS's `JS_GetPropertyStr` returns an **owned** value; `core/` frees none of
  the four it reads per descriptor. The mock is garbage-collected, so it cannot
  see the obligation. `createBuffer({size, usage, label: "x"})` leaks a JSString
  every call. See block 01 → **R22**, and R23 for why the mock must henceforth be
  the strictest engine, not the most forgiving.
- **`ClassSpec`'s data-driven half is unexercised.** The adapter dispatches on
  hardcoded `("GPUDevice", "createBuffer")` string pairs, with the generic path
  unreachable. Block 01 → **R24**.
- **The conversion is written once; the *dispatch* is not.** Those are different
  claims, and only the first was earned.

### The GAT is not ceremony — retracted

This entry previously said: *"`E::Context<'a>` did not need the GAT. A plain
`Copy` handle sufficed."* **Withdrawn.**

R22's fix is a **per-call handle scope**, and `Context<'a>` is exactly where it
lives: QuickJS's context carries the owned values `get_property` produced and
frees them when the scope drops; JSC's and the mock's carry nothing. That fix
requires **no change to `core/`'s logic and no `free_value` on the trait**, and
`E::Value: Copy` survives.

So the lifetime parameter is what lets an engine attach per-call obligations
without leaking them into `core/`. It is the mechanism that keeps the bet
alive. The reviewer who found R22 concluded the fix "is not additive to `core/`";
that conclusion is wrong for this reason, and the finding is right anyway.

### Secondary answers that stand

- **`Box<dyn Any + Send>` was the right payload.** No `dyn` reaches the
  conversion path; the downcast happens once per finalizer.
- The claim that QuickJS delivers `magic == 0` was **false**, and is now
  root-caused: QuickJS stores a C function's magic as an **`int16_t`**
  (`quickjs.c:1101`). Phase 1's encoding packed a class id into the high bits;
  a 16-bit field truncated them. Refusing to record the claim as fact was right.
  **Failing to demand the root-cause was the mistake** — it cost a hardcoded
  dispatch table that would have metastasised across forty interfaces in Phase 4.

### Resolved: the boundary is re-entrant

`core/` calls back into the adapter (`E::payload`, `E::get_property`,
`E::type_error`) while servicing a method. Phase 1's first generic-dispatch build
deadlocked, holding the class-registry mutex across a call into `core/` that
re-entered `payload`. Block 01 → **R25**: take what you need from a lock, drop it,
then call `core/`. This is a property of the boundary, and JavaScriptCore will hit
it identically.

### R13 — recorded deviation: `createBuffer` on failure

`wgpuDeviceCreateBuffer` is declared `WGPU_NULLABLE`. WebGPU's IDL says
`createBuffer` **always returns a `GPUBuffer`** — an *invalid* one on failure —
and routes the error to the current error scope.

Phase 1 raises a synchronous engine exception on a null handle instead, because
error scopes do not exist until Phase 6. This is a **deliberate, temporary
divergence from the IDL**, written down here rather than quietly shipped. When
error scopes land, `createBuffer` must stop throwing and start returning an
invalid buffer.

### B15 — recorded deviation: every `createXxx` failure, not just `createBuffer`

R13 recorded that `createBuffer` throws on a null handle where the IDL returns an
*invalid* `GPUBuffer` and routes to an error scope. Block 03 added **eight more
constructors** with the same throw-on-null shape: `createShaderModule`,
`createBindGroupLayout`, `createPipelineLayout`, `createBindGroup`,
`createComputePipeline`, `createCommandEncoder`, `commandEncoder.finish`, and
`beginComputePass`. All are recorded here now, not just the first.

**And there is a sharper problem the null check does not reach.** Only
`wgpuDeviceCreateBuffer` is declared `WGPU_NULLABLE`. The other seven **cannot
return null**: on a validation failure they return a non-null **invalid** handle,
which the binding wraps and hands to script as though it were fine. The failure
surfaces **nowhere** — not as an exception, not in an error scope, because error
scopes are Phase 6.

So the deviation is asymmetric: `createBuffer` fails loudly and wrongly; the other
seven fail silently and wrongly. Both are temporary. When error scopes land, all
nine must stop throwing, return their invalid object, and route the error.

Until then, this is written down rather than shipped quietly — which is the whole
point of B15.

### R11 — recorded limit: `GPUSize64` through a JS `Number`

`size` is `[EnforceRange] unsigned long long` in WebIDL and `uint64_t` in the C
ABI, but a JS `Number` represents integers exactly only up to `2^53 - 1`. Values
above that which are not exactly representable have **already lost information
before the binding sees them**. The conversion follows the IDL — it accepts the
integral value that arrives, rejecting non-finite, non-integral, negative, and
out-of-range inputs — and does not invent a stricter rule.

This is an inherent property of `Number`-typed `GPUSize64`, shared with every
browser. It is recorded so nobody later "fixes" it into a `2^53` cap, and so that
a future `BigInt` accepting path, if the IDL ever grows one, has somewhere to
attach.

---

## Q6 — JavaScriptCore's vote on the two deferred decisions

Both were deferred with the same reasoning: *shaping an abstraction with one engine
in the tree is the error that produced P2-C3.* JSC now votes, on evidence measured
against the system framework.

### A27 — `trace_payload`, not `associate_value`. **Keep it.**

`JSClassDefinition` has **no `gc_mark`** — only `initialize` and `finalize`
(`JSObjectRef.h:355`). JSC keeps a value alive by protecting it outright, so
`trace_payload` is simply a **no-op** there, and `duplicate_value` /
`release_value` map to `JSValueProtect` / `JSValueUnprotect`.

The payloads that hold engine values are the mapped ranges, and they form **no
cycle** back to their wrapper — the range's `ArrayBuffer` holds a native handle in
its `opaque`, not a JS reference. So protection is sufficient, and the
`associate_value` primitive A27 sketched buys nothing. **`trace_payload` stays.**

The cost A27 recorded — `core/` grew a lock-free side list because `gc_mark` runs
inside the collector and cannot take a lock — is therefore permanent, and paid for
QuickJS alone. That is the honest accounting: the hook is engine-neutral in its
signature and QuickJS-shaped in its existence.

### B20 — WebIDL sequence conversion is implementable. **Fix it.**

Measured: JSC reaches `Symbol.iterator` through two plain string property reads
(`globalThis.Symbol`, then `.iterator`), yielding a symbol `JSValueRef` that
`JSObjectGetPropertyForKey` accepts. **No `eval` required.** A `Set` has the
property; `{length:2, 0:1, 1:2}` does not; iterating the `Set` to completion works
from pure C.

QuickJS reaches it the same way. So the primitive is engine-neutral and additive —
`global`, `get_property_value`, `call` — and the `length`+index shortcut goes.
Array-likes must be **rejected**; `Set`s and generators **accepted**. B20's
deviation tests become conformance tests.

---

## Q7 — the JSC exit gate fired, and it caught engine-shaped `core/`

**Measured.** JSC's public C API has **no microtask pump**, and it drains
microtasks **when the JS stack empties**: resolving a promise from C runs its
`.then()` *before the C call returns*. Verified by resolving from C and then
reading `globalThis.ran` with `JSObjectGetProperty`, which executes no JS.

QuickJS does the opposite — `event-loop.md` → E2 measured that resolving leaves the
continuation pending until `JS_ExecutePendingJob`.

So the WebGPU callback, which runs inside `wgpuInstanceProcessEvents`, would
execute **script** inside that callstack under JSC and not under QuickJS. Two
engines, two orderings, one script. `CLAUDE.md` makes behavioural parity a
first-class concern; this breaks it.

**And the cause is that `core/` had encoded QuickJS's checkpoint semantics as
universal.** Nobody wrote that assumption down; it was simply what "settle the
promise in the callback" means on a refcounted engine with an explicit job queue.

The fix, block 04 → J1/J2: the callback becomes **pure Rust** — it records a
result and returns, touching no engine — and `tick()` settles every recorded
result **inside a single JS frame**, then drains. Measured: two separate C→JS
resolve calls give JSC two microtask checkpoints; one trampoline gives one, which
is what QuickJS does natively.

This is the **second** time the JSC gate has caught engine-shaped `core/` logic
before codegen could multiply it by forty. The first was P2-C3, a phase early. The
gate is doing exactly what `CLAUDE.md` built it for, and both findings argue for
running Phase 3 *before* Phase 4 rather than after.

---

## Q5 — Did the boundary survive Phase 2?

**Status: YES, and this is the first result worth much.** Phase 1 only needed
property reads and object creation, where the two engines agree. Phase 2 added
Promises, the microtask pump, external `ArrayBuffer`s, and detach — where they do
not.

**Every capability was an *addition* to `JsEngine`**: `type Deferred`,
`type AsyncContext`, `const MAPPED_RANGE_STRATEGY`, `with_async_scope`,
`new_promise`/`settle`, `drain_microtasks`, `new_external_arraybuffer`,
`detach_arraybuffer`, `arraybuffer_len`. `core/`'s conversion and object-model
logic changed only to *grow features* (`mapAsync`, range tracking), never to
accommodate an engine.

Asked precisely — *is there any line in `core/` that exists because QuickJS is
refcounted, or that would differ if JSC were the only engine?* — the answer is
**no**. Refcounting is contained in the adapter's trait impl.

`core/` now implements **both** `MappedRangeStrategy` arms and unit-tests both
against a const-generic mock, so the JSC arm is exercised before JSC exists. That
is R23 doing its job.

### The hole that opened, and why it matters more than the result

The first Phase 2 attempt gave the trait
`fn context_from_async(cx) -> Context<'static>`, and QuickJS implemented it as
`Context { ctx, scope: None }`. The WebGPU callback fires outside any JS call, so
it has no per-call scope to inherit — and the design handed it a context with
**no owner for the values it created**. That is P1-C1 verbatim, one layer down,
and it is why the real-engine `mapAsync` test leaked a promise graph at runtime
teardown (`JS_FreeRuntime`'s leak report was the diagnostic).

Two rules came out of it (block 02 → A23): `Context`'s scope is **not** an
`Option` — an engine that can silently decline the obligation will — and the
callback opens its own scope via `with_async_scope`, which drops on the way out.
`Context<'static>` must not exist.

**Value ownership has now been the boundary's hardest edge twice.** It is the one
place where QuickJS's model and JSC's genuinely differ, and both times the escape
was a type that made the obligation optional.

`AsyncContext` survives, and legitimately: a WebGPU callback outlives the JS call
that started it, so it cannot hold a `Context<'_>`. It is a raw engine token, and
reviving it is `unsafe` — which the type now says.

### `#[allow]` on a soundness lint hid a real defect for a whole phase

`cargo clippy -D warnings` was green through Phase 1 because `pub fn wrap_device`
carried `#[allow(clippy::not_unsafe_ptr_arg_deref)]`: a **safe** public function
taking a raw pointer and dereferencing it. Three independent reviewers and every
gate walked past it. Phase 2's clippy run — with the allow removed — also found
`arc_with_non_send_sync`, an `Arc` whose payload is `!Send + !Sync`.

That `Arc` is **my error**, from a Phase 0 handoff: *"an `Arc` of a non-`Send`
type is fine as long as it never crosses a thread."* Clippy is right. The type
lied about what it permitted; `Rc` is both sound here — `AllowProcessEvents`
guarantees the callback fires on the pumping thread (`event-loop.md` → E3) — and
honest. `CLAUDE.md` now treats an unjustified `#[allow]` on a correctness lint as
a review finding.

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

### E10 — `quickjs.h` puts 38 of its API behind `static inline` (build note)

Found while bringing up the Q1a spike. `quickjs.h` v0.15.1 declares **260
`JS_EXTERN` symbols and 38 `static inline` functions.** The inline set is not
peripheral — it contains `JS_FreeValue`, `JS_DupValue`, `JS_IsException`,
`JS_ToCString`, `JS_NewInt32`, and the `JSValue` tag predicates. **bindgen does
not emit `static inline` functions**, so a naive `bindgen::Builder` produces
bindings that fail to link exactly these.

Two ways out:

1. `bindgen::Builder::wrap_static_fns(true)` — bindgen emits a `.c` shim with
   real symbols wrapping each inline; compile it alongside quickjs. Mechanical.
2. Reimplement the 38 in Rust.

**Use (1).** Option (2) looks cheap and is a trap: several inlines depend on the
`JSValue` representation, which changes under `JS_NAN_BOXING`. Hand-maintaining
that in Rust duplicates an ABI decision the C header already owns, and it would
break silently on a build-flag change.

This does **not** trip the Q2b fallback — it is one builder call plus one
compiled shim, not a week. Recorded because a future reader hitting
`cannot find function JS_FreeValue` will otherwise assume raw bindgen was a
mistake.

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

---

## Q8 — What the 2026-07-10 design review changed at the boundary

**Status: RECORDED (2026-07-10).** Full findings and triage:
`phase-reviews.md` → "Design Review — 2026-07-10".

Three boundary-level corrections, each a contract that existed only as one
engine's habit until it was written down:

1. **`Self::Error` is an owned error value (block 01 → R26).** The Tier 1
   adapter was running two conventions at once — sentinel-with-pending-state in
   `get_property`, owned-object-with-cleared-state in `to_f64`/`to_str` — and the
   callback glue understood only one, so a coercion failure was *returned* to
   script instead of thrown. JSC has no pending-exception state at all, which is
   what makes the owned-value contract the engine-neutral one.

2. **`tick()`'s four-step ordering moves into `core/` (block 02 → A30).**
   `drain_microtasks` becomes the trait method A18 had listed all along. The
   deletion experiment E8 proved the settlement-batching step cannot go red under
   QuickJS — the only structural guarantee available is writing the ordering
   once, plus a mock call-count assertion in core.

3. **Engine delta, recorded before the JSC adapter exists:** JSC's public C API
   has no host promise-rejection-tracker (block 04 → F1 sweep), so A22's
   unhandled-rejection surfacing is a Tier 1 diagnostic, not a portable
   contract. J17's conformance script must catch every rejection explicitly
   (block 04 → J20). This is the same class of silent divergence J1/J2 closed,
   caught at the spec stage this time.

---

## Q9 — Phase 3 handoffs: J11 first, then the JSC adapter

**Dispatched 2026-07-10, in this order and deliberately.** J11 (iterator-based
sequence conversion) changes `core/`'s conversion machinery and both existing
test surfaces. Landing it *before* the JSC adapter keeps the JSC exit gate
honest: when the adapter is wired, `core/` is already sequence-correct and the
gate measures only the adapter. Landing them together would let sequence churn
hide under "the gate firing, as declared (J3)".

**P3a — J11 / B20 close-out.** Trait gains `global` / `get_property_value` /
`call` (additive); `sequence_len`/`sequence_item` are deleted with their only
consumer; conversion walks `Symbol.iterator` per WebIDL. B20's deviation tests
become conformance tests: array-likes rejected, `Set`s accepted, mid-iteration
failure aborts cleanly (B5). The mock models the protocol as the harder engine:
property reads on it can throw, and iteration is observable.

**P3b — the JSC adapter (J4–J21).** Follows P3a. Recorded here when dispatched.

**P3a landed (2026-07-10), reviewed and accepted.** Two known simplifications
against WebIDL's full sequence algorithm, recorded rather than silent:
(1) a non-object value (e.g. a primitive string) is not pre-rejected — it walks
its prototype's `Symbol.iterator` and fails per-element, so the outcome is still
a `TypeError` with a less precise message; (2) an element-conversion failure
does not invoke `IteratorClose` (`return()`), so a generator's `finally` does
not run on abort. Both are honest-mistake-visible and trusted-script-acceptable
(invariant 8); revisit if the upstream CTS is ever run.
