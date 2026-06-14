# Import Tracing

## What

Import timing instrumentation that records per-step durations to `~/.bae-traces/imports.jsonl`. Each import appends one JSON line with a timestamp, album/artist info, total duration, and a breakdown by step.

Enabled by setting `BAE_IMPORT_TRACE=1` (set by default in `bae-macos/run.sh`).

## Why

The import progress bar was misleading. It sat at 0% for most of the import, then jumped to 100%. We added tracing to understand where time actually goes.

### What we found (March 2026)

Typical local folder import: ~3.5s total.

| Step | Time | % |
|---|---|---|
| resolve_metadata | ~1.5s | 43% |
| download_cover_art | ~1.5s | 46% |
| discover_files | ~2ms | 0% |
| validate_tracks | ~0ms | 0% |
| save_to_database | ~7ms | 0% |
| extract_durations | ~60ms | 2% |
| storage | ~400ms | 11% |

90% of the time is network (metadata API call + cover art download). These happen during the "Preparing" phase, which was not reflected in the progress bar at all. The progress bar only moved during "storage" — the last 11%.

## Current state

- Progress bar is hidden. We show a spinner + status text instead (e.g., "Downloading cover art...", "Extracting durations...").
- Tracing stays on in dev builds via `run.sh`.

## Future

- Decide if a progress bar is worth bringing back. If so, it needs weighted progress that reflects actual time distribution (network-heavy).
- The traces will tell us if the distribution shifts as we optimize steps.
