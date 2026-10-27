# FFmpeg seek behavior, per format bae supports

How FFmpeg's demuxers satisfy a seek (`av_seek_frame` / `avformat_seek_file`),
for every audio format bae decodes. This matters because bae plays remote
releases by streaming the encrypted blob through a custom AVIO over a cloud home:
each demuxer read becomes a ranged cloud fetch (~1s of latency for a ~4 MB
window). A seek that the demuxer answers from an in-memory index costs **one**
fetch; a seek that falls back to FFmpeg's generic binary search costs **~15**
(it reads the file's end, then bisects, reading real frames at each step) —
that's the ~10s "track takes forever to start" on CUE/FLAC albums, where every
track is a seek into one big file.

## Grounding

Every line cited below is from upstream FFmpeg at:

- tag **`n8.0.1`**, commit **`894da5ca7d742e4429ffb2af534fcda0103ef593`**
  (the FFmpeg release bae shipped when this was written, bae-ffmpeg
  `v8.0.1-bae8`). bae now ships bae-ffmpeg `v8.1.2-bae7`; the cited
  conditionals have not been re-checked against 8.1.2.

Blob links use that commit, e.g.
`https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/flacdec.c#L358`.
When bae bumps FFmpeg, re-pin this doc: line numbers drift, but the *behaviors*
below have been stable across many releases — re-verify the cited conditionals,
not just the line numbers.

The seek mechanism is a property of the **demuxer**, not the codec. ALAC and AAC
both seek via the MP4 demuxer; the codec is irrelevant to seeking.

## Quick reference

| Format | Demuxer | Seek mechanism | Needs `AVFMT_FLAG_FAST_SEEK`? | Falls back to binary search? |
|--------|---------|----------------|------------------------------|------------------------------|
| FLAC | `flacdec.c` | seektable → stream index | **Yes** (and a seektable must be present) | **Yes**, when flag unset or no seektable |
| APE | `ape.c` | embedded seektable → stream index (always) | No | No — seektable is mandatory to even open |
| MP3 | `mp3dec.c` | Xing TOC, or CBR byte-scaling, with the flag; else the generic index | The flag turns on the estimates; bae leaves it off | No — no `read_timestamp`; without the flag a seek goes to the generic index |
| ALAC / AAC | `mov.c` (MP4) | full sample table built at open | No | No — always fully indexed (but see *moov location*) |
| WAV / PCM | `wavdec.c` → `pcm.c` | direct byte arithmetic | No | No — no index, no reads |

"Binary search" = FFmpeg's generic `ff_seek_frame_binary`: the slow,
~15-fetch-over-cloud path. Avoiding it is the whole game for remote playback.

## FLAC — `flacdec.c`

The decisive code is `flac_seek`
([L350](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/flacdec.c#L350)):

```c
if (!flac->found_seektable || !(s->flags&AVFMT_FLAG_FAST_SEEK)) {
    return -1;                       // flacdec.c:358
}
index = av_index_search_timestamp(st, timestamp, flags);
...
```

- The SEEKTABLE metadata block sets `found_seektable = 1`, but it only populates
  the stream's seek index **when `AVFMT_FLAG_FAST_SEEK` is set**
  ([L179–L185](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/flacdec.c#L179)).
- So `flac_seek` returns `-1` (the demuxer declines) whenever the flag is unset
  **or** the file has no seektable. The demuxer registers
  `.read_seek = flac_seek` + `.read_timestamp = flac_read_timestamp`
  ([L384–L385](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/flacdec.c#L384)),
  so a `-1` makes avformat fall back to `ff_seek_frame_binary` — read the end,
  then bisect reading real FLAC frames.
- The bisection can't place a target in the stream's final frames. It reads a
  timestamp by parsing from an arbitrary offset (`flac_read_timestamp`), and the
  FLAC parser dates a fixed-blocksize frame only once it has seen the frame after
  it (`get_best_header` sets `pts` only with a `best_child`), so the last frames
  have no readable timestamp. Measured on a 20-frame file: a target in the
  third-to-last frame fails ("read_timestamp() failed in the middle", the reader
  left at the file's end); one in the last frame lands a frame short. Which final
  frames fail depends on their byte sizes: in a quiet file whose last 3 frames
  are loud, a seek to 8 frames before a last-frame target still fails.

**Implication:** FLAC is the format that needs the fix. Set
`AVFMT_FLAG_FAST_SEEK` before `avformat_open_input` and a seektable-bearing FLAC
seeks in one fetch. Without the flag, or without a seektable, it binary-searches
the cloud (~10s). The seektable lives near the file *start* (after STREAMINFO),
so reading it costs nothing extra — the problem is purely that FFmpeg ignores it
unless asked. (FLACs without a usable seektable: see *What bae does*.)

## APE (Monkey's Audio) — `ape.c`

APE is the best-behaved format and needs no fix:

- The seektable is **mandatory**. `ape_read_header` errors out if it's missing or
  shorter than the frame count
  ([L248–L253](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/ape.c#L248)):
  `"Number of seek entries is less than number of frames" → AVERROR_INVALIDDATA`.
  An APE without a seektable does not open at all — there is no slow fallback,
  because APE frames have no sync codes and can only be located via the table.
- The seektable lives in the file **header**; `ape_read_header` reads it there and
  builds the full stream index unconditionally — `av_add_index_entry` per frame
  ([L357](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/ape.c#L357)),
  with `frames[i].pos` taken from the seektable
  ([L273](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/ape.c#L273)).
- `ape_read_seek`
  ([L433](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/ape.c#L433))
  does `av_index_search_timestamp` then seeks straight to the frame, and crucially
  sets `ape->currentframe = index` — the decoder's frame counter. This is why a
  raw `AVSEEK_FLAG_BYTE` is **wrong** for APE: a byte seek bypasses `read_seek`,
  so `currentframe` never updates and the decode desyncs.

**Implication:** remote APE opens by fetching ~the header window (seektable
included, a few tens of KB), then every seek is one fetch via the index. No
whole-file read, no binary search. Confirmed empirically (open touched ~98 KB;
a mid-file seek jumped straight to the target frame's byte).

## MP3 — `mp3dec.c`

MP3's header index is also gated on `AVFMT_FLAG_FAST_SEEK`. `mp3_seek`
([L552](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mp3dec.c#L552)):

```c
if (mp3->xing_toc && (mp3->usetoc || (fast_seek && !mp3->is_cbr))) {
    // use the Xing TOC index   (mp3dec.c:570)
} else if (fast_seek && st->duration > 0 && filesize > 0) {
    // CBR byte-rate scaling estimate
} else {
    return -1;                    // → the generic index
}
```

- The Xing TOC is only loaded into the index when `usetoc` **or**
  `AVFMT_FLAG_FAST_SEEK` is set
  ([L137–L138](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mp3dec.c#L137)).
- Both fast paths are **imprecise**: the TOC is a 100-point table (warns
  `"Using MP3 TOC to seek; may be imprecise."`), and CBR scaling assumes a
  constant bitrate. Without the flag (and no `usetoc`), MP3 returns `-1`.

- Neither path is a binary search: the MP3 demuxer registers no
  `read_timestamp`, so when `mp3_seek` returns `-1` avformat seeks through
  the stream's generic index (`AVFMT_GENERIC_INDEX`, `seek_frame_generic` in
  `seek.c`), which holds the frames the demuxer has read and any entry added
  with `av_add_index_entry`. A seek finds the last entry at or before the
  timestamp, reads from its byte, and dates the frames from its timestamp
  (`avpriv_update_cur_dts`). Past the last entry it reads forward from there
  to the timestamp.
- With the flag, the estimate is all a seek has. Measured on synthetic 60 s
  files (n8.1.2, a one-frame pre-roll): a CBR 128 kbps seek 40 s in played
  1,152 samples late; a VBR file with no Xing table, 5,976 late at 10 s and
  27,687 late at 40 s; a VBR file with a Xing table, from 4,792 early to
  5,000 late. A seek to before the first TOC entry fails, and its
  `ff_read_frame_flush` drops the packets `avformat_find_stream_info` had
  buffered: a VBR file with a Xing table started at sample 0 that way played
  from 59,760 samples in.
- A byte jump (`AVSEEK_FLAG_BYTE`) keeps no place: the demuxer restarts its
  timestamps at 0, and `read_frame_internal` (`demux.c`) then puts the
  encoder delay it drops at the stream's first frame (`start_skip_samples`,
  for `pts == 0`) on the jumped-to frame, and computes the end padding it
  drops from the restarted timestamps, so drops it nowhere.

**Implication:** an MP3 seek is exact only through an index entry that is a
real frame at its real timestamp. bae opens an MP3 without the flag and
gives the demuxer that entry: a stored scrub point (see *What bae does*).

## ALAC / AAC — `mov.c` (MP4 / M4A container)

ALAC and AAC reach FFmpeg inside an MP4/M4A container, so the **MP4 demuxer**
handles seeking — the codec is irrelevant.

- The full sample table (sample→time→byte) is built at open by `mov_build_index`
  ([L4650](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mov.c#L4650),
  called from `read_header` at
  [L5187](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mov.c#L5187)).
- `mov_read_seek`
  ([L11387](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mov.c#L11387))
  → `mov_seek_stream` → `av_index_search_timestamp`
  ([L11314](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mov.c#L11314))
  on that prebuilt index. Always precise, always indexed, no `FAST_SEEK`
  dependency, never a binary search.

**Implication — the MP4 remote cost is at OPEN, not per-seek.** The sample table
lives in the `moov` atom. If the file is *not* "faststart", `moov` is at the
**end** of the file, so opening it fetches the end (the whole sample table) before
playback can start. Per-seek is then free, but the first open of a non-faststart
M4A over the cloud pays a one-time end-fetch. (Faststart files put `moov` up
front and avoid this.) This is a different shape of remote cost than FLAC's
per-seek binary search — worth keeping in mind if M4A albums feel slow to *start*
but fine to *scrub*.

## WAV / PCM — `wavdec.c` → `pcm.c`

For real PCM, seeking is pure arithmetic — no index, no reads.
`wav_read_seek`
([L801](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/wavdec.c#L801))
delegates to `ff_pcm_read_seek`
([pcm.c L73](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/pcm.c#L73)),
which computes the byte position directly from `block_align` and `byte_rate`:
`pos = timestamp * byte_rate / ... `, aligned to `block_align`.

- Exact, one `avio_seek`, no fetches beyond the target window.
- Note: a *compressed* stream wrapped in WAV (MP3-in-WAV, AC3, DTS, XMA2) returns
  `-1` and uses the generic index path
  ([wavdec.c L833-ish, the codec switch in `wav_read_seek`](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/wavdec.c#L801)).
  bae's WAV content is PCM, so this is the byte-math path.

## Cross-cutting summary

- **`AVFMT_FLAG_FAST_SEEK` is the lever** for FLAC: it turns on the seektable
  index. It's a no-op for APE, MP4, and WAV (already optimal). For MP3 it turns
  on the TOC and bitrate estimates, which bae doesn't want.
- **The binary search is the enemy** and only FLAC (no flag or no seektable)
  ever hits it. APE/MP4/WAV/MP3 never do.
- **Two different remote-cost shapes:** FLAC pays *per-seek* (binary search);
  MP4 pays *at-open* (non-faststart `moov` at end). APE pays a small at-open
  header read. WAV pays nothing extra.
- **Accuracy:** FLAC/APE/MP4/WAV seeks land at or before the target frame and
  bae's trim loop (`start_at_sample`) makes the output sample-accurate. MP3's
  fast paths (TOC/scaling) are inherently approximate; its generic index is
  exact for the entries it holds.

## What bae does

- **Track starts jump by byte.** Import records each track's first byte, the
  frame holding its first sample (`seek_landing_bytes`, opened without
  `AVFMT_FLAG_FAST_SEEK` so a FLAC binary-searches its real frames rather than
  trusting a coarse seektable; its final frames, which the search can't place,
  are read in order once and indexed to bound the search), stored on
  `audio_format_segments.start_byte`. A start in a FLAC's last frame is
  recorded as the frame before it, since a decode that jumps to the last frame
  can't date it. Starting a FLAC, WavPack, Opus or Vorbis track seeks the
  demuxer straight to that byte, so no track start needs a seektable, in any
  layout, local or remote: their packets state their own first sample (frame
  header, block header, Ogg page granule). Every other codec's track start
  seeks by sample instead (`bae_audio::byte_jump_keeps_place`): after a byte
  jump the WAV, AIFF and MP3 demuxers restart their timestamps at 0, MP4's
  can't jump by byte, and APE's packets state no byte. The recorded byte is
  still fetched ahead. Over a cloud home the header and the track's audio are
  fetched in parallel. An MP3 starts at a scrub point or its first frame,
  below; bae binds no CUE sheet to lossy audio, so an MP3 track is its whole
  file.
- **In-track scrubs seek by sample** with `AVFMT_FLAG_FAST_SEEK` set
  (`bae-audio/src/decode.rs`), so a seektable-bearing FLAC (nearly
  every rip) uses its index: one fetch. The container is probed before
  `avformat_open_input` so the flag is set for every demuxer but MP3's. The
  exceptions are a FLAC with scrub points and every MP3, below.
- **A FLAC without a usable seektable, or with a sparse one, scrubs by byte,
  to a stored scrub point.** Left to FFmpeg, such a scrub is slow in one of
  three ways (FFmpeg n8.1.2, `flacdec.c`):
  - *No SEEKTABLE:* `flac_seek` returns -1 and avformat binary-searches the
    file (`ff_seek_frame_binary`). Waiting on the buffer doesn't fail the search:
    its read of the file's end waits for the fill, and each step is a fetch
    (a target in the file's last frames does fail it, as the FLAC section
    says).
    Measured on real files: the middle of the last track of a 1.5 GB
    single-file album took 26 fetches one after another, against 3 for an
    album with a seektable.
  - *A SEEKTABLE of only placeholder points* (sample number all ones, the
    space a ripper reserves): `found_seektable` is set and each placeholder
    goes to `av_add_index_entry` at timestamp -1, so the index holds one entry,
    at the first frame, before every sample. `flac_seek` finds it and
    succeeds, and the decode reads every byte from the first frame to the
    target. A seek to 20 s in a synthetic 30 s file read all 12 MB up to it.
  - *A sparse SEEKTABLE* (points minutes apart, or none near the end):
    `flac_seek` lands on the last point at or before the target, and the
    decode reads everything from there. With one point per 10 minutes a seek
    decodes up to 10 minutes of audio. MEASUREMENT

  Import's loudness pass, which decodes every segment from a frame start (its
  recorded landing byte, or the file's first frame), checks the demuxer's
  index right after `avformat_open_input` and before
  `avformat_find_stream_info` (whose packets add entries of their own, and
  which estimates a duration the header doesn't state). A FLAC gets scrub
  points unless that index lands every seek within their 10-second spacing
  of its target: it has an entry past sample 0, and no stretch from sample 0
  to its first entry, between entries, or from its last entry to the stream
  length STREAMINFO states is longer than 10 seconds
  (`bae_audio::HeaderSeekIndex::lands_within`). The points
  (`audio_segment_scrub_points`) come from the decode: each packet the decoder accepts gives its
  frame's byte (`pkt->pos`) and first sample (`pkt->pts`, the frame header's
  own number, since the FLAC demuxer parses with `PARSER_FLAG_USE_CODEC_TS`).
  A segment keeps its first frame, then the first frame each 10 seconds of
  audio on, or sooner where the frame would start more than one fill window,
  less the reads that follow it, past the last point. Those reads: FFmpeg's
  FLAC parser holds `FLAC_MIN_HEADERS` (10) frames before it hands the first
  on, and reads through a 32 KiB AVIO buffer, so the margin is 10 of the
  segment's largest frames plus 32 KiB (`bae-core/src/import/scrub_points.rs`).
  A one-frame margin was not enough: a scrub to the frame before the next
  point read past the window. The segment's last frame is never a point: the
  parser gives a file's last frame no timestamp, so a decode that jumps to it
  can't trim to the target. The frame before it stands in.

  A scrub jumps by byte (`AVSEEK_FLAG_BYTE`) to the last point at or before
  its target and decodes forward; the `start_at_sample` trim makes the output
  sample-exact. Over the buffer it reads the file's head (the probe) and one
  window from the point. Measured on real files with a prototype (points every
  10 s, direct jump), against the binary search: the 1.5 GB album's middle of
  its last track went from 26 jumps to 3, the middle of its first from 6 to 2;
  a 245 MB album's from 4 to 3 and 6 to 2; a per-track file's from 4 to 2 and
  8 to 2; the album with a seektable takes 3 and 2.
- **An MP3 starts at a stored scrub point, or its first frame.** Its frames
  state no position, and its audio leans on the frames before it: a frame's
  main data begins up to 511 bytes back in the main data of earlier frames
  (the bit reservoir, `main_data_begin`), and its output overlaps the frame
  before. Import's loudness pass reads every MP3 from its first frame (a
  lossy sink decode always does), and each packet gives its frame's byte,
  size and first sample (`pkt->pts` on the stream's count, the first frame's
  before sample 0 by the encoder delay). A frame decodes as from the start
  once the decode has its main data and the frame before it whole, which
  needs the frames from the start of the decode to two before it to hold 511
  bytes of main data; counted as each frame's size less 38 (header, CRC,
  MPEG-1 stereo side information), that gives the latest frame a decode can
  start at for each frame. Measured against the least that played exactly
  (n8.1.2's float decoder, pink noise and a noise/tone mix): 3 frames back at
  128 kbps, where the count also says 3; 8 at 32 kbps, where it says 9; 2 to
  3 in VBR quality 2 files. For the first frame each 10 seconds past the
  stream's first, the point is that latest frame, with `exact_from`, the
  first sample a decode from it plays exactly. Ten seconds are at most
  400 KB (320 kbps), well inside one window.

  The decoder opens the MP3 without `AVFMT_FLAG_FAST_SEEK`, adds the point's
  frame to the stream's index (`av_add_index_entry`, replacing any entry at
  its timestamp) and seeks to its timestamp: the generic index jumps to its
  byte and dates the frames from there, so the encoder delay is dropped only
  at the stream's first frame and the end padding at its last. A scrub takes
  the last point whose `exact_from` is at or before its target; before the
  first it reads from the stream's start. Every sample it plays equals a
  decode from the stream's start, in CBR and VBR files with and without a
  Xing table, mono, MPEG-2 (22.05 kHz) and MPEG-2.5 (8 kHz, 8 kbps), a file
  shorter than 10 s (no points), and a 10.8 MB one. An MP3 asked for a sample
  seek with no point (a preview of an unimported file) reads from its start:
  without the flag a sample seek would read forward from the first frames
  anyway, with a pre-roll short of the reservoir.

  Over the fill, a scrub 4 minutes into that 10.8 MB, 320 kbps file fetched
  the head window, two 32 KiB read-aheads past it (the fill keeps a window
  ahead of the probe's reads until the decode seeks), and the window from
  the point: 4 fetches, 4.66 MB. Opening an MP3 with no ID3v2 tag reads its
  last 128 bytes for an ID3v1 tag (`ff_id3v1_read` in `mp3_read_header`);
  the fill evicts the head window behind that read and fetches it again
  when the demuxer reads on: 7 fetches, 8.89 MB, for the same scrub.
- **A lossy start is placed on the stream's own sample count.** Sample 0 is
  the stream's `start_time` (an MP3's LAME delay puts it 1,105 samples past
  the first packet), the decoder gets the stream's time base
  (`pkt_timebase`) so a frame it trims an encoder delay from keeps the right
  timestamp, and a sample seek converts the sample to the stream's time base
  (an MP3's is 1/14,112,000 s). A sample seek lands a pre-roll early: the
  codec's `seek_preroll` (Opus, 80 ms) or one frame (AAC's MDCT overlap),
  reaching before sample 0 where an AAC stream's priming frame sits. The
  decoder is not flushed after the seek, which would leave AAC's decoder in
  other than its opening state. A sink decode (loudness, save, transcode)
  reads a lossy stream from its start rather than seeking, because a lossy
  decoder's output after a seek is not the decode from the start: Opus's
  float decoder still differed 8,000 samples past a target it landed 7,700
  before. Playback seeks; an MP3's seeks start at a scrub point, above.
- **Probe read-ahead is one window.** Before the decoder has seeked to the
  track, the buffer keeps one window ahead of byte 0, not four, since the
  demuxer reads only the front metadata before it seeks away.
