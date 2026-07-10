#![warn(missing_docs)]

//! JavaScriptCore adapter for `webgpu-native-js`.
//!
//! The `jsc` feature is enabled by default for this Apple-only (macOS/iOS)
//! Tier 1 adapter. The implementation compiles for macOS and iOS; iOS runtime
//! verification is deferred to mobile bring-up (block 06).

#[cfg(all(feature = "jsc", any(target_os = "macos", target_os = "ios")))]
mod imp;

#[cfg(all(feature = "jsc", any(target_os = "macos", target_os = "ios")))]
pub use imp::*;
