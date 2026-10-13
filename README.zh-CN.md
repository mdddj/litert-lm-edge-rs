# litert-lm-edge-rs

[English](README.md) | [简体中文](README.zh-CN.md)

LiteRT-LM C API 的 Rust 绑定。这个 workspace 绑定 `c/engine.h` 和
`c/conversation.h` 中的 C API，不直接绑定 C++ 类。

## Crates

- `litert-lm-edge-sys`：原始 unsafe FFI 绑定和 native 库链接。
- `litert-lm-edge`：安全封装，支持 engine/session 生成、流式输出、多模态输入、conversation 和工具调用。

## 默认 Native Runtime

默认构建会链接已经打包好的 native runtime，覆盖 Apple Silicon macOS、Linux x86_64 GNU 和 Windows x86_64 MSVC：

```text
litert-lm-edge-sys/vendor/darwin-arm64/liblitert_lm_c_api.dylib
litert-lm-edge-sys/vendor/linux-x86_64/liblitert_lm_c_api.so
litert-lm-edge-sys/vendor/windows-x86_64/litert_lm_c_api.dll
litert-lm-edge-sys/vendor/windows-x86_64/litert_lm_c_api.lib
```

这些目标平台上的使用者不需要设置 `LITERT_LM_LIB_DIR`、`LITERT_LM_LINK_LIB`，也不需要安装 Bazel 或下载 LiteRT-LM 源码。运行时只需要提供 `.litertlm` 模型文件。其他平台需要使用 `system` 模式。

每个已提交 runtime 的精确上游 tag 记录在 `litert-lm-edge-sys/vendor/<target>/VERSION`。runtime 准备脚本和 workflow 默认使用 `google-ai-edge/LiteRT-LM` `v0.18.0`，构建目标是 CPU-only C API。Rust API 暴露了 GPU、Metal、NPU、vision 和 audio 相关设置，但默认打包 runtime 以 CPU 优先。需要自定义 accelerator 时，请使用 `system` 模式链接自己的 native build。

macOS runtime 通过一份导出符号白名单链接，该白名单由 `scripts/prepare_litert_lm_darwin_arm64.sh` 从公开 C 头文件生成。凡是头文件没列入白名单的 C API 函数，都会在链接期被从 dylib 中剥离，只会在下游使用时以缺少符号的形式暴露出来；因此该清单在 `c/engine.h` 和 `c/conversation.h` 之外还列入了 `c/embedding_engine.h`。Linux 和 Windows 导出全部 C API，不需要这份清单。

### SDK 0.2 迁移说明

SDK 0.2.1 对应 LiteRT-LM v0.17.1，并包含工具调用参数保持整数类型的上游修复。Rust 安全封装 API 保持不变，但 `litert-lm-edge-sys` 的原始 ABI 与 v0.13.1 不兼容：输入和采样参数改为不透明原生对象，生成接口接收输入指针数组，流式回调接收 `LiteRtLmStreamChunk` 对象。包括 `system` 模式在内，不得混用旧版原生库。UE FFI 桥接库也必须重新构建，并与其原生运行库副本一起替换。

SDK 0.2.5 对应 LiteRT-LM v0.18.0，新增 `EngineBuilder::single_threaded_execution` 和 `SessionConfig.enable_speculative_decoding`。v0.17.1 的 C 调用签名没有变化。手写的 `SessionConfig` 结构体字面量需要补上新字段，或者使用 `..SessionConfig::default()`。

CPU runtime 实现的是 `SamplerType::TopP`；需要确定性采样时可使用 `TopP` 配合 `top_k: 1`。显式使用 `TopK` 或 `Greedy` 时，上游 CPU runtime 当前会返回不支持该 sampler 的错误。

## 构建模式

默认打包 runtime：

```bash
cargo check --workspace
cargo run -p litert-lm-edge --example simple_generate -- /path/to/model.litertlm "你好"
```

自定义 LiteRT-LM native build 或其他平台使用 `system` 模式：

```bash
export LITERT_LM_LIB_DIR=/path/to/native/lib
export LITERT_LM_LINK_LIB=litert_lm_c_api
export LITERT_LM_LINK_KIND=dylib
cargo check --workspace --no-default-features --features system
```

Windows 打包 runtime：

```powershell
cargo check --workspace
cargo run -p litert-lm-edge --example simple_generate -- C:\path\to\model.litertlm "hello"
```

从本地 LiteRT-LM checkout 重新生成 bindgen 绑定：

```bash
export LITERT_LM_ROOT=/path/to/LiteRT-LM
cargo check -p litert-lm-edge-sys --features generate-bindings
```

## 准备打包 Runtime

Apple Silicon macOS 上可以重新构建打包 runtime：

```bash
scripts/prepare_litert_lm_darwin_arm64.sh
```

脚本需要 Git LFS 和 Bazel/Bazelisk，会把 LiteRT-LM `v0.18.0` 下载到 `.litert-lm-build/LiteRT-LM-v0.18.0/`，获取目标平台的 LFS 依赖，构建共享 CPU C API 库，把产物复制到 `litert-lm-edge-sys/vendor/darwin-arm64/`，并写入 `VERSION` 和 `SHA256SUMS`。

Windows runtime 必须在 Windows x86_64 和 MSVC Build Tools 环境中构建：

```powershell
scripts\prepare_litert_lm_windows_x86_64.ps1
```

仓库也提供了手动触发的 GitHub Actions workflow：

```text
.github/workflows/build-windows-runtime.yml
```

在 GitHub 上运行它，下载 `litert-lm-edge-windows-x86_64-runtime` artifact，然后把内容复制到 `litert-lm-edge-sys/vendor/windows-x86_64/`。

Linux x86_64 GNU runtime 必须在 Linux x86_64 上构建：

```bash
scripts/prepare_litert_lm_linux_x86_64.sh
```

仓库也提供了手动触发的 GitHub Actions workflow：

```text
.github/workflows/build-linux-runtime.yml
```

在 GitHub 上运行它，下载 `litert-lm-edge-linux-x86_64-runtime` artifact，然后把内容复制到 `litert-lm-edge-sys/vendor/linux-x86_64/`。

### Runtime 升级检查清单

升级打包 LiteRT-LM runtime 时，先同时更新所有准备脚本和 runtime workflow 里的 tag，然后分别重建并确认每个已提交的 `vendor/<target>/VERSION`。不要把 Linux 成功当成 Windows 已经没问题：GitHub Windows runner 可能从不同镜像或 Bazel cache 获取 `http_archive` 依赖。

上游 v0.17.1 及之后的版本已为 `minizip` archive 配置镜像 URL，不再需要 v0.13.1 的本地 URL patch。源码缓存按版本隔离，升级不会覆盖之前已打过补丁的 checkout。

每次升级按这个顺序检查：

1. 对比新旧 tag 的 `c/engine.h` 和 `c/conversation.h`，重新生成 raw FFI，并确认所有打包 runtime 都导出了声明的符号。macOS export list 直接从这两个头文件生成。
2. 先在 GitHub 上分别跑 Windows 和 Linux runtime workflow，成功后再把 artifact 复制进 `vendor/`。
3. 在 macOS 或 Linux 上复制 Windows artifact 后，必要时规范化文本文件换行，再运行 `shasum -a 256 -c SHA256SUMS`；PowerShell 生成的文件可能带 CRLF。
4. 检查重建后的库是否真的导出了预期 C API，尤其是本次新增的符号。
5. 最后跑 `cargo fmt --all --check`、`cargo check --workspace --all-targets`、`git diff --check`，并校验每个 `vendor/<target>/SHA256SUMS`。
6. 分别使用默认绑定和 `generate-bindings` 构建验证实际生成、流式调用；更新 UE ThirdParty 库前必须重新构建 UE FFI 桥接库。

## 使用方式

在应用里添加 safe wrapper crate：

```toml
[dependencies]
litert-lm-edge = { git = "https://github.com/mdddj/litert-lm-edge-rs" }
```

工具调用和 raw JSON 示例还需要 `serde_json`：

```toml
[dependencies]
litert-lm-edge = { git = "https://github.com/mdddj/litert-lm-edge-rs" }
serde_json = "1"
```

模型下载工具是可选 feature：

```toml
[dependencies]
litert-lm-edge = {
    git = "https://github.com/mdddj/litert-lm-edge-rs",
    features = ["model-download"],
}
```

如果在本地直接依赖这个 workspace：

```toml
[dependencies]
litert-lm-edge = { path = "/path/to/litert-lm-edge-rs/litert-lm-edge" }
```

默认 features 会在 Apple Silicon macOS、Linux x86_64 GNU 和 Windows x86_64 MSVC 上使用打包 runtime。运行时仍然需要 `.litertlm` 模型文件：

```bash
MODEL=/path/to/model.litertlm
```

### 下载已知模型

启用 `model-download` 后，可以下载以下已打包支持的 LiteRT-LM 模型：

| 模型 | 文件 | 大小 | SHA256 |
| --- | --- | ---: | --- |
| Gemma 4 E2B | `gemma-4-E2B-it.litertlm` | 2.4 GiB | `181938105e0eefd105961417e8da75903eacda102c4fce9ce90f50b97139a63c` |
| Gemma 4 E4B | `gemma-4-E4B-it.litertlm` | 3.4 GiB | `0b2a8980ce155fd97673d8e820b4d29d9c7d99b8fa6806f425d969b145bd52e0` |
| EmbeddingGemma 2 740M | `embeddinggemma-2-740m.litertlm` | 462 MiB | `e7a8a2204b91e0f96e92960e84a09a89212e1633dcb7575a9bf3378b4df77f4c` |

前两个 Gemma 4 模型是给 `Engine` 用的对话模型。EmbeddingGemma 2 是给
`EmbeddingEngine` 用的嵌入模型，它只产出向量，没有文本生成路径。

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

CLI 工具可以使用进度回调：

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

downloader 会先写入 `.partial` 文件，并在 SHA256 校验通过后移动到最终路径。使用 Hugging Face 镜像时可以设置 `base_url`：

```rust
use litert_lm_edge::{KnownModel, ModelDownloader};

fn main() -> litert_lm_edge::Result<()> {
    let downloader = ModelDownloader::new().base_url("https://hf-mirror.com");
    let path = downloader.download_to_dir(KnownModel::Gemma4E2B, "models")?;
    println!("{}", path.display());
    Ok(())
}
```

### 文本生成

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

### 流式文本

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

### Tokio 异步

启用可选的 `tokio` feature 后，可以使用 async wrapper：

```toml
[dependencies]
litert-lm-edge = {
    git = "https://github.com/mdddj/litert-lm-edge-rs",
    features = ["tokio"],
}
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

`TokioEngine` 会在专用 worker 线程中运行 LiteRT-LM。Tokio task 通过 channel await 请求结果，所以阻塞 native 调用不会跑在 Tokio worker 线程上，也不会给 raw FFI handle 强行标记 `Send` 或 `Sync`。

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

异步流式输出使用 `TokioTextStream::next().await`：

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

### Session 配置

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
        enable_speculative_decoding: None,
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

### Engine 配置

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
        .single_threaded_execution(false)
        .multi_token_prediction(true)
        .build()?;

    let mut session = engine.create_session(SessionConfig::default())?;
    let text = session.generate_text("Say hello.")?;
    println!("{text}");

    Ok(())
}
```

默认打包 runtime 是 CPU-first。`Backend::Gpu` 和自定义 backend 可以用于带有对应 accelerator 支持的 native runtime：

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

### 嵌入（Embeddings）

`EmbeddingEngine` 用来跑 EmbeddingGemma 2 这类 `.litertlm` 嵌入模型并返回向量。它是独立于 `Engine` 的另一套引擎：嵌入模型没有文本生成路径，不能用 `Engine::builder` 加载。

```rust
use litert_lm_edge::{EmbeddingEngine, EmbeddingOptions, EmbeddingTask, InputData};

fn main() -> litert_lm_edge::Result<()> {
    let mut engine = EmbeddingEngine::builder("/path/to/embeddinggemma-2-740m.litertlm")
        .num_threads(4)
        .build()?;

    // normalize(true) 后，点积即余弦相似度。
    let options = EmbeddingOptions::new().normalize(true);
    let vector = engine.compute_embedding_with_options(
        &[InputData::text(
            EmbeddingTask::Query.prompt("a cat resting on a window"),
        )],
        options,
    )?;
    println!("{} dimensions", vector.len());

    Ok(())
}
```

`compute_embedding_batch` 可以一次嵌入多个请求。输入沿用与生成 API 相同的多模态 `InputData` 格式，因此单个请求内可以混用文本、图片和音频内容。

EmbeddingGemma 2 使用非对称的任务前缀，`EmbeddingTask` 已内置官方前缀：

- `EmbeddingTask::Document` — `task: search result | text: `
- `EmbeddingTask::Query` — `task: search query | text: `
- `EmbeddingTask::Classification` — `task: classification | text: `
- `EmbeddingTask::Clustering` — `task: clustering | text: `
- `EmbeddingTask::SentenceSimilarity` — `task: sentence similarity | text: `

建索引用 `Document`，查询用 `Query`。两侧使用同一个前缀会明显降低检索质量。

#### 验证多模态嵌入

纯文本的输出无法证明视觉和音频编码器真的跑了，因此 `embedding_multimodal` 会把每种媒介各自嵌入一遍，再用文本 probe 对它排序。管线正常时，真正描述该媒介的 probe 会排在第一位；管线坏掉时顺序是随机的。

```bash
cargo run --release -p litert-lm-edge --example embedding_multimodal -- \
  "$EMBEDDING_MODEL" \
  --image .litert-lm-build/LiteRT-LM-v0.18.0/support/preprocessor/testdata/apple.png \
  --image .litert-lm-build/LiteRT-LM-v0.18.0/runtime/testdata/colored_rect_163_586_615_957.jpg \
  --audio .litert-lm-build/LiteRT-LM-v0.18.0/runtime/testdata/have_a_wonderful_day.wav
```

`--image` 和 `--audio` 都可以重复传入。`--probe TEXT` 用来替换默认 probe 集合，默认值是按上游自带夹具挑选的。

CPU 上跑 EmbeddingGemma 2 740M，正常结果如下：

```text
image: .../apple.png  (768 dimensions)
  0.8075  a photo of a red apple in a wooden bowl
  0.6297  an empty quiet room

image: .../colored_rect_163_586_615_957.jpg  (768 dimensions)
  0.7931  a green rectangle on a white background
  0.6318  an empty quiet room

audio: .../have_a_wonderful_day.wav  (768 dimensions)
  0.8392  a person speaking the words have a wonderful day
  0.6065  an empty quiet room

pairwise similarity between media:
  [0] <-> [1]: 0.6233
  [0] <-> [2]: 0.5993
  [1] <-> [2]: 0.5871
```

有三条特征用来区分"真的通了"和"看起来通了"：

- 正确 probe 以明显优势胜出，而不是零点几的差距。这里的预期量级是 0.15–0.25；分数整体被压缩，是因为各媒介处在同一个联合空间里，而不是各自独立的两个空间。
- 各媒介互斥。苹果图片得分最低的 probe 正是绿色矩形，而绿色矩形得分最低的 probe 正是苹果。
- 各媒介落在互不相同的向量上。三路编码器都活着，所以没有任何一对的相似度接近 1.0。媒介之间相似度普遍偏高且彼此接近，是编码器返回常量向量的典型特征。

音频不需要额外工具链。运行时会自行重采样到前端期望的 16 kHz，所以任意采样率的 `.wav` 都能用：同一段录音转成 8 kHz、16 kHz、24 kHz、22.05 kHz 后，彼此分数差距不超过 0.0024，向量相似度不低于 0.9974。上游提供的是单声道 16-bit PCM，但立体声会被降混、32-bit float 也支持。

图片同样不需要预缩放。预处理器会按模型包的 patch 预算自行适配，并打印出决策，所以全分辨率照片可以直接用：

```text
stb_image_preprocessor.cc:72] Resize image from 1131x1077 to 576x528 which
will result in 1188 patches to fit the max_num_patches: 1260 limit.
```

### 多模态 Session API

当 prompt 包含文本、图片或音频时，使用 `generate_content`。文件输入会先读取成 owned bytes，再传给 native 调用。

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

也可以直接传图片和音频 bytes：

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

同样的多模态输入格式也支持流式输出：

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

`Conversation` 适合 chat-style 状态、system message、初始历史、工具调用和 raw JSON escape hatch。

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

### Conversation 多模态内容

Conversation 里的图片和音频文件内容会在 JSON 中保留路径，由 native runtime 加载。

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

bytes 内容会在 conversation JSON 中序列化为 base64 blob：

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

### 工具调用

`JsonTool` 是注册函数最简单的方式。自动工具调用默认开启，最多递归调用 25 次。

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

closure 不够用时，可以直接实现 `Tool`：

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

使用 `SendOptions` 可以关闭自动工具执行，或调整递归上限：

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

当 native conversation JSON 结构变化快于 typed Rust wrapper 时，可以使用 `send_message_raw`。

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

### 取消任务

`Session` 和 `Conversation` 都暴露了取消方法。在应用代码中，当其他 owner 持有对应 mutable value 时，这个能力比较有用。

```rust
session.cancel_process();
conversation.cancel_process();
```

### 仓库内 Examples

仓库也包含可以直接运行的 examples：

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

cargo run -p litert-lm-edge --example embedding_generate -- \
  "$EMBEDDING_MODEL"

cargo run -p litert-lm-edge --example embedding_multimodal -- \
  "$EMBEDDING_MODEL" --image /path/to/image.png --audio /path/to/audio.wav
```
