# Block 16 — ES modules under JavaScriptCore: an investigation

Rules are numbered **L1–L14**. Blocks 04 (JSC adapter), 08 (P1–P8), and 12
(M1–M6) bind.

**This block produces evidence and a recommendation. It does not produce
production code.** Its exit is an owner decision, not a merge. Read L1 before
anything else.

---

## 1. What this block is

**L1 — this is a spike, and its deliverable is written findings.** No file under
`core/`, `adapters/`, `ffi/`, or `codegen/` is modified by this block. The work
product is a throwaway spike under `spikes/jsc-modules/` (excluded from the
workspace, like `spikes/jsc-detach/`) plus answers recorded in
`specs/tracking/engine-boundary.md` as numbered questions with citations.

The reason is CLAUDE.md's standing rule for open design questions: *answer with
evidence; do not let the draft plan's guesses harden into assumptions.* Every
candidate solution below rests on claims about Apple's SDK that **have not been
verified** and **cannot be verified from the environment this spec was written
in** (Windows; no Apple SDK). A plan built on them now would be exactly the
failure mode §7 of the project plan exists to record.

---

## 2. The gap, with evidence from `main`

**L2 — the facts, checked, not remembered.**

| Claim | Evidence on `main` |
|---|---|
| Boa has full ES modules | `adapters/boa/src/lib.rs`: `use boa_engine::module::{ModuleLoader, Referrer}` (:21), `struct FileModuleLoader` (:220), `pub fn eval_module(&self, path: &Path)` (:622), `set_module_alias` (:599), `set_module_transform` (:612). Tests cover relative import chains (:2168), aliases, diamond graphs (:2246). Block 12 → M1–M3. |
| The JSC adapter has none of it | The adapter is four files (`build.rs`, `Cargo.toml`, `src/lib.rs`, `src/imp.rs`). A repo-wide grep for `module`/`Module`/`eval_module`/`ModuleLoader`/`JSScript` returns **zero** hits inside it. |
| The JSC **C** API has no module entry point | Block 12 → M4: *"JSC's public C API has no module loader."* The adapter's own hand-written `#[link(name = "JavaScriptCore", kind = "framework")] unsafe extern "C"` block (`imp.rs`:135) declares `JSEvaluateScript` — which evaluates a **script** — and nothing that evaluates a **module**. **L-Q1 re-verifies this against the SDK rather than trusting M4.** |
| The JSC adapter is pure C FFI | No `bindgen`, no `jsc`/`javascriptcore` crate: the C API is hand-declared in `imp.rs`. Any Objective-C path is therefore a **new kind of dependency** for this adapter, not an extension of an existing one. |
| Our context has a **custom global object class** | `imp.rs`:890 — `JSGlobalContextCreate(global_object_class)`. The WebGPU classes, `print`, and host globals live on that custom global. This is the detail that decides L-Q3. |

**L3 — why M4's justification does not cover this, and why the block exists.**
M4 accepts the gap because *"the CTS path is Boa-only."* For the CTS runner that
is sound. But the gap is not confined to a dev tool:

> A real game's JavaScript is multi-file. If module loading works on Android
> (Boa) and does not exist on iOS (JSC), the two Tier-1 engines disagree about
> **how game code is loaded at all** — not about a conversion detail, but about
> the shape of the program.

That is the exact class of divergence the two-engine strategy exists to prevent,
and it is the largest known one. CLAUDE.md's engine tiers promise
*"iOS(JSC)↔Android(Boa) parity guaranteed by verification"* — this is a place
where there is currently nothing to verify, because one side cannot run the
program at all.

---

## 3. The candidates

Four answers exist. The investigation's job is to kill the ones that do not
survive contact with the SDK, and to price the ones that do.

**L4 — Candidate A: bridge to JavaScriptCore's Objective-C API.**
*Believed, unverified:* the ObjC API exposes module loading that the C API does
not (a script object with a module type, and a module-loader delegate on
`JSContext`), and a C `JSGlobalContextRef` can be bridged into an ObjC
`JSContext`. If all of that is true and the bridge preserves our custom global,
the adapter gains real ES modules while keeping its C foundation.

*Cost, if viable:* an Objective-C dependency in an adapter that is pure C FFI
today; a minimum-OS floor set by the ObjC API's availability; and a second async
model (the delegate's resolve/reject handlers) to reconcile with the `tick()`
contract.

**L5 — Candidate B: non-public / SPI entry points. Rejected outright, do not
investigate.** Any use of private JavaScriptCore API is an App Store rejection
risk and is not a supportable foundation for a shipping iOS product. If the
agent finds a module symbol that is not in a public SDK header, that is a
finding to *record*, not a path to pursue.

**L6 — Candidate C: implement module semantics inside the JSC adapter** (parse
imports, rewrite into a runtime registry over `JSEvaluateScript`). **Rejected
regardless of what the evidence says**, and the spec states the reason so that
nobody re-proposes it as the cheap option:

It would give the two engines *different* module semantics — live bindings, the
temporal dead zone, circular-import behaviour, `import.meta` — with a real
loader on one side and a hand-rolled approximation on the other. That converts a
**visible** gap into an **invisible seam**, which is strictly worse. The two
engines are Tier 1 precisely because parity is proven by verification rather than
assumed; a seam whose two sides are different by construction cannot be verified,
only hoped about.

**L7 — Candidate D: no runtime modules for game code — bundle at build time.**
The application's build flattens its module graph into a single script (the
ordinary JS toolchain job: esbuild, rollup, swc). Both engines then execute the
**identical single script**, so there is no runtime module seam and parity is
exact *by construction* rather than by verification.

This is not a workaround, and it must be evaluated on its merits rather than
dismissed:

- **Precedent (believed, cheap to confirm, and not load-bearing on its own):**
  React Native ships a bundler-produced single JavaScript file to JavaScriptCore
  on iOS. If so, shipping a bundled script to JSC is the industry's standing
  answer to exactly this gap, at very large scale.
- **It does not regress the CTS end-state.** M4 already scoped the CTS runner to
  Boa-only. Boa's runtime module loader (block 12) stays exactly as it is and
  keeps serving the tool it was built for.
- **It moves the seam to a place where it can be tested.** One bundle, two
  engines, byte-identical output — that is what the parity suite (P1) already
  measures.

*Cost:* game authors need a build step (normal for this audience, but it must be
stated in user-facing docs, not discovered); dynamic `import()` and `import.meta`
become build-time concerns; and the binding ships no bundler, so the guidance
must be concrete enough to follow.

---

## 4. The verification task

**L8 — this is the work.** Answer each question below on macOS, against the
**actual SDK headers** and a **running spike**. Web articles, blog posts, and
recollection are not evidence. Each answer cites the header file name, the exact
symbol, and its availability attribute.

**Report the finding, never the path.** CLAUDE.md forbids any local or sibling
filesystem path in a committed file. The SDK location found via `xcrun` is a
tool-use detail: record *"`JSContext.h` in the JavaScriptCore framework's public
headers declares X, `API_AVAILABLE(macos(N), ios(M))`"*, never the absolute path
that reached it.

### The questions

**L-Q1 — Confirm the C API really has nothing.** Exhaustively search the
JavaScriptCore framework's **public C headers** for any module-related entry
point. Expected answer: none. Confirm or refute M4, and list the headers
searched. *If this turns up a public C module API, candidates A and D both
collapse into something much simpler and the block's conclusion changes
completely — so do not skip it on the assumption that M4 is right.*

**L-Q2 — What does the Objective-C API actually expose?** For each of the
following, state: exists / does not exist; declaring header; exact signature;
availability attribute.
- A script object supporting a **module** type (as opposed to a program/script
  type), and how it is constructed.
- A **module loader delegate** property on `JSContext`, and the protocol it
  conforms to (including the exact fetch/resolve/reject method signatures).
- A method on `JSContext` that **evaluates** such a script object.
- `+[JSContext contextWithJSGlobalContextRef:]` or whatever the public C→ObjC
  context bridge actually is.

**L-Q3 — The bridge question. This one decides candidate A, and it must be
answered by a running spike, not by reading headers.** Our context is created by
`JSGlobalContextCreate(global_object_class)` with a **custom global object
class** (`imp.rs`:890); `print`, `device`, and every WebGPU class live on that
global.

Build a spike that: creates the context exactly the way `imp.rs` does (custom
global class and all), bridges it to an ObjC `JSContext`, installs a module
loader delegate, evaluates a two-file module graph, and — from **inside the
module** — reads a global that the C side installed on the custom global object
and calls a C-registered host function.

- If the module's scope cannot see the custom global's properties, **candidate A
  is dead** and the answer is D.
- If the bridge produces a context with a *different* global object, **candidate
  A is dead.**
- If it works, say precisely what "works" required.

**L-Q4 — Does the delegate's async model fit `tick()`?** The module loader
delegate resolves fetches through handler blocks. Our loader reads files from
disk, i.e. synchronously.

- Can `resolve` be invoked **synchronously, inside** the fetch callback?
- Does module evaluation then complete without a run-loop turn or a dispatch
  queue drain — i.e. can it complete inside a `tick()` the way Boa's
  `load_link_evaluate` + `run_jobs` does (block 12 → M1: *"top-level await
  advances through the ordinary `tick()`"*)?
- If it needs a run loop, **say so plainly** — a JS-facing async mechanism that
  requires a run-loop turn is a second event-loop contract next to the one
  invariant 3 already pins, and that is a serious cost, not a detail.

**L-Q5 — What does the ObjC dependency actually cost?** Can the bridge be
written with hand-declared `objc_msgSend` externs in the existing `imp.rs` style,
or does it force an `objc2`-family crate? Does whatever it needs still
cross-compile for `aarch64-apple-ios` under the toolchain block 06 established?
An adapter that builds on macOS but breaks the iOS cross-compile has solved
nothing — iOS is the production target.

**L-Q6 — What is the OS floor, and can we accept it?** This project has **no
recorded iOS/macOS deployment target** (checked: block 06 records the triples,
not a minimum version). L-Q2's availability attributes propose one. State it, and
state whether the ObjC module API's floor is above or below the oldest OS this
project intends to support. *This block is what forces that number to exist.*

**L-Q7 — Dynamic `import()` and `import.meta`.** Boa supports them. Does the
delegate path? Parity means knowing which of the two engines can run which
program, so an unsupported form is a finding, not an omission.

**L-Q8 — Errors.** Does the ObjC path introduce a second exception channel
(`NSError`, `JSValue` exceptions) that must be marshalled into the adapter's
existing R26 error discipline? Show what a module *resolution failure* and a
module *evaluation throw* each look like coming out.

**L-Q9 — Confirm or drop the candidate-D precedent.** Establish cheaply whether a
major shipping product does in fact deliver a bundler-produced single script to
JavaScriptCore on iOS. This is corroboration, not proof: **do not let a "yes"
substitute for L-Q3's spike, and do not let a "no" kill candidate D** — D stands
or falls on its own cost, and it is viable today with zero new code.

---

## 5. The decision rule

**L9 — how the answers pick an answer.** The agent returns evidence and a
recommendation; the **owner decides**. But the logic is fixed in advance so the
conclusion cannot be steered by whichever candidate the investigation happened to
enjoy more:

1. **L-Q1 finds a public C module API** → everything below is moot; re-plan.
2. **L-Q3 fails, or L-Q4 requires a run loop, or L-Q5 breaks the iOS
   cross-compile** → candidate A is not viable. **The answer is D**, and the gap
   is closed by documentation and tooling guidance rather than by code.
3. **A is viable** → the recommendation must be an explicit cost comparison
   against D, not a default in A's favour for being the more interesting
   engineering. A buys real ES modules on both engines; D buys exact parity with
   no new code and no new dependency. **State which you would choose and why**,
   in one paragraph, and make the case for the one you did not choose too.

**L10 — the null result is a real result.** If candidate A dies, the block has
succeeded: it converted the project's largest unverified parity risk into a
recorded fact and a cheap answer. Do not pad it into an implementation.

---

## 6. Boundaries

**L11 — no production code.** `core/`, `adapters/`, `ffi/`, `codegen/`,
`examples/`, and `tests/` are untouched. The spike lives in `spikes/jsc-modules/`
and is excluded from the workspace (`Cargo.toml`'s `exclude` list already carries
`spikes/jsc-detach`, the precedent).

**L12 — no private API** (L5), in the spike or anywhere else.

**L13 — no local or sibling paths in anything committed** (L8). The `xcrun`
invocation is a tool-use detail; the finding is the citation.

**L14 — findings land in `specs/tracking/engine-boundary.md`**, numbered, each
with its evidence, in the style of that file's existing Q1/Q1b entries. Block 12
→ M4 gets a cross-reference to whatever L-Q1 concludes: if M4 is confirmed, it
now has a citation instead of an assertion; if refuted, it is corrected in place.

---

## 7. Exit criteria

1. L-Q1 through L-Q9 answered, each with a header citation or a spike result.
   *"I could not determine this"* is an acceptable answer and is far better than
   a confident guess — say which question, and what would settle it.
2. The L-Q3 spike exists, runs, and its result is stated unambiguously: does a
   module evaluated through the ObjC bridge see the C-created custom global, or
   not.
3. A recommendation under L9, with the cost comparison, including the argument
   *against* the recommended option.
4. Findings recorded per L14; M4 confirmed-with-citation or corrected.
5. No file outside `spikes/` and `specs/` is modified.
6. The owner decides. Only then does an implementation block get written — and
   it will be written against this block's findings, not against this block's
   guesses.
