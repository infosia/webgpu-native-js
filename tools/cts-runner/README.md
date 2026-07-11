# CTS runner

This crate runs selected cases from an owner-provided, built WebGPU CTS
checkout. The CTS is not vendored and the runner never assumes a checkout at a
particular repository-relative location.

Point `--cts-path` or the `CTS_PATH` environment variable at the CTS built-JS
output directory, then select cases with repeatable `--query` arguments or a
query-list `--suite` file:

```text
cargo run -p cts-runner -- --cts-path <built-cts-output> --query '<query>'
```

`--suite` paths are resolved relative to the process's current working
directory. From the repository root, run the Phase A self-test suite with:

```text
CTS_PATH=<built-cts-output> cargo run -p cts-runner -- --suite tools/cts-runner/suites/unittests.txt
```

The tested WebGPU CTS revision is
`e8389d86fc5dbb97839f26697e44181a601dc8c9`, Dawn's `DEPS` pin used in
lockstep by the oracle protocol. Build that checkout with:

```text
npm ci && npm run standalone
```

The built output is expected to contain clean ESM under `CTS_PATH`, including
`common/internal/file_loader.js`, `common/internal/query/parseQuery.js`,
`common/internal/logging/{logger,log_message}.js`, and
`common/framework/test_config.js`. The host registers exact aliases only for
those glue entry modules. Imports inside the CTS remain relative to their CTS
importers, including the loader's dynamic `../../<suite>/listing.js` and
`../../<suite>/<file>.spec.js` imports.
The five aliases in `src/main.rs` are validated against the pinned revision
and its standalone built-output layout.

The runner installs only headless shims: `navigator.gpu`, monotonic
`performance.now()`, console output, host-drained timers, UTF-8
`TextEncoder`/`TextDecoder`, `DOMException`, `EventTarget`/`MessageEvent`, and
the `self` alias. Each used shim is emitted as a `shim:` diagnostic. Timers are
kept in a JavaScript min-heap; while module evaluation is pending, the host
checks due timers before each WebGPU/microtask tick. Canvas, the DOM, and fetch
are intentionally absent.

## Exit codes and summary

| Exit code | Meaning |
|---|---|
| 0 | Every selected case passed or matched its explicit expectation; list mode found at least one case. |
| nonzero | A case failed, warned without an expectation, unexpectedly passed, mismatched its expectation, timed out, or the query/list selection was empty. |

A skip under an expected-fail rule is deliberately an expectation mismatch
and therefore fails the run. This is stricter than C2's letter: it keeps stale
expectations visible instead of silently accepting a test that no longer
exercises its recorded failure. The summary prints all eight counters: pass,
fail, skip, warn, expected-fail, unexpected-pass, expectation-mismatch, and
unexpected-warn.
