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
await advances through the ordinary `tick()` (verified against the vendored
quickjs source: modules compile as async functions, evaluation returns its
promise). R26 error discipline throughout.

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

**M4 — QuickJS-first, recorded.** JSC's public C API has no module loader;
the CTS path is QuickJS-only until that changes (consistent with the
JSC-runs-what-macOS-tests story: the CTS validates the BINDING's shared core,
whose conversions are engine-generic by construction).

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
