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
})();
