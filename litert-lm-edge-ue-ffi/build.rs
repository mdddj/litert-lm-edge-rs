use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let include_dir = crate_dir.join("include");
    fs::create_dir_all(&include_dir).expect("create include directory");

    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml")).unwrap_or_default();
    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("generate C bindings")
        .write_to_file(include_dir.join("litert_lm_edge_ue_ffi.h"));
}
