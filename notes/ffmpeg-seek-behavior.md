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
| MP3 | `mp3dec.c` | Xing TOC, or CBR byte-scaling | **Yes** (or `usetoc` opt); imprecise either way | **Yes**, when neither holds |
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

MP3 mirrors FLAC: its index is also gated on `AVFMT_FLAG_FAST_SEEK`. `mp3_seek`
([L552](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mp3dec.c#L552)):

```c
if (mp3->xing_toc && (mp3->usetoc || (fast_seek && !mp3->is_cbr))) {
    // use the Xing TOC index   (mp3dec.c:570)
} else if (fast_seek && st->duration > 0 && filesize > 0) {
    // CBR byte-rate scaling estimate
} else {
    return -1;                    // → generic binary search
}
```

- The Xing TOC is only loaded into the index when `usetoc` **or**
  `AVFMT_FLAG_FAST_SEEK` is set
  ([L137–L138](https://github.com/FFmpeg/FFmpeg/blob/894da5ca7d742e4429ffb2af534fcda0103ef593/libavformat/mp3dec.c#L137)).
- Both fast paths are **imprecise**: the TOC is a 100-point table (warns
  `"Using MP3 TOC to seek; may be imprecise."`), and CBR scaling assumes a
  constant bitrate. Without the flag (and no `usetoc`), MP3 returns `-1` →
  binary search.

**Implication:** `AVFMT_FLAG_FAST_SEEK` helps MP3 too (avoids the binary search),
but MP3 seeks are approximate by nature. MP3 is rarely a single-file CUE album in
practice (MP3 rips are usually per-track), so the remote-seek cost mostly applies
to long single MP3s, and the imprecision is the tradeoff for not binary-searching.

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

- **`AVFMT_FLAG_FAST_SEEK` is the lever.** It turns on the seektable/TOC index for
  **FLAC and MP3**; it's a no-op for APE, MP4, and WAV (already optimal). Setting
  it before `avformat_open_input` is the general fix for slow remote seeks. It
  does not hurt the formats that ignore it.
- **The binary search is the enemy** and only FLAC (no flag or no seektable) and
  MP3 (no flag/`usetoc`) ever hit it. APE/MP4/WAV never do.
- **Two different remote-cost shapes:** FLAC/MP3 pay *per-seek* (binary search);
  MP4 pays *at-open* (non-faststart `moov` at end). APE pays a small at-open
  header read. WAV pays nothing extra.
- **Accuracy:** FLAC/APE/MP4/WAV seeks land at or before the target frame and
  bae's trim loop (`start_at_sample`) makes the output sample-accurate. MP3's
  fast paths (TOC/scaling) are inherently approximate.

## What bae does

- **Track starts jump by byte.** Import records each track's first byte, the
  frame a playback seek would land on (`seek_landing_bytes`, opened without
  `AVFMT_FLAG_FAST_SEEK` so a FLAC binary-searches its real frames rather than
  trusting a coarse seektable), stored on `audio_format_segments.start_byte`.
  Starting a track seeks the demuxer straight to that byte, so no track start
  needs a seektable, in any layout, local or remote. Over a cloud home the
  header and the track's audio are fetched in parallel.
- **In-track scrubs seek by sample** with `AVFMT_FLAG_FAST_SEEK` set
  (`bae-core/src/audio_codec/decode.rs`), so a seektable-bearing FLAC (nearly
  every rip) and MP3 use their index: one fetch. The exception is a FLAC with
  scrub points, below.
- **A FLAC without a usable seektable scrubs by byte, to a stored scrub
  point.** Left to FFmpeg, such a scrub is slow in one of two ways (FFmpeg
  n8.1.2, `flacdec.c`):
  - *No SEEKTABLE:* `flac_seek` returns -1 and avformat binary-searches the
    file (`ff_seek_frame_binary`). The search doesn't fail over the buffer: its
    read of the file's end waits for the fill, and each step is a fetch.
    Measured on real files: the middle of the last track of a 1.5 GB
    single-file album took 26 fetches one after another, against 3 for an
    album with a seektable.
  - *A SEEKTABLE of only placeholder points* (sample number all ones, the
    space a ripper reserves): `found_seektable` is set and each placeholder
    goes to `av_add_index_entry` at timestamp -1, so the index holds one entry,
    at the first frame, before every sample. `flac_seek` finds it and
    succeeds, and the decode reads every byte from the first frame to the
    target. A seek to 20 s in a synthetic 30 s file read all 12 MB up to it.

  Import's loudness pass, which decodes every segment from a frame start (its
  recorded landing byte, or the file's first frame), checks the demuxer's
  index right after `avformat_open_input` and before
  `avformat_find_stream_info` (whose packets add entries of their own). A FLAC
  whose index has no entry past sample 0 gets scrub points
  (`audio_segment_scrub_points`): each packet the decoder accepts gives its
  frame's byte (`pkt->pos`) and first sample (`pkt->pts`, the frame header's
  own number, since the FLAC demuxer parses with `PARSER_FLAG_USE_CODEC_TS`).
  A segment keeps its first frame, then the first frame each 10 seconds of
  audio on, or sooner where the frame would start more than one fill window,
  less the reads that follow it, past the last point. Those reads: FFmpeg's
  FLAC parser holds `FLAC_MIN_HEADERS` (10) frames before it hands the first
  on, and reads through a 32 KiB AVIO buffer, so the margin is 10 of the
  segment's largest frames plus 32 KiB (`audio_codec/decode/scrub_points.rs`).
  A one-frame margin was not enough: a scrub to the frame before the next
  point read past the window.

  A scrub jumps by byte (`AVSEEK_FLAG_BYTE`) to the last point at or before
  its target and decodes forward; the `start_at_sample` trim makes the output
  sample-exact. Over the buffer it reads the file's head (the probe) and one
  window from the point. Measured on real files with a prototype (points every
  10 s, direct jump), against the binary search: the 1.5 GB album's middle of
  its last track went from 26 jumps to 3, the middle of its first from 6 to 2;
  a 245 MB album's from 4 to 3 and 6 to 2; a per-track file's from 4 to 2 and
  8 to 2; the album with a seektable takes 3 and 2.
- **Probe read-ahead is one window.** Before the decoder has seeked to the
  track, the buffer keeps one window ahead of byte 0, not four, since the
  demuxer reads only the front metadata before it seeks away.
