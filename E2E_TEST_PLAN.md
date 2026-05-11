# E2E Test Plan: OpenAI Realtime API Protocol

## Language & Stack
- **Rust** with `tokio` async runtime
- **WebSocket client:** `tokio-tungstenite` (via `tungstenite`)
- **Serialization:** `serde` / `serde_json`
- **Config:** `dotenvy` for `.env` loading
- **Audio:** `base64` crate for encoding/decoding, raw PCM16 generation

## Dependencies (`Cargo.toml`)

```toml
[package]
name = "open-realtime"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dotenvy = "0.15"
base64 = "0.22"
anyhow = "1"
url = "2"

[dev-dependencies]
tokio-test = "0.4"
```

## Test Infrastructure

### Harness: `tests/common/mod.rs`

A shared test harness that:
1. Loads `OAI_KEY` from `.env`
2. Connects to `wss://api.openai.com/v1/realtime?model=gpt-realtime-2`
3. Waits for `session.created` before returning the connection
4. Provides helpers: `send(&mut ws, event)`, `recv(&mut ws) -> Event`, `expect_event(&mut ws, type)`
5. Cleans up connections on drop

### Event Types: `src/protocol.rs`

Strongly-typed Rust enums/structs for all client and server events:
- `ClientEvent` enum covering: `session.update`, `response.create`, `response.cancel`, `conversation.item.create`, `conversation.item.delete`, `conversation.item.truncate`, `input_audio_buffer.append`, `input_audio_buffer.commit`, `input_audio_buffer.clear`
- `ServerEvent` enum covering: `session.created`, `session.updated`, `response.created`, `response.done`, `response.output_audio.delta`, `response.output_audio_transcript.delta`, `response.output_text.delta`, `conversation.item.added`, `conversation.item.done`, `input_audio_buffer.speech_started`, `input_audio_buffer.speech_stopped`, `input_audio_buffer.committed`, `error`

### Audio Utilities: `src/audio.rs`

- `generate_silence(duration_ms: u32) -> Vec<u8>` — PCM16 silence at 24kHz
- `generate_tone(freq: f32, duration_ms: u32) -> Vec<u8>` — test tone
- `to_base64(pcm_data: &[i16]) -> String`
- `from_base64(b64: &str) -> Vec<i16>`

## Test Categories & Cases

---

### 1. Connection Lifecycle (`tests/connection.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| C1 | `connect_with_valid_key` | Connect with valid `OAI_KEY` header | WebSocket opens, `session.created` received |
| C2 | `connect_with_invalid_key` | Connect with `Bearer sk-invalid` | Connection refused or `error` event |
| C3 | `connect_with_model_param` | Connect to `?model=gpt-realtime-2` | Session uses specified model in `session.created` |
| C4 | `connect_without_model` | Connect to `/v1/realtime` without query string | Default model assigned, `session.created` received |
| C5 | `header_beta_version` | Verify `OpenAI-Beta: realtime=v1` header is accepted | Connection succeeds |
| C6 | `graceful_disconnect` | Close WebSocket cleanly | No unexpected errors, resources freed |

---

### 2. Session Management (`tests/session.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| S1 | `session_created_event` | On connect, verify `session.created` structure | Event has `type: "session.created"`, `session` object with model, modalities, etc. |
| S2 | `update_instructions` | Send `session.update` with new `instructions` | `session.updated` received, `instructions` match |
| S3 | `update_voice_before_audio` | Set voice to `alloy` before any audio response | `session.updated` confirms voice change |
| S4 | `update_voice_after_audio` | Change voice after model has spoken | Should be rejected or ignored (voice locked) |
| S5 | `update_output_modalities` | Switch between `["audio"]`, `["text"]`, both | `session.updated` reflects change |
| S6 | `update_temperature` | Set `temperature` to valid range (0.0-2.0) | `session.updated` confirms |
| S7 | `update_invalid_field` | Send a non-existent session field | `error` event or field ignored silently |
| S8 | `update_bad_reasoning_effort` | Set `reasoning.effort` to `"extreme"` | `error` event (invalid enum value) |
| S9 | `update_turn_detection` | Set `turn_detection.type` to `semantic_vad` and `disabled` | Both should succeed |

---

### 3. Text Conversation (`tests/text_conversation.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| T1 | `simple_text_qa` | Create text item + `response.create` | Receive text response in `response.done` output |
| T2 | `text_response_structure` | Verify response event ordering | Events fire in correct lifecycle order (see plan) |
| T3 | `text_delta_streaming` | Verify `response.output_text.delta` events | Multiple deltas received, concatenated == final text |
| T4 | `multi_turn_conversation` | Send 3 text messages sequentially | Each gets a coherent response, context preserved |
| T5 | `text_only_modality` | Set `output_modalities: ["text"]`, send text | Only text output, no audio deltas |
| T6 | `empty_text_input` | Send `input_text` with empty string | Model handles gracefully (asks for input or errors) |
| T7 | `long_text_input` | Send text near context limit boundary | Should not panic; response truncated or `error` |

---

### 4. Audio Conversation (`tests/audio_conversation.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| A1 | `stream_audio_vad` | Stream PCM16 audio chunks via `input_audio_buffer.append` with VAD on | `speech_started` → `speech_stopped` → `response.done` |
| A2 | `stream_audio_manual` | Stream audio with VAD disabled, then `commit` + `response.create` | Model responds after explicit `response.create` |
| A3 | `audio_output_deltas` | Verify `response.output_audio.delta` events | Multiple base64 PCM16 deltas, can be concatenated and played |
| A4 | `audio_output_transcript` | Verify `response.output_audio_transcript.delta` | Transcript text matches spoken audio content |
| A5 | `full_audio_message` | Use `conversation.item.create` with `input_audio` content | Model processes full audio message correctly |
| A6 | `audio_format_pcm16_24k` | Verify output audio byte format | Output is valid PCM16 at 24000 Hz |
| A7 | `silence_input` | Send silence (near-zero PCM16 samples) | VAD detects it as no-speech or speech_stopped |
| A8 | `append_large_chunk` | Send audio chunk near 15MB limit | Accepted (under limit) or rejected with `error` |

---

### 5. Turn Detection (`tests/turn_detection.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| D1 | `vad_semantic_default` | Connect with default VAD settings | Turn detection works automatically |
| D2 | `vad_speech_started` | Stream audio, verify `input_audio_buffer.speech_started` | Event fires when speech detected |
| D3 | `vad_speech_stopped` | Stop audio, verify `input_audio_buffer.speech_stopped` | Event fires after silence threshold |
| D4 | `vad_disabled_no_auto_response` | Set `turn_detection: disabled`, stream audio without `response.create` | No model response generated |
| D5 | `vad_disabled_manual_response` | VAD disabled, send `commit` + `response.create` | Model responds correctly |
| D6 | `clear_audio_buffer` | Stream audio then send `input_audio_buffer.clear` | Buffer discarded, no response |
| D7 | `commit_empty_buffer` | Send `commit` with no audio appended | Model handles gracefully |

---

### 6. Modality Control (`tests/modalities.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| M1 | `audio_only_output` | Set `output_modalities: ["audio"]`, ask question | Only audio output received, no text deltas |
| M2 | `text_only_output` | Set `output_modalities: ["text"]`, ask question | Only text output received, no audio deltas |
| M3 | `both_modalities` | Set `output_modalities: ["audio", "text"]` | Both audio and text deltas received |
| M4 | `empty_modalities` | Set `output_modalities: []` | `error` event (invalid) |
| M5 | `per_response_modalities` | Override modalities on `response.create` | That specific response uses overridden modality |

---

### 7. Function Calling / Tools (`tests/tools.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| F1 | `register_single_tool` | Add one tool via `session.update` | Tool appears in `session.updated` |
| F2 | `register_multiple_tools` | Add 3 tools in one update | All three registered |
| F3 | `trigger_tool_call` | Send text that triggers a tool, verify `response.function_call_arguments.done` | Tool name and arguments received |
| F4 | `send_tool_output` | After tool call, send `function_call_output` item + `response.create` | Model uses tool output in final response |
| F5 | `tool_missing_required_param` | Trigger tool with text missing a required parameter | Model asks for clarification or errors |
| F6 | `tool_invalid_name` | Register tool with empty name or special chars | `error` event |
| F7 | `tool_no_handler` | Trigger tool but don't send output (simulate timeout) | Model may retry or escalate |

---

### 8. Response Phases — Realtime 2 (`tests/response_phases.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| P1 | `commentary_phase_present` | Ask a question that triggers reasoning + tool use | `response.done` output has `phase: "commentary"` item(s) |
| P2 | `final_answer_phase` | Verify `phase: "final_answer"` in response output | Final phase contains the answer |
| P3 | `no_commentary_on_simple` | Ask a simple direct question | Commentary phase may be absent, only final_answer |
| P4 | `reasoning_effort_affects_phases` | Compare `minimal` vs `medium` reasoning effort | Higher effort may produce more commentary |

---

### 9. Conversation Items (`tests/conversation_items.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| I1 | `create_text_item` | `conversation.item.create` with `input_text` | `conversation.item.added` + `conversation.item.done` |
| I2 | `create_audio_item` | Create item with `input_audio` | Item added successfully |
| I3 | `create_item_with_previous_item_id` | Reference a prior item ID | Item placed correctly in conversation |
| I4 | `delete_item` | Delete an existing conversation item | Item removed, subsequent response ignores it |
| I5 | `truncate_audio_item` | Truncate an audio item at sample offset | Audio content truncated correctly |
| I6 | `item_done_event` | Verify `conversation.item.done` fires after create | Event contains complete item data |

---

### 10. Response Lifecycle Ordering (`tests/response_lifecycle.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| L1 | `text_response_event_order` | Track all events for a text response | Strict ordering: `response.created` → `output_item.added` → `content_part.added` → `output_text.delta`* → `output_text.done` → `content_part.done` → `output_item.done` → `response.done` → `rate_limits.updated` |
| L2 | `audio_response_event_order` | Track all events for an audio response | Same pattern with `output_audio.delta` and `output_audio_transcript.delta` |

See [Interruptions](#11-interruptions-testsinterruptionsrs) for response cancellation and VAD-based interruption tests.

---

### 11. Interruptions (`tests/interruptions.rs`)

Tests for `response.cancel`, `input_audio_buffer.clear`, VAD-based automatic interruption, and rapid cancel/recovery patterns.

#### 11a. Core `response.cancel` Behavior

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IC1 | `cancel_during_audio_playback` | Send `response.cancel` while model is streaming audio output | Audio deltas stop; `response.done` fires with status `"cancelled"` or partial response; no further audio |
| IC2 | `cancel_during_text_streaming` | Send `response.cancel` while model is streaming text deltas | Text deltas stop; `response.done` fires with truncated or cancelled status |
| IC3 | `cancel_with_valid_response_id` | Cancel using the `response_id` from `response.created` | Cancellation succeeds; that specific response stops |
| IC4 | `cancel_with_wrong_response_id` | Cancel using a non-existent or stale `response_id` | `error` event returned; active response unaffected |
| IC5 | `cancel_after_response_done` | Send `response.cancel` after `response.done` has already fired | No-op or `error` event (response already completed) |
| IC6 | `cancel_before_any_response` | Send `response.cancel` when no response is in progress | `error` event or silently ignored |
| IC7 | `cancel_missing_response_id` | Send `response.cancel` with no `response_id` field | `error` event (missing required field) |

#### 11b. Cancel During Response Phases (Realtime 2)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IP1 | `cancel_during_commentary_phase` | Cancel while model is in `commentary` phase (preamble) | Commentary stops; no final_answer phase emitted; `response.done` shows cancelled |
| IP2 | `cancel_during_final_answer_phase` | Cancel while model is in `final_answer` phase | Final answer truncated; `response.done` fires |
| IP3 | `cancel_during_tool_call` | Cancel while model is emitting a `function_call` item (before tool output sent) | Function call aborted; no tool output expected; `response.done` fires |
| IP4 | `cancel_after_tool_output_before_final` | Send tool output, then cancel before model generates final answer | Final answer cancelled; tool output may still be in conversation |

#### 11c. Cancel + Immediate New Response

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IR1 | `cancel_then_new_text` | Cancel → immediately `conversation.item.create` (text) + `response.create` | Model responds to new text; old cancelled response is gone |
| IR2 | `cancel_then_new_audio` | Cancel → immediately `input_audio_buffer.append` + `commit` + `response.create` | Model responds to new audio; no artifacts from cancelled response |
| IR3 | `cancel_then_retry_same_input` | Cancel → `response.create` without new input | Model handles empty turn; may ask for input or repeat previous context |
| IR4 | `cancel_then_cancel_again` | Send two cancels in rapid succession before first completes | Both handled; no crash; final state is cancelled |
| IR5 | `cancel_then_session_update_then_response` | Cancel → `session.update` → `response.create` | Session update applies; new response uses updated config |

#### 11d. Cancel + Conversation State Integrity

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IS1 | `cancel_preserves_prior_items` | Verify conversation items from before the cancelled turn still exist | Prior `message` items remain; no data loss |
| IS2 | `cancel_excludes_interrupted_content` | Verify the cancelled response's partial content is NOT in conversation | No partial output from cancelled response in item list |
| IS3 | `cancel_with_audio_sample_offset` | Cancel audio playback at a specific `sample_count` to truncate played audio | Item truncated at sample offset; audio before offset preserved; audio after offset removed |
| IS4 | `cancel_sample_offset_beyond_audio` | Provide `sample_count` larger than actual audio length | `error` event or clamp to max length |

#### 11e. `input_audio_buffer.clear` Behavior

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IB1 | `clear_buffer_during_vad_speech` | `input_audio_buffer.clear` while VAD has detected speech | Buffer emptied; pending auto-response cancelled; `speech_stopped` may fire |
| IB2 | `clear_buffer_vad_disabled` | `clear` buffer with `turn_detection: disabled` after appending audio | Buffer emptied; subsequent `commit` has no audio to process |
| IB3 | `clear_empty_buffer` | `clear` when no audio has been appended | No-op; no error |
| IB4 | `clear_then_append_new_audio` | `clear` → `append` new audio → `commit` → `response.create` | Only the new audio is processed; old audio discarded |
| IB5 | `clear_during_model_response` | Model is generating audio, client sends `clear` on input buffer | Input buffer cleared; active response generation unaffected (input/output buffers are independent) |

#### 11f. VAD-Based Automatic Interruption

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IV1 | `vad_interrupts_model_audio` | Model is speaking; user starts talking (VAD detects new speech) | Model output stops; server implicitly cancels current response; `speech_started` fires |
| IV2 | `vad_interrupt_fires_speech_started` | Verify `input_audio_buffer.speech_started` event fires when user interrupts model | Event timestamp is after interruption, before new audio processing |
| IV3 | `vad_interrupt_new_turn_starts` | After VAD interrupt, verify the new user turn is processed | Model responds to the interrupting speech, not the interrupted context |
| IV4 | `vad_manual_cancel_vs_auto` | Compare manual `response.cancel` vs VAD-triggered interruption timeline | Both stop model output; VAD auto-commits new audio buffer; manual cancel requires explicit new input |
| IV5 | `vad_no_interrupt_on_silence` | Background silence during model speech (VAD disabled or semantic_vad on silence) | Model continues speaking uninterrupted |

#### 11g. Interruption During Tool Execution

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IT1 | `cancel_while_waiting_for_tool_output` | Trigger tool call → cancel before sending `function_call_output` | Tool call is abandoned; model does not wait for output |
| IT2 | `cancel_tool_call_then_new_tool_call` | Cancel tool call → send new input that triggers same tool → send output | Second tool call proceeds normally; no interference from cancelled call |
| IT3 | `cancel_after_partial_tool_args` | Cancel while `response.function_call_arguments.delta` is still streaming arguments | Arguments truncated; incomplete call abandoned |

#### 11h. Interruption Edge Cases

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| IE1 | `cancel_with_empty_response_id` | Send `response.cancel` with `response_id: ""` | `error` event (invalid ID) |
| IE2 | `cancel_very_short_response` | Start response → cancel within ~50ms (before first delta) | Cancellation succeeds; response never emits content |
| IE3 | `cancel_during_long_reasoning` | Cancel while model is in hidden reasoning (no preamble yet) with high reasoning effort | Reasoning stops; no preamble or answer emitted |
| IE4 | `interleave_cancel_and_append` | Race condition: `input_audio_buffer.append` and `response.cancel` sent back-to-back | Both processed in order; no crash or dropped events |
| IE5 | `cancel_multiple_responses_serial` | Generate 3 responses, cancel each one mid-stream, verify 4th succeeds | Each cancellation is clean; 4th response completes normally |
| IE6 | `cancel_response_then_disconnect` | Cancel → immediately close WebSocket | Clean disconnect; no server error |

---

### 12. Error Handling (`tests/errors.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| E1 | `invalid_event_type` | Send `{ "type": "nonexistent_event" }` | `error` event with descriptive message |
| E2 | `malformed_json` | Send non-JSON string over WebSocket | Connection may close or `error` event |
| E3 | `missing_type_field` | Send `{ "data": "no type field" }` | `error` event |
| E4 | `invalid_session_config` | Set `model: "nonexistent-model"` | `error` event in `session.updated` response |
| E5 | `send_before_session_created` | Fire events immediately on connect before `session.created` | Should be buffered or produce `error` |
| E6 | `rate_limits_updated` | Observe `rate_limits.updated` event structure | Contains valid token/request limit fields |

---

### 13. Audio Format & Encoding (`tests/audio_format.rs`)

| ID | Test | Description | Expected Result |
|----|------|-------------|-----------------|
| AF1 | `pcm16_24k_output_valid` | Decode received base64 audio, verify sample count | Samples align to 24000 Hz expectation |
| AF2 | `pcm16_range` | Verify output samples are within i16 range [-32768, 32767] | All samples in range |
| AF3 | `base64_decode_roundtrip` | Encode known PCM16, decode, verify match | Data integrity preserved |
| AF4 | `pcmu_output_format` | Set output format to `audio/pcmu`, verify format in `session.updated` | Format is accepted, output uses μ-law |

---

## Test Execution Plan

### Phase 1: Scaffold (Day 1)
1. Create `Cargo.toml` with dependencies
2. Create `.env.example` (no real key)
3. Implement `src/protocol.rs` — event type definitions
4. Implement `src/audio.rs` — PCM16 utilities
5. Implement `tests/common/mod.rs` — test harness

### Phase 2: Core Protocol (Day 2)
1. Connection tests (C1-C6)
2. Session management tests (S1-S9)
3. Text conversation tests (T1-T7)

### Phase 3: Audio & Advanced (Day 3)
1. Audio conversation tests (A1-A8)
2. Turn detection tests (D1-D7)
3. Modality tests (M1-M5)

### Phase 4: Tools, Phases & Items (Day 4)
1. Function calling tests (F1-F7)
2. Response phase tests (P1-P4)
3. Conversation item tests (I1-I6)

### Phase 5: Interruptions (Day 5)
1. Core cancel behavior (IC1-IC7)
2. Cancel during response phases (IP1-IP4)
3. Cancel + immediate new response (IR1-IR5)
4. Cancel + conversation state integrity (IS1-IS4)
5. Input audio buffer clear (IB1-IB5)
6. VAD-based automatic interruption (IV1-IV5)
7. Interruption during tool execution (IT1-IT3)
8. Interruption edge cases (IE1-IE6)

### Phase 6: Lifecycle, Errors & Polish (Day 6)
1. Response lifecycle ordering (L1-L2)
2. Error handling tests (E1-E6)
3. Audio format tests (AF1-AF4)
4. Integration / stress test (all categories combined in one long session)

### CI Considerations
- Tests require a live API key → mark with `#[ignore]` by default, run with `--ignored` in CI
- Use tier/rate-limit aware test ordering (rate-limited tests last)
- Timeout each test at 30 seconds
- Never commit real API keys

## Running Tests

```bash
# Create .env from example (add your real key)
cp .env.example .env

# Run all tests (requires API key)
cargo test -- --ignored --test-threads=1

# Run a specific category
cargo test connection -- --ignored

# Run with logging
RUST_LOG=debug cargo test -- --ignored --nocapture
```

## Notes

- **Rate limits:** Tier 1 allows 200 RPM / 40,000 TPM. Tests should batch small requests and include delays between test categories.
- **Audio is expensive:** Audio token pricing is $32/$64 per 1M tokens (input/output). Keep test audio clips short (< 2 seconds).
- **Session reuse:** Some test categories can share a session (e.g., all text tests in one connection) to reduce overhead.
- **VAD sensitivity:** `semantic_vad` may behave differently with synthetic silence vs real silence. Use generated PCM16 silence for deterministic tests.
- **Voice lock:** Once the model emits audio, voice can't change. Tests that verify voice switching must do so before any audio response.
- **Interruption timing:** Cancel tests are timing-sensitive. Use `tokio::time::timeout` on `recv` loops to avoid hanging when cancellation doesn't fire `response.done`. Factor in network latency (50-200ms) when testing cancel-after-first-delta scenarios.
- **Response ID tracking:** The `response_id` from `response.created` must be captured and used for `response.cancel`. Tests that cancel must track IDs from prior events.
- **Audio truncation:** `response.cancel` with `sample_count` requires computing the sample offset from the playback position. Tests verifying truncation need to decode base64 audio and count samples.
