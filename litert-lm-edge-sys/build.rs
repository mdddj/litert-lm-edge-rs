use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use sha2::{Digest, Sha256};

// Runtime loading note
//
// `cargo:rustc-link-arg` applies only to the package that emits it, so a
// dependency cannot inject an rpath into a downstream binary. The macOS dylib
// therefore carries an `@loader_path/liblitert_lm_c_api.dylib` install name:
// the loader resolves it against the directory of the binary that loads it,
// which is exactly where `copy_vendor_runtimes_to_target_dirs` puts every
// runtime. No rpath is needed. Linux has no equivalent, so consumers that ship
// a binary must add `-Wl,-rpath,$ORIGIN` themselves; see the README.

/// Platforms with a published runtime asset, matching Cargo feature names and
/// the `vendor/<platform>` directory layout.
const PLATFORM_DARWIN_ARM64: &str = "darwin-arm64";
const PLATFORM_LINUX_X86_64: &str = "linux-x86_64";
const PLATFORM_WINDOWS_X86_64: &str = "windows-x86_64";

/// Default asset host: the GitHub Release whose tag matches this crate version.
const DEFAULT_RELEASE_BASE: &str = "https://github.com/mdddj/litert-lm-edge-rs/releases";

fn main() {
    validate_link_mode();

    println!("cargo:rerun-if-env-changed=LITERT_LM_ROOT");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LIB_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LINK_LIB");
    println!("cargo:rerun-if-env-changed=LITERT_LM_LINK_KIND");
    println!("cargo:rerun-if-env-changed=LITERT_LM_RUNTIME_BASE_URL");
    println!("cargo:rerun-if-env-changed=LITERT_LM_RUNTIME_CACHE_DIR");
    println!("cargo:rerun-if-env-changed=LITERT_LM_RUNTIME_OFFLINE");

    let checksums = manifest_dir().join("runtime-checksums.txt");
    if checksums.is_file() {
        println!("cargo:rerun-if-changed={}", checksums.display());
    }

    link_runtime();

    #[cfg(feature = "generate-bindings")]
    generate_bindings();
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
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

    let vendor_dir = resolve_runtime_dir(PLATFORM_DARWIN_ARM64);
    let dylib = vendor_dir.join("liblitert_lm_c_api.dylib");
    require_runtime_file(&dylib, PLATFORM_DARWIN_ARM64);

    println!("cargo:rerun-if-changed={}", dylib.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
}

fn link_vendor_linux_x86_64() {
    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target != "x86_64-unknown-linux-gnu" {
        panic!(
            "the bundled Linux LiteRT-LM runtime supports only x86_64-unknown-linux-gnu; \
             enable a matching vendor feature or use system mode for {target}"
        );
    }

    let vendor_dir = resolve_runtime_dir(PLATFORM_LINUX_X86_64);
    let so = vendor_dir.join("liblitert_lm_c_api.so");
    require_runtime_file(&so, PLATFORM_LINUX_X86_64);

    println!("cargo:rerun-if-changed={}", so.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
}

fn link_vendor_windows_x86_64() {
    let target = env::var("TARGET").expect("Cargo sets TARGET");
    if target != "x86_64-pc-windows-msvc" {
        panic!(
            "the bundled Windows LiteRT-LM runtime supports only x86_64-pc-windows-msvc; \
             enable a matching vendor feature or use system mode for {target}"
        );
    }

    let vendor_dir = resolve_runtime_dir(PLATFORM_WINDOWS_X86_64);
    let import_lib = vendor_dir.join("litert_lm_c_api.lib");
    let dll = vendor_dir.join("litert_lm_c_api.dll");
    require_runtime_file(&import_lib, PLATFORM_WINDOWS_X86_64);
    require_runtime_file(&dll, PLATFORM_WINDOWS_X86_64);

    println!("cargo:rerun-if-changed={}", import_lib.display());
    println!("cargo:rerun-if-changed={}", dll.display());
    copy_vendor_runtimes_to_target_dirs(&vendor_dir);
    println!("cargo:rustc-link-search=native={}", vendor_dir.display());
    println!("cargo:rustc-link-lib=dylib=litert_lm_c_api");
}

/// Returns the directory holding the runtime for `platform`.
///
/// A checked-out `vendor/<platform>` directory wins, which keeps the
/// `scripts/prepare_litert_lm_*` development flow network-free. Published crates
/// omit `vendor/`, so the runtime is fetched into a cache inside `target/`.
fn resolve_runtime_dir(platform: &str) -> PathBuf {
    let local = manifest_dir().join("vendor").join(platform);
    if is_populated_runtime_dir(&local) {
        return local;
    }
    fetch_runtime(platform)
}

fn is_populated_runtime_dir(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .filter_map(Result::ok)
        .any(|entry| entry.file_name() == "SHA256SUMS" || is_runtime_library(&entry.path()))
}

fn is_runtime_library(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("dylib" | "dll" | "so")
    )
}

/// `<target>/litert-lm-runtime`, so every profile and binary in a workspace
/// shares one download. `LITERT_LM_RUNTIME_CACHE_DIR` overrides it (CI caches).
fn runtime_cache_dir() -> PathBuf {
    if let Some(dir) = env::var_os("LITERT_LM_RUNTIME_CACHE_DIR") {
        return PathBuf::from(dir);
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let target_root = out_dir
        .ancestors()
        .nth(4)
        .unwrap_or_else(|| panic!("unexpected OUT_DIR layout: {}", out_dir.display()));
    target_root.join("litert-lm-runtime")
}

fn fetch_runtime(platform: &str) -> PathBuf {
    let destination = runtime_cache_dir().join(platform);
    if is_populated_runtime_dir(&destination) {
        return destination;
    }

    if env::var_os("LITERT_LM_RUNTIME_OFFLINE").is_some()
        || env::var("CARGO_NET_OFFLINE").as_deref() == Ok("true")
    {
        panic!(
            "the LiteRT-LM runtime for {platform} is not available locally and downloads are \
             disabled (LITERT_LM_RUNTIME_OFFLINE / CARGO_NET_OFFLINE). Either run the matching \
             scripts/prepare_litert_lm_*.sh (or .ps1) to populate vendor/{platform}, or enable \
             the `system` feature with LITERT_LM_LIB_DIR pointing at an existing installation"
        );
    }

    let (base_url, is_default_base) = runtime_base_url();
    let url = format!("{base_url}/litert-lm-runtime-{platform}.tar.gz");
    println!("cargo:warning=fetching LiteRT-LM runtime for {platform} from {url}");

    let archive = download(&url, platform);
    verify_archive_checksum(platform, &archive, is_default_base);
    extract_runtime(&archive, &destination);

    println!(
        "cargo:warning=LiteRT-LM runtime for {platform} ready at {}",
        destination.display()
    );
    destination
}

fn runtime_base_url() -> (String, bool) {
    match env::var("LITERT_LM_RUNTIME_BASE_URL") {
        Ok(base_url) => (base_url.trim_end_matches('/').to_owned(), false),
        Err(_) => (
            format!(
                "{DEFAULT_RELEASE_BASE}/download/v{}",
                env!("CARGO_PKG_VERSION")
            ),
            true,
        ),
    }
}

fn download(url: &str, platform: &str) -> Vec<u8> {
    if let Some(path) = local_source_path(url) {
        return fs::read(&path).unwrap_or_else(|error| {
            panic!("failed to read runtime archive {}: {error}", path.display())
        });
    }

    let cache_dir = runtime_cache_dir();
    fs::create_dir_all(&cache_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", cache_dir.display()));
    let partial = cache_dir.join(format!(
        ".download-{platform}-{}.tar.gz",
        std::process::id()
    ));
    let partial_arg = partial.display().to_string();

    let result = if cfg!(windows) {
        run_powershell_download(url, &partial_arg).or_else(|_| run_curl(url, &partial_arg))
    } else {
        run_curl(url, &partial_arg)
    };

    let bytes = result.and_then(|()| fs::read(&partial).map_err(|error| error.to_string()));
    let _ = fs::remove_file(&partial);

    match bytes {
        Ok(bytes) => bytes,
        Err(error) => panic!(
            "failed to download the LiteRT-LM runtime from {url}: {error}\n\
             Install `curl`, set LITERT_LM_RUNTIME_BASE_URL to a reachable mirror, or populate \
             vendor/{platform} from a local build"
        ),
    }
}

fn run_curl(url: &str, destination: &str) -> Result<(), String> {
    let output = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
            destination,
            url,
        ])
        .output()
        .map_err(|error| format!("curl is not runnable: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(format!("curl exited with {}: {stderr}", output.status))
}

fn run_powershell_download(url: &str, destination: &str) -> Result<(), String> {
    let script = format!(
        "$ProgressPreference='SilentlyContinue'; \
         Invoke-WebRequest -Uri '{}' -OutFile '{}'",
        url.replace('\'', "''"),
        destination.replace('\'', "''"),
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| format!("powershell is not runnable: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(format!(
        "powershell exited with {}: {stderr}",
        output.status
    ))
}

/// Accepts `file://` URLs and bare paths so local assets can be tested without
/// publishing a release.
fn local_source_path(url: &str) -> Option<PathBuf> {
    if let Some(rest) = url.strip_prefix("file://") {
        return Some(PathBuf::from(rest));
    }
    if url.contains("://") {
        return None;
    }
    let path = PathBuf::from(url);
    path.is_file().then_some(path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn verify_archive_checksum(platform: &str, archive: &[u8], is_default_base: bool) {
    let actual = sha256_hex(archive);

    // The pin is authoritative whenever it exists, including for mirrors and
    // local fixtures: it is what stops a compromised host from substituting a
    // different runtime. The manifest inside the archive only catches corruption.
    match pinned_checksum(platform) {
        Some(expected) => {
            if expected != actual {
                panic!(
                    "SHA-256 mismatch for the {platform} runtime archive:\n  expected {expected}\n  actual   {actual}\n\
                     The asset does not match the hash pinned in runtime-checksums.txt. Refusing to continue.\n\
                     If you intentionally rebuilt the runtime, regenerate the pin with \
                     python3 scripts/build_runtime_assets.py and re-upload the release assets."
                );
            }
        }
        None if is_default_base => panic!(
            "runtime-checksums.txt has no pinned SHA-256 for {platform}; regenerate it with \
             python3 scripts/build_runtime_assets.py after uploading the runtime assets"
        ),
        None => println!(
            "cargo:warning=no pinned SHA-256 for the {platform} runtime in runtime-checksums.txt; \
             relying on the manifest inside the archive"
        ),
    }
}

fn pinned_checksum(platform: &str) -> Option<String> {
    let contents = fs::read_to_string(manifest_dir().join("runtime-checksums.txt")).ok()?;
    contents.lines().find_map(|line| {
        let line = line.split('#').next()?.trim();
        let mut fields = line.split_whitespace();
        let name = fields.next()?;
        if name != platform {
            return None;
        }
        fields.next().map(str::to_owned)
    })
}

fn extract_runtime(archive: &[u8], destination: &Path) {
    let parent = destination
        .parent()
        .unwrap_or_else(|| panic!("invalid runtime cache path {}", destination.display()));
    fs::create_dir_all(parent)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", parent.display()));

    let platform = destination
        .file_name()
        .unwrap_or_else(|| panic!("invalid runtime cache path {}", destination.display()))
        .to_string_lossy()
        .into_owned();
    let staging = parent.join(format!(".{platform}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", staging.display()));

    let mut archive_reader = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    archive_reader
        .unpack(&staging)
        .unwrap_or_else(|error| panic!("failed to unpack the LiteRT-LM runtime archive: {error}"));

    let staged = staging.join(&platform);
    if !is_populated_runtime_dir(&staged) {
        let _ = fs::remove_dir_all(&staging);
        panic!(
            "the LiteRT-LM runtime archive did not contain a {platform} directory; \
             the release asset is malformed"
        );
    }
    verify_manifest(&staged, &platform);

    match fs::rename(&staged, destination) {
        Ok(()) => {
            let _ = fs::remove_dir_all(&staging);
        }
        Err(_) if is_populated_runtime_dir(destination) => {
            // Another build script won the race; its copy is equivalent.
            let _ = fs::remove_dir_all(&staging);
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            panic!(
                "failed to install the LiteRT-LM runtime into {}: {error}",
                destination.display()
            );
        }
    }
}

/// Verifies every entry of the `SHA256SUMS` manifest written by the
/// `scripts/prepare_litert_lm_*` build scripts.
fn verify_manifest(directory: &Path, platform: &str) {
    let manifest_path = directory.join("SHA256SUMS");
    let manifest = match fs::read_to_string(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => panic!(
            "the {platform} runtime archive is missing SHA256SUMS ({}): {error}",
            manifest_path.display()
        ),
    };

    let mut verified = 0usize;
    for line in manifest.lines() {
        let mut fields = line.split_whitespace();
        let (Some(expected), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        let name = name.trim_start_matches('*');
        let path = directory.join(name);
        let actual = sha256_file(&path).unwrap_or_else(|error| {
            panic!(
                "failed to read {} from the runtime archive: {error}",
                path.display()
            )
        });
        if expected != actual {
            panic!(
                "SHA-256 mismatch for {name} in the {platform} runtime archive:\n  expected {expected}\n  actual   {actual}"
            );
        }
        verified += 1;
    }

    if verified == 0 {
        panic!("the {platform} runtime archive has an empty SHA256SUMS manifest");
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn require_runtime_file(path: &Path, platform: &str) {
    if path.is_file() {
        return;
    }
    panic!(
        "missing bundled LiteRT-LM runtime at {}; run the matching \
         scripts/prepare_litert_lm_*.sh (or .ps1) for {platform}, or allow the automatic \
         runtime download",
        path.display()
    );
}

fn copy_vendor_runtimes_to_target_dirs(vendor_dir: &Path) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));
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
            if !is_runtime_library(&path) {
                continue;
            }
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

#[cfg(feature = "generate-bindings")]
fn generate_bindings() {
    let root = env::var("LITERT_LM_ROOT").unwrap_or_else(|_| {
        panic!(
            "LITERT_LM_ROOT must point to a LiteRT-LM checkout when generate-bindings is enabled"
        )
    });
    let header = Path::new(&root).join("c").join("conversation.h");
    println!("cargo:rerun-if-changed={}", header.display());

    let mut builder = bindgen::Builder::default()
        .header(header.display().to_string())
        .allowlist_function("litert_lm_.*")
        .allowlist_type("LiteRtLm.*")
        .allowlist_var("kLiteRtLm.*")
        .prepend_enum_name(false)
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
