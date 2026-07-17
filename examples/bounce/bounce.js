globalThis.ready = false;

// Route initialization through the microtask pump and collect failures in `.catch`.
Promise.resolve()
    .then(function () {
        const N = 8;
        const BODY_FLOATS = 8;
        const WALL = 0.9;
        const bodies = [
            { x: -0.75, y: -0.65, vx: 0.42, vy: 0.31, r: 1.0, g: 0.25, b: 0.2 },
            { x: -0.50, y: -0.15, vx: 0.33, vy: -0.47, r: 1.0, g: 0.65, b: 0.15 },
            { x: -0.25, y: 0.55, vx: -0.38, vy: 0.29, r: 0.85, g: 0.9, b: 0.2 },
            { x: 0.00, y: 0.10, vx: 0.51, vy: 0.37, r: 0.2, g: 0.9, b: 0.45 },
            { x: 0.25, y: -0.45, vx: -0.44, vy: -0.35, r: 0.15, g: 0.75, b: 1.0 },
            { x: 0.70, y: 0.65, vx: 0.36, vy: -0.41, r: 0.3, g: 0.4, b: 1.0 },
            { x: 0.70, y: 0.25, vx: -0.49, vy: 0.32, r: 0.7, g: 0.3, b: 1.0 },
            { x: 0.40, y: -0.75, vx: 0.31, vy: 0.46, r: 1.0, g: 0.3, b: 0.65 },
        ];
        const staging = new Float32Array(N * BODY_FLOATS);
        const indirectArgs = new Uint32Array([6, N, 0, 0]);
        const queue = device.queue;
        let frameCount = 0;
        let alive = N;
        let visibilityCountdown = 6;

        const shader = device.createShaderModule({
            code: `
                struct Body {
                    position: vec2f,
                    halfSize: vec2f,
                    color: vec4f,
                };

                @group(0) @binding(0)
                var<storage, read> bodies: array<Body>;

                struct VertexOutput {
                    @builtin(position) position: vec4f,
                    @location(0) color: vec3f,
                };

                @vertex
                fn vertexMain(
                    @builtin(vertex_index) vertexIndex: u32,
                    @builtin(instance_index) instanceIndex: u32,
                ) -> VertexOutput {
                    const corners = array(
                        vec2f(-1.0, -1.0),
                        vec2f( 1.0, -1.0),
                        vec2f(-1.0,  1.0),
                        vec2f(-1.0,  1.0),
                        vec2f( 1.0, -1.0),
                        vec2f( 1.0,  1.0),
                    );
                    let body = bodies[instanceIndex];
                    var output: VertexOutput;
                    output.position = vec4f(
                        body.position + corners[vertexIndex] * body.halfSize,
                        0.0,
                        1.0,
                    );
                    output.color = body.color.rgb;
                    return output;
                }

                @fragment
                fn fragmentMain(input: VertexOutput) -> @location(0) vec4f {
                    return vec4f(input.color, 1.0);
                }

                @fragment
                fn fragmentMainDim(input: VertexOutput) -> @location(0) vec4f {
                    return vec4f(input.color * 0.4, 1.0);
                }
            `,
        });
        const storage = device.createBuffer({
            size: staging.byteLength,
            usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
        });
        const indirectBuffer = device.createBuffer({
            size: indirectArgs.byteLength,
            usage: GPUBufferUsage.INDIRECT | GPUBufferUsage.COPY_DST,
        });
        queue.writeBuffer(indirectBuffer, 0, indirectArgs);
        const bindGroupLayout = device.createBindGroupLayout({
            entries: [{
                binding: 0,
                visibility: GPUShaderStage.VERTEX,
                buffer: { type: "read-only-storage" },
            }],
        });
        const bindGroup = device.createBindGroup({
            layout: bindGroupLayout,
            entries: [{ binding: 0, resource: { buffer: storage } }],
        });
        const pipelineLayout = device.createPipelineLayout({
            bindGroupLayouts: [bindGroupLayout],
        });
        const basePipeline = device.createRenderPipeline({
            layout: pipelineLayout,
            vertex: {
                module: shader,
                entryPoint: "vertexMain",
            },
            fragment: {
                module: shader,
                entryPoint: "fragmentMain",
                targets: [{ format: globalThis.surfaceFormat }],
            },
            primitive: { topology: "triangle-list" },
        });
        const variantPipeline = device.createRenderPipeline({
            layout: pipelineLayout,
            vertex: {
                module: shader,
                entryPoint: "vertexMain",
            },
            fragment: {
                module: shader,
                entryPoint: "fragmentMainDim",
                targets: [{ format: globalThis.surfaceFormat }],
            },
            primitive: { topology: "triangle-list" },
        });
        const encoder = device.createRenderBundleEncoder({
            colorFormats: [globalThis.surfaceFormat],
        });
        encoder.setPipeline(basePipeline);
        encoder.setBindGroup(0, bindGroup);
        encoder.drawIndirect(indirectBuffer, 0);
        globalThis.bounceBundle = encoder.finish();
        globalThis.bundleGeneration = 1;

        // update reuses fixed objects and staging arrays because GC pressure is the significant JIT-less per-frame cost, not arithmetic.
        globalThis.update = function (dt) {
            for (let i = 0; i < N; i += 1) {
                const body = bodies[i];
                body.x += body.vx * dt;
                body.y += body.vy * dt;
                if (body.x < -WALL || body.x > WALL) {
                    body.vx = -body.vx;
                }
                if (body.y < -WALL || body.y > WALL) {
                    body.vy = -body.vy;
                }
                const offset = i * BODY_FLOATS;
                staging[offset] = body.x;
                staging[offset + 1] = body.y;
                staging[offset + 2] = 0.075;
                staging[offset + 3] = 0.075;
                staging[offset + 4] = body.r;
                staging[offset + 5] = body.g;
                staging[offset + 6] = body.b;
                staging[offset + 7] = 1.0;
            }
            frameCount += 1;
            if (frameCount > 30 && frameCount <= 60) {
                visibilityCountdown -= 1;
                if (visibilityCountdown === 0) {
                    alive -= 1;
                    visibilityCountdown = 6;
                }
            } else if (frameCount > 60 && frameCount <= 90) {
                visibilityCountdown -= 1;
                if (visibilityCountdown === 0) {
                    if (alive < N) {
                        alive += 1;
                    }
                    visibilityCountdown = 6;
                }
            }
            queue.writeBuffer(storage, 0, staging);
            indirectArgs[1] = alive;
            queue.writeBuffer(indirectBuffer, 0, indirectArgs);

            if (frameCount === 45) {
                const supersededBundle = globalThis.bounceBundle;
                const replacementEncoder = device.createRenderBundleEncoder({
                    colorFormats: [globalThis.surfaceFormat],
                });
                replacementEncoder.setPipeline(variantPipeline);
                replacementEncoder.setBindGroup(0, bindGroup);
                replacementEncoder.drawIndirect(indirectBuffer, 0);
                globalThis.bounceBundle = replacementEncoder.finish();
                globalThis.bundleGeneration += 1;
                signalBundleSwap();
                supersededBundle.destroy();
            }

            if (
                globalThis.verify
                && (frameCount === 30
                    || frameCount === 45
                    || frameCount === 60
                    || frameCount === globalThis.VERIFY_FRAMES)
            ) {
                print("checkpoint", frameCount, alive, globalThis.bundleGeneration);
            }
            if (globalThis.verify && frameCount === globalThis.VERIFY_FRAMES) {
                for (let i = 0; i < N; i += 1) {
                    const body = bodies[i];
                    print(body.x, body.y);
                }
            }
            return undefined;
        };
        globalThis.ready = true;
    })
    .catch(function (error) {
        print("bounce initialization failed:", error);
    });
