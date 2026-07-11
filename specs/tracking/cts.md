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
standalone` by the owner. Recorded in tools/cts-runner/README.md (verified present after the review fixes).

### Runner shape (as landed)

A-1: crate `tools/cts-runner` — CLI (--query/--suite/--expectations/--list/
--timeout-secs), host fns (__report/__list/print/__perf_now/__log_shim),
eval_module + tick until completion, exit-code table (unexpected-pass = fail).
A-2: real glue (parseQuery → loadCases → Logger → per-case run → __report),
shims per C3, five exact aliases. Planner decision: `call_global_function`
was NOT restored — JS→host reporting suffices (recorded against the block's
inventory note).

### Phase A review (one focused lens) — closed 2026-07-12

0 CRITICAL / 3 MAJOR / 8 MINOR. The MAJORs: the README pin was missing while
this file claimed it existed (the recurring record-honesty class — fixed, and
the false claim above is annotated rather than erased); the acceptance suite
file was not committed (now `suites/unittests.txt` — the 1,031-case run is
reproducible from the tree); and `setInterval(0)` looped forever inside one
eval, unreachable by `--timeout-secs` (repeating timers now re-arm after the
drain with a fresh now — a bare-Runtime regression test pins it). Selected
minors fixed: clearTimeout no-op semantics + cancellation-set hygiene, the
exit-code deviation from C2's letter documented as deliberate
(skip-under-expected-fail = mismatch, stale-expectation hygiene), all eight
summary counters printed, empty `--list` exits nonzero, the Bool/String/Null
host-return paths tested. Recorded approximations (from the review, kept):
expectations are Rust-side query-prefix matching, NOT the framework's
subcase-level expectations — a case failing 1 of 100 subcases can only be
expected wholesale (revisit if Phase B needs finer grain); glue/shims are
covered by the live CTS run plus targeted shim unit tests, not by a full
offline harness — acceptable for the spike, stated here. expectations.txt
deliberately does not exist yet (unittests needed zero entries); Phase B
creates it with the codegen-deltas-derived initial population per C5.

## Phase B — headless validation subset

**B-1 landed (2026-07-12): requiredFeatures/requiredLimits plumbed (C7).**
The block-10 recorded gap closes: requestDevice converts the feature-name
sequence through the generated enum join and the requiredLimits RECORD type
(a new WebIDL shape — string-keyed open dictionary) through a new additive
`JsEngine::own_property_names` primitive (both engines + mock, per J13).
Unknown feature → TypeError; unknown limit key → OperationError (spec wording
quoted from the pin); undefined values → the header's UINT32/64_MAX
sentinels; compatibility chain mirrored from block 10 in reverse. Timestamp
query sets now creatable under a requesting device (tested); the parity
features line finally observes ordering with two features (block 10's
rescoped I7 claim can un-rescope) — 123 lines, byte-identical on yawgpu AND
Dawn (gated run: Dawn's Metal adapter advertises timestamp-query, confirmed).
timestampWrites conversion itself stays skipped with an updated reason —
both IDL timestamp dicts join one shared C struct, a name-map shape deferred
to its own slice. Suites: core 138, JSC 29+1.
