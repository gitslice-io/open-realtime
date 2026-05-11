use open_realtime::protocol::ServerEvent;
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, TestSession};

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn c1_connect_with_valid_key() {
    dotenvy::dotenv().ok();
    let session = openai_connect().await.expect("should connect successfully");
    assert!(session.session_id.is_some());
    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn c2_connect_with_invalid_key() {
    use futures_util::StreamExt;
    use tokio_tungstenite::connect_async;

    let url_str = "wss://api.openai.com/v1/realtime?model=gpt-realtime";
    let uri: tokio_tungstenite::tungstenite::http::Uri = url_str.parse().unwrap();
    let host = uri.host().unwrap().to_string();

    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(uri)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .header("Authorization", "Bearer sk-invalid-key-12345")
        .header("OpenAI-Beta", "realtime=v1")
        .body(())
        .unwrap();

    let result = connect_async(request).await;
    match result {
        Ok((mut ws, _)) => {
            // Might connect but then get error event
            let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
                .await
                .ok()
                .flatten();
            if let Some(Ok(msg)) = msg {
                if let Ok(text) = msg.to_text() {
                    let event: Result<ServerEvent, _> = serde_json::from_str(text);
                    if let Ok(ServerEvent::Error { error }) = event {
                        assert!(!error.message.is_empty());
                        return;
                    }
                }
            }
            // Some servers just 401 at HTTP level
        }
        Err(e) => {
            // Connection failed - expected behavior
            assert!(e.to_string().contains("401") || e.to_string().contains("403") || e.to_string().contains("HTTP"), "Expected auth error, got: {}", e);
        }
    }
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn c5_connect_model_query_param() {
    dotenvy::dotenv().ok();
    use futures_util::StreamExt;
    use tokio_tungstenite::connect_async;

    let api_key = std::env::var("OAI_KEY").unwrap_or_default();
    let is_local = std::env::var("REALTIME_URL").is_ok();

    let url_str = if is_local {
        format!("{}?model=gpt-realtime", std::env::var("REALTIME_URL").unwrap())
    } else {
        "wss://api.openai.com/v1/realtime?model=gpt-realtime".to_string()
    };

    let uri: tokio_tungstenite::tungstenite::http::Uri = url_str.parse().unwrap();
    let host = uri.host().unwrap().to_string();

    let mut builder = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(uri)
        .header("Host", host)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        );

    if !is_local {
        builder = builder
            .header("Authorization", format!("Bearer {}", api_key))
            .header("OpenAI-Beta", "realtime=v1");
    }

    let request = builder.body(()).unwrap();
        .unwrap();

    let (mut ws, _) = connect_async(request).await.unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let text = msg.to_text().unwrap();
    let event: ServerEvent = serde_json::from_str(text).unwrap();
    match event {
        ServerEvent::SessionCreated { session: s } => {
            if !is_local {
                assert!(
                    s.model.contains("realtime"),
                    "Expected realtime model, got: {}",
                    s.model
                );
            }
        }
        other => panic!("Expected session.created, got: {}", other.event_type()),
    }
    ws.close(None).await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn c6_graceful_disconnect() {
    dotenvy::dotenv().ok();
    let session = openai_connect().await.expect("should connect successfully");
    session.close().await.expect("should close cleanly");
}

#[tokio::test]
async fn local_fake_connect_works() {
    let fake = fake_transport();
    let session = connect_with(fake).await.unwrap();
    assert!(session.session_id.is_some());
    session.close().await.ok();
}
