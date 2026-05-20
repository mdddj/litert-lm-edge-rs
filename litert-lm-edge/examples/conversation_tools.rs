use litert_lm_edge::{Backend, ConversationConfig, Engine, JsonTool, Message, ToolDefinition};
use serde_json::{json, Value};

fn main() -> litert_lm_edge::Result<()> {
    let mut args = std::env::args().skip(1);
    let model_path = args
        .next()
        .expect("usage: conversation_tools <model.litertlm>");

    let product_tool = JsonTool::new(
        ToolDefinition::new("product")
            .description("Get the product of a list of numbers.")
            .parameters(json!({
                "type": "object",
                "properties": {
                    "numbers": {
                        "type": "array",
                        "items": {"type": "number"},
                        "description": "The numbers to multiply."
                    }
                },
                "required": ["numbers"]
            })),
        |args: Value| {
            let numbers = args
                .get("numbers")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let product = numbers
                .iter()
                .filter_map(Value::as_f64)
                .fold(1.0, |acc, number| acc * number);
            Ok(json!({ "product": product }))
        },
    );

    let engine = Engine::builder(model_path).backend(Backend::Cpu).build()?;
    let mut conversation = engine.create_conversation(
        ConversationConfig::new()
            .system_message(Message::system("You can use function calling."))
            .tool(product_tool),
    )?;

    let response = conversation.send_message(Message::new(
        "What is the product of 12.34 and 98.76? Use a tool if useful.",
    ))?;
    println!("{}", response.to_string_content());
    Ok(())
}
