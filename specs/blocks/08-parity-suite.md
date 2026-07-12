# Block 08 — the parity suite

Owner directive (2026-07-10): *platform parity is the priority; grow the parity
suite.* Rules **P1–P8**. This block turns J17's 12-line script into the
project's platform-parity instrument: iOS (JSC) and Android
(Boa) will agree because this suite says so, not because anyone assumed an
engine.

## 1. What the suite is for

The deletion lens recorded it plainly: *the adapter script suites validate
plumbing, never conversion values* — and conversely, mock tests can never see
an engine. The parity suite is the only instrument that observes **both real
engines running identical script** and demands identical bytes. Every line it
gains is a class of silent engine divergence retired.

**macOS is the parity laboratory** (owner observation, 2026-07-10): it is the
one platform where JavaScriptCore and Boa run side by side in the same
checkout, so every dev-machine test run exercises the exact comparison that
predicts iOS (JSC) agreeing with Android (Boa). That is what
makes desktop development *evidence* for mobile parity rather than hope — the
same argument that chose a JIT-less engine, now applied to the two-engine
world.

## 2. Rules

**P1 — one script, one expected file, byte-identical, forever.** The existing
shape stands: `tests/parity/parity.js` + `tests/parity/expected.txt`,
`include_str!` by both adapters, exact string equality. Lines are
`section:detail`, deterministic, ordered. No timing, no GC-dependent behavior
(F5), no uncaught rejections (J20), no backend nondeterminism (yawgpu Noop is
the backend for both engines, so backend-deterministic validation errors are
usable — the block 07 precedent).

**P2 — a divergence found is a finding, never a dodge.** When the two engines
produce different bytes for a candidate line, the implementing agent STOPS for
that line and reports it; the planner triages: fix (core/adapter) or record
(`codegen-deltas.md` / adapter docs) — then the line lands pinning the decided
behavior. The suite must never be shaped to avoid a divergence silently.

**P3 — coverage classes.** The suite grows to cover, at minimum:

1. **Identity semantics**: method identity within a wrapper AND across two
   wrappers of the same class (WebIDL puts methods on the prototype — one
   function object per class, not per instance); `device.queue` (B21);
   `device.lost`; error-class prototype identity.
2. **The full rejection surface**: every async rejection's `name` and message
   (mapAsync ×3 statuses distinguishable, empty-stack pop, request failures)
   — messages are core-generated, so cross-engine they must be identical.
3. **Coercion edges, one line each**: EnforceRange at 0, 2^32−1, 2^32, 2^64,
   −1, 1.5, NaN, Infinity; [Clamp] NaN→0 / ties-to-even / saturation;
   BigInt rejection everywhere a number is taken; `label` given a number, an
   object with `toString`, and `null` (→ `"null"`); required-member absence
   messages.
4. **String round-trips**: non-ASCII BMP, non-BMP (surrogate pairs), and a
   **lone surrogate** (USVString: must become U+FFFD — suspected divergence,
   P2 applies), empty string vs absent.
5. **Iterator protocol edges**: Set, generator, array-like rejection message,
   a **string** as a sequence (iterable of code points — element conversion
   must fail identically), an iterator throwing mid-walk.
6. **Mapping semantics from script**: multi-range unmap, `byteLength === 0`
   after detach on every range, post-unmap `getMappedRange` error name,
   offset window bytes, view-window writeBuffer bytes (exists).
7. **Error model**: instanceof chains for all four classes, subclass via
   `extends` (new_target), `constructor.length`, scope push/pop nesting with
   a validation error crossing an inner mismatched filter.
8. **Event/loss surface**: forwarded uncaptured class+message; lost reason
   mapping (exists) and lost-promise identity.
9. **Ordering**: the existing one-tick settle/then interleaving, plus
   `.then` chains across two ticks, `Promise.all` over two binding promises,
   and an `await`-based line (async function + ticks — the E4 class).

**P4 — `typeof` and prototype-shape lines are included deliberately.**
`typeof device.createBuffer`, `Object.getPrototypeOf(buffer) ===
Object.getPrototypeOf(otherBuffer)` — these are exactly where a callable-object
implementation detail (JSC) can leak. If they diverge, P2.

**P5 — what the suite deliberately omits**, recorded here: property
enumeration order (`Object.keys` on wrappers), error `.stack` shape, anything
GC-observable, dictionary getter side-effect ORDER beyond what core already
fixes (the read order is core-driven and therefore identical cross-engine —
one line pins that it stays so), and performance.

**P6 — the expected file is regenerated, never hand-edited**, and a one-byte
corruption must fail BOTH adapters (the existing deletion-experiment guarantee
stays demonstrable).

**P7 — the suite is a gate.** Both adapters' parity tests remain in the
default macOS gate set; the tracking doc's Phase Review sections quote the
line count as part of "parity byte-identical".

**P8 — growth is cheap by construction.** Adding a line = script + expected +
nothing else. If a candidate line needs new binding surface, it is out of
scope for this block (the line waits for the surface, not vice versa).

## 3. Exit criteria

1. The suite covers every P3 class with at least the enumerated lines
   (target: ≥60 lines from today's 12).
2. Every divergence found en route is triaged and recorded (P2) — the count
   of divergences found is itself reported in tracking.
3. Both engines byte-identical on the grown file; one-byte corruption fails
   both.
4. All prior suites unchanged.
