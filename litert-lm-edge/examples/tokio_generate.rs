use litert_lm_edge::{Backend, Engine, SessionConfig};

#[tokio::main]
async fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: tokio_generate <model.litertlm> <prompt>");
    let prompt = args
        .next()
        .expect("usage: tokio_generate <model.litertlm> <prompt>");

    let engine = Engine::builder(model_path)
        .backend(Backend::Cpu)
        .build_tokio()
        .await?;
    let session = engine.create_session(SessionConfig::default()).await?;
    let text = session.generate_text(prompt).await?;
    println!("{text}");
    Ok(())
}
