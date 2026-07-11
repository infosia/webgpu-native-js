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
