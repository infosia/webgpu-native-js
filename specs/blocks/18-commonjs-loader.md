# Block 18 — the CommonJS module loader (first-party, engine-neutral, Node-free)

**Status: OPEN.** A first-party way to deliver multi-file game code without a
Node/JS build toolchain, running identically on both engines.

## 1. Why this exists

Block 16 chose candidate D for game code: flatten the module graph into one
script at build time (esbuild/rollup/swc), and run that identical single script
on both engines so parity is exact by construction. That decision stands for the
*registry model* — one identical script, both engines — but it outsources the
*flattening step* to a Node toolchain, which is at odds with this project being a
portable, self-contained library (`no browser, no Node`).

This block keeps candidate D's parity-by-construction and removes the Node
dependency: the binding assembles the single script itself, from CommonJS module
sources the host supplies. It does **not** reopen block 16's candidate C (rejected
L7): candidate C gave the two engines *different* module semantics (a real ES
loader on one side, an approximation on the other). Here both engines evaluate the
**identical** bootstrap — the same require runtime and the same registry — so
there is no per-engine seam. Parity is proven the same way the pre-bundled script
was.

**Scope: CommonJS only.** `require` / `module.exports` / `exports`. ES-module
syntax (`import` / `export`) in a module source is a `SyntaxError`, because it is
module-goal-only and cannot be evaluated as a classic script. ES modules stay a
Boa-only development-tooling capability (block 12); shipping ESM without a bundler
is a separate, larger question (Rust-side ESM→registry transform) and is out of
scope here.

This is additive: build-time bundling (candidate D) still works and remains the
path for minification and tree-shaking. This block makes it optional, not required.

## 2. The delivery model

The host supplies the binding a set of named CommonJS modules and one entry id:

- **id → source**, a list of `(module id, source code)` pairs. The host chooses
  the ids (path-like strings, `/`-separated by convention).
- **entry**, the id of the module to run first.

**`core/` never touches the filesystem.** The host reads its own files or bundled
assets — on iOS/Android the host owns asset access — and passes the sources in.
This is what makes the loader portable: no filesystem assumption crosses into the
engine-neutral core.

Host-facing call (adapter `Runtime`):

```rust
runtime.run_modules(&modules, "game/main")?;   // modules: &[(String, String)]
```

`run_modules` returns the entry module's `module.exports`.

## 3. Where the logic lives

**All loader logic lives once in `core/`, engine-generic, and adds no `JsEngine`
trait method** (the adapters already expose `Runtime::eval(&self, source, name)`).

- **CM1 — `core::commonjs::RUNTIME: &str`** — the require runtime as one JS string:
  the registry, `__register(id, factory)`, and `__require(id, from)` with
  resolution, caching, and circular-import handling (§4). It is engine-neutral JS;
  both engines run the identical text.
- **CM2 — `core::commonjs::build_bootstrap(modules: &[(id, source)], entry: &str)
  -> String`** — pure Rust. Emits `RUNTIME`, then one `__register("<id>",
  function (module, exports, require, __filename) {\n<source>\n});` per module with
  the **source embedded verbatim** (no string escaping — the webpack/Metro `__d`
  shape), then `return __require("<entry>");` so the bootstrap's completion value
  is the entry exports. Only the id and entry strings are escaped, via CM3.
- **CM3 — `core::commonjs::escape_js_string(&str) -> String`** — a minimal JS
  string-literal escaper (`\\`, `"`, `\n`, `\r`, U+2028, U+2029, and other C0
  controls as `\uXXXX`). Used for ids only; sources are never escaped because they
  are embedded as function bodies, not string literals.
- **CM4 — adapter `Runtime::run_modules(&self, modules, entry)`** — the only
  per-adapter code: `self.eval(&core::commonjs::build_bootstrap(modules, entry),
  entry)`. Two lines, identical logic, using the existing `eval`.

**Verbatim embedding, not `new Function` and not string-escaped sources.** The
source is placed as a function body inside the bootstrap, so the engine compiles
all modules with the bootstrap. This needs no escaper and no `serde_json`, and the
factory is a real closure (correct scoping). The tradeoff is that a syntax error in
any module fails the whole bootstrap compile rather than only its own `require` —
acceptable for trusted first-party code (invariant 8) and how a real bundle behaves.

## 4. Runtime semantics (CM1)

- **Resolution.** Given `require(id)` from module `from`:
  1. If `id` starts with `.` and `from` is present, join `id` onto the directory of
     `from` (POSIX-style, resolving `.`/`..`, `/`-separated).
  2. Probe the result in order: exact, then `<id>.js`, then `<id>/index.js`. First
     hit in the registry wins.
  3. A miss throws `Error("Cannot find module '<id>'" [+ " from '<from>'"])` whose
     message lists the ids tried.
- **Caching + circular imports.** Before calling a module's factory, insert
  `module = { exports: {}, id }` into the cache. A `require` cycle therefore returns
  the *partial* `exports` populated so far — Node's documented behaviour. The cache
  is keyed by resolved id; a module evaluates at most once.
- **Factory contract.** `function (module, exports, require, __filename)`; the
  factory is called with `module`, `module.exports`, a `require` bound to the
  current module's id (so relative requires resolve against it), and the resolved
  id as `__filename`. `require` returns `module.exports` (live object, so late
  assignments to `module.exports` are visible per Node semantics — assigning a new
  object to `module.exports` after a cyclic require returns the old reference, which
  is the documented Node hazard, not a bug here).
- **Errors.** A module that throws during evaluation propagates through `require`
  and out of `run_modules` as the eval error. A missing module and a `SyntaxError`
  (e.g. ESM syntax) surface the same way.

## 5. Rules

- **CM5.** Both engines evaluate the byte-identical bootstrap. No per-engine branch
  in the loader; parity is by construction, and a script-level parity test proves it.
- **CM6.** `core/` contains zero filesystem access for this feature; sources arrive
  from the host as strings.
- **CM7.** No new `JsEngine` trait method. If one seems required, stop — the design
  is `build_bootstrap` (pure Rust) plus the existing `Runtime::eval`.
- **CM8.** CommonJS only; ESM syntax is a `SyntaxError`, documented, not worked around.
- **CM9.** Generated bootstrap is not written to disk or committed; it is a runtime
  string.

## 6. Acceptance

**Unit (core, pure Rust — no engine):**

1. `escape_js_string` escapes `"`, `\`, newline, CR, U+2028/U+2029, and a C0 control,
   and leaves ordinary text untouched.
2. `build_bootstrap` for two modules + entry contains `RUNTIME`, one `__register`
   per module with the source substring present verbatim, an escaped id literal, and
   a final `__require("<entry>")`; module order is the input order.
3. `build_bootstrap` with an id needing escaping produces a valid escaped literal.

**Script-level (run under BOTH engines via `run_modules`, byte-identical output):**

4. Two modules where the entry `require`s the other and logs a value from it → the
   value round-trips.
5. Relative resolution: `require("./util")` from `game/main` resolves `game/util`.
6. `index.js` resolution: `require("./lib")` resolves `lib/index.js`.
7. Circular imports: `a` requires `b`, `b` requires `a`; assert the partial-exports
   Node behaviour (the value observed on the cycle edge), identical on both engines.
8. Caching: a module required twice runs once (a side-effect counter increments once).
9. Missing module: `require("nope")` throws `Cannot find module 'nope'` with the
   tried ids in the message.
10. A module throwing at top level propagates the error out of `run_modules`.
11. ESM syntax (`export const x = 1`) in a module → `SyntaxError` (CommonJS-only).

The script-level cases live in the parity harness (or a sibling dual-engine test)
and assert **byte-identical** output under Boa and JavaScriptCore.

**Gates:** `cargo fmt --all -- --check`; core with the backend env var UNSET;
workspace test (both engines); both clippys `-D warnings`; the existing parity suite
stays byte-identical.

## 7. Documentation

Rewrite the README **JavaScript delivery** section: the first-party CommonJS loader
is the default, needs no Node and no build step (`run_modules` with host-supplied
sources); esbuild/rollup/swc become the optional path for ESM, minification, and
tree-shaking, not a requirement. Keep the note that ESM at runtime is a Boa-only
dev-tooling capability (block 12). Retire the implication that a JS build toolchain
is mandatory. The "bundling erases TDZ" caveat stays, scoped to the ESM-bundle path
it actually describes.

## 8. Relationship to blocks 12 and 16

- **Block 12** (Boa ES-module loader) is dev tooling for running TypeScript-authored
  suites; it stays Boa-only and is not a game-code delivery path. Untouched.
- **Block 16** (candidate D) chose build-time bundling; its parity-by-construction
  argument is preserved here (identical single bootstrap on both engines) and its
  candidate C rejection is respected (no per-engine module semantics). This block
  adds the Node-free first-party path candidate D outsourced.
