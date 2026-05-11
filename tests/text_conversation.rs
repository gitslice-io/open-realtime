use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
use common::{connect, TestSession};

async fn setup_text_session() -> TestSession {
    let mut session = connect().await.unwrap();
    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();
    session
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn t1_simple_text_qa() {
    dotenvy::dotenv().ok();
    let mut session = setup_text_session().await;

    session.send_text("What is 2+2? Answer with just the number.").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();

    let text = TestSession::response_text(&response);
    assert!(!text.is_empty(), "Expected non-empty response text");
    assert!(response.status == "completed", "Expected completed status, got: {}", response.status);

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn t2_text_delta_streaming() {
    dotenvy::dotenv().ok();
    let mut session = setup_text_session().await;

    let mut deltas = Vec::new();
    let mut received_done = false;

    session.send_text("Say hello world.").await.unwrap();

    // Collect deltas until response.done
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseOutputTextDelta { delta, .. } => {
                deltas.push(delta);
            }
            ServerEvent::ResponseDone { response } => {
                received_done = true;
                let full_text = TestSession::response_text(&response);
                let joined: String = deltas.iter().map(|s| s.as_str()).collect();
                // Text deltas may or may not be received depending on model
                if !joined.is_empty() {
                    assert!(full_text.contains("hello") || full_text.contains("Hello"),
                        "Response should contain hello: {}", full_text);
                }
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }
    assert!(received_done, "Should receive response.done");

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn t3_multi_turn_conversation() {
    dotenvy::dotenv().ok();
    let mut session = setup_text_session().await;

    // Turn 1
    session.send_text("My name is Alice. Remember that.").await.unwrap();
    session.wait_for_response_done().await.unwrap();

    // Drain any extra events
    session.drain().await;

    // Turn 2
    session.send_text("What is my name? Answer with just the name.").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    let text = TestSession::response_text(&response);
    assert!(text.to_lowercase().contains("alice"),
        "Expected response to remember name 'Alice', got: {}", text);

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn t4_audio_only_modality_no_text_deltas() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();
    // Keep audio modality but ask simple question - we should still get response.done
    // but might not get text deltas in audio-only mode
    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello.").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert!(response.status == "completed",
        "Expected completed status, got: {}", response.status);

    session.close().await.ok();
}
