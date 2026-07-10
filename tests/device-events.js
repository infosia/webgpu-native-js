(function () {
    "use strict";

    globalThis.uncapturedEventPassed = false;
    globalThis.deviceLostPassed = false;

    if (device.lost !== device.lost) {
        throw new Error("device.lost was not cached");
    }

    device.onuncapturederror = function (error) {
        if (!(error instanceof GPUValidationError)) {
            throw new Error("uncaptured error had the wrong class");
        }
        if (error.message !== "script uncaptured") {
            throw new Error("uncaptured error message mismatch");
        }
        globalThis.uncapturedEventPassed = true;
    };

    device.lost.then(function (info) {
        if (info.reason !== "destroyed") {
            throw new Error("lost reason mismatch: " + info.reason);
        }
        if (info.message !== "script lost") {
            throw new Error("lost message mismatch");
        }
        globalThis.deviceLostPassed = true;
    });
}());
