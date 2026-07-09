(function () {
  var buffer = device.createBuffer({
    size: 256,
    usage: 8,
    label: "descriptor-label",
    mappedAtCreation: "",
  });
  if (buffer.label !== "descriptor-label") {
    throw new Error("descriptor label failed");
  }
  buffer.label = "staging";
  if (buffer.label !== "staging") {
    throw new Error("label round-trip failed");
  }
  if (buffer.size !== 256) {
    throw new Error("size getter failed");
  }
  if (buffer.usage !== 8) {
    throw new Error("usage getter failed");
  }
  buffer.destroy();
  if (buffer.size !== 256 || buffer.usage !== 8 || buffer.label !== "staging") {
    throw new Error("destroy changed observable buffer properties");
  }
  buffer.destroy();
}());
