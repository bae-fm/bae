# Seeking audio over the cloud: one fetch, not fifteen

*Draft. FFmpeg behaviour grounded in FFmpeg n8.0.1 (bae now ships 8.1.2);
numbers from bae's own measurements while building cloud playback.*

Other titles to consider: "Why our CUE albums took ten seconds to start",
"Every demuxer read is a round trip".

## The setup

bae can keep a library in the person's own cloud storage and play it from
there. A release's file is stored encrypted, and playback streams it: FFmpeg
reads through a custom I/O layer, and every read FFmpeg makes becomes a ranged
fetch from the cloud. Each fetch costs about a second of latency for a window
of a few megabytes. Bandwidth was never the problem; the number of fetches was.

That matters most for a CUE/FLAC album: the whole album as one large FLAC with
a CUE sheet listing where each track starts. Every track start is a seek into
the middle of a 200+ MB file, and every scrub is another.

## What a seek costs

How FFmpeg answers a seek depends on the demuxer, not the codec:

| Format | How it seeks | Cost over the cloud |
|---|---|---|
| FLAC | the file's seektable, but only with `AVFMT_FLAG_FAST_SEEK` set | 1 fetch with both; ~15 without |
| APE | its seektable, which the format requires | 1 fetch |
| MP3 | the Xing table of contents or bitrate arithmetic, only with `AVFMT_FLAG_FAST_SEEK`; approximate | 1 fetch with it; ~15 without |
| MP4 (ALAC, AAC) | the full sample table, built when the file opens | free per seek; a non-"faststart" file pays one fetch of its end at open |
| WAV | byte arithmetic | free |

The "~15" is FFmpeg's generic fallback, a binary search: read the end of the
file, then bisect, reading real frames at each step. Locally that's
milliseconds. Over the cloud it's fifteen round trips, about ten seconds, and
that was the ten-second wait to start a track of a CUE album.

The surprise was FLAC. Nearly every FLAC rip carries a seektable near the start
of the file, and FFmpeg reads it, but its FLAC demuxer only uses it when the file
is opened with `AVFMT_FLAG_FAST_SEEK`. Without the flag it ignores the table it
already has and binary-searches. (`flacdec.c`: `if (!flac->found_seektable ||
!(s->flags & AVFMT_FLAG_FAST_SEEK)) return -1;`)

## What we changed

**Set `AVFMT_FLAG_FAST_SEEK`.** One flag before opening the file: a
seektable-bearing FLAC, and an MP3 with a table of contents, now seek in one
fetch. It changes nothing for APE, MP4 or WAV, which were already indexed. A
test records the demuxer's read offsets on a CUE/FLAC fixture and fails if a
mid-file seek ever reads the file's end, the binary search's signature.

**Record where every track starts, at import.** Import already walks the
file; it now also records the byte each track's first frame starts at. A track
start then jumps the demuxer straight to that byte, with no seektable needed at
all. To get that byte right, import opens the file *without* the fast-seek
flag, so a FLAC binary-searches its real frames instead of trusting a coarse or
placeholder seektable; that's a local read, done once.

**Fetch the header and the track in parallel.** Starting a track deep in a
cloud album took two fetches one after the other: the file's header at byte 0,
then, once the seek was resolved, the track's audio. With the start byte known
in advance both go out at once. On a 243 MB album that took the start from about
2.8 s toward a single round trip.

**Read ahead one window, not four, before the seek.** While FFmpeg probes the
header, the stream buffered 16 MB ahead of byte 0, which the demuxer abandons
the moment it seeks to the track. Now it buffers one window.

**Store scrub points for FLACs without a seektable.** A FLAC with no
seektable still binary-searched when you scrubbed inside a track: FFmpeg reads
the file's end, then bisects, and over the cloud each step is a round trip.
We'd assumed that search failed over a streaming buffer and fell back to
decoding from the start; measuring on real files showed it doesn't fail, it
just waits for each read. Scrubbing to the middle of the last track of a
1.5 GB single-file vinyl rip took 26 fetches one after another, against 3 for
an album with a seektable. A seektable of only placeholder points is worse in
a different way: FFmpeg "finds" a seek point before the first sample and
decodes the whole file up to the target.

The first idea, handing FFmpeg index points before a scrub, didn't help: it
still bisects between them. What worked was not asking FFmpeg to seek at all.
Import already decodes every track to measure its loudness, and every packet
it reads carries its frame's exact byte and first sample. So import keeps a
few of them: each track's first frame, then one every 10 seconds of audio, or
sooner when the bytes since the last would outgrow the 4 MB window one fetch
brings in, less the ten frames FFmpeg's FLAC parser reads ahead before it
hands one over. A scrub jumps by byte to the last point before the target and
decodes forward, trimming to the exact sample. The points sync with the rest
of the album's layout, so another device streaming it scrubs the same way.

Measured on the same files with a prototype of the jump (fetches one after
another, before and after):

| File | middle of last track | 10% into last track | middle of first track |
|---|---|---|---|
| 1.5 GB single-file vinyl rip, no seektable | 26 → 3 | 14 → 3 | 6 → 2 |
| 417 MB single-file CD rip, no seektable | 6 → 3 | 7 → 2 | 7 → 2 |
| 245 MB single-file CD rip, no seektable | 4 → 3 | 4 → 2 | 6 → 2 |
| per-track file, no seektable | 4 → 2 | 6 → 1 | 8 → 2 |
| 310 MB single-file album with a seektable | 3 | 2 | 2 |

The output stayed sample-exact in every case: the same samples as decoding
from the track's start and trimming.

## The lesson

Over a high-latency link, count the reads your decoder makes, not the bytes.
Log every read's offset against a real file, and the slow path shows itself
immediately: a read at the very end of a file you're seeking into the middle of.
