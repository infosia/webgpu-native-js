(function () {
    "use strict";

    globalThis.uncapturedEventPassed = false;
    globalThis.uncapturedListenerPassed = false;
    globalThis.deviceLostPassed = false;

    if (device.lost !== device.lost) {
        throw new Error("device.lost was not cached");
    }

    function checkEvent(event) {
        if (!(event instanceof GPUUncapturedErrorEvent) || !(event instanceof Event)) {
            throw new Error("uncaptured callback did not receive an Event");
        }
        if (!(event.error instanceof GPUValidationError)) {
            throw new Error("uncaptured error had the wrong class");
        }
        if (event.error !== event.error || event.error.message !== "script uncaptured") {
            throw new Error("uncaptured error message mismatch");
        }
    }

    var removedListenerCalled = false;
    function removedListener() { removedListenerCalled = true; }
    device.addEventListener("uncapturederror", function (event) {
        checkEvent(event);
        if (event.defaultPrevented) {
            throw new Error("event started defaultPrevented");
        }
        event.preventDefault();
        if (!event.defaultPrevented) {
            throw new Error("preventDefault was not observable");
        }
        globalThis.uncapturedListenerPassed = true;
    });
    device.addEventListener("uncapturederror", removedListener);
    device.removeEventListener("uncapturederror", removedListener);

    device.onuncapturederror = function (event) {
        checkEvent(event);
        if (!event.defaultPrevented || removedListenerCalled) {
            throw new Error("listener dispatch/removal mismatch");
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
