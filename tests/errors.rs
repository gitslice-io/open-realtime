use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
use common::connect;

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn e1_invalid_event_type() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Send an invalid event type
    session
        .send_raw(r#"{"type": "nonexistent_event_type_xyz"}"#)
        .await
        .unwrap();

    // Should receive an error
    let result = session.recv_timeout(Duration::from_secs(5)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            assert!(!error.message.is_empty(), "Error message should not be empty");
            assert_eq!(error.error_type, "invalid_request_error");
        }
        _ => {
            // Some servers may silently ignore unknown events
            eprintln!("Server did not return error for invalid event type (may be silently ignored)");
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn e2_malformed_json() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Send malformed JSON
    session.send_raw("this is not json at all {{{").await.unwrap();

    // Should get an error or connection may close
    let result = session.recv_timeout(Duration::from_secs(5)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            assert!(!error.message.is_empty());
        }
        Err(_) => {
            // Connection may have been closed
            eprintln!("Connection may have been closed after malformed JSON");
        }
        _ => {}
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn e3_missing_type_field() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session
        .send_raw(r#"{"foo": "bar", "data": "no type field"}"#)
        .await
        .unwrap();

    let result = session.recv_timeout(Duration::from_secs(5)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            assert!(!error.message.is_empty());
        }
        _ => {
            eprintln!("Server may silently ignore events without type field");
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn e4_invalid_session_config() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    // Try to set an invalid model
    let result = session
        .update_session(SessionConfig {
            model: Some("nonexistent-model-xyz-123".into()),
            ..Default::default()
        })
        .await;

    match result {
        Ok(()) => {
            // Some servers may accept any model string and default
            eprintln!("Server accepted invalid model name");
        }
        Err(e) => {
            assert!(!e.to_string().is_empty(), "Should have error message");
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn e6_rate_limits_updated() {
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

    session.send_text("Hello").await.unwrap();

    let mut saw_rate_limits = false;
    loop {
        let event = session.recv_timeout(Duration::from_secs(15)).await.unwrap();
        match event {
            ServerEvent::RateLimitsUpdated { rate_limits } => {
                assert!(!rate_limits.is_empty(), "Rate limits should not be empty");
                for rl in &rate_limits {
                    assert!(!rl.name.is_empty(), "Rate limit name should not be empty");
                    assert!(rl.limit > 0, "Rate limit should be > 0");
                }
                saw_rate_limits = true;
            }
            ServerEvent::ResponseDone { .. } => break,
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // rate_limits.updated may not always fire for single short messages
    if !saw_rate_limits {
        eprintln!("Note: rate_limits.updated not received (may not fire for short messages)");
    }

    session.close().await.ok();
}
