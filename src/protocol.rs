use serde::{Deserialize, Serialize};

// ============================================================
// Client Events (client -> server)
// ============================================================

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ClientEvent {
    #[serde(rename = "session.update")]
    SessionUpdate {
        session: SessionConfig,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "conversation.item.create")]
    ConversationItemCreate {
        item: ConversationItem,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "conversation.item.delete")]
    ConversationItemDelete {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "conversation.item.truncate")]
    ConversationItemTruncate {
        item_id: String,
        content_index: u32,
        audio_end_ms: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "response.create")]
    ResponseCreate {
        #[serde(skip_serializing_if = "Option::is_none")]
        response: Option<ResponseConfig>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "response.cancel")]
    ResponseCancel {
        #[serde(skip_serializing_if = "Option::is_none")]
        response_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sample_count: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "input_audio_buffer.append")]
    InputAudioBufferAppend {
        audio: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "input_audio_buffer.commit")]
    InputAudioBufferCommit {
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
    #[serde(rename = "input_audio_buffer.clear")]
    InputAudioBufferClear {
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
    },
}

// ============================================================
// Server Events (server -> client)
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "session.created")]
    SessionCreated { session: SessionState },

    #[serde(rename = "session.updated")]
    SessionUpdated { session: SessionState },

    #[serde(rename = "conversation.item.added")]
    ConversationItemAdded { item: ConversationItem },

    #[serde(rename = "conversation.item.done")]
    ConversationItemDone { item: ConversationItem },

    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    ConversationItemInputAudioTranscriptionCompleted {
        item_id: String,
        content_index: u32,
        transcript: String,
    },

    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    ConversationItemInputAudioTranscriptionFailed {
        item_id: String,
        content_index: u32,
        error: serde_json::Value,
    },

    #[serde(rename = "input_audio_buffer.speech_started")]
    InputAudioBufferSpeechStarted {
        #[serde(default)]
        item_id: Option<String>,
        audio_start_ms: Option<u32>,
    },

    #[serde(rename = "input_audio_buffer.speech_stopped")]
    InputAudioBufferSpeechStopped { audio_end_ms: u32, item_id: Option<String> },

    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioBufferCommitted {
        item_id: String,
        #[serde(default)]
        previous_item_id: Option<String>,
    },

    #[serde(rename = "response.created")]
    ResponseCreated { response: ResponseState },

    #[serde(rename = "response.done")]
    ResponseDone { response: ResponseState },

    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded {
        response_id: String,
        item: ConversationItem,
        output_index: u32,
    },

    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone {
        response_id: String,
        item: ConversationItem,
        output_index: u32,
    },

    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        #[serde(default)]
        part: Option<ContentPart>,
    },

    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        #[serde(default)]
        part: Option<ContentPart>,
    },

    #[serde(rename = "response.output_audio.delta")]
    ResponseOutputAudioDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
    },

    #[serde(rename = "response.output_audio.done")]
    ResponseOutputAudioDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
    },

    #[serde(rename = "response.output_audio_transcript.delta")]
    ResponseOutputAudioTranscriptDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
    },

    #[serde(rename = "response.output_audio_transcript.done")]
    ResponseOutputAudioTranscriptDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        transcript: String,
    },

    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
    },

    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
    },

    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        call_id: String,
        delta: String,
    },

    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        call_id: String,
        name: String,
        arguments: String,
    },

    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated { rate_limits: Vec<RateLimit> },

    #[serde(rename = "error")]
    Error { error: ErrorDetail },

    #[serde(other)]
    Unknown,
}

// ============================================================
// Shared types
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(rename = "input_audio_format", skip_serializing_if = "Option::is_none")]
    pub input_audio_format: Option<AudioFormat>,
    #[serde(rename = "output_audio_format", skip_serializing_if = "Option::is_none")]
    pub output_audio_format: Option<AudioFormat>,
    #[serde(rename = "input_audio_transcription", skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<InputAudioTranscription>,
    #[serde(rename = "turn_detection", skip_serializing_if = "Option::is_none")]
    pub turn_detection: Option<TurnDetection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(rename = "tool_choice", skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(rename = "max_response_output_tokens", skip_serializing_if = "Option::is_none")]
    pub max_response_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub model: String,
    #[serde(default)]
    pub modalities: Vec<String>,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub voice: String,
    #[serde(rename = "input_audio_format", default)]
    pub input_audio_format: Option<AudioFormat>,
    #[serde(rename = "output_audio_format", default)]
    pub output_audio_format: Option<AudioFormat>,
    #[serde(rename = "input_audio_transcription", default)]
    pub input_audio_transcription: Option<InputAudioTranscription>,
    #[serde(rename = "turn_detection", default)]
    pub turn_detection: Option<TurnDetection>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(rename = "tool_choice", default)]
    pub tool_choice: String,
    #[serde(default)]
    pub temperature: f32,
    #[serde(rename = "max_response_output_tokens", default)]
    pub max_response_output_tokens: serde_json::Value,
    #[serde(default)]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default)]
    pub object: String,
    #[serde(default)]
    pub speed: Option<f32>,
    #[serde(default)]
    pub tracing: Option<serde_json::Value>,
    #[serde(default)]
    pub truncation: Option<String>,
    #[serde(default)]
    pub prompt: Option<serde_json::Value>,
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub client_secret: Option<serde_json::Value>,
    #[serde(default)]
    pub include: Option<serde_json::Value>,
    #[serde(rename = "input_audio_noise_reduction", default)]
    pub input_audio_noise_reduction: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudioFormat {
    String(String),
    Object {
        #[serde(rename = "type")]
        format_type: String,
        #[serde(default)]
        rate: u32,
    },
}

impl Default for AudioFormat {
    fn default() -> Self {
        AudioFormat::Object {
            format_type: "audio/pcm".into(),
            rate: 24000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputAudioTranscription {
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TurnDetection {
    #[serde(rename = "type")]
    pub turn_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    #[serde(rename = "silence_duration_ms", skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<u32>,
    #[serde(rename = "prefix_padding_ms", skip_serializing_if = "Option::is_none")]
    pub prefix_padding_ms: Option<u32>,
    #[serde(rename = "create_response", skip_serializing_if = "Option::is_none")]
    pub create_response: Option<bool>,
    #[serde(rename = "interrupt_response", skip_serializing_if = "Option::is_none")]
    pub interrupt_response: Option<bool>,
    #[serde(rename = "idle_timeout_ms", skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(rename = "generate_summary", skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(rename = "output_audio_format", skip_serializing_if = "Option::is_none")]
    pub output_audio_format: Option<AudioFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(rename = "tool_choice", skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(rename = "max_response_output_tokens", skip_serializing_if = "Option::is_none")]
    pub max_response_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseState {
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(rename = "status_details", default)]
    pub status_details: Option<serde_json::Value>,
    #[serde(default)]
    pub output: Vec<ResponseOutputItem>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseOutputItem {
    pub id: String,
    #[serde(default)]
    pub object: String,
    #[serde(rename = "type", default)]
    pub item_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Vec<OutputContent>,
    #[serde(default)]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputContent {
    #[serde(rename = "type", default)]
    pub content_type: String,
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub audio: String,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationItem {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(rename = "type", default)]
    pub item_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ContentPart>,
    #[serde(rename = "call_id", default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentPart {
    InputText {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
    },
    InputAudio {
        #[serde(rename = "type")]
        content_type: String,
        audio: String,
    },
    OutputText {
        #[serde(rename = "type")]
        content_type: String,
        text: String,
    },
    OutputAudio {
        #[serde(rename = "type")]
        content_type: String,
        audio: String,
        transcript: Option<String>,
    },
    Other(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    #[serde(rename = "total_tokens", default)]
    pub total_tokens: u64,
    #[serde(rename = "input_tokens", default)]
    pub input_tokens: u64,
    #[serde(rename = "output_tokens", default)]
    pub output_tokens: u64,
    #[serde(rename = "input_token_details", default)]
    pub input_token_details: Option<serde_json::Value>,
    #[serde(rename = "output_token_details", default)]
    pub output_token_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub name: String,
    pub limit: u64,
    pub remaining: u64,
    pub reset_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: Option<String>,
    pub message: String,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(rename = "event_id", default)]
    pub event_id: Option<String>,
}

impl ClientEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            ClientEvent::SessionUpdate { .. } => "session.update",
            ClientEvent::ConversationItemCreate { .. } => "conversation.item.create",
            ClientEvent::ConversationItemDelete { .. } => "conversation.item.delete",
            ClientEvent::ConversationItemTruncate { .. } => "conversation.item.truncate",
            ClientEvent::ResponseCreate { .. } => "response.create",
            ClientEvent::ResponseCancel { .. } => "response.cancel",
            ClientEvent::InputAudioBufferAppend { .. } => "input_audio_buffer.append",
            ClientEvent::InputAudioBufferCommit { .. } => "input_audio_buffer.commit",
            ClientEvent::InputAudioBufferClear { .. } => "input_audio_buffer.clear",
        }
    }
}

impl ServerEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            ServerEvent::SessionCreated { .. } => "session.created",
            ServerEvent::SessionUpdated { .. } => "session.updated",
            ServerEvent::ConversationItemAdded { .. } => "conversation.item.added",
            ServerEvent::ConversationItemDone { .. } => "conversation.item.done",
            ServerEvent::ConversationItemInputAudioTranscriptionCompleted { .. } => {
                "conversation.item.input_audio_transcription.completed"
            }
            ServerEvent::ConversationItemInputAudioTranscriptionFailed { .. } => {
                "conversation.item.input_audio_transcription.failed"
            }
            ServerEvent::InputAudioBufferSpeechStarted { .. } => {
                "input_audio_buffer.speech_started"
            }
            ServerEvent::InputAudioBufferSpeechStopped { .. } => {
                "input_audio_buffer.speech_stopped"
            }
            ServerEvent::InputAudioBufferCommitted { .. } => "input_audio_buffer.committed",
            ServerEvent::ResponseCreated { .. } => "response.created",
            ServerEvent::ResponseDone { .. } => "response.done",
            ServerEvent::ResponseOutputItemAdded { .. } => "response.output_item.added",
            ServerEvent::ResponseOutputItemDone { .. } => "response.output_item.done",
            ServerEvent::ResponseContentPartAdded { .. } => "response.content_part.added",
            ServerEvent::ResponseContentPartDone { .. } => "response.content_part.done",
            ServerEvent::ResponseOutputAudioDelta { .. } => "response.output_audio.delta",
            ServerEvent::ResponseOutputAudioDone { .. } => "response.output_audio.done",
            ServerEvent::ResponseOutputAudioTranscriptDelta { .. } => {
                "response.output_audio_transcript.delta"
            }
            ServerEvent::ResponseOutputAudioTranscriptDone { .. } => {
                "response.output_audio_transcript.done"
            }
            ServerEvent::ResponseOutputTextDelta { .. } => "response.output_text.delta",
            ServerEvent::ResponseOutputTextDone { .. } => "response.output_text.done",
            ServerEvent::ResponseFunctionCallArgumentsDelta { .. } => {
                "response.function_call_arguments.delta"
            }
            ServerEvent::ResponseFunctionCallArgumentsDone { .. } => {
                "response.function_call_arguments.done"
            }
            ServerEvent::RateLimitsUpdated { .. } => "rate_limits.updated",
            ServerEvent::Error { .. } => "error",
            ServerEvent::Unknown => "unknown",
        }
    }
}
