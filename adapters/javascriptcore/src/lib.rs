#![warn(missing_docs)]

//! JavaScriptCore adapter for `webgpu-native-js`.
//!
//! The implementation is opt-in and available only when both the `jsc`
//! feature and the macOS target are selected.

#[cfg(all(feature = "jsc", target_os = "macos"))]
mod imp;

#[cfg(all(feature = "jsc", target_os = "macos"))]
pub use imp::*;
