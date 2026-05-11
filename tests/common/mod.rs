use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use open_realtime::protocol::{ClientEvent, ResponseConfig, ServerEvent, SessionConfig};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A connected test session to the OpenAI Realtime API.
pub struct TestSession {
    pub ws: WsStream,
    pub session_id: Option<String>,
}

/// Connect to the Realtime API and wait for session.created.
pub async fn connect() -> Result<TestSession> {
    let api_key = std::env::var("OAI_KEY").context("OAI_KEY not set in environment")?;

    let url_str = "wss://api.openai.com/v1/realtime?model=gpt-realtime";
    let uri: tokio_tungstenite::tungstenite::http::Uri = url_str
        .parse()
        .context("Failed to parse URL as URI")?;
    let host = uri.host().context("No host in URL")?.to_string();

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
        .header(
            "Authorization",
            format!("Bearer {}", api_key),
        )
        .header("OpenAI-Beta", "realtime=v1")
        .body(())
        .context("Failed to build WebSocket request")?;

    let (ws, _) = connect_async(request).await.context("Failed to connect")?;
    let mut session = TestSession {
        ws,
        session_id: None,
    };

    // Wait for session.created
    let event = session
        .recv_timeout(Duration::from_secs(10))
        .await
        .context("Timed out waiting for session.created")?;
    match event {
        ServerEvent::SessionCreated { session: s } => {
            session.session_id = Some(s.id);
        }
        ServerEvent::Error { error } => {
            anyhow::bail!("Connection error: {}", error.message);
        }
        other => {
            anyhow::bail!("Expected session.created, got: {}", other.event_type());
        }
    }

    Ok(session)
}

impl TestSession {
    /// Send a JSON client event.
    pub async fn send(&mut self, event: &ClientEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        self.ws
            .send(tokio_tungstenite::tungstenite::Message::Text(
                json.into(),
            ))
            .await?;
        Ok(())
    }

    /// Send a raw JSON string.
    pub async fn send_raw(&mut self, json: &str) -> Result<()> {
        self.ws
            .send(tokio_tungstenite::tungstenite::Message::Text(
                json.to_string().into(),
            ))
            .await?;
        Ok(())
    }

    /// Receive the next server event with a timeout.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Result<ServerEvent> {
        let msg = tokio::time::timeout(timeout, self.ws.next())
            .await
            .context("Timeout waiting for message")?
            .context("WebSocket stream ended")?
            .context("WebSocket error")?;

        let text = msg.to_text().context("Expected text message")?;
        let event: ServerEvent =
            serde_json::from_str(text).context(format!("Failed to parse: {}", text))?;
        Ok(event)
    }

    /// Receive the next server event (default 15s timeout).
    pub async fn recv(&mut self) -> Result<ServerEvent> {
        self.recv_timeout(Duration::from_secs(15)).await
    }

    /// Wait until we receive an event of a specific type, returning it.
    pub async fn expect_event_type(
        &mut self,
        expected_type: &str,
        timeout: Duration,
    ) -> Result<ServerEvent> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline
                .checked_duration_since(tokio::time::Instant::now())
                .unwrap_or(Duration::from_millis(100));
            if remaining.is_zero() {
                anyhow::bail!("Timed out waiting for event: {}", expected_type);
            }
            let event = self.recv_timeout(remaining).await?;
            if event.event_type() == expected_type || expected_type == "*" {
                return Ok(event);
            }
            // If we get an error while waiting, fail
            if let ServerEvent::Error { error } = &event {
                anyhow::bail!("Received error while waiting for {}: {}", expected_type, error.message);
            }
        }
    }

    /// Drain any pending events from the WebSocket (non-blocking, short timeout).
    pub async fn drain(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        loop {
            match self.recv_timeout(Duration::from_millis(200)).await {
                Ok(event) => events.push(event),
                Err(_) => break,
            }
        }
        events
    }

    /// Update session configuration and wait for session.updated.
    pub async fn update_session(&mut self, config: SessionConfig) -> Result<()> {
        let event = ClientEvent::SessionUpdate {
            session: config,
            event_id: None,
        };
        self.send(&event).await?;
        self.expect_event_type("session.updated", Duration::from_secs(10))
            .await?;
        Ok(())
    }

    /// Send a text message and trigger a response.
    pub async fn send_text(&mut self, text: &str) -> Result<()> {
        let item = open_realtime::protocol::ConversationItem {
            id: String::new(),
            item_type: "message".to_string(),
            status: String::new(),
            role: "user".to_string(),
            content: vec![open_realtime::protocol::ContentPart::InputText {
                content_type: "input_text".to_string(),
                text: text.to_string(),
            }],
            call_id: None,
            name: None,
            arguments: None,
            output: None,
        };
        self.send(&ClientEvent::ConversationItemCreate {
            item,
            previous_item_id: None,
            event_id: None,
        })
        .await?;
        self.send(&ClientEvent::ResponseCreate {
            response: Some(ResponseConfig {
                modalities: None,
                instructions: None,
                voice: None,
                output_audio_format: None,
                tools: None,
                tool_choice: None,
                temperature: None,
                max_response_output_tokens: None,
                reasoning: None,
            }),
            event_id: None,
        })
        .await?;
        Ok(())
    }

    /// Wait for response.done and return the full response state.
    pub async fn wait_for_response_done(&mut self) -> Result<open_realtime::protocol::ResponseState> {
        let event = self
            .expect_event_type("response.done", Duration::from_secs(30))
            .await?;
        match event {
            ServerEvent::ResponseDone { response } => Ok(response),
            other => anyhow::bail!("Expected response.done, got: {}", other.event_type()),
        }
    }

    /// Get all text from a response's output items.
    pub fn response_text(response: &open_realtime::protocol::ResponseState) -> String {
        response
            .output
            .iter()
            .filter_map(|item| {
                item.content
                    .iter()
                    .find(|c| c.content_type == "text" || c.content_type == "output_text")
                    .map(|c| {
                        if !c.text.is_empty() {
                            c.text.clone()
                        } else {
                            c.transcript.clone()
                        }
                    })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Close the WebSocket connection.
    pub async fn close(mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .context("Failed to close WebSocket")?;
        Ok(())
    }
}
