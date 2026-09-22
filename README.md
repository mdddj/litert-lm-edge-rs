# litert-lm-edge-rs

[English](README.md) | [简体中文](README.zh-CN.md)

Rust bindings for the LiteRT-LM C API in `c/engine.h` and `c/conversation.h`,
not the C++ classes.

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

The exact upstream tag for each checked-in runtime is recorded in
`litert-lm-edge-sys/vendor/<target>/VERSION`. The runtime preparation scripts
and workflows default to `google-ai-edge/LiteRT-LM` `v0.17.1` with the CPU-only
C API target. GPU, Metal, NPU, vision, and audio settings are exposed in Rust,
but the bundled runtimes are intentionally CPU-first. Use `system` mode for a
custom native build.

### SDK 0.2 migration

SDK 0.2.1 targets LiteRT-LM v0.17.1 and includes its fix for preserving integer
types in tool-call arguments. The safe Rust API is unchanged, but the raw
`litert-lm-edge-sys` ABI is incompatible with v0.13.1: inputs and sampler
parameters are opaque native objects, generation takes an array of input
pointers, and streaming callbacks receive `LiteRtLmStreamChunk` objects.
Do not mix this SDK with older native runtimes, including in `system` mode.
Rebuild the UE FFI bridge and replace its native runtime copies together.

The CPU runtime implements `SamplerType::TopP`; use `top_k: 1` with `TopP`
for deterministic sampling. Explicit `TopK` and `Greedy` currently return an
upstream unsupported-sampler error on CPU.

## Build Modes

Default bundled runtime:

```bash
cargo check --workspace
cargo run -p litert-lm-edge --example simple_generate -- /path/to/model.litertlm "你好"
```

### Downloading The Bundled Runtime

The runtimes are ~200 MB per crate, so they are excluded from the published
`litert-lm-edge-sys` package and shipped as GitHub Release assets instead. When
`litert-lm-edge-sys/vendor/<target>/` is absent, the build script downloads the
archive that the crate version points at:

```bash
cargo build -p litert-lm-edge

# mirror or a local fixture directory
export LITERT_LM_RUNTIME_BASE_URL=https://example.com/litert-lm-runtime

# refuse to download (air-gapped or vendored builds)
export LITERT_LM_RUNTIME_OFFLINE=1
```

A local `vendor/<target>/` directory always takes precedence, so the
`scripts/prepare_litert_lm_*` development flow stays network-free. Downloads go
to the shared `target/litert-lm-runtime` cache; relocate it with
`LITERT_LM_RUNTIME_CACHE_DIR`.

Every archive must match the SHA-256 pin in
`litert-lm-edge-sys/runtime-checksums.txt` and the `SHA256SUMS` manifest inside
the archive. A mismatch aborts the build.

Rebuild and republish the assets after changing a runtime:

```bash
python3 scripts/build_runtime_assets.py
gh release upload v0.2.1 dist/runtime/*.tar.gz --clobber
```

Then update the platform hashes in `litert-lm-edge-sys/runtime-checksums.txt`
with the values the script prints.

### Shipping A Binary

`cargo build` and `cargo run` work out of the box because cargo adds the target
directory to the loader search path. A binary you hand to someone else does
not, so copy the runtimes next to the executable:

| Platform | Shipped alongside the executable | Extra link flag |
| --- | --- | --- |
| macOS | `liblitert_lm_c_api.dylib`, `libGemmaModelConstraintProvider.dylib` | none |
| Linux | `liblitert_lm_c_api.so` plus the `libLiteRt*.so` files | none |
| Windows | `litert_lm_c_api.dll` plus the other `*.dll` files | none |

The `litert-lm-edge-sys` build script copies every runtime into the profile
directory (`target/<profile>/`), which is where the executable ends up. The
loaders then find them without help from your build: the macOS dylib carries an
`@loader_path/liblitert_lm_c_api.dylib` install name, and the Linux library
carries a `$ORIGIN/liblitert_lm_c_api.so` soname. Neither depends on an rpath,
which matters because a dependency cannot inject one into your binary.

A consumer that builds its own runtime set can still add
`-Wl,-rpath,$ORIGIN` on Linux as belt-and-braces.

#### Known issue: Linux exit-time segfault

On Linux the process segfaults while tearing down static state after `main`
returns, so it exits with status 139 and may dump core. It reproduces with the
untouched `v0.2.2` runtime under `LD_LIBRARY_PATH`, so it comes from the
upstream LiteRT-LM `v0.17.1` Linux build rather than from this crate, and it
does not happen on macOS. The `.github/workflows/consumer-smoke.yml` job
records it as a warning instead of a failure. A rebuild of the Linux runtime
may clear it; see the runtime upgrade checklist.

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

The script requires Git LFS and Bazel/Bazelisk. It downloads LiteRT-LM `v0.17.1`
into `.litert-lm-build/LiteRT-LM-v0.17.1/`, builds a shared CPU C API library,
fetches platform-specific LFS dependencies, and copies the libraries into
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

### Runtime Upgrade Checklist

When bumping the bundled LiteRT-LM runtime, update the tag in all preparation
scripts and runtime workflows, then rebuild and verify every checked-in
`vendor/<target>/VERSION`. Do not treat a Linux success as proof that Windows is
fixed: GitHub's Windows runner may fetch Bazel `http_archive` dependencies from
different mirrors or caches.

Upstream v0.17.1 includes mirror URLs for the `minizip` archive;
the v0.13.1 local URL patch is no longer needed. Source caches are versioned
so upgrading does not overwrite a previously patched checkout.

For each upgrade:

1. Compare upstream `c/engine.h` and `c/conversation.h` against the previous
   tag. Regenerate raw FFI bindings and verify that every declared symbol is
   exported by all bundled runtimes. macOS exports are derived from these headers.
2. Run the Windows and Linux runtime workflows from GitHub before copying their
   artifacts into `vendor/`.
3. After copying Windows artifacts on macOS or Linux, normalize text files if
   needed and run `shasum -a 256 -c SHA256SUMS`; PowerShell-generated files may
   contain CRLF line endings.
4. Confirm the expected C API exports exist in the rebuilt libraries, especially
   for newly added symbols.
5. Finish with `cargo fmt --all --check`, `cargo check --workspace
   --all-targets`, `git diff --check`, and checksum verification for every
   `vendor/<target>/SHA256SUMS`.
6. Exercise generation and streaming with both default and `generate-bindings`
   builds, and rebuild the UE bridge before updating its ThirdParty libraries.
7. Refresh the release runtime assets with
   `python3 scripts/build_runtime_assets.py`, upload them with
   `gh release upload v<crate version> dist/runtime/*.tar.gz --clobber`, and
   update `litert-lm-edge-sys/runtime-checksums.txt`. Published crates fetch
   these assets, so a stale asset silently ships an old runtime.

## Usage

Add the safe wrapper crate to an application:

```toml
[dependencies]
litert-lm-edge = { git = "https://github.com/mdddj/litert-lm-edge-rs" }
```

Tool calling and raw JSON examples also use `serde_json`:

```toml
[dependencies]
litert-lm-edge = { git = "https://github.com/mdddj/litert-lm-edge-rs" }
serde_json = "1"
```

The model downloader is optional:

```toml
[dependencies]
litert-lm-edge = {
    git = "https://github.com/mdddj/litert-lm-edge-rs",
    features = ["model-download"],
}
```

For local development against this workspace:

```toml
[dependencies]
litert-lm-edge = { path = "/path/to/litert-lm-edge-rs/litert-lm-edge" }
```

The default features use the bundled native runtime on Apple Silicon macOS,
Linux x86_64 GNU, and Windows x86_64 MSVC. A `.litertlm` model file is still
required at runtime:

```bash
MODEL=/path/to/model.litertlm
```

### Downloading Known Models

Enable `model-download` to download the two supported Gemma 4 LiteRT-LM models:

| Model | File | Size | SHA256 |
| --- | --- | ---: | --- |
| Gemma 4 E2B | `gemma-4-E2B-it.litertlm` | 2.4 GiB | `181938105e0eefd105961417e8da75903eacda102c4fce9ce90f50b97139a63c` |
| Gemma 4 E4B | `gemma-4-E4B-it.litertlm` | 3.4 GiB | `0b2a8980ce155fd97673d8e820b4d29d9c7d99b8fa6806f425d969b145bd52e0` |

```rust
use litert_lm_edge::{KnownModel, ModelDownloader};

fn main() -> litert_lm_edge::Result<()> {
    let path = ModelDownloader::new().download_to_dir(
        KnownModel::Gemma4E2B,
        "models",
    )?;

    println!("{}", path.display());
    Ok(())
}
```

Progress callbacks are available for CLI tools:

```rust
use litert_lm_edge::{DownloadProgress, KnownModel, ModelDownloader};

fn main() -> litert_lm_edge::Result<()> {
    let path = ModelDownloader::new().download_to_dir_with_progress(
        KnownModel::Gemma4E4B,
        "models",
        |progress: DownloadProgress| {
            if let Some(total) = progress.total_bytes {
                eprintln!("downloaded {} / {} bytes", progress.downloaded_bytes, total);
            }
        },
    )?;

    println!("{}", path.display());
    Ok(())
}
```

The downloader verifies SHA256 before moving the `.partial` file into place. To
use a Hugging Face mirror:

```rust
use litert_lm_edge::{KnownModel, ModelDownloader};

fn main() -> litert_lm_edge::Result<()> {
    let downloader = ModelDownloader::new().base_url("https://hf-mirror.com");
    let path = downloader.download_to_dir(KnownModel::Gemma4E2B, "models")?;
    println!("{}", path.display());
    Ok(())
}
```

### Text Generation

```rust
use litert_lm_edge::{Backend, Engine, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let text = session.generate_text("用一句话介绍杭州。")?;
    println!("{text}");

    Ok(())
}
```

### Streaming Text

```rust
use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};
use std::io::{self, Write};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let stream = session.generate_text_stream("写三句话介绍杭州。")?;

    for event in stream {
        match event {
            StreamEvent::Chunk(chunk) => {
                print!("{chunk}");
                io::stdout().flush().ok();
            }
            StreamEvent::Final => break,
            StreamEvent::Error(message) => eprintln!("stream error: {message}"),
        }
    }

    Ok(())
}
```

### Tokio Async

Enable the optional `tokio` feature to use async wrappers:

```toml
[dependencies]
litert-lm-edge = {
    git = "https://github.com/mdddj/litert-lm-edge-rs",
    features = ["tokio"],
}
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`TokioEngine` runs LiteRT-LM on a dedicated worker thread. Tokio tasks await
requests over channels, so blocking native calls do not run on Tokio worker
threads and the raw FFI handles are not marked `Send` or `Sync`.

```rust
use litert_lm_edge::{Backend, Engine, SessionConfig};

#[tokio::main]
async fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build_tokio()
        .await?;

    let session = engine.create_session(SessionConfig::default()).await?;
    let text = session.generate_text("用一句话介绍杭州。").await?;
    println!("{text}");

    Ok(())
}
```

Async streaming uses `TokioTextStream::next().await`:

```rust
use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};

#[tokio::main]
async fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build_tokio()
        .await?;

    let session = engine.create_session(SessionConfig::default()).await?;
    let mut stream = session.generate_text_stream("写三句话介绍杭州。").await?;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Chunk(chunk) => print!("{chunk}"),
            StreamEvent::Final => break,
            StreamEvent::Error(message) => eprintln!("stream error: {message}"),
        }
    }

    Ok(())
}
```

### Session Options

```rust
use litert_lm_edge::{
    Backend, Engine, SamplerParams, SamplerType, SessionConfig,
};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;

    let mut session = engine.create_session(SessionConfig {
        max_output_tokens: Some(256),
        apply_prompt_template: Some(true),
        sampler: Some(SamplerParams {
            sampler_type: SamplerType::TopP,
            top_k: 40,
            top_p: 0.95,
            temperature: 0.8,
            seed: 1234,
        }),
    })?;

    let text = session.generate_text("Give me one short paragraph about LiteRT-LM.")?;
    println!("{text}");

    Ok(())
}
```

### Engine Options

```rust
use litert_lm_edge::{Backend, Engine, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .vision_backend(Backend::Cpu)
        .audio_backend(Backend::Cpu)
        .max_num_tokens(4096)
        .max_num_images(2)
        .prefill_chunk_size(256)
        .parallel_file_section_loading(true)
        .multi_token_prediction(true)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let text = session.generate_text("Say hello.")?;
    println!("{text}");

    Ok(())
}
```

The bundled runtimes are CPU-first. `Backend::Gpu` and custom backends are
available for native runtimes built with the matching accelerator support:

```rust
use litert_lm_edge::{Backend, Engine};

fn main() -> litert_lm_edge::Result<()> {
    let _engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Gpu)
        .vision_backend(Backend::Gpu)
        .audio_backend(Backend::Cpu)
        .dispatch_lib_dir("/path/to/litert/dispatch/libs")
        .build()?;

    Ok(())
}
```

### Multimodal Session API

Use `generate_content` when a prompt contains text, images, or audio. File
inputs are read into owned bytes before the native call.

```rust
use litert_lm_edge::{Backend, Engine, InputData, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .vision_backend(Backend::Cpu)
        .audio_backend(Backend::Cpu)
        .max_num_images(1)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let response = session.generate_content(&[
        InputData::text("Describe this image in one paragraph."),
        InputData::image_file("/path/to/image.png"),
        InputData::ImageEnd,
    ])?;
    println!("{response}");

    Ok(())
}
```

Image and audio bytes can also be passed directly:

```rust
use litert_lm_edge::{Backend, Engine, InputData, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let image = std::fs::read("/path/to/image.png")?;
    let audio = std::fs::read("/path/to/audio.wav")?;

    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .vision_backend(Backend::Cpu)
        .audio_backend(Backend::Cpu)
        .max_num_images(1)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let response = session.generate_content(&[
        InputData::text("Describe the image and audio."),
        InputData::image_bytes(image),
        InputData::ImageEnd,
        InputData::audio_bytes(audio),
        InputData::AudioEnd,
    ])?;
    println!("{response}");

    Ok(())
}
```

The same multimodal input format supports streaming:

```rust
use litert_lm_edge::{Backend, Engine, InputData, SessionConfig, StreamEvent};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .vision_backend(Backend::Cpu)
        .max_num_images(1)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let stream = session.generate_content_stream(&[
        InputData::text("Describe this image."),
        InputData::image_file("/path/to/image.png"),
        InputData::ImageEnd,
    ])?;

    for event in stream {
        match event {
            StreamEvent::Chunk(chunk) => print!("{chunk}"),
            StreamEvent::Final => break,
            StreamEvent::Error(message) => eprintln!("stream error: {message}"),
        }
    }

    Ok(())
}
```

### Conversation API

Use `Conversation` for chat-style state, system messages, initial history, tool
calling, and the raw JSON escape hatch.

```rust
use litert_lm_edge::{Backend, ConversationConfig, Engine, Message};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;

    let mut conversation = engine.create_conversation(
        ConversationConfig::new()
            .system_message(Message::system("Answer concisely in Chinese."))
            .initial_messages(vec![
                Message::new("你好，我在测试 LiteRT-LM。"),
            ]),
    )?;

    let response = conversation.send_message(Message::new("请介绍一下你自己。"))?;
    println!("{}", response.to_string_content());

    Ok(())
}
```

### Conversation With Multimodal Content

Conversation image and audio file content keeps paths in JSON and lets the
native runtime load them.

```rust
use litert_lm_edge::{
    Backend, Content, ConversationConfig, Engine, Message, SendOptions,
};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .vision_backend(Backend::Cpu)
        .audio_backend(Backend::Cpu)
        .max_num_images(1)
        .build()?;

    let mut conversation = engine.create_conversation(ConversationConfig::new())?;
    let response = conversation.send_message_with_options(
        Message::user(vec![
            Content::text("What is in this image? Mention the audio if relevant."),
            Content::image_file("/path/to/image.png"),
            Content::audio_file("/path/to/audio.wav"),
        ]),
        SendOptions {
            visual_token_budget: Some(1024),
            ..SendOptions::default()
        },
    )?;

    println!("{}", response.to_string_content());
    Ok(())
}
```

Bytes are serialized as base64 blobs in the conversation JSON:

```rust
use litert_lm_edge::{Content, Message};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let image = std::fs::read("/path/to/image.png")?;
    let message = Message::user(vec![
        Content::text("Describe this image."),
        Content::image_bytes(image),
    ]);

    println!("{}", serde_json::to_string_pretty(&message.to_json())?);
    Ok(())
}
```

### Tool Calling

`JsonTool` is the simplest way to register a function. Automatic tool calling
is enabled by default and stops after 25 recurring tool calls.

```rust
use litert_lm_edge::{
    Backend, ConversationConfig, Engine, JsonTool, Message, ToolDefinition,
};
use serde_json::{json, Value};

fn main() -> litert_lm_edge::Result<()> {
    let product_tool = JsonTool::new(
        ToolDefinition::new("product")
            .description("Get the product of a list of numbers.")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "numbers": {
                        "type": "array",
                        "items": {"type": "number"}
                    }
                },
                "required": ["numbers"]
            })),
        |args: Value| {
            let product = args["numbers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .fold(1.0, |acc, number| acc * number);
            Ok(json!({ "product": product }))
        },
    );

    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;
    let mut conversation = engine.create_conversation(
        ConversationConfig::new()
            .system_message(Message::system("Use tools when they are useful."))
            .tool(product_tool),
    )?;

    let response = conversation.send_message(Message::new(
        "What is the product of 12.34 and 98.76?",
    ))?;
    println!("{}", response.to_string_content());

    Ok(())
}
```

Implement `Tool` directly when a closure is not enough:

```rust
use litert_lm_edge::{Result, Tool, ToolDefinition};
use serde_json::{json, Value};

struct WeatherTool;

impl Tool for WeatherTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new("weather")
            .description("Return current weather for a city.")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "city": {"type": "string"}
                },
                "required": ["city"]
            }))
    }

    fn call(&self, args: Value) -> Result<Value> {
        let city = args["city"].as_str().unwrap_or("unknown");
        Ok(json!({
            "city": city,
            "condition": "sunny",
            "temperature_c": 24
        }))
    }
}
```

Disable automatic tool execution or change the recursion limit with
`SendOptions`:

```rust
use litert_lm_edge::{
    Backend, ConversationConfig, Engine, Message, SendOptions,
};

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;
    let mut conversation = engine.create_conversation(ConversationConfig::new())?;

    let response = conversation.send_message_with_options(
        Message::new("Return a tool call but do not execute it."),
        SendOptions {
            automatic_tool_calling: false,
            recurring_tool_call_limit: 4,
            ..SendOptions::default()
        },
    )?;

    println!("{}", response.to_string_content());
    Ok(())
}
```

### Raw JSON Conversation

Use `send_message_raw` when the native conversation JSON shape moves faster
than the typed Rust wrapper.

```rust
use litert_lm_edge::{Backend, ConversationConfig, Engine, SendOptions};
use serde_json::json;

fn main() -> litert_lm_edge::Result<()> {
    let engine = Engine::builder("/path/to/model.litertlm")
        .backend(Backend::Cpu)
        .build()?;

    let mut conversation = engine.create_conversation(ConversationConfig::new())?;
    let response = conversation.send_message_raw(
        json!({
            "role": "user",
            "content": [{"type": "text", "text": "Return a short greeting."}]
        }),
        SendOptions {
            automatic_tool_calling: false,
            ..SendOptions::default()
        },
    )?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
```

### Canceling Work

`Session` and `Conversation` both expose cancellation. This is mainly useful
from another owner of the mutable value in application code.

```rust
session.cancel_process();
conversation.cancel_process();
```

### Included Examples

This repository also includes runnable examples:

```bash
cargo run -p litert-lm-edge --example simple_generate -- \
  "$MODEL" "hello"

cargo run -p litert-lm-edge --example stream_generate -- \
  "$MODEL" "hello"

cargo run -p litert-lm-edge --features tokio --example tokio_generate -- \
  "$MODEL" "hello"

cargo run -p litert-lm-edge --features tokio --example tokio_stream_generate -- \
  "$MODEL" "hello"

cargo run -p litert-lm-edge --features model-download --example download_model -- \
  e2b models

cargo run -p litert-lm-edge --features model-download --example download_model -- \
  e4b models

cargo run -p litert-lm-edge --example multimodal_generate -- \
  "$MODEL" /path/to/image.png /path/to/audio.wav

cargo run -p litert-lm-edge --example conversation_tools -- \
  "$MODEL"

cargo run -p litert-lm-edge --example conversation_multimodal -- \
  "$MODEL" /path/to/image.png /path/to/audio.wav

cargo run -p litert-lm-edge --example raw_conversation_json -- \
  "$MODEL"

cargo run -p litert-lm-edge --example engine_config -- \
  "$MODEL"
```
