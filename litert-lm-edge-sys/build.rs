#[cfg(not(feature = "generate-bindings"))]
use std::path::Path;
#[cfg(feature = "generate-bindings")]
use std::path::{Path, PathBuf};
use std::{env, fs};

fn main() {
    validate_link_mode();

    println!("cargo:rerun-if-env-changed=LITERT_LM_ROOT");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LIB_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LINK_LIB");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LINK_KIND");

    link_runtime();

    #[cfg(feature = "generate-bindings")]
    generate_bindings();
}

fn validate_link_mode() {
    if !cfg!(feature = "system")
        && !cfg!(feature = "vendor-darwin-arm64")
        && !cfg!(feature = "vendor-linux-x86_64")
        && !cfg!(feature = "vendor-windows-x86_64")
    {
        panic!(
            "enable a LiteRT-LM link mode: vendor-darwin-arm64, vendor-linux-x86_64, vendor-windows-x86_64, or system"
        );
    }
}

fn link_runtime() {
    if cfg!(feature = "system") {
        link_system_runtime();
        return;
    }

    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target == "aarch64-apple-darwin" && cfg!(feature = "vendor-darwin-arm64") {
        link_vendor_darwin_arm64();
    } else if target == "x86_64-unknown-linux-gnu" && cfg!(feature = "vendor-linux-x86_64") {
        link_vendor_linux_x86_64();
    } else if target == "x86_64-pc-windows-msvc" && cfg!(feature = "vendor-windows-x86_64") {
        link_vendor_windows_x86_64();
    } else {
        panic!(
            "no bundled LiteRT-LM runtime is enabled for {target}; enable a matching vendor feature or use system mode"
        );
    }
}

fn link_system_runtime() {
    let lib_dir = env::var("LITERT_LM_LIB_DIR").unwrap_or_else(|_| {
        panic!(
            "LITERT_LM_LIB_DIR must be set when building litert-lm-edge-sys with the system feature"
        )
    });
    let link_kind = env::var("LITERT_LM_LINK_KIND").unwrap_or_else(|_| "dylib".to_owned());
    let link_lib = env::var("LITERT_LM_LINK_LIB").unwrap_or_else(|_| "litert_lm_c_api".to_owned());

    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib={link_kind}={link_lib}");
}

fn link_vendor_darwin_arm64() {
    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target != "aarch64-apple-darwin" {
        panic!(
            "the bundled LiteRT-LM runtime supports only aarch64-apple-darwin; \
             enable a matching vendor feature or use system mode for {target}"
        );
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vendor_dir = manifest_dir.join("vendor").join("darwin-arm64");
    let dylib = vendor_dir.join("liblitert_lm_c_api.dylib");
    if !dylib.is_file() {
        panic!(
            "missing bundled LiteRT-LM runtime at {}; run scripts/prepare_litert_lm_darwin_arm64.sh",
            dylib.display()
        );
    }

    println!("cargo:rerun-if-changed={}", dylib.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor_dir.display());
}

fn link_vendor_linux_x86_64() {
    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target != "x86_64-unknown-linux-gnu" {
        panic!(
            "the bundled Linux LiteRT-LM runtime supports only x86_64-unknown-linux-gnu; \
             enable a matching vendor feature or use system mode for {target}"
        );
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vendor_dir = manifest_dir.join("vendor").join("linux-x86_64");
    let so = vendor_dir.join("liblitert_lm_c_api.so");
    if !so.is_file() {
        panic!(
            "missing bundled Linux LiteRT-LM runtime at {}; run scripts/prepare_litert_lm_linux_x86_64.sh on Linux",
            so.display()
        );
    }

    println!("cargo:rerun-if-changed={}", so.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vendor_dir.display());
}

fn link_vendor_windows_x86_64() {
    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target != "x86_64-pc-windows-msvc" {
        panic!(
            "the bundled Windows LiteRT-LM runtime supports only x86_64-pc-windows-msvc; \
             enable a matching vendor feature or use system mode for {target}"
        );
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vendor_dir = manifest_dir.join("vendor").join("windows-x86_64");
    let import_lib = vendor_dir.join("litert_lm_c_api.lib");
    let dll = vendor_dir.join("litert_lm_c_api.dll");
    if !import_lib.is_file() || !dll.is_file() {
        panic!(
            "missing bundled Windows LiteRT-LM runtime at {}; run scripts/prepare_litert_lm_windows_x86_64.ps1 on Windows",
            vendor_dir.display()
        );
    }

    println!("cargo:rerun-if-changed={}", import_lib.display());
    println!("cargo:rerun-if-changed={}", dll.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
}

fn copy_vendor_runtimes_to_target_dirs(vendor_dir: &Path) {
    let out_dir = Path::new(&env::var("OUT_DIR").expect("Cargo sets OUT_DIR")).to_path_buf();
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .unwrap_or_else(|| panic!("unexpected OUT_DIR layout: {}", out_dir.display()));

    for dir in [
        profile_dir.to_path_buf(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ] {
        fs::create_dir_all(&dir)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", dir.display()));
        for entry in fs::read_dir(vendor_dir)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", vendor_dir.display()))
        {
            let path = entry
                .unwrap_or_else(|error| panic!("failed to read vendor entry: {error}"))
                .path();
            let extension = path.extension().and_then(|extension| extension.to_str());
            if matches!(extension, Some("dylib" | "dll" | "so")) {
                let file_name = path
                    .file_name()
                    .unwrap_or_else(|| panic!("vendor dylib has no file name: {}", path.display()));
                let destination = dir.join(file_name);
                let _ = fs::remove_file(&destination);
                fs::copy(&path, destination)
                    .unwrap_or_else(|error| panic!("failed to copy LiteRT-LM runtime: {error}"));
            }
        }
    }
}

#[cfg(feature = "generate-bindings")]
fn generate_bindings() {
    let root = env::var("LITERT_LM_ROOT").unwrap_or_else(|_| {
        panic!(
            "LITERT_LM_ROOT must point to a LiteRT-LM checkout when generate-bindings is enabled"
        )
    });
    let header = Path::new(&root).join("c").join("engine.h");
    println!("cargo:rerun-if-changed={}", header.display());

    let mut builder = bindgen::Builder::default()
        .header(header.display().to_string())
        .allowlist_function("litert_lm_.*")
        .allowlist_type("LiteRtLm.*")
        .allowlist_var("kLiteRtLm.*")
        .layout_tests(false)
        .derive_debug(true)
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    builder = builder.clang_arg(format!("-I{}", Path::new(&root).join("c").display()));

    let bindings = builder
        .generate()
        .expect("failed to generate LiteRT-LM bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write generated bindings");
}
