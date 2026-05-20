use litert_lm_edge::{Backend, Content, ConversationConfig, Engine, Message, SendOptions};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: conversation_multimodal <model.litertlm> <image-path> [audio-path]");
    let image_path = args
        .next()
        .expect("usage: conversation_multimodal <model.litertlm> <image-path> [audio-path]");
    let audio_path = args.next();

    let engine = Engine::builder(model_path)
        .backend(Backend::Cpu)
        .vision_backend(Backend::Gpu)
        .audio_backend(Backend::Cpu)
        .build()?;
    let mut conversation = engine.create_conversation(ConversationConfig::new())?;

    let mut contents = vec![
        Content::text("Describe this image. Mention audio if an audio clip is included."),
        Content::image_file(image_path),
    ];
    if let Some(audio_path) = audio_path {
        contents.push(Content::audio_file(audio_path));
    }

    let response = conversation.send_message_with_options(
        Message::user(contents),
        SendOptions {
            visual_token_budget: Some(1024),
            ..SendOptions::default()
        },
    )?;
    println!("{}", response.to_string_content());
    Ok(())
}
