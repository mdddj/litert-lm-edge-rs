use litert_lm_edge::{Backend, Engine, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: simple_generate <model.litertlm> <prompt>");
    let prompt = args
        .next()
        .expect("usage: simple_generate <model.litertlm> <prompt>");

    let engine = Engine::builder(model_path).backend(Backend::Cpu).build()?;
    let mut session = engine.create_session(SessionConfig::default())?;
    let text = session.generate_text(&prompt)?;
    println!("{text}");
    Ok(())
}
