use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

const BACKEND_LIB_DIR_ENV: &str = "WEBGPU_NATIVE_JS_BACKEND_LIB_DIR";

#[derive(Clone, Copy)]
struct Backend {
    feature_env: &'static str,
    cargo_feature: &'static str,
    library: &'static str,
    pkg_config_names: &'static [&'static str],
}

const BACKENDS: &[Backend] = &[
    Backend {
        feature_env: "CARGO_FEATURE_BACKEND_YAWGPU",
        cargo_feature: "backend-yawgpu",
        library: "yawgpu",
        pkg_config_names: &["yawgpu"],
    },
    Backend {
        feature_env: "CARGO_FEATURE_BACKEND_WGPU_NATIVE",
        cargo_feature: "backend-wgpu-native",
        library: "wgpu_native",
        pkg_config_names: &["wgpu-native", "wgpu_native"],
    },
    Backend {
        feature_env: "CARGO_FEATURE_BACKEND_DAWN",
        cargo_feature: "backend-dawn",
        library: "webgpu_dawn",
        pkg_config_names: &[],
    },
];

fn main() {
    println!("cargo:rerun-if-env-changed={BACKEND_LIB_DIR_ENV}");
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS is set by Cargo");

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let repo_root = manifest_dir
        .parent()
        .expect("ffi crate lives directly under the repository root");
    let header = repo_root
        .join("third_party")
        .join("webgpu-headers")
        .join("webgpu.h");

    println!("cargo:rerun-if-changed={}", header.display());

    let bindings = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen can generate bindings from the canonical webgpu.h");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    bindings
        .write_to_file(out_dir.join("webgpu_bindings.rs"))
        .expect("generated bindings can be written to OUT_DIR");

    let enabled_backends: Vec<Backend> = BACKENDS
        .iter()
        .copied()
        .filter(|backend| env::var_os(backend.feature_env).is_some())
        .collect();

    if enabled_backends.len() != 1 {
        return;
    }

    let backend = enabled_backends[0];
    let lib_dir = resolve_backend_lib_dir(backend);

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if target_os == "windows" {
        // For MSVC dylibs, rustc appends `.lib` to the requested name. Requesting
        // `<library>.dll` therefore makes it link Cargo's `<library>.dll.lib` import library.
        println!("cargo:rustc-link-lib=dylib={}.dll", backend.library);
    } else {
        println!("cargo:rustc-link-lib=dylib={}", backend.library);
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }
}

fn resolve_backend_lib_dir(backend: Backend) -> PathBuf {
    if let Some(dir) = env::var_os(BACKEND_LIB_DIR_ENV) {
        let dir = PathBuf::from(dir);
        if has_dynamic_library(&dir, backend.library) {
            return dir;
        }

        panic!(
            "could not locate the backend dynamic library for feature '{}': {BACKEND_LIB_DIR_ENV} was set to '{}', but it does not contain the expected dynamic library (library name '{}')",
            backend.cargo_feature,
            dir.display(),
            backend.library
        );
    }

    if let Some(dir) = pkg_config_lib_dir(backend) {
        return dir;
    }

    if backend.pkg_config_names.is_empty() {
        panic!(
            "could not locate the backend dynamic library for feature '{}'. Set {BACKEND_LIB_DIR_ENV} to the directory containing the expected dynamic library '{}'.",
            backend.cargo_feature, backend.library
        );
    } else {
        panic!(
            "could not locate the backend dynamic library for feature '{}'. Set {BACKEND_LIB_DIR_ENV} to the directory containing the expected dynamic library '{}', or install a pkg-config file for the backend.",
            backend.cargo_feature, backend.library
        );
    }
}

fn has_dynamic_library(dir: &Path, library: &str) -> bool {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let dll = dir.join(format!("{library}.dll"));
        let import_library = dir.join(format!("{library}.dll.lib"));
        if dll.is_file() && !import_library.is_file() {
            panic!(
                "found '{}', but not '{}': MSVC linking needs the import library next to the DLL",
                dll.display(),
                import_library.display()
            );
        }
        return dll.is_file() && import_library.is_file();
    }

    let candidates = [format!("lib{library}.dylib"), format!("lib{library}.so")];

    candidates
        .iter()
        .map(Path::new)
        .any(|file_name| dir.join(file_name).is_file())
}

fn pkg_config_lib_dir(backend: Backend) -> Option<PathBuf> {
    backend
        .pkg_config_names
        .iter()
        .find_map(|name| pkg_config_variable(name, "libdir"))
        .filter(|dir| has_dynamic_library(dir, backend.library))
}

fn pkg_config_variable(package: &str, variable: &str) -> Option<PathBuf> {
    let variable_arg = format!("--variable={variable}");
    let output = Command::new("pkg-config")
        .args([variable_arg, package.to_owned()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}
