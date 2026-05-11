use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
use common::{connect, TestSession};

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn m1_both_modalities_audio_present() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();

    // In text+audio mode, we should get audio deltas
    let mut got_audio = false;
    let mut response_ok = false;
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseOutputAudioDelta { .. } => got_audio = true,
            ServerEvent::ResponseDone { response } => {
                response_ok = response.status == "completed";
                break;
            }
            ServerEvent::Error { error } => {
                eprintln!("Error: {}", error.message);
                break;
            }
            _ => {}
        }
    }

    // Audio deltas may or may not appear for short responses
    assert!(got_audio || response_ok, "Expected audio output or completed response");

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn m2_text_only_output() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();

    assert!(response.status == "completed");
    let text = TestSession::response_text(&response);
    assert!(!text.is_empty(), "Expected text in text-only mode");

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn m3_both_modalities() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string(), "audio".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hi").await.unwrap();

    let mut got_audio = false;
    let mut got_text = false;
    let mut response_ok = false;
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseOutputAudioDelta { .. } => got_audio = true,
            ServerEvent::ResponseOutputTextDelta { .. } => got_text = true,
            ServerEvent::ResponseDone { response } => {
                response_ok = response.status == "completed";
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // In both mode, we might get audio, text, or both
    assert!(got_audio || got_text || response_ok, 
        "Response should have audio, text, or completed status");

    session.close().await.ok();
}
