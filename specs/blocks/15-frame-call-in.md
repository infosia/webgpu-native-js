# Block 15 — the per-frame call-in

**Status: COMPLETE (2026-07-14). Phase Review clean of CRITICAL and MAJOR.**

F12 held: zero methods were added to `trait JsEngine` (49 items before and after).
The contract composes from `global`, `get_property`, `is_callable`, `is_object`,
`call` and `drain_microtasks`.

Phase Review findings, all closed: F18 (the plan's §1.3 and §2.7 were not updated —
§1.3 is the paragraph `examples/triangle` read as "JS never runs during the frame
loop"); the F14 parity artefacts had been committed into block 16's commit, leaving
two commits on `main` whose parity test failed (history corrected); three golden
lines were Rust-supplied literals and observed nothing (the variant is now derived
from the returned `FrameError`); the pump-failure release drain was untested.

F13's deferred decision is recorded as `specs/tracking/engine-boundary.md` → HV1.

**F6 verified on two real backends:** `cargo run -p example-bounce -- --verify`
matches the golden and exits 0 on **Dawn** and on **yawgpu with
`YAWGPU_BACKEND=metal`** (60 frames presented, state matched). Corrupting one
golden line gives exit 1, so the golden gates.

**Correction to the bounce commit message (`6ac798b`).** Its "Backend note"
claimed the examples fail to link against yawgpu because the Metal build predates
the `wgpuXxxSetLabel` symbols. That is false. yawgpu's default `target/release`
build carries those symbols and runs the example on Metal. The failure observed
came from pointing `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` at a stale `target-metal/`
*variant* directory, not at the default build. No backend work is required.

Rules are numbered **F1–F18**. Blocks 01 (R1–R27), 02 (A1–A32), 03 (B1–B22),
08 (P1–P8), and 11 (X1–X10) still bind.

Every claim below about the existing code was checked against `main` while
writing. Re-open the files; do not restate from memory.

---

## 1. The question this block exists to answer

The project's stated purpose is *"JavaScript as a scripting/authoring layer
inside native game engines"* (plan §1.3), and the scoping invariant is *"JS is
not the render hot path."* Both are true. But the repository today only
demonstrates the **negative** half:

- `examples/triangle` records a `GPURenderBundle` at init and its README says
  *"JS never runs during the frame loop"* (X5). Its `Renderer::render` calls no
  `tick()` at all — the claim is implemented literally.
- There is **no way for a host to call into JS at all.** `Runtime` exposes
  `eval`, `set_global_value`, `clear_global`, and `register_host_function`
  (JS → host). Nothing goes host → JS. `call_global_function` is referenced by
  block 13 as *"Per-frame / call-in"* but it lives only on the deleted Three.js
  branch (commit `6cd9d96`, never cherry-picked) and **is not on `main`.**

So the product's central promise — *write game logic in JS* — has no entry point
and no example. A reader of this repository would reasonably conclude that JS
does nothing after initialization.

> **The question:** what is the exact per-frame contract between the host and
> the script, such that JS owns game logic while the number of JS↔native
> crossings stays independent of the draw count?

The answer is one new host API and one new example. The example comes first in
this document because **the example decides the contract's shape**, not the
other way round (owner directive, 2026-07-13).

**"Not in the hot path" does not mean "does not run per frame."** It means the
crossing count does not scale with the draw count. One `update(dt)` call per
frame is O(1); a `pass.draw()` per object is O(objects). This block writes that
distinction down as executable code, because the phrasing in plan §1.3 has
already been read the stronger way once — by `examples/triangle`.

---

## 2. Scope

**In.** `Runtime::call_global_function`; `Runtime::frame`; the six-step frame
order in `core/`; `examples/bounce`; the parity-suite section that pins the
simulation's cross-engine bit-exactness; the X5 README cross-reference.

**Out.** Input handling (keyboard/mouse/touch into JS) — a separate host API and
a separate block. Fixed-timestep accumulators and interpolation — the host's
business, not the binding's; `frame()` takes whatever `dt` the host hands it.
Any change to `tick()`'s existing four-step behaviour or its callers. Codegen.
Anything that would let JS issue draw calls per frame.

---

## 3. The example (`examples/bounce`)

### 3.1 Why bounce and not a spinning cube

**F1 — the example must do something a shader cannot do for it.** A rotating
cube's `update()` computes `mvp = perspective * rotateY(t)` and writes it. A
vertex shader given a `time` uniform computes the same thing with no JS at all,
so such an example demonstrates the *plumbing* and proves nothing about the
*scoping invariant*. Bouncing bodies hold state across frames and **branch**
(`if (x < -1 || x > 1) vx = -vx`); a shader cannot hold that state, and the
branch is what makes it recognisably game logic rather than animation.

**F2 — the simulation uses only `+`, `-`, `*`, and comparison on f64, and no
transcendentals.** These are IEEE-754-exact and therefore **bit-identical
between Boa and JavaScriptCore**, which is what lets F14 put the simulation in
the parity suite with an exact golden. `Math.sin`/`Math.cos` have
implementation-defined precision, so a cube example would force the parity
comparison down to a tolerance and weaken the instrument. No `Math.random`, no
`Date.now`, no `performance.now` anywhere in the simulation.

### 3.2 What it does

**F3 — one draw call, N bodies, one crossing per frame.** `N` is a `const` in
the JS (start at 8; it is a knob, see F13).

At init, `bounce.js`:

1. creates a shader module whose vertex stage reads a **read-only storage
   buffer** of per-body records (`var<storage, read>`; read-only storage in a
   vertex stage is permitted by WebGPU — if a backend rejects it, fall back to a
   uniform array and record the delta in `specs/tracking/backend-deltas.md`, do
   not work around it in the binding);
2. creates that storage buffer with `STORAGE | COPY_DST`, the bind group layout,
   the bind group, and the render pipeline;
3. records **one** `GPURenderBundle`: `setPipeline`, `setBindGroup(0, bg)`,
   `draw(6, N)` — an instanced quad;
4. allocates the staging `Float32Array` **once** and keeps it;
5. sets `globalThis.update` and then `globalThis.ready = true`.

Per frame, `globalThis.update(dt)`:

1. integrates each body (`x += vx * dt`), reflects velocity at the walls;
2. writes the reused `Float32Array` into the storage buffer with **one**
   `queue.writeBuffer` call;
3. returns `undefined`.

Per frame, the host:

1. `runtime.frame(instance, "update", &[HostValue::Number(dt)])?`;
2. acquires the surface texture, begins a render pass, calls
   `wgpuRenderPassEncoderExecuteBundles` with the **same bundle recorded at
   init**, ends, submits, presents.

**F4 — no allocation inside `update()`.** The `Float32Array` is created at init
and rewritten in place; the body records are a fixed array of objects created at
init and mutated. Under a JIT-less engine GC pressure is the real per-frame cost,
not the arithmetic, and the example is the place to show the discipline rather
than describe it. A comment in `bounce.js` says exactly this in one line.

**F5 — the host never touches the instance buffer, and never re-records the
bundle.** The commands submitted are byte-for-byte identical every frame; only
buffer *contents* change. If a reviewer can find a host-side write to the
simulation state, the example has failed at its one job.

### 3.3 Verification

**F6 — `--verify` is deterministic and golden-compared.** Fixed `dt = 1.0/60.0`
(not wall-clock), fixed initial state, `K` frames, then `update` prints each
body's `x,y` to a fixed number of decimals via `print`. The host compares the
captured output against a golden file and exits non-zero on mismatch. Argument
conventions follow X9 (`--verify`, auto-exit).

Pixel readback is **not** duplicated here — X5 already proves pixels reach the
surface. This example's claim is *"JS logic drove the frame"*, and the state
dump is the direct evidence for it. What the host must additionally assert is
that every one of the `K` frames presented successfully.

**F7 — the README states the contract in one paragraph.** N bodies, **one**
draw call, **one** JS→native crossing per frame, bundle recorded once and never
re-recorded. Raising N raises the JS cost and the memcpy; it does not raise the
*binding's* cost, because the binding is crossed once regardless. X5 says what
JS must not do in a frame; this example says what it does instead. The two
READMEs cross-reference each other (F17).

---

## 4. The contract (`Runtime::frame`)

The example above forces exactly one new capability: **the host must be able to
call a JS function once per frame, in a defined position relative to the four
things `tick()` already does.**

### 4.1 The order

`core::tick` today runs four steps:

1. `wgpuInstanceProcessEvents(instance)`
2. settlement drain
3. engine microtask drain
4. native release-queue drain

**F8 — `frame()` runs six steps, in this exact order:**

| # | Step | Why it is where it is |
|---|---|---|
| 1 | `wgpuInstanceProcessEvents` | Fires WebGPU callbacks; resolves Promises. |
| 2 | settlement drain | Hands those resolutions to the engine. |
| 3 | **microtask drain** | **Before** the callback. A `mapAsync` resolved in step 1 must have its `.then()` run before `update()` reads the result, or every readback is silently one frame stale. |
| 4 | **call `globalThis[name](...args)`** | The game logic. |
| 5 | **microtask drain** | **After** the callback. Without it, a `.then()` scheduled *inside* `update()` is deferred a full frame — silently. This is the microtask checkpoint that follows a callback in the WHATWG model, and it is cheap when the queue is empty. |
| 6 | release-queue drain | **After** the callback, so wrappers `update()` dropped are freed *this* frame rather than next. |

`tick()` keeps its current four-step body and every current caller (init loops,
the CTS runner, X4, X5) is untouched. Steps 1–3 and 6 *are* `tick()`'s steps:
factor `core::tick` into `pump` (1–3) and the release drain (6), and express
`frame` as `pump` → call → microtask drain → release drain. **Do not duplicate
the sequence into a second function** — one of the two will drift.

**F9 — step 5 and step 6 run even when the callback throws.** If `update()`
throws, `frame()` still drains microtasks and the release queue, *then* returns
the exception. Otherwise a script bug stalls the release queue on every frame and
leaks GPU memory for as long as the game keeps running — the exact failure mode
invariant 5 exists to prevent. A throwing script must be a script bug, not a
resource leak.

The host chooses what to do with the error. `examples/bounce` aborts loudly; the
README notes that a shipping host would rate-limit-log and keep rendering, and
that this is the host's policy, not the binding's.

### 4.2 The guardrails

**F10 — an `async` frame callback is a hard error.** `async function update(dt)`
returns immediately with a pending Promise; the body's remainder runs at some
later microtask drain. "The frame's logic ran" is then false, and *nothing in the
system says so* — the frame renders stale state forever and every symptom points
somewhere else. This is the single most likely honest mistake a JS author will
make here, and invariant 8 says the effort goes into catching honest mistakes
with clear, early errors.

`frame()` therefore inspects the callback's return value: if it is an object with
a **callable `then` property**, it returns `FrameError::AsyncCallback` naming the
global, with a message that says the function must be synchronous and that
`await` inside it straddles frames.

Detect the *thenable*, not the native Promise: an `async function` returns a
native Promise and a hand-rolled `{ then(resolve) {...} }` is the identical trap,
and thenable detection uses `get_property` + `is_callable`, which already exist
(F12). Do not add an `is_promise` trait method for this.

**F11 — a missing or non-callable global is an error, never a silent no-op.**
`FrameError::NotCallable(name)`. A host with no script logic calls `tick()`; a
host that asked for `update` and silently got nothing has no diagnostic.

### 4.3 The boundary

**F12 — this block adds zero methods to `trait JsEngine`.** The entire contract
composes from what the trait already has: `global`, `get_property`,
`is_callable`, `call`, `number`, `string`, `boolean`, `undefined`, `to_f64`,
`drain_microtasks`. The adapters add only the `HostValue` ↔ `E::Value` marshalling
they already do for `register_host_function_with_result`.

If implementing this turns out to require a new trait method, **stop and report
it** rather than adding one. Under CLAUDE.md's engine-boundary rule a forced
widening is a signal the boundary was drawn wrong; here it would specifically
mean that "call a function" is not expressible against the abstraction, which
would be a much larger finding than this block.

### 4.4 The two public methods

**F13 — two APIs, one primitive.**

- `Runtime::call_global_function(&self, name: &str, args: &[HostValue]) -> Result<HostValue>`
  — the general call-in. Block 13's CTS runner wants this independently of any
  frame. Converts the return value with the **existing** `HostValue` rules, but
  applies the F10 thenable check first.
- `Runtime::frame(&self, instance, name: &str, args: &[HostValue]) -> Result<usize>`
  — the frame contract of F8. Returns the release count, like `tick()`.
  **Discards the callback's return value** (after the F10 check); a host that
  needs a value out of the frame reads a global.

Both on both adapters, with identical semantics and identical error variants.

**Known sharp edge, recorded not fixed here:** the adapters' `host_value()`
falls back to `to_string()` for any non-primitive, so an object returned to
`call_global_function` arrives as `"[object Object]"`. F10 catches the case that
matters (a Promise). Whether the fallback should become an error for *all*
non-primitives is a separate question with a blast radius into every registered
host function's arguments (`print({})` would start throwing) — **do not change it
in this block.** Log it in `specs/tracking/` for a later decision.

---

## 5. Tests

**F14 — the parity suite pins the *contract*, not the example.** `parity.js`
gains a self-contained section that exercises `frame()` under both engines and
prints, byte-identically (P1):

- the F8 ordering (`pre` / `update` / `post`);
- the F10 rejection of an `async` callback, and of a thenable;
- the F11 rejection of a missing global;
- the F9 guarantee that a throwing callback still drains.

These are *this project's code* and are exactly what can diverge between Boa and
JavaScriptCore. They need no GPU and no shared source with any example.

**The example's simulation does NOT go in the parity suite.** An earlier draft of
this block required `examples/bounce`'s integrator to be shared with `parity.js`
so its bit-exactness could be pinned cross-engine. That was over-reach, and it is
withdrawn:

- The parity suite exists to catch engine divergence **in the binding** (block 08
  §1). A bouncing-body integrator is not the binding — it is `+`, `*`, and `<` on
  f64, which any conformant engine reproduces. Pinning it would test
  ECMAScript's spec compliance, not our code.
- The requirement was the *only* thing that wanted cross-file source sharing, and
  it dragged a real problem in behind it: block 12 → M4 records that **JSC's
  public C API has no module loader**, so a shared ES module is unrunnable on one
  of the two engines, and the fallback — `eval`-ing a shared file to define a
  global — trades a visible problem for a hidden one.
- The example's `--verify` golden runs on a single engine (examples link
  `boa_adapter`). It needs *intra*-engine reproducibility, which F2's arithmetic
  gives for free. Cross-engine bit-exactness of the example was never required.

So `examples/bounce` owns its simulation inside `bounce.js`, a single plain
script loaded exactly the way `triangle.js` already is. **F2 still stands** — the
no-transcendentals rule is what makes the example's own golden stable and keeps
the door open to pinning a simulation cross-engine later, if a reason ever
appears.

The JSC module gap is a real and separate problem — see §8.

**F15 — the headless unit tests are the gate; the example is gated.** Inline
`#[cfg(test)]` tests in **both** adapters, all against the Noop backend, all
required in CI (principle 7 — the example is a windowed real-GPU run and is
gated like X5, so it can never be the gate):

1. **Order.** A script appends to an array from (a) a `.then()` continuation
   pending from the previous tick, (b) the body of `update`, (c) a `.then()`
   scheduled *inside* `update`. After one `frame()` the array must read
   `["pre", "update", "post"]` — this pins steps 3, 4, and 5 against reordering.
2. **`async function update`** → `FrameError::AsyncCallback`.
3. **A thenable return** (`() => ({ then() {} })`) → the same error. This is the
   test that fails if someone "simplifies" F10 into a native-Promise check.
4. **Missing global**, and **a non-callable global** (`globalThis.update = 42`)
   → `FrameError::NotCallable`.
5. **A throwing `update`** → the error is returned **and** the release queue was
   still drained (F9). Assert on the drained count, not on the absence of a
   crash.
6. **A wrapper dropped inside `update`** is released within the same `frame()`
   call (F8 step 6), not the next.
7. **Argument round-trip:** `HostValue::Number`, `String`, `Bool`, `Null`,
   `Undefined` each arrive as the corresponding JS value.
8. **`tick()` is unchanged:** its existing tests still pass untouched, and a
   `tick()` call never invokes `update` even when the global exists.

**F16 — the boundaries hold.** No backend branch anywhere; `core/` names no
engine; `frame()` and its error type live in `core/` and the adapters only
marshal; no new `JsEngine` method (F12); no absolute or sibling paths in
anything committed.

---

## 6. Documentation

**F17 — X5's README cross-references bounce.** `examples/triangle`'s README
currently says *"JS never runs during the frame loop"* with no pointer to what JS
*does* do. Read alone it says the product does nothing. It gains one sentence
pointing at `examples/bounce`, and `bounce`'s README points back: **triangle
shows the static case (zero per-frame JS), bounce shows the dynamic case (one
per-frame JS call, still one draw call).** Neither is the whole picture without
the other.

**F18 — the plan states the positive contract.** Plan §2.7 documents the host
event-loop contract (`ProcessEvents` + microtask drain). It gains the frame
contract of F8 — the six steps, and the sentence *"not in the render hot path"
means the crossing count does not scale with the draw count, not that JS does
not run per frame.* Plan §1.3's "Explicitly not the goal" paragraph gains a
forward reference so it cannot be read as "JS does not run per frame" again.

---

## 7. Exit criteria

1. Both adapters pass the F15 suite; the standard workspace gate is green and
   untouched (`cargo test`, clippy `-D warnings`, `missing_docs`).
2. The parity suite's `frame()`-contract section is byte-identical under Boa and
   JSC on macOS (F14, P1).
3. `cargo run -p example-bounce -- --verify` matches its golden on a gated
   real-GPU run; `cargo run -p example-bounce` shows N bodies bouncing.
   Planner- or owner-verified, never CI.
4. `tick()`'s behaviour and callers are unchanged; X4 and X5 are untouched apart
   from X5's one README sentence (F17).
5. Zero new `JsEngine` methods (F12). If this was not achievable, the block does
   not exit — it reopens the boundary question.
6. Phase Review clean of CRITICAL and MAJOR.

---

## 8. A gap this block surfaced and does not close: JSC has no modules

Designing F14 surfaced a standing gap. Recorded here; **it is not in this block's
scope.**

**The facts, checked on `main`:**

- Boa has full ES-module support: `eval_module`, an alias map, importer-relative
  and extension-probing resolution, a pre-compile source transform hook, and
  top-level await that completes through `tick()` (block 12, M1–M3).
- The JavaScriptCore adapter contains **zero** occurrences of the string
  `module`. It has none of it.
- Block 12 → M4 records the reason: *"JSC's public C API has no module loader."*
  `JSEvaluateScript` evaluates a **script**; there is no C entry point that
  evaluates a **module**.

**Why M4's justification does not fully cover it.** M4 rationalises the gap as
acceptable because *"the CTS path is Boa-only"* — and for the CTS runner, that
reasoning is sound. But the gap is not confined to the CTS runner. It is a
**production** gap:

> A real game's JavaScript is multi-file. If module loading works on Android
> (Boa) and does not exist on iOS (JSC), then the two Tier-1 engines disagree
> about **how game code is loaded at all** — not about a conversion detail, but
> about the shape of the program.

That is precisely the class of divergence the two-engine parity strategy exists
to prevent, and it is currently the largest known one. It should be an explicit
open question in `CLAUDE.md`, not a line inside a test-tooling block.

**This gap is now owned by block 16** (`specs/blocks/16-jsc-modules.md`), which
runs on macOS and establishes — against the actual Apple SDK and a running spike
— whether JavaScriptCore can be given real ES modules at all, then implements
either that or the fallback (game code bundled to a single script at build time).
Nothing is planned against it here, because every candidate answer rests on claims
about the SDK that cannot be verified from the environment this block was written
in.

The only thing block 15 asserts about it is the part that is already proven on
`main`: **Boa has modules and JavaScriptCore has none**, and block 15 does not
need either.
