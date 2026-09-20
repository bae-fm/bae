# Reset import setup

## Queue and execution
Execute after [restore unused audio sources](restore-unused-audio-sources.md), on a separate focused branch in the same background worktree. The serial queue is field-origin removal, Avalonia per-FILE CUE caller repair, release/album cross-reference enrichment, unexpected release-selection diagnostics, cover format support, restoring unused sources, then Reset. Follow research, detailed plan, failing regression tests, implementation, contract review, normal checks/hooks, and coordinated fast-forward integration.

## User contract
Add “Reset” to the existing macOS candidate menu alongside Reset to tags and Clear metadata. Reset returns the whole candidate import setup to its initial scanned state: restore all source tracks, original automatic CUE and file assignments, original cover selection, and initial metadata according to the current Pre-fill with tags preference.

Reset to tags remains a metadata application operation. Reset replaces the entire candidate import setup. Do not introduce manual-edit provenance, per-field origin protection, or an ongoing metadata binding. Source files remain untouched.

## Research and design requirements
Read the full existing scan, candidate initialization, reset/application, file/CUE decisions, cover selection, and persistence paths. Reuse the production initialization primitives rather than constructing a parallel version of initial state. Specify how the latest authoritative scan and current preference define the initial setup, including unavailable/changed source files; never fabricate stale source availability.

Replace candidate state atomically and honor the existing file/edit/metadata revision and concurrency rules. Define cancellation or invalidation of stale in-flight metadata/identify/cover operations so they cannot overwrite the reset result. Fail through existing error presentation without leaving partially reset durable state. Repeating Reset must produce the same setup for the same authoritative scan and preference.

## Source-grounded implementation requirements

The relevant source paths are `handle/scan.rs`, `preparations.rs`,
`preparations/pane_edits.rs`, `preparation.rs`, `file_tags_seed.rs`,
`release_candidate.rs`, `folder_scanner/categorize.rs`, and the database writers
`client/folder_scans/write.rs`, `client/import_state.rs`, and
`client/import_state/preparation_rows.rs`.

For a folder candidate, clear the contents of the existing file-decision maps
and pass the latest scanned files through `apply_candidate_file_edits`. Its
existing settlement restores proposed audio roles, resolves the CUE FILE
references again, and assigns discs from the resulting automatic bindings.
Competing CUEs retain the initializer's unresolved selection policy. Do not
invent an original-state archive or reconstruct those decisions in the UI.
The reset file revision must advance from the current revision; clearing the
decisions does not reset their revision counter to zero.

Construct all included tracks through the same source seed as discovery.
With prefilling enabled, use the existing FileTagsSeed projection without a
`keeping` filter; Reset to tags deliberately supplies that filter and therefore
cannot implement this action. With prefilling disabled, use the existing blank
source construction. Replace the metadata, provenance, cover choice, and prepared
asset set together. Do not carry an old remote cover or applied online source
into that seed. Use the existing cover selection policy rather than a second
ranking algorithm. Keep the candidate's folder boundaries and combination
identity; resetting a combined candidate must not silently separate its parts
or change another candidate's file decisions. Trace combination initialization
before extending the reset command to that source shape.

Prepare the replacement under the existing candidate commit coordination,
revalidate editability and scanned identity at commit, and save the replacement
with both revision expectations. Clear the previous identification answer and
signals as part of the same state replacement. Cancel obsolete identification
work and advance revisions so any already prepared metadata or cover result
cannot overwrite the replacement. Reuse the existing candidate change events;
do not use a persisted reset flag or a later repair pass. Identification started
after Reset continues to follow the normal preference and operation rules.

### Snapshot and file-row transaction ordering

The current shared save path writes a supplied tag snapshot before checking the
metadata revision. It can then return `CandidateSaved::Superseded` as a successful
database call. Reproduce this with a stale metadata revision and a different
snapshot: a refused save must leave the snapshot, cover bytes, and tag facts
unchanged as well as leaving the included tracks unchanged. Move mutation after
the complete revision guard rather than relying on the caller to repair it.

The same save path writes a supplied snapshot before `reshaped_files`.
`settle_scanned_candidates` replaces `scan_candidate_file` rows; the tag facts
reference those rows with `ON DELETE CASCADE`. Reset needs both restored file
rows and a corresponding tag reading, so adding both existing extras unchanged
would erase the newly written facts. Store restored file rows before their tag
snapshot and validate the snapshot against the resulting file revision and the
authoritative scan generation. Keep the old revision solely as the write's
expectation. All scanned candidates sharing the preparation's content identity
must remain consistent with the committed file decisions.

Add a production-path test for a reset that changes file decisions and prefills
tags, then reload both the candidate and its complete tag snapshot. Verify that
every expected file fact survives, its revision matches the restored candidate,
and a subsequent Reset to tags uses those facts. Also force a stale save and an
operation failure to prove that no snapshot or file-row mutation escapes the
failed replacement. Do not split Reset into a file edit followed by metadata
application: those operations intentionally preserve parts of existing state
and would expose an intermediate setup.

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
