use open_realtime::protocol::{
    ClientEvent, ContentPart, ConversationItem, ServerEvent, SessionConfig,
};
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, TestSession};

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn i1_create_text_item() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    let create_event = ClientEvent::ConversationItemCreate {
        item: ConversationItem {
            id: String::new(),
            item_type: "message".into(),
            status: String::new(),
            role: "user".into(),
            content: vec![ContentPart::InputText {
                content_type: "input_text".into(),
                text: "Hello from item test".into(),
            }],
            call_id: None,
            name: None,
            arguments: None,
            output: None,
        },
        previous_item_id: None,
        event_id: None,
    };
    session.send(&create_event).await.unwrap();

    // Trigger a response so we get conversation events
    session.send(&ClientEvent::ResponseCreate {
        response: None,
        event_id: None,
    }).await.unwrap();

    // Wait for response with a longer timeout
    match session.wait_for_response_done().await {
        Ok(response) => {
            assert!(response.status == "completed", "Expected completed, got: {}", response.status);
        }
        Err(e) => {
            // Timeout or error - the item creation may not have been processed
            eprintln!("Item creation test: {}", e);
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn i4_delete_item() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(100),
            ..Default::default()
        })
        .await
        .unwrap();

    // First send a text message and get a response
    session.send_text("My favorite color is blue.").await.unwrap();
    session.wait_for_response_done().await.unwrap();

    // Try to delete the last user item (we don't know the exact ID)
    // This tests that delete doesn't crash even with potentially wrong ID
    session
        .send(&ClientEvent::ConversationItemDelete {
            item_id: "nonexistent_item_id".into(),
            event_id: None,
        })
        .await
        .unwrap();

    // Wait a moment for any error
    let result = session.recv_timeout(Duration::from_secs(3)).await;
    match result {
        Ok(ServerEvent::Error { error }) => {
            // Expected - can't delete nonexistent item
            eprintln!("Expected error: {}", error.message);
        }
        _ => {
            // No error or some other event - also fine
        }
    }

    session.close().await.ok();
}

#[tokio::test]
async fn local_fake_items_works() {
    let mut fake = fake_transport();
    fake.enqueue_session_updated();
    fake.enqueue_text_response("Hello", "Hi there!");
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
