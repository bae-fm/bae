# Release Export Presets And Single-File CUE

## Goal

Release and track export use configured presets instead of hard-coded format
choices. A library starts with MP3 and FLAC presets, Original is always
available, and users can configure release-applicable and track-applicable
presets in settings.

For release export, presets can choose how pregaps are written. Single-file CUE
mode writes one encoded release image plus a `.cue` sheet whose track indexes
match the exported audio. External playback follows the CUE layout: pregaps are
audible where the CUE layout places them.

## Core Model

- Add `ExportPregapPlacement::SingleFileWithCue`.
- Keep `Original` outside presets and default-selected for track and release
  exports.
- Validate presets in core:
  - a preset must apply to track export, release export, or both;
  - single-file CUE applies only to release export;
  - single-file CUE is allowed only for formats with a CUE `FILE` type that we
    can write correctly.
- Release exports use the release export queue.
- Make preset writes fail before committing config when a default selection would
  reference a removed preset.

## Single-File CUE Export

- Build each release track from the same export plan shape used by track export.
- For each track, decode the selected source window, including the source pregap
  region when the track has one.
- Add generated leading silence when the CUE layout requires hidden track-one
  audio or inter-track pregap placement.
- Concatenate decoded PCM into one release image.
- Reject mixed sample rates or channel layouts before writing output.
- Encode the image using the selected preset codec.
- Write a `.cue` sheet next to the image:
  - `FILE` references only the image filename;
  - CUE file type is derived from the encoded container;
  - `INDEX 01` points to the start of each track's program audio;
  - `INDEX 00` points to pregap start when the track has a pregap;
  - titles, performers, album title, album artist, catalog, and year are written
    when present.
- Use one output guard for both files so cancellation or an encode/write error
  removes partial output.

## CUE Scope

- Support single-file image plus CUE export.
- CUE-backed import and playback behavior is unchanged.
- Multi-file CUE export is not supported.
- Lossy packet-copy "Original" slicing is not supported without a
  codec/container-specific path with explicit packet-boundary and timestamp
  behavior.

## macOS

- Settings > Export shows preset configuration for track and release export.
- The pregap picker shows "Single file + CUE" disabled when the selected codec
  cannot write a CUE file type.
- Selecting "Single file + CUE" forces release-applicable on and
  track-applicable off.
- Saving presets sends the edited values to core; invalid combinations fail
  through config validation.
- Track export keeps the save-panel format picker, fed by track-applicable
  presets.
- Release export shows a format picker for both album-detail export and Storage
  Manager batch export:
  - fixed export directory: ask for format, then enqueue;
  - ask-each-time directory: ask for folder and format in one panel, then
    enqueue.

## Windows

- Settings > Export shows the same preset fields and pregap choices as macOS.
- The pregap picker includes "Single file + CUE" only for supported codecs.
- Selecting "Single file + CUE" forces release-applicable on and
  track-applicable off.
- Windows FFI exposes release export with a selected `ExportSelection`.
- Album-detail release export asks for format and target folder, then calls core
  release export.
- Track export keeps its format selection and uses track-applicable
  presets.
