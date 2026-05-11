
/// Turn-taking state machine inspired by pipecat's model.
///
/// States:
/// - Idle: waiting for user speech
/// - Listening: user is speaking, accumulating audio
/// - Processing: user stopped, model is generating response
/// - Speaking: model is outputting audio
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Listening,
    Processing,
    Speaking,
}

/// Configuration for turn-taking behavior.
#[derive(Debug, Clone)]
pub struct TurnTakingConfig {
    /// Minimum speech duration to consider as valid speech (ms).
    pub min_speech_duration_ms: u32,
    /// Silence duration to detect endpoint (ms).
    pub silence_duration_ms: u32,
    /// Padding added before detected speech start (ms).
    pub prefix_padding_ms: u32,
    /// Audio energy threshold for VAD (0.0 - 1.0).
    pub vad_threshold: f32,
    /// Sample rate for energy calculation.
    pub sample_rate: u32,
    /// Whether to allow user interruption of model speech.
    pub allow_interruptions: bool,
}

impl Default for TurnTakingConfig {
    fn default() -> Self {
        Self {
            min_speech_duration_ms: 100,
            silence_duration_ms: 200,
            prefix_padding_ms: 100,
            vad_threshold: 0.02,
            sample_rate: 24000,
            allow_interruptions: true,
        }
    }
}

/// State machine that manages turn-taking in realtime audio conversations.
///
/// Tracks VAD energy levels to detect speech start/stop, enforces
/// minimum speech and silence durations, and handles interruption
/// when the user speaks over the model.
pub struct TurnTakingMachine {
    config: TurnTakingConfig,
    state: TurnState,
    accumulated_speech_ms: u32,
    accumulated_silence_ms: u32,
    model_speaking: bool,
}

impl TurnTakingMachine {
    pub fn new(config: TurnTakingConfig) -> Self {
        Self {
            config,
            state: TurnState::Idle,
            accumulated_speech_ms: 0,
            accumulated_silence_ms: 0,
            model_speaking: false,
        }
    }

    pub fn state(&self) -> TurnState {
        self.state
    }

    pub fn model_speaking(&self) -> bool {
        self.model_speaking
    }

    /// Compute RMS energy of PCM16 audio samples (as a simple VAD proxy).
    fn compute_energy(audio: &[u8]) -> f32 {
        if audio.len() < 2 {
            return 0.0;
        }
        let samples: Vec<i16> = audio
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        let sum_sq: f64 = samples.iter().map(|&s| s as f64 * s as f64 ).sum();
        let rms = (sum_sq / samples.len() as f64).sqrt();
        (rms / 32768.0) as f32 // Normalize to 0.0-1.0
    }

    /// Process an audio chunk and return any turn events.
    /// Returns events that occurred during this chunk.
    pub fn process_chunk(&mut self, audio: &[u8]) -> Vec<TurnMachineEvent> {
        let mut events = Vec::new();
        let energy = Self::compute_energy(audio);
        let chunk_duration_ms =
            ((audio.len() as u64 / 2) * 1000 / self.config.sample_rate as u64) as u32;
        let is_speech = energy > self.config.vad_threshold;

        match self.state {
            TurnState::Idle => {
                if is_speech {
                    self.accumulated_speech_ms = chunk_duration_ms;
                    self.accumulated_silence_ms = 0;
                    self.state = TurnState::Listening;
                    events.push(TurnMachineEvent::SpeechStarted);
                }
            }
            TurnState::Listening => {
                if is_speech {
                    self.accumulated_speech_ms += chunk_duration_ms;
                    self.accumulated_silence_ms = 0;
                } else {
                    self.accumulated_silence_ms += chunk_duration_ms;
                    if self.accumulated_silence_ms >= self.config.silence_duration_ms {
                        // Endpoint detected
                        if self.accumulated_speech_ms >= self.config.min_speech_duration_ms {
                            self.state = TurnState::Processing;
                            events.push(TurnMachineEvent::SpeechStopped);
                        } else {
                            // Too short, ignore as noise
                            self.state = TurnState::Idle;
                            self.accumulated_speech_ms = 0;
                            self.accumulated_silence_ms = 0;
                        }
                    }
                }
            }
            TurnState::Processing => {
                // Waiting for model response; handled externally.
            }
            TurnState::Speaking => {
                if is_speech && self.config.allow_interruptions {
                    // User interrupted the model
                    events.push(TurnMachineEvent::Interrupted);
                    self.state = TurnState::Listening;
                    self.accumulated_speech_ms = chunk_duration_ms;
                    self.accumulated_silence_ms = 0;
                    self.model_speaking = false;
                }
            }
        }

        events
    }

    /// Called when the model starts generating/speaking.
    pub fn on_model_start_speaking(&mut self) {
        self.state = TurnState::Speaking;
        self.model_speaking = true;
    }

    /// Called when the model finishes generating/speaking.
    pub fn on_model_stop_speaking(&mut self) {
        self.state = TurnState::Idle;
        self.model_speaking = false;
        self.accumulated_speech_ms = 0;
        self.accumulated_silence_ms = 0;
    }

    /// Reset to idle state (e.g., new session).
    pub fn reset(&mut self) {
        self.state = TurnState::Idle;
        self.accumulated_speech_ms = 0;
        self.accumulated_silence_ms = 0;
        self.model_speaking = false;
    }
}

/// Events produced by the turn-taking machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnMachineEvent {
    SpeechStarted,
    SpeechStopped,
    Interrupted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio;

    #[test]
    fn test_idle_to_listening_on_speech() {
        let config = TurnTakingConfig::default();
        let mut tm = TurnTakingMachine::new(config);
        assert_eq!(tm.state(), TurnState::Idle);

        // Generate a 200ms tone (should trigger VAD)
        let tone = audio::generate_tone(440.0, 200);
        let events = tm.process_chunk(&tone);

        assert_eq!(tm.state(), TurnState::Listening);
        assert!(events.contains(&TurnMachineEvent::SpeechStarted));
    }

    #[test]
    fn test_silence_stays_idle() {
        let config = TurnTakingConfig::default();
        let mut tm = TurnTakingMachine::new(config);
        assert_eq!(tm.state(), TurnState::Idle);

        let silence = audio::generate_silence(200);
        let events = tm.process_chunk(&silence);

        assert_eq!(tm.state(), TurnState::Idle);
        assert!(events.is_empty());
    }

    #[test]
    fn test_endpoint_detection_on_silence() {
        let mut config = TurnTakingConfig::default();
        config.silence_duration_ms = 50; // Short for test
        let mut tm = TurnTakingMachine::new(config);

        // Start speech
        let tone = audio::generate_tone(440.0, 150);
        tm.process_chunk(&tone);
        assert_eq!(tm.state(), TurnState::Listening);

        // Send silence to trigger endpoint
        let silence = audio::generate_silence(200);
        let events = tm.process_chunk(&silence);

        assert_eq!(tm.state(), TurnState::Processing);
        assert!(events.contains(&TurnMachineEvent::SpeechStopped));
    }

    #[test]
    fn test_model_speaking_transition() {
        let config = TurnTakingConfig::default();
        let mut tm = TurnTakingMachine::new(config);

        tm.on_model_start_speaking();
        assert_eq!(tm.state(), TurnState::Speaking);
        assert!(tm.model_speaking());

        tm.on_model_stop_speaking();
        assert_eq!(tm.state(), TurnState::Idle);
        assert!(!tm.model_speaking());
    }

    #[test]
    fn test_interruption_during_model_speech() {
        let mut config = TurnTakingConfig::default();
        config.allow_interruptions = true;
        let mut tm = TurnTakingMachine::new(config);

        tm.on_model_start_speaking();
        assert_eq!(tm.state(), TurnState::Speaking);

        // User speaks over the model
        let tone = audio::generate_tone(440.0, 200);
        let events = tm.process_chunk(&tone);

        assert_eq!(tm.state(), TurnState::Listening);
        assert!(events.contains(&TurnMachineEvent::Interrupted));
        assert!(!tm.model_speaking());
    }

    #[test]
    fn test_short_noise_ignored() {
        let mut config = TurnTakingConfig::default();
        config.min_speech_duration_ms = 500;
        config.silence_duration_ms = 10;
        let mut tm = TurnTakingMachine::new(config);

        // Very short burst
        let tone = audio::generate_tone(440.0, 50);
        tm.process_chunk(&tone);
        assert_eq!(tm.state(), TurnState::Listening);

        // Quick silence
        let silence = audio::generate_silence(20);
        let events = tm.process_chunk(&silence);

        // Should be ignored as noise
        assert_eq!(tm.state(), TurnState::Idle);
        assert!(!events.contains(&TurnMachineEvent::SpeechStopped));
    }
}
