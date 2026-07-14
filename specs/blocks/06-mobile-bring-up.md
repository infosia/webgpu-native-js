# Block 06 — Mobile bring-up

**Status: IN PROGRESS (Phase 1, cross-compile). Opened 2026-07-13.**

iOS and Android are the *production* targets (CLAUDE.md → Target platforms).
Everything shipped so far has been verified on macOS. This block closes the gap
between "the code is portable in principle" and "the code builds for the
platforms it is meant to ship on".

**Owner decision (2026-07-13): cross-compilation first.** Execution and any
performance claim are explicitly *out of scope for Phase 1*. A build that does
not exist cannot be run; get the build, then decide whether running it is worth
the next slice.

**Owner decision (2026-07-13): no simulators, no emulators.** Ship targets are
physical iOS and Android devices. A simulator differs in ABI (Apple) and GPU stack
(both), so it does not verify the ship target. Execution, when it happens, is on
hardware.

Consequence: `aarch64-apple-ios-sim` and `x86_64-linux-android` are not supported
targets; code serving only them is deleted.

## Why this is tractable now, and was not before

The engine is pure Rust. Under quickjs-ng, Android meant an NDK C toolchain, an
engine `bindgen`, and a cross-compiled C build. Under Boa (block 14) it is an
ordinary `cargo build --target`. **That was one of the three stated reasons for
the engine swap** — this block is where that claim gets tested rather than
asserted.

## Scope — what must cross-compile

| Crate | iOS (`aarch64-apple-ios`) | Android (`aarch64-linux-android`) |
|---|---|---|
| `webgpu-native-js-ffi` | required | required |
| `webgpu-native-js-core` | required | required |
| `boa-adapter` (Tier 1, all platforms) | required | required |
| `javascriptcore-adapter` (Tier 1, Apple) | required | **not applicable** — compiles to an empty crate off Apple platforms; that this holds is itself a check |
| `webgpu-native-js-codegen` | host-only (build-time) | host-only |
| `cts-runner`, `examples/*`, `spikes/*` | **out of scope** — host development tools; they pull `winit` and a CTS checkout | out of scope |

**Android is 64-bit ARM only.** `armv7` is deferred until a concrete requirement
appears (adding a target is cheap once the first one works). `x86_64-linux-android`
is the *emulator* target and is out of scope by the owner decision above, not
merely deferred.

## The known blocker (measured 2026-07-13)

`cargo check -p webgpu-native-js-ffi --target aarch64-linux-android` fails:

```
third_party/webgpu-headers/webgpu.h:66:10: fatal error: 'math.h' file not found
```

`ffi/build.rs` never tells `bindgen` what it is compiling *for*. It runs clang
with the **host's** default target and include paths. On iOS this happens to work
— macOS and iOS share the arm64 ABI and the headers `webgpu.h` needs (`stdint`,
`stddef`, `math`) are layout-identical — so `aarch64-apple-ios` already builds,
**by luck rather than by construction**. On Android there is no such luck: the
NDK's sysroot is the only place its libc headers live.

## Rules

**M1 — `bindgen` is told the target explicitly.** `ffi/build.rs` passes
`--target=<triple>` derived from Cargo's own `TARGET`, for *every* target
including the host. Not "for Android"; a cross-compile that works by accident is
a bug that has not fired yet.

**M2 — The NDK is located by environment, never by a committed path.**
`ANDROID_NDK_HOME` (falling back to `ANDROID_NDK_ROOT`, the two names the NDK
itself uses) resolves the sysroot. A committed absolute path violates the
repository's no-local-paths rule and makes the build machine-specific. Same
discipline as `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR`. Absence must produce a *clear
build error naming the variable*, not a clang diagnostic about `math.h`.

**M3 — The Apple SDK is located by `xcrun`, never by a committed path.**
`xcrun --sdk iphoneos --show-sdk-path`. **Device SDK only** — there is no
simulator target (owner decision above).

**M4 — No backend library is required to cross-compile.** `webgpu-native-js-ffi`
with zero backend features is a **types-only** crate: it emits no link
directives. Cross-compiling the binding must not require an iOS or Android build
of yawgpu to exist. (Linking one is a *later* phase, and a different problem —
the backend is the host's to supply.)

**M5 — The JSC adapter's empty-crate claim is a gate, not a comment.** It must
`cargo check` for Android and produce nothing; if it does not, the `cfg` gating
is wrong and the "default feature costs nothing elsewhere" claim in CLAUDE.md is
false.

**M6 — Cross-compile joins the gate table** (`specs/reference/workflow.md`), as a
local gate like the others. There is no hosted CI (owner, 2026-07-13).

## Phase 1 — cross-compile — **DONE (2026-07-13)**

All eight checks pass by exit code, with `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR`
unset:

| | `aarch64-apple-ios` | `aarch64-linux-android` |
|---|---|---|
| `webgpu-native-js-ffi` | 0 | 0 |
| `webgpu-native-js-core` | 0 | 0 |
| `boa-adapter` | 0 | 0 |
| `javascriptcore-adapter` | 0 | 0 (empty crate) |

One root cause. `ffi/build.rs` now passes `--target=<triple>` to `bindgen` for
every target including the host (M1), resolves the Android sysroot from
`ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` (M2), and the Apple SDK from `xcrun` (M3).
No other workspace change was required; no dependency was a blocker.

**M5 verified.** The JSC adapter's Android rlib contains zero JSC symbols — the
`jsc` default feature costs nothing off Apple platforms.

**NDK sysroot verified by breaking it.** A bogus `ANDROID_NDK_HOME` fails the
build; an absent one fails with a message naming the variable. Neither falls back
to host headers (the previous behaviour, which surfaced as a `math.h` error).

### Finding — Cargo and clang do not always spell a target the same way

Rust's `aarch64-apple-ios-sim` is not a triple clang accepts; clang requires
`aarch64-apple-ios-simulator`. Cargo's `TARGET` therefore cannot always be passed
to `bindgen` verbatim.

The simulator is out of scope (owner, 2026-07-13) and the translation was deleted.
Before deleting it, both supported targets were checked: clang accepts Cargo's
spelling verbatim for each, so the translation was dead.

```
xcrun clang --target=aarch64-apple-ios     -fsyntax-only -x c /dev/null   # exit 0
xcrun clang --target=aarch64-linux-android -fsyntax-only -x c /dev/null   # exit 0
```

**Rule:** when adding a target, verify clang accepts the triple Cargo supplies. A
mismatch surfaces as a header error, not as an unknown-target error.

## Phase 2 — backend link — DONE (2026-07-14)

The binding now links against a real backend on both ship targets, and produces
executable binaries.

**yawgpu builds** (recipe from yawgpu's own README; no changes needed on either
side):

| Target | Command | Artifact |
|---|---|---|
| iOS | `cargo build --release --target aarch64-apple-ios -p yawgpu --features metal` | `libyawgpu.{dylib,a}` |
| Android | `cargo build --release --target aarch64-linux-android -p yawgpu --features vulkan` | `libyawgpu.{so,a}` |

Android additionally needs the NDK toolchain env block yawgpu's README documents
(`CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`, `CC_*`, `AR_*`).

**Binding links against it**, with `WEBGPU_NATIVE_JS_BACKEND_LIB_DIR` pointing at
the per-target directory:

| | iOS | Android |
|---|---|---|
| `boa-adapter` (link) | exit 0 | exit 0 |
| `javascriptcore-adapter` | exit 0 | exit 0 (empty crate) |
| `example-compute` (headless) | `Mach-O 64-bit executable arm64` | `ELF 64-bit LSB pie executable, ARM aarch64` |

`examples/compute` is the headless example (no window), so it is the artifact a
device run would use.

**M4 restated.** M4 says the *cross-compile gate* must not require a backend — that
still holds and is still the gate. Phase 2 is the separate question of whether a
backend, once supplied, links. It does.

**Note on the bindgen fix.** yawgpu solves the same `math.h` failure from the
outside, with `BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android` in the environment.
This project solves it inside `ffi/build.rs` (M1–M2), so no caller-side env var is
needed for the binding's own bindgen pass.

## Deferred, and recorded so it is not mistaken for done

- **Running anything.** Not done. Executable binaries exist for both targets
  (Phase 2), but nothing has been run on a device. When it happens it is **on
  hardware only** (owner: no simulators, no emulators).
- ~~**Linking a backend.**~~ **DONE (Phase 2, 2026-07-14).**
- **Performance.** Owner-deferred (2026-07-12): Boa publishes its own benchmarks;
  in-process JSC-on-iOS claims stay unwritten until measured.
- **`jsvalue-enum`.** Boa's NaN-boxing assumes a pointer alignment some platforms
  break (block 14 → B9). Whether the feature is needed is a *runtime* question
  and cannot be answered by a build. Flag it when Phase 2 opens.
- **App Store Review Guidelines 4.7** — re-verify immediately before any iOS
  release (CLAUDE.md open question); unaffected by using the system JSC.
