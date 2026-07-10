# Workflow & roles

Implementation is performed by a **separate coding agent**. Claude acts as
**planner and orchestrator**, not implementer.

This document is the operational companion to `CLAUDE.md` (which holds the
invariants) and `specs/webgpu-native-js-project-plan.md` (which holds the design
and phasing).

## Role split

| Actor | Responsibilities |
|---|---|
| **Claude** (planner/orchestrator) | Author & maintain `specs/` (blocks, tracking, reference); decompose each phase into a self-contained **task handoff**; review the coding agent's diff against acceptance criteria; run/inspect `cargo build`, `cargo test`, `cargo clippy`; manage version control (`git add`, `git commit`); update the area's `tracking/<topic>.md`; decide go/no-go for the next slice. |
| **Coding agent** (implementer) | Read the assigned task handoff + referenced block spec; write the code and its inline unit tests; make the targeted gate green headless; report what it changed. Does **not** edit `specs/`, commit, or change scope. |

Claude does **not** write production code itself. The coding agent does **not**
plan or commit.

**Investigation is Claude's.** Answering a question by reading a header, a spec,
or an upstream implementation is planning work, not implementation work, and
Claude does it directly. Only work that produces *committed source in this
repository* goes to the coding agent. A Phase 0 spike that needs a compiled
Rust harness is a coding-agent task; a Phase 0 question answerable by reading
`webgpu.h` is not.

## Tracking convention

Work is logged in **per-topic** tracking docs, `specs/tracking/<topic>.md` —
for example `engine-boundary.md`, `backend-deltas.md`, `release-queue.md`.
Per-phase `phase-N.md` logs are **not** written.

A tracking doc records, for its topic: the questions asked, the evidence found
(with primary-source citations, never local paths — see `CLAUDE.md` → "No local
or sibling paths in committed files"), the decisions taken, the handoffs
dispatched, and the review findings.

## A "slice"

A slice is the smallest unit of dispatchable work. Depending on the phase:

- **Phase 0** — one spike deliverable (one question, one answer, one written
  decision).
- **Phase 1–3, 5–6** — one API area or one adapter capability.
- **Phase 4** — one IDL interface group brought under the generator.

## Per-slice loop

1. **Plan (Claude)** — ensure the relevant `specs/blocks/<area>.md` states the
   public API and its behaviour contract as numbered rules (R1..Rn). Emit a task
   handoff (template below).
2. **Implement (coding agent)** — produce code + inline unit tests; make the
   targeted gate green headless; report.
3. **Review (Claude)** — verify against the handoff's acceptance criteria:
   - every rule R1..Rn is exercised by at least one test,
   - `core/` contains no engine-specific and no backend-specific reference,
   - no panics in library code; FFI-boundary `expect` only where `CLAUDE.md`
     principle 8 allows; every `extern "C"` callback catches unwinds,
   - conventions in `CLAUDE.md` honoured, including the no-local-paths rule,
   - the gate is clean (below).

   On failure: return a **revision handoff**. Do not fix it inline — an
   orchestrator who patches the implementer's diff stops being able to review it.
4. **Integrate (Claude)** — update `specs/tracking/<topic>.md`; `git add` +
   `git commit` with a message referencing the phase and the area.

## Gates

| Gate | Command | Who |
|---|---|---|
| Build | `cargo build --workspace --features webgpu-native-js-ffi/backend-yawgpu` | both |
| Test | `cargo test --workspace --features webgpu-native-js-ffi/backend-yawgpu` | Claude (backstop); agent runs targeted subsets |
| Lint | `cargo clippy --workspace --all-targets --features webgpu-native-js-ffi/backend-yawgpu -- -D warnings` | both |
| Engine-agnostic | `cargo test -p webgpu-native-js-core` with **no** backend feature and `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` **unset** | both |
| JSC (Tier 1, Apple) | `cargo test -p javascriptcore-adapter` (no feature flag — `jsc` is default since the 2026-07-10 promotion; also runs inside the workspace gate on macOS) | both |
| Dual-engine | the slice's `.js` conformance script under **both** engines, identical expected output — concretely, the parity suite (`tests/parity/`, block 08): byte-identical, both adapters, every run | Claude |

**The engine-agnostic gate is not optional.** `core/` depends on
`webgpu-native-js-ffi` for its `bindgen` types (block 01 → R1a) but must compile
and pass its unit tests with no engine, no backend library, and no GPU. If that
gate ever needs a backend, the boundary has leaked.

**Everything else needs a backend.** `webgpu-native-js-ffi` builds with zero
backend features as a **types-only** crate — it emits no link directives — and
`compile_error!`s only on *more than one*. Any crate that actually **calls**
`webgpu.h` needs one backend feature and needs
`WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` set to a directory containing the backend's
dynamic library (see `specs/reference/dependencies.md`). Wire it up in a
**gitignored** `.cargo/config.toml`, never in a committed file.

For yawgpu, point it at `target/release` of a yawgpu checkout. That directory is
self-contained since D6 was fixed upstream: it colocates `libtint_shim.dylib`,
and `libyawgpu.dylib` resolves it via `@loader_path`.

**Spikes are split.** `spikes/event-loop-pump` and `spikes/release-queue` depend on
`ffi` and are **workspace members**, gated with `-p <name> --features
ffi/backend-yawgpu`. `spikes/jsc-detach` and `spikes/quickjs-detach` have no
workspace dependency and are **excluded**, gated individually with
`cargo test --offline --manifest-path spikes/<name>/Cargo.toml`.

**Headless-first** (`CLAUDE.md` principle 7): every gate above must pass with no
GPU and no window. Real-GPU and native-surface tests are separately gated and
never required for CI.

**Dual-engine applies only where the slice touches the adapter boundary.** A
pure `core/` conversion slice is verified against the mock `JsEngine`.

## Task handoff template

Claude produces one of these per slice, recorded in the area's
`specs/tracking/<topic>.md`.

```
## Task: <area> — <short goal>

Phase: <N>
Goal: <one line>

Inputs to read:
- specs/blocks/<block>.md  (rules R1..Rn)
- specs/reference/workflow.md, CLAUDE.md

Produce:
- <crate>/src/<file>.rs   (+ inline #[cfg(test)] mod tests)
- <script-level test, if the API is JS-visible>

Out of scope: real GPU, native surface, unrelated APIs, spec edits, commits.

Acceptance criteria:
- [ ] each rule R1..Rn exercised by >= 1 test
- [ ] headless: passes with no GPU, no window
- [ ] core/ has zero engine-specific and zero backend-specific references
- [ ] no panics in library code; extern "C" callbacks catch unwinds
- [ ] no local or sibling filesystem paths in any committed file
- [ ] clippy clean with -D warnings

Report back: files changed, rules intentionally deferred (+ why), gate output.
```

## Coding-agent command execution (output-polling constraint)

Inherited from yawgpu, where it was root-caused. The coding agent runs in
**codex**, whose `exec_command` is asynchronous: it launches the process, then
drains stdout in **30-second polling windows** with limited output per chunk. A
command that streams a burst of output fills the stdout pipe buffer and **blocks
on `write()` until codex reads it 30 s later** (pipe back-pressure). This
throttles throughput by roughly two orders of magnitude — in yawgpu, a
workspace test run that took ~25 s in a normal shell took 30–73 min inside
codex, entirely from this drain. It is **not** build time and **not** cargo lock
contention.

**Rule:** in a codex handoff, any long-running or verbose command must redirect
output to a file and report only the exit code and a short tail:

```
cargo test --workspace > "$TMPDIR/out.log" 2>&1; echo "EXIT=$?"; tail -n 40 "$TMPDIR/out.log"
```

A test-name filter does **not** avoid the cost: `cargo test -p <pkg> <filter>`
still spawns every test binary in the package, each printing `running 0 tests`,
so codex polls through dozens of flushes. Use `--test <binary> <filter>` to run
one binary, or just redirect to a file.

**Claude** runs the full `cargo test --workspace` on review directly via its own
Bash (no polling harness), so it remains the backstop.

## Phase Review (mandatory — "Clean Review Then Fix")

Every phase ends with a **mandatory Phase Review** before it can be marked
COMPLETE. Per-slice review (Claude, full session context) catches slice-local
issues; the Phase Review catches **accumulated / cross-slice** issues that a
context-primed reviewer rationalizes away.

1. **Clean Review (fresh agent, no session context).** Claude spawns a subagent
   with **no conversation history**. It is given only: the phase's cumulative
   `git diff`, the phase's `specs/blocks/<area>.md`, `CLAUDE.md`, this document,
   and the phase exit criteria. It does **not** see the conversation or the prior
   rationale. It produces **severity-tagged findings**, each with `file:line` +
   rationale:
   - **CRITICAL** — memory unsafety/UB, soundness, FFI ABI mismatch, a panic
     reachable from the C ABI or unwinding across the JS engine boundary on
     valid input, a spec rule silently wrong, data loss, a dangling pointer
     handed to script.
   - **MAJOR** — a rule not actually enforced, missing/empty test coverage for a
     rule, an engine- or backend-specific reference leaking into `core/`, a
     convention breach with real impact, a resource/refcount leak, a local path
     in a committed file.
   - **MINOR** — naming, dead code, redundant work, doc/comment gaps,
     non-idiomatic but correct code.
2. **Triage (Claude).** Drop false positives with a one-line written reason;
   keep the rest. Anything dropped is recorded in the area's tracking doc.
3. **Fix in severity order.** CRITICAL first, then MAJOR, then MINOR.
   Production-code fixes go to the coding agent via a **fix handoff** (Claude
   does not write production code); spec fixes are Claude's. Re-run the full gate
   after each severity tier.
4. **Gate.** A phase cannot be marked COMPLETE while any **CRITICAL** or
   **MAJOR** finding is open. **MINOR** may be deferred only with an explicit
   written rationale logged in the area's tracking doc.
5. **Log.** The tracking doc records the finding list with severities +
   `file:line`, triage decisions, fix commits, and the final gate result. Commit:
   `phase-N: phase review — <n> findings (<c> CRITICAL / <m> MAJOR / <k> MINOR) fixed`.

The Clean Review reviewer is a throwaway subagent per phase, with no memory of
previous phases beyond what the diff shows. This is deliberate.

**Reviewers that mutate the tree get their own `git worktree`.** Block 03's review
ran three lenses in parallel over one working tree. One of them proved a guard by
deleting it and re-running; another, running its suite at that moment, saw the
resulting failure and reported it as a **non-deterministic flake in the guard's
own test** — a finding it could then never reproduce in fifty attempts.

The reported failure was `a21_rejects_offsets…: "mapAsync offset=2^32 must be
rejected"`, which is *exactly and only* the assertion that fails when that guard
is removed. It was not a flake. It was another reviewer's experiment.

An experiment that deletes a guard is the most valuable thing a Clean Review can
do (block 03's fifth tautology was found that way). It must not be paid for with a
phantom defect in someone else's report. **Give any reviewer licensed to edit the
tree an isolated worktree, or run it alone.**

### The JSC phase carries an extra exit gate

Per `CLAUDE.md`, wiring the JavaScriptCore adapter must require **zero changes to
`core/`'s logic** — only additive `JsEngine` trait methods or capability
declarations. Non-trivial core churn means the engine boundary was drawn wrong.
**Stop and revisit before scaling up codegen**; do not absorb the churn. Adding
a trait method or a capability variant is additive and does not trip this gate.

## Version control

Repository initialized 2026-07-09 on branch `main`. Claude commits per slice;
the coding agent never commits.

Commit message convention: `phase-N: <area> — <short>`, e.g.
`phase-0: engine-boundary — resolve JSC ArrayBuffer detach`.

Network operations (`git push` / `git pull`, submodule fetches) are invoked by
the **user** via the `!` prompt, never by Claude with the sandbox disabled.
