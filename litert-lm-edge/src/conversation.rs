use crate::message::{Content, Message, Role, ToolCall};
use crate::tool::{Tool, ToolRegistry};
use crate::{ffi, Engine, Error, Result, SessionConfig};
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;

const DEFAULT_TOOL_CALL_LIMIT: usize = 25;

#[derive(Clone, Default)]
pub struct ConversationConfig {
    pub system_message: Option<Message>,
    pub initial_messages: Vec<Message>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub tools_json: Option<Value>,
    pub session_config: SessionConfig,
    pub enable_constrained_decoding: Option<bool>,
    pub filter_channel_content_from_kv_cache: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SendOptions {
    pub extra_context: Option<Value>,
    pub visual_token_budget: Option<i32>,
    pub automatic_tool_calling: bool,
    pub recurring_tool_call_limit: usize,
}

pub struct Conversation<'engine> {
    raw: NonNull<ffi::LiteRtLmConversation>,
    tools: ToolRegistry,
    owned_raw: bool,
    _engine: PhantomData<&'engine Engine>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            extra_context: None,
            visual_token_budget: None,
            automatic_tool_calling: true,
            recurring_tool_call_limit: DEFAULT_TOOL_CALL_LIMIT,
        }
    }
}

impl ConversationConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn system_message(mut self, message: Message) -> Self {
        self.system_message = Some(if message.role == Role::System {
            message
        } else {
            Message::with_role(Role::System, message.contents)
        });
        self
    }

    pub fn initial_messages(mut self, messages: Vec<Message>) -> Self {
        self.initial_messages = messages;
        self
    }

    pub fn tool<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tools.push(Arc::new(tool));
        self
    }

    pub fn tools(mut self, tools: Vec<Arc<dyn Tool>>) -> Self {
        self.tools = tools;
        self
    }

    pub fn tools_json(mut self, tools_json: Value) -> Self {
        self.tools_json = Some(tools_json);
        self
    }

    pub fn session_config(mut self, session_config: SessionConfig) -> Self {
        self.session_config = session_config;
        self
    }

    pub fn enable_constrained_decoding(mut self, enabled: bool) -> Self {
        self.enable_constrained_decoding = Some(enabled);
        self
    }

    pub fn filter_channel_content_from_kv_cache(mut self, enabled: bool) -> Self {
        self.filter_channel_content_from_kv_cache = Some(enabled);
        self
    }
}

impl<'engine> Conversation<'engine> {
    pub(crate) fn new(engine: &'engine Engine, config: ConversationConfig) -> Result<Self> {
        let ConversationConfig {
            system_message,
            initial_messages,
            tools,
            tools_json,
            session_config,
            enable_constrained_decoding,
            filter_channel_content_from_kv_cache,
        } = config;

        let registry = ToolRegistry::from_tools(tools);
        let session_config = crate::RawSessionConfig::new(session_config)?;
        let raw_config = RawConversationConfig::new()?;

        if let Some(raw_session_config) = session_config.as_ptr() {
            // SAFETY: raw_config and raw_session_config are valid for the duration of this call.
            unsafe {
                ffi::litert_lm_conversation_config_set_session_config(
                    raw_config.raw.as_ptr(),
                    raw_session_config.as_ptr(),
                );
            }
        }

        let system_message_json = system_message
            .as_ref()
            .map(|message| serde_json::to_string(&content_array(message)))
            .transpose()?
            .map(CString::new)
            .transpose()?;
        if let Some(system_message_json) = system_message_json.as_ref() {
            // SAFETY: raw_config is valid and system_message_json lives for this call.
            unsafe {
                ffi::litert_lm_conversation_config_set_system_message(
                    raw_config.raw.as_ptr(),
                    system_message_json.as_ptr(),
                );
            }
        }

        let tools_json_text = match tools_json {
            Some(value) => Some(serde_json::to_string(&value)?),
            None if !registry.is_empty() => Some(serde_json::to_string(&registry.schemas_json())?),
            None => None,
        };
        let tools_json = tools_json_text.map(CString::new).transpose()?;
        if let Some(tools_json) = tools_json.as_ref() {
            // SAFETY: raw_config is valid and tools_json lives for this call.
            unsafe {
                ffi::litert_lm_conversation_config_set_tools(
                    raw_config.raw.as_ptr(),
                    tools_json.as_ptr(),
                );
            }
        }

        let initial_messages_json = if initial_messages.is_empty() {
            None
        } else {
            Some(CString::new(serde_json::to_string(
                &initial_messages
                    .iter()
                    .map(Message::to_json)
                    .collect::<Vec<_>>(),
            )?)?)
        };
        if let Some(initial_messages_json) = initial_messages_json.as_ref() {
            // SAFETY: raw_config is valid and initial_messages_json lives for this call.
            unsafe {
                ffi::litert_lm_conversation_config_set_messages(
                    raw_config.raw.as_ptr(),
                    initial_messages_json.as_ptr(),
                );
            }
        }

        if let Some(enabled) = enable_constrained_decoding {
            // SAFETY: raw_config is valid.
            unsafe {
                ffi::litert_lm_conversation_config_set_enable_constrained_decoding(
                    raw_config.raw.as_ptr(),
                    enabled,
                );
            }
        }
        if let Some(enabled) = filter_channel_content_from_kv_cache {
            // SAFETY: raw_config is valid.
            unsafe {
                ffi::litert_lm_conversation_config_set_filter_channel_content_from_kv_cache(
                    raw_config.raw.as_ptr(),
                    enabled,
                );
            }
        }

        // SAFETY: engine.raw and raw_config are valid for this call.
        let raw = unsafe {
            ffi::litert_lm_conversation_create(engine.raw().as_ptr(), raw_config.raw.as_ptr())
        };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_conversation_create"))?;
        Ok(Self {
            raw,
            tools: registry,
            owned_raw: true,
            _engine: PhantomData,
        })
    }

    pub fn send_message(&mut self, message: Message) -> Result<Message> {
        self.send_message_with_options(message, SendOptions::default())
    }

    pub fn send_message_with_options(
        &mut self,
        message: Message,
        options: SendOptions,
    ) -> Result<Message> {
        let response = self.send_message_raw(message.to_json(), options)?;
        Ok(Message::from_json(response))
    }

    pub fn send_message_raw(&mut self, message: Value, options: SendOptions) -> Result<Value> {
        let mut current = message;
        for _ in 0..options.recurring_tool_call_limit {
            let response = self.send_once_raw(&current, &options)?;
            let tool_calls = parse_tool_calls(&response);
            if !options.automatic_tool_calling || tool_calls.is_empty() {
                return Ok(response);
            }
            current = self.tool_response_message(tool_calls)?;
        }

        Err(Error::RecurringToolCallLimit(
            options.recurring_tool_call_limit,
        ))
    }

    pub fn cancel_process(&mut self) {
        // SAFETY: self.raw is a valid conversation pointer owned by this Conversation.
        unsafe { ffi::litert_lm_conversation_cancel_process(self.raw.as_ptr()) };
    }

    fn send_once_raw(&mut self, message: &Value, options: &SendOptions) -> Result<Value> {
        let message = CString::new(serde_json::to_string(message)?)?;
        let extra_context = options
            .extra_context
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?
            .map(CString::new)
            .transpose()?;
        let optional_args = RawConversationOptionalArgs::new(options.visual_token_budget)?;

        // SAFETY: self.raw is valid; message/extra_context/optional_args pointers are valid for
        // the duration of this blocking call. The returned response is owned and freed below.
        let raw = unsafe {
            ffi::litert_lm_conversation_send_message(
                self.raw.as_ptr(),
                message.as_ptr(),
                extra_context
                    .as_ref()
                    .map_or(std::ptr::null(), |value| value.as_ptr()),
                optional_args.as_ptr(),
            )
        };
        let raw =
            NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_conversation_send_message"))?;
        let response = RawJsonResponse { raw };
        response.to_value()
    }

    fn tool_response_message(&self, tool_calls: Vec<ToolCall>) -> Result<Value> {
        let mut responses = Vec::new();
        for tool_call in tool_calls {
            let name = tool_call.function.name;
            let result = self.tools.call(&name, tool_call.function.arguments)?;
            responses.push(Content::tool_response(name, result).to_json());
        }
        Ok(json!({
            "role": "tool",
            "content": responses,
        }))
    }
}

impl Drop for Conversation<'_> {
    fn drop(&mut self) {
        if self.owned_raw {
            // SAFETY: self.raw was returned by litert_lm_conversation_create and is owned here.
            unsafe { ffi::litert_lm_conversation_delete(self.raw.as_ptr()) };
        }
    }
}

struct RawConversationConfig {
    raw: NonNull<ffi::LiteRtLmConversationConfig>,
}

impl RawConversationConfig {
    fn new() -> Result<Self> {
        // SAFETY: creates a new conversation config and returns null on failure.
        let raw = unsafe { ffi::litert_lm_conversation_config_create() };
        let raw =
            NonNull::new(raw).ok_or(Error::NullPointer("litert_lm_conversation_config_create"))?;
        Ok(Self { raw })
    }
}

impl Drop for RawConversationConfig {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_conversation_config_create and is owned here.
        unsafe { ffi::litert_lm_conversation_config_delete(self.raw.as_ptr()) };
    }
}

struct RawConversationOptionalArgs {
    raw: NonNull<ffi::LiteRtLmConversationOptionalArgs>,
}

impl RawConversationOptionalArgs {
    fn new(visual_token_budget: Option<i32>) -> Result<Self> {
        // SAFETY: creates optional args and returns null on failure.
        let raw = unsafe { ffi::litert_lm_conversation_optional_args_create() };
        let raw = NonNull::new(raw).ok_or(Error::NullPointer(
            "litert_lm_conversation_optional_args_create",
        ))?;
        if let Some(visual_token_budget) = visual_token_budget {
            // SAFETY: raw is a valid optional args pointer owned by this wrapper.
            unsafe {
                ffi::litert_lm_conversation_optional_args_set_visual_token_budget(
                    raw.as_ptr(),
                    visual_token_budget,
                );
            }
        }
        Ok(Self { raw })
    }

    fn as_ptr(&self) -> *const ffi::LiteRtLmConversationOptionalArgs {
        self.raw.as_ptr()
    }
}

impl Drop for RawConversationOptionalArgs {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by litert_lm_conversation_optional_args_create and is
        // owned here.
        unsafe { ffi::litert_lm_conversation_optional_args_delete(self.raw.as_ptr()) };
    }
}

struct RawJsonResponse {
    raw: NonNull<ffi::LiteRtLmJsonResponse>,
}

impl RawJsonResponse {
    fn to_value(&self) -> Result<Value> {
        // SAFETY: self.raw is a valid JSON response pointer owned by this wrapper.
        let ptr = unsafe { ffi::litert_lm_json_response_get_string(self.raw.as_ptr()) };
        if ptr.is_null() {
            return Err(Error::NullPointer("litert_lm_json_response_get_string"));
        }
        // SAFETY: ptr is documented as a null-terminated string valid while response is alive.
        let text = unsafe { CStr::from_ptr(ptr) }.to_str()?;
        Ok(serde_json::from_str(text)?)
    }
}

impl Drop for RawJsonResponse {
    fn drop(&mut self) {
        // SAFETY: self.raw was returned by LiteRT-LM and is owned here.
        unsafe { ffi::litert_lm_json_response_delete(self.raw.as_ptr()) };
    }
}

fn content_array(message: &Message) -> Value {
    Value::Array(message.contents.iter().map(Content::to_json).collect())
}

fn parse_tool_calls(response: &Value) -> Vec<ToolCall> {
    response
        .get("tool_calls")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .unwrap_or_default()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JsonTool, ToolDefinition};
    use serde_json::json;

    #[test]
    fn serializes_tool_response_message() {
        let tool = JsonTool::new(ToolDefinition::new("sum"), |_| Ok(json!({"sum": 3})));
        let config = ConversationConfig::new().tool(tool);
        assert_eq!(config.tools.len(), 1);

        let registry = ToolRegistry::from_tools(config.tools);
        let conversation = Conversation {
            raw: NonNull::dangling(),
            tools: registry,
            owned_raw: false,
            _engine: PhantomData,
        };
        let message = conversation
            .tool_response_message(vec![ToolCall {
                id: None,
                function: crate::ToolCallFunction {
                    name: "sum".to_owned(),
                    arguments: json!({"numbers": [1, 2]}),
                    extra: Default::default(),
                },
                extra: Default::default(),
            }])
            .unwrap();

        assert_eq!(
            message,
            json!({
                "role": "tool",
                "content": [{"type": "tool_response", "name": "sum", "response": {"sum": 3}}]
            })
        );
    }

    #[test]
    fn parses_tool_calls_from_response() {
        let calls = parse_tool_calls(&json!({
            "tool_calls": [{"function": {"name": "sum", "arguments": {"x": 1}}}]
        }));
        assert_eq!(calls[0].function.name, "sum");
    }
}
