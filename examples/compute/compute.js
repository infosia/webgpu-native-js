globalThis.ok = false;
globalThis.done = false;

function fail(error) {
    print("compute failed:", error);
    globalThis.ok = false;
    globalThis.done = true;
}

gpu.requestAdapter()
    .then(function (adapter) {
        return adapter.requestDevice();
    })
    .then(function (device) {
        const input = new Uint32Array([1, 2, 3, 4, 5, 6, 7, 8]);
        const byteLength = input.byteLength;
        const storage = device.createBuffer({
            size: byteLength,
            usage: 128 | 8 | 4,
        });
        const readback = device.createBuffer({
            size: byteLength,
            usage: 1 | 8,
        });
        device.queue.writeBuffer(storage, 0, input);

        const shader = device.createShaderModule({
            code: `
                @group(0) @binding(0)
                var<storage, read_write> values: array<u32>;

                @compute @workgroup_size(1)
                fn main(@builtin(global_invocation_id) id: vec3<u32>) {
                    values[id.x] = values[id.x] * 2u;
                }
            `,
        });
        const bindGroupLayout = device.createBindGroupLayout({
            entries: [
                {
                    binding: 0,
                    visibility: 4,
                    buffer: { type: "storage" },
                },
            ],
        });
        const bindGroup = device.createBindGroup({
            layout: bindGroupLayout,
            entries: [
                {
                    binding: 0,
                    resource: { buffer: storage },
                },
            ],
        });
        const pipelineLayout = device.createPipelineLayout({
            bindGroupLayouts: [bindGroupLayout],
        });
        const pipeline = device.createComputePipeline({
            layout: pipelineLayout,
            compute: { module: shader },
        });

        const encoder = device.createCommandEncoder();
        const pass = encoder.beginComputePass();
        pass.setPipeline(pipeline);
        pass.setBindGroup(0, bindGroup);
        pass.dispatchWorkgroups(input.length);
        pass.end();
        encoder.copyBufferToBuffer(storage, 0, readback, 0, byteLength);
        device.queue.submit([encoder.finish()]);

        return device.queue.onSubmittedWorkDone().then(function () {
            return readback.mapAsync(1, 0, byteLength).then(function () {
                const result = new Uint32Array(readback.getMappedRange(0, byteLength));
                const numbers = Array.from(result);
                print("result:", numbers.join(", "));
                readback.unmap();
                storage.destroy();
                readback.destroy();
                globalThis.ok = true;
                globalThis.done = true;
            });
        });
    })
    .catch(fail);
