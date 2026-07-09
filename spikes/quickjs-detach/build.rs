use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("spike crate lives under spikes/quickjs-detach");
    let quickjs_dir = repo_root.join("third_party").join("quickjs");

    println!(
        "cargo:rerun-if-changed={}",
        quickjs_dir.join("quickjs.h").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        quickjs_dir.join("quickjs.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        quickjs_dir.join("libregexp.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        quickjs_dir.join("libunicode.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        quickjs_dir.join("dtoa.c").display()
    );

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
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("generate quickjs bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("write quickjs bindings");

    cc::Build::new()
        .include(&quickjs_dir)
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
