use crate::cache::CacheManager;
use crate::cloud_storage::CloudStorageManager;
use crate::db::DbChunk;
use crate::encryption::EncryptionService;
use crate::library::LibraryManager;
use crate::playback::{PcmSource, PlaybackError};
use futures::stream::{self, StreamExt};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Reassemble chunks for a track and decode to PCM for playback
///
/// Returns decoded PCM audio ready for cpal output.
/// Uses libFLAC for decoding, which handles non-standard FLAC files better than symphonia.
pub async fn reassemble_track(
    track_id: &str,
    library_manager: &LibraryManager,
    cloud_storage: &CloudStorageManager,
    cache: &CacheManager,
    encryption_service: &EncryptionService,
    chunk_size_bytes: usize,
) -> Result<Arc<PcmSource>, PlaybackError> {
    info!("Reassembling chunks for track: {}", track_id);

    // Step 1: Get track chunk coordinates (has all location info)
    let coords = library_manager
        .get_track_chunk_coords(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Chunk coordinates", track_id))?;

    // Step 2: Get audio format (has FLAC headers if needed)
    let audio_format = library_manager
        .get_audio_format_by_track_id(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Audio format", track_id))?;

    debug!(
        "Track spans chunks {}-{} with byte offsets {}-{}",
        coords.start_chunk_index,
        coords.end_chunk_index,
        coords.start_byte_offset,
        coords.end_byte_offset
    );

    // Step 3: Get track to find release_id
    let track = library_manager
        .get_track(track_id)
        .await
        .map_err(PlaybackError::database)?
        .ok_or_else(|| PlaybackError::not_found("Track", track_id))?;

    // Step 4: Get all chunks in range
    let chunk_range = coords.start_chunk_index..=coords.end_chunk_index;
    let chunks = library_manager
        .get_chunks_in_range(&track.release_id, chunk_range)
        .await
        .map_err(PlaybackError::database)?;

    if chunks.is_empty() {
        return Err(PlaybackError::not_found("Chunks", track_id));
    }

    debug!("Found {} chunks to reassemble", chunks.len());

    // Sort chunks by index to ensure correct order
    let mut sorted_chunks = chunks;
    sorted_chunks.sort_by_key(|c| c.chunk_index);

    // Download and decrypt all chunks in parallel (max 10 concurrent)
    let chunk_results: Vec<Result<(i32, Vec<u8>), PlaybackError>> = stream::iter(sorted_chunks)
        .map(move |chunk| {
            let cloud_storage = cloud_storage.clone();
            let cache = cache.clone();
            let encryption_service = encryption_service.clone();
            async move {
                let chunk_data =
                    download_and_decrypt_chunk(&chunk, &cloud_storage, &cache, &encryption_service)
                        .await?;
                Ok::<_, PlaybackError>((chunk.chunk_index, chunk_data))
            }
        })
        .buffer_unordered(10) // Download up to 10 chunks concurrently
        .collect()
        .await;

    // Check for errors and collect indexed chunks
    let mut indexed_chunks: Vec<(i32, Vec<u8>)> = Vec::new();
    for result in chunk_results {
        indexed_chunks.push(result?);
    }

    // Sort by chunk index to ensure correct order (parallel downloads may complete out of order)
    indexed_chunks.sort_by_key(|(idx, _)| *idx);

    // Extract only the chunk data (without the index)
    let chunk_data: Vec<Vec<u8>> = indexed_chunks.into_iter().map(|(_, data)| data).collect();

    // Use byte offsets from coordinates to extract exactly the track data
    debug!(
        "Extracting track data: {} chunks, start_offset={}, end_offset={}, chunk_size={}",
        chunk_data.len(),
        coords.start_byte_offset,
        coords.end_byte_offset,
        chunk_size_bytes
    );
    let audio_data = extract_file_from_chunks(
        &chunk_data,
        coords.start_byte_offset,
        coords.end_byte_offset,
        chunk_size_bytes,
    );

    debug!(
        "Extracted {} bytes of audio data ({}MB)",
        audio_data.len(),
        audio_data.len() / 1_000_000
    );

    // Build complete FLAC data for decoding
    let flac_data = if audio_format.needs_headers {
        if let Some(ref headers) = audio_format.flac_headers {
            debug!("CUE/FLAC track: prepending headers for decode");
            let mut temp_flac = headers.clone();
            temp_flac.extend_from_slice(&audio_data);
            temp_flac
        } else {
            warn!("Audio format needs headers but none provided");
            audio_data
        }
    } else {
        // Regular FLAC file - data is already complete
        audio_data
    };

    // Decode FLAC to PCM using libFLAC
    debug!("Decoding {} bytes of FLAC data to PCM", flac_data.len());
    let decoded = decode_flac_to_pcm(&flac_data).await?;

    info!(
        "Successfully decoded track {}: {} samples, {}Hz, {} channels",
        track_id,
        decoded.samples.len(),
        decoded.sample_rate,
        decoded.channels
    );

    Ok(Arc::new(PcmSource::new(
        decoded.samples,
        decoded.sample_rate,
        decoded.channels,
        decoded.bits_per_sample,
    )))
}

/// Download and decrypt a single chunk with caching
async fn download_and_decrypt_chunk(
    chunk: &DbChunk,
    cloud_storage: &CloudStorageManager,
    cache: &CacheManager,
    encryption_service: &EncryptionService,
) -> Result<Vec<u8>, PlaybackError> {
    // Check cache first
    let encrypted_data = match cache.get_chunk(&chunk.id).await {
        Ok(Some(cached_encrypted_data)) => {
            debug!("Cache hit for chunk: {}", chunk.id);
            cached_encrypted_data
        }
        Ok(None) => {
            debug!("Cache miss - downloading chunk from cloud: {}", chunk.id);
            // Download from cloud storage
            let data = cloud_storage
                .download_chunk(&chunk.storage_location)
                .await
                .map_err(PlaybackError::cloud)?;

            // Cache the encrypted data for future use
            if let Err(e) = cache.put_chunk(&chunk.id, &data).await {
                warn!("Failed to cache chunk (non-fatal): {}", e);
            }
            data
        }
        Err(e) => {
            warn!("Cache error (continuing with download): {}", e);
            // Download from cloud storage
            cloud_storage
                .download_chunk(&chunk.storage_location)
                .await
                .map_err(PlaybackError::cloud)?
        }
    };

    // Decrypt in spawn_blocking to avoid blocking the async runtime
    // Uses decrypt_simple since storage uses simple nonce+ciphertext format
    let encryption_service = encryption_service.clone();
    let decrypted_data = tokio::task::spawn_blocking(move || {
        encryption_service
            .decrypt_simple(&encrypted_data)
            .map_err(PlaybackError::decrypt)
    })
    .await
    .map_err(PlaybackError::task)??;

    Ok(decrypted_data)
}

/// Extract file data from chunks using byte offsets
///
/// Given a list of chunks and the file's byte offsets within those chunks,
/// this function extracts exactly the bytes that belong to the file.
///
/// # Arguments
/// * `chunks` - Decrypted chunk data in order (chunk 0, chunk 1, chunk 2, ...)
/// * `start_byte_offset` - Byte offset within the first chunk where the file starts
/// * `end_byte_offset` - Byte offset within the last chunk where the file ends (inclusive)
/// * `chunk_size` - Size of each chunk in bytes
///
/// # Returns
/// The extracted file data
fn extract_file_from_chunks(
    chunks: &[Vec<u8>],
    start_byte_offset: i64,
    end_byte_offset: i64,
    _chunk_size: usize,
) -> Vec<u8> {
    if chunks.is_empty() {
        return Vec::new();
    }

    let mut file_data = Vec::new();

    if chunks.len() == 1 {
        // File is entirely within a single chunk
        let start = start_byte_offset as usize;
        let end = ((end_byte_offset + 1) as usize).min(chunks[0].len()); // end_byte_offset is inclusive
        file_data.extend_from_slice(&chunks[0][start..end]);
    } else {
        // File spans multiple chunks
        // First chunk: from start_byte_offset to end of chunk
        let first_chunk_start = start_byte_offset as usize;
        file_data.extend_from_slice(&chunks[0][first_chunk_start..]);

        // Middle chunks: use entirely
        for chunk in &chunks[1..chunks.len() - 1] {
            file_data.extend_from_slice(chunk);
        }

        // Last chunk: from start to end_byte_offset
        let last_chunk = &chunks[chunks.len() - 1];
        let last_chunk_end = ((end_byte_offset + 1) as usize).min(last_chunk.len()); // end_byte_offset is inclusive
        file_data.extend_from_slice(&last_chunk[0..last_chunk_end]);
    }

    file_data
}

/// Decode FLAC data to PCM using libFLAC
pub(crate) async fn decode_flac_to_pcm(
    flac_data: &[u8],
) -> Result<crate::flac_decoder::DecodedFlac, PlaybackError> {
    let flac_data = flac_data.to_vec();
    tokio::task::spawn_blocking(move || {
        crate::flac_decoder::decode_flac_range(&flac_data, None, None)
    })
    .await
    .map_err(PlaybackError::task)?
    .map_err(PlaybackError::flac)
}
