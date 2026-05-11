use open_realtime::protocol::{
    ClientEvent, ConversationItem, ServerEvent, SessionConfig, Tool,
};
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, TestSession};

fn weather_tool() -> Tool {
    Tool {
        tool_type: "function".into(),
        name: "get_weather".into(),
        description: "Get the current weather for a location".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city name"
                }
            },
            "required": ["location"]
        }),
    }
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn f1_register_single_tool() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            tools: Some(vec![weather_tool()]),
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    // Verify session.updated contains the tool
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn f3_trigger_tool_call() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            tools: Some(vec![weather_tool()]),
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session
        .send_text("What is the weather in Paris?")
        .await
        .unwrap();

    let mut tool_name = String::new();
    let mut tool_args = String::new();
    let mut call_id = String::new();

    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseFunctionCallArgumentsDelta { delta, call_id: cid, .. } => {
                tool_args.push_str(&delta);
                call_id = cid;
            }
            ServerEvent::ResponseFunctionCallArgumentsDone { name, arguments, call_id: cid, .. } => {
                tool_name = name;
                tool_args = arguments;
                call_id = cid;
            }
            ServerEvent::ResponseDone { .. } => break,
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    assert_eq!(tool_name, "get_weather", "Expected get_weather tool call");
    assert!(!tool_args.is_empty(), "Expected non-empty tool arguments");

    // Send back the tool output
    let output_item = ClientEvent::ConversationItemCreate {
        item: ConversationItem {
            id: String::new(),
            item_type: "function_call_output".into(),
            status: String::new(),
            role: String::new(),
            content: vec![],
            call_id: Some(call_id.clone()),
            name: None,
            arguments: None,
            output: Some("{\"temperature\": 22, \"condition\": \"sunny\"}".into()),
        },
        previous_item_id: None,
        event_id: None,
    };
    session.send(&output_item).await.unwrap();
    session
        .send(&ClientEvent::ResponseCreate {
            response: None,
            event_id: None,
        })
        .await
        .unwrap();

    let response = session.wait_for_response_done().await.unwrap();
    assert!(response.status == "completed", "Expected completed status after tool output");

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn f6_register_multiple_tools() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            tools: Some(vec![
                weather_tool(),
                Tool {
                    tool_type: "function".into(),
                    name: "get_time".into(),
                    description: "Get the current time for a location".into(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }),
                },
            ]),
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
async fn local_fake_tools_works() {
    let mut fake = fake_transport();
    fake.enqueue_session_updated();
    fake.enqueue_text_response("Hello", "Hi!");
    let mut session = connect_with(fake).await.unwrap();
    session.update_session(SessionConfig {
        modalities: Some(vec!["text".to_string()]),
        ..Default::default()
    }).await.unwrap();
    session.send_text("Hello").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    session.close().await.ok();
}
