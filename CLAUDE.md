# CLAUDE.md — webgpu-native-js permanent development rules

These rules are inherited/adapted from `yawgpu`'s conventions (which in turn
inherited from `mgpu`) and apply to all work in this repository.

**`specs/webgpu-native-js-project-plan.md` is a working draft, not a
contract.** Twelve of Rev 1's claims were checked against the real `webgpu.h`
and against `dawn.node`; three were wrong outright. The plan is now at Rev 2,
and its §7 records every correction with its evidence — **read §7 before
reasoning from anything the plan asserts**, and do not reintroduce a Rev 1
claim from memory.

This file holds only what is *invariant* — roles, boundaries, and conventions.
The plan holds design and phasing. When the plan and this file disagree, this
file wins; when evidence disagrees with either, fix both.

## Roles (read first)

Implementation is done by a **separate coding agent**. **Claude plans and
orchestrates** — it authors `specs/`, emits task handoffs, reviews the coding
agent's diffs against acceptance criteria, runs `cargo build`/`cargo test`,
and manages git (`init`/`add`/`commit`). Claude does not write production
code; the coding agent does not plan, edit `specs/`, change scope, or commit.
Full detail: `specs/reference/workflow.md` (to be authored, mirroring
yawgpu's).

## Scope boundary (read second)

- **JS is not the render hot path.** JS is the scripting/authoring layer:
  initialization, resource/pipeline definition, game logic. Per-frame draw call
  submission stays in the native host. Any proposal that puts JS in the frame
  loop is out of scope — this scoping is what makes the engine choice
  tractable.
- **Backend conformance is not this project's job.** Whether *yawgpu* correctly
  implements WebGPU is owned by
  [webgpu-native-cts](https://github.com/infosia/webgpu-native-cts), which links
  the `webgpu.h` C ABI directly with Dawn as the oracle. This project's job is
  whether the *JS binding* faithfully presents that C ABI as WebGPU-shaped JS.
  Those are different layers and need different oracles — see "Testing the
  binding layer".
- **The host owns the GPU.** In the target use case the native engine has
  already created the instance/adapter/device before any script runs. See
  invariant 6 below, and plan §2.8.

## Design invariants carried over from the plan's corrections

The plan's §7 has the full evidence. These are the conclusions that are now
**rules**, not proposals:

1. **`trait JsEngine` with associated types is the engine boundary.** There is
   no engine-agnostic `JsValueHandle` — Boa's `JsValue` is a Rust enum over a
   traced `Gc` pointer and is not even `Copy`, JSC's `JSValueRef` is an opaque
   GC pointer needing a context per operation. Descriptor conversion, which is the *bulk* of the
   work (not method dispatch), is written once in `core/` against the trait and
   monomorphized per engine. No `dyn` on the conversion path. This is the
   project's central design bet.
2. **Every JS-facing async op uses `WGPUCallbackMode_AllowProcessEvents`.**
   Callback threading is a contract the caller chooses, not a property to be
   discovered. `AllowSpontaneous` is forbidden on JS-facing paths — `webgpu.h`
   documents re-entrant API calls from such callbacks as undefined behaviour.
   **Device-lost is configurable too** and must use `AllowProcessEvents` like
   everything else: `WGPUDeviceLostCallbackInfo` has a `mode` field
   (`webgpu.h`, "Controls when the callback may be called"). **Only
   `WGPUUncapturedErrorCallbackInfo` has no mode** — the header warns it "may be
   called at any time (like `AllowSpontaneous`)", so calls into `webgpu.h` from
   it are unsafe and it alone must marshal to the JS thread. *(Corrected
   2026-07-09 by the Phase 0 review; the two callbacks had been conflated. See
   `specs/tracking/phase-reviews.md` → P0-C1.)*
3. **The host pumps two queues per frame, and this is public API.**
   `wgpuInstanceProcessEvents()` fires the WebGPU callbacks that resolve
   `Promise`s; the engine's microtask queue then runs the `.then()`
   continuations. Resolving a Promise does not run its callbacks. A binding
   that pumps only the first passes every test that avoids `await`. **Verified
   end-to-end** in `specs/tracking/event-loop.md`. `JS_ExecutePendingJob`
   returns `>0` / `0` / `<0`; the `<0` case must surface the exception, or a
   throwing `.then()` vanishes silently.
4. **Finalizers never call `webgpu.h` directly.** They push onto the release
   queue; a designated thread drains it. Reason (corrected 2026-07-09, evidence
   in `specs/tracking/release-queue.md` → Q1): `webgpu.h` *is* thread-safe —
   **but only "where multithreading is supported"**, and an implementation is
   explicitly allowed to confine every object except `WGPUInstance` to its
   creating thread, making off-thread use undefined behaviour. **Nothing in the
   API lets a caller ask which kind of implementation it has.** A JSC finalizer
   fires on an arbitrary GC thread, so calling `wgpuXxxRelease` from it is UB
   against a conformant backend, undetectably. `WGPUInstance`,
   `wgpuInstanceProcessEvents`, and the `destroy()` family *are* unconditionally
   thread-safe — which is what lets the pump thread be the drain thread, and the
   drain thread is the `tick()` thread (`release-queue.md` → Q3).
   **The queue is a plain FIFO and never sorts.** Ordering is made irrelevant
   instead: each child wrapper takes a *native* `wgpuXxxAddRef` on its parent
   handle and drops it with the child's release. Finalizer order differs by
   engine — Boa's tracing collector gives no ordering, and JSC gives none at
   all — so no design may depend on it (`release-queue.md` → Q2).
5. **Codegen input is WebIDL joined with `webgpu.yml`.** `webgpu.yml` describes
   the C ABI and carries no dictionary defaults, string enums, flag namespaces,
   `Promise` types, or `[EnforceRange]` coercion. `dawn.node` generates from
   WebIDL for exactly this reason.
6. **Handle adoption is the primary entry point.** `wrap_device(WGPUDevice)`,
   not `requestAdapter()`. The host owns the GPU before the script VM starts.
7. **GC is a backstop, not a resource-management strategy** — and **under JSC
   it is barely even that.** Measured (`release-queue.md` → R3): a JSC object
   whose only reference is dropped is **not** finalized by `JSGarbageCollect`,
   even after four calls; it is finalized at `JSGlobalContextRelease`. The
   public C API offers no other GC entry point and no synchronous collect. So a
   script that forgets `destroy()` may hold GPU memory until the context dies,
   and neither host nor binding can force the finalizer. `destroy()` is not
   "good practice" — under JSC it is the **only bounded path**. Say exactly that
   in user-facing docs. Corollary: **no test may provoke a JSC finalizer via
   GC.**
8. **Scripts are trusted.** First-party game logic, not a browser sandbox.
   Spend no effort hardening against adversarial JS; spend it on catching
   honest mistakes with clear, early errors.
9. **`getMappedRange()` never hands an engine a pointer it cannot revoke.**
   No supported engine can hand out a revocable pointer over GPU memory. JSC's
   public C API has no ArrayBuffer detach (evidence:
   `specs/tracking/engine-boundary.md` → Q1), so a zero-copy view would leave
   script holding a dangling pointer after `unmap()`; Boa owns its ArrayBuffer
   allocation and cannot wrap external memory. Mapped ranges therefore always
   copy into a script-visible ArrayBuffer, and write mappings copy back at
   `unmap()`. This is spec-conformant (WebGPU defines mapped contents as becoming
   visible to the GPU at `unmap()`), so it is a performance property, not a
   behavioural one.
10. **Under JSC, never take the C bytes pointer of a buffer script can see.**
    `JSObjectGetArrayBufferBytesPtr` and `JSObjectGetTypedArrayBytesPtr` invoke
    WebKit's `pinAndLock()`: the buffer becomes permanently non-detachable, and
    a later `transfer()` **silently succeeds without detaching**. The obvious
    `CopyInCopyOut` implementation therefore leaves script holding a live buffer
    after `unmap()` with no error raised anywhere. Copy through a private
    staging buffer and detach via `transfer()` *before* touching any pointer —
    protocol in `engine-boundary.md` → Q1b. Treat any C pointer taken from a
    script-reachable buffer as a CRITICAL finding. (Only the two bytes-pointer
    accessors pin; `JSObjectGetArrayBufferByteLength` is safe — E12.)
11. **JSC does not report a failed detach; Boa does.** JSC's `transfer()`
    silently no-ops on a pinned buffer. Boa's
    `JsArrayBuffer::detach() -> JsResult<AlignedVec<u8>>` reports failure
    natively — the first engine here that does. Because one engine still lies,
    `unmap()` must **verify** detachment and raise a hard error when it did not
    happen, and that check stays in `core/` once, not in each adapter — it is
    shared behaviour, not engine-specific defensive coding. Do not delete it
    just because the newer engine is honest.

## Engine support tiers

| Tier | Engines | Meaning |
|---|---|---|
| **Tier 1 — Supported, all platforms** | [Boa](https://github.com/boa-dev/boa) (MIT/Unlicense), pure Rust, crates.io exact pin | **Adopted 2026-07-12 (owner decision), replacing quickjs-ng.** The cross-platform engine. Pure Rust: no C toolchain, no engine `bindgen`, and cross-compilation is an ordinary `cargo build --target`. Rationale and evidence: `specs/blocks/14-boa-engine.md`. |
| **Tier 1 — Supported, Apple platforms** | JavaScriptCore (system framework, LGPL-2.1, dynamic link) | **Promoted 2026-07-10 (owner decision).** `jsc` is a **default** feature; the adapter compiles to an empty crate off Apple platforms, so the default costs nothing elsewhere. macOS is fully tested (the JSC suite is part of the standard workspace gate); iOS compiles (`cargo check` verified) with runtime verification deferred to mobile bring-up (block 06). Android and Windows are unsupported by Apple. Using the system framework removes the App Store 4.7 bundled-engine question entirely. |

**What Tier 1 for two engines means:** iOS(JSC)↔Android(Boa) parity is
guaranteed by **verification, not by sharing an engine** — the parity suite
(`specs/blocks/08-parity-suite.md`; one script, byte-identical output, both
engines, every test run) is load-bearing and grows with the API surface.
macOS is the parity laboratory: the one platform where both engines run side
by side. Two standing JSC cautions remain (both recorded and user-documented):
finalizers effectively never run before context teardown, so `destroy()` is
the only bounded release path; and performance claims for in-process JSC on
iOS stay unwritten until measured (owner-deferred).

**Operational rule (engine-independent core).** `core/` must contain **zero**
references to Boa or JSC types — only `E: JsEngine`. Validation and
lifetime rules must behave identically under both engines. If wiring an engine
adapter forces a change to `core/`'s *logic* (as opposed to adding a method to
the `JsEngine` trait), that is a signal the boundary was drawn incorrectly —
**stop and revisit; never widen `core/` to make an engine fit.**

**Node.js / N-API remains out of scope** as a runtime target. It may be
revisited later purely as a desktop tooling/editor target.

## Backend support tiers

| Tier | Backends | Meaning |
|---|---|---|
| **Tier 1 — Supported** | yawgpu | Primary development and CI backend (the Noop backend runs every gate headless). |
| **Oracle** | Dawn | **Promoted from Experimental 2026-07-12 (owner decision)** — the reference arbiter, not a support tier. Dawn passes both engines' full suites with byte-identical parity, is the conformance oracle of `webgpu-native-cts` itself, and — decisively — **our `webgpu-headers` pin IS Dawn's `DEPS` pin**, which is what makes ABI-identical arbitration valid. See "The oracle protocol" below. |
| **Tier 2 — Experimental (best-effort)** | wgpu-native | Selected by Cargo feature. Must link and pass the vertical slice; divergences from canonical `webgpu.h` are catalogued, not worked around. |

**The oracle protocol.**

- **Presumption, not axiom:** when the binding disagrees with Dawn, the
  presumption is a binding bug — but the D11 lesson stands: isolate WHERE the
  paths diverge before assigning blame (D11's "divergence" was an
  observability gap on our side, not either backend). When Dawn disagrees
  with the pinned header or spec text, **the pins win** and the disagreement
  is investigated and recorded, never blindly followed.
- **Pin lockstep:** the oracle status rests on the shared header pin. When
  the `webgpu-headers` pin moves, it follows Dawn's `DEPS` (the standing
  policy), and the local Dawn build must match before oracle runs resume.
- **When oracle runs happen:** gated real-GPU runs (never CI): every slice
  that extends the parity suite or the API surface gets a gated Dawn parity
  run before it lands (the de-facto practice, now the rule), and the CTS
  runner's real-GPU phase uses Dawn as its arbiter (fail-on-Dawn = presumed
  binding bug; fail-on-yawgpu-only = backend delta, catalogued).
- yawgpu-vs-Dawn disagreements are yawgpu findings, handled by the owner's
  handoff flow — never worked around in the binding.

**Operational rule (backend-independent core).** `core/`, `codegen/`, and the
engine adapters must contain **zero** backend-specific branches. All GPU calls
cross the `webgpu.h` C ABI. A backend divergence is fixed **upstream in that
backend**, or documented in `specs/tracking/backend-deltas.md` — never papered
over with a `cfg(backend)` check above the FFI layer.

## Target platforms

| Tier | Platforms | Meaning |
|---|---|---|
| **Production (execution)** | iOS, Android | Ship targets. |
| **Development / testing** | Windows, macOS | Dev targets. |

**Behavioral parity across all four is a first-class concern**, because
dev/test results on Windows/macOS are only useful if they predict behavior on
iOS/Android. This is the entire reason a JIT-less engine was chosen — verify
the parity actually holds rather than assuming it.

## Language

- **All repository documentation, specs, comments, and identifiers: English.**
- Conversation with the user (chat responses): Japanese.

## Core principles

1. **Every public API has a direct unit test.** Any `pub fn` in `core`, `ffi`,
   `codegen`, or the engine adapters must have an inline `#[cfg(test)] mod
   tests` test that exercises it directly (happy path + error / edge cases as
   relevant). New public API ships in the same commit as its unit test.
   JS-visible behaviour additionally gets a script-level test per engine.
2. **All GPU calls cross the `webgpu.h` C ABI.** Never bind to yawgpu's
   internal Rust API, even though it is the same language. Bindings are
   `bindgen`-generated from canonical
   [`webgpu-headers`](https://github.com/webgpu-native/webgpu-headers). This is
   what preserves backend-swappability with Dawn (C++) and wgpu-native. A
   convenience shortcut through yawgpu's Rust internals is a design violation,
   not an optimization.
3. **The engine boundary is `trait JsEngine` with associated types, and it is
   the one abstraction that must not leak.** No opaque `JsValueHandle`, no
   `dyn` in the conversion path. Descriptor conversion is written once in
   `core/` against the trait (invariant 1).
4. **Finalizers never call `webgpu.h` directly.** They push onto the release
   queue; the `tick()` thread drains it. Reason: `webgpu.h` is thread-safe
   **only "where multithreading is supported"**, an implementation may confine
   every object but `WGPUInstance` to its creating thread, and **nothing in the
   API lets you ask which you have**. The queue is a **plain FIFO and never
   sorts**; child-before-parent is made irrelevant by each child wrapper taking
   a native `wgpuXxxAddRef` on its parent handle. Evidence and the full
   correction: `specs/tracking/release-queue.md` → Q1, Q2, Q3. See also design
   invariant 4, which this principle must always agree with.
5. **GC is a backstop, not a resource-management strategy.** WebGPU has
   explicit `destroy()` on buffers, textures, and devices for a reason. On
   mobile, waiting for a finalizer to free GPU memory is a bug. Scripts are
   expected to call `destroy()`; the finalizer exists so that forgetting is a
   leak-until-GC, not a leak-forever. Say this in the user-facing docs.
6. **Every JS-facing async op uses `WGPUCallbackMode_AllowProcessEvents`, and
   the host pumps both queues per frame** (invariants 2 and 3).
   `AllowSpontaneous` is forbidden on JS-facing paths.
7. **Headless-first.** Every unit and validation test must pass with **no GPU
   and no window**, against yawgpu's Noop backend or a compute/offscreen path.
   Real-GPU and native-surface work is gated and never required for CI.
8. **No panics in library code.** `core` and the adapters return `Result`; use
   `?`. The single exception is the **FFI boundary**, where invalid C handles /
   null where the spec forbids null may `expect(...)` (mirrors wgpu-native).
   Spec-level validation failures route to the device error sink or a rejected
   JS `Promise`. **A panic must never unwind across the JS engine's C
   boundary** — every `extern "C"` callback catches.
9. **Generated code is never hand-edited.** `bindgen` output and codegen output
   are build artifacts. Fix the generator, not its output.
10. **Scripts are trusted.** This is first-party game logic, not a browser
    sandbox. Do not spend effort hardening against adversarial JS; do spend it
    on catching honest mistakes with clear errors.

## Testing the binding layer

`webgpu-native-cts` validates the *backend* against Dawn. It cannot validate
this project, because the bug class here is "the JS binding mis-converts a
descriptor", which never reaches the C ABI in a distinguishable way.

The binding layer's natural oracle is the **upstream WebGPU CTS itself**, which
is written in TypeScript and therefore runs *in* the engine under test. This is
exactly what `dawn.node` does (`src/dawn/node/cts.cjs`, `tools/src/cmd/run-cts`)
— it is the same trick, one engine down. Running it under Boa needs a
module loader and some Web shims, which is a real lift and explicitly **not**
Phase 0–3 scope. But it is the end state worth steering toward, and it is why
invariant 5 (generate from WebIDL) matters: only an IDL-faithful binding can
pass an IDL-derived suite.

Until then: per-conversion unit tests in `core/`, plus one `.js` conformance
script executed under **both** engines with identical expected output.

## Code conventions

- `#[non_exhaustive]` on extensible public enums/structs.
- `#[must_use]` on builders and handle-producing fns.
- Every public item carries a `///` doc comment. Enforced by
  `#![warn(missing_docs)]` at each crate root and escalated by the `-D warnings`
  clippy gate. Generated bindings are exempt via `#[allow(missing_docs)]`.
- Engine dispatch is **static** (`E: JsEngine` monomorphization), never `dyn`.
- C↔Rust conversions live in `ffi/src/conv.rs` (macro-driven, like
  wgpu-native's `conv.rs`). JS↔Rust conversions live in `core/`, generic over
  `E`.
- bindgen output is `include!`d into a `pub mod native { ... }`; never edit
  generated code.
- **Every `unsafe impl Send`/`Sync` carries a `// SAFETY:` comment, and a Phase
  Review greps for the ones that do not.** They are *necessary* here: a payload
  must be `Send` because a JSC finalizer may run on any thread, and a
  `WGPUxxx` handle is a raw pointer. The justification is that handles are
  **moved** across threads and never **dereferenced** off the `tick()` thread —
  `webgpu.h` allows an implementation to make off-thread *use* undefined
  (`release-queue.md` → Q1), and moving is not use. Write that at each impl.
  Phase 2 shipped four of them with no comment; three reviewers and every gate
  walked past. **Never smuggle a handle as `usize` to dodge `Send`** — that
  discards the type and does not remove the obligation (block 01 → R18).
- **`#[allow]` on a correctness or soundness lint is a silenced review.** It
  needs a `// SAFETY`-style comment saying why the lint is wrong *here*, and it
  is a Phase Review finding without one. Phase 1 shipped
  `#[allow(clippy::not_unsafe_ptr_arg_deref)]` on a `pub fn` taking a raw
  pointer; the gate was green, and three independent reviewers walked past it.
  Blanket `#[allow]` on a generated-bindings module is the one exemption.
- Colocate each object's binding with its own module, not one giant `spec.rs`
  (mgpu/yawgpu convention).

## Workflow per API area

1. Write/extend `specs/blocks/<area>.md` — the new public API + its behaviour
   contract.
2. Write the **inline unit test** (Red).
3. Implement (Green).
4. Add a script-level test when the API crosses into JS-visible behaviour the
   unit test cannot reach.
5. Verify headless under **both** engines where the area touches the adapter
   boundary; log in `specs/tracking/<topic>.md`. No per-phase logs.
6. Refactor for reuse/clarity before moving on.

**Every phase ends with a mandatory Phase Review ("Clean Review Then Fix"):** a
fresh no-context subagent reviews the phase's cumulative diff and emits
`CRITICAL`/`MAJOR`/`MINOR` findings; findings are fixed in severity order; a
phase cannot be COMPLETE with any open CRITICAL/MAJOR.

**The JSC phase carries an extra exit gate.** Wiring it must require zero
changes to `core/` logic — only additive `JsEngine` trait methods. Non-trivial
core churn means principle 3 was violated; revisit the boundary before scaling
up codegen, do not absorb the churn.

## Open design questions

Genuinely undecided. Answer with evidence; do not let the draft plan's guesses
harden into assumptions.

- **Who owns the GPU-release thread** — the host engine or this project?
- ~~**Which quickjs-ng revision is pinned?**~~ **MOOT (2026-07-12):** quickjs-ng
  was dropped for Boa (`specs/blocks/14-boa-engine.md`). Boa is pinned exactly
  from crates.io.
- ~~**Should `ZeroCopyDetach` be deleted?**~~ **ANSWERED (2026-07-12, owner):
  deleted.** Both shipping engines use copy-in/copy-out (invariant 9).
  *(Corrected 2026-07-12: an earlier version of
  this entry claimed the path was "untested by construction". That was wrong —
  the mock engine's default is the zero-copy strategy and a named test pins it.
  The claim was made without checking `core/src/mock.rs`.)* The owner accepted
  the future cost of re-adding an abstraction for a zero-copy-capable engine in
  exchange for deleting an `unsafe` trait method no shipping engine implemented,
  its adapter stubs, and the strategy branch in `core/`.
- **Full WebIDL coverage vs. a trimmed engine-oriented subset.** Revisit after
  the first codegen pass shows the real effort delta.
- ~~**Where does `webgpu.idl` come from**, and how is it pinned against the
  `webgpu.h` version? (Plan §6.4.)~~ **ANSWERED (2026-07-10, block 05 → G1):**
  the gpuweb/gpuweb repository, pinned as `third_party/gpuweb` at the revision
  Dawn's `DEPS` pins for `dawn_node` — the same follow-Dawn policy as
  `webgpu-headers`. On IDL-vs-header conflict the header wins; skips are
  catalogued, never approximated.
- **App Store Review Guidelines** re: bundled custom JS engines (4.7 tightened
  Nov 2025, aimed at remotely-delivered "mini app" content, not engine-bundled
  scripting). Precedent for bundling is strong. **Re-verify immediately before
  any iOS release.**

## Out of scope (initially)

- **Backend conformance testing.** Owned by `webgpu-native-cts`.
- **JS in the render hot path.** Permanently out of scope.
- **Node.js / N-API as a runtime target.** Possibly revisited as desktop tooling.
- **JavaScriptCore on Android and Windows** (plan §3.2).
- **Multithreaded script execution** (multiple `JSRuntime`/`JSContextGroup`).
  One engine instance per game instance; revisit only on a concrete requirement.
- **Native surface / windowing** — deferred. Early phases proceed against
  compute/offscreen only, so windowing never blocks core work.
- **Running the upstream TS CTS** — the end state, not near-term scope.

## Privacy / repo hygiene

- No credentials, signing material, or device-specific secrets committed.
- `.gitignore`: `target/`, `.claude/`, local test transcripts.
- Generated bindings are build artifacts (`$OUT_DIR`), not committed.

### No local or sibling paths in committed files

**Nothing committed to this repository may reference a path outside it.** This
applies to every tracked file — docs, specs, comments, code, tests, `build.rs`,
CI config, and commit messages.

Forbidden:

- Absolute paths into a developer's filesystem — any home directory, user
  profile, or drive-letter root.
- Any relative path that escapes the repository root (a leading parent-directory
  traversal), including one naming a sibling checkout.
- Machine- or user-specific names: local usernames, hostnames, workspace
  directory names, IDE workspace files.

Required instead:

- **Cite external projects by upstream URL and by their own repo-relative
  path**, never by where they happen to sit on someone's disk. Name the project,
  link its repository, and give the path *as that repository sees it*.
- **Pin external sources as git submodules or fetched artifacts**, resolved by
  `build.rs` / env var (e.g. `WEBGPU_HEADERS_DIR`), with a documented default.
  A build must never assume a sibling checkout exists.
- Paths *within* this repository are repo-relative and are fine
  (`core/convert/`, `specs/blocks/<area>.md`).

**Why:** such a reference makes the repo build only on the machine that wrote
it, silently couples this project's directory layout to another project's, and
leaks the author's filesystem structure. It also quietly undoes principle 2: the
`webgpu.h`-only rule exists to keep the backend swappable, and a filesystem path
to a backend checkout in `build.rs` re-couples them through the filesystem.

This rule is checked at review time. When Claude verifies a claim against a
local checkout of another project, that is a **tool-use detail**: record the
*finding* and the upstream citation, never the local path used to reach it.

## Tooling — sandbox

- **Avoid `dangerouslyDisableSandbox: true` whenever possible.** Prefer
  sandboxed Bash commands. Only disable when there is no alternative — e.g.
  real-GPU or device/simulator runs, or an operation already shown to fail under
  the sandbox in this session. Network ops (`git push`/`pull`, submodule
  fetches) should be invoked by the user via the `!` prompt, not run by Claude
  with the sandbox disabled.
