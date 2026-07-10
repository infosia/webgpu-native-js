# Block 05 — codegen: WebIDL ⋈ `webgpu.yml`

Phase 4. Rules **G1–G12**. Blocks 01 (R1–R27), 02 (A1–A32), 03 (B1–B22) and
04 (J1–J21) all still bind — especially R24 (no hand-coded names in adapters),
B11 (no conversion differs per engine), and principle 9 (generated code is
never hand-edited).

Every claim below about upstream artifacts was checked while writing: Dawn's
`DEPS` and `src/dawn/node/BUILD.gn` (for what dawn.node actually consumes),
and the pinned `third_party/webgpu-headers/webgpu.yml`. Reopen them; do not
restate from memory.

---

## 1. What Phase 3 proved and what this block spends it on

Block 03 measured the input this block needs: **80–85% of a descriptor
conversion is mechanical**, and the non-mechanical remainder is a short list of
*policy* decisions, each decidable once, globally (block 03 → §7). Phase 3 then
proved the target is stable: ten hard interfaces, two engines, zero per-engine
conversions, zero core churn for the second engine.

So the generator's job is precisely bounded: **emit the mechanical 80–85% of
each binding into `core/`, driven by data, under the policies the hand-written
code already established.** It does not invent design; it replays it.

## 2. Inputs and their pins

**G1 — the input is WebIDL joined with `webgpu.yml`, and both are pinned.**
`webgpu.yml` (the C ABI description, already in the pinned
`third_party/webgpu-headers`) carries no dictionary defaults, no string enums,
no flag namespaces, no `Promise` types, and no `[EnforceRange]` — WebIDL
carries all of those and no C signatures. Neither input alone can generate a
binding; `dawn.node` generates from WebIDL for exactly this reason.

- **`webgpu.idl` comes from the gpuweb/gpuweb repository**, pinned as the
  submodule `third_party/gpuweb`, at the revision **Dawn's `DEPS` pins for
  `dawn_node`**: `acaf809d9323e72429d2252e372ee4d917fc40eb`
  (https://github.com/gpuweb/gpuweb). Verified in Dawn's `BUILD.gn`:
  `dawn_webgpu_idl_path = "$dawn_root/third_party/gpuweb/webgpu.idl"` — the
  IDL sits at the gpuweb repo root. Same pin-selection policy as
  `webgpu-headers` (follow Dawn's), recorded in
  `specs/reference/dependencies.md`. This closes plan §6.4 and the CLAUDE.md
  open question.
- **The header pin wins on conflict.** Where the IDL (spec-current) describes
  API the pinned `webgpu.h` cannot express, the C ABI is ground truth: the
  interface/member is **skipped and catalogued** in
  `specs/tracking/codegen-deltas.md`, never approximated. The reverse (header
  has it, IDL does not) is expected for `wgpu*`-only affordances and is not a
  finding.

**G2 — the join is by name, and every mismatch is loud.** `GPUBufferDescriptor`
⋈ `WGPUBufferDescriptor`, `mapAsync` ⋈ `wgpuBufferMapAsync`, following
`webgpu.yml`'s own naming discipline. A member present on one side and not the
other is a generation-time **error** listed in the run's report — resolved by a
policy entry (G5), never by silent omission.

## 3. Shape of the generator

**G3 — the generator is a workspace crate, `codegen/`, and its output is a
build artifact.** `codegen/` is a library with a thin CLI; `core/build.rs`
invokes it (build-dependency) and `include!`s the output from `OUT_DIR`,
exactly as `ffi/` does with bindgen. Generated code is never committed and
never hand-edited (principle 9; repo hygiene). Fix the generator or the policy
file, not the output.

**G4 — generated code is engine-generic or it is wrong.** Output is `fn ...<E:
JsEngine>` against the same trait surface the hand-written conversions use —
no engine names, no `dyn`, no new trait methods emitted by the generator. If
an IDL construct cannot be expressed with the existing `JsEngine` surface,
that is a **planner decision** (a new capability is additive per A18/J13), not
a generator workaround.

**G5 — policy lives in one committed file, `codegen/policy.toml`, and it is
the complete list of human decisions.** Block 03 §7 enumerated what a human had
to decide that WebIDL did not say; those decisions become data:

- which chained `sType`s are accepted per descriptor (B3), and that an unknown
  chain is an error;
- which interfaces/members are **in the subset** (G6) and which are skipped
  (with a reason string, surfaced in the deltas doc);
- error routing while error scopes do not exist (B15): throw-on-null policy
  per constructor;
- nullable vs non-null string classification where the IDL and
  `webgpu.yml` must agree (B4);
- `[SameObject]` attributes (B21) and single-use consumables (B19).

A policy entry the generator does not consume is an error (dead policy is a
lie); a generated deviation without a policy entry is an error (silent
divergence). B15's rule — do not quietly diverge — becomes machine-checked.

**G6 — subset first, full coverage measured.** The first target is the
already-shipped surface (blocks 01–03: buffer, queue, shader module, bind
group family, compute pipeline, encoder, passes) plus nothing. The CLAUDE.md
open question "full WebIDL coverage vs a trimmed subset" is answered **after**
this slice, with the measured effort delta in hand — the plan's own
instruction.

## 4. The oracle

**G7 — the hand-written bindings are the generator's conformance suite.** The
first slice generates interfaces that already exist, hand-written and tested
under two engines and a Phase Review. Acceptance is behavioural equivalence:

1. the generated implementation **replaces** the hand-written one behind the
   same public names;
2. **every existing test passes unchanged** — core mock tests, both adapters'
   suites, and the byte-identical parity script;
3. the hand-written conversion code is deleted in the same slice (two copies
   of a conversion is how they drift apart).

A generated binding that needs a test weakened is a generator bug by
definition. This is the strongest oracle the project owns and it is the entire
reason to generate the known surface first.

**G8 — generator unit tests run offline on committed fixtures.** Small
hand-authored IDL + YAML fixture pairs (inputs, not generated output — fine to
commit) exercise the parser, the join, each policy kind, and each failure mode
(name mismatch, unknown sType, dead policy). Snapshot the emitted Rust for
fixtures; snapshots are test expectations, not build inputs.

**G9 — the WebIDL parser is chosen in the first slice, with evidence.**
Requirements: parses the pinned `webgpu.idl` completely (dictionaries,
interfaces with mixins/includes, enums, typedefs, namespaces, extended
attributes incl. `[EnforceRange]`/`[SameObject]`); pure Rust; maintained.
Candidate to evaluate first: the `weedle2` crate. If no crate survives contact
with the real file, a purpose-built parser for the subset webgpu.idl actually
uses is acceptable — webgpu.idl is machine-generated and regular. Record the
decision and evidence in `specs/tracking/codegen.md`.

## 5. Boundaries that keep binding

**G10 — the generator emits `core/` code only.** Adapters are already generic
(R24: driven by `ClassSpec` at runtime); they need nothing per-interface. If a
generated interface makes an adapter want per-interface code, the boundary
broke — stop and report (the J13 discipline, one phase on).

**G11 — width and ownership rules are generated from the types, not
remembered.** `uint64_t`-vs-`u32` widening rejections (R8), `size_t` narrowing
rejections (A21/B7), `[EnforceRange]` coercion, non-null vs nullable strings
(B4), required members (the DR-M3 class), native AddRef on every stored handle
(B8), `ReturnedWithOwnership` vs extra-ref returns — each is a *derivable*
property of the joined inputs, and each already has a hand-written precedent
with a named test. The generator's emission for each must cite the rule ID in
a comment, so a reviewer can grep from code to contract.

**G12 — every generated public item still gets a direct test.** Principle 1
does not exempt machines. The per-rule *mechanism* tests exist (the oracle,
G7); what each newly generated interface adds is at minimum: one happy-path
mock test and one error-path mock test per new conversion, generated alongside
the conversion or hand-written in the same slice — plus a parity-script
extension when the interface is script-observable.

## 6. Tests

- Generator: fixture suite (G8), offline, no engine, no backend.
- Equivalence: the full existing gate, unchanged, over the generated surface
  (G7) — core, both adapters, parity byte-identical.
- First new never-hand-written interface (after G7 lands): its generated tests
  (G12) plus a hand-review of the emitted code as if a coding agent had
  written it — a Phase Review lens over generated output, once, before volume.

## 7. Exit criteria

1. `third_party/gpuweb` pinned at Dawn's revision; recorded in
   `dependencies.md`.
2. The generator regenerates the block 01–03 surface; **all existing tests
   pass unchanged**; the hand-written conversions are deleted.
3. `codegen/policy.toml` is the complete, machine-checked policy list; dead
   or missing policy is a generation error.
4. Zero engine-specific and zero backend-specific lines in emitted code;
   adapters unchanged.
5. At least one interface **not** previously hand-written generates cleanly
   with its tests (the actual payoff), and the full gate stays green.
6. The full-vs-subset coverage question is answered with measured data.
7. Phase Review clean of CRITICAL and MAJOR.

## 8. Open questions this block will answer

- **Parser** (G9) — decided with evidence in the first slice.
- **Full IDL vs subset** (G6) — decided after the first slice, with numbers.
- **Does `Promise`-returning method emission need anything beyond
  `new_promise`/`SettlementQueue`?** The async surface is small
  (`mapAsync`, `onSubmittedWorkDone`, `requestAdapter`, `requestDevice`,
  Phase 6's `popErrorScope`); expect no, verify while generating `mapAsync`.
