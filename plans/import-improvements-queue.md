# Import improvements: serial execution plan

## Execution contract

This is the authoritative order for the background worker. Read this file and the
linked task contract before each task; do not rely on conversation memory or
message delivery. Execute all tasks serially in the existing worker worktree,
using a separate focused branch and commit for each concern. Do not stop after
one task or ask whether to continue. Read matching rules and affected source,
complete the implementation design, reproduce bugs with failing production-path
tests, implement, review every requirement, fix findings, run affected checks and
normal hooks, commit, and push. Coordinate parent review and fast-forward landing
on main; report branch, commit, checks, and unresolved failures accurately.

The user has explicitly removed CI as a gate between tasks. Complete local
verification, normal hooks, and requirement/rules review, then land each concern
and continue the queue without waiting for CI. Record CI failures and resolve
them after the queued implementation; the final outcome still requires passing
CI. The parent owns the existing CI watcher and coordinates that final repair.

The user also explicitly replaced the automated per-rule review matrix with one
manual review by us. Stop existing matrix reviewers and do not start or rerun
them. Keep confirmed source findings, make the necessary corrections, review
the diff manually against its contract, and continue. This instruction overrides
earlier matrix requirements in the linked task plans.

Update this plan's execution record with commit and verification evidence as
work advances. Preserve all queued plans in Git. Product UI scope is macOS;
shared canonical model changes update required callers on all platforms. The
Avalonia prerequisite below repairs existing broken callers, not new product
scope. Do not mutate the user's live database or media during verification.

## Order and complete contracts

1. [Remove field-origin tracking](remove-field-origins.md). Applying a metadata
   source overwrites editable values, including manual edits. Remove origin and
   disagreement machinery, not the actual values, source identities, archived
   documents, or evidence-file origins. Keep source application a one-time
   operation. Implementation committed as `e857e2990` on `remove-field-origins`
   and pushed; integrated through `403f2b4de` after parent review.
2. [Repair Avalonia CUE FILE callers](avalonia-cue-file-reference-bindings.md).
   Preserve every FILE reference and its independent association through the
   existing bridge. This is the verification prerequisite before enrichment.
3. [Release and album cross-reference enrichment](release-and-album-cross-reference.md).
   Always obtain the selected release's master/release group when present and
   cross-reference both layers in both directions through provider URL
   relationships. Follow linked release parents; fetch each document once and
   terminate cycles. The selected release wins where it has data, corresponding
   release data fills missing release fields, and master/group data fills missing
   album fields and links. Keep album identity separate from pressing identity;
   never invent a matching pressing from a master/group match. Preserve selected
   track order and known sides. Apply the resulting values as a replacement,
   with no protection of manual edits. Missing years remain absent, never zero.
4. [Unexpected release-selection diagnostics](release-selection-error-details.md).
   Show and allow copying the actual unexpected error through existing alert or
   inline components. Preserve the friendly summary and Retry. Expected provider
   or domain failures, including handled 404/500 cases, do not get this diagnostic
   presentation. Classify by error semantics, not HTTP status alone.
5. [Cover image formats](cover-image-formats.md). Rust accepts JPEG, PNG, GIF,
   and WebP for folder, embedded, and remote covers. Animated images use their
   first frame. Retain normalized static JPEG output, longest dimension at most
   600 pixels with no upscaling, and preserve original files. Align selection,
   validation, and decoding; wider attachment preview support does not imply
   cover eligibility. Test real remote validation and normalization, malformed
   input, decoding limits, transparency, and animation.
6. [Restore unused audio sources](restore-unused-audio-sources.md). Removed
   whole-file and selected CUE-track sources remain visible and individually
   addable using a **+** button with **Add track** on hover. Never say "draft" in
   this product UI. Keep available source audio separate from included tracks;
   do not introduce soft-deleted track rows. Adding must preserve other edits,
   current source order, and current CUE/file associations without duplicating
   audio or resurrecting unavailable slices. Specify new-row metadata and
   numbering using existing construction rules before implementation; do not
   reapply the entire selected release to add one track.
7. [Reset import setup](reset-import-setup.md). Add **Reset** to the existing
   menu beside **Reset to tags** and **Clear metadata**. Restore the initial
   scanned setup: all source tracks, automatic CUE/file assignments, original
   cover choice, and initial metadata respecting **Pre-fill with tags**. Reuse
   initialization, replace state atomically, and prevent stale in-flight source
   operations from overwriting the reset. Files on disk remain untouched.
8. [CUE audio assignment presentation](cue-audio-assignment-presentation.md).
   Correct the misleading **Choose audio…** state for a resolved multi-file
   CUE and expose per-file associations directly rather than hiding them in
   nested menus. This is separate from restoring omitted source tracks.
9. [Release result matching](release-result-matching.md). Compare release
   identity evidence before album text grouping. Normalize identifier formatting
   and semantic pressing values. Label spelling is supporting evidence, not a
   mandatory exact match. Keep ambiguous candidates separate; do not choose the
   first or infer a pressing match from a shared master/release group.

## Existing model and scope guardrails

The existing audio-backed import model remains the foundation. Included tracks
refer to whole files or CUE slices through their audio source. Metadata providers
do not create missing-audio song rows or silently switch CUEs. CUE selection and
FILE associations determine available audio. Unknown side assignments remain
unknown; numbered vinyl tracks without side boundaries remain usable without
inventing a boundary. Do not build artwork extraction, a side-boundary editor,
renumbering facilities, or manual-edit provenance as part of this queue.

Use synthetic fixtures in durable artifacts. Translate new or changed user-facing
strings in every relevant locale. Check stale source/tests/docs references and
report CI independently from local checks; a committed task is not evidence
that main or CI has passed.

## Execution record

- Field-origin removal: `e857e2990`, reviewed and integrated on main through
  `403f2b4de`.
- Avalonia prerequisite: `540ae9326`, pushed on `fix-avalonia-cue-file-bindings`;
  normal hooks passed and all 262 Avalonia view tests passed. Reviewed and
  integrated on main through `403f2b4de`.
- Enrichment: the latest full-core verification against dependency `8506256`
  passed 2,237 tests; native bridge generation and building passed. A macOS
  selection ran nine tests in two suites; the import-store suite identifier
  needed correction, so that result does not cover the store tests. Earlier review
  found and fixed optional-document admission, ambiguous linked identity, and
  mutable partner-archive replay defects; focused verification passed 43 payload,
  10 partner, 76 sweep, 7 reidentify, and 2 partner-snapshot migration tests.
  The actual partner import failed before freezing and passes afterward.
  Persistence review also reproduced historical INSERT/UPDATE failures across
  the record-kind migration. Dependency and host work follows
  [synced-schema-history.md](synced-schema-history.md). The dependency passed
  349 database tests, 1,008 replication tests, three documentation tests, and
  workspace clippy; seven focused converter tests include the added immutable
  identity/clock guards. Remote same-batch INSERT/UPDATE, rollback/retry,
  original/rebased journal versions, recovery, and discard are exercised.
  Host verification passes 57 migration tests, including historical schemas 38–41;
  disabling the origin transformation reproduces the migration 41 replay failure.
  Normal commit hooks found owner-boundary violations; the corrections pass the
  owner checker, 40 database schema tests, 20 replication schema tests, and 16
  migration tests. Dependency commit `8506256faf0859f913e3164ef3680e685caef477`
  passed normal hooks and is pushed in Coven PR 221. Its first rules review
  completed; CI was superseded by the correction push. Review identified six issues, corrected
  together in `f9a8e2302493898d17baeaf94a17ff4c6c9f2ace`: operation-specific
  historical cells and shared migration validation/recovery helpers. Its normal
  hooks passed, as did 40 database schema, 20 replication schema, seven converter,
  and 11 Circle tests. The host pins this corrected revision; verification passed
  57 historical migration tests, 43 import-service integration tests, 31 automation,
  46 bridge, 16 Subsonic unit and 17 Subsonic integration tests, 20 macOS tests in
  five suites, and all 262 Avalonia view tests with regenerated C# bindings.
  Disk exhaustion interrupted an earlier attempt; after removing idle compiler
  output, the checks passed. The .NET test host also needed its generated bridge
  in the native probe directory with the FFmpeg runtime path.
  The affected rules rerun approved the operation model and shared helpers;
  three unused-helper claims are false positives because the host's historical
  migration tests use that facade API. The duplicated test transformation was
  extracted in Coven `63294f5f`; publication and rebase tests and normal hooks
  passed. Parent fast-forwarded Coven main to that revision while preserving its
  existing uncommitted specification edit. The host retains the verified
  production pin `f9a8e230`.
  Enrichment source is committed and pushed as `b9ec6718e`. The user's replacement
  of matrix review with one manual review is active. Manual review identified
  duplicated online/archive traversal and test-only mapper reconstruction;
  corrections share traversal state and remove the test-only metadata algorithms.
  Shared traversal passes all 43 payload tests; all 81 mapper tests pass. The
  unrelated Discogs-document identity regression failed against the removed test
  wrapper and passes through the production projection. Existing provider module ownership remains unchanged; no new provider
  singleton or alternate access path was introduced. Sparse changeset UPDATE
  cells intentionally distinguish undefined from SQL NULL independently before
  and after a change; clearing a transformed no-op does not imply a missing
  domain record. Pressing parent keys intentionally share the pressing catalog,
  rather than permit contradictory cross-catalog parents. Optional parent titles
  contribute no value to the existing blank-aware album merge and never erase a
  selected title. Historical missing-audio durations and their warning predate
  this branch; the existing migration is not rewritten. An empty observed audio
  list must not be described as equivalent to an unknown list, because Discogs
  index selection distinguishes them.
  Enrichment and its manual correction `04f577b26` passed normal hooks and parent
  review, then fast-forwarded to main through `403f2b4de`. The correction also
  passed 19 search and three editor-seed tests. CI is handled at the end under the
  user's revised execution contract.
- Unexpected release diagnostics: implemented on `release-selection-error-details`.
  The production flow regression failed before retaining `DisplayError`; six
  artwork classification cases, three bridge classification cases, and the
  MusicBrainz invalid-request case also failed before their producer corrections.
  A further three real request-cause regressions demonstrated that reqwest's
  display text omitted its underlying cause; the copied diagnostic retains it.
  Verification passed 20 artwork, 21 MusicBrainz, 44 import-service, three
  request-cause, and 68 desktop bridge tests. Native bridge generation and the
  macOS build passed, followed by 43 Swift Testing tests in five suites and one
  XCTest for replacing a focused field. Hosted tests clicked the actual Copy
  details button for short and multiline diagnostics and verified the complete
  original clipboard text; expected 404 failures had no diagnostic controls,
  while Retry remained functional. A stale test member access was found and
  corrected during the build. Parent manual review found no remaining issue in
  the inspected paths. Mobile conditional compilation was read-audited, but no
  mobile cross-build was run for this change. Normal hooks passed; commit
  `f09c20bd3` was reviewed, fast-forwarded, and pushed to main. The final
  post-lint test selection repeated all 13 flow and copy tests successfully.
  CI remains an end-of-queue check.
- Cover image formats: implemented on `cover-image-formats`. Production baselines
  reproduced rejected GIF/WebP inputs, the compact-image heuristic, unsupported
  automatic BMP choices, and the hosted picker offering BMP. Shared bounded
  decoding now handles JPEG/PNG/GIF/WebP, first frames, and static JPEG output;
  attachment galleries retain wider previews. Automatic retained BMP observations
  are skipped while explicit saved choices fail visibly. Tests passed 125 cover,
  34 mapper, seven import cover, 15 snapshot, targeted retained-import and
  transparency cases, 69 desktop bridge, and 262 Avalonia view tests. Native
  generation and the final macOS selection passed 14 tests in three suites:
  CoverPickerTests, LibraryArtworkBrowserTests, and InternReleaseDetailTests.
  Allocation-limit sabotage reproduced the expected failure before restoring the
  cap. The Avalonia native test copy needed its FFmpeg runtime path; native
  relinking also needed idle compiler output reclaimed after disk exhaustion.
  Parent manual requirement review accepted the final automatic-selection and
  transparency corrections. Mobile canonical callers were updated; mobile
  cross-builds remain unexecuted locally. Normal hooks passed and parent
  fast-forwarded/pushed main to `994b3dd72`. The next contract-only commit
  `96d77d916` is also on main. CI remains an end-of-queue gate.
- Restore unused sources: implemented on `restore-unused-audio-sources`.
  The actual drop-and-reload baseline failed because removed audio disappeared.
  Core verification passes 15 restoration/persistence tests and 18 mapping tests,
  including preference-sensitive initialization, exact CUE identity, all-removed
  sheet headers, unchanged surviving swaps, stale source/metadata rejection,
  concurrent adds, combined-disc numbering, and artist-asset rollback. Included
  metadata positions remain independent of physical sheet-disc assignments.
  Native bridge generation passed after exposing the existing audio conversion
  to the new command. The wider import-handle suite passed 106 tests. Hosted
  macOS baseline failed the two editable Add cases; the final selection passed
  29 tests in three suites, including exact viewed-revision forwarding and
  read-only audition. All four Avalonia baseline cases failed their incorrect
  AwaitingPick label; the corrected full view suite passed 266 tests. Parent's
  manual review found that unused CUE sources could inherit invented vinyl
  sides; its regression failed, then all 18 mapping tests passed after keeping
  physical disc labels independent of selected pressing metadata. Add track is
  translated in all 28 macOS locales. Native correction rebuild and normal
  hooks precede coordinated landing; CI remains an end-of-queue gate.
- Remaining entries: queued in the order above. Their linked contracts are
  part of this plan, not optional follow-up work.

## End-of-queue CI evidence

At `f09c20bd3`, Build 35522350486 reports Android/iOS Clippy E0433 in
`bae-core/src/db/client/read.rs:257`: `MetadataRef` is not accessible through its
current conditional re-export. Jobs 106108553203 and 106108553234 retain the
compiler evidence. The macOS build passed, then the 487-test/130-suite run failed
only the multiline diagnostic Retry OCR assertion at
`ReleaseSelectionFailureTests.swift:193` (recognized text `KeLl`); job
106108553114 retains that failure. macOS capture passed; mobile capture failures
still need their exact causes inspected. These are required final CI repairs,
not gates between the queued implementation tasks.
