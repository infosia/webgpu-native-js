use std::env;
use std::path::PathBuf;

const BACKEND_LIB_DIR_ENV: &str = "WEBGPU_NATIVE_JS_BACKEND_LIB_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={BACKEND_LIB_DIR_ENV}");

    if let Some(lib_dir) = env::var_os(BACKEND_LIB_DIR_ENV) {
        if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
            println!(
                "cargo:rustc-link-arg=-Wl,-rpath,{}",
                PathBuf::from(lib_dir).display()
            );
        }
    }
}
