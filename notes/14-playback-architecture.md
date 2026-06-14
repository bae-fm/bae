# Playback Architecture

## The pipeline

Every audio playback — main player, preview, local file, cloud file — uses the same pipeline:

```
Reader → SparseBuffer → decode_audio_streaming (via AVIO) → StreamingPcmSink/Source → cpal
```

- **Reader**: `LocalFileReader` or `CloudStorageReader`. Reads chunks of bytes and calls `buffer.append_at()`. That's all it does.
- **SparseBuffer**: Stores byte ranges. Supports non-contiguous data (sparse). `read()` blocks until data is available at the current read position. Doesn't care where bytes came from.
- **Decoder**: FFmpeg reads from SparseBuffer via a custom AVIO read callback. It blocks if data isn't there yet. It doesn't know whether the source is a local file or a cloud range request.
- **StreamingPcmSink/Source**: Lock-free ring buffer between decoder thread and cpal audio callback.

## Local and cloud are the same

The only difference between local and cloud is which `Reader` feeds bytes into the SparseBuffer. Everything downstream is identical. There is no special-casing for local vs cloud in the decoder, seek logic, or audio output.

Local files are NOT buffered entirely into RAM. They stream through the SparseBuffer in chunks, same as cloud. Local I/O is faster, so data arrives sooner, but the architecture doesn't assume it's instant.

## Seeking

Seeking works the same regardless of data source:

1. Cancel the old streaming source (makes the decoder's `push_samples_blocking` return immediately)
2. Cancel the SparseBuffer (unblocks the decoder if it's waiting for data in `buffer.read()`)
3. Wait for the old decoder thread to exit (join the thread handle)
4. Uncancel the SparseBuffer (so the new decoder can read from it)
5. Reset buffer read position to 0 (`buffer.seek(0)`)
6. If seeking forward past what's been downloaded: start a new reader at the target byte on the **same** SparseBuffer (it supports concurrent writes via mutex)
7. Start a new decoder with `seek_to` — FFmpeg opens the format context (reads headers from byte 0), then `avformat_seek_file` jumps to the target time
8. The AVIO read callback blocks until data at the seek position is available — normal buffering UX

The SparseBuffer retains all previously-downloaded data. Backward seeks into already-buffered regions are instant. Forward seeks past the download frontier block until the reader delivers data.

## Why one SparseBuffer per track

The SparseBuffer is created once when a track starts playing and retained for the entire playback session. On seek, the same buffer is reused — no new buffer, no duplicate data. The reader may still be writing to it (cloud download in progress), and that's fine.

## Preview vs main player

Preview uses the exact same pipeline. The only differences are:
- Preview uses a separate `AudioOutput` and cpal stream
- Preview has no queue, no preloading, no pregap logic
- Preview pauses the main player while active

The playback mechanics (buffer, decoder, seeking) are identical.

## Track extraction strategies

Tracks come in two flavors that need different decode handling, both
feeding the same pipeline:

**ByteRange** (CUE+FLAC). FLAC frames are independent — each decodable
without prior decoder state. At import we extract just the bytes for
one track from the source file (using the FLAC seektable) and prepend
synthetic FLAC headers, producing a buffer that looks like a
standalone FLAC file. Trimming happens at the byte level *before*
decoding. `samples_to_skip` handles leftover samples from FLAC frame
alignment. See "CUE/FLAC seeking" below for the seektable mechanics.

**FullFile** (per-track files; CUE+APE). The decoder reads the entire
source file:

- Per-track files (MP3, standalone FLAC): the whole file is one
  track, no trimming.
- CUE+APE: APE frames are stateful — each depends on the previous
  frame's decoder state, so byte-range extraction produces corrupt
  audio. The decoder sees the full file; `seek_to_sample` jumps
  FFmpeg to the track's start, `start_at_sample` trims lead-in (APE
  frames don't land on exact sample boundaries), `stop_at_sample`
  stops at the track end.

ByteRange trims at the byte level before decoding; FullFile trims at
the sample level after decoding (or doesn't trim at all for per-track
files).

Cache behavior differs: ByteRange buffers are track-specific (headers
+ bytes for that one track) and aren't reused. FullFile buffers hold
the entire source file and are reused across tracks from the same
file to avoid re-downloading.

## CUE/FLAC seeking

CUE-split tracks are reconstructed buffers: `[audio_headers][audio from file[start_byte..end_byte]]`. This creates two problems for FFmpeg seeking:

1. FLAC frames have **file-absolute** sample numbers (e.g., track 3's frames say sample 4,000,000, not sample 0)
2. The byte layout doesn't match the original file, so any seektable byte offsets would be wrong

We solve both:

- **Seek time**: Pass `track_start_time + user_offset` to `avformat_seek_file`. FFmpeg looks for the absolute sample number, which matches the FLAC frames in the buffer.
- **Seektable**: At import time, we build a dense seektable (~93ms precision, one entry per FLAC frame) and strip the embedded seektable from the FLAC headers. At playback time, we inject a new FLAC seektable metadata block into the headers with byte offsets adjusted for the reconstructed buffer layout: `adjusted_byte = audio_data_start + entry.byte - start_byte`. FFmpeg reads this seektable and jumps directly to the right byte position — no binary search.

This means CUE/FLAC seeking uses the same pipeline as regular files: cancel → join → uncancel → buffer.seek(0) → new decoder with `seek_to`. The seektable is baked into the headers that are already at position 0 in the buffer.
