// Phase A-2 replaces this synthetic case with imports from the built CTS.
const query = "synthetic:placeholder";
const selected = globalThis.__ctsRunnerConfig.queries.some(pattern => {
  const prefix = pattern.endsWith("*") ? pattern.slice(0, -1) : pattern;
  return pattern.endsWith("*") ? query.startsWith(prefix) : query === pattern;
});
if (selected) {
  if (globalThis.__ctsRunnerConfig.list) {
    print(query);
  } else {
    __report(query, "pass", "Phase A-1 synthetic runner plumbing");
  }
}
