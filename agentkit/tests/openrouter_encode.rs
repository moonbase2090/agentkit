use agentkit::session::{Event, SessionLog};
use agentkit::{encode_openrouter_message, openrouter_chat_completion_body};
use serde_json::Value;

#[test]
fn error_tool_result_encodes_for_openrouter_followup() {
    let events = vec![
        Event::SystemPrompt {
            content: "sys".to_owned(),
        },
        Event::UserMessage {
            content: "go".to_owned(),
        },
        Event::ToolCall {
            id: "call_1".to_owned(),
            name: "rookrunner_list_failed".to_owned(),
            arguments: serde_json::json!({}),
        },
        Event::ToolResult {
            call_id: "call_1".to_owned(),
            name: "rookrunner_list_failed".to_owned(),
            content: "error: missing log".to_owned(),
        },
    ];
    let messages = SessionLog::messages_from_events(&events);
    let body = openrouter_chat_completion_body("anthropic/claude-haiku-5.5", &messages, &[]);
    let encoded = body
        .get("messages")
        .and_then(Value::as_array)
        .expect("messages array");
    assert_eq!(encoded.len(), 4);

    let assistant = &encoded[2];
    let tool_calls = assistant
        .get("tool_calls")
        .and_then(Value::as_array)
        .expect("assistant tool_calls");
    let arguments = tool_calls[0]
        .pointer("/function/arguments")
        .and_then(Value::as_str)
        .expect("string arguments");
    assert!(serde_json::from_str::<Value>(arguments).is_ok());

    let tool = &encoded[3];
    assert_eq!(tool.get("role").and_then(Value::as_str), Some("tool"));
    assert_eq!(
        tool.get("tool_call_id").and_then(Value::as_str),
        Some("call_1")
    );
    assert_eq!(
        tool.get("name").and_then(Value::as_str),
        Some("rookrunner_list_failed")
    );
    assert!(tool
        .get("content")
        .and_then(Value::as_str)
        .is_some_and(|content| content.starts_with("error:")));

    let roundtrip = encode_openrouter_message(&messages[2]);
    assert!(roundtrip.get("tool_calls").is_some());
}
