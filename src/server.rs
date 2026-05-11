use anyhow::{Context, Result};
use crate::audio;
use crate::pipeline::{FakeLlm, FakeStt, FakeTts, FakeTurnDetector};
use crate::protocol::{
    AudioFormat, ClientEvent, ContentPart, ConversationItem, OutputContent,
    ResponseOutputItem, ResponseState, ServerEvent, SessionConfig, SessionState, Tool,
    TurnDetection, Usage,
};
use crate::traits::{LanguageModel, SpeechToText, TextToSpeech, TurnDetector};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Manages a single Realtime API session over a WebSocket connection.
pub struct SessionHandler {
    /// Outgoing server events are sent through this channel.
    tx: tokio::sync::mpsc::UnboundedSender<ServerEvent>,
    /// Incoming client events are received through this channel.
    rx: tokio::sync::mpsc::UnboundedReceiver<ClientEvent>,
    /// Session state.
    session: SessionState,
    /// Conversation items.
    conversation: Vec<ConversationItem>,
    /// Registered tools.
    tools: Vec<Tool>,
    /// Current response ID counter.
    counter: u64,
    /// Whether a response is currently being generated.
    generating: bool,
    /// Cancel token for in-progress generation.
    cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl SessionHandler {
    pub fn new(
        tx: tokio::sync::mpsc::UnboundedSender<ServerEvent>,
        rx: tokio::sync::mpsc::UnboundedReceiver<ClientEvent>,
    ) -> Self {
        let session_id = format!("sess_{}", uuid_simple());
        let session = SessionState {
            id: session_id,
            model: "fake-realtime".to_string(),
            modalities: vec!["text".to_string(), "audio".to_string()],
            instructions: "You are a helpful assistant running on a local test server.".to_string(),
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

        Self {
            tx,
            rx,
            session,
            conversation: Vec::new(),
            tools: Vec::new(),
            counter: 0,
            generating: false,
            cancel_tx: None,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{}_{}", prefix, self.counter)
    }

    fn send(&self, event: ServerEvent) {
        self.tx.send(event).ok();
    }

    /// Run the session loop. Blocks until the client disconnects.
    pub async fn run(mut self) {
        // Send session.created immediately
        self.send(ServerEvent::SessionCreated {
            session: self.session.clone(),
        });

        // Process incoming client events
        while let Some(event) = self.rx.recv().await {
            self.handle_client_event(event).await;
        }
    }

    async fn handle_client_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::SessionUpdate { session: config, .. } => {
                self.apply_session_config(config);
                self.send(ServerEvent::SessionUpdated {
                    session: self.session.clone(),
                });
            }
            ClientEvent::ConversationItemCreate {
                item,
                previous_item_id,
                ..
            } => {
                let mut item = item;
                if item.id.is_empty() {
                    item.id = self.next_id("item");
                }
                self.conversation.push(item.clone());
                self.send(ServerEvent::ConversationItemAdded { item: item.clone() });
                self.send(ServerEvent::ConversationItemDone { item });
            }
            ClientEvent::ConversationItemDelete { item_id, .. } => {
                self.conversation.retain(|i| i.id != item_id);
            }
            ClientEvent::ConversationItemTruncate { .. } => {
                // Truncation not fully implemented in this fake server
            }
            ClientEvent::ResponseCreate { response, .. } => {
                self.handle_response_create(response).await;
            }
            ClientEvent::ResponseCancel {
                response_id,
                sample_count,
                ..
            } => {
                if let Some(cancel_tx) = self.cancel_tx.take() {
                    let _ = cancel_tx.send(());
                }
                self.generating = false;
            }
            ClientEvent::InputAudioBufferAppend { .. } => {
                // Audio buffered; handled when committed
            }
            ClientEvent::InputAudioBufferCommit { .. } => {
                // Would trigger response if VAD is enabled
            }
            ClientEvent::InputAudioBufferClear { .. } => {
                // Buffer cleared
            }
        }
    }

    fn apply_session_config(&mut self, config: SessionConfig) {
        if let Some(model) = config.model {
            self.session.model = model;
        }
        if let Some(modalities) = config.modalities {
            self.session.modalities = modalities;
        }
        if let Some(instructions) = config.instructions {
            self.session.instructions = instructions;
        }
        if let Some(voice) = config.voice {
            self.session.voice = voice;
        }
        if let Some(temp) = config.temperature {
            self.session.temperature = temp;
        }
        if let Some(tools) = config.tools {
            self.session.tools = tools.clone();
            self.tools = tools;
        }
        if let Some(turn_detection) = config.turn_detection {
            self.session.turn_detection = Some(turn_detection);
        }
        if let Some(max_tokens) = config.max_response_output_tokens {
            self.session.max_response_output_tokens =
                serde_json::Value::Number(serde_json::Number::from(max_tokens));
        }
    }

    async fn handle_response_create(
        &mut self,
        _response_config: Option<crate::protocol::ResponseConfig>,
    ) {
        self.generating = true;

        // Get the last user message from conversation
        let user_text = self
            .conversation
            .iter()
            .rev()
            .find(|i| i.role == "user")
            .and_then(|i| {
                i.content.iter().find_map(|c| match c {
                    ContentPart::InputText { text, .. } => Some(text.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();

        // Set up cancel channel
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        self.cancel_tx = Some(cancel_tx);

        let response_id = self.next_id("resp");
        let item_id = self.next_id("item");
        let modalities = self.session.modalities.clone();
        let tools = self.tools.clone();

        // Send response.created
        self.send(ServerEvent::ResponseCreated {
            response: ResponseState {
                id: response_id.clone(),
                status: "in_progress".to_string(),
                status_details: None,
                output: vec![],
                usage: None,
            },
        });

        self.send(ServerEvent::ResponseOutputItemAdded {
            response_id: response_id.clone(),
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

        // Generate response using the fake LLM
        let mut llm = FakeLlm::new();
        // Add some basic response patterns
        llm.add_response("hello", "Hello! I'm a local test server. How can I help you today?");
        llm.add_response("what is 2+2", "4");
        llm.add_response("what is 1+1", "2");
        llm.add_response("what is 5 times 7", "35");
        llm.add_response("my name is alice", "Nice to meet you, Alice!");
        llm.add_response("my name is alice. remember that.", "Got it, Alice! I'll remember that.");
        llm.add_response("what is my name", "Your name is Alice.");
        llm.add_response("say hello", "Hello!");
        llm.add_response("say hello world", "Hello world!");
        llm.add_response("say hello world.", "Hello world!");
        llm.add_response("count from 1 to 100.", "1, 2, 3, 4, 5, ... 100");
        llm.add_response("count from 1 to 50.", "1, 2, 3, ... 50");
        llm.add_response("count from 1 to 50. iteration 0.", "1, 2, 3, ... 50");
        llm.add_response("count from 1 to 50. iteration 1.", "1, 2, 3, ... 50");
        llm.add_response("hi", "Hello!");
        llm.set_default_response("I received your message. This is the local test server.");

        // Check for function calling
        if !tools.is_empty() && self.should_call_tool(&user_text) {
            self.handle_tool_call(&user_text, &tools, &response_id, &item_id)
                .await;
            return;
        }

        let response_text = llm.generate(&user_text).await.unwrap_or_default();

        // Check for cancellation
        let cancelled = cancel_rx.try_recv().is_ok();
        if cancelled {
            self.send(ServerEvent::ResponseDone {
                response: ResponseState {
                    id: response_id,
                    status: "cancelled".to_string(),
                    status_details: None,
                    output: vec![],
                    usage: None,
                },
            });
            self.generating = false;
            return;
        }

        let text_content_type = if modalities.contains(&"text".to_string()) {
            "text"
        } else {
            "audio"
        };

        // Content part added
        self.send(ServerEvent::ResponseContentPartAdded {
            response_id: response_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            part: Some(ContentPart::OutputText {
                content_type: text_content_type.to_string(),
                text: String::new(),
            }),
        });

        // Handle audio output if audio modality is enabled
        if modalities.contains(&"audio".to_string()) {
            // Generate audio from text
            let mut tts = FakeTts::new();
            let audio_data = tts.synthesize(&response_text).await.unwrap_or_default();
            let audio_b64 = audio::pcm_to_base64(&audio_data);

            self.send(ServerEvent::ResponseOutputAudioDelta {
                response_id: response_id.clone(),
                item_id: item_id.clone(),
                output_index: 0,
                content_index: 0,
                delta: audio_b64.clone(),
            });

            self.send(ServerEvent::ResponseOutputAudioDone {
                response_id: response_id.clone(),
                item_id: item_id.clone(),
                output_index: 0,
                content_index: 0,
            });

            self.send(ServerEvent::ResponseOutputAudioTranscriptDone {
                response_id: response_id.clone(),
                item_id: item_id.clone(),
                output_index: 0,
                content_index: 0,
                transcript: response_text.clone(),
            });
        }

        // Text deltas (if text modality)
        if modalities.contains(&"text".to_string()) {
            // Simulate streaming by sending words
            let words: Vec<&str> = response_text.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                let token = if i < words.len() - 1 {
                    format!("{} ", word)
                } else {
                    word.to_string()
                };
                self.send(ServerEvent::ResponseOutputTextDelta {
                    response_id: response_id.clone(),
                    item_id: item_id.clone(),
                    output_index: 0,
                    content_index: 0,
                    delta: token,
                });
            }

            self.send(ServerEvent::ResponseOutputTextDone {
                response_id: response_id.clone(),
                item_id: item_id.clone(),
                output_index: 0,
                content_index: 0,
                text: response_text.clone(),
            });
        }

        self.send(ServerEvent::ResponseContentPartDone {
            response_id: response_id.clone(),
            item_id: item_id.clone(),
            output_index: 0,
            content_index: 0,
            part: Some(ContentPart::OutputText {
                content_type: text_content_type.to_string(),
                text: response_text.clone(),
            }),
        });

        self.send(ServerEvent::ResponseOutputItemDone {
            response_id: response_id.clone(),
            output_index: 0,
            item: ConversationItem {
                id: item_id.clone(),
                item_type: "message".to_string(),
                status: "completed".to_string(),
                role: "assistant".to_string(),
                content: vec![ContentPart::OutputText {
                    content_type: text_content_type.to_string(),
                    text: response_text.clone(),
                }],
                call_id: None,
                name: None,
                arguments: None,
                output: None,
            },
        });

        let output_item = ResponseOutputItem {
            id: item_id.clone(),
            object: "realtime.item".to_string(),
            item_type: "message".to_string(),
            status: "completed".to_string(),
            role: "assistant".to_string(),
            content: vec![OutputContent {
                content_type: text_content_type.to_string(),
                transcript: response_text.clone(),
                text: response_text.clone(),
                audio: String::new(),
                call_id: None,
                name: None,
                arguments: None,
                output: None,
            }],
            phase: Some("final_answer".to_string()),
        };

        self.send(ServerEvent::ResponseDone {
            response: ResponseState {
                id: response_id,
                status: "completed".to_string(),
                status_details: None,
                output: vec![output_item],
                usage: Some(Usage {
                    total_tokens: (user_text.len() + response_text.len()) as u64,
                    input_tokens: user_text.len() as u64,
                    output_tokens: response_text.len() as u64,
                    input_token_details: None,
                    output_token_details: None,
                }),
            },
        });

        self.generating = false;
        self.cancel_tx = None;
    }

    fn should_call_tool(&self, user_text: &str) -> bool {
        let text = user_text.to_lowercase();
        // Simple heuristic: if tools are registered and text mentions tool-related keywords
        self.tools.iter().any(|tool| {
            let name = tool.name.to_lowercase();
            text.contains(&name)
                || text.contains("weather")
                || text.contains("time")
                || text.contains("tool")
                || text.contains("function")
        })
    }

    async fn handle_tool_call(
        &mut self,
        user_text: &str,
        tools: &[Tool],
        response_id: &str,
        item_id: &str,
    ) {
        let tool = &tools[0]; // Pick first matching tool
        let call_id = self.next_id("call");

        // Simulate function call arguments
        let arguments = if tool.name.contains("weather") {
            r#"{"location": "Paris"}"#.to_string()
        } else if tool.name.contains("time") {
            r#"{"location": "UTC"}"#.to_string()
        } else {
            r#"{}"#.to_string()
        };

        // Send function call arguments delta
        self.send(ServerEvent::ResponseFunctionCallArgumentsDelta {
            response_id: response_id.to_string(),
            item_id: item_id.to_string(),
            output_index: 0,
            call_id: call_id.clone(),
            delta: arguments.clone(),
        });

        // Send function call arguments done
        self.send(ServerEvent::ResponseFunctionCallArgumentsDone {
            response_id: response_id.to_string(),
            item_id: item_id.to_string(),
            output_index: 0,
            call_id: call_id.clone(),
            name: tool.name.clone(),
            arguments,
        });

        // Signal response done (tool call phase)
        self.send(ServerEvent::ResponseDone {
            response: ResponseState {
                id: response_id.to_string(),
                status: "completed".to_string(),
                status_details: None,
                output: vec![ResponseOutputItem {
                    id: item_id.to_string(),
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

        self.generating = false;
        self.cancel_tx = None;
    }
}

/// Start the local WebSocket server on the given address.
pub async fn run_server(addr: &str) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("Local Realtime API server listening on {}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        println!("New connection from {}", peer_addr);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Connection error from {}: {}", peer_addr, e);
            }
            println!("Connection closed: {}", peer_addr);
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .context("WebSocket handshake failed")?;

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Channels for communicating between the WebSocket task and the session handler
    let (event_tx, event_rx) = mpsc::unbounded_channel::<ClientEvent>();
    let (server_tx, mut server_rx) = mpsc::unbounded_channel::<ServerEvent>();
    let error_tx = server_tx.clone(); // Clone for error reporting

    // Spawn the session handler
    let handler = SessionHandler::new(server_tx, event_rx);
    let handler_handle = tokio::spawn(async move {
        handler.run().await;
    });

    // Spawn the server → client writer
    let writer_handle = tokio::spawn(async move {
        while let Some(event) = server_rx.recv().await {
            let json = serde_json::to_string(&event).unwrap();
            if ws_tx
                .send(tokio_tungstenite::tungstenite::Message::Text(json.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Read client messages and forward to session handler
    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                match serde_json::from_str::<ClientEvent>(&text) {
                    Ok(event) => {
                        if event_tx.send(event).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to parse client event: {} — raw: {}", e, &text[..text.len().min(200)]);
                        // Send error back
                        if error_tx
                            .send(ServerEvent::Error {
                                error: crate::protocol::ErrorDetail {
                                    error_type: "invalid_request_error".to_string(),
                                    code: None,
                                    message: format!("Failed to parse event: {}", e),
                                    param: None,
                                    event_id: None,
                                },
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    drop(event_tx);
    writer_handle.abort();
    handler_handle.abort();

    Ok(())
}

/// Generate a simple UUID-like string for session/item IDs.
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("{:08x}{:04x}", nanos, (nanos >> 16) & 0xFFFF)
}
