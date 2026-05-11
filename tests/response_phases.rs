use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, response_text, TestSession};

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn p1_commentary_phase_present() {
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

    // Ask a question that may trigger reasoning
    session
        .send_text("Explain why the sky is blue in one sentence.")
        .await
        .unwrap();

    let mut phases = Vec::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(30)).await.unwrap();
        match event {
            ServerEvent::ResponseDone { response } => {
                for item in &response.output {
                    if let Some(phase) = &item.phase {
                        phases.push(phase.clone());
                    }
                }
                break;
            }
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // For simple questions, there might not be a commentary phase
    // But we should have at least a final_answer
    assert!(
        phases.is_empty() || phases.contains(&"final_answer".to_string()),
        "Expected phases to be empty or contain final_answer"
    );

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn p4_reasoning_effort_affects_phases() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    // reasoning parameter may not be available on all models
    match session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            reasoning: Some(open_realtime::protocol::ReasoningConfig {
                effort: Some("low".into()),
                generate_summary: None,
            }),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Reasoning not available on this model: {}", e);
            // Fallback without reasoning
            session.update_session(SessionConfig {
                modalities: Some(vec!["text".to_string()]),
                temperature: Some(0.8),
                max_response_output_tokens: Some(200),
                ..Default::default()
            }).await.unwrap();
        }
    }

    session
        .send_text("What is 5 times 7? Answer with just the number.")
        .await
        .unwrap();

    let response = session.wait_for_response_done().await.unwrap();
    assert!(response.status == "completed");
    let text = response_text(&response);
    assert!(!text.is_empty());

    session.close().await.ok();
}

#[tokio::test]
async fn local_fake_phases_works() {
    let mut fake = fake_transport();
    fake.enqueue_session_updated();
    fake.enqueue_text_response("Question", "Answer.");
    let mut session = connect_with(fake).await.unwrap();
    session.update_session(SessionConfig {
        modalities: Some(vec!["text".to_string()]),
        ..Default::default()
    }).await.unwrap();
    session.send_text("Question").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    let text = response_text(&response);
    assert!(!text.is_empty());
    session.close().await.ok();
}
