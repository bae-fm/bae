use std::future::{poll_fn, Future};
use std::io::Cursor;
use std::pin::pin;
use std::task::Poll;

use tokio::io::{AsyncWriteExt, DuplexStream};

use super::*;

fn message(payload: &str) -> CastMessage {
    CastMessage::text("sender-0", "receiver-0", "urn:x-cast:test", payload.to_string())
}

fn frame_of_body(body: &[u8]) -> Vec<u8> {
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    frame
}

fn reader_over(bytes: Vec<u8>) -> FrameReader<Cursor<Vec<u8>>> {
    FrameReader::new(Cursor::new(bytes))
}

#[tokio::test]
async fn a_frame_round_trips_through_the_encoder_and_the_reader() {
    let sent = CastMessage {
        continued: Some(false),
        remaining_length: Some(7),
        ..message(r#"{"type":"PING"}"#)
    };
    let mut reader = reader_over(encode_frame(&sent).unwrap());
    assert_eq!(reader.next().await.unwrap(), Some(sent));
    assert!(reader.next().await.unwrap().is_none());
}

#[tokio::test]
async fn a_binary_payload_round_trips() {
    let sent = CastMessage {
        payload_type: PayloadType::Binary as i32,
        payload_utf8: None,
        payload_binary: Some(vec![0, 1, 2, 255]),
        ..message("")
    };
    let mut reader = reader_over(encode_frame(&sent).unwrap());
    assert_eq!(reader.next().await.unwrap(), Some(sent));
}

#[test]
fn the_encoder_refuses_a_body_over_the_longest_frame() {
    let too_long = message(&"x".repeat(MAX_BODY_LEN));
    assert!(
        matches!(encode_frame(&too_long), Err(WireError::BodyTooLong(len)) if len > MAX_BODY_LEN)
    );
}

#[tokio::test]
async fn a_frame_stating_a_body_over_the_longest_ends_the_reader_and_nothing_past_it_is_read() {
    let mut bytes = ((MAX_BODY_LEN + 1) as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(&encode_frame(&message("after")).unwrap());
    let mut reader = reader_over(bytes);

    assert!(
        matches!(reader.next().await, Err(WireError::BodyTooLong(len)) if len == MAX_BODY_LEN + 1)
    );
    assert_eq!(reader.reader.position(), HEADER_LEN as u64, "only the header was read");
    assert!(matches!(reader.next().await, Err(WireError::Stopped)));
    assert_eq!(reader.reader.position(), HEADER_LEN as u64);
}

#[tokio::test]
async fn a_body_of_exactly_the_longest_is_read_to_its_end() {
    let mut bytes = (MAX_BODY_LEN as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(&[0u8; 10]);
    let mut reader = reader_over(bytes);
    assert!(matches!(reader.next().await, Err(WireError::Truncated)));
}

#[tokio::test]
async fn a_body_that_is_not_a_cast_message_ends_the_reader() {
    let mut bytes = frame_of_body(&[0xFF; 12]);
    bytes.extend_from_slice(&encode_frame(&message("after")).unwrap());
    let mut reader = reader_over(bytes);

    assert!(matches!(reader.next().await, Err(WireError::NotACastMessage(_))));
    assert!(
        matches!(reader.next().await, Err(WireError::Stopped)),
        "the frame after it is not read"
    );
}

#[tokio::test]
async fn a_chunked_frame_ends_the_reader() {
    let chunk = CastMessage { continued: Some(true), remaining_length: Some(10), ..message("x") };
    let mut reader = reader_over(encode_frame(&chunk).unwrap());
    assert!(matches!(reader.next().await, Err(WireError::Chunked)));
    assert!(matches!(reader.next().await, Err(WireError::Stopped)));
}

#[tokio::test]
async fn several_frames_in_one_read_come_out_whole_and_in_order() {
    let sent: Vec<CastMessage> = ["one", "two", "three"].into_iter().map(message).collect();
    let bytes: Vec<u8> = sent.iter().flat_map(|m| encode_frame(m).unwrap()).collect();
    let mut reader = reader_over(bytes);
    for expected in sent {
        assert_eq!(reader.next().await.unwrap(), Some(expected));
    }
    assert!(reader.next().await.unwrap().is_none());
}

#[tokio::test]
async fn a_frame_split_over_reads_comes_out_whole() {
    let sent = [message("first"), message("second")];
    let bytes: Vec<u8> = sent.iter().flat_map(|m| encode_frame(m).unwrap()).collect();
    // A pipe that holds one byte at a time hands the frames over in 1-byte reads.
    let (mut writer, read_half) = tokio::io::duplex(1);
    let writing = async move {
        writer.write_all(&bytes).await.unwrap();
        // The writer is dropped here, so the reader sees the end.
    };
    let reading = async move {
        let mut reader = FrameReader::new(read_half);
        for expected in sent {
            assert_eq!(reader.next().await.unwrap(), Some(expected));
        }
        assert!(reader.next().await.unwrap().is_none());
    };
    tokio::join!(writing, reading);
}

#[tokio::test]
async fn a_frame_read_just_before_the_end_is_handed_on_then_the_end_is_reported() {
    let sent = message("last words");
    let mut reader = reader_over(encode_frame(&sent).unwrap());
    assert_eq!(reader.next().await.unwrap(), Some(sent));
    assert!(reader.next().await.unwrap().is_none());
}

#[tokio::test]
async fn the_stream_ending_inside_a_frame_is_an_error_not_an_end() {
    let whole = encode_frame(&message("cut short")).unwrap();
    for cut in [1, HEADER_LEN, whole.len() - 1] {
        let mut reader = reader_over(whole[..cut].to_vec());
        assert!(matches!(reader.next().await, Err(WireError::Truncated)), "cut at {cut}");
    }
}

#[tokio::test]
async fn a_read_given_up_on_halfway_loses_nothing() {
    let sent = message("kept");
    let bytes = encode_frame(&sent).unwrap();
    let (mut writer, read_half): (DuplexStream, DuplexStream) = tokio::io::duplex(1024);
    let mut reader = FrameReader::new(read_half);

    writer.write_all(&bytes[..6]).await.unwrap();
    {
        let mut waiting = pin!(reader.next());
        let pending = poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx).is_pending())).await;
        assert!(pending, "half a frame is not a message");
    }
    writer.write_all(&bytes[6..]).await.unwrap();
    assert_eq!(reader.next().await.unwrap(), Some(sent));
}
