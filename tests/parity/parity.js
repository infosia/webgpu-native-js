(function () {
    "use strict";

    globalThis.parityLog = [];
    globalThis.parityDone = false;
    globalThis.parityReadyForDeviceLoss = false;

    var finished = false;
    var labelBuffer;
    var retainedTextureView;
    var retainedBindGroup;
    var parityQuerySet;
    var uncapturedEventLines = [];

    function removedUncapturedListener() {
        log("event:removed-listener:called");
    }
    device.addEventListener("uncapturederror", function (event) {
        uncapturedEventLines.push("event:listener:" + event.type + ":" + event.error.constructor.name + ":" +
            (event instanceof GPUUncapturedErrorEvent) + "," +
            (event instanceof Event) + "," + event.defaultPrevented);
        event.preventDefault();
        uncapturedEventLines.push("event:prevented:" + event.defaultPrevented);
    });
    device.addEventListener("uncapturederror", removedUncapturedListener);
    device.removeEventListener("uncapturederror", removedUncapturedListener);
    device.onuncapturederror = function (event) {
        uncapturedEventLines.push("event:handler:" + event.error.message + ":" + event.defaultPrevented);
    };

    function log(line) {
        globalThis.parityLog.push(String(line));
    }

    var frameContractLines = [];
    var frameContractOrder = [];
    var frameContractPostThrow = false;
    Promise.resolve().then(function () {
        frameContractOrder.push("pre");
    });
    globalThis.frameContractUpdate = function () {
        frameContractOrder.push("update");
        Promise.resolve().then(function () {
            frameContractOrder.push("post");
        });
    };
    globalThis.frameContractAsync = async function () {};
    globalThis.frameContractThenable = function () {
        return { then: function () {} };
    };
    globalThis.frameContractThrow = function () {
        enqueueFrameContractRelease();
        Promise.resolve().then(function () {
            frameContractPostThrow = true;
        });
        throw new Error("frame contract throw");
    };
    globalThis.frameContractRecordOrder = function () {
        frameContractLines.push("frame:order:" + frameContractOrder.join(","));
    };
    globalThis.frameContractRecordError = function (caseName, variant) {
        frameContractLines.push("frame:" + caseName + ":" + variant);
    };
    globalThis.frameContractRecordThrow = function (variant, released, remaining) {
        frameContractLines.push("frame:throw:" + variant + ":" +
            frameContractPostThrow + ":" + released + ":" + remaining);
    };

    function fail(error) {
        if (finished) {
            return;
        }
        finished = true;
        log("ERROR:" + String(error && error.name) + ":" + String(error && error.message));
        globalThis.parityDone = true;
    }

    function bytesOf(range) {
        return Array.prototype.join.call(new Uint8Array(range), ",");
    }

    function bytesOfView(view) {
        return Array.prototype.join.call(view, ",");
    }

    function caught(action) {
        try {
            action();
            return null;
        } catch (error) {
            return error;
        }
    }

    // Static ESM-only input makes esbuild hoist and concatenate the modules into
    // one flat scope. It emits no registry or helper for this input shape, and
    // lowers every top-level let, const, and class to var. The source bindings
    // have lexical TDZs, exercised in nested scopes because this fixture is a
    // shipped script, but those top-level TDZs do not survive bundling.
    (function runFlatStaticEsmBundle() {
        var flatThrowCallCount = 0;

        var sourceLetTdzError = caught(function () {
            return sourceLet;
            let sourceLet = "source-let-ready";
        });
        var sourceConstTdzError = caught(function () {
            return sourceConst;
            const sourceConst = "source-const-ready";
        });
        var sourceClassTdzError = caught(function () {
            return SourceClass;
            class SourceClass {}
        });
        var flatLetBeforeInitialization = flatLet;
        var flatConstBeforeInitialization = flatConst;
        var flatClassBeforeInitialization = FlatClass;

        var flatCircleB = "b-ready";
        log("bundle:flat:esm:b saw a = " + flatCircleA);

        var flatCircleA = "a-ready";
        log("bundle:flat:esm:a saw b = " + flatCircleB);

        log("bundle:flat:esm:entry " + flatCircleA + " " + flatCircleB);

        var flatThrown = caught(flatThrow);
        if (!flatThrown) {
            throw new Error("flat bundled function did not throw");
        }
        log("bundle:flat:throw:hoisted-function:" + (flatThrowCallCount === 1) + ":" +
            flatThrowCallCount + ":" + flatThrown.name + ":" + flatThrown.message);

        var flatLet = "let-ready";
        var flatConst = "const-ready";
        var FlatClass = class {};
        log("bundle:flat:tdz-erasure:source-lexical=" +
            (sourceLetTdzError ? sourceLetTdzError.name : "none") + "," +
            (sourceConstTdzError ? sourceConstTdzError.name : "none") + "," +
            (sourceClassTdzError ? sourceClassTdzError.name : "none") +
            ":shipped-bundle=" +
            flatLetBeforeInitialization + "," + flatConstBeforeInitialization + "," +
            flatClassBeforeInitialization + ":initialized=" + flatLet + "," +
            flatConst + "," + typeof FlatClass);

        function flatThrow() {
            flatThrowCallCount += 1;
            throw new Error("flat bundle failure");
        }
    }());

    // CommonJS input, or ESM importing a CommonJS dependency, makes esbuild emit
    // its __commonJS helper and __require closures. The helper caches before
    // evaluation for cycles and resets `mod` when a factory throws.
    (function runCommonJsBundle() {
        var __getOwnPropNames = Object.getOwnPropertyNames;
        var __commonJS = function (callback, mod) {
            return function __require() {
                try {
                    return mod || (0, callback[__getOwnPropNames(callback)[0]])(
                        (mod = { exports: {} }).exports,
                        mod
                    ), mod.exports;
                } catch (error) {
                    throw mod = 0, error;
                }
            };
        };

        var cjsOrder = [];
        var cjsSharedEvaluationCount = 0;
        var cjsThrowEvaluationCount = 0;

        var requireCjsShared = __commonJS({
            "cjs/shared.js": function (exports) {
                cjsOrder.push("shared");
                cjsSharedEvaluationCount += 1;
                exports.value = "shared";
            }
        });
        var requireCjsLeft = __commonJS({
            "cjs/left.js": function (exports) {
                cjsOrder.push("left");
                exports.shared = requireCjsShared();
            }
        });
        var requireCjsRight = __commonJS({
            "cjs/right.js": function (exports) {
                cjsOrder.push("right");
                exports.shared = requireCjsShared();
            }
        });
        var requireCjsCircleA = __commonJS({
            "cjs/circle-a.js": function (exports) {
                cjsOrder.push("circle-a");
                exports.phase = "a-initialising";
                var circleB = requireCjsCircleB();
                exports.sawB = circleB.phase;
                exports.phase = "a-ready";
            }
        });
        var requireCjsCircleB = __commonJS({
            "cjs/circle-b.js": function (exports) {
                cjsOrder.push("circle-b");
                exports.phase = "b-initialising";
                var circleA = requireCjsCircleA();
                exports.sawA = circleA.phase;
                exports.phase = "b-ready";
            }
        });
        var requireCjsThrows = __commonJS({
            "cjs/throws.js": function (exports) {
                cjsOrder.push("throws");
                cjsThrowEvaluationCount += 1;
                exports.phase = "before-throw";
                throw new Error("cjs module failure " + cjsThrowEvaluationCount);
            }
        });
        var requireCjsThrowChain = __commonJS({
            "cjs/throw-chain.js": function (exports) {
                cjsOrder.push("throw-chain");
                exports.value = requireCjsThrows().value;
            }
        });
        // esbuild leaves the entry inline, after its static CommonJS imports.
        var left = requireCjsLeft();
        var right = requireCjsRight();
        var circleA = requireCjsCircleA();
        var circleB = requireCjsCircleB();
        cjsOrder.push("entry");
        log("bundle:cjs:memoised:" + (left.shared === right.shared) + ":" +
            cjsSharedEvaluationCount + ":" + left.shared.value + "," +
            right.shared.value);
        log("bundle:cjs:circle:" + circleA.phase + "," + circleB.phase + "," +
            circleA.sawB + "," + circleB.sawA);

        var firstThrown = caught(requireCjsThrowChain);
        var secondThrown = caught(requireCjsThrows);
        if (!firstThrown || !secondThrown) {
            throw new Error("CommonJS throwing module handed out partial exports");
        }
        log("bundle:cjs:throw:rerun:" + (cjsThrowEvaluationCount === 2) +
            ":new-error:" + (firstThrown !== secondThrown) + ":" +
            cjsThrowEvaluationCount + ":" + firstThrown.name + ":" +
            firstThrown.message + ":" + secondThrown.name + ":" +
            secondThrown.message);
        log("bundle:cjs:order:" + cjsOrder.join(","));
    }());

    // A dynamic import() makes esbuild emit its lazy __esm helper. The helper
    // clears `fn` before evaluation, memoises a thrown error, and rethrows that
    // same error without re-running the initializer.
    (function runEsmHelperBundle() {
        var __getOwnPropNames = Object.getOwnPropertyNames;
        var __esm = function (fn, res, err) {
            return function __init() {
                if (err) {
                    throw err[0];
                }
                try {
                    return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
                } catch (error) {
                    throw err = [error], error;
                }
            };
        };

        var esmOrder = [];
        var esmSharedEvaluationCount = 0;
        var esmThrowEvaluationCount = 0;
        var esmShared;
        var esmLeftShared;
        var esmRightShared;
        var esmCircleA;
        var esmCircleB;
        var esmCircleASawB;
        var esmCircleBSawA;

        var initEsmShared = __esm({
            "esm/shared.js": function () {
                esmOrder.push("shared");
                esmSharedEvaluationCount += 1;
                esmShared = "shared";
            }
        });
        var initEsmLeft = __esm({
            "esm/left.js": function () {
                initEsmShared();
                esmOrder.push("left");
                esmLeftShared = esmShared;
            }
        });
        var initEsmRight = __esm({
            "esm/right.js": function () {
                initEsmShared();
                esmOrder.push("right");
                esmRightShared = esmShared;
            }
        });
        var initEsmCircleA = __esm({
            "esm/circle-a.js": function () {
                initEsmCircleB();
                esmOrder.push("a");
                esmCircleA = "a-ready";
                esmCircleASawB = esmCircleB;
                log("bundle:esm:circle:a-saw-b:" + esmCircleASawB);
            }
        });
        var initEsmCircleB = __esm({
            "esm/circle-b.js": function () {
                initEsmCircleA();
                esmOrder.push("b");
                esmCircleB = "b-ready";
                esmCircleBSawA = esmCircleA;
                log("bundle:esm:circle:b-saw-a:" + esmCircleBSawA);
            }
        });
        var initEsmThrows = __esm({
            "esm/throws.js": function () {
                esmOrder.push("throws");
                esmThrowEvaluationCount += 1;
                throw new Error("esm module failure");
            }
        });
        var initEsmThrowChain = __esm({
            "esm/throw-chain.js": function () {
                initEsmThrows();
            }
        });
        // esbuild leaves the entry inline; only its lazy modules use __esm.
        esmOrder.push("entry");
        initEsmLeft();
        initEsmRight();
        initEsmCircleA();
        initEsmCircleB();
        log("bundle:esm:circle:entry:" + esmCircleA + "," + esmCircleB);
        log("bundle:esm:memoised:" + (esmSharedEvaluationCount === 1) + ":" +
            esmSharedEvaluationCount + ":" + esmLeftShared + "," + esmRightShared);

        var firstThrown = caught(initEsmThrowChain);
        var secondThrown = caught(initEsmThrows);
        if (!firstThrown || !secondThrown) {
            throw new Error("__esm throwing initializer did not throw");
        }
        log("bundle:esm:throw:same-error:" + (firstThrown === secondThrown) +
            ":evaluate-once:" + (esmThrowEvaluationCount === 1) + ":" +
            esmThrowEvaluationCount + ":" + secondThrown.name + ":" +
            secondThrown.message);
        log("bundle:esm:order:" + esmOrder.join(","));
    }());

    log("namespace:GPUBufferUsage:object:" +
        (typeof GPUBufferUsage === "object"));
    log("namespace:GPUBufferUsage:VERTEX:" + GPUBufferUsage.VERTEX);
    var originalVertexUsage = GPUBufferUsage.VERTEX;
    caught(function () { GPUBufferUsage.VERTEX = 0; });
    log("namespace:GPUBufferUsage:readonly:" +
        (GPUBufferUsage.VERTEX === originalVertexUsage));
    log("namespace:GPUBufferUsage:enumerable:" +
        Object.keys(GPUBufferUsage).length);
    var vertexDescriptor = Object.getOwnPropertyDescriptor(GPUBufferUsage, "VERTEX");
    log("namespace:GPUBufferUsage:constant-descriptor:" +
        vertexDescriptor.writable + "," + vertexDescriptor.enumerable + "," +
        vertexDescriptor.configurable);
    var namespaceDescriptor = Object.getOwnPropertyDescriptor(globalThis, "GPUBufferUsage");
    log("namespace:GPUBufferUsage:global-descriptor:" +
        namespaceDescriptor.writable + "," + namespaceDescriptor.enumerable + "," +
        namespaceDescriptor.configurable);
    log("namespace:GPUBufferUsage:tag:" +
        Object.prototype.toString.call(GPUBufferUsage));
    log("interface:GPURenderPassEncoder:function:" +
        (typeof GPURenderPassEncoder === "function"));
    log("interface:GPURenderPassEncoder:setBindGroup:" +
        ("setBindGroup" in GPURenderPassEncoder.prototype));
    var supportedLimitsConstructorError = caught(function () {
        new GPUSupportedLimits();
    });
    log("interface:GPUSupportedLimits:illegal-constructor:" +
        (supportedLimitsConstructorError instanceof TypeError) + ":" +
        supportedLimitsConstructorError.message);
    var interfaceEncoder = device.createCommandEncoder();
    var interfaceComputePass = interfaceEncoder.beginComputePass();
    log("interface:GPUComputePassEncoder:instanceof:" +
        (interfaceComputePass instanceof GPUComputePassEncoder));
    interfaceComputePass.end();
    device.queue.submit([interfaceEncoder.finish()]);

    function errorLine(section, error) {
        return section + ":" + error.name + ":" + error.message;
    }

    function validationScope(section, action) {
        // This helper observes validation failures only; OOM and internal failures are outside its filter.
        device.pushErrorScope("validation");
        return Promise.resolve(action()).then(function () {
            return device.popErrorScope();
        }).then(function (error) {
            log("scope:" + section + ":" +
                (error === null ? "null" : error.constructor.name));
        });
    }

    function createAndDestroyBuffer(size, usage) {
        var buffer = device.createBuffer({ size: size, usage: usage });
        buffer.destroy();
    }

    function runOrdering() {
        var orderingBuffer = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.MAP_READ
        });
        var sameTickOrder = [];
        var mapPromise = orderingBuffer.mapAsync(GPUMapMode.READ, 0, 4).then(function () {
            sameTickOrder.push("mapAsync");
        });
        var workPromise = device.queue.onSubmittedWorkDone().then(function () {
            sameTickOrder.push("workDone");
        });

        return Promise.all([mapPromise, workPromise]).then(function () {
            log("order:same-tick:" + sameTickOrder.join(","));
            log("order:promise-all:resolved");
            orderingBuffer.unmap();
            orderingBuffer.destroy();

            var chainOrder = [];
            return gpu.requestAdapter().then(function () {
                chainOrder.push("first");
                return gpu.requestAdapter();
            }).then(function () {
                chainOrder.push("second");
                log("order:then-chain:" + chainOrder.join(","));
            });
        }).then(function () {
            var awaitFirst = false;
            var awaitSecond = false;

            async function awaitAcrossTicks() {
                await gpu.requestAdapter();
                awaitFirst = true;
                await gpu.requestAdapter();
                awaitSecond = true;
            }

            return awaitAcrossTicks().then(function () {
                log("order:await:" + awaitFirst + "," + awaitSecond);
            });
        });
    }

    function runRequestedFeatureOrdering() {
        return gpu.requestAdapter().then(function (adapter) {
            var requested = ["timestamp-query", "core-features-and-limits"];
            for (var i = 0; i < requested.length; i++) {
                if (!adapter.features.has(requested[i])) {
                    throw new Error("parity feature unavailable: " + requested[i]);
                }
            }
            return adapter.requestDevice({ requiredFeatures: requested });
        }).then(function (requestedDevice) {
            log("features:requested:" + Array.from(requestedDevice.features).join(","));
            requestedDevice.pushErrorScope("validation");
            var timestampQuerySet = requestedDevice.createQuerySet({
                type: "timestamp",
                count: 4
            });
            var target = requestedDevice.createTexture({
                size: [1],
                format: "rgba8unorm",
                usage: GPUTextureUsage.RENDER_ATTACHMENT
            });
            var encoder = requestedDevice.createCommandEncoder();
            var renderPass = encoder.beginRenderPass({
                colorAttachments: [{
                    view: target.createView(),
                    loadOp: "clear",
                    storeOp: "store"
                }],
                timestampWrites: {
                    querySet: timestampQuerySet,
                    beginningOfPassWriteIndex: 0,
                    endOfPassWriteIndex: 1
                }
            });
            renderPass.end();
            var computePass = encoder.beginComputePass({
                timestampWrites: {
                    querySet: timestampQuerySet,
                    beginningOfPassWriteIndex: 2,
                    endOfPassWriteIndex: 3
                }
            });
            computePass.end();
            requestedDevice.queue.submit([encoder.finish()]);
            return requestedDevice.popErrorScope().then(function (error) {
                log("timestampWrites:render-compute:" +
                    (error === null ? "null" : error.constructor.name));
                timestampQuerySet.destroy();
                target.destroy();
                requestedDevice.destroy();
            });
        });
    }

    function runErrorScopes() {
        var scopedBuffer = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_DST });
        device.pushErrorScope("validation");
        device.pushErrorScope("out-of-memory");
        device.queue.writeBuffer(scopedBuffer, 8, new Uint8Array(4));

        return device.popErrorScope().then(function (inner) {
            return device.popErrorScope().then(function (outer) {
                log("scope:nested:" +
                    (inner === null ? "null" : inner.constructor.name) + "," +
                    (outer === null ? "null" : outer.constructor.name));
                scopedBuffer.destroy();
            });
        }).then(function () {
            return device.popErrorScope().then(function () {
                throw new Error("empty pop unexpectedly resolved");
            }, function (error) {
                var stablePrefix = "popErrorScope failed:";
                if (error.message.indexOf(stablePrefix) !== 0) {
                    throw new Error("popErrorScope rejection message prefix mismatch: " +
                        error.message);
                }
                log("reject:popErrorScope:" + error.name + ":" +
                    stablePrefix + "[backend-detail]");
            });
        });
    }

    function runOffsetWindowRoundTrip() {
        var source = device.createBuffer({
            size: 12,
            usage: GPUBufferUsage.COPY_SRC,
            mappedAtCreation: true
        });
        var destination = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var window = source.getMappedRange(8, 4);
        new Uint8Array(window).set([21, 22, 23, 24]);
        source.unmap();

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(source, 8, destination, 0, 4);
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return destination.mapAsync(GPUMapMode.READ, 0, 4);
        }).then(function () {
            log("mapping:offset-window:" + bytesOf(destination.getMappedRange()));
            destination.unmap();
            source.destroy();
            destination.destroy();
        });
    }

    function runMappedAtCreationRoundTrip() {
        var mapped = device.createBuffer({
            size: 8,
            usage: GPUBufferUsage.COPY_SRC,
            mappedAtCreation: true
        });
        var readback = device.createBuffer({
            size: 8,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var mappedRange = mapped.getMappedRange(0, 4);
        new Uint8Array(mappedRange).set([7, 8, 9, 10]);
        mapped.unmap();

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(mapped, 0, readback, 0, 8);
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return readback.mapAsync(GPUMapMode.READ, 0, 8);
        }).then(function () {
            log("mappedAtCreation:" + bytesOf(readback.getMappedRange()));
            readback.unmap();
            mapped.destroy();
            readback.destroy();
        });
    }

    function runMappingDetach() {
        var mapped = device.createBuffer({
            size: 12,
            usage: GPUBufferUsage.COPY_SRC,
            mappedAtCreation: true
        });
        var first = mapped.getMappedRange(0, 4);
        var second = mapped.getMappedRange(8, 4);
        mapped.unmap();
        log("mapping:detach:" + first.byteLength + "," + second.byteLength);
        var postUnmapError = caught(function () {
            mapped.getMappedRange();
        });
        log(errorLine("mapping:post-unmap", postUnmapError));
        mapped.destroy();

        var sizeTwoError = caught(function () {
            device.createBuffer({
                size: 2,
                usage: GPUBufferUsage.MAP_WRITE,
                mappedAtCreation: true
            });
        });
        log("coerce:mappedAtCreation-size-2:" + sizeTwoError.name);

        var hugeMappedError = caught(function () {
            device.createBuffer({
                size: 9007199254740984,
                usage: GPUBufferUsage.COPY_SRC,
                mappedAtCreation: true
            });
        });
        if (hugeMappedError === null || hugeMappedError.name !== "RangeError") {
            throw new Error("huge mappedAtCreation buffer did not throw RangeError");
        }
        log("mapping:mappedAtCreation-oom:" + hugeMappedError.name);

        var destroyed = device.createBuffer({ size: 4, usage: GPUBufferUsage.MAP_READ });
        destroyed.destroy();
        device.pushErrorScope("validation");
        return destroyed.mapAsync(GPUMapMode.READ, 0, 4).then(function () {
            throw new Error("destroyed mapAsync unexpectedly resolved");
        }, function (destroyedMapError) {
            log(errorLine("reject:mapAsync", destroyedMapError));
            return device.popErrorScope();
        }).then(function (validationError) {
            if (!(validationError instanceof GPUValidationError)) {
                throw new Error("destroyed mapAsync did not emit validation");
            }
            return runMappedAtCreationRoundTrip().then(runOffsetWindowRoundTrip);
        });
    }

    function runDeviceDestroyDetach() {
        return gpu.requestAdapter().then(function (adapter) {
            return adapter.requestDevice();
        }).then(function (mappedDevice) {
            var buffer = mappedDevice.createBuffer({
                size: 4,
                usage: GPUBufferUsage.COPY_SRC,
                mappedAtCreation: true
            });
            var range = buffer.getMappedRange();
            mappedDevice.destroy();
            log("mapping:device-destroy-detach:" + range.byteLength);
        });
    }

    function runMappedRangeOverlap() {
        var buffer = device.createBuffer({ size: 16, usage: GPUBufferUsage.MAP_READ });
        var overlap;
        return buffer.mapAsync(GPUMapMode.READ, 0, 16).then(function () {
            buffer.getMappedRange(0, 8);
            overlap = caught(function () {
                buffer.getMappedRange(4, 4);
            });
            if (overlap === null || overlap.name !== "OperationError") {
                throw new Error("overlapping mapped range did not throw OperationError");
            }

            buffer.getMappedRange(8, 0);
            buffer.getMappedRange(8, 8);
            buffer.unmap();
            return buffer.mapAsync(GPUMapMode.READ, 0, 16);
        }).then(function () {
            buffer.getMappedRange(0, 8);
            buffer.unmap();
            buffer.destroy();
            log("mapping:overlap:" + overlap.name + ":reset");
        });
    }

    function runWriteBufferRoundTrip() {
        var source = device.createBuffer({
            size: 12,
            usage: GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST
        });
        var destination = device.createBuffer({
            size: 12,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var shortExplicitDestination = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var shortDefaultDestination = device.createBuffer({
            size: 12,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var longDefaultDestination = device.createBuffer({
            size: 8,
            usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST
        });
        var emptySource = device.createBuffer({
            size: 0,
            usage: GPUBufferUsage.COPY_SRC
        });
        var emptyDestination = device.createBuffer({
            size: 0,
            usage: GPUBufferUsage.COPY_DST
        });
        var bytes = new ArrayBuffer(8);
        new Uint8Array(bytes).set([3, 1, 4, 1, 5, 9, 2, 6]);
        device.queue.writeBuffer(source, 0, bytes, 0, 8);
        var viewBacking = new ArrayBuffer(8);
        new Uint8Array(viewBacking).set([99, 98, 8, 5, 3, 0, 97, 96]);
        device.queue.writeBuffer(source, 8, new Uint8Array(viewBacking, 2, 4));

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(source, 0, destination, 0, 12);
        encoder.copyBufferToBuffer(source, shortExplicitDestination, 4);
        encoder.copyBufferToBuffer(source, shortDefaultDestination);
        encoder.copyBufferToBuffer(source, 4, longDefaultDestination, 0);
        encoder.copyBufferToBuffer(emptySource, emptyDestination);
        var overloadError = caught(function () {
            encoder.copyBufferToBuffer(source, 0, destination);
        });
        if (overloadError === null || overloadError.name !== "TypeError") {
            throw new Error("three-argument numeric overload did not throw TypeError");
        }
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return destination.mapAsync(GPUMapMode.READ, 0, 12);
        }).then(function () {
            var range = destination.getMappedRange();
            var result = new Uint8Array(range);
            log("writeBuffer:" + bytesOfView(result.subarray(0, 8)));
            log("writeBuffer view:" + bytesOfView(result.subarray(8, 12)));
            destination.unmap();
            return shortExplicitDestination.mapAsync(GPUMapMode.READ, 0, 4);
        }).then(function () {
            log("copyBufferToBuffer:3-explicit:" + bytesOfView(
                new Uint8Array(shortExplicitDestination.getMappedRange())
            ));
            shortExplicitDestination.unmap();
            return shortDefaultDestination.mapAsync(GPUMapMode.READ, 0, 12);
        }).then(function () {
            log("copyBufferToBuffer:3-default:" + bytesOfView(
                new Uint8Array(shortDefaultDestination.getMappedRange())
            ));
            shortDefaultDestination.unmap();
            return longDefaultDestination.mapAsync(GPUMapMode.READ, 0, 8);
        }).then(function () {
            log("copyBufferToBuffer:5-default:" + bytesOfView(
                new Uint8Array(longDefaultDestination.getMappedRange())
            ));
            longDefaultDestination.unmap();
            log("copyBufferToBuffer:zero-default:ok");
            log("copyBufferToBuffer:no-match:" + overloadError.name);
            source.destroy();
            destination.destroy();
            shortExplicitDestination.destroy();
            shortDefaultDestination.destroy();
            longDefaultDestination.destroy();
            emptySource.destroy();
            emptyDestination.destroy();
        });
    }

    function runMapAsyncErrorRouting() {
        var mapped = device.createBuffer({ size: 4, usage: GPUBufferUsage.MAP_READ });
        return mapped.mapAsync(GPUMapMode.READ, 0, 4).then(function () {
            device.pushErrorScope("validation");
            var repeat = mapped.mapAsync(GPUMapMode.READ, 0, 4).then(function () {
                throw new Error("mapped mapAsync unexpectedly resolved");
            }, function (error) {
                return error;
            });
            return Promise.all([repeat, device.popErrorScope()]);
        }).then(function (results) {
            var rejection = results[0];
            var validation = results[1];
            if (rejection.name !== "OperationError" ||
                !(validation instanceof GPUValidationError)) {
                throw new Error("mapped mapAsync routing mismatch");
            }
            log("mapAsync:mapped:" + rejection.name + ":" +
                validation.constructor.name);
            mapped.unmap();
            mapped.destroy();

            var canceled = device.createBuffer({ size: 4, usage: GPUBufferUsage.MAP_READ });
            var pending = canceled.mapAsync(GPUMapMode.READ, 0, 4);
            canceled.unmap();
            return pending.then(function () {
                throw new Error("canceled mapAsync unexpectedly resolved");
            }, function (error) {
                if (error.name !== "AbortError") {
                    throw new Error("canceled mapAsync rejection mismatch");
                }
                log("mapAsync:cancel:" + error.name);
                canceled.destroy();
            });
        });
    }

    function runMapStateParity() {
        var buffer = device.createBuffer({ size: 4, usage: GPUBufferUsage.MAP_READ });
        log("mapState:" + buffer.mapState);
        var pending = buffer.mapAsync(GPUMapMode.READ, 0, 4);
        log("mapState:" + buffer.mapState);
        return pending.then(function () {
            log("mapState:" + buffer.mapState);
            buffer.unmap();
            log("mapState:" + buffer.mapState);
            buffer.destroy();
        });
    }

    function runDepthSliceParity() {
        function record(texture, depthSlice) {
            var attachment = {
                view: texture.createView(),
                loadOp: "clear",
                storeOp: "store"
            };
            if (depthSlice !== undefined) {
                attachment.depthSlice = depthSlice;
            }
            var encoder = device.createCommandEncoder();
            var pass = encoder.beginRenderPass({ colorAttachments: [attachment] });
            pass.end();
            encoder.finish();
        }

        var texture2d = device.createTexture({
            size: [1, 1, 1], dimension: "2d", format: "rgba8unorm",
            usage: GPUTextureUsage.RENDER_ATTACHMENT
        });
        var texture3d = device.createTexture({
            size: [1, 1, 1], dimension: "3d", format: "rgba8unorm",
            usage: GPUTextureUsage.RENDER_ATTACHMENT
        });
        device.pushErrorScope("validation");
        record(texture2d, 0xffffffff);
        return device.popErrorScope().then(function (error) {
            log("depthSlice:2d-max:" + (error === null ? "null" : error.constructor.name));
            device.pushErrorScope("validation");
            record(texture3d, 0);
            return device.popErrorScope();
        }).then(function (error) {
            log("depthSlice:3d-zero:" + (error === null ? "null" : error.constructor.name));
            texture2d.destroy();
            texture3d.destroy();
        });
    }

    function runLockedEncoderValidation() {
        var encoder = device.createCommandEncoder();
        var firstPass = encoder.beginComputePass();
        device.pushErrorScope("validation");
        var secondPassError = caught(function () {
            encoder.beginComputePass();
        });
        if (secondPassError !== null) {
            throw new Error("second pass while locked threw: " + secondPassError);
        }
        firstPass.end();
        var finishError = caught(function () {
            encoder.finish();
        });
        if (finishError !== null) {
            throw new Error("invalid locked encoder finish threw: " + finishError);
        }
        return device.popErrorScope().then(function (error) {
            if (!(error instanceof GPUValidationError)) {
                throw new Error("locked encoder finish did not emit validation");
            }
            log("encoder:second-pass-while-open:" +
                error.constructor.name + ":returned");
        });
    }

    function runSubmitErrorRouting() {
        var encoder = device.createCommandEncoder();
        var command = encoder.finish();
        device.queue.submit([command]);
        device.pushErrorScope("validation");
        var exception = caught(function () {
            device.queue.submit([command]);
        });
        if (exception !== null) {
            throw new Error("double submit threw: " + exception);
        }
        return device.popErrorScope().then(function (error) {
            if (!(error instanceof GPUValidationError)) {
                throw new Error("double submit did not emit validation");
            }
            log("submit:double:" + error.constructor.name + ":returned");
        });
    }

    function runDestroyedErrorSuppression() {
        var encoder = device.createCommandEncoder();
        var command = encoder.finish();
        device.queue.submit([command]);
        device.pushErrorScope("validation");
        device.destroy();
        globalThis.parityReadyForDeviceLoss = true;

        var submitException = caught(function () {
            device.queue.submit([command]);
        });
        var finishException = caught(function () {
            encoder.finish();
        });
        if (submitException !== null || finishException !== null) {
            throw new Error("lost-device validation path threw");
        }
        return device.lost.then(function () {
            return device.popErrorScope();
        }).then(function (error) {
            if (error !== null) {
                throw new Error("lost-device validation was surfaced");
            }
            log("destroy:validation-suppressed:null:returned");
        });
    }

    function runIterators() {
        var setEncoder = device.createCommandEncoder();
        var setCommand = setEncoder.finish();
        device.queue.submit(new Set([setCommand]));
        log("sequence Set:ok");

        var generatorEncoder = device.createCommandEncoder();
        var generatorCommand = generatorEncoder.finish();
        function* commands() {
            yield generatorCommand;
        }
        device.queue.submit(commands());
        log("sequence generator:ok");

        var stringError = caught(function () {
            device.queue.submit("ab");
        });
        log(errorLine("sequence string", stringError));

        var arrayLikeEncoder = device.createCommandEncoder();
        var arrayLikeCommand = arrayLikeEncoder.finish();
        var arrayLikeError = caught(function () {
            device.queue.submit({ length: 1, 0: arrayLikeCommand });
        });
        log(errorLine("sequence array-like", arrayLikeError));
        device.queue.submit([arrayLikeCommand]);

        var throwingEncoder = device.createCommandEncoder();
        var throwingCommand = throwingEncoder.finish();
        function* throwingCommands() {
            yield throwingCommand;
            throw new Error("mid-iteration next failed");
        }
        var iteratorError = caught(function () {
            device.queue.submit(throwingCommands());
        });
        log(errorLine("sequence iterator-throw", iteratorError));
        device.queue.submit([throwingCommand]);
    }

    function runErrorModel() {
        var validation = new GPUValidationError("validation");
        var oom = new GPUOutOfMemoryError("oom");
        var internal = new GPUInternalError("internal");
        log("error:GPUValidationError:" +
            (validation instanceof GPUValidationError) + "," +
            (validation instanceof GPUError));
        log("error:GPUOutOfMemoryError:" +
            (oom instanceof GPUOutOfMemoryError) + "," +
            (oom instanceof GPUError));
        log("error:GPUInternalError:" +
            (internal instanceof GPUInternalError) + "," +
            (internal instanceof GPUError));
        log("error:GPUError:" +
            (validation instanceof GPUError) + "," +
            (oom instanceof GPUError) + "," +
            (internal instanceof GPUError));

        class DerivedValidationError extends GPUValidationError {}
        var derived = new DerivedValidationError("derived");
        log("error:subclass:" +
            (derived instanceof DerivedValidationError) + "," +
            (derived instanceof GPUValidationError) + "," +
            (derived instanceof GPUError));
        log("error:length:" + [
            GPUError.length,
            GPUValidationError.length,
            GPUOutOfMemoryError.length,
            GPUInternalError.length
        ].join(","));
        var pipelineValidation = new GPUPipelineError("pipeline failed", {
            reason: "validation"
        });
        var pipelineInternal = new GPUPipelineError(undefined, { reason: "internal" });
        log("pipelineError:constructor:" +
            (typeof GPUPipelineError === "function") + ":" + GPUPipelineError.length);
        log("pipelineError:validation:" + pipelineValidation.name + ":" +
            pipelineValidation.message + ":" + pipelineValidation.reason + ":" +
            (pipelineValidation instanceof Error));
        log("pipelineError:default:" + pipelineInternal.message.length + ":" +
            pipelineInternal.reason + ":" + (pipelineInternal instanceof Error));

        var constructedEvent = new GPUUncapturedErrorEvent("manual", {
            error: validation,
            cancelable: true
        });
        constructedEvent.preventDefault();
        log("event:construct:" + constructedEvent.type + ":" +
            (constructedEvent.error === validation) + "," +
            constructedEvent.cancelable + "," + constructedEvent.defaultPrevented);
        log("event:inheritance:" +
            (GPUDevice.prototype instanceof EventTarget) + "," +
            (GPUUncapturedErrorEvent.prototype instanceof Event));
        var target = new EventTarget();
        var onceCalls = 0;
        target.addEventListener("manual", function () { onceCalls += 1; }, { once: true });
        var genericEvent = new Event("manual");
        target.dispatchEvent(genericEvent);
        target.dispatchEvent(genericEvent);
        log("event:target-once:" + onceCalls);
    }

    function textureAttributes(texture) {
        return [
            texture.width,
            texture.height,
            texture.depthOrArrayLayers,
            texture.mipLevelCount,
            texture.sampleCount,
            texture.dimension,
            texture.format,
            texture.usage
        ].join(",");
    }

    function runTextures() {
        // Headless Noop texture copies are no-ops. These checks intentionally
        // cover creation/conversion/attributes only, never texel bytes.
        var texture = device.createTexture({
            size: { width: 4, height: 2 },
            format: "r8unorm",
            usage: GPUTextureUsage.TEXTURE_BINDING
        });
        log("texture:create:ok");
        log("texture:width:" + texture.width);
        log("texture:height:" + texture.height);
        log("texture:depthOrArrayLayers:" + texture.depthOrArrayLayers);
        log("texture:mipLevelCount:" + texture.mipLevelCount);
        log("texture:sampleCount:" + texture.sampleCount);
        log("texture:dimension:" + texture.dimension);
        log("texture:format:" + texture.format);
        log("texture:usage:" + texture.usage);
        texture.createView({ usage: GPUTextureUsage.TEXTURE_BINDING });
        log("texture:view-usage:ok");

        var dictTexture = device.createTexture({
            size: { width: 4, height: 2 },
            format: "r8unorm",
            usage: GPUTextureUsage.TEXTURE_BINDING
        });
        var sequenceTexture = device.createTexture({
            size: [4, 2],
            format: "r8unorm",
            usage: GPUTextureUsage.TEXTURE_BINDING
        });
        var dictAttributes = textureAttributes(dictTexture);
        var sequenceAttributes = textureAttributes(sequenceTexture);
        log("texture:extent-dict:" + dictAttributes);
        log("texture:extent-sequence:" + sequenceAttributes);
        log("texture:extent-equal:" + (dictAttributes === sequenceAttributes));

        var formatError = caught(function () {
            device.createTexture({
                size: [1],
                format: "not-a-format",
                usage: GPUTextureUsage.TEXTURE_BINDING
            });
        });
        log(errorLine("texture:format-rejection", formatError));
        var lengthError = caught(function () {
            device.createTexture({
                size: [],
                format: "r8unorm",
                usage: GPUTextureUsage.TEXTURE_BINDING
            });
        });
        log(errorLine("texture:extent-length", lengthError));
        var overLengthError = caught(function () {
            device.createTexture({
                size: [1, 2, 3, 4],
                format: "r8unorm",
                usage: GPUTextureUsage.TEXTURE_BINDING
            });
        });
        log(errorLine("texture:extent-over-length", overLengthError));
        var primitiveExtentError = caught(function () {
            device.createTexture({
                size: "12",
                format: "r8unorm",
                usage: GPUTextureUsage.TEXTURE_BINDING
            });
        });
        log(errorLine("texture:extent-primitive", primitiveExtentError));

        var retainedTexture = device.createTexture({
            size: [1],
            format: "r8unorm",
            usage: GPUTextureUsage.TEXTURE_BINDING
        });
        retainedTextureView = retainedTexture.createView();
        retainedTexture = null;
        texture.destroy();
        dictTexture.destroy();
        sequenceTexture.destroy();
    }

    function runTextureRetention() {
        return gpu.requestAdapter().then(function () {
            return gpu.requestAdapter();
        }).then(function () {
            log("texture:view-alive:" +
                (retainedTextureView !== null &&
                 typeof Object.getPrototypeOf(retainedTextureView) === "object"));
            retainedTextureView = null;
        });
    }

    function runBindGroups() {
        var layout = device.createBindGroupLayout({
            entries: [
                { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "uniform" } },
                { binding: 1, visibility: GPUShaderStage.COMPUTE, sampler: { type: "filtering" } },
                { binding: 2, visibility: GPUShaderStage.COMPUTE, texture: {
                    sampleType: "float", viewDimension: "2d", multisampled: false
                } }
            ]
        });
        var resourceBuffer = device.createBuffer({ size: 4, usage: GPUBufferUsage.UNIFORM });
        var resourceSampler = device.createSampler();
        var resourceTexture = device.createTexture({
            size: [1], format: "rgba8unorm", usage: GPUTextureUsage.TEXTURE_BINDING
        });
        var resourceView = resourceTexture.createView();
        retainedBindGroup = device.createBindGroup({
            layout: layout,
            entries: [
                { binding: 0, resource: { buffer: resourceBuffer } },
                { binding: 1, resource: resourceSampler },
                { binding: 2, resource: resourceView }
            ]
        });
        log("bindGroup:resources:buffer,sampler,texture-view:ok");
        device.createBindGroup({
            layout: layout,
            entries: [
                { binding: 0, resource: resourceBuffer },
                { binding: 1, resource: resourceSampler },
                { binding: 2, resource: resourceTexture }
            ]
        });
        log("bindGroup:resources:direct-buffer:ok");
        log("bindGroup:resources:direct-texture:ok");

        var samplerTypeError = caught(function () {
            device.createBindGroupLayout({
                entries: [{
                    binding: 0,
                    visibility: GPUShaderStage.COMPUTE,
                    sampler: { type: "bad" }
                }]
            });
        });
        log(errorLine("bindGroup:sampler-type-rejection", samplerTypeError));

        layout = resourceBuffer = resourceSampler = resourceTexture = resourceView = null;
    }

    function runBindGroupRetention() {
        return gpu.requestAdapter().then(function () {
            return gpu.requestAdapter();
        }).then(function () {
            log("bindGroup:resources-alive:" +
                (retainedBindGroup !== null &&
                 typeof Object.getPrototypeOf(retainedBindGroup) === "object"));
            retainedBindGroup = null;
        });
    }

    function runRenderPipelines() {
        // Noop validates pipeline creation headlessly but executes no render work.
        var module = device.createShaderModule({
            code: "@vertex fn main() -> @builtin(position) vec4f { return vec4f(0); } " +
                "@fragment fn fragment_main() {}"
        });
        var introspectionPipeline = device.createRenderPipeline({
            layout: "auto",
            vertex: { module: module, entryPoint: "main" },
            fragment: { module: module, entryPoint: "fragment_main", targets: [] }
        });
        log("renderPipeline:create:ok");
        var derivedLayout = introspectionPipeline.getBindGroupLayout(0);
        log("getBindGroupLayout:create/release:ok");
        derivedLayout = null;

        var enumError = caught(function () {
            device.createRenderPipeline({
                layout: "auto",
                vertex: { module: module, entryPoint: "main" },
                primitive: { topology: "bad" }
            });
        });
        log(errorLine("renderPipeline:topology-rejection", enumError));

        device.createRenderPipeline({
            layout: "auto",
            vertex: { module: module, entryPoint: "main", buffers: [null] },
            fragment: {
                module: module,
                entryPoint: "fragment_main",
                targets: [null]
            }
        });
        log("renderPipeline:nullable-holes:ok");
        var asyncRenderModule = device.createShaderModule({
            code: "@vertex fn main() -> @builtin(position) vec4f { return vec4f(0); } " +
                "@fragment fn fs() -> @location(0) vec4f { return vec4f(1); }"
        });
        return device.createRenderPipelineAsync({
            layout: "auto",
            vertex: { module: asyncRenderModule, entryPoint: "main" },
            fragment: {
                module: asyncRenderModule,
                entryPoint: "fs",
                targets: [{ format: "rgba8unorm" }]
            }
        }).catch(function (error) {
            throw new Error("async render failed: " + error.message);
        }).then(function () {
            var computeModule = device.createShaderModule({
                code: "@compute @workgroup_size(1) fn main() {}"
            });
            return device.createComputePipelineAsync({
                layout: "auto",
                compute: { module: computeModule, entryPoint: "main" }
            }).catch(function (error) {
                throw new Error("async compute failed: " + error.message);
            });
        }).then(function () {
            log("pipelineAsync:compute,render:ok");
            var invalidModule = device.createShaderModule({
                code: "this is not valid WGSL"
            });
            return device.createComputePipelineAsync({
                layout: "auto",
                compute: { module: invalidModule, entryPoint: "main" }
            }).then(function () {
                throw new Error("invalid async compute pipeline unexpectedly resolved");
            }, function (error) {
                log("pipelineAsync:rejection:" + error.name + ":" + error.reason + ":" +
                    (error instanceof Error));
            });
        });
    }

    function runCompilationInfo() {
        var validModule = device.createShaderModule({
            code: "@compute @workgroup_size(1) fn main() {}"
        });
        var invalidModule = device.createShaderModule({
            code: "this is not valid WGSL"
        });
        var unicodeSource =
            "/* é😀 */ @compute @workgroup_size(1) fn main() { let x = missing; }";
        var unicodeModule = device.createShaderModule({ code: unicodeSource });
        function utf8Length(value) {
            var length = 0;
            for (var index = 0; index < value.length; ++index) {
                var unit = value.charCodeAt(index);
                if (unit <= 0x7f) {
                    length += 1;
                } else if (unit <= 0x7ff) {
                    length += 2;
                } else if (unit >= 0xd800 && unit <= 0xdbff && index + 1 < value.length) {
                    length += 4;
                    index += 1;
                } else {
                    length += 3;
                }
            }
            return length;
        }
        return validModule.getCompilationInfo().then(function (info) {
            log("compilationInfo:valid:" + Array.isArray(info.messages) + ":" +
                Object.isFrozen(info.messages) + ":" + info.messages.length + ":" +
                (info instanceof GPUCompilationInfo) + ":" +
                (info.messages === info.messages));
            var infoCall = caught(function () { GPUCompilationInfo(); });
            var infoConstruct = caught(function () { new GPUCompilationInfo(); });
            var messageCall = caught(function () { GPUCompilationMessage(); });
            var messageConstruct = caught(function () { new GPUCompilationMessage(); });
            log("compilationInfo:interfaces:" + infoCall.name + "," +
                infoConstruct.name + "," + messageCall.name + "," +
                messageConstruct.name);
            return invalidModule.getCompilationInfo();
        }).then(function (info) {
            var message = info.messages[0];
            if (!message || ["error", "warning", "info"].indexOf(message.type) === -1) {
                throw new Error("invalid shader did not produce a compilation message");
            }
            log("compilationInfo:diagnostic:" + message.type + ":" +
                message.lineNum + ":" + message.linePos + ":" +
                (message instanceof GPUCompilationMessage));
            return unicodeModule.getCompilationInfo();
        }).then(function (info) {
            var utf16Offset = unicodeSource.indexOf("missing");
            var utf8Offset = utf8Length(unicodeSource.slice(0, utf16Offset));
            var converted = false;
            var positions = [];
            for (var index = 0; index < info.messages.length; ++index) {
                var message = info.messages[index];
                positions.push(message.lineNum + "," + message.linePos + "," +
                    message.offset + "," + message.length);
                converted = converted || (message.offset === utf16Offset &&
                    message.lineNum === 1 &&
                    message.linePos === utf16Offset + 1 &&
                    message.length === "missing".length &&
                    unicodeSource.slice(message.offset, message.offset + message.length) ===
                        "missing");
            }
            log("compilationInfo:utf16:" + (utf8Offset !== utf16Offset) + ":" + converted + ":" +
                positions.join(";"));
        });
    }

    function runQuerySets() {
        parityQuerySet = device.createQuerySet({
            type: "occlusion",
            count: 4,
            label: "parity-query-set"
        });
        log("querySet:create:ok");
        log("querySet:type:" + parityQuerySet.type);
        log("querySet:count:" + parityQuerySet.count);
        parityQuerySet.label = "query-set-round-trip";
        log("querySet:label:" + parityQuerySet.label);

        var enumError = caught(function () {
            device.createQuerySet({ type: "bad", count: 1 });
        });
        log(errorLine("querySet:type-rejection", enumError));
    }

    function runCreationParity() {
        return validationScope("sampler", function () {
            var paritySampler = device.createSampler({
                label: "parity-sampler",
                addressModeU: "repeat",
                magFilter: "linear",
                minFilter: "linear",
                mipmapFilter: "linear",
                lodMinClamp: 1.5,
                lodMaxClamp: 9.5,
                compare: "less-equal",
                maxAnisotropy: 4
            });
            paritySampler.label = "sampler-round-trip";
            log("sampler:" + paritySampler.label);
        }).then(function () {
            return validationScope("texture", runTextures);
        }).then(function () {
            return validationScope("bind-group-family", runBindGroups);
        }).then(function () {
            return validationScope("pipelines", runRenderPipelines);
        }).then(function () {
            return runCompilationInfo();
        }).then(function () {
            return validationScope("querySet", runQuerySets);
        }).then(function () {
            return validationScope("occlusion-query", function () {
                var texture = device.createTexture({
                    size: [1], format: "rgba8unorm",
                    usage: GPUTextureUsage.RENDER_ATTACHMENT
                });
                var encoder = device.createCommandEncoder();
                var pass = encoder.beginRenderPass({
                    colorAttachments: [{
                        view: texture.createView(),
                        loadOp: "clear",
                        storeOp: "store"
                    }],
                    occlusionQuerySet: parityQuerySet
                });
                pass.beginOcclusionQuery(2);
                pass.endOcclusionQuery();
                pass.end();
                var destination = device.createBuffer({
                    size: 1024,
                    usage: GPUBufferUsage.QUERY_RESOLVE | GPUBufferUsage.COPY_DST
                });
                var clearTypeError = caught(function () {
                    encoder.clearBuffer(null);
                });
                log("commandEncoder:clearBuffer-type:" + clearTypeError.name);
                var clearOffsetError = caught(function () {
                    encoder.clearBuffer(destination, -1);
                });
                log("commandEncoder:clearBuffer-offset:" + clearOffsetError.name);
                var resolveTypeError = caught(function () {
                    encoder.resolveQuerySet(null, 0, 1, destination, 0);
                });
                log("commandEncoder:resolveQuerySet-type:" + resolveTypeError.name);
                var resolveOffsetError = caught(function () {
                    encoder.resolveQuerySet(parityQuerySet, 0, 1, destination, -1);
                });
                log("commandEncoder:resolveQuerySet-offset:" + resolveOffsetError.name);
                encoder.clearBuffer(destination);
                encoder.clearBuffer(destination, 256, 256);
                encoder.resolveQuerySet(parityQuerySet, 0, 4, destination, 0);
                log("debug:pushDebugGroup:" +
                    (encoder.pushDebugGroup("group\0🌞") === undefined ? "undefined" : "other"));
                log("debug:insertDebugMarker:" +
                    (encoder.insertDebugMarker("a\uD800b") === undefined ? "undefined" : "other"));
                log("debug:popDebugGroup:" +
                    (encoder.popDebugGroup() === undefined ? "undefined" : "other"));
                device.queue.submit([encoder.finish()]);
                log("querySet:occlusion-pass:ok");
            });
        }).then(function () {
            parityQuerySet.destroy();
            log("querySet:destroy:ok");
        });
    }

    function runRenderPipelineValidation() {
        device.pushErrorScope("validation");
        var module = device.createShaderModule({
            code: "@vertex fn main() -> @builtin(position) vec4f { return vec4f(0); }"
        });
        device.createRenderPipeline({
            layout: "auto",
            vertex: { module: module, entryPoint: "main" },
            depthStencil: {
                format: "depth24plus",
                depthWriteEnabled: false,
                depthCompare: "always"
            }
        });
        return device.popErrorScope().then(function (error) {
            if (error !== null) {
                throw new Error("minimal render pipeline validation failed: " + error.message);
            }
            device.pushErrorScope("validation");
            device.pushErrorScope("out-of-memory");
            device.createRenderPipeline({
                layout: "auto",
                vertex: { module: module, entryPoint: "main" }
            });
            return device.popErrorScope().then(function (inner) {
                return device.popErrorScope().then(function (outer) {
                    log("scope:render-pipeline:" +
                        (inner === null ? "null" : inner.constructor.name) + "," +
                        (outer === null ? "null" : outer.constructor.name));
                });
            });
        });
    }

    function runRenderPassAndCopyValidation() {
        // Noop validates these command streams but executes neither draws nor
        // texture copies, so every line below is validation-only evidence.
        var module = device.createShaderModule({
            code: "@vertex fn main(@builtin(vertex_index) i: u32) -> " +
                "@builtin(position) vec4f { var p = array(vec2f(-1,-1), " +
                "vec2f(3,-1), vec2f(-1,3)); return vec4f(p[i],0,1); } " +
                "@fragment fn fragment_main() -> @location(0) vec4f { " +
                "return vec4f(1,0,0,1); }"
        });
        var pipeline = device.createRenderPipeline({
            layout: "auto",
            vertex: { module: module, entryPoint: "main" },
            fragment: {
                module: module,
                entryPoint: "fragment_main",
                targets: [{ format: "rgba8unorm", writeMask: GPUColorWrite.ALL }]
            }
        });
        var sourceTexture = device.createTexture({
            size: [4, 4], format: "rgba8unorm",
            usage: GPUTextureUsage.COPY_SRC |
                GPUTextureUsage.COPY_DST |
                GPUTextureUsage.RENDER_ATTACHMENT
        });
        var destinationTexture = device.createTexture({
            size: [4, 4], format: "rgba8unorm",
            usage: GPUTextureUsage.COPY_SRC |
                GPUTextureUsage.COPY_DST |
                GPUTextureUsage.RENDER_ATTACHMENT
        });
        var view = sourceTexture.createView();

        var missingLoad = caught(function () {
            device.createCommandEncoder().beginRenderPass({
                colorAttachments: [{ view: view, storeOp: "store" }]
            });
        });
        log("renderPass:loadOp-missing:" + missingLoad.name);

        var wrongColor = caught(function () {
            device.createCommandEncoder().beginRenderPass({
                colorAttachments: [{
                    view: view,
                    clearValue: [0, 0, 1],
                    loadOp: "clear",
                    storeOp: "store"
                }]
            });
        });
        log("renderPass:clearValue-wrong-length:" + wrongColor.name);

        var maxDrawCountPass = device.createCommandEncoder().beginRenderPass({
            colorAttachments: [],
            maxDrawCount: 4
        });
        maxDrawCountPass.end();
        log("renderPass:maxDrawCount:ok");

        [
            ["negative", -1],
            ["2^64", 18446744073709551616],
            ["fractional", 1.5]
        ].forEach(function (entry) {
            var error = caught(function () {
                device.createCommandEncoder().beginRenderPass({
                    colorAttachments: [],
                    maxDrawCount: entry[1]
                });
            });
            log("renderPass:maxDrawCount-" + entry[0] + ":" + error.name);
        });

        var textureEncoder = device.createCommandEncoder();
        var textureAsView = textureEncoder.beginRenderPass({
                colorAttachments: [{
                    view: sourceTexture,
                    loadOp: "load",
                    storeOp: "store"
                }]
            });
        textureAsView.end();
        log("renderPass:texture-as-view:ok");

        var stateEncoder = device.createCommandEncoder();
        var statePass = stateEncoder.beginRenderPass({
            colorAttachments: [{
                view: view,
                loadOp: "load",
                storeOp: "store"
            }]
        });
        statePass.end();
        device.pushErrorScope("validation");
        var doubleEnd = caught(function () { statePass.end(); });
        if (doubleEnd !== null) {
            throw new Error("render pass double end threw: " + doubleEnd);
        }
        return device.popErrorScope().then(function (error) {
            if (!(error instanceof GPUValidationError)) {
                throw new Error("render pass double end did not emit validation");
            }
            log("renderPass:double-end:" + error.constructor.name + ":returned");

            device.pushErrorScope("validation");
            var methodAfterEnd = caught(function () { statePass.draw(3); });
            if (methodAfterEnd !== null) {
                throw new Error("render pass method after end threw: " + methodAfterEnd);
            }
            return device.popErrorScope();
        }).then(function (error) {
            if (!(error instanceof GPUValidationError)) {
                throw new Error("render pass method after end did not emit validation");
            }
            log("renderPass:method-after-end:" +
                error.constructor.name + ":returned");

        var missingTexture = caught(function () {
            device.createCommandEncoder().copyTextureToTexture(
                {}, { texture: destinationTexture }, [1, 1]
            );
        });
        log("copy:missing-texture:" + missingTexture.name);

        device.pushErrorScope("validation");
        var encoder = device.createCommandEncoder();
        var pass = encoder.beginRenderPass({
            colorAttachments: [{
                view: view,
                clearValue: [0.25, 0.5, 0.75, 1],
                loadOp: "clear",
                storeOp: "store"
            }]
        });
        pass.setPipeline(pipeline);
        pass.setViewport(0, 0, 4, 4, 0, 1);
        pass.draw(3);
        pass.end();
        device.queue.submit([encoder.finish()]);
        return device.popErrorScope().then(function (error) {
            if (error !== null) {
                throw new Error("render pass validation failed: " + error.message);
            }
            log("renderPass:chain-ok");

            device.pushErrorScope("validation");
            [
                [0.25, 0.5, 0.75, 1],
                { r: 0.25, g: 0.5, b: 0.75, a: 1 }
            ].forEach(function (clearValue) {
                var equivalentEncoder = device.createCommandEncoder();
                var equivalentPass = equivalentEncoder.beginRenderPass({
                    colorAttachments: [{
                        view: view,
                        clearValue: clearValue,
                        loadOp: "clear",
                        storeOp: "store"
                    }]
                });
                equivalentPass.end();
                device.queue.submit([equivalentEncoder.finish()]);
            });
            return device.popErrorScope();
        }).then(function (error) {
            if (error !== null) {
                throw new Error("clearValue equivalence validation failed: " + error.message);
            }
            log("renderPass:clearValue-dict-sequence:no-validation-error");

            var buffer = device.createBuffer({
                size: 1024,
                usage: GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST
            });
            var bufferInfo = {
                buffer: buffer, bytesPerRow: 256, rowsPerImage: 4
            };
            var sourceInfo = { texture: sourceTexture };
            var destinationInfo = { texture: destinationTexture };
            device.pushErrorScope("validation");
            var copyEncoder = device.createCommandEncoder();
            copyEncoder.copyBufferToTexture(bufferInfo, sourceInfo, [4, 4]);
            copyEncoder.copyTextureToBuffer(sourceInfo, bufferInfo, [4, 4]);
            copyEncoder.copyTextureToTexture(sourceInfo, destinationInfo, [4, 4]);
            device.queue.submit([copyEncoder.finish()]);
            device.queue.writeTexture(
                destinationInfo,
                new Uint8Array(1024),
                { bytesPerRow: 256, rowsPerImage: 4 },
                [4, 4]
            );
            return device.popErrorScope();
        }).then(function (error) {
            if (error !== null) {
                throw new Error("copy validation failed: " + error.message);
            }
            log("copy:texture-validation:no-validation-error");
        });
        });
    }

    function runRenderBundles() {
        var formatError = caught(function () {
            device.createRenderBundleEncoder({ colorFormats: ["bad"] });
        });
        log(errorLine("renderBundle:format-rejection", formatError));

        device.pushErrorScope("validation");
        device.createRenderBundleEncoder({ colorFormats: ["rgba8unorm", ,] });
        var module = device.createShaderModule({
            code: "@vertex fn main(@builtin(vertex_index) i: u32) -> " +
                "@builtin(position) vec4f { var p = array(vec2f(-1,-1), " +
                "vec2f(3,-1), vec2f(-1,3)); return vec4f(p[i],0,1); } " +
                "@fragment fn fragment_main() -> @location(0) vec4f { " +
                "return vec4f(0,1,0,1); }"
        });
        var pipeline = device.createRenderPipeline({
            layout: "auto",
            vertex: { module: module, entryPoint: "main" },
            fragment: {
                module: module,
                entryPoint: "fragment_main",
                targets: [{ format: "rgba8unorm" }]
            }
        });
        var texture = device.createTexture({
            size: [4, 4], format: "rgba8unorm",
            usage: GPUTextureUsage.RENDER_ATTACHMENT
        });
        var bundleEncoder = device.createRenderBundleEncoder({
            colorFormats: ["rgba8unorm"]
        });
        bundleEncoder.setPipeline(pipeline);
        bundleEncoder.draw(3);
        var bundle = bundleEncoder.finish();
        device.pushErrorScope("validation");
        var useAfterFinish = caught(function () { bundleEncoder.draw(3); });
        if (useAfterFinish !== null) {
            throw new Error("render bundle use after finish threw: " + useAfterFinish);
        }
        return device.popErrorScope().then(function (error) {
            if (!(error instanceof GPUValidationError)) {
                throw new Error("render bundle use after finish did not emit validation");
            }
            log("renderBundle:use-after-finish:" +
                error.constructor.name + ":returned");

        var encoder = device.createCommandEncoder();
        var pass = encoder.beginRenderPass({
            colorAttachments: [{
                view: texture.createView(),
                loadOp: "clear",
                storeOp: "store"
            }]
        });
        var bundleTypeError = caught(function () {
            pass.executeBundles([pipeline]);
        });
        log(errorLine("renderBundle:type-confusion", bundleTypeError));
        pass.executeBundles([bundle, bundle]);
        pass.end();
        device.queue.submit([encoder.finish()]);
        return device.popErrorScope().then(function (error) {
            log("renderBundle:sparse-colorFormats:" +
                (error === null ? "null" : error.constructor.name));
            log("renderBundle:chain:" +
                (error === null ? "null" : error.constructor.name));
        });
        });
    }

    function runIndirectCommands() {
        return validationScope("indirect-commands", function () {
            var indirectBuffer = device.createBuffer({
                size: 32,
                usage: GPUBufferUsage.INDIRECT
            });
            var indexBuffer = device.createBuffer({ size: 8, usage: GPUBufferUsage.INDEX });

            var computeModule = device.createShaderModule({
                code: "@compute @workgroup_size(1) fn main() {}"
            });
            var computePipeline = device.createComputePipeline({
                layout: "auto",
                compute: { module: computeModule, entryPoint: "main" }
            });
            var computeBindGroup = device.createBindGroup({
                layout: computePipeline.getBindGroupLayout(0),
                entries: []
            });
            var computeEncoder = device.createCommandEncoder();
            var computePass = computeEncoder.beginComputePass();
            computePass.setPipeline(computePipeline);
            computePass.setBindGroup(0, computeBindGroup, []);
            computePass.setBindGroup(0, computeBindGroup, new Uint32Array([]), 0, 0);
            computePass.setImmediates(0, new Uint32Array([10, 20]), 1, 1);
            var immediateSizeError = caught(function () {
                computePass.setImmediates(0, new Uint8Array([1, 2, 3]));
            });
            log("setImmediates:content-size:" + immediateSizeError.name);
            var computeOffsetsError = caught(function () {
                computePass.setBindGroup(0, computeBindGroup, [0, -1]);
            });
            log("setBindGroup:compute-offsets:" + computeOffsetsError.name);
            var offsetsWindowError = caught(function () {
                computePass.setBindGroup(0, computeBindGroup, new Uint32Array([0]), 1, 1);
            });
            log("setBindGroup:u32array-window:" + offsetsWindowError.name);
            var offsetsTypeError = caught(function () {
                computePass.setBindGroup(0, computeBindGroup, new Int32Array([]), 0, 0);
            });
            log("setBindGroup:u32array-type:" + offsetsTypeError.name);
            computePass.setBindGroup(0, null);
            log("setBindGroup:compute-null:ok");
            computePass.dispatchWorkgroupsIndirect(indirectBuffer, 0);
            var computeTypeError = caught(function () {
                computePass.dispatchWorkgroupsIndirect({}, 0);
            });
            log("indirect:dispatchWorkgroupsIndirect:" + computeTypeError.name);
            computePass.end();

            var renderModule = device.createShaderModule({
                code: "@vertex fn main() -> @builtin(position) vec4f { " +
                    "return vec4f(0); } @fragment fn fs() -> @location(0) vec4f { " +
                    "return vec4f(1); }"
            });
            var renderPipeline = device.createRenderPipeline({
                layout: "auto",
                vertex: { module: renderModule, entryPoint: "main" },
                fragment: {
                    module: renderModule,
                    entryPoint: "fs",
                    targets: [{ format: "rgba8unorm" }]
                }
            });
            var renderBindGroup = device.createBindGroup({
                layout: renderPipeline.getBindGroupLayout(0),
                entries: []
            });
            var texture = device.createTexture({
                size: [1], format: "rgba8unorm",
                usage: GPUTextureUsage.RENDER_ATTACHMENT
            });
            var renderEncoder = device.createCommandEncoder();
            var renderPass = renderEncoder.beginRenderPass({
                colorAttachments: [{
                    view: texture.createView(),
                    loadOp: "clear",
                    storeOp: "store"
                }]
            });
            renderPass.setPipeline(renderPipeline);
            renderPass.setBindGroup(0, renderBindGroup, []);
            renderPass.setBindGroup(0, renderBindGroup, new Uint32Array([]), 0, 0);
            var immediateBytes = new Uint8Array([0, 1, 2, 3, 4, 5, 6, 7]).buffer;
            renderPass.setImmediates(0, new DataView(immediateBytes), 0, 4);
            var renderOffsetsError = caught(function () {
                renderPass.setBindGroup(0, renderBindGroup, [0, 4294967296]);
            });
            log("setBindGroup:render-offsets:" + renderOffsetsError.name);
            renderPass.setBindGroup(0, null);
            log("setBindGroup:render-null:ok");
            renderPass.setBlendConstant([0.125, 0.25, 0.5, 1]);
            renderPass.setBlendConstant({ r: 0.75, g: 0.5, b: 0.25, a: 1 });
            renderPass.setStencilReference(4294967295);
            log("dynamicState:blend-stencil:ok");
            var blendError = caught(function () {
                renderPass.setBlendConstant([0, 1]);
            });
            log("dynamicState:blend-length:" + blendError.name);
            var stencilError = caught(function () {
                renderPass.setStencilReference(4294967296);
            });
            log("dynamicState:stencil-range:" + stencilError.name);
            renderPass.setIndexBuffer(indexBuffer, "uint16");
            renderPass.drawIndirect(indirectBuffer, 0);
            var drawOffsetError = caught(function () {
                renderPass.drawIndirect(indirectBuffer, -1);
            });
            log("indirect:drawIndirect:" + drawOffsetError.name);
            renderPass.drawIndexedIndirect(indirectBuffer, 0);
            var indexedTypeError = caught(function () {
                renderPass.drawIndexedIndirect(null, 0);
            });
            log("indirect:drawIndexedIndirect:" + indexedTypeError.name);

            var bundleEncoder = device.createRenderBundleEncoder({
                colorFormats: ["rgba8unorm"]
            });
            bundleEncoder.setPipeline(renderPipeline);
            bundleEncoder.setBindGroup(0, renderBindGroup, []);
            bundleEncoder.setBindGroup(0, renderBindGroup, new Uint32Array([]), 0, 0);
            bundleEncoder.setImmediates(0, immediateBytes, 4, 4);
            log("setImmediates:all-encoders:ok");
            var bundleOffsetsError = caught(function () {
                bundleEncoder.setBindGroup(0, renderBindGroup, [1.5]);
            });
            log("setBindGroup:bundle-offsets:" + bundleOffsetsError.name);
            bundleEncoder.setIndexBuffer(indexBuffer, "uint16");
            bundleEncoder.drawIndirect(indirectBuffer, 0);
            var bundleTypeError = caught(function () {
                bundleEncoder.drawIndirect("buffer", 0);
            });
            log("indirect:bundle.drawIndirect:" + bundleTypeError.name);
            bundleEncoder.drawIndexedIndirect(indirectBuffer, 0);
            var bundleOffsetError = caught(function () {
                bundleEncoder.drawIndexedIndirect(indirectBuffer, NaN);
            });
            log("indirect:bundle.drawIndexedIndirect:" + bundleOffsetError.name);
            renderPass.executeBundles([bundleEncoder.finish()]);
            renderPass.end();

            device.queue.submit([computeEncoder.finish(), renderEncoder.finish()]);
        });
    }

    function runRequiredMembers() {
        var entriesError = caught(function () {
            device.createBindGroupLayout({});
        });
        log(errorLine("required:entries", entriesError));

        var layoutsError = caught(function () {
            device.createPipelineLayout({});
        });
        log(errorLine("required:bindGroupLayouts", layoutsError));

        var emptyLayout = device.createBindGroupLayout({ entries: [] });
        device.createPipelineLayout({ bindGroupLayouts: [null, emptyLayout] });
        log("pipelineLayout:null-slot:ok");
        var bindingError = caught(function () {
            device.createBindGroup({
                layout: emptyLayout,
                entries: [{ resource: { buffer: labelBuffer } }]
            });
        });
        log(errorLine("required:binding", bindingError));

        var layoutError = caught(function () {
            device.createBindGroup({ entries: [] });
        });
        log(errorLine("required:layout", layoutError));
    }

    function runStrings() {
        var bmp = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: "ラベルé"
        });
        log("string:bmp:" + bmp.label);
        bmp.destroy();

        var pair = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: "🎮"
        });
        log("string:pair:" + pair.label);
        pair.destroy();

        var loneSurrogate = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: "a\uD800b"
        });
        log("string:lone-surrogate:" + loneSurrogate.label);
        loneSurrogate.destroy();

        var empty = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: ""
        });
        log("string:empty:" + empty.label.length);
        empty.destroy();

        var absent = device.createBuffer({ size: 4, usage: GPUBufferUsage.COPY_DST });
        log("string:absent:" + absent.label.length);
        absent.destroy();
    }

    function runCoercions() {
        createAndDestroyBuffer(0, GPUBufferUsage.COPY_DST);
        log("coerce:size-0:ok");
        createAndDestroyBuffer(4294967295, GPUBufferUsage.COPY_DST);
        log("coerce:size-u32-max:ok");

        // specs/tracking/codegen-deltas.md records that enforce_u64 accepts
        // integral values through 2^64-1 instead of WebIDL's 2^53-1 cap.
        createAndDestroyBuffer(9007199254740992, GPUBufferUsage.COPY_DST);
        log("coerce:size-2^53:ok");

        var cases = [
            ["2^64", 18446744073709551616],
            ["negative", -1],
            ["fractional", 1.5],
            ["nan", NaN],
            ["infinity", Infinity]
        ];
        cases.forEach(function (entry) {
            var error = caught(function () {
                createAndDestroyBuffer(entry[1], GPUBufferUsage.COPY_DST);
            });
            log("coerce:size-" + entry[0] + ":" + error.name);
        });

        var usageError = caught(function () {
            createAndDestroyBuffer(4, 4294967296);
        });
        log("coerce:usage-2^32:" + usageError.name);
        log("typeerror-name:" + usageError.name);

        var sizeBigintError = caught(function () {
            createAndDestroyBuffer(BigInt(8), GPUBufferUsage.COPY_DST);
        });
        log("bigint:size:" + sizeBigintError.name);

        var offsetBigintError = caught(function () {
            device.queue.writeBuffer(labelBuffer, BigInt(0), new Uint8Array(0));
        });
        log("bigint:writeBuffer-offset:" + offsetBigintError.name);

        var numberLabel = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: 42
        });
        log("label:number:" + numberLabel.label);
        numberLabel.destroy();

        var negativeZeroLabel = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: -0
        });
        log("label:-0:" + negativeZeroLabel.label);
        negativeZeroLabel.destroy();

        var exponentialLabel = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: 1e21
        });
        log("label:1e21:" + exponentialLabel.label);
        exponentialLabel.destroy();

        var objectLabel = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: { toString: function () { return "object-label"; } }
        });
        log("label:object:" + objectLabel.label);
        objectLabel.destroy();

        [3.5, 70000].forEach(function (value, index) {
            device.createSampler({
                magFilter: "linear",
                minFilter: "linear",
                mipmapFilter: "linear",
                maxAnisotropy: value
            });
            log("clamp:" + ["ties-ok", "saturation-ok"][index]);
        });
    }

    function finishConformance() {
        uncapturedEventLines.forEach(log);
        return runTextureRetention()
            .then(runBindGroupRetention)
            .then(runRenderPipelineValidation)
            .then(runRenderPassAndCopyValidation)
            .then(runLockedEncoderValidation)
            .then(runDepthSliceParity)
            .then(runRenderBundles)
            .then(runIndirectCommands)
            .then(runErrorScopes)
            .then(runMapAsyncErrorRouting)
            .then(runMapStateParity)
            .then(runOrdering)
            .then(runRequestedFeatureOrdering)
            .then(runSubmitErrorRouting)
            .then(function () {
                labelBuffer.destroy();
                log("destroy:ok");
                return runDestroyedErrorSuppression();
            })
            .then(function () {
                frameContractLines.forEach(log);
                finished = true;
                globalThis.parityDone = true;
            });
    }

    try {
        var stableMethod = device.createBuffer === device.createBuffer;
        labelBuffer = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST,
            label: null
        });
        var nullLabel = labelBuffer.label;
        labelBuffer.label = "round-trip";
        log("buffer:" + nullLabel + "," + labelBuffer.label + ";method:" + stableMethod);
        var embeddedNulLabel = "null\0in\0label";
        var embeddedNulBuffer = device.createBuffer({
            size: 4, usage: GPUBufferUsage.COPY_DST, label: embeddedNulLabel
        });
        var embeddedNulRoundTrip = embeddedNulBuffer.label === embeddedNulLabel;
        embeddedNulBuffer.destroy();
        log("label:nul-round-trip-destroy:" + embeddedNulRoundTrip + ":" +
            (embeddedNulBuffer.label === embeddedNulLabel));
        var labelDescriptor = Object.getOwnPropertyDescriptor(GPUBuffer.prototype, "label");
        var methodDescriptor = Object.getOwnPropertyDescriptor(GPUBuffer.prototype, "mapAsync");
        var constructorDescriptor = Object.getOwnPropertyDescriptor(
            GPUBuffer.prototype,
            "constructor"
        );
        var reflectedKeys = [];
        for (var reflectedKey in labelBuffer) {
            reflectedKeys.push(reflectedKey);
        }
        log("enumerable:label-method-constructor:" + labelDescriptor.enumerable + "," +
            methodDescriptor.enumerable + "," + constructorDescriptor.enumerable + ":" +
            (reflectedKeys.indexOf("label") !== -1) + "," +
            (reflectedKeys.indexOf("size") !== -1));
        var interfacePrototypeDescriptor = Object.getOwnPropertyDescriptor(
            GPUBuffer,
            "prototype"
        );
        log("webidl:interface.prototype:" + interfacePrototypeDescriptor.writable + "," +
            interfacePrototypeDescriptor.enumerable + "," +
            interfacePrototypeDescriptor.configurable);
        log("webidl:interface.keys:" + Object.keys(GPUBuffer).join(","));
        var externalTexturePrototypeDescriptor = Object.getOwnPropertyDescriptor(
            GPUExternalTexture,
            "prototype"
        );
        var externalTextureLabelDescriptor = Object.getOwnPropertyDescriptor(
            GPUExternalTexture.prototype,
            "label"
        );
        var externalTextureConstructorDescriptor = Object.getOwnPropertyDescriptor(
            GPUExternalTexture.prototype,
            "constructor"
        );
        var externalTextureCallError = caught(function () {
            GPUExternalTexture();
        });
        var externalTextureConstructError = caught(function () {
            new GPUExternalTexture();
        });
        var ordinaryTexture = device.createTexture({
            size: [1, 1, 1],
            format: "rgba8unorm",
            usage: GPUTextureUsage.TEXTURE_BINDING
        });
        log("webidl:external-texture:" + (typeof GPUExternalTexture) + ":" +
            externalTextureCallError.name + "," + externalTextureCallError.message + ":" +
            externalTextureConstructError.name + "," + externalTextureConstructError.message + ":" +
            externalTexturePrototypeDescriptor.writable + "," +
            externalTexturePrototypeDescriptor.enumerable + "," +
            externalTexturePrototypeDescriptor.configurable + ":" +
            externalTextureLabelDescriptor.enumerable + "," +
            externalTextureConstructorDescriptor.enumerable + ":" +
            GPUExternalTexture.prototype[Symbol.toStringTag] + ":" +
            (ordinaryTexture instanceof GPUExternalTexture));
        ordinaryTexture.destroy();
        var reflectedMethod = GPUBuffer.prototype.mapAsync;
        log("webidl:method:" + (reflectedMethod instanceof Function) + "," +
            (Object.getPrototypeOf(reflectedMethod) === Function.prototype) + "," +
            reflectedMethod.name + "," + reflectedMethod.length + "," +
            typeof reflectedMethod.call + "," + typeof reflectedMethod.bind);
        var reflectedGetter = Object.getOwnPropertyDescriptor(
            GPUBuffer.prototype,
            "size"
        ).get;
        log("webidl:getter:" + (reflectedGetter instanceof Function) + "," +
            (Object.getPrototypeOf(reflectedGetter) === Function.prototype) + "," +
            reflectedGetter.name + "," + reflectedGetter.length + "," +
            typeof reflectedGetter.call + "," + typeof reflectedGetter.bind);
        var illegalInterfaceCall = caught(function () {
            GPUPipelineError("message", { reason: "internal" });
        });
        log("webidl:constructible-call:" + illegalInterfaceCall.name);
        log("webidl:object-tag:" + Object.prototype.toString.call(labelBuffer));
        log("webidl:prototype-tag:" + GPUBuffer.prototype[Symbol.toStringTag]);
        log("identity:queue:" + (device.queue === device.queue));
        log("identity:lost:" + (device.lost === device.lost));
        log("features:" + Array.from(device.features).join(","));
        log("features:has:" + device.features.has("core-features-and-limits") +
            "," + device.features.has("definitely-not-a-webgpu-feature"));
        log("limits:minUniformBufferOffsetAlignment:" +
            device.limits.minUniformBufferOffsetAlignment);
        log("identity:features:" + (device.features === device.features));
        log("identity:limits:" + (device.limits === device.limits));
        log("identity:adapterInfo:" + (device.adapterInfo === device.adapterInfo));
        log("typeof:device.createBuffer:" + typeof device.createBuffer);
        var prototypeBuffer = device.createBuffer({
            size: 4,
            usage: GPUBufferUsage.COPY_DST
        });
        log("identity:cross-instance-prototype:" +
            (Object.getPrototypeOf(labelBuffer) ===
                Object.getPrototypeOf(prototypeBuffer)));
        log("identity:cross-instance-method:" +
            (labelBuffer.mapAsync === prototypeBuffer.mapAsync));
        prototypeBuffer.destroy();

        runCreationParity().then(function () {
            runCoercions();
            runStrings();
            runRequiredMembers();
            runErrorModel();
            runIterators();

        device.lost.then(function (info) {
            log("lostReason:" + info.reason);
        }).catch(fail);

            gpu.requestAdapter().then(function (firstAdapter) {
            log("identity:adapter.features:" +
                (firstAdapter.features === firstAdapter.features));
            log("identity:adapter.limits:" +
                (firstAdapter.limits === firstAdapter.limits));
            log("identity:adapter.info:" +
                (firstAdapter.info === firstAdapter.info));
            var order = [];
            var settleIndex = 0;
            var thenCount = 0;
            var adapterPrototype = Object.getPrototypeOf(firstAdapter);
            Object.defineProperty(adapterPrototype, "then", {
                configurable: true,
                get: function () {
                    settleIndex += 1;
                    order.push("settle" + settleIndex);
                    return undefined;
                }
            });

            function afterThen() {
                thenCount += 1;
                if (thenCount === 2) {
                    delete adapterPrototype.then;
                    log("tick:" + order.join(","));
                    runMappingDetach()
                        .then(runDeviceDestroyDetach)
                        .then(runMappedRangeOverlap)
                        .then(runWriteBufferRoundTrip)
                        .then(finishConformance)
                        .catch(fail);
                }
            }

            gpu.requestAdapter().then(function () {
                order.push("then1");
                afterThen();
            }).catch(fail);
            gpu.requestAdapter().then(function () {
                order.push("then2");
                afterThen();
            }).catch(fail);
            }).catch(fail);
        }).catch(fail);
    } catch (error) {
        fail(error);
    }
}());
