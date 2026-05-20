use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    fn call(&self, args: Value) -> Result<Value>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Option<Value>,
}

pub struct JsonTool {
    definition: ToolDefinition,
    handler: Arc<dyn Fn(Value) -> Result<Value> + Send + Sync>,
}

#[derive(Default)]
pub(crate) struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            parameters: None,
        }
    }

    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn parameters(mut self, parameters: Value) -> Self {
        self.parameters = Some(parameters);
        self
    }

    pub fn to_schema_json(&self) -> Value {
        let mut function = serde_json::Map::new();
        function.insert("name".to_owned(), json!(self.name));
        if let Some(description) = &self.description {
            function.insert("description".to_owned(), json!(description));
        }
        if let Some(parameters) = &self.parameters {
            function.insert("parameters".to_owned(), parameters.clone());
        }
        json!({
            "type": "function",
            "function": Value::Object(function),
        })
    }
}

impl JsonTool {
    pub fn new<F>(definition: ToolDefinition, handler: F) -> Self
    where
        F: Fn(Value) -> Result<Value> + Send + Sync + 'static,
    {
        Self {
            definition,
            handler: Arc::new(handler),
        }
    }
}

impl Tool for JsonTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    fn call(&self, args: Value) -> Result<Value> {
        (self.handler)(args)
    }
}

impl ToolRegistry {
    pub(crate) fn from_tools(tools: Vec<Arc<dyn Tool>>) -> Self {
        let tools = tools
            .into_iter()
            .map(|tool| (tool.definition().name, tool))
            .collect();
        Self { tools }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub(crate) fn schemas_json(&self) -> Value {
        Value::Array(
            self.tools
                .values()
                .map(|tool| tool.definition().to_schema_json())
                .collect(),
        )
    }

    pub(crate) fn call(&self, name: &str, args: Value) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::ToolNotFound(name.to_owned()))?;
        tool.call(args).map_err(|err| Error::ToolError {
            name: name.to_owned(),
            message: err.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_tool_schema_as_function() {
        let definition = ToolDefinition::new("product")
            .description("Multiply numbers")
            .parameters(json!({
                "type": "object",
                "properties": {"numbers": {"type": "array", "items": {"type": "number"}}},
                "required": ["numbers"]
            }));

        assert_eq!(
            definition.to_schema_json(),
            json!({
                "type": "function",
                "function": {
                    "name": "product",
                    "description": "Multiply numbers",
                    "parameters": {
                        "type": "object",
                        "properties": {"numbers": {"type": "array", "items": {"type": "number"}}},
                        "required": ["numbers"]
                    }
                }
            })
        );
    }

    #[test]
    fn json_tool_executes_closure() {
        let tool = JsonTool::new(ToolDefinition::new("echo"), Ok);
        assert_eq!(tool.call(json!({"x": 1})).unwrap(), json!({"x": 1}));
    }
}
