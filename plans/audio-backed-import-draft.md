# Audio-backed import drafts

## Scope and outcome

Implement the import model in bae-core, the Rust bridge, and bae-macos. The user
explicitly excluded Avalonia implementation from this task. Do not add a track
renumbering editor, side-boundary editor, or artwork extraction mechanism.

The draft describes the audio intended for import. It owns an ordered list of
tracks and editable album/release metadata. Metadata application is an explicit,
one-time assignment, not a binding that subsequently rebuilds the draft from a
provider's track list. This supersedes the row-reconstruction and persistent
exclusion behavior described in import-metadata-draft-and-sources.md.

## Contract

### Audio and tracks

Each draft track requires an audio source: a whole scanned file or one playable
slice identified by its file, CUE, and slice index. Track metadata contains its
title, artist assignments, required number, and optional disc/side information.
Store ordering independently of numbering. Preserve supplied numbers; initialize
missing numbers from ordered position. Do not invent side boundaries.

The available files and CUE definitions are separate from the draft's included
tracks. Deleting a track deletes its draft row, without deleting a physical file
or retaining a dropped-row tombstone. Applying metadata only describes included
tracks; it cannot recreate deleted rows. Explicitly changing the corresponding
audio/CUE choice can create tracks again.

Remove metadata-only draft tracks, missing-audio assignment rows, the positional
remapping of whole discs onto song metadata, and hidden exclusion carry-forward.
Keep library metadata editing distinct where an existing library audio binding
does not need to cross the editing boundary.

### CUE associations and selection

A CUE's file associations answer which actual audio file each FILE reference
describes. Its selection answers whether its track boundaries are used. Ignoring
a CUE retains its associations. Independent CUEs can coexist; competing CUEs
covering the same audio require an explicit choice. Do not silently switch CUEs
when selecting online metadata.

Every required audio reference must resolve before a CUE can be activated. For
14 available files and a CUE referring to 15, show the specific unresolved
reference and allow association with the actual file if its name differs.
Resolve references individually rather than inferring validity from counts.
Never activate only the resolved portion or invent a missing-audio draft track.
A CUE may have multiple tracks per referenced file.

Changing a CUE choice replaces only tracks backed by the affected audio:
whole-file tracks become CUE slices, ignored slices become whole-file tracks,
and selecting another CUE replaces the old slices. Unaffected tracks preserve
their identities, audio, metadata, and edits. Album/release metadata remains.
Removed slice metadata never transfers onto a whole-file track. Commit file
decisions and the replacement draft atomically, with revision checks; any
failure leaves the prior draft and decisions intact.

### Initial metadata

Respect the existing Pre-fill with tags preference both at discovery and when
audio changes create replacement tracks. When enabled, use whole-file embedded
tags or CUE metadata, with filename defaults for absent titles and ordered
position for absent numbers. When disabled, descriptive metadata is blank and
required numbering is initialized from order. Explicit file/CUE metadata
application remains available independently of the preference.

### Metadata application and mismatches

File/CUE metadata and MusicBrainz/Discogs metadata are applied once to existing
audio-backed draft tracks. Reapplication replaces relevant metadata, including
manual metadata edits, without changing audio choices or resurrecting tracks.
Keep provenance as an account of the applied source, not a live binding.
Preserve the applied provider documents and the audio durations that selected
Discogs' index/sub-track layout. Supplemental work and role credits refer to
tracks in that frozen interpretation; deleting or replacing audio cannot make
those credits attach to another source track. Another lookup updating the shared
provider cache must not alter this candidate's applied metadata.

External metadata requires a compatible correspondence between its ordered
tracks and the draft, accounting for known disc/side grouping. Equal counts do
not by themselves establish compatible grouping. Unknown grouping is not a
contradiction. A mismatch leaves the draft unchanged and shows the selected
release and a useful explanation for review. Do not silently apply a subset,
replace audio choices, or assume a particular cause for the mismatch: inputs
may be individual files, CUE slices, or a mixture.

### Numeric vinyl metadata with unknown sides

A MusicBrainz vinyl release with one medium and tracks numbered 1 through 12,
without side assignments, is usable. Preserve its vinyl format, ordered tracks,
and numeric track numbers. Represent unknown sides as absent, not side A or a
guessed boundary. If its tracks correspond to the audio, allow application and
render an ordered list without side headings. Treat incomplete provider
positions consistently across MusicBrainz and Discogs; retain side assignments
where they are actually supplied. A CD's whole disc is its single side/group.

## Implementation

1. Trace candidate creation, file decisions, metadata application, persistence,
   import consumption, provider position mapping, and the macOS projection.
   Reuse AudioFile and the existing whole-file/CUE audio distinction.
2. Add failing behavioral tests for replacing CUE slices, metadata mismatches,
   unresolved multi-file CUE activation, persistent deletion, prefill settings,
   and numeric vinyl metadata with unknown sides. Exercise the production
   operations and their atomic persistence boundaries.
3. Make candidate draft audio required, numbers required, and side information
   optional. Remove the dropped-row and fileless-row mechanisms at their roots.
   Add ordered database migrations for persisted shape changes; preserve valid
   stored edits and migrate invalid legacy rows deliberately and atomically.
4. Replace positional audio remapping with identity-based retention of unchanged
   audio and initialization of changed audio. Validate complete CUE associations
   and competing selections before committing the file decision.
5. Apply source metadata onto the current track list through explicit
   compatibility validation. Keep failure visible in source review and leave
   the draft/provenance/cover unchanged on failure. Remove manual-field and
   dropped-row carry-forward from explicit metadata replacement.
6. Propagate unknown side information through provider parsing, candidate and
   library persistence, import, bridge, and macOS grouping; preserve explicit
   track ordering wherever side/number sorting formerly implied it.
7. Update macOS to render audio-backed rows, source mismatch explanations,
   unresolved file associations, removal, and non-editable required numbering.
   Update bridge definitions and macOS fixtures. Translate every new or changed
   string in all supported locales; remove orphan strings.

## Verification

- Four image files with active CUEs produce 56 tracks; ignoring all sheets
  produces four whole-file tracks, with no song metadata moved by position.
  Reactivating sheets produces their slices, respecting prefill settings.
- Whole-file tags populate the new whole-file tracks; absent tags use filename
  defaults only with prefill enabled. Unaffected rows retain user edits.
- A 15-track external release cannot mutate a 14-track draft. A known grouping
  mismatch also refuses application; unknown grouping alone does not.
- Mixed standalone/CUE imports and independent multi-file sheets work. Missing
  references and competing sheets cannot partially alter the draft.
- Deletion survives reopening and metadata application without stored dropped
  rows. Explicit incompatible metadata application does not restore deletion.
- Numeric vinyl/cassette positions without side boundaries parse, apply,
  persist, import, and display without invented headings. Explicit sided
  releases and multi-disc CDs retain their grouping.
- Test migration from released schema, transactions and revision races, focused
  core/import/database tests, bridge checks, macOS build/tests, and catalog
  checks. Review the full diff against every contract above; search for removed
  patterns and stale docs/fixtures. Run normal commit hooks, commit and push,
  then inspect the relevant CI results. Report validation limits accurately.

## Execution record

Implemented the required-audio draft, one-time metadata application, CUE
replacement and per-reference associations, unknown sides, ordered persistence,
and the macOS controls. Added migrations 037–040, including frozen applied
provider documents for supplemental credits. Avalonia was not changed.

Behavioral checks passed: 2,196 core unit tests; the later import subset (798),
candidate persistence (80), migrations (48), desktop bridge (60), and the affected
import/delete/reset/validation/CUE integration suites. Additional tests exercise
numeric vinyl through import and database readback, prefill on/off during CUE
replacement, and provider-cache changes after metadata application. Desktop
clippy and macOS build/dead-code checks passed.

The focused macOS import tests passed. Full macOS testing also exposed snapshot
failures in appearance and document presentation and test-daemon launch timeouts;
these are not counted as passing validation. Final commit-hook and CI results
are reported with the implementation commit.
