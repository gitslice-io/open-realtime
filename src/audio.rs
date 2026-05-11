/// Generate PCM16 silence at 24000 Hz.
/// Returns a Vec<u8> of raw PCM16 samples (little-endian).
pub fn generate_silence(duration_ms: u32) -> Vec<u8> {
    let sample_rate = 24000;
    let num_samples = (sample_rate as u64 * duration_ms as u64) / 1000;
    let mut buf = Vec::with_capacity((num_samples * 2) as usize);
    for _ in 0..num_samples {
        buf.extend_from_slice(&0i16.to_le_bytes());
    }
    buf
}

/// Generate a sine tone at the given frequency and duration (PCM16, 24000 Hz).
pub fn generate_tone(freq: f32, duration_ms: u32) -> Vec<u8> {
    let sample_rate: f32 = 24000.0;
    let num_samples = (sample_rate * duration_ms as f32 / 1000.0) as usize;
    let mut buf = Vec::with_capacity(num_samples * 2);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate;
        let sample = (t * 2.0 * std::f32::consts::PI * freq).sin();
        let pcm = (sample * 16000.0) as i16; // Keep volume moderate
        buf.extend_from_slice(&pcm.to_le_bytes());
    }
    buf
}

/// Encode raw PCM16 bytes to base64 string.
pub fn pcm_to_base64(pcm_data: &[u8]) -> String {
    use base64::Engine;
    use base64::engine::general_purpose;
    general_purpose::STANDARD.encode(pcm_data)
}

/// Decode base64 string to raw PCM16 bytes.
pub fn base64_to_pcm(b64: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    use base64::engine::general_purpose;
    general_purpose::STANDARD.decode(b64)
}

/// Decode base64 string to Vec<i16> PCM16 samples.
pub fn base64_to_samples(b64: &str) -> Result<Vec<i16>, String> {
    let bytes = base64_to_pcm(b64).map_err(|e| e.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err("PCM16 data has odd length".to_string());
    }
    let samples: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    Ok(samples)
}

/// Validate that PCM16 data is within acceptable range.
pub fn validate_pcm16_range(samples: &[i16]) -> bool {
    samples.iter().all(|&s| s >= i16::MIN && s <= i16::MAX)
}

/// Get the duration in milliseconds of PCM16 data at 24000 Hz.
pub fn pcm_duration_ms(pcm_data: &[u8]) -> u32 {
    let num_samples = pcm_data.len() / 2;
    ((num_samples as u64 * 1000) / 24000) as u32
}

/// Convert Vec<i16> samples to raw PCM16 bytes (little-endian).
pub fn samples_to_pcm(samples: &[i16]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    buf
}
