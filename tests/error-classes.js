(function () {
    const cases = [
        [GPUValidationError, "validation message"],
        [GPUOutOfMemoryError, "out-of-memory message"],
        [GPUInternalError, "internal message"],
    ];
    for (const [ErrorClass, message] of cases) {
        if (ErrorClass.length !== 1) throw new Error(`${ErrorClass.name} length failed`);
        const error = new ErrorClass(message);
        if (!(error instanceof ErrorClass)) throw new Error(`${ErrorClass.name} instanceof failed`);
        if (!(error instanceof GPUError)) throw new Error(`${ErrorClass.name} base instanceof failed`);
        if (error.message !== message) throw new Error(`${ErrorClass.name} message failed`);
    }
    let baseRejected = false;
    try { new GPUError("not allowed"); }
    catch (error) { baseRejected = error instanceof TypeError; }
    if (!baseRejected) throw new Error("GPUError constructor was accepted");

    class DerivedValidationError extends GPUValidationError {}
    const derived = new DerivedValidationError("derived message");
    if (!(derived instanceof DerivedValidationError)) throw new Error("derived instanceof failed");
    if (!(derived instanceof GPUValidationError)) throw new Error("derived base instanceof failed");
    if (Object.getPrototypeOf(derived) !== DerivedValidationError.prototype) {
        throw new Error("new-target prototype was not applied");
    }
    if (derived.message !== "derived message") throw new Error("derived message failed");
})();
