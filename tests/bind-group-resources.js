(function () {
    "use strict";

    var layouts = [
        { sampler: { type: "comparison" } },
        { texture: { sampleType: "uint", viewDimension: "2d", multisampled: false } },
        { storageTexture: {
            access: "write-only", format: "rgba8unorm", viewDimension: "2d"
        } }
    ].map(function (kind, binding) {
        var entry = { binding: binding, visibility: 4 };
        Object.assign(entry, kind);
        return device.createBindGroupLayout({ entries: [entry] });
    });
    if (layouts.length !== 3) {
        throw new Error("supported bind group layout kinds did not create");
    }

    [
        ["sampler", { type: "bad" }, "GPUSamplerBindingType"],
        ["texture", { sampleType: "bad" }, "GPUTextureSampleType"],
        ["storageTexture", {
            access: "bad", format: "rgba8unorm"
        }, "GPUStorageTextureAccess"]
    ].forEach(function (test) {
        var entry = { binding: 0, visibility: 4 };
        entry[test[0]] = test[1];
        var enumError;
        try {
            device.createBindGroupLayout({ entries: [entry] });
        } catch (caught) {
            enumError = caught;
        }
        if (!(enumError instanceof TypeError) || enumError.message !== test[2]) {
            throw new Error(test[0] + " enum rejection changed: " + enumError);
        }
    });

    var externalError;
    try {
        device.createBindGroupLayout({
            entries: [{ binding: 0, visibility: 1, externalTexture: {} }]
        });
    } catch (caught) {
        externalError = caught;
    }
    if (!(externalError instanceof TypeError) ||
        externalError.message !== "externalTexture bindings are not supported yet") {
        throw new Error("externalTexture rejection changed: " + externalError);
    }

    var bufferResourceLayout = device.createBindGroupLayout({
        entries: [{ binding: 0, visibility: 4, buffer: { type: "uniform" } }]
    });
    var textureResourceLayout = device.createBindGroupLayout({
        entries: [{ binding: 0, visibility: 4, texture: {} }]
    });
    var directBuffer = device.createBuffer({ size: 4, usage: 8 });
    var directTexture = device.createTexture({
        size: [1], format: "rgba8unorm", usage: 4
    });
    device.createBindGroup({
        layout: bufferResourceLayout,
        entries: [{ binding: 0, resource: directBuffer }]
    });
    device.createBindGroup({
        layout: textureResourceLayout,
        entries: [{ binding: 0, resource: directTexture }]
    });
    var resourceError;
    try {
        device.createBindGroup({
            layout: bufferResourceLayout,
            entries: [{ binding: 0, resource: { sampler: {} } }]
        });
    } catch (caught) {
        resourceError = caught;
    }
    if (!(resourceError instanceof TypeError) ||
        resourceError.message !== "resource must be a GPUBindingResource") {
        throw new Error("unknown resource did not retain the resource TypeError");
    }
    directBuffer.destroy();
    directTexture.destroy();
}());
