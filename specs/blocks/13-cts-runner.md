# Block 13 — the upstream TS CTS runner (full phase plan)

Owner-approved direction (2026-07-12). This document is deliberately
self-contained: **the next manager agent may not be the one who wrote it**,
so it carries the context, the inventory, the phase plan, the operational
handbook, and the traps. Read this whole file before dispatching anything.

---

## 0. Why this exists (context for a fresh agent)

This project is a JS binding: it presents the `webgpu.h` C ABI as
WebGPU-shaped JavaScript inside native engines (Boa Tier 1 everywhere;
JavaScriptCore Tier 1 on Apple platforms). The bug class that defines the
project is *"the binding mis-converts a descriptor"* — invisible to backend
test suites, because it never reaches the C ABI in a distinguishable way.

The natural oracle for that bug class is the **upstream WebGPU CTS**
(https://github.com/gpuweb/cts) — written in TypeScript, therefore able to
run *inside the engine under test*. This is exactly what dawn.node does one
engine up (Node instead of Boa). CLAUDE.md has named this the end state
since day one ("Testing the binding layer").

**The oracle logic (why failures are attributable):** the Dawn backend was
promoted to **Oracle** status (CLAUDE.md → Backend support tiers → "The
oracle protocol", 2026-07-12). Our `webgpu-headers` pin IS Dawn's `DEPS` pin,
Dawn passes both engines' full suites with byte-identical parity, and it is
the conformance oracle of webgpu-native-cts itself. Therefore:

- CTS case fails against **Dawn** → presumed **binding bug** (investigate the
  divergence point first — the D11 lesson, see backend-deltas.md — never
  assume).
- CTS case passes on Dawn but fails against **yawgpu** → backend finding,
  catalogued in `specs/tracking/backend-deltas.md` and handed to the owner
  via the yawgpu handoff flow. Never worked around in the binding.
- Known, recorded binding deviations (see `specs/tracking/codegen-deltas.md`)
  are carried in an **expectations file**, never silently.

**Scale honesty:** the full CTS is >1.6M subcases. A JIT-less interpreter
will not run it all, and does not need to. This block is subset-driven from
the first line: meaningful slices, grown like the parity suite (one query at
a time), with the full sweep never a goal.

---

## 1. Inventory — what already exists (verified landed, with names)

A fresh agent should verify these still exist before planning work
(`grep` the names; do not trust this list over the tree):

| Capability | Where | Notes |
|---|---|---|
| ES module evaluation | `adapters/boa/src/lib.rs` → `Runtime::eval_module(path)` | Returns a completion handle (`ModuleEvaluation`: Pending/Fulfilled); **top-level await completes through the ordinary `tick()`**. |
| Bare-specifier aliases | `Runtime::set_module_alias(specifier, path)` | Exact-match wins first; alias target is the probe base. |
| Source transform hook | `Runtime::set_module_transform(f)` | Runs on EVERY module source (root + imports) before compilation. The TS enabler: the binding ships no transpiler; a runner may plug one. Err(msg) → load error naming the path. Trusted host; must not re-enter the runtime. |
| Resolution probing | module loader internals, same file | as-is → `.js` → `.mjs` → `/index.js`, importer-relative; miss error lists every probe. **Module identity is lexically normalized** (`./`/`..` collapsed) — the block 12 review's MAJOR; do not regress it. |
| Host functions | `Runtime::register_host_function(name, f)` with `HostValue` (String/Number/Bool/Null/Undefined) | block 11 (X2). `console.log`/`performance.now` ride this. |
| Per-frame / call-in | `Runtime::call_global_function(name, &[HostValue])` | Exists on main? **VERIFY** — it was written on the deleted Three.js branch (slice 2, commit 6cd9d96, NOT cherry-picked). If absent, it is Phase A work (small; the design is in the deleted branch's description in `specs/tracking/engine-boundary.md` history and in git reflog if still reachable — otherwise re-derive: one JS call, HostValue args, R26 error surfacing). |
| WebGPU JS surface | `core/src/lib.rs` + `codegen/policy.toml` (generated) | buffers, textures, samplers, bind groups, pipelines (compute+render), passes, bundles, querySets, error scopes (`pushErrorScope`/`popErrorScope`/GPUError classes), device events (`onuncapturederror`, `device.lost`), `features`(real Set)/`limits`/`adapterInfo`, `getBindGroupLayout`. |
| Async plumbing | J1/J2 architecture: pure-Rust WebGPU callbacks → settlement queue → one-JS-frame trampoline → microtasks → releases, all inside `tick()` | The CTS's promise-heavy style rides this. |
| Engine parity | `tests/parity/` (122 lines, byte-identical, 2 engines × yawgpu/Dawn) | The CTS runner is Boa-only (JSC has no module C API — recorded in block 12 → M4), but core conversions are engine-generic, so CTS coverage benefits JSC transitively. |
| Gates & culture | `specs/reference/workflow.md` | Per-slice loop, codex discipline, **gate on exit codes** (crash-shaped failures print no FAILED line), review culture (three lenses + deletion experiments), phase reviews. |

**Known-missing items that become load-bearing here:**

1. **`requiredFeatures`/`requiredLimits` are unplumbed** in `requestDevice`
   (hard-coded `requiredFeatureCount: 0`) — recorded in
   `specs/tracking/codegen-deltas.md` → "Block 10 additions". Many CTS tests
   request features/limits. This is Phase B's prerequisite slice.
2. **No DOMException hierarchy** — rejections are plain Error objects with
   `name`/`message` (recorded deviation). CTS asserts DOMException types in
   places → expectations entries, or a shim-level DOMException class.
3. **`GPUUncapturedErrorEvent` shape** — handler receives the bare GPUError
   (recorded deviation).
4. **enforce_u64 accepts up to 2^64−1** where WebIDL caps at 2^53−1
   (recorded). CTS boundary tests will hit this → expectations entries or a
   revisit.
5. Whatever JS language features the CTS framework needs beyond Boa's
   coverage — **unknown until Phase A measures it**.

---

## 2. Rules (C1–C10)

**C1 — the CTS is never vendored.** The owner provides a checkout and builds
it (`npm` is owner-run tooling; network is owner-run). The runner locates it
via `CTS_PATH` (env var or `--cts-path`), pointing at the CTS's **built
JS output directory** (start with the standalone ESM build, `out/`; adjust to
reality in Phase A and record what the real layout is). The tested CTS
revision is pinned in the runner's README once Phase A passes. No sibling or
absolute path ever appears in a committed file (CLAUDE.md rule; absolute).

**C2 — the runner is a workspace binary crate, `tools/cts-runner`** (bin
name `cts-runner`), backend passthrough features like the examples
(default `backend-yawgpu`), `test = false`, with the rpath `build.rs`
(the examples' lesson: link-args do not propagate). CLI shape:
`cts-runner --cts-path <dir> --query '<cts query>' [--expectations <file>]
[--list]`. Exit code: 0 iff every selected case passed or was expected-fail /
skipped; nonzero otherwise. **Gate on the exit code.**

**C3 — the shim prelude is the runner's, minimal, and honest.**
`tools/cts-runner/shims.js`: `navigator.gpu` over the existing `gpu` global;
timers (setTimeout family — a queue drained by the host loop between ticks);
`performance.now` (host fn); `console.*` (host print); TextEncoder/Decoder
(pure JS, USVString-correct surrogate handling — the block 08 semantics);
`self`/`globalThis` aliases; a minimal `DOMException` class (name/message —
closes deviation #2 at the shim layer, honestly labeled). Every shim call is
recordable (a `__shimLog`) so Phase A's catalogue is data, not memory.
**No canvas, no DOM, no fetch** — CTS cases needing them are out of scope
(skip-listed), matching dawn.node's own posture.

**C4 — the runner's JS glue drives the CTS's own framework.** Do not fork
the CTS. The glue imports the CTS's `common/framework` /
`common/internal` modules (whatever the built tree exposes — Phase A
discovers the real entry points; dawn.node's `src/dawn/node/cts.cjs` and the
CTS's `src/common/runtime/cmdline.ts` are the reference shapes, both read
from their upstream repos, cited by URL + repo-relative path only), builds a
test query, iterates cases, and reports `{query, status, message}` per case
through a host function. Status vocabulary follows the CTS's own
(pass/fail/skip/warn).

**C5 — expectations are data, reviewed like policy.** `tools/cts-runner/
expectations.txt` (committed): one entry per line — query prefix + expected
status + a MANDATORY reason string. The initial population is exactly the
recorded deviations in `codegen-deltas.md` (DOMException where the shim
class is insufficient, GPUUncapturedErrorEvent shape, enforce_u64 2^53,
IteratorClose, read-order, mutable-Set). An expectations entry without a
reason fails the run (the G5 discipline, applied here). An UNEXPECTED pass
(expected-fail that passes) is reported loudly so stale entries die.

**C6 — subsets are named, versioned lists.** `tools/cts-runner/suites/
<name>.txt` — query lists (one per line, comments allowed). Phase A ships
`unittests.txt`; Phase B ships `validation-core.txt` (the curated
api,validation subset); growth = adding lines (the parity-suite economy).
CI runs named suites only; ad-hoc queries are for investigation.

**C7 — requiredFeatures/requiredLimits plumbing is a real slice, not runner
glue.** `requestDevice`'s descriptor conversion gains `requiredFeatures`
(sequence of feature-name enum strings → `WGPUFeatureName` array via the
generated join) and `requiredLimits` (the limits dict → `WGPULimits` with
the chain, absent members → the C "undefined limit" sentinels — verify
`WGPU_LIMIT_U32_UNDEFINED`/`U64_UNDEFINED` in the pinned header). Tests per
principle 1 (mock: array + sentinel fields asserted; script: a device
requesting a feature reports it in `device.features`; parity line for a
deterministic shape). Side effects to claim in the same slice: timestamp
query sets become creatable (the block-10 recorded gap) and the parity
features line can finally observe ordering with ≥2 features (rescoped I7
claim in block 10 — un-rescope it if this lands).

**C8 — backend roles per the oracle protocol** (CLAUDE.md): CI/headless =
yawgpu Noop (validation-class suites only — Noop validates fully, executes
nothing); gated real-GPU = Dawn (the arbiter; failures presumed binding
bugs); yawgpu-real-GPU optional (differences vs Dawn → owner handoff);
wgpu-native explicitly NOT a CTS target (Tier 2, known panicky gaps D8/D9).

**C9 — every phase ends with the standard review** (three lenses + deletion
experiments where guards exist; see workflow.md → Phase Review), and the
catalogue lives in `specs/tracking/cts.md`: framework-feature gaps found,
shims added, expectations added/retired, suite growth, timing data
(cases/second — spike-quality, labeled), and per-phase go/no-go notes.

**C10 — the standing boundaries bind unchanged.** Additive APIs only; core
stays engine-generic; no JSC changes (Boa runner recorded); no
sibling/absolute paths; commits per slice by the manager; **the owner runs
all network operations (npm, git push/pull) — the manager NEVER pushes**
(CLAUDE.md + workflow.md; this rule has history — read the workflow section
"Network git operations").

---

## 3. Phase plan (dispatch-sized)

### Phase A — bootstrap: the framework runs
*Goal: `cts-runner --query 'unittests:*'` (or the narrowest green unittest
query) exits 0. `unittests:*` are the CTS framework's self-tests — they need
NO WebGPU, isolating: module graph loading, shims, and the runner loop.*

- **A1**: verify/restore `call_global_function` (inventory note); scaffold
  `tools/cts-runner` (crate, CLI parsing, README skeleton). Owner action
  needed at this point: provide a CTS checkout + build it (`npm ci && npm run
  standalone` or whatever the CTS's current docs say — the owner confirms
  the output directory; record the revision).
- **A2**: shims.js + the JS glue: import the framework, list a query
  (`--list` prints case names — proves module graph + framework init), then
  run `unittests:` cases, host-reported results, summary + exit code.
  EXPECT iteration here: Boa language/feature gaps surface as loud
  errors — polyfill in shims or record. Timeout discipline for codex
  (30-minute ceiling — split; a dead-silent session may still have finished:
  check the tree before assuming loss).
- **A3**: catalogue (`specs/tracking/cts.md` created): what the framework
  needed, what was shimmed, cases/second on `unittests`, the pinned CTS
  revision. Review pass (one focused lens is acceptable for A; full three
  lenses at the Phase B boundary).
- **Acceptance**: unittests suite green (or failures explained in
  expectations with reasons); gates untouched-green; catalogue exists.

### Phase B — headless validation subset (the CI seed)
*Goal: a curated `webgpu:api,validation,*` suite green against yawgpu Noop,
headless, in CI-viable time.*

- **B1**: the C7 slice (requiredFeatures/requiredLimits) — FIRST, it
  unblocks device-requesting tests and closes two standing gaps.
- **B2**: adapter/device acquisition glue: the CTS acquires devices per
  test (GPUTest fixtures) — map to our `gpu.requestAdapter()` flow; device
  reuse/pooling if the CTS framework expects it (discover in A; dawn.node
  had a device pool — check its cts.cjs for the semantics we must mimic).
- **B3**: curate `validation-core.txt` — start with createBuffer /
  createTexture / createBindGroup(+Layout) / pipeline validation families
  (the areas our binding converts most). Run, triage failures three-ways:
  binding bug (fix via the normal slice loop) / recorded deviation
  (expectations entry) / CTS-needs-unsupported-API (skip entry with reason).
- **B4**: phase review (full three lenses) + CI wiring: the suite joins the
  standard gate table in workflow.md as a headless job (exit-code gated).
- **Acceptance**: named suite green in CI; every expectation reasoned; the
  binding bugs found (there WILL be some — that is the point) fixed through
  the normal review'd loop.

### Phase C — the oracle runs (gated, real GPU)
*Goal: the same suites + an api,operation starter set against Dawn; failures
triaged as presumed binding bugs.*

- **C1**: run validation suites on Dawn (gated, unsandboxed — the sandbox
  blocks Metal; this is the established practice, see workflow). Divergences
  from the Noop run = investigate (Noop-vs-Dawn behavioral differences in
  VALIDATION are themselves findings).
- **C2**: `operation-core.txt` — buffer mapping / copy round-trips /
  writeBuffer-writeTexture families (real execution; Dawn only).
- **C3**: yawgpu real-GPU optional pass; Dawn-vs-yawgpu diffs → owner
  handoff flow (D11 precedent: isolate the divergence point first).
- **Acceptance**: oracle suites green-or-expected on Dawn; at least one
  binding bug found-and-fixed via the oracle to prove the loop works (if
  literally none surface, say so — do not manufacture).

### Phase D — settle
- CI job documented in workflow.md's gate table; suite growth economy
  documented (add a line, run, triage); the catalogue's timing data updated;
  block 13 phase review; COMPLETE.

---

## 4. Operational handbook for the next manager (read workflow.md first)

- **Roles**: the manager plans, specs, reviews, runs gates, commits. The
  coding agent (codex MCP, `mcp__codex__codex`, cwd = repo root, sandbox
  `workspace-write`, approval `never`) implements. Handoffs must be
  self-contained (the agent has no session memory); demand exit codes +
  output-redirected cargo runs (codex's 30-min stdout ceiling: require
  `cmd > log 2>&1; echo EXIT=$?; tail`).
- **Gates** (each slice): workspace test (yawgpu features), core with the
  backend env var truly UNSET, both clippys `-D warnings`, fmt, parity
  byte-identical; JSC suite untouched. Judge by EXIT CODES.
- **Gated Dawn runs**: need the sandbox disabled (Metal is blocked inside
  it) and `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` pointing at a directory
  containing `libwebgpu_dawn.dylib` (a Dawn CMake build's
  `src/dawn/native`); backend feature `backend-dawn`,
  `--no-default-features` where a crate defaults to yawgpu.
- **Owner-only operations**: anything network (npm, cargo fetch for NEW
  crates, git push/pull, submodule changes) and anything on other machines.
  Ask, wait.
- **Review culture**: findings are triaged by the manager (accept/drop with
  written reasons), fixed in severity order via handoffs, recorded in
  `specs/tracking/` (phase-reviews.md for reviews; cts.md for this block).
  Deletion experiments run in isolated worktrees (`.claude/worktrees` —
  clean them up after; `git worktree prune` + branch deletion).
- **Honesty rules that have teeth here**: a test that cannot fail is a
  finding (six tautologies shipped historically); "green" proves only what
  is observed (the D11 retraction); a claim about an upstream artifact is
  not written down until the artifact was opened in-session.

## 5. Open questions Phase A must answer (do not guess in advance)

1. The CTS build output's real layout and entry modules (and whether the
   ESM build runs un-transformed on Boa or needs a downlevel step via
   the transform hook).
2. Which JS features the framework needs that Boa lacks (if any).
3. The device-acquisition/pooling semantics the fixtures expect.
4. Whether `--list`-style enumeration needs filesystem directory listing
   (the loader currently reads files only — a `read_dir` host capability may
   be needed for the CTS's listing; decide when the need is concrete).
5. Cases/second — the number that sizes every suite after A.
