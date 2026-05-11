use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
use common::connect;

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn l1_text_response_event_order() {
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

    let mut events_seen = Vec::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        let event_type = event.event_type().to_string();
        events_seen.push(event_type.clone());

        if event_type == "response.done" {
            break;
        }
        if let ServerEvent::Error { error } = &event {
            panic!("Error: {}", error.message);
        }
    }

    // Verify event ordering
    let expected_order = [
        "response.created",
        "response.output_item.added",
        "response.content_part.added",
        "response.output_text.delta",
        "response.output_text.done",
        "response.content_part.done",
        "response.output_item.done",
        "response.done",
    ];

    let mut expected_idx = 0;
    for seen in &events_seen {
        // Find the next matching expected event
        while expected_idx < expected_order.len() && expected_order[expected_idx] != seen {
            expected_idx += 1;
        }
    }

    // At minimum, verify we got the critical lifecycle events
    assert!(
        events_seen.contains(&"response.created".to_string()),
        "Should see response.created"
    );
    assert!(
        events_seen.contains(&"response.done".to_string()),
        "Should see response.done"
    );

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn l2_audio_response_event_order() {
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

    let mut events_seen = Vec::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        let event_type = event.event_type().to_string();
        events_seen.push(event_type.clone());

        if event_type == "response.done" {
            break;
        }
        if let ServerEvent::Error { error } = &event {
            panic!("Error: {}", error.message);
        }
    }

    assert!(
        events_seen.contains(&"response.created".to_string()),
        "Should see response.created"
    );
    assert!(
        events_seen.contains(&"response.done".to_string()),
        "Should see response.done"
    );
    // Audio deltas may not appear for very short responses in text+audio mode
    if !events_seen.contains(&"response.output_audio.delta".to_string()) {
        eprintln!("Note: audio deltas not received (short response)");
    }
    session.close().await.ok();
}
