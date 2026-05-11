use anyhow::{Context, Result};
use open_realtime::protocol::{
    ClientEvent, ConversationItem, ContentPart, ResponseConfig, ResponseState, ServerEvent,
    SessionConfig,
};
use open_realtime::traits::RealtimeTransport;
use std::time::Duration;

/// A test session that wraps any RealtimeTransport.
pub struct TestSession<T: RealtimeTransport> {
    pub transport: T,
    pub session_id: Option<String>,
}

/// Connect using the given transport and wait for session.created.
pub async fn connect_with<T: RealtimeTransport>(mut transport: T) -> Result<TestSession<T>> {
    transport.connect().await.context("Failed to connect")?;

    let event = transport
        .recv(Duration::from_secs(10))
        .await
        .context("No response from transport")?
        .context("Transport returned None")?;

    let session_id = match event {
        ServerEvent::SessionCreated { session: s } => Some(s.id),
        ServerEvent::Error { error } => {
            anyhow::bail!("Connection error: {}", error.message);
        }
        other => {
            anyhow::bail!("Expected session.created, got: {}", other.event_type());
        }
    };

    Ok(TestSession {
        transport,
        session_id,
    })
}

impl<T: RealtimeTransport> TestSession<T> {
    /// Send a JSON client event.
    pub async fn send(&mut self, event: &ClientEvent) -> Result<()> {
        self.transport.send(event).await
    }

    /// Receive the next server event with a timeout.
    pub async fn recv_timeout(&mut self, timeout: Duration) -> Result<ServerEvent> {
        self.transport
            .recv(timeout)
            .await?
            .context("Transport returned None (timeout or closed)")
    }

    /// Wait until we receive an event of a specific type.
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
            if let ServerEvent::Error { error } = &event {
                anyhow::bail!(
                    "Received error while waiting for {}: {}",
                    expected_type,
                    error.message
                );
            }
        }
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
        let item = ConversationItem {
            id: String::new(),
            item_type: "message".to_string(),
            status: String::new(),
            role: "user".to_string(),
            content: vec![ContentPart::InputText {
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
    pub async fn wait_for_response_done(&mut self) -> Result<ResponseState> {
        let event = self
            .expect_event_type("response.done", Duration::from_secs(30))
            .await?;
        match event {
            ServerEvent::ResponseDone { response } => Ok(response),
            other => anyhow::bail!("Expected response.done, got: {}", other.event_type()),
        }
    }

    /// Drain any pending events with a short timeout.
    pub async fn drain(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        loop {
            match self.transport.recv(Duration::from_millis(100)).await {
                Ok(Some(event)) => events.push(event),
                _ => break,
            }
        }
        events
    }

    /// Close the transport.
    pub async fn close(mut self) -> Result<()> {
        self.transport.close().await
    }
}

/// Get all text from a response's output items.
pub fn response_text(response: &ResponseState) -> String {
    response
        .output
        .iter()
        .flat_map(|item| &item.content)
        .filter(|c| c.content_type == "text" || c.content_type == "output_text")
        .map(|c| {
            if !c.text.is_empty() {
                c.text.clone()
            } else {
                c.transcript.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Create a fake transport pre-configured with a session.
pub fn fake_transport() -> open_realtime::transport::fake::FakeTransport {
    let mut fake = open_realtime::transport::fake::FakeTransport::new();
    fake.setup_session("sess_fake_test");
    fake
}

/// Connect using the OpenAI realtime API (requires OAI_KEY env var).
/// If REALTIME_URL is set, connects to that URL instead (e.g. ws://localhost:8080).
pub async fn openai_connect() -> Result<TestSession<open_realtime::transport::openai::OpenAiTransport>> {
    dotenvy::dotenv().ok();
    if let Ok(url) = std::env::var("REALTIME_URL") {
        // Use localhost or custom URL
        let transport = open_realtime::transport::openai::OpenAiTransport::custom(&url, "fake-realtime");
        return connect_with(transport).await;
    }
    let transport = open_realtime::transport::openai::OpenAiTransport::from_env()?;
    connect_with(transport).await
}

/// Connect to the local realtime server at ws://127.0.0.1:8080.
pub async fn localhost_connect() -> Result<TestSession<open_realtime::transport::openai::OpenAiTransport>> {
    let transport = open_realtime::transport::openai::OpenAiTransport::localhost("fake-realtime");
    connect_with(transport).await
}
