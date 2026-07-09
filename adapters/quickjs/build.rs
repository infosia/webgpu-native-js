use std::env;
use std::path::{Path, PathBuf};

const BACKEND_LIB_DIR_ENV: &str = "WEBGPU_NATIVE_JS_BACKEND_LIB_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={BACKEND_LIB_DIR_ENV}");

    if let Some(lib_dir) = env::var_os(BACKEND_LIB_DIR_ENV) {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            PathBuf::from(lib_dir).display()
        );
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("adapter crate lives under adapters/quickjs");
    let quickjs_dir = repo_root.join("third_party").join("quickjs");

    for file in [
        "quickjs.h",
        "quickjs.c",
        "libregexp.c",
        "libunicode.c",
        "dtoa.c",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            quickjs_dir.join(file).display()
        );
    }

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let static_wrappers = out_path.join("bindgen").join("extern");

    let bindings = bindgen::Builder::default()
        .header(quickjs_dir.join("quickjs.h").display().to_string())
        .clang_arg(format!("-I{}", quickjs_dir.display()))
        .wrap_static_fns(true)
        .wrap_static_fns_path(&static_wrappers)
        .allowlist_function("JS_.*")
        .allowlist_type("JS.*")
        .allowlist_var("JS_.*")
        .allowlist_var("JS_EVAL_.*")
        .allowlist_var("JS_PROP_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("generate quickjs bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("write quickjs bindings");

    cc::Build::new()
        .include(Path::new(&quickjs_dir))
        .define("_GNU_SOURCE", None)
        .flag_if_supported("-std=c11")
        .flag_if_supported("-Wno-unused-parameter")
        .file(quickjs_dir.join("quickjs.c"))
        .file(quickjs_dir.join("libregexp.c"))
        .file(quickjs_dir.join("libunicode.c"))
        .file(quickjs_dir.join("dtoa.c"))
        .file(static_wrappers.with_extension("c"))
        .compile("quickjs_ng");
}
