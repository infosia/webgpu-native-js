#![warn(missing_docs)]

//! Raw WebGPU C ABI bindings generated from the pinned canonical `webgpu.h`.

#[cfg(not(any(
    feature = "backend-yawgpu",
    feature = "backend-wgpu-native",
    feature = "backend-dawn"
)))]
compile_error!(
    "enable exactly one backend feature: backend-yawgpu, backend-wgpu-native, or backend-dawn"
);

#[cfg(any(
    all(feature = "backend-yawgpu", feature = "backend-wgpu-native"),
    all(feature = "backend-yawgpu", feature = "backend-dawn"),
    all(feature = "backend-wgpu-native", feature = "backend-dawn")
))]
compile_error!("enable exactly one backend feature");

#[cfg(feature = "backend-dawn")]
compile_error!("not yet supported");

/// Raw WebGPU C ABI declarations generated from `webgpu.h`.
pub mod native {
    #![allow(missing_docs)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(non_upper_case_globals)]
    #![allow(clippy::all)]

    include!(concat!(env!("OUT_DIR"), "/webgpu_bindings.rs"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn create_instance_and_release_roundtrip() {
        let instance = unsafe { super::native::wgpuCreateInstance(std::ptr::null()) };
        assert!(!instance.is_null());

        unsafe {
            super::native::wgpuInstanceRelease(instance);
        }
    }

    #[test]
    fn instance_process_events_is_callable() {
        let instance = unsafe { super::native::wgpuCreateInstance(std::ptr::null()) };
        assert!(!instance.is_null());

        unsafe {
            super::native::wgpuInstanceProcessEvents(instance);
            super::native::wgpuInstanceRelease(instance);
        }
    }
}
