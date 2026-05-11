use async_trait::async_trait;
use crate::traits::{LanguageModel, SpeechToText, TextToSpeech};

// ============================================================
// Fake STT — returns pre-configured transcriptions
// ============================================================

pub struct FakeStt {
    transcriptions: Vec<String>,
    current_index: usize,
}

impl FakeStt {
    pub fn new() -> Self {
        Self {
            transcriptions: Vec::new(),
            current_index: 0,
        }
    }

    /// Set the transcriptions that will be returned in sequence.
    pub fn set_transcriptions(&mut self, transcriptions: Vec<String>) {
        self.transcriptions = transcriptions;
        self.current_index = 0;
    }
}

impl Default for FakeStt {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SpeechToText for FakeStt {
    async fn transcribe(&mut self, _audio: &[u8]) -> anyhow::Result<Option<String>> {
        // Streaming mode: return None during speech, full text at end
        Ok(None)
    }

    async fn end_audio(&mut self) -> anyhow::Result<Option<String>> {
        if self.current_index < self.transcriptions.len() {
            let text = self.transcriptions[self.current_index].clone();
            self.current_index += 1;
            Ok(Some(text))
        } else {
            Ok(Some("I didn't catch that.".to_string()))
        }
    }

    fn reset(&mut self) {
        // Keep the current transcriptions
    }
}

// ============================================================
// Fake LLM — returns pre-configured responses
// ============================================================

pub struct FakeLlm {
    responses: std::collections::HashMap<String, String>,
    default_response: String,
}

impl FakeLlm {
    pub fn new() -> Self {
        Self {
            responses: std::collections::HashMap::new(),
            default_response: "I'm a fake model response.".to_string(),
        }
    }

    /// Map an input text to a specific response.
    pub fn add_response(&mut self, input: &str, response: &str) {
        self.responses
            .insert(input.to_lowercase(), response.to_string());
    }

    /// Set the default response when no match is found.
    pub fn set_default_response(&mut self, response: &str) {
        self.default_response = response.to_string();
    }
}

impl Default for FakeLlm {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LanguageModel for FakeLlm {
    async fn generate(&mut self, input: &str) -> anyhow::Result<String> {
        let key = input.to_lowercase();
        // Exact match first
        if let Some(response) = self.responses.get(&key) {
            return Ok(response.clone());
        }
        // Substring match as fallback
        for (pattern, response) in &self.responses {
            if key.contains(pattern.as_str()) || pattern.contains(key.as_str()) {
                return Ok(response.clone());
            }
        }
        Ok(self.default_response.clone())
    }

    async fn generate_stream(
        &mut self,
        input: &str,
        sender: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> anyhow::Result<()> {
        let response = self.generate(input).await?;
        // Stream word-by-word to simulate token-level generation
        let words: Vec<&str> = response.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            let token = if i < words.len() - 1 {
                format!("{} ", word)
            } else {
                word.to_string()
            };
            sender.send(token).ok();
        }
        Ok(())
    }
}

// ============================================================
// Fake TTS — generates simple tones instead of speech
// ============================================================

pub struct FakeTts {
    /// Duration in ms per character of text.
    ms_per_char: u32,
}

impl FakeTts {
    pub fn new() -> Self {
        Self { ms_per_char: 50 }
    }

    /// Set the synthesized speech duration per character.
    pub fn set_speed(&mut self, ms_per_char: u32) {
        self.ms_per_char = ms_per_char;
    }
}

impl Default for FakeTts {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextToSpeech for FakeTts {
    async fn synthesize(&mut self, text: &str) -> anyhow::Result<Vec<u8>> {
        let duration_ms = text.len() as u32 * self.ms_per_char;
        // Generate a 440Hz tone as fake audio output
        Ok(crate::audio::generate_tone(440.0, duration_ms.max(10)))
    }

    async fn synthesize_stream(
        &mut self,
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> anyhow::Result<()> {
        while let Some(text_chunk) = receiver.recv().await {
            let audio = self.synthesize(&text_chunk).await?;
            sender.send(audio).ok();
        }
        Ok(())
    }

    async fn flush(&mut self) -> anyhow::Result<Vec<u8>> {
        // No buffering in fake TTS — return empty
        Ok(Vec::new())
    }
}

// ============================================================
// Fake Turn Detector — simple energy-based VAD
// ============================================================

use crate::traits::{TurnDetector, TurnEvent};

pub struct FakeTurnDetector {
    events: Vec<Vec<TurnEvent>>,
    current_index: usize,
    model_speaking: bool,
}

impl FakeTurnDetector {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            current_index: 0,
            model_speaking: false,
        }
    }

    /// Pre-program a sequence of turn events for each audio chunk.
    pub fn set_event_sequence(&mut self, events: Vec<Vec<TurnEvent>>) {
        self.events = events;
        self.current_index = 0;
    }
}

impl Default for FakeTurnDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TurnDetector for FakeTurnDetector {
    async fn process_audio(&mut self, _audio: &[u8]) -> anyhow::Result<Vec<TurnEvent>> {
        if self.current_index < self.events.len() {
            let events = self.events[self.current_index].clone();
            self.current_index += 1;
            Ok(events)
        } else {
            Ok(vec![])
        }
    }

    fn model_started_speaking(&mut self) {
        self.model_speaking = true;
    }

    fn model_stopped_speaking(&mut self) {
        self.model_speaking = false;
    }

    fn reset(&mut self) {
        self.current_index = 0;
        self.model_speaking = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_stt() {
        let mut stt = FakeStt::new();
        stt.set_transcriptions(vec!["Hello world".to_string()]);

        let result = stt.end_audio().await.unwrap();
        assert_eq!(result, Some("Hello world".to_string()));
    }

    #[tokio::test]
    async fn test_fake_llm_match() {
        let mut llm = FakeLlm::new();
        llm.add_response("hello", "Hi there!");

        let result = llm.generate("Hello").await.unwrap();
        assert_eq!(result, "Hi there!");
    }

    #[tokio::test]
    async fn test_fake_llm_default() {
        let mut llm = FakeLlm::new();
        llm.set_default_response("I don't know.");

        let result = llm.generate("unknown query").await.unwrap();
        assert_eq!(result, "I don't know.");
    }

    #[tokio::test]
    async fn test_fake_tts() {
        let mut tts = FakeTts::new();
        let audio = tts.synthesize("Hi").await.unwrap();

        // Should produce some audio data
        assert!(!audio.is_empty());
        // Should be even number of bytes (PCM16)
        assert_eq!(audio.len() % 2, 0);
    }

    #[tokio::test]
    async fn test_fake_turn_detector() {
        let mut td = FakeTurnDetector::new();
        td.set_event_sequence(vec![
            vec![TurnEvent::SpeechStarted { audio_start_ms: 0 }],
            vec![TurnEvent::SpeechStopped { audio_end_ms: 1000 }],
        ]);

        let events = td.process_audio(&[]).await.unwrap();
        assert_eq!(events, vec![TurnEvent::SpeechStarted { audio_start_ms: 0 }]);

        let events = td.process_audio(&[]).await.unwrap();
        assert_eq!(events, vec![TurnEvent::SpeechStopped { audio_end_ms: 1000 }]);
    }

    #[tokio::test]
    async fn test_pipeline_streaming() {
        use crate::traits::RealtimePipeline;

        // Set up fake components
        let mut stt = FakeStt::new();
        stt.set_transcriptions(vec!["Hello".to_string()]);

        let mut llm = FakeLlm::new();
        llm.add_response("hello", "Hi there how are you");

        let tts = FakeTts::new();

        let mut td = FakeTurnDetector::new();
        td.set_event_sequence(vec![
            vec![TurnEvent::SpeechStarted { audio_start_ms: 0 }],
            vec![TurnEvent::SpeechStopped { audio_end_ms: 1000 }],
        ]);

        let mut pipeline = RealtimePipeline::new(stt, llm, tts, td);

        // Feed audio through turn detector
        let events = pipeline.process_audio(&[]).await.unwrap();
        assert_eq!(events, vec![TurnEvent::SpeechStarted { audio_start_ms: 0 }]);

        let events = pipeline.process_audio(&[]).await.unwrap();
        assert_eq!(events, vec![TurnEvent::SpeechStopped { audio_end_ms: 1000 }]);

        // Handle the turn with streaming
        let mut audio_chunks = Vec::new();
        let text = pipeline
            .handle_turn_streaming(&mut |audio| {
                audio_chunks.push(audio);
            })
            .await
            .unwrap();

        // Verify text was generated
        assert!(text.contains("Hi there"), "Expected streaming response, got: {}", text);
        // Verify audio chunks were produced (5 words = 5 chunks)
        assert_eq!(audio_chunks.len(), 5, "Expected 5 audio chunks for 5 words");
        // Each chunk should be non-empty PCM16
        for chunk in &audio_chunks {
            assert!(!chunk.is_empty());
            assert_eq!(chunk.len() % 2, 0);
        }
    }

    #[tokio::test]
    async fn test_llm_streaming_token_by_token() {
        let mut llm = FakeLlm::new();
        llm.add_response("test", "one two three four five");

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        llm.generate_stream("test", tx).await.unwrap();

        let mut tokens = Vec::new();
        while let Some(token) = rx.recv().await {
            tokens.push(token);
        }

        assert_eq!(tokens.len(), 5, "Expected 5 tokens for 5 words");
        assert_eq!(tokens[0], "one ");
        assert_eq!(tokens[4], "five");
    }

    #[tokio::test]
    async fn test_tts_streaming() {
        let mut tts = FakeTts::new();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (audio_tx, mut audio_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        // Send text chunks
        tx.send("hello ".to_string()).unwrap();
        tx.send("world".to_string()).unwrap();
        drop(tx);

        tts.synthesize_stream(&mut rx, audio_tx).await.unwrap();

        let mut audio_chunks = Vec::new();
        while let Ok(chunk) = audio_rx.try_recv() {
            audio_chunks.push(chunk);
        }

        assert_eq!(audio_chunks.len(), 2, "Expected 2 audio chunks");
    }
}
