# litert-lm-edge-rs

Rust bindings for the LiteRT-LM C API. This workspace binds the stable C ABI in
`c/engine.h`, not the C++ classes.

## Crates

- `litert-lm-edge-sys`: raw unsafe FFI bindings and native library linking.
- `litert-lm-edge`: safe wrapper for engine/session generation, streaming,
  multimodal inputs, conversations, and tool calling.

## Default Native Runtimes

The default build links bundled native runtimes for Apple Silicon macOS, Linux
x86_64 GNU, and Windows x86_64 MSVC:

```text
litert-lm-edge-sys/vendor/darwin-arm64/liblitert_lm_c_api.dylib
litert-lm-edge-sys/vendor/linux-x86_64/liblitert_lm_c_api.so
litert-lm-edge-sys/vendor/windows-x86_64/litert_lm_c_api.dll
litert-lm-edge-sys/vendor/windows-x86_64/litert_lm_c_api.lib
```

That means users on those targets do not need `LITERT_LM_LIB_DIR`,
`LITERT_LM_LINK_LIB`, Bazel, or a LiteRT-LM source checkout. They only need a
`.litertlm` model file at runtime. Other targets should use `system` mode.

The bundled runtimes are built from `google-ai-edge/LiteRT-LM` `v0.12.0` with
the CPU-only C API target. GPU, Metal, NPU, vision, and audio settings are
exposed in Rust, but the bundled runtimes are intentionally CPU-first. Use
`system` mode for a custom native build.

## Build Modes

Default bundled runtime:

```bash
cargo check --workspace
cargo run -p litert-lm-edge --example simple_generate -- /path/to/model.litertlm "你好"
```

System runtime for custom LiteRT-LM builds or other platforms:

```bash
export LITERT_LM_LIB_DIR=/path/to/native/lib
export LITERT_LM_LINK_LIB=litert_lm_c_api
export LITERT_LM_LINK_KIND=dylib
cargo check --workspace --no-default-features --features system
```

Windows bundled runtime:

```powershell
cargo check --workspace
cargo run -p litert-lm-edge --example simple_generate -- C:\path\to\model.litertlm "hello"
```

Regenerate bindgen bindings from a local LiteRT-LM checkout:

```bash
export LITERT_LM_ROOT=/path/to/LiteRT-LM
cargo check -p litert-lm-edge-sys --features generate-bindings
```

## Preparing The Bundled Runtime

The bundled runtime can be rebuilt on Apple Silicon macOS:

```bash
scripts/prepare_litert_lm_darwin_arm64.sh
```

The script downloads LiteRT-LM `v0.12.0` into `.litert-lm-build/`, builds a
shared CPU C API library with Bazel/Bazelisk, copies it into
`litert-lm-edge-sys/vendor/darwin-arm64/`, and writes `VERSION` plus
`SHA256SUMS`.

The Windows runtime must be built on Windows x86_64 with MSVC Build Tools:

```powershell
scripts\prepare_litert_lm_windows_x86_64.ps1
```

There is also a manual GitHub Actions workflow:

```text
.github/workflows/build-windows-runtime.yml
```

Run it from GitHub, download the `litert-lm-edge-windows-x86_64-runtime`
artifact, and copy its contents into
`litert-lm-edge-sys/vendor/windows-x86_64/`.

The Linux x86_64 GNU runtime must be built on Linux x86_64:

```bash
scripts/prepare_litert_lm_linux_x86_64.sh
```

There is also a manual GitHub Actions workflow:

```text
.github/workflows/build-linux-runtime.yml
```

Run it from GitHub, download the `litert-lm-edge-linux-x86_64-runtime`
artifact, and copy its contents into
`litert-lm-edge-sys/vendor/linux-x86_64/`.

## Examples

Text generation:

```bash
cargo run -p litert-lm-edge --example simple_generate -- /path/to/model.litertlm "hello"
```

Streaming generation:

```bash
cargo run -p litert-lm-edge --example stream_generate -- /path/to/model.litertlm "hello"
```

Multimodal generation with an image file:

```bash
cargo run -p litert-lm-edge --example multimodal_generate -- \
  /path/to/model.litertlm /path/to/image.png "describe this image"
```

Conversation with tool calling:

```bash
cargo run -p litert-lm-edge --example conversation_tools -- \
  /path/to/model.litertlm "what is the weather in Shanghai?"
```

Conversation with image content:

```bash
cargo run -p litert-lm-edge --example conversation_multimodal -- \
  /path/to/model.litertlm /path/to/image.png "what is in this image?"
```

Raw JSON conversation escape hatch:

```bash
cargo run -p litert-lm-edge --example raw_conversation_json -- \
  /path/to/model.litertlm '{"role":"user","content":[{"type":"text","text":"hello"}]}'
```

Engine configuration options:

```bash
cargo run -p litert-lm-edge --example engine_config -- /path/to/model.litertlm
```
