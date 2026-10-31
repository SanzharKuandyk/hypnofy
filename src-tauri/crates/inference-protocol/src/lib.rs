//! Versioned, length-prefixed IPC for the isolated native inference process.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

pub const VERSION: u32 = 2;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub job_id: String,
    pub operation: Operation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    Ping,
    Cancel,
    Shutdown,
    Generate {
        model_path: String,
        prompt: String,
        system_prompt: String,
        context_tokens: u32,
        device: Device,
    },
    Synthesize {
        model_path: String,
        architecture: Architecture,
        voice: String,
        language: String,
        speed: f32,
        text: String,
        device: Device,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    Auto,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    Kokoro,
    Supertonic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub version: u32,
    pub job_id: String,
    pub sequence: u32,
    pub payload: Payload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Payload {
    Ready,
    ModelInfo { context_tokens: u32 },
    TextDelta { text: String },
    TextCompleted { text: String },
    AudioCompleted { filename: String },
    Failed { code: String, message: String },
}

/// Read one frame. EOF between frames is clean; partial frames are errors.
pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<Option<T>> {
    let mut length = [0_u8; 4];
    match reader.read(&mut length[..1])? {
        0 => return Ok(None),
        1 => reader.read_exact(&mut length[1..])?,
        _ => unreachable!("one-byte read"),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid frame length",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Write and flush a frame without placing audio bytes on the control channel.
pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Frame too large",
        ));
    }
    let length = u32::try_from(bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Frame too large"))?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_multiple_frames_and_clean_eof() {
        let request = Request {
            version: VERSION,
            job_id: "job-1".into(),
            operation: Operation::Ping,
        };
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &request).unwrap();
        write_frame(&mut bytes, &request).unwrap();
        let mut reader = Cursor::new(bytes);
        assert_eq!(
            read_frame::<Request>(&mut reader).unwrap().unwrap().job_id,
            "job-1"
        );
        assert!(read_frame::<Request>(&mut reader).unwrap().is_some());
        assert!(read_frame::<Request>(&mut reader).unwrap().is_none());
    }

    #[test]
    fn rejects_truncation_and_oversized_frames() {
        assert!(read_frame::<Request>(&mut Cursor::new(vec![1, 0])).is_err());
        assert!(read_frame::<Request>(&mut Cursor::new(u32::MAX.to_le_bytes())).is_err());
        assert!(read_frame::<Request>(&mut Cursor::new(vec![2, 0, 0, 0, b'{'])).is_err());
    }
}
