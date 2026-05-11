use async_trait::async_trait;
use crate::protocol::{ClientEvent, ServerEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Core transport abstraction for a realtime API connection.
/// Implementations can be real (OpenAI WebSocket) or fake (in-memory channels).
#[async_trait]
pub trait RealtimeTransport: Send {
    /// Establish the connection. Must be called before send/recv.
    async fn connect(&mut self) -> anyhow::Result<()>;

    /// Send a client event over the transport.
    async fn send(&mut self, event: &ClientEvent) -> anyhow::Result<()>;

    /// Receive the next server event with an optional timeout.
    /// Returns None on timeout, Some(Err) on transport error, Some(Ok) on success.
    async fn recv(&mut self, timeout: Duration) -> anyhow::Result<Option<ServerEvent>>;

    /// Close the transport connection.
    async fn close(&mut self) -> anyhow::Result<()>;

    /// Returns true if the transport is connected.
    fn is_connected(&self) -> bool;
}

// ============================================================
// Speech-to-Text trait
// ============================================================

/// Converts raw audio bytes (PCM16, 24kHz) into transcribed text.
#[async_trait]
pub trait SpeechToText: Send {
    /// Transcribe audio and return text. Called with chunks of audio data.
    async fn transcribe(&mut self, audio: &[u8]) -> anyhow::Result<Option<String>>;

    /// Signal end of audio stream for final transcription.
    async fn end_audio(&mut self) -> anyhow::Result<Option<String>>;

    /// Reset transcription state.
    fn reset(&mut self);
}

// ============================================================
// Language Model trait
// ============================================================

/// Generates text responses from text input.
///
/// Supports both batch generation (full response) and streaming generation
/// where tokens are sent through a channel as they become available.
#[async_trait]
pub trait LanguageModel: Send {
    /// Generate a full response for the given input text.
    async fn generate(&mut self, input: &str) -> anyhow::Result<String>;

    /// Generate a response token-by-token, sending each token through the sender.
    /// The sender is dropped when generation is complete, signaling the receiver.
    /// Returns the full concatenated response text.
    async fn generate_stream(
        &mut self,
        input: &str,
        sender: mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<()>;
}

// ============================================================
// Text-to-Speech trait
// ============================================================

/// Converts text into audio bytes (PCM16, 24kHz).
///
/// Supports both batch synthesis (return full audio) and streaming synthesis
/// where text chunks arrive via a receiver and audio chunks are sent through a sender.
#[async_trait]
pub trait TextToSpeech: Send {
    /// Synthesize speech from a full text. Returns PCM16 audio bytes at 24kHz.
    async fn synthesize(&mut self, text: &str) -> anyhow::Result<Vec<u8>>;

    /// Stream synthesis: reads text chunks from the receiver, synthesizes each,
    /// and sends audio chunks through the sender. Returns when the receiver is closed.
    async fn synthesize_stream(
        &mut self,
        receiver: &mut mpsc::UnboundedReceiver<String>,
        sender: mpsc::UnboundedSender<Vec<u8>>,
    ) -> anyhow::Result<()>;

    /// Flush any buffered audio (for streaming TTS that accumulates context).
    async fn flush(&mut self) -> anyhow::Result<Vec<u8>>;
}

// ============================================================
// Turn Detection trait (pipecat-inspired)
// ============================================================

/// Represents a turn event detected from audio.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    /// User started speaking.
    SpeechStarted { audio_start_ms: u32 },
    /// User stopped speaking (endpoint detected).
    SpeechStopped { audio_end_ms: u32 },
    /// User interrupted the model's response.
    Interrupted,
    /// No speech detected in the audio chunk.
    Silence,
}

/// Detects turn boundaries in streaming audio using VAD + endpoint detection.
/// Inspired by pipecat's turn-taking model.
#[async_trait]
pub trait TurnDetector: Send {
    /// Process an audio chunk and return any turn events.
    async fn process_audio(&mut self, audio: &[u8]) -> anyhow::Result<Vec<TurnEvent>>;

    /// Signal that the model has started speaking (for interruption detection).
    fn model_started_speaking(&mut self);

    /// Signal that the model has stopped speaking.
    fn model_stopped_speaking(&mut self);

    /// Reset the detector state for a new turn.
    fn reset(&mut self);
}

// ============================================================
// Realtime Pipeline — orchestrates STT → LLM → TTS
// ============================================================

/// High-level pipeline that orchestrates the full realtime audio chain.
pub struct RealtimePipeline<STT, LLM, TTS, TD>
where
    STT: SpeechToText,
    LLM: LanguageModel,
    TTS: TextToSpeech,
    TD: TurnDetector,
{
    pub stt: STT,
    pub llm: LLM,
    pub tts: TTS,
    pub turn_detector: TD,
}

impl<STT, LLM, TTS, TD> RealtimePipeline<STT, LLM, TTS, TD>
where
    STT: SpeechToText,
    LLM: LanguageModel,
    TTS: TextToSpeech,
    TD: TurnDetector,
{
    pub fn new(stt: STT, llm: LLM, tts: TTS, turn_detector: TD) -> Self {
        Self {
            stt,
            llm,
            tts,
            turn_detector,
        }
    }

    /// Process an incoming audio chunk through turn detection.
    pub async fn process_audio(
        &mut self,
        audio: &[u8],
    ) -> anyhow::Result<Vec<TurnEvent>> {
        self.turn_detector.process_audio(audio).await
    }

    /// Handle a completed user turn: transcribe → generate → synthesize (batch).
    pub async fn handle_turn(&mut self) -> anyhow::Result<(String, Vec<u8>)> {
        let transcript = self.stt.end_audio().await?.unwrap_or_default();
        let response = self.llm.generate(&transcript).await?;
        let audio = self.tts.synthesize(&response).await?;
        self.stt.reset();
        Ok((response, audio))
    }

    /// Handle a completed user turn with streaming LLM → TTS.
    ///
    /// LLM tokens flow to TTS as they are generated. The pipeline first runs the
    /// LLM to collect all tokens into a channel buffer, then runs TTS to consume
    /// them. In a production implementation, these would run concurrently using
    /// separate tasks with Arc-wrapped components.
    ///
    /// Each audio chunk is passed to the callback as soon as it's synthesized.
    /// Returns the full concatenated response text.
    pub async fn handle_turn_streaming(
        &mut self,
        on_audio_chunk: &mut (dyn FnMut(Vec<u8>) + Send),
    ) -> anyhow::Result<String> {
        let transcript = self.stt.end_audio().await?.unwrap_or_default();

        // Channel: LLM → TTS (text tokens)
        let (llm_tx, mut tts_rx) = mpsc::unbounded_channel::<String>();

        // Phase 1: Generate LLM tokens into the channel
        {
            let llm_ref = &mut self.llm;
            llm_ref.generate_stream(&transcript, llm_tx.clone()).await?;
        }
        drop(llm_tx); // Close the sender so TTS knows when to stop

        // Phase 2: Synthesize each token through TTS
        let mut full_text = String::new();
        {
            let tts_ref = &mut self.tts;
            while let Some(token) = tts_rx.recv().await {
                full_text.push_str(&token);
                let audio = tts_ref.synthesize(&token).await?;
                if !audio.is_empty() {
                    on_audio_chunk(audio);
                }
            }
            // Flush any remaining TTS buffer
            let audio = tts_ref.flush().await?;
            if !audio.is_empty() {
                on_audio_chunk(audio);
            }
        }

        self.stt.reset();
        Ok(full_text)
    }

    /// Signal that the model has started speaking (enables interruption detection).
    pub fn model_speaking(&mut self) {
        self.turn_detector.model_started_speaking();
    }

    /// Signal that the model has finished speaking.
    pub fn model_done(&mut self) {
        self.turn_detector.model_stopped_speaking();
        self.stt.reset();
    }
}
