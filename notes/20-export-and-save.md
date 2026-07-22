# Export and save

Two distinct ways audio leaves the library. They look similar (both write
files to a place the user picks) but their contracts are opposites, and
conflating them muddles both.

- **Export** is the inverse of import. It reproduces the file set that was
  imported, byte for byte. What came in comes out.
- **Save** is "Save As". It produces *output* — a workup of what bae knows
  about the music, rendered into standalone files in a chosen format.

The crispest way to tell them apart: export returns the files you gave bae;
save renders the music bae has. Metadata edits made inside bae are invisible
to export (source bytes are never rewritten) and authoritative for save
(they're what gets tagged into the output).

## Export

Release level only. A release was imported as a folder of files; export
reconstructs that folder.

**Contract: the import ↔ export round trip is bit-identical.** Every imported
file comes back with its verbatim bytes, at its original relative path
(`original_filename`, which may include subfolders like `CD1/…`), under the
release's `source_folder_name`. Audio, cue sheets, logs, artwork, scans —
whatever the import took, in its original layout. No transcoding, no tag
rewriting, no renaming, no cover embedding, no file filtering. Export has no
format options because format is not a degree of freedom — fidelity to the
imported set *is* the feature.

One addition rides along: a hidden `.bae-output` marker file naming the
release id. It is what lets a re-export safely replace a prior export of the
same folder (an unmarked directory at the target is never touched). The files
themselves are still exact; the folder carries one extra hidden file.

Mechanics:

- Reads each blob through coven's locality-aware read — a Remote release
  fetches from cloud/cache and decrypts. Export never requires a local copy
  and never changes release state: no gate flips, no locality change.
- Destination is chosen per export in a folder dialog, seeded with the
  last-used output folder (remembered by the UI; destination is a per-run
  choice, not configuration). Output lands at `<dir>/<source_folder_name>/`.
- Releases queue through the serial output queue, shared with release-level
  save (one release-level output at a time, pause/cancel/retry, transient
  across restarts).
- All-or-nothing: files write into a hidden staging directory that is renamed
  into place only after every file succeeds; failure or cancel removes the
  staging directory and leaves nothing at the final path.

## Save

"Save As" — a workup that produces standalone output files. Track level or
release level.

A save always names its output format: the preset specifies the codec, and
the source audio is decoded and encoded to it. Whether that format matches
the source's is irrelevant — there is no "keep the original format" save;
original is export's contract, not a save option. Even when the preset's
codec coincides with the source's, the output is a constructed file: split
out of any image, tagged from bae's metadata, named by pattern. The source's
shape never shows in the result.

**Image splitting.** A CUE/single-file source (one image file backing many
tracks) has no per-track files to hand out, so save extracts: decode the
image, cut the track's sample window, encode a standalone file. This is
inherent to save — a track saved from an image is always constructed, never
copied.

**The workup**, applied to every constructed output file:

- Tags written from bae's metadata: title, artist, album, year, track
  number/total, disc (digital media only — sides are not discs).
- Cover art embedded, optionally (a preset choice). The embedded art is the
  release's own cover.
- Filename rendered from the preset's token pattern (title, artist, album,
  year, track number, disc number, track total), sanitized for
  macOS/Windows.

**Presets** (`SavePreset`) bundle the save options: codec + quality
(FLAC/WAV/AIFF with bit depth, MP3/Opus with bitrate), filename tokens,
pregap placement, cover embedding, and which levels the preset applies to
(track, release).

**Release-level save** produces one of two shapes, chosen by the preset:

- **File per track**: every track saved as its own file, names deduplicated
  within the output folder. Pregap placement options decide where CUE pregap
  audio goes (append to previous track, include or exclude the hidden
  track-one pregap, or exclude pregaps entirely).
- **Single file + CUE**: the whole release as one audio image plus a
  generated CUE sheet (title, performer, catalog/barcode, date, per-track
  index points computed from the sample windows). Release level only, and
  only for codecs CUE can name (not Opus/Ogg).

Destinations: a release-level save targets a directory chosen the same way
as export's (folder dialog, last-used seed) and runs through the same serial
output queue; its output folder carries the same `.bae-output` marker with
the same replacement rules. A track-level save runs through the platform
save panel, seeded with the selected preset's rendered filename suggestion —
re-rendered when the preset changes, and seeding reads only the database,
never audio or the cloud.

Like export, save reads through coven (cloud-only sources download +
decrypt), and output is atomic: partial files are removed on failure or
cancel.

## The line between them

| | export | save |
|---|---|---|
| level | release | release or track |
| source of truth | imported bytes | bae's metadata + decoded audio |
| audio bytes | verbatim | decoded, encoded to the preset codec |
| image splitting | never (image stays an image) | yes (tracks are constructed) |
| tags | untouched | written from bae |
| cover | as imported (a file among files) | optionally embedded |
| filenames | original, verbatim | rendered from token pattern |
| options | destination only | preset: format, quality, naming, pregaps, cover |
| metadata edits in bae | invisible | applied |

"I want my rip back" is export. "I want files for my phone / another player /
a friend's format" is save.
