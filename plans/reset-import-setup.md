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

A combined release is a `release_grouping` row: its stored `scan_candidate`
row carries the grouping key, its files, and its ordered `scan_candidate_part`
rows (folder and path prefix). `FolderCandidate::blank_source` titles a grouped
release by its name, and `DiscLayout` gives each part its own run of discs;
`FolderCandidate::file_tag_edit` preserves that layout when projecting tags.
Reuse these for Reset's current-prefill choice. Preserve the grouping key,
member order, prefixes, and disc runs: Reset reads the stored grouped candidate,
never the member folders, so it cannot change the release's identity or disc
layout. The existing admission checks must still reject a changed or
unavailable source. Test a combined candidate with removed tracks and changed metadata under
both prefill settings, and assert that member candidates remain unchanged.

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

## Agreed implementation detail

The handle captures the current source identity, both preparation revisions,
scan generation, and current prefill preference. It prepares the entire
replacement before committing. For folders, settle all stored folder entries
sharing that preparation hash with empty decision maps; for a combination,
retain its stored files and ordered parts. Never reconstruct a combination
from its live members. Initialize the requested candidate with
`FileTagsSeed::project` and no `keeping` rows, or its existing blank source
constructor; blank combination initialization restores its stored name.

When tags are required, call `extract_file_tag_snapshot` directly on the
replacement source with the destination file revision, then project that
reading. Do not call the caching `file_tag_snapshot_with_reader` operation:
it persists an intermediate reading before returning. Discovery's wrapper
also deliberately accepts a failed tag read by seeding blanks; explicit Reset
instead reports the failure and leaves the preparation unchanged.

The preparation operation replaces metadata, provenance, author, cover,
prepared assets, decisions, identification, and extracted signals together.
It advances both revision counters, retaining neither a prior applied source
nor remote artwork. Its final commit rechecks editability and source identity
under the existing coordination lock, then uses both revision expectations.
Source preparation must not persist anything before that final write.

The database's first mutation is the preparation revision compare-and-set.
A superseded save returns before touching snapshots or dependent rows. The
transaction then replaces folder files, advances source revisions, and writes
the optional snapshot after the files it references exist. Validate its scan
generation against the current authoritative generation and its file revision
against the destination preparation revision. Reuse the existing snapshot
coverage and embedded-cover membership checks at this boundary; the lower
level insertion helper alone does not enforce them. Every later failure rolls
back the whole write, including the revision change.

`CandidateSaveExtras.reshaped_files` continues to carry the complete folder
replacement set. `Some(empty)` is meaningful when only combinations share the
identity. Inside the transaction, enumerate every scan entry sharing the hash,
check its expected revision, and dispatch on stored source kind. Folder rows
require a corresponding supplied file set and are rewritten; combination rows
retain their files and membership and advance their revision. Supplied folder
keys must exactly match the stored folder subset. A later folder scan can
share a combination's hash even though creation rejects duplicate combinations;
therefore a combination-only update at the selected key is insufficient.

Return the actual `ReleaseCandidate` values after saving. Existing file-edit
callers and Reset publish folder binding events for folders and metadata events
for combinations; neither converts a combination into a `FolderCandidate`.
Reset cancels obsolete identification/extraction for every affected key. The
mixed-source tests must compare available audio with the reset draft, not only
revision stamps: immutable combination layouts and newly settled folder
layouts must not leave a shared preparation naming unavailable audio.
The source-update transaction verifies that every included draft `AudioFile`
belongs to every resulting candidate. Compatible mixed identities advance
together. If their resulting audio sets cannot support the shared draft, fail
the operation atomically; do not rewrite a frozen combination, fabricate a
shared identity, or save a draft that another affected candidate cannot play.
This check also applies to existing file-role/CUE source updates.

### Implementation ownership and regression paths

The core command and initialization live in `handle/reset.rs`, registered in
`handle/mod.rs`; the whole-preparation operation lives in
`preparations/reset.rs`, registered in `preparations.rs`. Handle regressions
live in `handle/tests/reset.rs`, registered beside the existing pane tests.
Use the real stored-candidate, CUE, combination, and blocking tag-reader
fixtures and reload persisted state after each operation. Test current prefill
on and off, removed audio, changed file/CUE choices, reset after online metadata
and cover selection, repeatability, unavailable source, stale metadata and
file revisions, and late prepared source writes. A combined reset preserves
member state and stored layout. Mixed folder/combination identities exercise
actual source compatibility from both initiating keys.

The database owner changes `client/import_state/preparation_rows.rs`, the
source-set writer in `client/import_state.rs`, and shared snapshot validation,
with regression tests based on `client/tests/file_tag_snapshot_tests.rs`.
Those tests reproduce a stale save changing snapshot bytes before the guard,
file replacement cascading newly inserted facts away, incomplete coverage,
and mixed source-kind updates. UI and bridge work remain separate ownership
within this same concern and use the one canonical Reset operation.

Reset's scanned-state expectation includes the captured scan generation even
when tag prefill is disabled. `CandidateScanExpectation::AtGeneration` holds
the scanned key and generation; existing writes use `Current` when their
contract requires the current source identity and revisions without pinning a
scan pass. The atomic writer checks the generation before dependent writes,
rolling back its revision compare-and-set when the scan has advanced. This
prevents a tag-free Reset prepared against an older scan from landing merely
because that scan retained the content hash and file revision.

### Identification choices

Reset also clears the stored per-candidate disc-ID exclusion, excluded barcodes,
chosen catalogs, and discounted catalogs in the same transaction. They are
import setup decisions; retaining them would make the next identification use
choices from the discarded setup. Reset to tags preserves these choices, as do
ordinary metadata edits. Reuse `replace_lookup_choices_on` with the initial
`LookupChoices` value. Replace the save extras' binary confirm-pick instruction
with the actual three operations: keep choices, confirm an applied pick, or
reset choices. No separate post-save clearing operation is permitted. Test the
real choice write, Reset to tags, and Reset, then reload choices and verify that
an atomic reset failure leaves them unchanged.

Reset also captures the current `LookupChoices` before asynchronous source
preparation. Ordinary lookup-choice writes deliberately do not revise draft
metadata: ranking and lookup decisions are independent of metadata edits.
`CandidateLookupUpdate::Reset` therefore carries those expected choices and
compares the complete current value within the reset transaction. A newer,
different choice refuses the reset and rolls back all changes. This avoids
changing ordinary lookup-write semantics or adding another revision counter.
A blocking tag-reader regression reproduces a choice made while Reset is
preparing; that choice must survive and the prepared reset must fail.
