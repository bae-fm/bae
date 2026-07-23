//! Media endpoints: audio streaming (raw or transcoded), cover art, and the
//! scrobble no-op.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use bae_core::audio_codec::{decode_audio_to_sink, StreamEncodeFormat, StreamingEncoder};
use bae_core::config::SaveCodec;
use bae_core::db::LibraryImageType;
use bae_core::library::LibraryManager;
use bae_core::playback::data_source::{create_audio_reader, FetchArbiter};
use bae_core::playback::sparse_buffer::create_sparse_buffer;
use bae_core::playback::SharedSparseBuffer;
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{debug, warn};

use crate::endpoints::respond;
use crate::error::SubError;
use crate::id::SubId;
use crate::library_map::lib_err;
use crate::params::Params;
use crate::AppState;

/// How many bytes each read/encode step hands to the response body at a time.
const STREAM_CHUNK: usize = 64 * 1024;

/// `stream` — serve a track's audio. With `format=raw` (or no transcode
/// parameters) the backing file's original bytes are served with HTTP range
/// support; otherwise the track is transcoded and streamed chunked.
pub(crate) async fn stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    let format = params.format();
    match stream_inner(&state, &params, &headers).await {
        Ok(response) => response,
        Err(error) => crate::envelope::error_response(&format, &error),
    }
}

async fn stream_inner(
    state: &AppState,
    params: &Params,
    headers: &HeaderMap,
) -> Result<Response, SubError> {
    let manager = &state.manager;
    let track_id = SubId::parse(params.require("id")?)?
        .expect_track()?
        .to_string();

    let existing = manager
        .filter_existing_track_ids(std::slice::from_ref(&track_id))
        .await
        .map_err(lib_err)?;
    if existing.is_empty() {
        return Err(SubError::not_found());
    }

    let requested_format = params.get("format");
    let max_bitrate = params.int("maxBitRate")?.filter(|&b| b > 0);
    let wants_original =
        requested_format == Some("raw") || (requested_format.is_none() && max_bitrate.is_none());

    if wants_original {
        stream_raw(manager, &track_id, headers).await
    } else {
        let estimate = params.bool_or("estimateContentLength", false);
        stream_transcode(manager, &track_id, requested_format, max_bitrate, estimate).await
    }
}

/// Serve the track's backing file bytes, honoring an HTTP `Range` request.
async fn stream_raw(
    manager: &LibraryManager,
    track_id: &str,
    headers: &HeaderMap,
) -> Result<Response, SubError> {
    let audio = manager
        .resolve_track_audio(track_id)
        .await
        .map_err(lib_err)?;
    // A raw serve streams the whole backing file — for a CUE-image track that
    // is the image itself, an accepted edge (raw is normally used with a
    // per-track file; clients wanting exact track bounds request a transcode).
    let segment = audio.segments.first().ok_or_else(SubError::not_found)?;
    let file = manager
        .get_file_by_id(&segment.file_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;

    let total = u64::try_from(file.file_size)
        .map_err(|_| SubError::generic(format!("file {} has a negative size", file.id)))?;
    let content_type = file.content_type.as_str().to_string();

    let buffer = open_file_stream(
        manager,
        &segment.file_id,
        segment.cloud_path.as_deref(),
        total,
    );

    match parse_range(headers, total) {
        Some((start, end_inclusive)) => {
            let len = end_inclusive - start + 1;
            let body = reader_body(buffer, start, len);
            Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(CONTENT_TYPE, content_type)
                .header(ACCEPT_RANGES, "bytes")
                .header(CONTENT_LENGTH, len)
                .header(
                    CONTENT_RANGE,
                    format!("bytes {start}-{end_inclusive}/{total}"),
                )
                .body(body)
                .expect("range response builds"))
        }
        None => {
            let body = reader_body(buffer, 0, total);
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(CONTENT_TYPE, content_type)
                .header(ACCEPT_RANGES, "bytes")
                .header(CONTENT_LENGTH, total)
                .body(body)
                .expect("full response builds"))
        }
    }
}

/// Open a sparse buffer over one release file, filled on demand through coven's
/// locality-aware read (the user's own file, the cache, or the cloud).
fn open_file_stream(
    manager: &LibraryManager,
    file_id: &str,
    cloud_path: Option<&str>,
    size: u64,
) -> SharedSparseBuffer {
    let buffer = create_sparse_buffer(size);
    let reader = create_audio_reader(
        manager,
        file_id,
        cloud_path,
        size,
        FetchArbiter::new(),
        None,
        false,
    );
    let file_id = file_id.to_string();
    reader.start_reading(
        buffer.clone(),
        Box::new(move |error| {
            warn!("subsonic stream: reading release file {file_id} failed: {error}");
        }),
    );
    buffer
}

/// Stream `len` bytes of `buffer` from `start` as an HTTP body. The blocking
/// reads run on the blocking pool; each chunk is handed to the response through
/// a bounded channel for backpressure.
fn reader_body(buffer: SharedSparseBuffer, start: u64, len: u64) -> Body {
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    tokio::task::spawn_blocking(move || {
        let mut reader = buffer.new_reader();
        if start > 0 && !reader.seek(start) {
            let _ = tx.blocking_send(Err(std::io::Error::other(format!(
                "failed to seek stream to byte {start}"
            ))));
            return;
        }
        let mut remaining = len;
        let mut chunk = vec![0u8; STREAM_CHUNK];
        while remaining > 0 {
            let want = remaining.min(chunk.len() as u64) as usize;
            match reader.read(&mut chunk[..want]) {
                // EOF or a cancelled buffer (the reader logs the cause): end the
                // body. A short body signals the truncation to the client.
                Some(0) | None => break,
                Some(n) => {
                    remaining -= n as u64;
                    if tx
                        .blocking_send(Ok(Bytes::copy_from_slice(&chunk[..n])))
                        .is_err()
                    {
                        break; // client hung up
                    }
                }
            }
        }
    });
    Body::from_stream(ReceiverStream::new(rx))
}

/// Transcode the track and stream the encoded bytes chunked. The encode runs on
/// the blocking pool, writing frame by frame into a non-seekable sink (the
/// response channel) — a socket can't patch a header, which is why the encoder's
/// streaming constructor is required.
async fn stream_transcode(
    manager: &LibraryManager,
    track_id: &str,
    requested_format: Option<&str>,
    max_bitrate: Option<i64>,
    estimate_length: bool,
) -> Result<Response, SubError> {
    let audio = manager
        .resolve_track_audio(track_id)
        .await
        .map_err(lib_err)?;
    // A single-track stream never uses preset-level splitting/filename/pregap
    // policy, so it takes a plain SaveCodec, not a SavePreset.
    let codec = transcode_codec(requested_format, max_bitrate);
    let (encode_format, content_type) = stream_encode_format(&codec)?;

    // One sparse buffer per distinct backing file; a CUE-image track's segments
    // share a file, so it opens once.
    let mut buffers: Vec<(String, SharedSparseBuffer)> = Vec::new();
    for segment in &audio.segments {
        if buffers.iter().any(|(id, _)| id == &segment.file_id) {
            continue;
        }
        let file = manager
            .get_file_by_id(&segment.file_id)
            .await
            .map_err(lib_err)?
            .ok_or_else(SubError::not_found)?;
        let size = u64::try_from(file.file_size)
            .map_err(|_| SubError::generic(format!("file {} has a negative size", file.id)))?;
        buffers.push((
            segment.file_id.clone(),
            open_file_stream(manager, &segment.file_id, file.cloud_path.as_deref(), size),
        ));
    }

    let segments: Vec<(SharedSparseBuffer, Option<u64>, Option<u64>)> = audio
        .segments
        .iter()
        .map(|segment| {
            let buffer = buffers
                .iter()
                .find(|(id, _)| id == &segment.file_id)
                .map(|(_, buffer)| buffer.clone())
                .expect("every segment's file was opened above");
            (
                buffer,
                Some(segment.span.start_sample),
                segment.span.end_sample,
            )
        })
        .collect();

    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(4);
    let cancel = Arc::new(AtomicBool::new(false));
    let track_id_owned = track_id.to_string();
    tokio::task::spawn_blocking(move || {
        let sink = ChannelSink { tx: tx.clone() };
        let mut encoder =
            StreamingEncoder::streaming(encode_format, Box::new(sink), cancel.clone());
        for (buffer, start, end) in segments {
            if let Err(error) =
                decode_audio_to_sink(buffer, start, end, &mut encoder, cancel.clone())
            {
                warn!("subsonic transcode of {track_id_owned} failed to decode: {error}");
                return; // dropping tx ends the body
            }
        }
        if let Err(error) = encoder.finish() {
            warn!("subsonic transcode of {track_id_owned} failed to finish: {error}");
        }
    });

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type);
    if estimate_length {
        if let Some(duration_ms) = audio.duration_ms {
            // Estimate ≈ duration × bitrate. Bytes = seconds × kbps × 1000 / 8.
            let bytes = (duration_ms.max(0) as u64)
                .saturating_mul(u64::from(codec_bitrate_kbps(&codec)))
                * 1000
                / 8
                / 1000;
            builder = builder.header(CONTENT_LENGTH, bytes);
        }
    }
    Ok(builder
        .body(Body::from_stream(ReceiverStream::new(rx)))
        .expect("transcode response builds"))
}

/// The transcode codec for the requested format and bitrate. An unknown or
/// absent format defaults to MP3 at 128 kbps — the Subsonic default transcode.
fn transcode_codec(requested_format: Option<&str>, max_bitrate: Option<i64>) -> SaveCodec {
    let bitrate = max_bitrate.filter(|&b| b > 0).unwrap_or(128) as u32;
    match requested_format {
        Some("opus") | Some("ogg") => SaveCodec::OpusOgg {
            bitrate_kbps: bitrate.clamp(32, 512),
        },
        _ => SaveCodec::Mp3 {
            bitrate_kbps: bitrate.clamp(32, 320),
        },
    }
}

/// Map a save codec to the streaming encoder format and the response MIME type.
/// Only the two streaming-safe codecs a transcode builds are accepted.
fn stream_encode_format(codec: &SaveCodec) -> Result<(StreamEncodeFormat, &'static str), SubError> {
    match codec {
        SaveCodec::Mp3 { bitrate_kbps } => Ok((
            StreamEncodeFormat::Mp3 {
                bitrate_kbps: *bitrate_kbps,
            },
            "audio/mpeg",
        )),
        SaveCodec::OpusOgg { bitrate_kbps } => Ok((
            StreamEncodeFormat::OpusOgg {
                bitrate_kbps: *bitrate_kbps,
            },
            "audio/ogg",
        )),
        SaveCodec::Flac { .. } | SaveCodec::Wav { .. } | SaveCodec::Aiff { .. } => Err(
            SubError::generic("stream transcode supports only mp3 and opus"),
        ),
    }
}

fn codec_bitrate_kbps(codec: &SaveCodec) -> u32 {
    match codec {
        SaveCodec::Mp3 { bitrate_kbps } | SaveCodec::OpusOgg { bitrate_kbps } => *bitrate_kbps,
        SaveCodec::Flac { .. } | SaveCodec::Wav { .. } | SaveCodec::Aiff { .. } => 128,
    }
}

/// A non-seekable `Write` sink that hands each encoded chunk to the response
/// channel. Runs on the blocking encode task, so it uses `blocking_send`.
struct ChannelSink {
    tx: mpsc::Sender<Result<Bytes, std::io::Error>>,
}

impl std::io::Write for ChannelSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // A closed receiver (client hung up) surfaces as a broken pipe, which
        // stops the encode rather than being silently dropped.
        self.tx
            .blocking_send(Ok(Bytes::copy_from_slice(buf)))
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Parse a single-range `Range: bytes=start-end` header against `total`. Only
/// the first range is honored; a malformed or unsatisfiable range serves the
/// whole file (Subsonic clients send simple single ranges).
fn parse_range(headers: &HeaderMap, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }
    let value = headers.get(RANGE)?.to_str().ok()?;
    let spec = value.strip_prefix("bytes=")?;
    let (start_raw, end_raw) = spec.split_once('-')?;
    let start: u64 = start_raw.trim().parse().ok()?;
    let end = match end_raw.trim() {
        "" => total - 1,
        raw => raw.parse::<u64>().ok()?.min(total - 1),
    };
    if start > end {
        return None;
    }
    Some((start, end))
}

/// `getCoverArt` — serve a release's cover image. Any namespaced id resolves to
/// a release (an album id directly; an artist or track to a release of theirs).
pub(crate) async fn get_cover_art(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let params = Params(params);
    let format = params.format();
    match cover_art_inner(&state, &params).await {
        Ok(response) => response,
        Err(error) => crate::envelope::error_response(&format, &error),
    }
}

async fn cover_art_inner(state: &AppState, params: &Params) -> Result<Response, SubError> {
    let manager = &state.manager;
    let release_id = cover_release_id(manager, params.require("id")?).await?;

    // `size` scaling isn't applied: covers are stored pre-resized, so the stored
    // image is served at its stored size.
    if params.get("size").is_some() {
        debug!("getCoverArt: size scaling is not applied; serving the stored cover");
    }

    let row = manager
        .get_library_image(&release_id, &LibraryImageType::Cover)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;
    let bytes = manager
        .read_cover_image_blob(&release_id)
        .await
        .map_err(lib_err)?
        .ok_or_else(SubError::not_found)?;

    Ok((
        [(CONTENT_TYPE, row.content_type.as_str().to_string())],
        bytes,
    )
        .into_response())
}

/// Resolve any namespaced id to the release whose cover to serve.
async fn cover_release_id(manager: &LibraryManager, raw_id: &str) -> Result<String, SubError> {
    match SubId::parse(raw_id)? {
        SubId::Album(release_id) => Ok(release_id),
        SubId::Track(track_id) => {
            let info = manager
                .get_playback_track_info(&track_id)
                .await
                .map_err(lib_err)?;
            Ok(info.release_id)
        }
        SubId::Artist(artist_id) => {
            let detail = manager
                .get_artist_detail(&artist_id)
                .await
                .map_err(lib_err)?
                .ok_or_else(SubError::not_found)?;
            // An artist has no cover of its own here; use the first release of
            // their first album.
            detail
                .albums
                .first()
                .and_then(|album| album.release_ids.first().cloned())
                .ok_or_else(SubError::not_found)
        }
    }
}

/// `scrobble` — accepted and ignored. bae keeps no play counts, so there is
/// nothing to record; returning ok keeps clients that scrobble on play happy.
pub(crate) async fn scrobble(Query(params): Query<HashMap<String, String>>) -> Response {
    let params = Params(params);
    if let Err(error) = params.require("id") {
        return crate::envelope::error_response(&params.format(), &error);
    }
    debug!("scrobble accepted and ignored (no play-count store)");
    respond(&params.format(), Ok(None))
}
