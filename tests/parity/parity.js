(function () {
    "use strict";

    globalThis.parityLog = [];
    globalThis.parityDone = false;

    var finished = false;
    var labelBuffer;

    function log(line) {
        globalThis.parityLog.push(String(line));
    }

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

    function errorLine(section, error) {
        return section + ":" + error.name + ":" + error.message;
    }

    function createAndDestroyBuffer(size, usage) {
        var buffer = device.createBuffer({ size: size, usage: usage });
        buffer.destroy();
    }

    function runOrdering() {
        var orderingBuffer = device.createBuffer({ size: 4, usage: 1 });
        var sameTickOrder = [];
        var mapPromise = orderingBuffer.mapAsync(1, 0, 4).then(function () {
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

    function runErrorScopes() {
        var scopedBuffer = device.createBuffer({ size: 4, usage: 8 });
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
                log(errorLine("reject:popErrorScope", error));
            });
        });
    }

    function runOffsetWindowRoundTrip() {
        var source = device.createBuffer({
            size: 12,
            usage: 4,
            mappedAtCreation: true
        });
        var destination = device.createBuffer({ size: 4, usage: 9 });
        var window = source.getMappedRange(8, 4);
        new Uint8Array(window).set([21, 22, 23, 24]);
        source.unmap();

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(source, 8, destination, 0, 4);
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return destination.mapAsync(1, 0, 4);
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
            usage: 4,
            mappedAtCreation: true
        });
        var readback = device.createBuffer({ size: 8, usage: 9 });
        var mappedRange = mapped.getMappedRange(0, 4);
        new Uint8Array(mappedRange).set([7, 8, 9, 10]);
        mapped.unmap();

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(mapped, 0, readback, 0, 8);
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return readback.mapAsync(1, 0, 8);
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
            usage: 4,
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

        var destroyed = device.createBuffer({ size: 4, usage: 1 });
        destroyed.destroy();
        var destroyedMapError = caught(function () {
            destroyed.mapAsync(1, 0, 4);
        });
        log(errorLine("reject:mapAsync", destroyedMapError));

        return runMappedAtCreationRoundTrip().then(runOffsetWindowRoundTrip);
    }

    function runWriteBufferRoundTrip() {
        var source = device.createBuffer({ size: 12, usage: 12 });
        var destination = device.createBuffer({ size: 12, usage: 9 });
        var bytes = new ArrayBuffer(8);
        new Uint8Array(bytes).set([3, 1, 4, 1, 5, 9, 2, 6]);
        device.queue.writeBuffer(source, 0, bytes, 0, 8);
        var viewBacking = new ArrayBuffer(8);
        new Uint8Array(viewBacking).set([99, 98, 8, 5, 3, 0, 97, 96]);
        device.queue.writeBuffer(source, 8, new Uint8Array(viewBacking, 2, 4));

        var encoder = device.createCommandEncoder();
        encoder.copyBufferToBuffer(source, 0, destination, 0, 12);
        device.queue.submit([encoder.finish()]);
        return device.queue.onSubmittedWorkDone().then(function () {
            return destination.mapAsync(1, 0, 12);
        }).then(function () {
            var range = destination.getMappedRange();
            var result = new Uint8Array(range);
            log("writeBuffer:" + bytesOfView(result.subarray(0, 8)));
            log("writeBuffer view:" + bytesOfView(result.subarray(8, 12)));
            destination.unmap();
            source.destroy();
            destination.destroy();
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
        var bmp = device.createBuffer({ size: 4, usage: 8, label: "ラベルé" });
        log("string:bmp:" + bmp.label);
        bmp.destroy();

        var pair = device.createBuffer({ size: 4, usage: 8, label: "🎮" });
        log("string:pair:" + pair.label);
        pair.destroy();

        var loneSurrogate = device.createBuffer({
            size: 4,
            usage: 8,
            label: "a\uD800b"
        });
        log("string:lone-surrogate:" + loneSurrogate.label);
        loneSurrogate.destroy();

        var empty = device.createBuffer({ size: 4, usage: 8, label: "" });
        log("string:empty:" + empty.label.length);
        empty.destroy();

        var absent = device.createBuffer({ size: 4, usage: 8 });
        log("string:absent:" + absent.label.length);
        absent.destroy();
    }

    function runCoercions() {
        createAndDestroyBuffer(0, 8);
        log("coerce:size-0:ok");
        createAndDestroyBuffer(4294967295, 8);
        log("coerce:size-u32-max:ok");

        // specs/tracking/codegen-deltas.md records that enforce_u64 accepts
        // integral values through 2^64-1 instead of WebIDL's 2^53-1 cap.
        createAndDestroyBuffer(9007199254740992, 8);
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
                createAndDestroyBuffer(entry[1], 8);
            });
            log("coerce:size-" + entry[0] + ":" + error.name);
        });

        var usageError = caught(function () {
            createAndDestroyBuffer(4, 4294967296);
        });
        log("coerce:usage-2^32:" + usageError.name);
        log("typeerror-name:" + usageError.name);

        var sizeBigintError = caught(function () {
            createAndDestroyBuffer(BigInt(8), 8);
        });
        log("bigint:size:" + sizeBigintError.name);

        var offsetBigintError = caught(function () {
            device.queue.writeBuffer(labelBuffer, BigInt(0), new Uint8Array(0));
        });
        log("bigint:writeBuffer-offset:" + offsetBigintError.name);

        var numberLabel = device.createBuffer({ size: 4, usage: 8, label: 42 });
        log("label:number:" + numberLabel.label);
        numberLabel.destroy();

        var negativeZeroLabel = device.createBuffer({
            size: 4,
            usage: 8,
            label: -0
        });
        log("label:-0:" + negativeZeroLabel.label);
        negativeZeroLabel.destroy();

        var exponentialLabel = device.createBuffer({
            size: 4,
            usage: 8,
            label: 1e21
        });
        log("label:1e21:" + exponentialLabel.label);
        exponentialLabel.destroy();

        var objectLabel = device.createBuffer({
            size: 4,
            usage: 8,
            label: { toString: function () { return "object-label"; } }
        });
        log("label:object:" + objectLabel.label);
        objectLabel.destroy();

        [NaN, 3.5, 70000].forEach(function (value, index) {
            device.createSampler({ maxAnisotropy: value });
            log("clamp:" + ["nan-ok", "ties-ok", "saturation-ok"][index]);
        });
    }

    function finishConformance() {
        return runErrorScopes()
            .then(runOrdering)
            .then(function () {
                labelBuffer.destroy();
                log("destroy:ok");
                finished = true;
                globalThis.parityDone = true;
            });
    }

    try {
        var stableMethod = device.createBuffer === device.createBuffer;
        labelBuffer = device.createBuffer({ size: 4, usage: 8, label: null });
        var nullLabel = labelBuffer.label;
        labelBuffer.label = "round-trip";
        log("buffer:" + nullLabel + "," + labelBuffer.label + ";method:" + stableMethod);
        log("identity:queue:" + (device.queue === device.queue));
        log("identity:lost:" + (device.lost === device.lost));
        log("typeof:device.createBuffer:" + typeof device.createBuffer);
        var prototypeBuffer = device.createBuffer({ size: 4, usage: 8 });
        log("identity:cross-instance-prototype:" +
            (Object.getPrototypeOf(labelBuffer) ===
                Object.getPrototypeOf(prototypeBuffer)));
        log("identity:cross-instance-method:" +
            (labelBuffer.mapAsync === prototypeBuffer.mapAsync));
        prototypeBuffer.destroy();

        var paritySampler = device.createSampler({
            label: "parity-sampler",
            addressModeU: "repeat",
            magFilter: "linear",
            mipmapFilter: "linear",
            lodMinClamp: 1.5,
            lodMaxClamp: 9.5,
            compare: "less-equal",
            maxAnisotropy: 4
        });
        paritySampler.label = "sampler-round-trip";
        log("sampler:" + paritySampler.label);

        runCoercions();
        runStrings();
        runRequiredMembers();
        runErrorModel();
        runIterators();

        device.lost.then(function (info) {
            log("lostReason:" + info.reason);
        }).catch(fail);

        gpu.requestAdapter().then(function (firstAdapter) {
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
    } catch (error) {
        fail(error);
    }
}());
