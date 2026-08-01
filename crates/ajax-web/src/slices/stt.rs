//! Versioned STT control/event wire types and bounded binary audio framing.

use serde::{Deserialize, Serialize};

pub const STT_PROTOCOL_VERSION: u32 = 1;

/// Maximum PCM16 payload bytes per binary audio frame (20 ms at 16 kHz mono).
pub const MAX_AUDIO_FRAME_BYTES: usize = 640;

const AUDIO_SEQUENCE_PREFIX_BYTES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SttClientMessage {
    #[serde(rename = "stt.start")]
    Start {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        encoding: String,
        #[serde(rename = "sampleRate")]
        sample_rate: u32,
        channels: u32,
    },
    #[serde(rename = "stt.stop")]
    Stop {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "stt.cancel")]
    Cancel {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SttServerEvent {
    #[serde(rename = "stt.ready")]
    Ready {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "pauseGracePeriodMs")]
        pause_grace_period_ms: u64,
        #[serde(rename = "finalizationTimeoutMs")]
        finalization_timeout_ms: u64,
    },
    #[serde(rename = "stt.partial")]
    Partial {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        sequence: u32,
        text: String,
    },
    #[serde(rename = "stt.final")]
    Final {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        sequence: u32,
        text: String,
    },
    #[serde(rename = "stt.speech_started")]
    SpeechStarted {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "stt.speech_ended")]
    SpeechEnded {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "stt.error")]
    Error {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
        code: String,
        message: String,
    },
    #[serde(rename = "stt.closed")]
    Closed {
        version: u32,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFrameError {
    Truncated,
    TooLarge,
}

pub fn encode_audio_frame(sequence: u32, pcm: &[u8]) -> Result<Vec<u8>, AudioFrameError> {
    if pcm.len() > MAX_AUDIO_FRAME_BYTES {
        return Err(AudioFrameError::TooLarge);
    }

    let mut frame = Vec::with_capacity(AUDIO_SEQUENCE_PREFIX_BYTES + pcm.len());
    frame.extend_from_slice(&sequence.to_be_bytes());
    frame.extend_from_slice(pcm);
    Ok(frame)
}

pub fn decode_audio_frame(frame: &[u8]) -> Result<(u32, &[u8]), AudioFrameError> {
    if frame.len() < AUDIO_SEQUENCE_PREFIX_BYTES {
        return Err(AudioFrameError::Truncated);
    }

    let sequence = u32::from_be_bytes(
        frame[..AUDIO_SEQUENCE_PREFIX_BYTES]
            .try_into()
            .expect("sequence prefix length checked"),
    );
    let pcm = &frame[AUDIO_SEQUENCE_PREFIX_BYTES..];
    Ok((sequence, pcm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_controls_use_versioned_stt_names_and_camel_case_fields() {
        let message = SttClientMessage::Start {
            version: STT_PROTOCOL_VERSION,
            session_id: "session-1".to_string(),
            encoding: "pcm16".to_string(),
            sample_rate: 16_000,
            channels: 1,
        };

        let json = serde_json::to_value(&message).expect("serialize start");

        assert_eq!(json["type"], "stt.start");
        assert_eq!(json["version"], STT_PROTOCOL_VERSION);
        assert_eq!(json["sessionId"], "session-1");
        assert_eq!(json["sampleRate"], 16_000);
        assert!(json.get("session_id").is_none());
        assert!(json.get("language").is_none());
    }

    #[test]
    fn ready_event_serializes_pause_grace_period_ms() {
        let event = SttServerEvent::Ready {
            version: STT_PROTOCOL_VERSION,
            session_id: "session-1".to_string(),
            pause_grace_period_ms: 4_000,
            finalization_timeout_ms: 5_000,
        };

        let json = serde_json::to_value(&event).expect("serialize ready");

        assert_eq!(json["type"], "stt.ready");
        assert_eq!(json["pauseGracePeriodMs"], 4_000);
        assert!(json.get("pause_grace_period_ms").is_none());
        assert_eq!(json["finalizationTimeoutMs"], 5_000);
        assert!(json.get("finalization_timeout_ms").is_none());
    }

    #[test]
    fn server_events_round_trip_sequence_and_error_fields() {
        let events = [
            SttServerEvent::Final {
                version: STT_PROTOCOL_VERSION,
                session_id: "session-1".to_string(),
                sequence: 7,
                text: "Inspect the adapter.".to_string(),
            },
            SttServerEvent::Error {
                version: STT_PROTOCOL_VERSION,
                session_id: "session-1".to_string(),
                code: "provider_unavailable".to_string(),
                message: "Local speech recognition is unavailable.".to_string(),
            },
            SttServerEvent::Closed {
                version: STT_PROTOCOL_VERSION,
                session_id: "session-1".to_string(),
            },
        ];

        for event in events {
            let encoded = serde_json::to_vec(&event).expect("serialize event");
            let decoded: SttServerEvent =
                serde_json::from_slice(&encoded).expect("deserialize event");
            assert_eq!(decoded, event);
        }
    }

    #[test]
    fn audio_frames_round_trip_sequence_and_reject_overflow_or_truncation() {
        let pcm = vec![1, 2, 3, 4];
        let frame = encode_audio_frame(42, &pcm).expect("encode audio");

        assert_eq!(
            decode_audio_frame(&frame).expect("decode audio"),
            (42, pcm.as_slice())
        );
        assert!(matches!(
            decode_audio_frame(&[0, 0, 0]),
            Err(AudioFrameError::Truncated)
        ));
        assert!(encode_audio_frame(43, &vec![0; MAX_AUDIO_FRAME_BYTES]).is_ok());
        assert!(matches!(
            encode_audio_frame(43, &vec![0; MAX_AUDIO_FRAME_BYTES + 1]),
            Err(AudioFrameError::TooLarge)
        ));
    }
}
