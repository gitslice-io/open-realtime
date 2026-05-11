use open_realtime::protocol::{
    ReasoningConfig, SessionConfig, TurnDetection,
};
mod common;
use common::connect;

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s1_session_created_structure() {
    dotenvy::dotenv().ok();
    let session = connect().await.unwrap();
    // session_id should be set from session.created
    assert!(session.session_id.is_some());
    assert!(!session.session_id.as_ref().unwrap().is_empty());
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s2_update_instructions() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            instructions: Some("You are a helpful pirate assistant. Respond like a pirate.".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s3_update_voice_before_audio() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            voice: Some("alloy".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s5_update_modalities() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    // Set text-only
    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    // Set both
    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string(), "audio".to_string()]),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s6_update_temperature() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            temperature: Some(0.8),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s7_update_reasoning_effort() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    // Set low reasoning effort (only available on gpt-realtime-2)
    match session
        .update_session(SessionConfig {
            reasoning: Some(ReasoningConfig {
                effort: Some("low".into()),
                generate_summary: None,
            }),
            ..Default::default()
        })
        .await
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Reasoning not available on this model: {}", e);
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s8_update_turn_detection() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    // Set semantic VAD
    session
        .update_session(SessionConfig {
            turn_detection: Some(TurnDetection {
                turn_type: "semantic_vad".into(),
                threshold: None,
                silence_duration_ms: None,
                prefix_padding_ms: None,
            ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn s9_update_max_tokens() {
    dotenvy::dotenv().ok();
    let mut session = connect().await.unwrap();

    session
        .update_session(SessionConfig {
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    session.close().await.ok();
}
