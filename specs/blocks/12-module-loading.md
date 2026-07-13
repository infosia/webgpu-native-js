# Block 12 — the module loading system (the CTS runner's first brick)

Owner directive (2026-07-12): module loading oriented at running
TypeScript-authored suites (the upstream WebGPU CTS is the end-state
consumer). Rules **M1–M6**. The ES-module core (`eval_module`, alias map,
top-level-await completing through `tick()`) was implemented and verified on
a feature branch and is cherry-picked here; this block owns it going forward.

## Rules

**M1 — `eval_module(path)` + creation-time loader (landed).** Aliases resolve
first (exact specifier), then importer-relative paths; misses name specifier
AND importer; module evaluation returns a completion handle and top-level
await advances through the ordinary `tick()`. R26 error discipline throughout.

**M2 — the host transform hook is what makes TS possible.**
`Runtime::set_module_transform(fn(source: &str, path: &Path) -> Result<String,
String>)`: runs on every loaded module source BEFORE compilation (and on the
`eval_module` root). The binding ships NO transpiler — a CTS runner plugs
swc/tsc output-side tooling; tests plug marker transforms. `Err` surfaces as
a module-load error naming the path. The hook must not re-enter the runtime
(document; trusted host).

**M3 — resolution conventions for compiled-TS graphs.** After alias and
exact-path misses, probe in order: `<spec>` as-is, `<spec>.js`, `<spec>.mjs`,
`<spec>/index.js`. First hit wins; the probe list appears in the miss error.
No implicit `.ts` probing — the transform hook owns TS, resolution stays
JS-shaped (what tsc emits).

**M4 — Boa runner, recorded (corrected 2026-07-12; justification corrected again
2026-07-13 by block 16).** JSC's public C API has no module loader; the CTS path
is Boa-only.

*The fact is right and now has a citation. The justification was too narrow.*

M4 originally rationalised the gap as acceptable because *"the CTS path is
Boa-only"*. That is sound for a dev tool and wrong as a general claim: a real
game's JavaScript is multi-file, so if modules work on Android (Boa) and do not
exist on iOS (JSC), the two Tier-1 engines disagree about **how game code is
loaded at all**. Block 15 §8 surfaced it; block 16 investigated it.

**The measured answer (block 16, Phase 1; full evidence in
`specs/tracking/engine-boundary.md` → Q11):** the module API is **not absent from
JavaScriptCore — it is absent from its published surface.** `JSScript` appears in
the SDK only as a bare forward declaration (`JSContext.h`), with **no `JSScript.h`
in either the macOS or the iOS SDK**; `moduleLoaderDelegate`, `evaluateJSScript:`,
and `kJSScriptTypeModule` are absent from every header in both. Yet the SDK's
`.tbd` advertises `JSScript` as one of five Objective-C classes, so the symbol
links. That is SPI, and block 16 → L6 forbids building on it.

**Consequence, decided and recorded:** game JavaScript is delivered to the runtime
as a **single script**, bundled by the application's build. Runtime ES modules are
a **Boa-only development-tooling capability** — the CTS runner uses them — and
game code must not rely on them. Parity between the engines is then exact *by
construction* rather than by verification.

**M5 — tests per principle 1.** Existing: relative chains, alias, miss
naming, eval-throw, TLA-through-tick. Added by this block: transform hook
(identity, marker rewrite, error path naming the path), extension probing
(each probe step + the miss error listing probes), root-file transform,
alias+probe interaction.

**M6 — boundaries.** Additive adapter API only; core untouched; no
sibling/absolute paths (aliases are runtime host input); gates green; JSC
suite untouched.

## Exit criteria

1. M2/M3 land with M5's tests; all standard gates green.
2. A smoke demonstration: a two-file "TS-shaped" module graph (extensionless
   imports, a marker transform standing in for a transpiler) evaluates and
   completes through tick — the CTS runner's minimal shape, proven.
3. Review pass over the new API before closing.
