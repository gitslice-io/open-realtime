use anyhow::{Context, Result};
use async_trait::async_trait;
use crate::protocol::{
    AudioFormat, ClientEvent, ConversationItem, ContentPart, OutputContent,
    ResponseOutputItem, ResponseState, ServerEvent, SessionConfig, SessionState,
    TurnDetection, Usage,
};
use crate::traits::RealtimeTransport;
use std::collections::VecDeque;
use std::time::Duration;

/// In-memory fake transport for testing without a network connection.
///
/// Pre-program server events into the queue. When the client sends events,
/// the fake transport processes them and returns pre-queued responses.
///
/// # Example
/// ```text
/// let mut fake = FakeTransport::new();
/// fake.setup_session("sess_test");
/// fake.enqueue_text_response("Hello", "Hi there!");
/// fake.connect().await.unwrap();
/// // receive session.created, then response events
/// ```
pub struct FakeTransport {
    event_queue: VecDeque<ServerEvent>,
    session_id: String,
    connected: bool,
    /// Track conversation items sent by client.
    conversation: Vec<ConversationItem>,
    counter: u64,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self {
            event_queue: VecDeque::new(),
            session_id: String::new(),
            connected: false,
            conversation: Vec::new(),
            counter: 0,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{}_{}", prefix, self.counter)
    }

    /// Configure the session that will be returned on connect.
    pub fn setup_session(&mut self, session_id: &str) {
        self.session_id = session_id.to_string();
        let session = SessionState {
            id: session_id.to_string(),
            model: "fake-realtime".to_string(),
            modalities: vec!["text".to_string(), "audio".to_string()],
            instructions: "You are a fake test model.".to_string(),
            voice: "alloy".to_string(),
            input_audio_format: Some(AudioFormat::String("pcm16".to_string())),
            output_audio_format: Some(AudioFormat::String("pcm16".to_string())),
            input_audio_transcription: None,
            turn_detection: Some(TurnDetection {
                turn_type: "server_vad".to_string(),
                threshold: Some(0.5),
                silence_duration_ms: Some(200),
                prefix_padding_ms: Some(100),
                create_response: Some(true),
                interrupt_response: Some(true),
                idle_timeout_ms: None,
            }),
            tools: vec![],
            tool_choice: "auto".to_string(),
            temperature: 0.8,
            max_response_output_tokens: serde_json::Value::String("inf".to_string()),
            reasoning: None,
            object: "realtime.session".to_string(),
            speed: None,
            tracing: None,
            truncation: None,
            prompt: None,
            expires_at: None,
            client_secret: None,
            include: None,
            input_audio_noise_reduction: None,
        };

        self.event_queue.push_back(ServerEvent::SessionCreated { session });
    }

    /// Enqueue a server event.
    pub fn enqueue(&mut self, event: ServerEvent) {
        self.event_queue.push_back(event);
    }

    /// Enqueue a text response to a specific input.
    pub fn enqueue_text_response(&mut self, _expected_input: &str, response_text: &str) {
        let item_id = self.next_id("item");
        let resp_id = self.next_id("resp");

        // Simulate the response lifecycle events
        self.enqueue(ServerEvent::ResponseCreated {
            response: ResponseState {
                id: resp_id.clone(),
                status: "in_progress".to_string(),
                status_details: None,
                output: vec![],
                usage: None,
            },
        });

        self.enqueue(ServerEvent::ResponseOutputItemAdded {
            response_id: resp_id.clone(),
            output_index: 0,
            item: ConversationItem {
                id: item_id.clone(),
                item_type: "message".to_string(),
                status: "in_progress".to_string(),
                role: "assistant".to_string(),
                content: vec![],
                call_id: None,
                name: None,
                arguments: None,
                output: None,
            },
        });

        self.enqueue(ServerEvent::ResponseContentPartAdded {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            part: Some(ContentPart::OutputText {
                content_type: "text".to_string(),
                text: String::new(),
            }),
        });

        // Send text delta
        self.enqueue(ServerEvent::ResponseOutputTextDelta {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            delta: response_text.to_string(),
        });

        self.enqueue(ServerEvent::ResponseOutputTextDone {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            text: response_text.to_string(),
        });

        self.enqueue(ServerEvent::ResponseContentPartDone {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            part: Some(ContentPart::OutputText {
                content_type: "text".to_string(),
                text: response_text.to_string(),
            }),
        });

        self.enqueue(ServerEvent::ResponseOutputItemDone {
            response_id: resp_id.clone(),
            output_index: 0,
            item: ConversationItem {
                id: item_id.clone(),
                item_type: "message".to_string(),
                status: "completed".to_string(),
                role: "assistant".to_string(),
                content: vec![ContentPart::OutputText {
                    content_type: "text".to_string(),
                    text: response_text.to_string(),
                }],
                call_id: None,
                name: None,
                arguments: None,
                output: None,
            },
        });

        self.enqueue(ServerEvent::ResponseDone {
            response: ResponseState {
                id: resp_id,
                status: "completed".to_string(),
                status_details: None,
                output: vec![ResponseOutputItem {
                    id: item_id,
                    object: "realtime.item".to_string(),
                    item_type: "message".to_string(),
                    status: "completed".to_string(),
                    role: "assistant".to_string(),
                    content: vec![OutputContent {
                        content_type: "text".to_string(),
                        transcript: String::new(),
                        text: response_text.to_string(),
                        audio: String::new(),
                        call_id: None,
                        name: None,
                        arguments: None,
                        output: None,
                    }],
                    phase: Some("final_answer".to_string()),
                }],
                usage: Some(Usage {
                    total_tokens: 10,
                    input_tokens: 5,
                    output_tokens: 5,
                    input_token_details: None,
                    output_token_details: None,
                }),
            },
        });
    }

    /// Enqueue a function call response.
    pub fn enqueue_function_call(
        &mut self,
        _expected_input: &str,
        call_id: &str,
        function_name: &str,
        arguments: &str,
        response_text: &str,
    ) {
        let item_id = self.next_id("item");
        let resp_id = self.next_id("resp");

        self.enqueue(ServerEvent::ResponseCreated {
            response: ResponseState {
                id: resp_id.clone(),
                status: "in_progress".to_string(),
                status_details: None,
                output: vec![],
                usage: None,
            },
        });

        self.enqueue(ServerEvent::ResponseFunctionCallArgumentsDone {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            call_id: call_id.to_string(),
            name: function_name.to_string(),
            arguments: arguments.to_string(),
        });

        self.enqueue(ServerEvent::ResponseDone {
            response: ResponseState {
                id: resp_id.clone(),
                status: "completed".to_string(),
                status_details: None,
                output: vec![ResponseOutputItem {
                    id: item_id,
                    object: "realtime.item".to_string(),
                    item_type: "function_call".to_string(),
                    status: "completed".to_string(),
                    role: "assistant".to_string(),
                    content: vec![],
                    phase: Some("commentary".to_string()),
                }],
                usage: Some(Usage {
                    total_tokens: 5,
                    input_tokens: 5,
                    output_tokens: 0,
                    input_token_details: None,
                    output_token_details: None,
                }),
            },
        });

        // After function call output, enqueue final response
        self.enqueue_text_response("", response_text);
    }

    /// Enqueue an audio output response.
    pub fn enqueue_audio_response(&mut self, transcript: &str, audio_base64: &str) {
        let item_id = self.next_id("item");
        let resp_id = self.next_id("resp");

        self.enqueue(ServerEvent::ResponseCreated {
            response: ResponseState {
                id: resp_id.clone(),
                status: "in_progress".to_string(),
                status_details: None,
                output: vec![],
                usage: None,
            },
        });

        self.enqueue(ServerEvent::ResponseOutputAudioDelta {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            delta: audio_base64.to_string(),
        });

        self.enqueue(ServerEvent::ResponseOutputAudioDone {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
        });

        self.enqueue(ServerEvent::ResponseOutputAudioTranscriptDone {
            response_id: resp_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            transcript: transcript.to_string(),
        });

        self.enqueue(ServerEvent::ResponseDone {
            response: ResponseState {
                id: resp_id,
                status: "completed".to_string(),
                status_details: None,
                output: vec![ResponseOutputItem {
                    id: item_id,
                    object: "realtime.item".to_string(),
                    item_type: "message".to_string(),
                    status: "completed".to_string(),
                    role: "assistant".to_string(),
                    content: vec![OutputContent {
                        content_type: "audio".to_string(),
                        transcript: transcript.to_string(),
                        text: String::new(),
                        audio: audio_base64.to_string(),
                        call_id: None,
                        name: None,
                        arguments: None,
                        output: None,
                    }],
                    phase: Some("final_answer".to_string()),
                }],
                usage: Some(Usage {
                    total_tokens: 10,
                    input_tokens: 5,
                    output_tokens: 5,
                    input_token_details: None,
                    output_token_details: None,
                }),
            },
        });
    }

    /// Enqueue a session.updated response.
    pub fn enqueue_session_updated(&mut self) {
        let session = SessionState {
            id: self.session_id.clone(),
            model: "fake-realtime".to_string(),
            modalities: vec!["text".to_string(), "audio".to_string()],
            instructions: "You are a fake test model.".to_string(),
            voice: "alloy".to_string(),
            input_audio_format: Some(AudioFormat::String("pcm16".to_string())),
            output_audio_format: Some(AudioFormat::String("pcm16".to_string())),
            input_audio_transcription: None,
            turn_detection: Some(TurnDetection {
                turn_type: "server_vad".to_string(),
                threshold: Some(0.5),
                silence_duration_ms: Some(200),
                prefix_padding_ms: Some(100),
                create_response: Some(true),
                interrupt_response: Some(true),
                idle_timeout_ms: None,
            }),
            tools: vec![],
            tool_choice: "auto".to_string(),
            temperature: 0.8,
            max_response_output_tokens: serde_json::Value::String("inf".to_string()),
            reasoning: None,
            object: "realtime.session".to_string(),
            speed: None,
            tracing: None,
            truncation: None,
            prompt: None,
            expires_at: None,
            client_secret: None,
            include: None,
            input_audio_noise_reduction: None,
        };
        self.enqueue(ServerEvent::SessionUpdated { session });
    }
}

impl Default for FakeTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RealtimeTransport for FakeTransport {
    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn send(&mut self, event: &ClientEvent) -> Result<()> {
        // Process the client event to potentially auto-respond
        match event {
            ClientEvent::SessionUpdate { .. } => {
                // Auto-enqueue session.updated if not already queued
                let has_session_updated = self
                    .event_queue
                    .iter()
                    .any(|e| matches!(e, ServerEvent::SessionUpdated { .. }));
                if !has_session_updated {
                    self.enqueue_session_updated();
                }
            }
            ClientEvent::ConversationItemCreate { item, .. } => {
                self.conversation.push(item.clone());
            }
            ClientEvent::ResponseCreate { .. } => {
                // Response events should already be enqueued by the test setup
            }
            _ => {}
        }
        Ok(())
    }

    async fn recv(&mut self, _timeout: Duration) -> Result<Option<ServerEvent>> {
        // Pop from the pre-configured event queue
        Ok(self.event_queue.pop_front())
    }

    async fn close(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OutputContent;

    #[tokio::test]
    async fn test_fake_transport_basic() {
        let mut transport = FakeTransport::new();
        transport.setup_session("sess_test");
        transport.enqueue_text_response("Hello", "Hi there!");

        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        // Receive session.created
        let event = transport.recv(Duration::from_secs(1)).await.unwrap().unwrap();
        match event {
            ServerEvent::SessionCreated { session } => {
                assert_eq!(session.id, "sess_test");
            }
            _ => panic!("Expected SessionCreated"),
        }

        // Send text and trigger response
        let item = ConversationItem {
            id: String::new(),
            item_type: "message".to_string(),
            status: String::new(),
            role: "user".to_string(),
            content: vec![ContentPart::InputText {
                content_type: "input_text".to_string(),
                text: "Hello".to_string(),
            }],
            call_id: None,
            name: None,
            arguments: None,
            output: None,
        };
        transport
            .send(&ClientEvent::ConversationItemCreate {
                item,
                previous_item_id: None,
                event_id: None,
            })
            .await
            .unwrap();
        transport
            .send(&ClientEvent::ResponseCreate {
                response: None,
                event_id: None,
            })
            .await
            .unwrap();

        // Consume events until response.done
        let mut response_text = String::new();
        loop {
            let event = transport.recv(Duration::from_secs(1)).await.unwrap();
            match event {
                Some(ServerEvent::ResponseOutputTextDelta { delta, .. }) => {
                    response_text.push_str(&delta);
                }
                Some(ServerEvent::ResponseDone { response }) => {
                    assert!(response.status == "completed");
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }

        assert_eq!(response_text, "Hi there!");
        transport.close().await.unwrap();
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_fake_transport_session_update() {
        let mut transport = FakeTransport::new();
        transport.setup_session("sess_test");

        transport.connect().await.unwrap();

        // Receive session.created
        transport.recv(Duration::from_secs(1)).await.unwrap();

        // Send session.update
        transport
            .send(&ClientEvent::SessionUpdate {
                session: SessionConfig {
                    instructions: Some("Be helpful.".into()),
                    ..Default::default()
                },
                event_id: None,
            })
            .await
            .unwrap();

        // Should auto-get session.updated
        let event = transport.recv(Duration::from_secs(1)).await.unwrap().unwrap();
        match event {
            ServerEvent::SessionUpdated { .. } => {}
            _ => panic!("Expected SessionUpdated"),
        }

        transport.close().await.unwrap();
    }
}
