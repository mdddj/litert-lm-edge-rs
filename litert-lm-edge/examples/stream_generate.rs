use litert_lm_edge::{Backend, Engine, SessionConfig, StreamEvent};
use std::io::{self, Write};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: stream_generate <model.litertlm> <prompt>");
    let prompt = args
        .next()
        .expect("usage: stream_generate <model.litertlm> <prompt>");

    let engine = Engine::builder(model_path).backend(Backend::Cpu).build()?;
    let mut session = engine.create_session(SessionConfig::default())?;
    let stream = session.generate_text_stream(&prompt)?;

    for event in stream {
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
