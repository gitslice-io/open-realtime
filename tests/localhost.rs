use open_realtime::protocol::{
    ClientEvent, ConversationItem, ContentPart, ResponseConfig, ServerEvent, SessionConfig, Tool,
};
use open_realtime::traits::RealtimeTransport;
use open_realtime::transport::openai::OpenAiTransport;
use std::time::Duration;

#[path = "common/mod.rs"]
mod common;
use common::{localhost_connect, response_text, TestSession};

// ============================================================
// These tests run against the local realtime server
// Start with: cargo run --bin server
// Then: cargo test --test localhost -- --nocapture
// ============================================================

#[tokio::test]
async fn localhost_connection() {
    let session = localhost_connect().await.expect("should connect to local server");
    assert!(session.session_id.is_some());
    session.close().await.ok();
}

#[tokio::test]
async fn localhost_text_conversation() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session
        .send_text("Hello, how are you?")
        .await
        .unwrap();

    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    let text = response_text(&response);
    assert!(!text.is_empty(), "Expected non-empty text response");
    println!("Response: {}", text);

    session.close().await.ok();
}

#[tokio::test]
async fn localhost_audio_response() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();

    let mut got_audio = false;
    loop {
        let event = session
            .transport
            .recv(Duration::from_secs(10))
            .await
            .unwrap();
        match event {
            Some(ServerEvent::ResponseOutputAudioDelta { .. }) => got_audio = true,
            Some(ServerEvent::ResponseDone { .. }) => break,
            Some(ServerEvent::Error { error }) => {
                panic!("Error: {}", error.message);
            }
            None => break,
            _ => {}
        }
    }

    assert!(got_audio, "Expected audio deltas in audio+text mode");
    session.close().await.ok();
}

#[tokio::test]
async fn localhost_function_calling() {
    let mut session = localhost_connect().await.unwrap();

    let weather_tool = Tool {
        tool_type: "function".into(),
        name: "get_weather".into(),
        description: "Get the current weather for a location".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {"type": "string", "description": "The city name"}
            },
            "required": ["location"]
        }),
    };

    session
        .update_session(SessionConfig {
            tools: Some(vec![weather_tool]),
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session
        .send_text("What is the weather in Paris?")
        .await
        .unwrap();

    let mut call_id = String::new();
    let mut tool_name = String::new();
    let mut tool_args = String::new();

    loop {
        let event = session
            .transport
            .recv(Duration::from_secs(10))
            .await
            .unwrap();
        match event {
            Some(ServerEvent::ResponseFunctionCallArgumentsDelta {
                call_id: cid,
                delta,
                ..
            }) => {
                call_id = cid;
                tool_args.push_str(&delta);
            }
            Some(ServerEvent::ResponseFunctionCallArgumentsDone {
                call_id: cid,
                name,
                arguments,
                ..
            }) => {
                call_id = cid;
                tool_name = name;
                tool_args = arguments;
            }
            Some(ServerEvent::ResponseDone { response }) => {
                if response.status == "completed" {
                    break;
                }
            }
            Some(ServerEvent::Error { error }) => {
                panic!("Error: {}", error.message);
            }
            None => break,
            _ => {}
        }
    }

    assert_eq!(tool_name, "get_weather", "Expected get_weather tool call");
    assert!(!call_id.is_empty(), "Expected call_id from tool call");
    assert!(!tool_args.is_empty(), "Expected tool arguments");

    // Send tool output
    let item = ClientEvent::ConversationItemCreate {
        item: ConversationItem {
            id: String::new(),
            item_type: "function_call_output".into(),
            status: String::new(),
            role: String::new(),
            content: vec![],
            call_id: Some(call_id.clone()),
            name: None,
            arguments: None,
            output: Some(r#"{"temperature": 22, "condition": "sunny"}"#.into()),
        },
        previous_item_id: None,
        event_id: None,
    };
    session.send(&item).await.unwrap();

    session
        .send(&ClientEvent::ResponseCreate {
            response: None,
            event_id: None,
        })
        .await
        .unwrap();

    // Get final response
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    println!("Final response status: {}", response.status);

    session.close().await.ok();
}

#[tokio::test]
async fn localhost_multi_turn() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Turn 1
    session.send_text("My name is Alice.").await.unwrap();
    session.wait_for_response_done().await.unwrap();

    // Turn 2
    session.send_text("What is my name?").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    println!("Multi-turn response: {:?}", response.output.first().map(|o| &o.content));

    session.close().await.ok();
}

#[tokio::test]
async fn localhost_session_update() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            instructions: Some("Be a pirate assistant.".into()),
            temperature: Some(0.8),
            ..Default::default()
        })
        .await
        .unwrap();

    // Verify we can send and get a response after update
    session.send_text("Hello").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");

    session.close().await.ok();
}

#[tokio::test]
async fn localhost_cancel_response() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session
        .send_text("Count from 1 to 100.")
        .await
        .unwrap();

    // Wait for response.created, then cancel
    let mut response_id = String::new();
    loop {
        let event = session
            .transport
            .recv(Duration::from_secs(10))
            .await
            .unwrap();
        match event {
            Some(ServerEvent::ResponseCreated { response }) => {
                response_id = response.id.clone();
                session
                    .send(&ClientEvent::ResponseCancel {
                        response_id: Some(response_id),
                        sample_count: None,
                        event_id: None,
                    })
                    .await
                    .unwrap();
                break;
            }
            Some(ServerEvent::Error { error }) => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Wait for cancelled response
    let response = session.wait_for_response_done().await.unwrap();
    println!("Cancel status: {}", response.status);
    // Local server sets cancelled status
    assert!(response.status == "cancelled" || response.status == "completed");

    session.close().await.ok();
}

#[tokio::test]
async fn localhost_streaming_text_deltas() {
    let mut session = localhost_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello world").await.unwrap();

    let mut deltas = Vec::new();
    loop {
        let event = session
            .transport
            .recv(Duration::from_secs(10))
            .await
            .unwrap();
        match event {
            Some(ServerEvent::ResponseOutputTextDelta { delta, .. }) => {
                deltas.push(delta);
            }
            Some(ServerEvent::ResponseDone { .. }) => break,
            Some(ServerEvent::Error { error }) => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    let text: String = deltas.iter().map(|s| s.as_str()).collect();
    assert!(!text.is_empty(), "Expected text deltas in streaming mode");
    println!("Streaming text: {}", text);

    session.close().await.ok();
}
