//! Cast frames: the `CastMessage` protobuf and the length-prefixed frame it
//! travels in.
//!
//! A frame is a 4-byte big-endian body length, then the body, a protobuf
//! `CastMessage`. The fields are those of openscreen's `cast_channel.proto`,
//! derived with prost, so there is no code generation step. [`FrameReader`]
//! yields whole messages from any byte stream; a frame it refuses leaves it
//! stopped for good.

use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt};

/// The longest body a frame may state: a 64 KiB frame less its 4-byte header.
pub(crate) const MAX_BODY_LEN: usize = 65_532;

const HEADER_LEN: usize = 4;

/// The protocol version bae declares. A receiver sends chunked messages
/// (`continued`) only to senders that declared a later one.
pub(crate) const PROTOCOL_VERSION_1_0: i32 = 0;

/// Which of a message's two payload fields holds its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub(crate) enum PayloadType {
    String = 0,
    Binary = 1,
}

/// The protobuf message every Cast frame carries.
#[derive(Clone, PartialEq, prost::Message)]
pub(crate) struct CastMessage {
    #[prost(int32, required, tag = "1")]
    pub(crate) protocol_version: i32,
    #[prost(string, required, tag = "2")]
    pub(crate) source_id: String,
    #[prost(string, required, tag = "3")]
    pub(crate) destination_id: String,
    #[prost(string, required, tag = "4")]
    pub(crate) namespace: String,
    #[prost(enumeration = "PayloadType", required, tag = "5")]
    pub(crate) payload_type: i32,
    #[prost(string, optional, tag = "6")]
    pub(crate) payload_utf8: Option<String>,
    #[prost(bytes = "vec", optional, tag = "7")]
    pub(crate) payload_binary: Option<Vec<u8>>,
    #[prost(bool, optional, tag = "8")]
    pub(crate) continued: Option<bool>,
    #[prost(uint32, optional, tag = "9")]
    pub(crate) remaining_length: Option<u32>,
}

impl CastMessage {
    /// A text-payload message of protocol version 1.0.
    pub(crate) fn text(
        source_id: &str,
        destination_id: &str,
        namespace: &str,
        payload: String,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_1_0,
            source_id: source_id.to_string(),
            destination_id: destination_id.to_string(),
            namespace: namespace.to_string(),
            payload_type: PayloadType::String as i32,
            payload_utf8: Some(payload),
            payload_binary: None,
            continued: None,
            remaining_length: None,
        }
    }
}

/// Why a frame could not be written or read. Every one but a clean end of the
/// stream ends the connection: nothing can be said about what follows a frame
/// that was refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WireError {
    #[error("a frame states a body of {0} bytes, over the {MAX_BODY_LEN} a frame may hold")]
    BodyTooLong(usize),
    #[error("a frame's body is not a CastMessage: {0}")]
    NotACastMessage(#[from] prost::DecodeError),
    #[error("a frame is a chunk of a longer message, which a protocol 1.0 sender is never sent")]
    Chunked,
    #[error("the stream ended inside a frame")]
    Truncated,
    #[error("reading the stream failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("the reader stopped at an earlier refused frame")]
    Stopped,
}

/// The bytes of one frame: the body's length, then the body.
pub(crate) fn encode_frame(message: &CastMessage) -> Result<Vec<u8>, WireError> {
    let body_len = message.encoded_len();
    if body_len > MAX_BODY_LEN {
        return Err(WireError::BodyTooLong(body_len));
    }
    let mut frame = Vec::with_capacity(HEADER_LEN + body_len);
    frame.extend_from_slice(&(body_len as u32).to_be_bytes());
    message.encode(&mut frame).expect("a Vec grows to hold the message it was sized for");
    Ok(frame)
}

/// Reads whole messages off a byte stream, in the order they were sent.
pub(crate) struct FrameReader<R> {
    reader: R,
    /// The bytes of the frame being read so far.
    pending: Vec<u8>,
    stopped: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    pub(crate) fn new(reader: R) -> Self {
        Self { reader, pending: Vec::new(), stopped: false }
    }

    /// The next message, `None` when the stream ended between frames. A frame
    /// read just before the end is returned first. After an error every call
    /// is [`WireError::Stopped`], and nothing more is read.
    ///
    /// Cancel safe: a frame partly read is kept for the next call. It reads
    /// only the bytes the frame in hand still needs, so none past a frame it
    /// refuses are taken off the stream.
    pub(crate) async fn next(&mut self) -> Result<Option<CastMessage>, WireError> {
        if self.stopped {
            return Err(WireError::Stopped);
        }
        let read = self.read_frame().await;
        if read.is_err() {
            self.stopped = true;
        }
        read
    }

    async fn read_frame(&mut self) -> Result<Option<CastMessage>, WireError> {
        loop {
            let needed = self.bytes_needed()?;
            if needed == 0 {
                return self.take_frame().map(Some);
            }
            let read = (&mut self.reader).take(needed as u64).read_buf(&mut self.pending).await?;
            if read == 0 {
                return if self.pending.is_empty() { Ok(None) } else { Err(WireError::Truncated) };
            }
        }
    }

    /// How many more bytes the frame in hand needs to be whole, 0 when it is.
    fn bytes_needed(&self) -> Result<usize, WireError> {
        let Some(header) = self.pending.first_chunk::<HEADER_LEN>() else {
            return Ok(HEADER_LEN - self.pending.len());
        };
        let body_len = u32::from_be_bytes(*header) as usize;
        if body_len > MAX_BODY_LEN {
            return Err(WireError::BodyTooLong(body_len));
        }
        Ok(HEADER_LEN + body_len - self.pending.len())
    }

    fn take_frame(&mut self) -> Result<CastMessage, WireError> {
        let message = CastMessage::decode(&self.pending[HEADER_LEN..])?;
        self.pending.clear();
        if message.continued == Some(true) {
            return Err(WireError::Chunked);
        }
        Ok(message)
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod tests;
