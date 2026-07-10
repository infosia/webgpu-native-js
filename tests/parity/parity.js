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

    function finishConformance() {
        try {
            var setEncoder = device.createCommandEncoder();
            var setCommand = setEncoder.finish();
            device.queue.submit(new Set([setCommand]));
            log("sequence Set:ok");

            var arrayLikeEncoder = device.createCommandEncoder();
            var arrayLikeCommand = arrayLikeEncoder.finish();
            var arrayLikeName = "none";
            try {
                device.queue.submit({ length: 1, 0: arrayLikeCommand });
            } catch (error) {
                arrayLikeName = error.name;
            }
            log("sequence array-like:" + arrayLikeName);

            var bigintName = "none";
            try {
                var unexpected = device.createBuffer({ size: BigInt(8), usage: 8 });
                unexpected.destroy();
            } catch (error) {
                bigintName = error.name;
            }
            log("bigint:" + bigintName);

            labelBuffer.destroy();
            paritySampler = null;
            log("destroy:ok");
            finished = true;
            globalThis.parityDone = true;
        } catch (error) {
            fail(error);
        }
    }

    function runWriteBufferRoundTrip() {
        try {
            var source = device.createBuffer({ size: 8, usage: 12 });
            var destination = device.createBuffer({ size: 8, usage: 9 });
            var bytes = new ArrayBuffer(8);
            new Uint8Array(bytes).set([3, 1, 4, 1, 5, 9, 2, 6]);
            device.queue.writeBuffer(source, 0, bytes, 0, 8);

            var encoder = device.createCommandEncoder();
            encoder.copyBufferToBuffer(source, 0, destination, 0, 8);
            device.queue.submit([encoder.finish()]);
            device.queue.onSubmittedWorkDone().then(function () {
                return destination.mapAsync(1, 0, 8);
            }).then(function () {
                var range = destination.getMappedRange();
                log("writeBuffer:" + bytesOf(range));
                destination.unmap();
                source.destroy();
                destination.destroy();
                finishConformance();
            }).catch(fail);
        } catch (error) {
            fail(error);
        }
    }

    function runMappedAtCreationRoundTrip() {
        try {
            var mapped = device.createBuffer({
                size: 8,
                usage: 4,
                mappedAtCreation: true
            });
            var readback = device.createBuffer({ size: 8, usage: 9 });
            var mappedRange = mapped.getMappedRange();
            new Uint8Array(mappedRange).set([7, 8, 9, 10, 11, 12, 13, 14]);
            mapped.unmap();
            if (mappedRange.byteLength !== 0) {
                throw new Error("mappedAtCreation range stayed attached");
            }

            var encoder = device.createCommandEncoder();
            encoder.copyBufferToBuffer(mapped, 0, readback, 0, 8);
            device.queue.submit([encoder.finish()]);
            device.queue.onSubmittedWorkDone().then(function () {
                return readback.mapAsync(1, 0, 8);
            }).then(function () {
                var range = readback.getMappedRange();
                log("mappedAtCreation:" + bytesOf(range));
                readback.unmap();
                mapped.destroy();
                readback.destroy();
                runWriteBufferRoundTrip();
            }).catch(fail);
        } catch (error) {
            fail(error);
        }
    }

    try {
        var stableMethod = device.createBuffer === device.createBuffer;
        labelBuffer = device.createBuffer({ size: 4, usage: 8, label: null });
        var nullLabel = labelBuffer.label;
        labelBuffer.label = "round-trip";
        log("buffer:" + nullLabel + "," + labelBuffer.label + ";method:" + stableMethod);

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

        gpu.requestAdapter().then(function (firstAdapter) {
            var order = [];
            var settleIndex = 0;
            var thenCount = 0;
            Object.defineProperty(Object.getPrototypeOf(firstAdapter), "then", {
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
                    log("tick:" + order.join(","));
                    runMappedAtCreationRoundTrip();
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
