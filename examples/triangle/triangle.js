globalThis.ready = false;

Promise.resolve()
    .then(function () {
        const shader = device.createShaderModule({
            code: `
                struct VertexOutput {
                    @builtin(position) position: vec4f,
                    @location(0) color: vec3f,
                };

                @vertex
                fn vertexMain(@builtin(vertex_index) vertexIndex: u32) -> VertexOutput {
                    const positions = array(
                        vec2f( 0.0,  0.7),
                        vec2f(-0.7, -0.6),
                        vec2f( 0.7, -0.6),
                    );
                    const colors = array(
                        vec3f(1.0, 0.2, 0.2),
                        vec3f(0.2, 1.0, 0.3),
                        vec3f(0.2, 0.4, 1.0),
                    );
                    var output: VertexOutput;
                    output.position = vec4f(positions[vertexIndex], 0.0, 1.0);
                    output.color = colors[vertexIndex];
                    return output;
                }

                @fragment
                fn fragmentMain(input: VertexOutput) -> @location(0) vec4f {
                    return vec4f(input.color, 1.0);
                }
            `,
        });
        const pipeline = device.createRenderPipeline({
            layout: "auto",
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
        const encoder = device.createRenderBundleEncoder({
            colorFormats: [globalThis.surfaceFormat],
        });
        encoder.setPipeline(pipeline);
        encoder.draw(3);
        globalThis.triangleBundle = encoder.finish();
        globalThis.ready = true;
    })
    .catch(function (error) {
        print("triangle initialization failed:", error);
    });
