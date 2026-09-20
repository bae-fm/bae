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
   and pushed; parent review and main integration remain required.
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

- Field-origin removal: `e857e2990`, pushed on `remove-field-origins`; awaiting
  parent review and main integration.
- Avalonia prerequisite: `540ae9326`, pushed on `fix-avalonia-cue-file-bindings`;
  normal hooks passed and all 262 Avalonia view tests passed. Parent review and
  main integration remain required.
- Remaining entries: queued in the order above. Their linked contracts are
  part of this plan, not optional follow-up work.
