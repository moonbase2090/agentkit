use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_owned(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_owned(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant_tool_call(id: String, name: &str, arguments: serde_json::Value) -> Self {
        Self {
            role: "assistant".to_owned(),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id,
                name: name.to_owned(),
                arguments,
            }]),
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(call_id: String, tool_name: &str, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_owned(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(call_id),
            name: Some(tool_name.to_owned()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub message: String,
    pub tool_calls: Vec<ToolCall>,
}

pub trait ChatProvider: Send {
    fn complete(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<ProviderResponse>;
}

pub fn openrouter_chat_completion_body(
    model: &str,
    messages: &[ChatMessage],
    tools: &[Value],
) -> Value {
    json!({
        "model": model,
        "messages": messages.iter().map(encode_openrouter_message).collect::<Vec<_>>(),
        "tools": tools,
    })
}

pub fn encode_openrouter_message(message: &ChatMessage) -> Value {
    match message.role.as_str() {
        "assistant" if message.tool_calls.is_some() => {
            let tool_calls = message
                .tool_calls
                .as_ref()
                .map(|calls| {
                    calls
                        .iter()
                        .map(|call| {
                            let arguments = serde_json::to_string(&call.arguments)
                                .unwrap_or_else(|_| "{}".to_owned());
                            json!({
                                "id": call.id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    "arguments": arguments,
                                }
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({
                "role": "assistant",
                "content": message.content,
                "tool_calls": tool_calls,
            })
        }
        "tool" => json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id,
            "name": message.name,
            "content": message.content,
        }),
        _ => json!({
            "role": message.role,
            "content": message.content,
        }),
    }
}

pub struct MockProvider {
    pub responses: Vec<ProviderResponse>,
    pub calls: std::sync::Mutex<usize>,
}

impl MockProvider {
    pub fn scripted(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses,
            calls: std::sync::Mutex::new(0),
        }
    }
}

impl ChatProvider for MockProvider {
    fn complete(
        &self,
        _model: &str,
        _messages: &[ChatMessage],
        _tools: &[serde_json::Value],
    ) -> Result<ProviderResponse> {
        let mut index = self.calls.lock().unwrap();
        let response = self
            .responses
            .get(*index)
            .cloned()
            .context("mock provider out of scripted responses")?;
        *index += 1;
        Ok(response)
    }
}

pub struct OpenRouterProvider {
    api_key: String,
    base_url: String,
    client: ureq::Agent,
}

impl OpenRouterProvider {
    pub fn from_env(base_url: &str) -> Result<Self> {
        let api_key =
            std::env::var("OPENROUTER_API_KEY").context("OPENROUTER_API_KEY is not set")?;
        Ok(Self {
            api_key,
            base_url: base_url.trim_end_matches('/').to_owned(),
            client: ureq::Agent::new_with_defaults(),
        })
    }
}

impl ChatProvider for OpenRouterProvider {
    fn complete(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
    ) -> Result<ProviderResponse> {
        let body = openrouter_chat_completion_body(model, messages, tools);
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send_json(body)
            .map_err(|err| anyhow::anyhow!("openrouter transport error: {err}"))?;
        let status = response.status();
        let body_text = response
            .into_body()
            .read_to_string()
            .map_err(|err| anyhow::anyhow!("openrouter read body: {err}"))?;
        let payload: Value =
            serde_json::from_str(&body_text).unwrap_or_else(|_| json!({ "raw_body": body_text }));
        if !status.is_success() {
            bail!("openrouter HTTP {status}: {payload}");
        }
        let choice = payload
            .pointer("/choices/0/message")
            .context("missing choices[0].message")?;
        let message = choice
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_owned();
        let tool_calls = choice
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|call| {
                        let id = call.get("id")?.as_str()?.to_owned();
                        let function = call.get("function")?;
                        let name = function.get("name")?.as_str()?.to_owned();
                        let arguments = function.get("arguments")?;
                        let arguments = if arguments.is_string() {
                            serde_json::from_str(arguments.as_str().unwrap_or("{}"))
                                .unwrap_or(json!({}))
                        } else {
                            arguments.clone()
                        };
                        Some(ToolCall {
                            id,
                            name,
                            arguments,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ProviderResponse {
            message,
            tool_calls,
        })
    }
}
