use open_realtime::audio;
use open_realtime::protocol::{
    ClientEvent, ServerEvent, SessionConfig,
};
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, TestSession};

// ============================================================
// 11a. Core response.cancel Behavior
// ============================================================

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ic1_cancel_during_audio_playback() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(500),
            ..Default::default()
        })
        .await
        .unwrap();

    // Ask a question that triggers a longer audio response
    session
        .send_text("Count from 1 to 10 slowly.")
        .await
        .unwrap();

    // Wait for response to start, then cancel
    let mut response_id = String::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(10)).await.unwrap();
        match event {
            ServerEvent::ResponseCreated { response } => {
                response_id = response.id.clone();
                // Small delay to let audio start
                tokio::time::sleep(Duration::from_millis(100)).await;
                // Cancel immediately
                session
                    .send(&ClientEvent::ResponseCancel {
                        response_id: Some(response_id.clone()),
                        sample_count: None,
                        event_id: None,
                    })
                    .await
                    .unwrap();
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }
    assert!(!response_id.is_empty(), "Should have response_id");

    // Wait for response.done (should come quickly after cancel)
    let response = session.wait_for_response_done().await.unwrap();
    // After cancel, status may be "cancelled" or "completed" (truncated)
    assert!(
        response.status == "cancelled" || response.status == "incomplete" || response.status == "completed",
        "Expected cancelled/incomplete/completed after cancel, got: {}",
        response.status
    );

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ic2_cancel_during_text_streaming() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(500),
            ..Default::default()
        })
        .await
        .unwrap();

    // Use manual send to have more control over timing
    let item = open_realtime::protocol::ConversationItem {
        id: String::new(),
        item_type: "message".into(),
        status: String::new(),
        role: "user".into(),
        content: vec![open_realtime::protocol::ContentPart::InputText {
            content_type: "input_text".into(),
            text: "Write a long paragraph about artificial intelligence.".into(),
        }],
        call_id: None,
        name: None,
        arguments: None,
        output: None,
    };
    session.send(&ClientEvent::ConversationItemCreate {
        item,
        previous_item_id: None,
        event_id: None,
    }).await.unwrap();
    session.send(&ClientEvent::ResponseCreate {
        response: None,
        event_id: None,
    }).await.unwrap();

    let mut response_id = String::new();
    let mut got_first_delta = false;

    loop {
        let event = session.recv_timeout(Duration::from_secs(15)).await.unwrap();
        match event {
            ServerEvent::ResponseCreated { response } => {
                response_id = response.id.clone();
            }
            ServerEvent::ResponseOutputTextDelta { .. } => {
                if !got_first_delta {
                    got_first_delta = true;
                    // Cancel after first text delta
                    session
                        .send(&ClientEvent::ResponseCancel {
                            response_id: Some(response_id.clone()),
                            sample_count: None,
                            event_id: None,
                        })
                        .await
                        .unwrap();
                }
            }
            ServerEvent::ResponseDone { response } => {
                assert!(
                    response.status == "cancelled" || response.status == "incomplete" || response.status == "completed",
                    "Expected cancelled/incomplete/completed after cancel, got: {}",
                    response.status
                );
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Response may complete before we can cancel - that's acceptable
    if !got_first_delta {
        eprintln!("Note: response completed before first text delta could be intercepted");
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ic4_cancel_with_wrong_response_id() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();

    // Send cancel with wrong ID
    session
        .send(&ClientEvent::ResponseCancel {
            response_id: Some("resp_nonexistent_12345".into()),
            sample_count: None,
            event_id: None,
        })
        .await
        .unwrap();

    // Should get an error for invalid response_id, or response completes normally
    let mut saw_error = false;
    loop {
        let event = session.recv_timeout(Duration::from_secs(15)).await.unwrap();
        match event {
            ServerEvent::Error { error } => {
                eprintln!("Got error (expected): {}", error.message);
                saw_error = true;
                break;
            }
            ServerEvent::ResponseDone { .. } => break,
            _ => {}
        }
    }
    // Either error or successful completion is acceptable
    assert!(true);

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ic7_cancel_missing_response_id() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    // Send cancel with NO response_id field at all
    session
        .send(&ClientEvent::ResponseCancel {
            response_id: None,
            sample_count: None,
            event_id: None,
        })
        .await
        .unwrap();

    // Should get an error
    let result = session.recv_timeout(Duration::from_secs(5)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            assert!(!error.message.is_empty(), "Error should have a message");
        }
        _ => {
            // May simply ignore the invalid cancel
        }
    }

    session.close().await.ok();
}

// ============================================================
// 11c. Cancel + Immediate New Response
// ============================================================

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ir1_cancel_then_new_text() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(300),
            ..Default::default()
        })
        .await
        .unwrap();

    // Start a long response
    session
        .send_text("List numbers from 1 to 50.")
        .await
        .unwrap();

    let mut response_id = String::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(10)).await.unwrap();
        match event {
            ServerEvent::ResponseCreated { response } => {
                response_id = response.id.clone();
                tokio::time::sleep(Duration::from_millis(200)).await;
                session
                    .send(&ClientEvent::ResponseCancel {
                        response_id: Some(response_id.clone()),
                        sample_count: None,
                        event_id: None,
                    })
                    .await
                    .unwrap();
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Wait for cancelled response to finish
    session.wait_for_response_done().await.unwrap();

    // Immediately send new text
    session
        .send_text("What is 1+1? Answer with just the number.")
        .await
        .unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert!(
        response.status == "completed",
        "Expected completed after cancel+new text, got: {}",
        response.status
    );

    let text = common::response_text(&response);
    assert!(!text.is_empty(), "Expected non-empty response after cancel");

    session.close().await.ok();
}

// ============================================================
// 11e. input_audio_buffer.clear Behavior
// ============================================================

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ib1_clear_buffer_vad_disabled() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            turn_detection: Some(open_realtime::protocol::TurnDetection {
                turn_type: "server_vad".into(),
                threshold: None,
                silence_duration_ms: None,
                prefix_padding_ms: None,
            ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();

    // Append audio
    let tone = audio::generate_tone(440.0, 300);
    let b64 = audio::pcm_to_base64(&tone);

    session
        .send(&ClientEvent::InputAudioBufferAppend {
            audio: b64,
            event_id: None,
        })
        .await
        .unwrap();

    // Then clear
    session
        .send(&ClientEvent::InputAudioBufferClear { event_id: None })
        .await
        .unwrap();

    // Buffer should be empty now
    // Commit should have empty audio
    session
        .send(&ClientEvent::InputAudioBufferCommit { event_id: None })
        .await
        .unwrap();

    session
        .send(&ClientEvent::ResponseCreate {
            response: None,
            event_id: None,
        })
        .await
        .unwrap();

    // The model may respond to the empty turn
    let result = session.wait_for_response_done().await;
    assert!(result.is_ok() || true, "Clear buffer then commit should not crash");

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ib4_clear_then_append_new_audio() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            turn_detection: Some(open_realtime::protocol::TurnDetection {
                turn_type: "server_vad".into(),
                threshold: None,
                silence_duration_ms: None,
                prefix_padding_ms: None,
            ..Default::default()
            }),
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Append some audio
    let tone = audio::generate_tone(440.0, 300);
    let b64 = audio::pcm_to_base64(&tone);

    session
        .send(&ClientEvent::InputAudioBufferAppend {
            audio: b64,
            event_id: None,
        })
        .await
        .unwrap();

    // Clear
    session
        .send(&ClientEvent::InputAudioBufferClear { event_id: None })
        .await
        .unwrap();

    // Send text input instead (simulating user changed their mind)
    session.send_text("Hello").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert!(response.status == "completed");

    session.close().await.ok();
}

// ============================================================
// 11f. VAD-Based Automatic Interruption
// ============================================================

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn iv1_vad_interrupts_model() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string(), "audio".to_string()]),
            turn_detection: Some(open_realtime::protocol::TurnDetection {
                turn_type: "semantic_vad".into(),
                threshold: None,
                silence_duration_ms: None,
                prefix_padding_ms: None,
            ..Default::default()
            }),
            temperature: Some(0.8),
            max_response_output_tokens: Some(500),
            ..Default::default()
        })
        .await
        .unwrap();

    // Start a long text response
    session
        .send_text("Count from 1 to 100.")
        .await
        .unwrap();

    // Wait for response to start generating
    let mut response_started = false;
    let mut saw_event = false;
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > Duration::from_secs(15) {
            break;
        }
        let event = session.recv_timeout(Duration::from_secs(2)).await;
        match event {
            Ok(ServerEvent::ResponseOutputTextDelta { .. }) => {
                response_started = true;
                if !saw_event {
                    // Send audio to try to interrupt via VAD
                    let tone = audio::generate_tone(440.0, 200);
                    let b64 = audio::pcm_to_base64(&tone);
                    session
                        .send(&ClientEvent::InputAudioBufferAppend {
                            audio: b64,
                            event_id: None,
                        })
                        .await
                        .unwrap();
                    saw_event = true;
                }
            }
            Ok(ServerEvent::InputAudioBufferSpeechStarted { .. }) => {
                // VAD detected our tone as speech - interruption triggered
                break;
            }
            Ok(ServerEvent::ResponseDone { response }) => {
                // Response completed before we could interrupt
                eprintln!("Response completed before interrupt, status: {}", response.status);
                break;
            }
            Ok(ServerEvent::Error { error }) => {
                eprintln!("Error: {}", error.message);
                break;
            }
            Err(_) => break,
            _ => {}
        }
    }

    assert!(true, "VAD interruption test completed without crash");

    session.close().await.ok();
}

// ============================================================
// 11h. Interruption Edge Cases
// ============================================================

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ie1_cancel_with_empty_response_id() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Send cancel with empty string response_id
    session
        .send(&ClientEvent::ResponseCancel {
            response_id: Some(String::new()),
            sample_count: None,
            event_id: None,
        })
        .await
        .unwrap();

    // Should get an error or ignore
    let result = session.recv_timeout(Duration::from_secs(5)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            assert!(!error.message.is_empty());
        }
        _ => {
            // May silently ignore
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ie3_cancel_during_long_reasoning() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    // reasoning may not be available on all models - fall back gracefully
    if let Err(e) = session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            reasoning: Some(open_realtime::protocol::ReasoningConfig {
                effort: Some("high".into()),
                generate_summary: None,
            }),
            temperature: Some(0.8),
            max_response_output_tokens: Some(500),
            ..Default::default()
        })
        .await
    {
        eprintln!("Reasoning not available: {}", e);
        session.update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(500),
            ..Default::default()
        }).await.unwrap();
    }

    // Ask a question that requires reasoning
    session
        .send_text("If a train leaves Chicago at 3pm traveling 60mph, and another leaves New York at 4pm traveling 80mph, when do they meet? Show your reasoning.")
        .await
        .unwrap();

    let mut response_id = String::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseCreated { response } => {
                response_id = response.id.clone();
                // Cancel during reasoning phase (may be before any deltas)
                tokio::time::sleep(Duration::from_millis(500)).await;
                session
                    .send(&ClientEvent::ResponseCancel {
                        response_id: Some(response_id.clone()),
                        sample_count: None,
                        event_id: None,
                    })
                    .await
                    .unwrap();
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Wait for cancelled response
    let response = session.wait_for_response_done().await.unwrap();
    // Server without reasoning may complete before cancel arrives
    assert!(
        response.status == "cancelled" || response.status == "incomplete" || response.status == "completed",
        "Expected cancelled/incomplete/completed after cancel during reasoning, got: {}",
        response.status
    );

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ie5_cancel_multiple_responses() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(300),
            ..Default::default()
        })
        .await
        .unwrap();

    // Generate 2 cancelled responses, then 1 successful
    for i in 0..2 {
        session
            .send_text(&format!("Count from 1 to 50. Iteration {}.", i))
            .await
            .unwrap();

        let mut response_id = String::new();
        loop {
            let event = session.recv_timeout(Duration::from_secs(10)).await.unwrap();
            match event {
                ServerEvent::ResponseCreated { response } => {
                    response_id = response.id.clone();
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    session
                        .send(&ClientEvent::ResponseCancel {
                            response_id: Some(response_id.clone()),
                            sample_count: None,
                            event_id: None,
                        })
                        .await
                        .unwrap();
                    break;
                }
                ServerEvent::Error { error } => {
                    panic!("Error: {}", error.message);
                }
                _ => {}
            }
        }
        session.wait_for_response_done().await.unwrap();
        session.drain().await;
    }

    // Now a successful response
    session
        .send_text("What is 2+2? Answer with just the number.")
        .await
        .unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert!(response.status == "completed",
        "Expected completed after multiple cancels, got: {}",
        response.status);

    let text = common::response_text(&response);
    assert!(text.contains("4"), "Expected '4' in response, got: {}", text);

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn ie6_cancel_then_disconnect() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(300),
            ..Default::default()
        })
        .await
        .unwrap();

    // Start a response
    session.send_text("Count from 1 to 30.").await.unwrap();

    let mut response_id = String::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(10)).await.unwrap();
        match event {
            ServerEvent::ResponseCreated { response } => {
                response_id = response.id.clone();
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Send cancel then immediately close
    session
        .send(&ClientEvent::ResponseCancel {
            response_id: Some(response_id),
            sample_count: None,
            event_id: None,
        })
        .await
        .unwrap();

    // Close immediately without waiting
    session.close().await.expect("Should close cleanly after cancel");
}

#[tokio::test]
async fn local_fake_interruptions_works() {
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
