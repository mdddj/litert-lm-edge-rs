# litert-lm-edge-rs

[English](README.md) | [简体中文](README.zh-CN.md)

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
