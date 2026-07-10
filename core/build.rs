use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use webgpu_native_js_codegen::generate_conversions;

const BACKEND_LIB_DIR_ENV: &str = "WEBGPU_NATIVE_JS_BACKEND_LIB_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={BACKEND_LIB_DIR_ENV}");

    generate_descriptor_conversions();

    if let Some(lib_dir) = env::var_os(BACKEND_LIB_DIR_ENV) {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            PathBuf::from(lib_dir).display()
        );
    }
}

fn generate_descriptor_conversions() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"));
    let repository = manifest_dir
        .parent()
        .expect("core is below repository root");
    let idl_path = repository.join("third_party/gpuweb/webgpu.idl");
    let yaml_path = repository.join("third_party/webgpu-headers/webgpu.yml");

    for path in [&idl_path, &yaml_path] {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let emitted = generate_conversions(&read_input(&idl_path), &read_input(&yaml_path))
        .expect("pinned WebGPU conversion inputs must generate");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"))
        .join("generated_conversions.rs");
    fs::write(output, emitted).expect("write generated descriptor conversions");
}

fn read_input(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}
