# CTS runner — tracking (block 13)

## Phase A — bootstrap: COMPLETE (2026-07-12)

**Result: `unittests:*` — 1,031/1,031 pass, exit 0, ~10s warm (~102
cases/second).** The CTS framework's own self-test suite runs entirely inside
QuickJS on this binding's module loader and shims.

### The §5 questions, answered empirically

1. **Build output layout**: the standalone build (`npm run standalone`)
   emits clean ESM with explicit `.js` import extensions under `out/`.
   Framework entries used by the glue: `common/internal/file_loader.js`
   (DefaultTestFileLoader), `common/internal/query/parseQuery.js`,
   `common/internal/logging/logger.js`, `common/framework/test_config.js`.
   Dynamic imports (listings/specs) are importer-relative
   (`../../<suite>/listing.js`) — **zero loader changes needed**; block 12's
   machinery (aliases exact-match for the five glue entries, everything else
   importer-relative) just worked. The transform hook was NOT needed —
   Babel's output runs untransformed on QuickJS-ng.
2. **JS feature gaps**: **none at the language level.** Missing were Web
   GLOBALS only: EventTarget, MessageEvent (minimal shims), plus the planned
   timers/performance/console/TextEncoder/TextDecoder/DOMException set.
   Shims actually exercised by `unittests:*`: performance.now,
   MessageEvent, setTimeout, TextEncoder.encode, EventTarget, DOMException.
3. **Device acquisition/pooling**: not yet exercised (unittests need no
   WebGPU) — Phase B question.
4. **Directory listing**: not needed — the CTS's generated `listing.js`
   modules carry the enumeration; the loader reads files only, and that
   suffices.
5. **Throughput**: ~102 cases/second on the unittests suite (interpreter,
   debug build). Sizing note for Phase B: a 10k-case validation suite ≈
   ~2 minutes at this rate — CI-viable; measure again on the real suite.

### Pins

CTS checkout at Dawn's DEPS pin (lockstep with the oracle protocol):
`e8389d86` (local short: e8389d86fc5). Built with `npm ci && npm run
standalone` by the owner. Recorded in tools/cts-runner/README.md.

### Runner shape (as landed)

A-1: crate `tools/cts-runner` — CLI (--query/--suite/--expectations/--list/
--timeout-secs), host fns (__report/__list/print/__perf_now/__log_shim),
eval_module + tick until completion, exit-code table (unexpected-pass = fail).
A-2: real glue (parseQuery → loadCases → Logger → per-case run → __report),
shims per C3, five exact aliases. Planner decision: `call_global_function`
was NOT restored — JS→host reporting suffices (recorded against the block's
inventory note).
