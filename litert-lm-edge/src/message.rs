use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Model,
    Tool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    ImageBytes(Vec<u8>),
    ImageFile(PathBuf),
    AudioBytes(Vec<u8>),
    AudioFile(PathBuf),
    ToolResponse { name: String, response: Value },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: Role,
    pub contents: Vec<Content>,
    pub channels: BTreeMap<String, String>,
    pub tool_calls: Vec<ToolCall>,
    pub raw: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub id: Option<String>,
    pub function: ToolCallFunction,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Content {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    pub fn image_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::ImageBytes(bytes.into())
    }

    pub fn image_file(path: impl AsRef<Path>) -> Self {
        Self::ImageFile(path.as_ref().to_path_buf())
    }

    pub fn audio_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self::AudioBytes(bytes.into())
    }

    pub fn audio_file(path: impl AsRef<Path>) -> Self {
        Self::AudioFile(path.as_ref().to_path_buf())
    }

    pub fn tool_response(name: impl Into<String>, response: Value) -> Self {
        Self::ToolResponse {
            name: name.into(),
            response,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Text(text) => json!({"type": "text", "text": text}),
            Self::ImageBytes(bytes) => json!({
                "type": "image",
                "blob": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
            Self::ImageFile(path) => json!({
                "type": "image",
                "path": path.to_string_lossy(),
            }),
            Self::AudioBytes(bytes) => json!({
                "type": "audio",
                "blob": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
            Self::AudioFile(path) => json!({
                "type": "audio",
                "path": path.to_string_lossy(),
            }),
            Self::ToolResponse { name, response } => json!({
                "type": "tool_response",
                "name": name,
                "response": response,
            }),
            Self::Raw(value) => value.clone(),
        }
    }
}

impl Message {
    pub fn new(text: impl Into<String>) -> Self {
        Self::user(vec![Content::Text(text.into())])
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::with_role(Role::System, vec![Content::Text(text.into())])
    }

    pub fn user(contents: Vec<Content>) -> Self {
        Self::with_role(Role::User, contents)
    }

    pub fn model(contents: Vec<Content>) -> Self {
        Self::with_role(Role::Model, contents)
    }

    pub fn tool(contents: Vec<Content>) -> Self {
        Self::with_role(Role::Tool, contents)
    }

    pub fn with_role(role: Role, contents: Vec<Content>) -> Self {
        Self {
            role,
            contents,
            channels: BTreeMap::new(),
            tool_calls: Vec::new(),
            raw: None,
        }
    }

    pub fn with_channels(mut self, channels: BTreeMap<String, String>) -> Self {
        self.channels = channels;
        self
    }

    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("role".to_owned(), json!(self.role));
        if !self.contents.is_empty() {
            object.insert(
                "content".to_owned(),
                Value::Array(self.contents.iter().map(Content::to_json).collect()),
            );
        }
        if !self.channels.is_empty() {
            object.insert("channels".to_owned(), json!(self.channels));
        }
        if !self.tool_calls.is_empty() {
            object.insert("tool_calls".to_owned(), json!(self.tool_calls));
        }
        Value::Object(object)
    }

    pub fn to_string_content(&self) -> String {
        self.contents
            .iter()
            .filter_map(|content| match content {
                Content::Text(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn from_json(value: Value) -> Self {
        let role = value
            .get("role")
            .and_then(Value::as_str)
            .and_then(parse_role)
            .unwrap_or(Role::Model);
        let contents = value
            .get("content")
            .and_then(Value::as_array)
            .map(|items| items.iter().cloned().map(content_from_json).collect())
            .unwrap_or_default();
        let channels = value
            .get("channels")
            .and_then(Value::as_object)
            .map(|object| {
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let tool_calls = value
            .get("tool_calls")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .unwrap_or_default()
            .unwrap_or_default();

        Self {
            role,
            contents,
            channels,
            tool_calls,
            raw: Some(value),
        }
    }
}

fn parse_role(value: &str) -> Option<Role> {
    match value {
        "system" => Some(Role::System),
        "user" => Some(Role::User),
        "model" => Some(Role::Model),
        "tool" => Some(Role::Tool),
        _ => None,
    }
}

fn content_from_json(value: Value) -> Content {
    match value.get("type").and_then(Value::as_str) {
        Some("text") => value
            .get("text")
            .and_then(Value::as_str)
            .map(|text| Content::Text(text.to_owned()))
            .unwrap_or(Content::Raw(value)),
        Some("image") => value
            .get("path")
            .and_then(Value::as_str)
            .map(|path| Content::ImageFile(PathBuf::from(path)))
            .unwrap_or(Content::Raw(value)),
        Some("audio") => value
            .get("path")
            .and_then(Value::as_str)
            .map(|path| Content::AudioFile(PathBuf::from(path)))
            .unwrap_or(Content::Raw(value)),
        Some("tool_response") => {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let response = value.get("response").cloned().unwrap_or(Value::Null);
            Content::ToolResponse { name, response }
        }
        _ => Content::Raw(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_content_json_shapes() {
        assert_eq!(
            Content::text("hi").to_json(),
            json!({"type": "text", "text": "hi"})
        );
        assert_eq!(
            Content::image_bytes(vec![1, 2, 3]).to_json(),
            json!({"type": "image", "blob": "AQID"})
        );
        assert_eq!(
            Content::image_file("/tmp/a.png").to_json(),
            json!({"type": "image", "path": "/tmp/a.png"})
        );
        assert_eq!(
            Content::audio_bytes(vec![4, 5]).to_json(),
            json!({"type": "audio", "blob": "BAU="})
        );
        assert_eq!(
            Content::audio_file("/tmp/a.wav").to_json(),
            json!({"type": "audio", "path": "/tmp/a.wav"})
        );
        assert_eq!(
            Content::tool_response("sum", json!({"value": 3})).to_json(),
            json!({"type": "tool_response", "name": "sum", "response": {"value": 3}})
        );
    }

    #[test]
    fn serializes_message_json_shape() {
        let message = Message::user(vec![Content::text("hello")]);
        assert_eq!(
            message.to_json(),
            json!({"role": "user", "content": [{"type": "text", "text": "hello"}]})
        );
    }

    #[test]
    fn parses_tool_calls_and_preserves_raw() {
        let value = json!({
            "role": "model",
            "tool_calls": [{
                "function": {"name": "sum", "arguments": {"numbers": [1, 2]}}
            }]
        });
        let message = Message::from_json(value);
        assert_eq!(message.role, Role::Model);
        assert_eq!(message.tool_calls[0].function.name, "sum");
        assert!(message.raw.is_some());
    }
}
