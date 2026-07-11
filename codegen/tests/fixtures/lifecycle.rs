/// Payload stored by a `GPUSampler` wrapper.
pub struct SamplerPayload {
    pub(super) sampler: WGPUSampler,
    pub(super) label: Mutex<String>,
}

// SAFETY: `SamplerPayload` stores WGPU handle values. Finalization only moves those values
// into `ReleaseRequest`; native handles are dereferenced only by
// `ReleaseRequest::run()` during release-queue drain on the creating `tick()` thread.
unsafe impl Send for SamplerPayload {}
    /// Release a `GPUSampler` and its retained descriptor handles.
    Sampler {
        /// Created native handle.
        sampler: WGPUSampler,
        /// Dispatch table used on the drain thread.
        gpu: GpuDispatch,
    },
    /// Release a conversion-created texture view without a wrapper parent.
    TextureViewOnly { /// Texture-view handle.
        texture_view: WGPUTextureView, /// Dispatch table.
        gpu: GpuDispatch },
            Self::Sampler { sampler, gpu } => unsafe {
/// Implements `GPUDevice.createSampler`.
pub fn device_create_sampler<E: JsEngine + 'static>(
    cx: E::Context<'_>,
    this: E::Value,
    args: &[E::Value],
) -> Result<E::Value, E::Error> {
    let device = device_handle::<E>(cx, this)?;
    let arena = Arena::new();
    let descriptor = args.first().copied().unwrap_or_else(|| E::undefined(cx));
    let native = convert_sampler_descriptor::<E>(cx, descriptor, &arena)?;
    let label = unsafe { string_view_to_owned(native.label) };
    let sampler = unsafe { (E::environment(cx).gpu().device_create_sampler)(device, ptr::from_ref(&native)) };
    if sampler.is_null() {
        return Err(E::operation_error(cx, "wgpuDeviceCreateSampler returned null"));
    }
    if let Err(error) = E::register_class(cx, sampler_class::<E>()) {
        unsafe {
            (E::environment(cx).gpu().sampler_release)(sampler);
        }
        return Err(error);
    }
    match E::new_instance(cx, GPU_SAMPLER_CLASS, Box::new(SamplerPayload {
        sampler,
        label: Mutex::new(label),
    })) {
        Ok(value) => Ok(value),
        Err(error) => {
            unsafe {
                (E::environment(cx).gpu().sampler_release)(sampler);
            }
            Err(error)
        }
    }
}

/// Implements the `GPUSampler.label` getter.
pub fn sampler_label_get<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value) -> Result<E::Value, E::Error> {
    let payload = E::payload(cx, this, GPU_SAMPLER_CLASS).and_then(|payload| payload.downcast_ref::<SamplerPayload>()).ok_or_else(|| E::type_error(cx, "GPUSampler.label called on an incompatible object"))?;
    let label = payload.label.lock().map_err(|_| E::operation_error(cx, "GPUSampler label is poisoned"))?;
    E::string(cx, &label)
}

/// Implements the `GPUSampler.label` setter.
pub fn sampler_label_set<E: JsEngine + 'static>(cx: E::Context<'_>, this: E::Value, value: E::Value) -> Result<(), E::Error> {
    let arena = Arena::new();
    let new_label = E::to_str(cx, value, &arena)?;
    let payload = E::payload(cx, this, GPU_SAMPLER_CLASS).and_then(|payload| payload.downcast_ref::<SamplerPayload>()).ok_or_else(|| E::type_error(cx, "GPUSampler.label called on an incompatible object"))?;
    let mut label = payload.label.lock().map_err(|_| E::operation_error(cx, "GPUSampler label is poisoned"))?;
    unsafe { (E::environment(cx).gpu().sampler_set_label)(payload.sampler, WGPUStringView::from_bytes(new_label.as_bytes())); }
    new_label.clone_into(&mut label);
    Ok(())
}

/// Finalizes a `GPUSampler` payload by enqueuing its release.
pub fn finalize_sampler(payload: Box<dyn Any + Send>, env: &Environment) {
    let Ok(payload) = payload.downcast::<SamplerPayload>() else { return; };
    let _ = env.queue().enqueue(ReleaseRequest::Sampler {
        sampler: payload.sampler,
        gpu: env.gpu(),
    });
}

pub(super) fn device_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_DEVICE_CLASS, || ClassSpec {
        name: "GPUDevice",
        id: GPU_DEVICE_CLASS,
        constructor: None,
        properties: &[],
        methods: Box::leak(Box::new([
            MethodSpec { name: "createSampler", length: 0, call: device_create_sampler::<E> },
        ])),
        finalizer: finalize_device::<E>,
    })
}

pub(super) fn sampler_class<E: JsEngine + 'static>() -> &'static ClassSpec<E> {
    class_spec_once::<E, _>(GPU_SAMPLER_CLASS, || ClassSpec {
        name: "GPUSampler",
        id: GPU_SAMPLER_CLASS,
        constructor: None,
        properties: Box::leak(Box::new([
            PropertySpec { name: "label", get: Some(sampler_label_get::<E>), set: Some(sampler_label_set::<E>) },
        ])),
        methods: &[],
        finalizer: finalize_sampler,
    })
}