use litert_lm_edge::{Backend, ConversationConfig, Engine, SendOptions};
use serde_json::json;

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: raw_conversation_json <model.litertlm>");

    let engine = Engine::builder(model_path).backend(Backend::Cpu).build()?;
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
