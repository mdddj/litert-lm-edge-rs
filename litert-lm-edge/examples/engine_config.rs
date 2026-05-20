use litert_lm_edge::{Backend, Engine, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args.next().expect("usage: engine_config <model.litertlm>");

    let engine = Engine::builder(model_path)
        .backend(Backend::Cpu)
        .vision_backend(Backend::Gpu)
        .audio_backend(Backend::Cpu)
        .max_num_tokens(4096)
        .max_num_images(2)
        .prefill_chunk_size(256)
        .multi_token_prediction(true)
        .parallel_file_section_loading(true)
        .build()?;

    let mut session = engine.create_session(SessionConfig {
        max_output_tokens: Some(128),
        apply_prompt_template: Some(true),
        sampler: None,
    })?;
    let response = session.generate_text("Give me one sentence about LiteRT-LM.")?;
    println!("{response}");
    Ok(())
}
