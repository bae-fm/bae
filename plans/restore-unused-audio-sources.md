# Restore unused audio sources

## Queue and execution
Execute after [cover image formats](cover-image-formats.md), on a separate focused branch in the same background worktree. The serial order is field-origin removal, Avalonia per-FILE CUE caller repair, release/album cross-reference enrichment, unexpected release-selection diagnostics, cover format support, then restoring unused audio sources. Follow research, detailed plan, regression tests, implementation, contract review, normal hooks, and coordinated fast-forward integration.

## User contract
Audio removed from the release's track list remains visible and individually addable. Use a plus button with the localized hover text “Add track”. Do not use “draft” in product UI. Adding one source must not reset any other track or reapply the selected release's metadata as a whole.

Cover whole audio files and individual slices from the currently selected CUE sheets. Prevent duplicate inclusion, preserve source order, respect current CUE selection and FILE bindings, and never restore a slice that is unavailable under the current source configuration. Keep the audio-backed track model and one-time metadata application; do not add field-origin or manual-edit protection machinery.

## Confirmed research starting point
The existing apply_track_edit path in preparations/pane_edits.rs removes CandidateTrack outright. mapping.rs draft_mapping_table gathers source units but renders the included draft.tracks and omits unused units. Consequently, both whole files and CUE slices can lose their visible route back into the release.

Read the complete production path before implementation, including source unit identity/order, track removal and candidate persistence, mapping projection, source selection changes, and existing track construction/defaults. Preserve removal as actual removal of the included row; represent the remaining available audio independently from included tracks rather than adding a soft-delete flag.

## Metadata and numbering design to settle before implementation
Document the exact metadata and number assignment rules after tracing existing source-to-track constructors. Reuse the same source metadata/default policy that creates audio-backed tracks, including the pre-fill-with-tags preference, explicit CUE metadata, and the required number default based on ordered track position. Specify whether and how re-adding can reuse a removed row's values without introducing hidden field-history or retention mechanisms; the contract does not authorize silently restoring stale release metadata.

The plan must explicitly cover a removed whole file, a removed selected CUE slice, mixed existing edits, absent source title/number, and source order across multiple discs. State how a new track's number is chosen without renumbering or rewriting existing rows. Do not infer an audio-to-release match merely from list position or reapply all selected catalog metadata to accomplish one addition.

## Required regression cases
- Remove a whole-file track; its source stays visible with Add track. Add it and verify the actual included row returns while all other metadata and audio assignments remain unchanged.
- Remove one slice from an active CUE; only that slice can be added, using its exact current file/sheet/index identity.
- Ignore or replace a CUE after removing a slice; unavailable slices are not offered or resurrected. Whole-file availability follows the current source configuration.
- Repeated or stale add commands cannot create duplicate inclusion. Source order remains deterministic after removing and restoring a middle track.
- Per-FILE CUE associations and partially unavailable sheets retain the existing source validity rules.
- Verify metadata/default/numbering decisions from the completed design against real production construction and persistence, not duplicated test logic.
- Exercise the actual macOS rendering and plus-button command path; translate the hover string and any other user-facing wording across all relevant catalogs.

## Constructor and write contract

The source trace for this contract is `track_slots.rs`, `pane.rs`,
`types/raw_release_edit.rs`, `file_tag_mapper.rs`, `file_tags_seed.rs`,
`release_candidate.rs`, `handle/scan.rs`, `handle/edits.rs`,
`preparations/pane_edits.rs`, and `mapping.rs`. The available audio list already
exists independently of included tracks: `audio_units(files)` derives whole
files and selected CUE slices from the current file roles, sheet choices, and
FILE associations. Use that list; do not persist an additional unused-source
list or a removed-track archive.

### New row metadata

Restoration creates a new included row from the current source configuration.
It does not recover the removed row's manually entered or online metadata.
Construct the full source seed through the existing initialization projection,
then take the one row with the requested exact `AudioFile` identity. Do not
construct a one-track candidate: its position-based default would incorrectly
become 1, and combined candidates would lose their source ordering and grouping.

With Pre-fill with tags enabled, reuse `FileTagsSeed::project` over the
candidate's current tag snapshot, with no included-track filter. Whole-file
titles, artists, sides, and valid positive track numbers come from the existing
file-tag mapper. A missing whole-file title uses the file stem. CUE slice titles
and performers come from the selected CUE entry, its side from the sheet's disc
assignment, and its number from its valid positive CUE number. A missing CUE
title stays editable and empty, as in initialization; the source row still
identifies the slice. Preserve the initializer's artist assignment semantics;
do not guess new artist identities from current displayed names.

With Pre-fill with tags disabled, reuse `ReleaseCandidate::blank_source`:
title stays empty, artist assignment inherits the current album artists, whole
files have unknown side, and selected CUE slices retain their physical disc
assignment. This setting also suppresses CUE title/performer prefilling; it does
not disable slicing. Combined candidates use their existing source projection
and numbering rather than being reconstructed as a single folder.

The existing initializer supplies a required number: a valid source number when
used, otherwise the source's one-based position in the complete available-audio
order. Retain that number when adding. Do not number from the shortened included
list, change surviving numbers, or invent a collision-avoidance numbering policy.
The source projection already permits a default number alongside partially tagged
numbers; restoration does not add a renumbering facility.

Assign a fresh row ID through the injected ID provider and set `source_index`
to absent. A current online release is not evidence that this newly added audio
corresponds to one of its tracks. Preserve all surviving rows, including their
IDs, metadata, numbers, audio assignments, and source indices; preserve album
fields, cover, provenance, and the applied-source snapshot.

### Inclusion and ordering

The add command names one exact currently offered audio identity, not a title,
track number, removed row ID, or list index. A slice identity includes its
container, sheet, and playable-entry index. Carry enough viewed source revision
information to reject a stale offer after a rescan or source configuration
change; checking only the filename is insufficient. Revalidate availability and
candidate editability when committing.

Insert relative to the current full source order without sorting or rewriting
surviving rows. Place the new row before the first surviving row whose audio
comes later in that source order; append when there is none. For the normal
source-ordered list this restores the removed middle position, including across
multiple discs. If a user has swapped audio between existing rows, preserve
those rows' relative order and assignments rather than undoing the swap.

Already included audio is a no-op after checking that the candidate remains
editable. Concurrent requests based on the same preparation must either observe
that inclusion or fail the existing revision check; they cannot append twice.
Ignoring/replacing a CUE removes its slices from the offer set. Restoring a
slice never changes the CUE FILE association, and it never makes the whole
container concurrently available as another track.

### Atomic preparation and rendering

Follow the existing prepared pane-edit path: build the changed draft and required
artist-image set, then save them together under `CandidateAsRead` file and
metadata revisions and the scanned-candidate identity. Reuse
`prepared_artist_images_for_active` with the resulting included tracks; an asset
failure leaves the candidate unchanged. Do not call metadata application to add
a row, clear identification merely because an included row changed, or fetch
the selected online release again.

Project unused sources from current available audio minus included audio in
core, with their existing source names, durations, and audition targets. Expose
an explicit not-included state to the existing mapping views; an omitted source
is not AwaitingPick, not Missing, and not an editable track with blank metadata.
Keep selected CUE controls reachable even when every one of their entries was
removed. The macOS control is the existing plus icon pattern with Add track
help/accessibility text; it sends the core-provided source identity. UI code
does not rebuild source order or decide which slices are available. Update
shared exhaustive consumers when extending a canonical mapping enum, without
adding unrelated platform interaction work.

Add assertions for the exact restored source number/title policy with prefilling
on and off, a restored row after online metadata application retaining no source
index, a completely removed selected CUE retaining controls, and existing audio
swaps remaining intact. These extend the required regression cases above.

## Separate observed issue
The macOS multi-file CUE header can say “Choose audio…” despite valid associations because ImportSheetBindingMenu uses containerName ?? placeholder and describesFiles has no single container name. This is an observed presentation issue pending design. Do not bundle a menu redesign into restoring unused audio sources.

## Delivery
Product work targets bae-macos; update shared canonical models and required callers together. Do not edit live database or media. Review the implementation against every contract item, run affected core/bridge/macOS checks and normal hooks, commit, then coordinate the main fast-forward landing.

## Queued successor
After individual source restoration lands, execute [Reset import setup](reset-import-setup.md). Reset is a separate menu action restoring the whole candidate to its initial scanned setup, including source tracks, automatic CUE/file choices, cover, and preference-dependent initial metadata.

## Concrete command and projection design

Expose `add_candidate_track(candidate_key, audio, CandidateAsRead)`. The existing
read record carries content hash plus file and metadata revisions; do not invent
a partial revision type. The candidate detail SQL snapshot already has all three
values. Pass that read into the draft pane/table projection and carry it beside
the exact audio identity in each NotIncluded offer. The bridge mirrors those
existing values, and the plus action sends the rendered offer without looking
up a newer UI state to replace its revision.

An actual insertion rejects a changed metadata revision through the existing
CandidateAsRead contract. An already included source may return without writing
only after the commit lock has verified editability and the viewed source hash
and file revision. Prepare the complete source seed and required artist images
before the final locked write; avoid holding the candidate commit lock across
snapshot operations that acquire it themselves. Recheck source availability,
editability, and revisions at commit, then atomically store the inserted row and
its prepared asset set. No source reapplication, verdict reset, schema change,
or removed-row history participates.

The implementation ownership within this concern is disjoint: the core worker
owns handle/edits.rs, preparations/pane_edits.rs, necessary constructor helpers,
and actual command/persistence regressions. The coordinator owns mapping/table
projection, canonical bridge callers, macOS controls/translations, and their
projection/hosted tests. Both share one branch/index/build schedule. Add the
handle tests beside metadata_edits using the existing stored_candidate,
pane_fixture, rescan_into, track_rows, and shut_down fixtures. Establish the
initial failure by removing an actual stored track and reading its missing
source back through the production pane before implementing restoration.
