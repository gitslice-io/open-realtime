use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use crate::protocol::{ClientEvent, ServerEvent};
use crate::traits::RealtimeTransport;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// OpenAI Realtime API transport over WebSocket.
pub struct OpenAiTransport {
    ws: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiTransport {
    /// Create a new transport pointing at OpenAI's production endpoint.
    pub fn new(api_key: String) -> Self {
        Self {
            ws: None,
            api_key,
            model: "gpt-realtime".to_string(),
            base_url: "wss://api.openai.com/v1/realtime".to_string(),
        }
    }

    /// Create a transport pointing at a localhost server (no auth needed).
    pub fn localhost(model: &str) -> Self {
        Self {
            ws: None,
            api_key: String::new(),
            model: model.to_string(),
            base_url: "ws://127.0.0.1:8080/v1/realtime".to_string(),
        }
    }

    /// Create a transport pointing at a custom URL (no auth needed for local testing).
    pub fn custom(base_url: &str, model: &str) -> Self {
        Self {
            ws: None,
            api_key: String::new(),
            model: model.to_string(),
            base_url: base_url.to_string(),
        }
    }

    /// Create from environment variable OAI_KEY.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OAI_KEY").context("OAI_KEY not set")?;
        Ok(Self::new(api_key))
    }
}

#[async_trait]
impl RealtimeTransport for OpenAiTransport {
    async fn connect(&mut self) -> Result<()> {
        let url_str = format!("{}?model={}", self.base_url, self.model);
        let uri: tokio_tungstenite::tungstenite::http::Uri =
            url_str.parse().context("Failed to parse URL")?;
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
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            .body(())
            .context("Failed to build request")?;

        let (ws, _) = connect_async(request).await.context("Failed to connect")?;
        self.ws = Some(ws);
        Ok(())
    }

    async fn send(&mut self, event: &ClientEvent) -> Result<()> {
        let json = serde_json::to_string(event)?;
        if let Some(ws) = &mut self.ws {
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                json,
            ))
            .await?;
        }
        Ok(())
    }

    async fn recv(&mut self, timeout: Duration) -> Result<Option<ServerEvent>> {
        let ws = self.ws.as_mut().context("Not connected")?;
        let msg = match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(msg))) => msg,
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(None) => return Ok(None),
            Err(_) => return Ok(None), // Timeout
        };
        let text = msg.to_text().context("Expected text message")?;
        let event: ServerEvent = serde_json::from_str(text)?;
        Ok(Some(event))
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(mut ws) = self.ws.take() {
            ws.close(None).await?;
        }
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.ws.is_some()
    }
}
