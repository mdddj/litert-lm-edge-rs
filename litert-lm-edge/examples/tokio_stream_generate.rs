use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: tokio_stream_generate <model.litertlm> <prompt>");
    let prompt = args
        .next()
        .expect("usage: tokio_stream_generate <model.litertlm> <prompt>");

    let engine = Engine::builder(model_path)
        .backend(Backend::Cpu)
        .build_tokio()
        .await?;
    let session = engine.create_session(SessionConfig::default()).await?;
    let mut stream = session.generate_text_stream(prompt).await?;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Chunk(chunk) => {
                print!("{chunk}");
                io::stdout().flush().ok();
            }
            StreamEvent::Final => {
                println!();
                break;
            }
            StreamEvent::Error(error) => {
                eprintln!("stream error: {error}");
                break;
            }
        }
    }

    Ok(())
}
