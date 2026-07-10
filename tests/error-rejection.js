globalThis.errorRejectionDone = false;
device.popErrorScope().then(function () {
    throw new Error("empty pop unexpectedly resolved");
}).catch(function (reason) {
    if (!(reason instanceof Error)) throw new Error("rejection reason is not an Error");
    if (reason.name !== "OperationError") throw new Error("rejection name mismatch");
    if (!reason.message) throw new Error("rejection message was empty");
    if (String(reason).indexOf("OperationError") < 0) throw new Error("error does not stringify usefully");
    globalThis.errorRejectionDone = true;
});
