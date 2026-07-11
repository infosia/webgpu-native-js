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

The tested upstream revision and exact built-output layout will be pinned here
after Phase A-2 validates the real CTS framework integration.

The built output is expected to contain clean ESM under `CTS_PATH`, including
`common/internal/file_loader.js`, `common/internal/query/parseQuery.js`,
`common/internal/logging/{logger,log_message}.js`, and
`common/framework/test_config.js`. The host registers exact aliases only for
those glue entry modules. Imports inside the CTS remain relative to their CTS
importers, including the loader's dynamic `../../<suite>/listing.js` and
`../../<suite>/<file>.spec.js` imports.

The runner installs only headless shims: `navigator.gpu`, monotonic
`performance.now()`, console output, host-drained timers, UTF-8
`TextEncoder`/`TextDecoder`, `DOMException`, `EventTarget`/`MessageEvent`, and
the `self` alias. Each used shim is emitted as a `shim:` diagnostic. Timers are
kept in a JavaScript min-heap; while module evaluation is pending, the host
checks due timers before each WebGPU/microtask tick. Canvas, the DOM, and fetch
are intentionally absent.
