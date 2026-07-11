import { DefaultTestFileLoader } from "cts/file_loader";
import { parseQuery } from "cts/parse_query";
import { Logger } from "cts/logger";
import { prettyPrintLog } from "cts/log_message";
import { globalTestConfig } from "cts/test_config";
import { GPUConst } from "cts/webgpu_constants";

// The binding accepts the standard numeric flags but does not expose the
// browser-global constant namespaces. Use the pinned CTS's canonical values.
for (const [name, value] of Object.entries({
  GPUBufferUsage: GPUConst.BufferUsage,
  GPUTextureUsage: GPUConst.TextureUsage,
  GPUColorWrite: GPUConst.ColorWrite,
  GPUShaderStage: GPUConst.ShaderStage,
  GPUMapMode: GPUConst.MapMode,
})) {
  if (!(name in globalThis)) {
    globalThis[name] = value;
    __shimLog(name);
  }
}

globalTestConfig.testHeartbeatCallback = () => {};

const loader = new DefaultTestFileLoader();
const logger = new Logger();

for (const queryText of globalThis.__query) {
  const cases = await loader.loadCases(parseQuery(queryText));
  for (const testcase of cases) {
    const name = testcase.query.toString();
    if (globalThis.__listOnly) {
      __list(name);
      continue;
    }
    const [recorder, result] = logger.record(name);
    await testcase.run(recorder, []);
    const message = (result.logs ?? []).map(prettyPrintLog).join("\n");
    __report(name, result.status, message);
  }
}
