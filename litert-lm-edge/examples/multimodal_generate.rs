use litert_lm_edge::{Backend, Engine, InputData, SessionConfig};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: multimodal_generate <model.litertlm> <image-path> [audio-path]");
    let image_path = args
        .next()
        .expect("usage: multimodal_generate <model.litertlm> <image-path> [audio-path]");
    let audio_path = args.next();

    let engine = Engine::builder(model_path)
        .backend(Backend::Cpu)
        .vision_backend(Backend::Gpu)
        .audio_backend(Backend::Cpu)
        .max_num_images(1)
        .build()?;
    let mut session = engine.create_session(SessionConfig::default())?;

    let mut inputs = vec![
        InputData::text("Describe the image. If audio is present, include relevant audio details."),
        InputData::image_file(image_path),
        InputData::ImageEnd,
    ];

    if let Some(audio_path) = audio_path {
        inputs.push(InputData::audio_file(audio_path));
        inputs.push(InputData::AudioEnd);
    }

    let response = session.generate_content(&inputs)?;
    println!("{response}");
    Ok(())
}
