# Block 16 — ES modules under JavaScriptCore

**Status: COMPLETE (2026-07-14). Phase Review clean of CRITICAL and MAJOR.**
**Outcome: candidate D — build-time bundling. Trigger: D4 (the only path to JSC
modules is non-public API).**

Evidence and decision: `specs/tracking/engine-boundary.md` → Q11 and its addenda.
Block 12 → M4 corrected. §10 (planner review) and the Phase Review findings are
closed.

Note for anyone shipping on this: **bundling erases top-level TDZ** (Q11 addendum
3). Development against Boa's module loader has real TDZ; the shipped flat bundle
does not. Stated in the README, pinned by the parity golden.

Rules are numbered **L1–L20**. Blocks 04 (JSC adapter), 08 (P1–P8), and 12
(M1–M6) bind.

**This block runs entirely on macOS, in one pass: verify, decide, implement.**
There is no hand-back to a Windows planner between the phases — the decision rule
in §5 is fixed in advance and the owner's calls are already recorded, so nothing
in this block needs to ask a question it cannot answer itself.

---

## 1. What this block is

**L1 — one agent, one pass, on macOS.** The environment this spec was written in
is Windows with no Apple SDK, so §4's questions could not be answered here. They
must be answered *before* the implementation shape is known, and answering them
requires the machine that will also do the implementing. Splitting that across
two machines would be pure latency.

So: Phase 1 (§4) gathers evidence. §5 turns that evidence into a decision with a
rule that is **already fixed** — it cannot be steered by whichever candidate the
investigation happened to enjoy more. Phase 2 (§6 or §7, whichever §5 selects)
implements. The standard project workflow applies to Phase 2: spec (this file) →
failing test → implementation → Phase Review.

**L2 — the null result is a first-class outcome, not a failure.** If the evidence
kills the Objective-C candidate, §7 is a real and correct deliverable and the
block is a success. Do not pad it into an implementation nobody needs, and do not
strain to make the ObjC path work because "implement" appeared in the assignment.

---

## 2. The gap, with evidence from `main`

**L3 — the facts, checked, not remembered.**

| Claim | Evidence on `main` |
|---|---|
| Boa has full ES modules | `adapters/boa/src/lib.rs`: `use boa_engine::module::{ModuleLoader, Referrer}` (:21), `struct FileModuleLoader` (:220), `pub fn eval_module(&self, path: &Path)` (:622), `set_module_alias` (:599), `set_module_transform` (:612). Tests cover relative import chains (:2168), aliases, diamond graphs (:2246). Block 12 → M1–M3. |
| The JSC adapter has none of it | The adapter is four files (`build.rs`, `Cargo.toml`, `src/lib.rs`, `src/imp.rs`). A repo-wide grep for `module` / `Module` / `eval_module` / `ModuleLoader` / `JSScript` returns **zero** hits inside it. |
| The JSC **C** API has no module entry point | Block 12 → M4: *"JSC's public C API has no module loader."* The adapter's hand-written `#[link(name = "JavaScriptCore", kind = "framework")] unsafe extern "C"` block (`imp.rs`:135) declares `JSEvaluateScript` — which evaluates a **script** — and nothing that evaluates a **module**. **L-Q1 re-verifies this against the SDK rather than trusting M4.** |
| The JSC adapter is pure C FFI | No `bindgen`, no `jsc` crate: the C API is hand-declared in `imp.rs`. An Objective-C path is a **new kind of dependency** for this adapter, not an extension of an existing one. |
| Our context has a **custom global object class** | `imp.rs`:890 — `JSGlobalContextCreate(global_object_class)`. `print`, `device`, and every WebGPU class live on that custom global. **This is the detail that decides L-Q3, and therefore the block.** |

**L4 — why this is worth an adapter-level dependency, and why M4 does not cover
it.** M4 accepts the gap because *"the CTS path is Boa-only."* For the CTS runner
that is sound. But the gap is not confined to a dev tool:

> A real game's JavaScript is multi-file. If module loading works on Android
> (Boa) and does not exist on iOS (JSC), the two Tier-1 engines disagree about
> **how game code is loaded at all** — not about a conversion detail, but about
> the shape of the program.

That is the exact class of divergence the two-engine strategy exists to prevent,
and it is the largest known one. CLAUDE.md promises *"iOS(JSC)↔Android(Boa)
parity guaranteed by verification"* — this is a place where there is currently
nothing to verify, because one side cannot run the program at all.

---

## 3. The candidates

**L5 — Candidate A: bridge to JavaScriptCore's Objective-C API.** *Believed,
unverified:* the ObjC API exposes module loading the C API does not (a script
object with a module type; a module-loader delegate on `JSContext`), and a C
`JSGlobalContextRef` can be bridged into an ObjC `JSContext`. If true, and if the
bridge preserves our custom global, the adapter gains real ES modules while
keeping its C foundation. **This is the preferred outcome** (see the owner
decision in §5).

**L6 — Candidate B: non-public / SPI entry points. Rejected outright; do not
pursue.** Private JavaScriptCore API is an App Store rejection risk and is not a
supportable foundation for a shipping iOS product. If a module symbol is found
that is not in a public SDK header, that is a finding to *record*, not a path to
take.

**L7 — Candidate C: hand-roll module semantics inside the JSC adapter** (parse
imports, rewrite into a runtime registry over `JSEvaluateScript`). **Rejected
regardless of the evidence**, and the reason is stated here so nobody
re-proposes it as the cheap option:

It would give the two engines *different* module semantics — live bindings, the
temporal dead zone, circular-import behaviour, `import.meta` — a real loader on
one side and an approximation on the other. That converts a **visible** gap into
an **invisible seam**, which is strictly worse than the gap. The two engines are
Tier 1 precisely because parity is *proven*, and a seam whose two sides differ by
construction cannot be proven.

**L8 — Candidate D: no runtime modules for game code — bundle at build time.**
The application's build flattens its module graph into one script (esbuild,
rollup, swc — the ordinary JS toolchain job). Both engines then execute the
**identical single script**: no runtime module seam, and parity is exact *by
construction* rather than by verification. Zero new code, zero new dependencies,
zero new OS floor. The cost is that game authors need a build step — normal for
this audience, but it must be *documented*, not discovered.

This is the fallback. §7 specifies it.

---

## 4. Phase 1 — the evidence

**L9 — answer these against the actual SDK headers and a running spike.** Web
articles, blog posts, and recollection are not evidence. Each answer cites the
declaring header, the exact symbol, and its availability attribute.

**Report the finding, never the path.** CLAUDE.md forbids any local or sibling
filesystem path in a committed file. The SDK location found via `xcrun` is a
tool-use detail: record *"`JSContext.h` in the JavaScriptCore framework's public
headers declares X, `API_AVAILABLE(macos(N), ios(M))`"* — never the absolute path
that reached it.

**L-Q1 — Confirm the C API really has nothing.** Exhaustively search the
JavaScriptCore framework's **public C headers** for any module-related entry
point. Expected: none. Confirm or refute M4 and list the headers searched. *If a
public C module API exists, everything below is moot and the block is far simpler
— so do not skip this on the assumption that M4 is right.*

**L-Q2 — What does the Objective-C API actually expose?** For each: exists /
does not exist; declaring header; exact signature; availability attribute.
- A script object supporting a **module** type (vs. a program/script type), and
  how it is constructed.
- A **module loader delegate** property on `JSContext` and the protocol it
  conforms to — including the exact fetch / resolve / reject method signatures.
- The `JSContext` method that **evaluates** such a script object.
- The public **C→ObjC context bridge** (`+[JSContext contextWithJSGlobalContextRef:]`,
  or whatever it actually is).

**L-Q3 — The bridge question. This decides the block, and only a running spike
can answer it.** Our context is created by `JSGlobalContextCreate(global_object_class)`
with a **custom global object class** (`imp.rs`:890); `print`, `device`, and every
WebGPU class live on that global.

Build a spike that creates the context **exactly the way `imp.rs` does** (custom
global class and all), bridges it to an ObjC `JSContext`, installs a module
loader delegate, evaluates a two-file module graph, and — *from inside the
module* — reads a global the C side installed on the custom global object and
calls a C-registered host function.

- If the module's scope cannot see the custom global's properties → **A is
  dead.**
- If the bridge yields a context with a *different* global object → **A is
  dead.**
- If it works, state precisely what making it work required.

**L-Q4 — Does the delegate's async model fit `tick()`?** The delegate resolves
fetches through handler blocks; our loader reads files from disk, i.e.
synchronously.
- Can `resolve` be invoked **synchronously, inside** the fetch callback?
- Does module evaluation then complete **without a run-loop turn or a dispatch
  queue drain** — i.e. inside a `tick()`, the way Boa's `load_link_evaluate` +
  `run_jobs` does (M1: *"top-level await advances through the ordinary
  `tick()`"*)?
- If it needs a run loop, **say so plainly.** A JS-facing async mechanism that
  requires a run-loop turn is a *second event-loop contract* standing next to the
  one invariant 3 already pins. That is a disqualifying cost, not a detail.

**L-Q5 — What does the ObjC dependency cost?** Can the bridge be written with
hand-declared `objc_msgSend` externs in `imp.rs`'s existing style, or does it
force an `objc2`-family crate? Either is acceptable (§5) — but it **must still
cross-compile for `aarch64-apple-ios`** under the toolchain block 06 established.
An adapter that builds on macOS and breaks the iOS cross-compile has solved
nothing; iOS is the production target.

**L-Q6 — What OS floor does it impose?** State the minimum macOS and iOS implied
by L-Q2's availability attributes. The project has **no recorded deployment
target** (checked: block 06 records triples, not a version floor). This block is
what forces that number to exist — see the owner decision in §5.

**L-Q7 — Dynamic `import()` and `import.meta`.** Boa supports them. Does the
delegate path? An unsupported form is a *finding* (and a parity fact that must be
documented), not an omission to paper over.

**L-Q8 — Errors.** Does the ObjC path introduce a second exception channel
(`NSError`, `JSValue` exceptions) that must be marshalled into the adapter's
existing R26 error discipline? Show what a module **resolution failure** and a
module **evaluation throw** each look like coming out.

**L-Q9 — Corroborate candidate D cheaply.** Establish whether a major shipping
product delivers a bundler-produced single script to JavaScriptCore on iOS
(React Native's bundle is the suspected precedent). This is corroboration only:
**do not let a "yes" substitute for L-Q3's spike, and do not let a "no" kill
candidate D** — D stands or falls on its own cost and is viable today with zero
new code.

**L10 — the spike is throwaway.** It lives in `spikes/jsc-modules/`, excluded
from the workspace (`Cargo.toml`'s `exclude` already carries `spikes/jsc-detach`,
the precedent). It is not the implementation; do not grow it into one.

---

## 5. The decision rule

**L11 — the owner's calls, recorded 2026-07-13, so the agent never has to ask.**

1. **If the Objective-C bridge is viable, take it.** An Objective-C dependency in
   the JSC adapter is **accepted** in exchange for real ES modules on both
   engines. Closing the parity gap at the binding layer is worth it, and it keeps
   users off a mandatory build step.
2. **There is no pre-existing iOS floor to protect.** No legacy-device
   requirement exists. Whatever minimum L-Q2's availability attributes imply is
   **acceptable**, and the measured value becomes the project's officially
   recorded floor (L19). L-Q6 therefore **cannot disqualify** candidate A — it
   *sets a number*.

**L12 — so candidate A is disqualified only by hard technical failure.** Any one
of these selects candidate D (§7):

- **D1** — L-Q3's spike fails: a module cannot see the C-created custom global,
  or the bridge yields a different global object.
- **D2** — L-Q4: evaluation cannot complete inside `tick()` and requires a run
  loop or a dispatch-queue drain (a second event-loop contract).
- **D3** — L-Q5: the dependency breaks the `aarch64-apple-ios` cross-compile.
- **D4** — the only viable path is non-public API (L6).

Otherwise, implement candidate A (§6).

**L13 — write the decision down before writing the code.** Record in
`specs/tracking/engine-boundary.md`, with citations: which branch was taken, and
which of D1–D4 fired or did not. A reader six months from now must be able to see
*why*, not just *what*.

---

## 6. Phase 2A — implement JSC modules

Only if §5 selects candidate A.

**L14 — API parity with Boa is the acceptance bar, not "modules work".** The JSC
adapter's public surface must match `adapters/boa`'s, name for name and semantic
for semantic, so a host can swap adapters without rewriting its loading code:

- `eval_module(&self, path: &Path)` returning an evaluation handle with the same
  status semantics as Boa's `ModuleEvaluation` / `ModuleEvaluationStatus`.
- `set_module_alias(&self, specifier: &str, path: &Path)`.
- `set_module_transform(&self, F)` — runs on **every** loaded module source
  before compilation, **including the `eval_module` root** (M2).
- M3's resolution order, identical: alias (exact specifier) → importer-relative
  exact path → probe `<spec>`, `<spec>.js`, `<spec>.mjs`, `<spec>/index.js`. First
  hit wins; **the miss error names the specifier, the importer, and the probe
  list**.
- Top-level await completes through the ordinary `tick()` (M1).
- R26 error discipline throughout, including whatever L-Q8 turned up.

**L15 — the tests mirror block 12's M5, test for test**, on the JSC side:
relative import chains, alias resolution, diamond graphs, miss-error naming,
eval-throw, TLA-through-tick, the transform hook (identity, marker rewrite, and
an error path that names the path), extension probing (each probe step, and the
miss error listing the probes), root-file transform, and alias+probe interaction.
A JSC module test that has no Boa counterpart, or vice versa, is a gap — say so.

**L16 — the parity suite gets a module graph.**
`tests/parity/` gains a module-graph section run under **both** engines
with **byte-identical** output (P1). Until now the module system could not be
parity-verified at all, because only one engine had one. That it now *can* be is
the deliverable; that it *is* is the exit criterion. A JSC module loader that
ships without a parity test has closed the visible gap and left the invisible one
(L7's warning applies to a real loader too, if nobody compares them).

**L17 — the boundary holds.** Module loading is **adapter-level on Boa**
(`FileModuleLoader` lives in `adapters/boa/src/lib.rs`, not in `core/`). JSC's
must be adapter-level too. Expect **zero** changes to `core/` and **zero** new
`JsEngine` methods; if either is forced, stop and report it — under CLAUDE.md's
engine-boundary rule that is the boundary telling us it was drawn wrong, and it
is a bigger finding than this block.

**L18 — block 12 → M4 is now wrong. Correct it in place**, with the L-Q1
citation, and note that JSC module support landed here. A stale M4 saying "the
CTS path is Boa-only because JSC has no modules" would send the next reader down
a path that no longer exists.

**L19 — record the OS floor.** L-Q6's measured minimum becomes the project's
documented macOS/iOS deployment floor. It goes where a reader will find it —
CLAUDE.md's platform table and block 06 — not only in a tracking file. This is a
new, load-bearing project fact that did not exist before this block.

---

## 7. Phase 2D — implement build-time bundling

Only if §5 selects candidate D.

**L20 — what shipping D means.**

1. **Correct block 12 → M4 with the citation** from L-Q1/L-Q2/L-Q3: it is right
   that JSC has no modules, but its justification (*"the CTS path is Boa-only"*)
   is too narrow. The real reason is that the ObjC path was investigated and
   failed for a stated reason (which of D1–D4), and the answer for game code is a
   build-time bundle.
2. **Say it where a user will read it.** The user-facing docs must state plainly:
   game JavaScript is delivered to the runtime as a **single script**; multi-file
   sources are bundled by the application's build; ES modules at runtime are a
   Boa-only development-tooling capability and must not be relied on by game
   code. This is a shipping constraint, and CLAUDE.md's standing rule for JSC's
   `destroy()` applies to it too: say exactly that, do not soften it.
3. **Prove the bundle is engine-neutral.** The parity suite gains a section that
   runs a single pre-bundled script — of the shape a real bundler emits, module
   registry and all — under **both** engines with byte-identical output (P1).
   That is what makes D's "parity by construction" a claim with a test behind it
   rather than an assertion.
4. **Record the finding**, with all of §4's answers, in
   `specs/tracking/engine-boundary.md`. The next person who wonders "why don't we
   just use JSC's module loader?" must find the answer, with citations, instead of
   re-running this investigation.
5. **Do not implement candidate C** (L7).

---

## 8. Boundaries

- **No private API** (L6), in the spike or the implementation.
- **No local or sibling paths in anything committed** (L9). The `xcrun`
  invocation is a tool-use detail; the finding is the citation.
- **`core/`, `codegen/`, and `ffi/` are untouched** (L17).
- **The Boa adapter is untouched**, except for a shared parity fixture if L16
  needs one.
- Standard gates stay green: `cargo test`, clippy `-D warnings`, `missing_docs`.
  The JSC suite and the parity suite both run on macOS — run them.

---

## 9. Exit criteria

1. L-Q1 through L-Q9 answered, each with a header citation or a spike result.
   *"I could not determine this"* is acceptable and is far better than a confident
   guess — say which question, and what would settle it.
2. The L-Q3 spike exists, runs, and its result is unambiguous: does a module
   evaluated through the ObjC bridge see the C-created custom global, or not.
3. The §5 decision is recorded with its reason (L13) **before** the implementation
   is written.
4. The selected phase (§6 or §7) is complete, with its tests, and the parity suite
   covers it under **both** engines (L16 / L20.3).
5. Block 12 → M4 is corrected either way (L18 / L20.1).
6. If §6 shipped: the OS floor is recorded where a reader will find it (L19), and
   `core/` gained no changes and `JsEngine` gained no methods (L17).
7. Phase Review clean of CRITICAL and MAJOR.

---

## 10. Planner review of Phase 2D (2026-07-13) — what must change

Reviewed at `04c43d7` against §9. **The decision, the evidence in Q11, the M4
correction, and the README constraint are all accepted as-is**, including the
L-Q3 deviation, which was recorded as L2 and L10 require. None of that is reopened.

What follows is about the *test fixture* Phase 2D shipped.

### L21 — MAJOR: the bundle fixture claims to be bundler-shaped, and on the error path it is not

`tests/parity/parity.js`'s registry caches a module *before* evaluating its
factory and never removes the entry if that factory throws:

```js
__cache[id] = module;
__modules[id](module, module.exports, __require);   // if this throws...
```

So a second `__require` of a throwing module **silently returns its partial
exports** — no re-run, no re-throw. The fixture asserts this, and
`expected.txt` now pins it:

```
bundle:throw:cached-partial:before-throw
```

**No module system behaves that way.** Under ES modules — which is what this
bundle is standing in for, since the entire premise is that game code is authored
as modules and bundled — a module that throws during evaluation is permanently
errored, and every later import **re-throws the same error**. Partial exports are
never handed out. The fixture's error path therefore diverges from the semantics
it exists to stand in for, and the golden has frozen the divergence: the next
person who *fixes* the registry will break the parity gate and be told they are
wrong.

The fixture's comment claims it is *"the CommonJS-style shape emitted by bundlers
such as esbuild and Metro."* **That claim was never checked against esbuild's or
Metro's actual output.**
Rule: a claim about another system's behaviour requires running that system.

**The fix:**

1. `delete __cache[id]` when a factory throws, and assert that a second
   `__require` **re-runs the factory and re-throws**. That exercises exception
   propagation through a nested require chain twice and a factory re-entry —
   genuinely more than the current assertion does.
2. Update `expected.txt` accordingly.
3. **Verify the comment's claim by construction:** bundle a small multi-file
   fixture with a real esbuild (or Metro) invocation, read what it actually emits
   on the error path, and either match it or delete the claim. Do not keep both a
   fidelity claim and a semantic no bundler has.

### L22 — MINOR: `import()` is the one uncited claim in Q11

Q11 → L-Q7 states that dynamic `import()` is out of reach. **It was not tested**,
and it is the one claim in Q11 with no citation. Unlike an `import` *declaration*, a
dynamic `import()` call is syntactically legal in the Script goal, so whether a bare
`JSGlobalContextRef` rejects it at runtime is an *empirical* question, not one the
headers answer.

Add a small test to the JSC suite establishing what actually happens, or mark the
claim untested in Q11. Game code must not rely on `import()` under candidate D
regardless.

### L23 — process: the Phase Review did not run

Exit criterion 7 requires a Phase Review, and the block shipped in two commits with
no review artifact — the gate that would have caught the MAJOR above. Run the review
over the cumulative diff before closing.

### Exit criteria for the reopen

1. L21 fixed: the registry drops a throwing module from the cache, the fixture
   asserts the re-throw, `expected.txt` is regenerated, and the bundler-fidelity
   claim is either verified against a real bundler's output or removed.
2. L22 answered or explicitly marked untested in Q11.
3. L23: a Phase Review runs over block 16's cumulative diff and is clean of
   CRITICAL and MAJOR.
4. The parity suite is byte-identical under **both** engines again after the golden
   changes (P1) — the whole point of the fixture.
5. Standard gates green.
