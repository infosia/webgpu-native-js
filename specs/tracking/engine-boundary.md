# Tracking: engine boundary (`trait JsEngine`)

**Historical engine note (2026-07-12):** the QuickJS selection, pin, detach
spike, and adapter findings below are retained as the record of earlier
decisions. QuickJS was later removed for Boa; current engine policy is in
`CLAUDE.md` and `specs/blocks/14-boa-engine.md`.

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

Found in Phase 0 rather than Phase 3, so the copy path did not have to be
retrofitted into ~40 generated interfaces.

---

## Q1b — The pinning hazard (found by the JSC spike, 2026-07-09)

**Status: ANSWERED (2026-07-09).**

The Q1a JSC-arm spike (`spikes/jsc-detach/`) reported that taking the buffer's C
bytes pointer appeared to prevent later detach. Independently reproduced; the
effect is broader than the spike reported.

### E5 — taking a C bytes pointer permanently and *silently* disables detach

| Sequence on an engine-owned `new ArrayBuffer(8)` | `transfer()` result |
|---|---|
| no C pointer taken | `detached = true` ✅ |
| `JSObjectGetArrayBufferBytesPtr` taken first | `detached = **false**`, no exception |
| `JSObjectGetTypedArrayBytesPtr` taken first | `detached = **false**`, no exception |

This is WebKit's `ArrayBuffer::pinAndLock()`: once a C client takes the pointer,
the buffer is non-detachable for the rest of its life. `transfer()` does not
throw — it **silently degrades to a copy** and leaves the original attached.

**Why the obvious implementation fails silently.** The natural implementation of
`CopyInCopyOut` is: allocate an engine-owned `ArrayBuffer`, take its bytes pointer,
`memcpy`, hand it to script; at `unmap()`, `memcpy` back and `transfer()` to
detach. Every step succeeds, no error is raised, and the buffer is never detached —
script keeps a live, readable, writable view after `unmap()`, with no diagnostic.
That reintroduces the exact hazard `CopyInCopyOut` exists to prevent.

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

**ASan caveat:** Apple platforms ship no
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

### The GAT is load-bearing — earlier claim retracted

This entry previously said: *"`E::Context<'a>` did not need the GAT. A plain
`Copy` handle sufficed."* **Withdrawn.**

R22's fix is a **per-call handle scope**, and `Context<'a>` is where it
lives: QuickJS's context carries the owned values `get_property` produced and
frees them when the scope drops; JSC's and the mock's carry nothing. That fix
requires **no change to `core/`'s logic and no `free_value` on the trait**, and
`E::Value: Copy` survives.

The lifetime parameter is what lets an engine attach per-call obligations
without leaking them into `core/`. The reviewer who found R22 concluded the fix
"is not additive to `core/`"; that conclusion is wrong for this reason, and the
finding stands.

### Secondary answers that stand

- **`Box<dyn Any + Send>` was the right payload.** No `dyn` reaches the
  conversion path; the downcast happens once per finalizer.
- The claim that QuickJS delivers `magic == 0` was **false**, and is now
  root-caused: QuickJS stores a C function's magic as an **`int16_t`**
  (`quickjs.c:1101`). Phase 1's encoding packed a class id into the high bits;
  a 16-bit field truncated them. The claim had been recorded without being
  root-caused; the consequence was a hardcoded dispatch table that Phase 4 would
  have replicated across forty interfaces.

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

**A second problem the null check does not reach.** Only
`wgpuDeviceCreateBuffer` is declared `WGPU_NULLABLE`. The other seven **cannot
return null**: on a validation failure they return a non-null **invalid** handle,
which the binding wraps and hands to script as though it were fine. The failure
surfaces **nowhere** — not as an exception, not in an error scope, because error
scopes are Phase 6.

So the deviation is asymmetric: `createBuffer` fails loudly and wrongly; the other
seven fail silently and wrongly. Both are temporary. When error scopes land, all
nine must stop throwing, return their invalid object, and route the error.

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

Cause: `core/` had encoded QuickJS's checkpoint semantics as universal — the
assumption was never written down.

The fix, block 04 → J1/J2: the callback becomes **pure Rust** — it records a
result and returns, touching no engine — and `tick()` settles every recorded
result **inside a single JS frame**, then drains. Measured: two separate C→JS
resolve calls give JSC two microtask checkpoints; one trampoline gives one, which
is what QuickJS does natively.

This is the second time the JSC gate has caught engine-shaped `core/` logic before
codegen could multiply it by forty (the first was P2-C3, a phase early). Both
findings argue for running Phase 3 before Phase 4 rather than after.

---

## Q5 — Did the boundary survive Phase 2?

**Status: YES.** Phase 1 only needed
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

### The hole that opened

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

Value ownership has been the boundary's hardest edge twice (P1-C1 and A23). It is
the one place where QuickJS's model and JSC's genuinely differ, and both times the
escape was a type that made the obligation optional.

`AsyncContext` survives, and legitimately: a WebGPU callback outlives the JS call
that started it, so it cannot hold a `Context<'_>`. It is a raw engine token, and
reviving it is `unsafe` — which the type now says.

### `#[allow]` on a soundness lint hid a real defect for a whole phase

`cargo clippy -D warnings` was green through Phase 1 because `pub fn wrap_device`
carried `#[allow(clippy::not_unsafe_ptr_arg_deref)]`: a **safe** public function
taking a raw pointer and dereferencing it. Neither the three reviewers nor any gate
flagged it. Phase 2's clippy run — with the allow removed — also found
`arc_with_non_send_sync`, an `Arc` whose payload is `!Send + !Sync`.

That `Arc` came from a Phase 0 handoff claiming *"an `Arc` of a non-`Send` type is
fine as long as it never crosses a thread."* Clippy is right: the type permitted
more than the code intended. `Rc` is sound here — `AllowProcessEvents` guarantees
the callback fires on the pumping thread (`event-loop.md` → E3). `CLAUDE.md` now
treats an unjustified `#[allow]` on a correctness lint as a review finding.

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

**Cost:** we own the NDK / iOS / MSVC build wiring for the C
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

**Use (1).** Option (2) is a trap: several inlines depend on the
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

**The exit gate fired a third time (2026-07-10, P3b-1 dispatch).** The JSC
adapter agent stopped without writing code, as J13 instructs: `core/`'s
`request_adapter_callback`/`request_device_callback`, on the A28
deferred-already-taken teardown path, dropped a Success-status non-null handle
without enqueuing its release. Reachable in the target architecture — the host
owns the `WGPUInstance`, it outlives the script runtime, and its next
`ProcessEvents` after a runtime drop fires the late callback with a live
handle. Silent native leak; the QuickJS suite never drives that sequence.
Fixed by planner-authorized handoff: both `None`-deferred arms now enqueue
non-null handles (callbacks stay pure Rust — enqueue only); two direct core
tests, both seen red first (`Ok(0)` against expected `Ok(1)`). Map/work-done
callbacks audited: they own no handles. Three gate firings, three core defects
caught before an adapter existed to hide them.

**A fourth firing, same day, same path.** The
re-dispatched P3b-1 agent refused the tree again: the authorized fix enqueued
the late handle into a queue whose **last `Arc` owner is the callback itself** —
`Runtime::drop` had already dropped every other owner — so the queued release
died un-drained the moment the callback returned. The leak had moved, not
closed. The regression tests accepted with it kept the queue alive and drained it
by hand, which hides that defect. Planner decision
(A8 amendment, 2026-07-10): on the post-teardown path only, the callback
releases the handle **directly** — the header exempts the `ProcessEvents`
callstack from the re-entrancy prohibition and `AllowProcessEvents` confines it
to the owning thread. Rejected alternatives, recorded: `ReleaseQueue::Drop`
self-drain (a JSC any-thread finalizer can be the last `Arc` owner via
`ArrayBufferOwner` — UB), and a host-owned queue (the right long-term shape,
but it is the open "who owns the GPU-release thread" question and needs a host
crate to answer it; this firing is evidence for it).

**Fix landed and verified (2026-07-10):** the `None`-deferred arms release
directly through the gpu dispatch with the A8-citing comment; the rewritten
tests drop the runtime for real, assert `Arc::strong_count == 1` on the
request's queue handle before firing the callback, and assert the release
counter with **no** hand-drain. Seen red against the enqueue version
(`left: 0, right: 1`, both tests). Gates re-run by the planner.

**P3b-1 landed (2026-07-10), reviewed and accepted with one carried finding.**
The adapter exists behind `jsc` (macOS-only, never default); eleven headless
tests pass from the tree (an rpath `build.rs` revision was needed — the JSC
test binary, like QuickJS's, must re-emit the backend rpath itself; the first
delivery passed only in the agent's hand-configured shell). Zero `core/`
changes — the exit gate did not fire on the adapter itself. The J2 ordering
test asserts `settle1,settle2,then1,then2` on the engine that can actually
fail it. Finalizers verified context-API-free (`JSObjectGetPrivate` only);
engine-value release routes through the deferred-unprotect queue.

**Carried finding (fix in P3b-2): method identity and per-call allocation.**
`wrapper_get_property` mints a fresh callable per property read, so
`device.createBuffer !== device.createBuffer` under JSC while QuickJS keeps a
stable function — a parity divergence J17's script would have to dodge — and
each read leaks a `MethodTarget` Box until context release, because F5 means
JSC will not finalize them sooner. Fix: cache the callable per (wrapper, name)
in the payload holder, protected, released through the deferred-unprotect
queue at finalize.

**The fifth firing.**
P3b-2's first parity run failed under JSC inside `unmap()`: `core/`'s
`CopyInCopyOut` copy-back requested the same mutable native range twice, and
`BufferMapping.md` makes overlapping non-const ranges fail — yawgpu is
conformant, the mock was not, and QuickJS's zero-copy arm never runs copy-back.
So the copy arm had been broken against every conformant backend since Phase 2,
visible only to the engine that actually uses it, in its first hour of use.
Fixed as **A32**: the native pointer is requested once, stored in the range
record, and copy-back writes through it; the mock now enforces the canonical
overlap rule, so the old shape fails a core test with no engine (seen red:
`a10_a20_copy_in_copy_out_detaches_and_copies_back`, "mapped range is
unavailable").

**P3b-2 landed (2026-07-10): the parity claim is now a passing test.** The same
`tests/parity/parity.js` produces byte-identical output under QuickJS and
JavaScriptCore (`tests/parity/expected.txt`, asserted by one test in each
adapter): label `null` → `"null"`, one-checkpoint tick ordering,
`mappedAtCreation` and `writeBuffer` byte round-trips, sequence conformance
both directions, BigInt rejection, `destroy()`. The carried method-identity
finding is fixed (per-wrapper cached callables, released through the
deferred-unprotect queue). J18's pinning red demo violates J9 in marked test
code and shows core's A12 verification firing. Bytes-pointer audit: staging,
transfer product, slice product — all private; the one script-visible use is
the marked demo.

**Phase 3 COMPLETE (2026-07-10).** Phase Review: 2 CRITICAL / 4 MAJOR / 9 MINOR,
all closed — full record in `phase-reviews.md` → "Phase 3 Phase Review". The
block's three open questions are answered in block 04 §6: the JSC scope is the
**root set** (PR3-C1), the any-thread finalizer premise is the header's own
documented contract, and the settlement trampoline costs ≈4 µs/tick. Zero core
logic changes for the adapter itself; five exit-gate firings, five core defects
landed before codegen.

---

## Q10 — Phase 4 slice 1: the parser decision (G9) and the first real join

**Landed 2026-07-10.** weedle2 5.0.0 parses the full pinned `webgpu.idl` with
one documented gap: **namespace `const` members** (the `GPU{Buffer,Texture,…}Usage`
flag namespaces) are unsupported; a thin pre-pass rewrites exactly those lines
(26, each surfaced verbatim in the CLI report) before parsing. 209 definitions,
zero remaining bytes; `[EnforceRange]`/`[SameObject]`/`[Exposed]` all reach the
model. New deps (fetched by the project owner): weedle2 5.0.0, serde 1.0.228,
serde_yaml 0.9.34, toml 1.1.2.

**The join of the pinned inputs (block 01–03 subset): 63 typed member pairs,
43 mismatches, 50 skips — and the mismatch list is the policy worklist:**

- `GPUBindGroupEntry.resource` (an IDL union) vs C's flattened
  `buffer/offset/size/sampler/textureView` — the union-flattening policy.
- IDL-only surface that is deliberately later or out of scope:
  `importExternalTexture`, `copyExternalImageToTexture`, `onuncapturederror`,
  `lost` (Phase 6 / out of scope) — policy skips with reasons.
- "IDL-only type" entries (`GPUProgrammableStage` ⋈ `WGPUComputeState`,
  `GPUShaderModuleDescriptor.code` ⋈ the WGSL chained struct,
  `GPUObjectDescriptorBase`/`GPUPipelineDescriptorBase` inlined by C) — name
  mapping is not always mechanical; these need explicit map entries (B3's
  chain policy included).
- C-only enum sentinels (`undefined`, `binding_not_used`) — ABI-only values,
  never script-visible.
- C-only functions (`GetConstMappedRange` = A29's second function,
  `Read/WriteMappedRange`, `HasFeature`, `GetLostFuture`, `WriteTimestamp`) —
  expected non-findings per G1.

Slice 2 turns this list into `policy.toml` entries and emits the first
generated conversion behind the G7 oracle.

**Phase 4 slice 2a landed (2026-07-10): the first generated conversion, behind
the G7 oracle.** `convert_buffer_descriptor` is now emitted by `codegen/` from
the joined model + `policy.toml` into `$OUT_DIR`, included by `core/`; the
hand-written implementation is deleted (grep: zero definitions in `core/src`),
and the generator itself contains no descriptor-name literal — the name flows
through as data, verified by grep. Every suite passed unchanged: core 61,
quickjs 41 (parity byte-identical), JSC 17, workspace green. The emitted code
cites its rule IDs (R8, B4, B7, DR-M3) at each guard, per G11. Policy gained
its first `[[descriptor]]` entry with both-directions enforcement (dead,
duplicate, disagreeing, and missing string policy all fail the run, each with
a test).

**Phase 4 slice 2b landed (2026-07-10): the hard constructs generate.** String
enums (IDL-listed values only; unknown → TypeError per B6; the C-only
`Undefined`/`BindingNotUsed` sentinels emitted solely for absent optionals),
nested dictionaries, `sequence<dict>` → arena count+pointer arrays, and IDL
dictionary-inheritance flattening. The bind-group-layout family
(`GPUBufferBindingLayout`, `GPUBindGroupLayoutEntry`,
`GPUBindGroupLayoutDescriptor` + referenced enums) is generated; hand-written
copies deleted. **One deliberate behavior change, decided by the planner after
the agent stopped on the contradiction:** a *present* `sampler`/`texture`/
`storageTexture`/`externalTexture` member — valid WebGPU this binding does not
support yet — now raises a TypeError naming the kind, instead of being
silently ignored into a wrong layout (invariant 8: clear early errors). The
hand-written code's silence was itself unrecorded; now both the behavior and
the decision are written down. Four new core rejection tests + one QuickJS
script test; every pre-existing test unchanged; core 66, quickjs 42, JSC 17,
parity byte-identical, all clippy/fmt green.

**Phase 4 slice 2c landed (2026-07-10): the whole block 01–03 descriptor
surface is generated.** Union flattening (`GPUBindGroupEntry.resource` →
buffer/offset/size with `WGPU_WHOLE_SIZE` for absent size), the WGSL chained
struct (B3, `sType` always set), handle sequences (`bindGroupLayouts`), the
`GPUProgrammableStage` ⋈ `WGPUComputeState` name-map, `layout: "auto"` → null
C handle, `timestampWrites`/`compilationHints`/`constants` policy-skipped with
reasons. Only two hand-written `convert_*` remain in core, both generic
non-descriptor machinery (the WebIDL sequence walker; command-buffer state
collection).

**G7 preserved five hand-written deviations as policy entries; all five were
retired.** `GPUBindGroupEntry.binding` optional-default-0 (IDL: `required` —
DR-M3's missed sibling); three `required sequence` members defaulting to empty
(`entries` ×2, `bindGroupLayouts`); and `entryPoint: null` treated as absent —
where the pinned IDL (`USVString entryPoint;`, line 685) refutes B4's own
prose, which was corrected in block 03. Each fix carries new member-named
TypeError tests (core + script level). Also deliberate, recorded: a present
non-buffer binding resource is now a clear TypeError instead of a silent
misread (2b's precedent applied to the union arm). Final: codegen 24, core 72,
quickjs 43, JSC 17+1, parity byte-identical both engines, workspace green,
clippys/fmt clean.

**Phase 4 slice 3 landed (2026-07-10): GPUSampler — the first interface with no
hand-written ancestor (block 05 exit criterion 5).** The descriptor (five
string enums, two restricted floats, one `[Clamp] unsigned short` — a NEW
attribute kind, verified at webgpu.idl:467, distinct from `[EnforceRange]`:
NaN→0, range-clamp, ties-to-even) is fully generated from policy. The
effort-delta datum for criterion 6: **hand-written plumbing was 217 added
lines** (payload+SAFETY, create fn with B16/R13 discipline, class spec, release
arm, dispatch fields, two 23-line adapter ABI thunks) **against zero lines of
conversion logic**. Parity extended by one line (`sampler:sampler-round-trip`),
byte-identical on both engines. Suites: codegen 26, core 79, quickjs 44,
JSC 17+1.

Consequence: descriptor conversion — invariant 1's "bulk of the work" — now costs
nothing per interface; the remaining per-interface cost is class/lifecycle
plumbing, which is mechanical and pattern-identical. The full-vs-subset decision
(criterion 6) and a class-spec-emission slice are the open items, then the Phase 4
review.

**Slice 4a landed (2026-07-10): G13, the dispatch triplicate removed.** The
generator now emits `GpuDispatch` and a `for_each_gpu_dispatch_entry!` macro
from `webgpu.yml`; each adapter's passthrough table and the mock's are one
macro invocation. Net deltas: quickjs −353, JSC −353, core −130, mock −44.
Adding an interface adds zero dispatch lines anywhere (G16's net-negative
demand met with room to spare). Five exceptional symbols are policy-listed
with reasons, enforced both directions. All suites unchanged-green; parity
byte-identical.

**Slice 4b landed (2026-07-10): G14/G15, lifecycle emission.** Seven
standard-pattern interfaces' payloads, create functions (R13/B16 + cleanup
symmetry), release variants, label accessors, and ALL fourteen class tables
are generated; the non-standard method bodies are policy-mapped with
both-directions enforcement. **B8 retention is derived from the joined model
and reproduced the hand-written sets exactly** (bind group = layout+buffers;
compute pipeline = module+nullable layout; others handle-only). Core shrank by
1,028 lines; the adapter diff is zero (G16). The 217-line per-interface datum
is now ~0 for standard-pattern interfaces: a new interface is a policy entry.
One run of this slice died at the 30-minute codex ceiling mid-flight; the
partial tree was verified green and a resume session finished it — the
split-heavy-tasks rule now has a codegen-sized data point.

**B22 closed (2026-07-10, owner-approved order item 3).** `writeBuffer` accepts
the full `AllowSharedBufferSource` (verified against the pinned IDL) — whole
`ArrayBuffer`s and `ArrayBufferView` windows — with **zero new `JsEngine`
primitives**: `.buffer`/`.byteOffset`/`.byteLength`/`BYTES_PER_ELEMENT` are
standard properties readable through `get_property`, so the JSC pinning hazard
is never approached (the backing buffer still flows through `arraybuffer_copy`'s
J19-safe slice path). The two optional args landed with the element-vs-byte
distinction, red-first proven (removing the multiplications: `left: [11,20,0,0]`
vs `right: [20,21,30,31]`). Bounds reject before narrowing (B7/A21). Parity
gains `writeBuffer view:8,5,3,0`, byte-identical both engines. Core 89.

**P6a landed (2026-07-10): error scopes, the GPUError classes, A9 retired.**
The first script-constructible classes in the binding (additive `ConstructorSpec`
slot on ClassSpec — QuickJS via the constructor-magic ABI, JSC via
`JSObjectMakeConstructor` with explicit prototype linking to avoid native-class
finalizer chains). `pushErrorScope` with a generated filter enum; `popErrorScope`
through the standard J1/J2 machinery (the header-verified `mode` field);
resolutions: null / typed GPUError instance / named `OperationError` rejection
carrying the backend message. Every shared async rejection reason is now an
Error object with name+message (A9's deferral closed; the one string-asserting
test updated, sanctioned). Parity gains `errorScope:GPUValidationError` — the
deterministic invalid op (OOB writeBuffer) verified against yawgpu's own
validation test, not assumed. Negative demo seen red (wrong class mapping →
ClassId mismatch). Suites: core 93, quickjs 48, JSC 19+1, parity byte-identical.
S6 (uncaptured, host-forwarded) and S7 (device.lost, header reading first)
remain for P6b.

**P6b landed (2026-07-10): S6 + S7.** `onuncapturederror` (writable attribute;
EventTarget absence recorded at the code site) and `device.lost` (cached
per-device promise, at-most-once, A28-clean teardown) both follow the
two-producers-one-queue shape: binding-created devices install pure-Rust
creation-time callbacks (the uncaptured one written for any-thread/any-time
delivery: copy the message inside the callback, enqueue, nothing else);
adopted devices get thread-safe `forward_uncaptured_error` /
`forward_device_lost`. Dispatch is tick step 2b — after the ONE batched
settlement, before microtasks — so A30's one-frame property holds and a
throwing handler exits through `TickError::Engine` like a throwing microtask.
Cross-thread forwarding is tested from a spawned thread. Parity gains
`lostReason:destroyed` via the forwarder (deterministic by construction).
Suites: core 97, quickjs 49, JSC 20+1. Both P6 slices ran ahead of the block's
own "review before P6b" sequencing — recorded deviation; one Phase 6 review
covers both.

Operational note: this slice hit the codex 30-minute ceiling again; the
timed-out session had in fact FINISHED the work, and the resume session's job
was pure audit + gates. The ceiling loses reports, not work — check the tree
before assuming loss.

**B15 narrowed (2026-07-10, Phase 6 / block 07 → S5) — superseding the older
passage above that said "when error scopes land, all nine must stop throwing".**
That sentence over-reached: error scopes have landed, and validation-failure
routing is the BACKEND's job (it routes into its own scope stack); the
binding's `createXxx` null-handle throws remain, narrowed to exactly the
catastrophic/misuse class (R13/B16). Nothing in the binding needed to stop
throwing, because a conformant backend does not return null for scope-routable
validation errors. The recorded deviation is now only: "a null handle from
createXxx is a synchronous exception", which is by design, permanently.

**Block 08 part 1 landed (2026-07-10): the parity suite grows 12 → 52 lines.**
Two real divergences found and excluded
pending triage: the SYNC operation-error throw path leaks the engine's native
error class name (`InternalError` under QuickJS via `JS_ThrowInternalError`,
plain `Error` under JSC via its error constructor) — S4 named async rejections
but never the synchronous throws. Triage: FIX — the `operation_error` contract
gains a stable `name: "OperationError"` on both adapters; the two lines then
land. Also pinned: yawgpu's nested-scope filter routing is deterministic
(validation crosses an inner oom scope to the outer validation scope,
identically on both engines); the P6 one-byte corruption proof re-demonstrated
(both adapters exit 101).

**Owner decisions (2026-07-10, JSC direction):** JSC will be enabled on iOS
regardless, so the in-process-JIT measurement is DEFERRED (perf claims stay
unwritten until measured; nothing currently depends on them). The **F9
deployment floor** (hard-linked `JSValueIsBigInt` → macOS 15 / iOS 18) is
queued to be fixed via **weak linking** — dyld resolves a weak import to NULL
instead of aborting at load, and the adapter branches to a BigInt-detection
fallback at runtime — **after** the parity suite has grown enough (block 08 is
the priority).

**Block 08 part 2 landed (2026-07-10): 60 lines; the sync error-name contract;
two more divergences confirmed verbatim.** `operation_error` now carries
`name: "OperationError"` on both engines (the native class leaked before:
InternalError vs Error); TypeError agreement pinned. Confirmed and triaged FIX
for part 3: (1) cross-instance method identity — QuickJS `true` (prototype
methods, WebIDL-correct), JSC `false` (the per-wrapper cache; prototypes
already shared) → JSC moves to per-class method objects; (2) lone-surrogate
USVString — QuickJS emits U+FFFD×3 (lossy WTF-8 bytes), JSC TRUNCATES at the
surrogate (data loss); the USVString answer is a single U+FFFD → JSC converts
via UTF-16 lossy, QuickJS gains a WTF-8 ill-sequence collapser. Landed safe
lines meanwhile: typeof function, shared prototypes, string-as-sequence
rejection identity, -0 → "0", 1e21 → "1e+21".

**Block 08 part 3 landed (2026-07-10): both divergences fixed at the root; 62
lines byte-identical.** (1) JSC method objects are now per-class (created once,
shared by every instance, protected in a dedicated ledger set released at
teardown; the per-wrapper cache and its deferred-unprotect plumbing removed;
the zero-forced-mop assertion keeps meaning via separate class-method
accounting) — `b1.mapAsync === b2.mapAsync` is now true on both engines, as
WebIDL says. (2) USVString conversion is spec-correct on both engines: JSC
converts through the UTF-16 view (`JSStringGetCharactersPtr` +
`from_utf16_lossy` — SDK-cited) instead of the truncating UTF8CString path;
QuickJS gains a WTF-8 sanitizer (quickjs.c's surrogate-preserving
`utf8_encode` verified at source) collapsing each ill-formed sequence to one
U+FFFD. `string:lone-surrogate:a\u{FFFD}b` (bytes 61 EF BF BD 62) pinned.
Suites: quickjs 54, JSC 21+1, core 104.

**Block 08 closed + F9 floor removed (2026-07-10).** The focused part-3
soundness pass returned zero CRITICAL/MAJOR (the CESU-8 hazard was ruled out
at source: quickjs merges surrogate pairs into 4-byte UTF-8 before encoding —
quickjs.c:4564 — so the sanitizer can never split an astral character); its
three MINORs (class-ref leak on an OOM path, a missing tearing_down guard, an
error built under the classes lock) are fixed. F9: `JSValueIsBigInt` is now a
runtime `dlsym` lookup with a retained `typeof`-helper fallback — both paths
exercised in the suite, `nm` confirms no hard link, the macOS 15 / iOS 18
deployment floor is gone. Block 08 exit criteria: all four met — 62 lines,
four divergences found and all fixed at the root, corruption fails both
adapters, prior suites unchanged.

**JSC promoted to Tier 1 on Apple platforms (2026-07-10, owner decision).**
`jsc` is a default feature (empty crate off Apple platforms, so the default is
free elsewhere); the JSC suite is now part of the plain workspace gate on
macOS; the adapter compiles for iOS (`cargo check --target aarch64-apple-ios`
verified) with runtime verification deferred to block 06. CLAUDE.md's tier
table, README, and the workflow gate table updated. The two-engine parity
story is explicitly verification-based: the block 08 suite (62 byte-identical
lines, growing with the surface) is load-bearing, macOS is the laboratory.
Standing cautions recorded in CLAUDE.md: F5 (destroy() is the only bounded
path) and the unmeasured iOS in-process performance (owner-deferred).

**Block 09 slice 1 landed (2026-07-10): textures.** The dict-or-sequence union
kind (T1) generates Extent3D/Origin3D with per-arm tests; GPUTexture and
GPUTextureView ride the generated lifecycle (readonly attributes through the
eight verified `wgpuTextureGet*` getters; view retains its texture per the B8
derivation). The GPUTextureFormat join: 101 IDL formats all joined, C-only
`undefined` sentinel policied — the enum joined without incident.
Parity 62 → 77 lines, byte-identical. Adapter diff:
zero. Suites: core 112, quickjs 54, JSC 23+1, codegen 40.

**Block 09 slice 2 landed: the bind-group resource arms complete (T4).** The
slice-2b "not supported yet" rejections for sampler/texture/storageTexture
convert to positive tests (externalTexture keeps its rejection, tested); the
resource union gains ClassSpec-driven sampler/view arms (no hard-coded names,
R24); derived retention grows to {layout, buffers[], samplers[], views[]} —
quoted before/after, matching expectation. Parity 77 → 80. Adapter production
diff zero (test-only, net −67 via a shared script).

**Block 09 slice 3 landed: createRenderPipeline (T5).** The biggest descriptor
in the API generated from policy: vertex state with NULLABLE buffer elements
(hole = attributes-empty + stepMode-Undefined, quoted from webgpu.yml:3444),
fragment targets with holes (format Undefined, yml:1931), depthStencil as the
plain nullable pointer (webgpu.h:4842) with `WGPUOptionalBool_Undefined` for
the IDL's default-less optional depthWriteEnabled, stencil faces, biases
(i32 + restricted f32), multisample, blend components. GPUVertexFormat joined
41/41 clean. Derived retention: vertex module + fragment module (null when
absent) + layout (null on "auto"), AddRef counts verified. Deferred with
policy reasons: getBindGroupLayout (needs a non-creator lifecycle for
pipeline-derived layouts — a real future design item), constants (standing).
Noop accepts a vertex-only pipeline only WITH a minimal depthStencil — pinned
that shape in parity (error scope resolves null). Parity 80 → 83.

**Block 09 slice 4 landed (2026-07-11): render passes and copies (T6/T7).**
Pinned TexelCopy names verified (`GPUTexelCopyBufferLayout/BufferInfo/
TextureInfo`); GPUColor rides the dict-or-sequence kind with f64 elements;
the render pass mirrors compute's state discipline exactly (parent-encoder
state only, use-after-end/double-end fail, no exclusivity because compute has
none). One IDL-vs-C mismatch recorded in codegen-deltas: attachments take
views only, a direct texture gets a transparent TypeError. Parity 83 → 90
(the block's exit target met). Suites: core 120, quickjs 53, JSC 23+1,
codegen 41.

**Block 09 COMPLETE (2026-07-11).** Review: 0 CRITICAL / 5 MAJOR / minors, all
fixed — full record in phase-reviews.md. Deletion lens: nine mutations, zero
survivors. Standing limitation recorded: JSC's F5 makes finalizer-driven
lifetime bugs structurally untestable under the iOS production engine —
lifetime coverage lives in core mocks and QuickJS; keep that in mind whenever
a retention bug class appears. Parity: 95 lines.

**Windows check (2026-07-11):** the MSVC Rust target is
installed on this macOS host, but `cargo check --target x86_64-pc-windows-msvc`
fails in ffi's build script — bindgen cannot resolve a Windows libc sysroot
(`math.h` not found) from macOS. Per block 03's own wording: no Windows run is
implied; a real Windows verification needs the Windows dev machine.

**Windows verification COMPLETE (2026-07-11, run by the owner's Windows-side
session).** The full suite passes on Windows/MSVC after `d82d689` (MSVC C11
flag spelling; a bindgen wrapper restoring the ui64-suffixed sentinel macros;
the earlier rpath/import-lib/gitattributes fixes). Both dev platforms now run
the binding.

**Block 10 landed (2026-07-11): introspection + the pipeline-derived layout.**
`construct` is the block's only trait addition (I1). `features` is a real,
sorted JS `Set` ([SameObject]-cached; mutability recorded as the I2 deviation;
C-only `subgroup_size_control` skipped through the join). `limits` introduced
the value-backed wrapper (hand-written by reasoned choice — a one-off
copied-struct lifecycle didn't justify generalizing the creator/release
generator), all 36 attributes + the compatibility chain. `adapterInfo` copies
its seven strings and frees immediately; FreeMembers pairings counter-asserted
(exactly 2+2, cache re-reads add zero). `getBindGroupLayout` is [NewObject]
per the pin, adopts the ReturnedWithOwnership handle with NO extra AddRef (per
the `device.queue` finding, B3-M1), and retains its pipeline per B8 because the
header guarantees nothing about independence. `isFallbackAdapter` derives from
`adapterType == CPU` (no direct C field — recorded). Parity 94 → 103,
byte-identical on yawgpu AND Dawn (gated run) — device-scoped features/limits
are backend-stable by construction (default-requested devices report core
features and spec-default limits). Suites: core 129, quickjs 54, JSC 25+1,
workspace 270.

**A-3 + A-5 landed (2026-07-11): GPUQuerySet and validity-observing parity.**
QuerySet rides the generated lifecycle (occlusion tested; timestamp untested —
**`requiredFeatures` is unplumbed in requestDevice**, recorded as a known gap;
timestampWrites stays policy-skipped). `occlusionQuerySet` + begin/end
occlusion query landed at validation level. The D11-retraction lesson applied:
parity's creation sections now run under validation scopes and log the pop
result, so "created ok" finally means VALID, not just non-null. Parity
103 → 116, byte-identical on yawgpu AND Dawn (gated run). Suites: core 130,
workspace 271.

**A-4 landed (2026-07-11): GPURenderBundle(Encoder).** Nullable colorFormats
elements (null → the Undefined hole), the mixin-derived method subset (no
viewport/scissor/blend-constant/occlusion on bundles, per the IDL's mixin
split), finish() mirroring the command encoder's state machine, and bundles
REUSABLE per the spec's own words ("without expiring after use like typical
command buffers") — unlike command buffers, no consumed flag. executeBundles
through convert_sequence, retention mirroring the pass's set* discipline
(none wrapper-side). Parity 116 → 119, byte-identical on yawgpu AND Dawn
(gated). Owner plan item A is COMPLETE: introspection, getBindGroupLayout,
querySet, renderBundle, and validity-observing parity all landed. Suites:
workspace 272.

**Correction (plan-A review, MINOR-7):** the block-10 entry above says
"adapterInfo copies its seven strings" — GPUAdapterInfo has seven ATTRIBUTES,
of which four are strings. The FreeMembers pairing claims are unaffected.

**Block 11 part 1 landed (2026-07-11): the compute example runs.** X2
(`register_host_function`, the HostValue v1 contract — String/Number/Bool/
Null/Undefined, everything else via ToString; host errors surface as catchable
JS exceptions per R26; tested on both engines) and X4 (`examples/compute`):
a host that creates the instance, registers `print`, and ticks while a JS
script requests the adapter and device, authors a WGSL doubling kernel,
dispatches, reads back through mapAsync, and prints. Gated run against Dawn
on real Metal: `result: 2, 4, 6, 8, 10, 12, 14, 16` — exit 0, via rpath alone
(the example needed the same rpath build.rs lesson as the adapters). The
triangle manifest is scaffolded; winit awaits the owner's fetch.

**Block 11 part 2 landed (2026-07-11): the windowed triangle runs.** X3
(`native_render_bundle`, class-checked on both adapters, handle-borrows-wrapper
documented and tested) and X5/X6/X7: a winit host that creates the surface
(CAMetalLayer via raw-window-metal), hands the preferred format to JS as a
string global, runs `triangle.js` once — the script builds the pipeline and
records a GPURenderBundle — then presents natively, frame after frame, with
JS never executing in the loop. Gated Dawn run on real Metal:
`rendered 120 frames`, exit 0. Both examples now demonstrate the
architecture's two halves: compute (JS authors and reads back through the
async machinery) and triangle (JS authors once; the host renders forever).
Gates: workspace 283, all exit 0.

**Android cross-compile VERIFIED (2026-07-11).** `cargo build -p quickjs-adapter
-p javascriptcore-adapter --target aarch64-linux-android` passes from the macOS
host — core + ffi (bindgen against the pinned header with the NDK sysroot via
`BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android`, the same load-bearing
variable yawgpu's recipe documents), quickjs-ng compiled by NDK clang (the
same three vendored sign-compare warnings as every other platform), and the
JSC adapter compiling to its intended empty crate off Apple platforms.
Toolchain: NDK 30.0.14904198 (the exact version yawgpu verified), target
aarch64-linux-android, API 24. Runtime verification on a device remains
block 06. The 32-bit `armv7-linux-androideabi` COMPILE check is available for
one `rustup target add` if wanted — A21's truncation guards are already
tested-by-construction on 64-bit hosts.

Platform status after today: macOS (both engines, tested) · Windows (QuickJS,
tested by the owner's session) · iOS (compiles) · Android (compiles).

**Block 12 COMPLETE (2026-07-12): the module loading system.** The ES-module
core (eval_module, alias map, TLA completing through tick — mechanics read
from the vendored quickjs source), the host transform hook (the TS enabler:
the binding ships no transpiler), and compiled-TS resolution probing landed
on main; the Three.js-specific work was reverted with its branch by owner
decision, and this system was extracted first. The close-out review found no
CRITICAL and one real MAJOR, empirically reproduced: the custom normalizer
kept `./`/`..` in module names, so equivalent import spellings loaded the
same file as SEPARATE instances (side effects twice, identity split) — the
exact hazard quickjs's own default normalizer lexically strips. Fixed with a
pure-lexical component walk (no filesystem canonicalization), the diamond
repro seen red then green; probe precedence pinned with conflicting fixtures,
the root transform-error message asserted, the origin map no longer
accumulates, and the rejection-matching caveat documented. QuickJS-first is
the recorded posture (JSC's C API has no module loader). Suites: quickjs 76,
workspace 302, all exit 0. The TS CTS runner's first brick is in place.

## Q11 — Can JavaScriptCore be given real ES modules? (block 16, Phase 1)

**Answered 2026-07-13, against the Xcode SDK on macOS. Decision: NO — candidate D
(build-time bundling). Trigger: D4 (the only path is non-public API).**

Written down *before* the implementation, per L13.

### L-Q1 — Does the public C API have any module entry point? **No. M4 confirmed.**

The JavaScriptCore framework's public C headers are `JSBase.h`, `JSContextRef.h`,
`JSObjectRef.h`, `JSStringRef.h`, `JSStringRefCF.h`, `JSTypedArray.h`,
`JSValueRef.h`. Searched exhaustively: **zero** `JS_EXPORT` declarations matching
`module` in any of them. `JSEvaluateScript` evaluates a *script*; nothing
evaluates a *module*. Block 12 → M4 was right about the fact, and this is the
citation it lacked.

### L-Q2 — What does the Objective-C API expose? **The bridge, and nothing to bridge to.**

| Symbol | Public header? | Note |
|---|---|---|
| `+[JSContext contextWithJSGlobalContextRef:]` | **YES** — `JSContext.h` | The C→ObjC bridge exists and is public. |
| `@property (readonly) JSGlobalContextRef` | **YES** — `JSContext.h` | The reverse direction, also public. |
| `-[JSContext evaluateScript:]` / `:withSourceURL:` | YES — `JSContext.h` | Evaluates a **script**. The only evaluation methods there are. |
| `JSScript` (the module-capable script object) | **NO** | Appears in the SDK **only as a bare forward declaration** — `@class JSScript, JSVirtualMachine, JSValue, JSContext;` (`JSContext.h:34`). **No `JSScript.h` exists in either SDK.** Nothing anywhere declares its interface. |
| `JSContext.moduleLoaderDelegate` | **NO** | Absent from every header in both SDKs. |
| `-[JSContext evaluateJSScript:]` | **NO** | Absent from every header in both SDKs. |
| `kJSScriptTypeModule` | **NO** | Absent from every header in both SDKs. |

Verified against **both** `xcrun --sdk macosx` and `xcrun --sdk iphoneos` — iOS is
the ship target, so a macOS-only answer would have been worthless.

The symbol nevertheless ships. The SDK's `JavaScriptCore.tbd` advertises exactly
five Objective-C classes — `JSContext`, `JSManagedValue`, `JSScript`, `JSValue`,
`JSVirtualMachine`. `JSScript` **links**. Its interface is simply not published.

That is the definition of **SPI**: the code is there, the API is not. Which means
the module API is reachable *only* through non-public API — and L6 settles that
without further argument: *"Private JavaScriptCore API is an App Store rejection
risk and is not a supportable foundation for a shipping iOS product. If a module
symbol is found that is not in a public SDK header, that is a finding to record,
not a path to take."*

### L-Q3 — The bridge spike: **not built.**

The block made this spike the decisive experiment, on the assumption that the ObjC
module API was public and the open question was whether the bridge preserved our
C-created custom global object (`JSGlobalContextCreate(global_object_class)` —
`imp.rs`:890).

The bridge is public; there is nothing public on the other side of it. A spike
would have to call SPI to reach a module at all, and whatever it returned, L6
forbids shipping it, so the experiment could not change the decision. L2 says the
null result is a first-class outcome and *"do not strain to make the ObjC path work
because 'implement' appeared in the assignment"*; L10 says the spike is throwaway,
not an end in itself.

**This is a deviation from the block's exit criterion 2**, which asked for the
spike to exist and run. Recording it as a deviation rather than quietly skipping
it: the criterion was written to force an unambiguous answer to "can a module see
the custom global", and the SDK answered a *prior* question — "can we make a
module at all, publicly" — with no. The prior question dominates.

### L-Q4, L-Q5, L-Q6, L-Q8 — **moot.**

The delegate's async model (L-Q4), the cost of an Objective-C dependency (L-Q5),
the OS floor it would impose (L-Q6), and its error channel (L-Q8) are all
questions *about candidate A*. Candidate A is dead on L-Q2, so they are
unanswerable and unnecessary.

Two consequences worth stating explicitly:

- **No OS floor is imposed by this block.** L19 (record the deployment floor)
  applies only if §6 ships. It did not. The project still has no recorded
  deployment target, and this block did not force one to exist — contrary to what
  §4's L-Q6 anticipated.
- **The JSC adapter stays pure C FFI.** No Objective-C dependency, no `objc2`
  crate, no new kind of dependency in the adapter (L3's last row), and no risk to
  the `aarch64-apple-ios` cross-compile that block 06 just established.

### L-Q7 — Dynamic `import()`: **measured under JSC, 2026-07-13 (block 16 → L22).**

*Correction: this entry originally asserted "out of reach" with no citation
(caught by the planner review, block 16 → §10, L22). It is now measured.*

Unlike an `import` **declaration**, a dynamic `import()` **call** is syntactically
legal in the Script goal, so what a bare `JSGlobalContextRef` does with one is an
*empirical* question the headers cannot answer. Run under our adapter's context
(a JSC-suite test now pins it):

- It does **not** throw synchronously.
- It returns a **genuine `Promise`**.
- That promise **rejects**, and the rejection callback runs before
  `JSEvaluateScript` returns — it does not hang.
- The rejection value is an `Error` whose message is exactly:
  `Could not import the module './x.js'.`

So `import()` is reachable and inert: JSC accepts the syntax and fails the load,
because there is no module loader to service it and no public way to install one.
The practical conclusion is unchanged.

`import.meta` is a **module-goal** construct and is not legal in a script at all,
so it is unreachable by syntax rather than by loader absence. Not separately
tested.

**Consequence for game code (unchanged):** under candidate D the game's code is a
single script on both engines. Runtime ES modules — including `import()` — remain
a **Boa-only, development-tooling** capability (the CTS runner uses them). Game
code must not rely on them.

### L-Q9 — Corroboration for candidate D.

React Native's long-standing iOS architecture is the precedent: Metro bundles a
multi-file JavaScript application into a **single script**, which is handed to
JavaScriptCore. A shipping product at scale has run exactly this shape for years.

Per L-Q9's own instruction, this is *corroboration only* — candidate D stands on
its own cost (zero new code, zero new dependencies, zero new OS floor) and would
have been viable regardless.

### The decision (L11 / L12)

**D4 fired: the only viable path to JSC modules is non-public API.** Candidate A
is therefore disqualified by the decision rule, without any of D1–D3 needing to be
tested. Candidate B was rejected in advance (L6). Candidate C is rejected
regardless of evidence (L7) — hand-rolled module semantics would give the two
engines *different* module behaviour and convert a visible gap into an invisible
seam, which is strictly worse.

**Candidate D — build-time bundling — is selected.** Both engines execute the
identical single script; parity is exact **by construction** rather than by
verification. Phase 2D (§7) implements it.

**The gap block 15 §8 surfaced is now closed as a decision, not as code:** Boa has
modules and JavaScriptCore has none, this is permanent for as long as the module
API stays SPI, and the answer for game code is that game code does not use runtime
modules on either engine.

### Q11 addendum 1 — L21: the bundle fixture's error path (2026-07-13)

The Phase 2D fixture cached a module before evaluating its factory and did not
remove the entry when the factory threw, so a second `__require` returned partial
exports. The golden pinned it (`bundle:throw:cached-partial:before-throw`). The
fixture's comment claimed the shape emitted by esbuild/Metro; that claim was not
verified.

Measured. `esbuild --bundle --format=cjs` over a two-module graph whose imported
module throws emits:

```js
var __esm = (fn, res, err) => function __init() {
  if (err) throw err[0];                       // permanently errored: re-throw the SAME error
  try {
    return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
  } catch (e) {
    throw err = [e], e;                        // memoise the ERROR
  }
};
```

Running that bundle: `FIRST → Error: bundled module failure`,
`SECOND → Error: bundled module failure`. Partial exports are never handed out.

§10 prescribed deleting the cache entry and re-running the factory. `__esm` does
not re-run: it memoises the error and re-throws the same object. Re-running would
execute the factory's side effects twice.

The fixture pins both properties independently:

```
bundle:throw:rethrown-same-error:true:Error:bundled module failure   ← same error, re-thrown
bundle:throw:evaluate-once:true:1                                    ← factory ran exactly ONCE
```

**Rule:** a claim about another system's behaviour requires running that system.

### Q11 addendum 2 — esbuild emits three different artifacts (2026-07-13)

Addendum 1 probed one input shape. Probing all of them showed the fixture modelled
a combination esbuild never emits.

Measured, `esbuild --bundle --format=cjs`, one probe per input shape:

| Input | What esbuild actually emits |
|---|---|
| **static ESM only** | **no registry at all** — modules hoisted and concatenated into one flat scope |
| ESM + a dynamic `import()` | the **`__esm`** helper (lazy init) |
| CommonJS input, or ESM importing a CJS dep | the **`__commonJS`** helper + `__require` |

And the two registries **disagree with each other** on the throw path:

```js
// __commonJS: resets `mod`, so the next require RE-RUNS the factory and throws a NEW error
catch (e) { throw mod = 0, e; }

// __esm: MEMOISES the error, re-throws the SAME error, factory never runs again
catch (e) { throw err = [e], e; }
```

Confirmed by running the emitted bundles: `__commonJS` → factory re-runs, second error
is a *different* object; `__esm` → same error re-thrown, factory ran once. (Node's own
`require()` matches `__commonJS`.) **Neither ever hands out partial exports** — the
original fixture was wrong against *both*.

§10's prescription (delete and re-run) is correct for `__commonJS`; addendum 1's
correction (memoise, do not re-run) is correct for `__esm`. The shipped fixture had
`__commonJS`'s structure with `__esm`'s error semantics — a combination esbuild does
not emit.

Block 16's premise is game code authored as ES modules and bundled. For static ESM
the artifact is a flat concatenation with no registry; the fixture had no such
section.

The fixture now models all three shapes:

- **flat concatenation** — hoisting, evaluation order, and a circular reference
  resolving to `undefined` through hoisting. Reproduces the measured trace exactly
  (`b saw a = undefined`).

  *(Corrected 2026-07-14 by the block 16 Phase Review. This entry originally said
  the flat section modelled "`let`/`const` TDZ across the flat scope". It does
  not, and it cannot — see the next section.)*
- **`__commonJS`** — cache-before-evaluate, partially-initialised exports on a
  circular require (correct for CommonJS), and a throw that **re-runs** the factory
  (counter reaches 2, two distinct errors).
- **`__esm`** — lazy init, memoised error, **same** error re-thrown, factory runs
  **exactly once**.

**Rule:** when the answer depends on the input, vary the input.

### Q11 addendum 3 — bundling erases top-level TDZ (2026-07-14)

Found by the Phase Review: the flat-ESM fixture section asserted that a flat bundle
preserves temporal-dead-zone semantics. It does not.

Measured (esbuild 0.28.1, `--bundle`; identical for `--format=cjs`, `esm`, `iife`): esbuild lowers **every** top-level `let`, `const` and `class` in a
bundled graph — in dependencies *and* in the entry — to `var`:

```js
// source:  export let letThing = 'let-ready';  export const constThing = 'const-ready';  export class C {}
// emitted: var letThing = "let-ready";         var constThing = "const-ready";           var C = class {};
```

Running the emitted bundle, a pre-initializer read prints `undefined undefined
undefined` and exits 0. Running the **same sources unbundled**, in the module goal,
throws `ReferenceError: Cannot access 'letThing' before initialization` and exits 1.

A flat bundle therefore has no top-level TDZ.

**Consequence.** Development against Boa's runtime module loader (module goal, real
TDZ) and the shipped flat bundle disagree: a cyclic read of a `const` export throws
in the former and yields `undefined` in the latter. This is not an engine divergence
— Boa and JSC agree on the bundle — but a development-vs-shipped divergence, which
candidate D introduces by construction and which the two-engine parity strategy does
not cover.

The parity golden pins both halves on one line:

```
bundle:flat:tdz-erasure:source-lexical=ReferenceError,ReferenceError,ReferenceError:shipped-bundle=undefined,undefined,undefined:initialized=let-ready,const-ready,function
```

The README records the constraint for authors: do not rely on TDZ or on
cyclic-initialisation errors; prefer acyclic module graphs.

## HV1 — Non-primitive `HostValue` fallback (decision deferred; block 15 → F13)

Both adapters' `host_value()` falls back to JavaScript `toString()` for any
non-primitive, so an object returned from `Runtime::call_global_function` becomes
`HostValue::String("[object Object]")`. F10 rejects thenables before this
conversion, which covers the case that matters for `frame()`.

Replacing the fallback with an error would also affect the **arguments** to every
registered host function: `print({})` would throw instead of receiving
`"[object Object]"`.

Options: retain string coercion for non-primitives; reject all non-primitives; or
split host-function argument conversion from call-in return conversion. No
behaviour changes until that API-wide choice is made.

## B10 — Boa overflows the platform default thread stack in debug builds (2026-07-14)

**Question.** A smoke run of the examples found `example-compute` aborting with
`STATUS_STACK_OVERFLOW` (`0xc00000fd`) on Windows. Logic bug, or stack size?

**Evidence.** Stack size, and the recursion is bounded:

| Run | Result |
|---|---|
| debug, Noop | `STATUS_STACK_OVERFLOW` |
| debug, Vulkan | `STATUS_STACK_OVERFLOW` |
| release, Vulkan | `result: 2, 4, 6, 8, 10, 12, 14, 16`, exit 0 |
| debug + `-C link-arg=/STACK:8388608`, Vulkan | `result: 2, 4, 6, 8, 10, 12, 14, 16`, exit 0 |

An 8 MiB stack completes the same work the 1 MiB stack died on, so the
recursion is deep, not infinite. Backend-independent. `example-triangle` and
`example-bounce` do **not** overflow in debug against Vulkan — their scripts do
not nest promises as deeply as `compute.js` does.

**Corrected claim.** Block 11's 2026-07-11 record — "default (Noop) compute
prints the un-doubled input and exits 0" — was measured under quickjs-ng and is
now false. Boa's interpreter recurses over the JS call graph; quickjs-ng's does
not. The engine swap (2026-07-12) gated the test suite, which passes; the
examples are in no gate (block 11 → X1) and were not re-run.

**Rule.** The host runs the engine on a thread whose stack size it chose. This
is a host obligation on every target, not a Windows dev-build artifact: iOS
secondary threads default to 512 KiB and Android native threads to 1 MiB. The
binding cannot do it for the host — the host owns its threads (invariant 6).
Written up as block 14 → B10 and block 11 → X12.

**Note on the shape of the fix.** A committed `.cargo/config.toml` carrying
`/STACK` is not available: `/.cargo/` is gitignored under the no-local-paths
rule, and it would fix only MSVC. Stack sizing is done in code.

## Handoff — examples/compute stack, and the release-queue clippy gate

```
## Task: examples + spikes — size the JS thread's stack; restore the clippy gate off macOS

Phase: maintenance (post block 15)
Goal: (1) example-compute survives a debug build on every platform;
      (2) `cargo clippy --workspace --all-targets -- -D warnings` is green off macOS.

Inputs to read:
- specs/blocks/11-examples.md  (X1, X4, X11, X12)
- specs/blocks/14-boa-engine.md  (B10)
- specs/reference/workflow.md, CLAUDE.md

Produce:
- examples/compute/src/main.rs
- examples/compute/README.md   (state the host obligation from X12)
- spikes/release-queue/src/lib.rs

Task 1 — examples/compute (block 11 → X12).
  Move ALL of create_instance() + run() + wgpuInstanceRelease() into a thread
  spawned with std::thread::Builder::new().stack_size(<N>), and have main()
  join it and map its bool to the ExitCode. Creating the instance inside the
  thread is the point: no WGPU handle then crosses a thread boundary, so NO
  `unsafe impl Send` and no usize-smuggling is needed (CLAUDE.md conventions
  forbid the latter outright). Name the stack size as a documented const with a
  comment citing B10. Do NOT change compute.js, and do NOT touch the windowed
  examples — they are main-thread-bound by winit and do not overflow.

Task 2 — spikes/release-queue (clippy).
  These items are constructed/called ONLY from #[cfg(target_os = "macos")] JSC
  code, so off macOS they are dead and -D warnings fails the gate:
    ReleaseLog::record_finalizer, ReleaseLog::record_panic,
    struct FinalizerPayload + its impl (incl. finalize),
    struct OrderingObservation + gc_count + total_finalizer_count (in tests).
  Gate them with #[cfg(target_os = "macos")] to match their only call sites.
  Do NOT silence this with #[allow(dead_code)]: CLAUDE.md treats a blanket
  allow as a silenced review, and the items genuinely are macOS-only.
  Do NOT change any spike behaviour or finding.

Out of scope: real GPU, native surface, core/ or adapter changes, spec edits,
commits, the windowed examples, compute.js.

Acceptance criteria:
- [ ] `cargo run -p example-compute` (DEBUG, no RUSTFLAGS) exits 0
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0
- [ ] no `unsafe impl Send`/`Sync` added; no handle smuggled as usize
- [ ] no #[allow] added on a correctness lint
- [ ] no local or sibling filesystem paths in any committed file

Report back: files changed, and the exit code of each of the three gates.
```

## K11 — Block 19 exit: what the bundle-swap contract required (2026-07-17)

Block 19 (`specs/blocks/19-skeleton-data-split.md`) evolved `examples/bounce`
into the smallest PoC of the skeleton/data split. This entry records what the
swap contract cost in `Runtime`-surface terms, for the future scene-graph
block. K7 held: zero diffs outside `examples/bounce/`, the cross-referenced
READMEs, plan §2.7's one sentence, and this file.

### (a) The swap contract, end to end

Script side (all existing primitives): a second pipeline created at init; at
the fixed frame, one `GPURenderBundleEncoder` + `finish()`, assignment to
`globalThis.bounceBundle`, `globalThis.bundleGeneration += 1`, and a call to
the registered host function `signalBundleSwap()`.

Host side: a `Rc<Cell<u64>>` signal counter incremented by the host function; a
swap point in `render()` after `frame()` returns and before surface-texture
acquisition; re-borrow via `eval("globalThis.bounceBundle")` +
`native_render_bundle()`; handle-inequality check (error without printing
pointer values); retention-global replacement
(`set_global_value("__hostBorrowedBounceBundle", value)`, which drops the
host's retention of the superseded wrapper); stored-handle swap. The superseded
native handle is never touched again — its release rides GC and the release
queue.

Verified: yawgpu Metal and Dawn both pass `--verify` (90 frames, one swap,
byte-identical golden incl. checkpoints `30 8 1 / 45 6 2 / 60 3 2 / 90 8 2`);
the corrupted-golden check exits 1. No yawgpu indirect-draw delta (K10's
contingency unused).

### (b) `Runtime`-surface ergonomics gaps hit

The same three the block-15 example surfaced; block 19 re-hit all of them and
added none:

1. **Eval-for-effect / value drop.** No method evaluates a script for effect
   and discards the result safely; the example routes every discard through a
   set-then-clear global pair (`eval_discard`, `__bounce_ready_probe`).
2. **Retention-while-borrowed.** `native_render_bundle()` returns a borrowed
   raw handle whose validity depends on the wrapper staying reachable, and
   nothing in the API ties the borrow to a retention; the host must maintain a
   JS global (`__hostBorrowedBounceBundle`) by convention.
3. **JS→host signalling is by registered function + globals.** The swap event
   needs a host function plus two script-owned globals; there is no typed
   event or channel surface. Adequate at this scale; the scene-graph block
   should decide whether it stays adequate at N bundles.

### (c) Cost observations

None taken. No timing was measured; nothing beyond the crossing counts stated
in the spec (K3) is claimed.
