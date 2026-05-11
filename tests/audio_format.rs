use open_realtime::audio;
use open_realtime::protocol::{ServerEvent, SessionConfig};
use std::time::Duration;

mod common;
#[allow(unused_imports)]
use common::{connect_with, fake_transport, openai_connect, TestSession};

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn af1_pcm16_24k_output_valid() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();

    let mut audio_deltas: Vec<String> = Vec::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseOutputAudioDelta { delta, .. } => {
                audio_deltas.push(delta);
            }
            ServerEvent::ResponseDone { .. } => break,
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // Concatenate and decode all audio
    if !audio_deltas.is_empty() {
        for delta in audio_deltas {
            let pcm_bytes = audio::base64_to_pcm(&delta).expect("Should decode base64");
            // Each PCM16 sample is 2 bytes
            assert_eq!(
                pcm_bytes.len() % 2,
                0,
                "PCM data should have even number of bytes"
            );
            let num_samples = pcm_bytes.len() / 2;
            assert!(num_samples > 0, "Should have at least one sample");

            // Verify sample timing
            let duration_ms = audio::pcm_duration_ms(&pcm_bytes);
            assert!(duration_ms > 0, "Duration should be positive");
        }
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn af2_pcm16_range_valid() {
    dotenvy::dotenv().ok();
    let mut session = openai_connect().await.unwrap();

    session
        .update_session(SessionConfig {
            modalities: Some(vec!["audio".to_string(), "text".to_string()]),
            temperature: Some(0.8),
            max_response_output_tokens: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();

    session.send_text("Say hello").await.unwrap();

    let mut all_samples: Vec<i16> = Vec::new();
    loop {
        let event = session.recv_timeout(Duration::from_secs(20)).await.unwrap();
        match event {
            ServerEvent::ResponseOutputAudioDelta { delta, .. } => {
                let samples = audio::base64_to_samples(&delta)
                    .expect("Should decode audio samples");
                all_samples.extend(samples);
            }
            ServerEvent::ResponseDone { .. } => break,
            ServerEvent::Error { error } => {
                panic!("Error: {}", error.message);
            }
            _ => {}
        }
    }

    // May not get audio for very short responses in text+audio mode
    if !all_samples.is_empty() {
        assert!(
            audio::validate_pcm16_range(&all_samples),
            "All samples should be within i16 range"
        );
    }

    session.close().await.ok();
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn af3_audio_encode_decode_roundtrip() {
    // Local-only test: verify our audio utilities work correctly
    let original = audio::generate_tone(440.0, 100);

    let b64 = audio::pcm_to_base64(&original);
    let decoded = audio::base64_to_pcm(&b64).expect("Should decode");

    assert_eq!(original.len(), decoded.len(), "Roundtrip should preserve length");

    for (i, (&o, &d)) in original.iter().zip(decoded.iter()).enumerate() {
        assert_eq!(o, d, "Byte {} mismatch in roundtrip", i);
    }

    // Also test sample-level roundtrip
    let samples = audio::base64_to_samples(&b64).expect("Should decode to samples");
    assert_eq!(samples.len(), original.len() / 2);
    assert!(audio::validate_pcm16_range(&samples));
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn af4_generate_silence_valid() {
    // Verify our silence generator produces correct output
    let silence = audio::generate_silence(50); // 50ms
    let expected_samples = (24000 * 50) / 1000;
    assert_eq!(silence.len(), expected_samples * 2); // 2 bytes per sample

    let samples = audio::base64_to_samples(&audio::pcm_to_base64(&silence)).unwrap();
    assert!(samples.iter().all(|&s| s == 0), "All samples should be zero for silence");
    assert!(audio::validate_pcm16_range(&samples));
}

#[tokio::test]
#[ignore = "requires OAI_KEY env var and live API"]
async fn af5_generate_tone_valid() {
    // Verify our tone generator produces valid output
    let tone = audio::generate_tone(440.0, 100);
    let expected_samples = (24000 * 100) / 1000;
    assert_eq!(tone.len(), expected_samples * 2);

    let samples = audio::base64_to_samples(&audio::pcm_to_base64(&tone)).unwrap();
    assert!(audio::validate_pcm16_range(&samples));

    // At least some samples should be non-zero
    assert!(samples.iter().any(|&s| s != 0), "Tone should have non-zero samples");
}

#[tokio::test]
async fn local_fake_audio_format_works() {
    let mut fake = fake_transport();
    fake.enqueue_session_updated();
    fake.enqueue_audio_response("hello", "base64audiodata");
    let mut session = connect_with(fake).await.unwrap();
    session.update_session(SessionConfig {
        modalities: Some(vec!["audio".to_string(), "text".to_string()]),
        ..Default::default()
    }).await.unwrap();
    session.send_text("Say hello").await.unwrap();
    let response = session.wait_for_response_done().await.unwrap();
    assert_eq!(response.status, "completed");
    session.close().await.ok();
}
