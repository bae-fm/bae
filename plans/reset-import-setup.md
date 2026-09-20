# Reset import setup

## Queue and execution
Execute after [restore unused audio sources](restore-unused-audio-sources.md), on a separate focused branch in the same background worktree. The serial queue is field-origin removal, Avalonia per-FILE CUE caller repair, release/album cross-reference enrichment, unexpected release-selection diagnostics, cover format support, restoring unused sources, then Reset. Follow research, detailed plan, failing regression tests, implementation, contract review, normal checks/hooks, and coordinated fast-forward integration.

## User contract
Add “Reset” to the existing macOS candidate menu alongside Reset to tags and Clear metadata. Reset returns the whole candidate import setup to its initial scanned state: restore all source tracks, original automatic CUE and file assignments, original cover selection, and initial metadata according to the current Pre-fill with tags preference.

Reset to tags remains a metadata application operation. Reset replaces the entire candidate import setup. Do not introduce manual-edit provenance, per-field origin protection, or an ongoing metadata binding. Source files remain untouched.

## Research and design requirements
Read the full existing scan, candidate initialization, reset/application, file/CUE decisions, cover selection, and persistence paths. Reuse the production initialization primitives rather than constructing a parallel version of initial state. Specify how the latest authoritative scan and current preference define the initial setup, including unavailable/changed source files; never fabricate stale source availability.

Replace candidate state atomically and honor the existing file/edit/metadata revision and concurrency rules. Define cancellation or invalidation of stale in-flight metadata/identify/cover operations so they cannot overwrite the reset result. Fail through existing error presentation without leaving partially reset durable state. Repeating Reset must produce the same setup for the same authoritative scan and preference.

## Required regression coverage
- Removed whole-file tracks and selected CUE slices return through the real reset operation.
- Altered CUE FILE associations, changed disc assignment, ignored CUEs, and file role decisions return to their original automatic scanned choices.
- Metadata and cover edits are replaced by the initial choices; no selected online release is silently retained or reapplied as a binding.
- Pre-fill with tags enabled and disabled produce the same initialization behavior as initial discovery.
- Repeated reset remains stable, and a stale in-flight source application cannot overwrite the reset.
- An operation failure rolls back the whole reset and reaches existing UI error handling.
- The actual macOS menu action calls the canonical operation and is distinct from Reset to tags and Clear metadata.

## Implementation boundaries and verification
Product UI scope is bae-macos. Update necessary shared core/bridge definitions and callers together. Translate any new label/help strings across every applicable locale, reusing existing messages where appropriate. Do not mutate the live database or media during tests. Run affected production-path core/bridge/macOS tests, build, review every contract point, run normal hooks, and commit this concern separately before coordinated landing.
