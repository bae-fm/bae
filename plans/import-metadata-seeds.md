# Import metadata seeds and artist assignment

## Product contract

An import candidate is a scanned collection of files before it is a release.
Selecting the candidate opens a neutral mapping pane. No release metadata is
implied merely because the pane is open.

The metadata section offers three equal ways to seed the release:

- **Lookup**: choose a specific MusicBrainz or Discogs release.
- **File Tags**: read metadata stored in the candidate's files and parsed CUE
  sheets.
- **Manual**: begin with blank release metadata and enter it directly.

Switching among these surfaces is navigation. It does not change the stored
seed. Each surface has its own affirmative action:

- selecting a pressing chooses an external-release seed;
- choosing `Use File Tags` chooses the stored file-tag snapshot;
- choosing `Enter Manually` chooses the blank manual seed.

The editor and Import action belong to the chosen seed. A candidate with no
chosen seed remains Pending and cannot be bulk-imported. The file list and its
role controls remain available because they describe the candidate, not its
metadata seed.

Automatic online lookup is controlled by a device-local Import setting. The
whole identification pipeline belongs to Lookup: signal extraction, including
DiscID, barcode OCR, catalog text, provider search, and matching runs in the
background only when an unseeded candidate resolves to Lookup and automatic
lookup is enabled. Manual and File Tags candidates perform none of that work.
Lookup opens with no search having run and offers `Find this release` when
automatic lookup is disabled. Explicitly opening Lookup may start an interactive
identification run regardless of the automatic setting. Stored results from an
earlier lookup remain visible; the setting controls new background work, not
whether existing evidence may be read.

A second Import setting chooses which metadata surface opens for an unseeded
candidate: Lookup, File Tags, Manual, or Last Used. Lookup is the default for a
new configuration so existing behavior remains recognizable. Last Used resolves
to the last surface the user explicitly selected; opening a candidate or
receiving a detail refresh does not rewrite that history. A candidate that
already has a stored seed always opens the surface corresponding to that seed.
The default surface determines whether an unseeded candidate is eligible for
background identification. The automatic setting is the second gate: Lookup
may still open empty when automatic lookup is disabled.

## Existing code that changes

The live implementation conflates several concepts:

- `IdentityPick::Unknown`, `IdentityChoice::Unknown`, bridge equivalents, and
  `pick_kind = 'unknown'` all mean “seed from File Tags.” They do not represent
  an unknown identity independently of metadata provenance.
- `ImportIdentity.release | unknown` is the macOS navigation state, while
  `BridgeIdentityPick` is the persisted decision. File Tags navigation currently
  writes the persisted decision immediately.
- `pane::unknown_pane`, `start_import`, and the import worker each reconstruct
  file-tag metadata by opening the audio files. The display and commit therefore
  do not consume one stored reading.
- `ReleaseUserEdit` and `RawReleaseEdit` reduce artist credits to names and
  comma-separated text. `resolve_artists_for_import` then takes the first
  case-insensitive name match when external IDs do not decide the artist.
- `ReleaseMetadataSource` and `releases.metadata_source` have no Manual case.
- The queue sweep starts online identification without a user setting that can
  disable it.

The change removes those conflations rather than layering new flags over them.

## Domain model

`IdentityPick` becomes `MetadataSeed`:

```rust
pub enum MetadataSeed {
    ExternalRelease {
        source: MetadataSource,
        release_id: String,
    },
    FileTags,
    Manual,
}
```

`MetadataSource` continues to mean an online provider and remains
`MusicBrainz | Discogs`. File Tags and Manual are seed kinds, not providers.

Navigation and its preference use separate types from the stored seed:

```rust
pub enum ImportMetadataMode {
    Lookup,
    FileTags,
    Manual,
}

pub enum DefaultImportMetadataMode {
    Lookup,
    FileTags,
    Manual,
    LastUsed,
}
```

`ImportMetadataMode` says which surface is presented. `MetadataSeed` says what
will be committed. `DefaultImportMetadataMode` is configuration policy rather
than either of those facts. The concrete `last_import_metadata_mode` is stored
separately so changing the policy away from Last Used does not erase history.

The same `MetadataSeed` value flows through candidate state, pane projection,
the bridge command, `ImportCommand`, and import preparation. Delete
`IdentityChoice` and `MetadataPointer` where they duplicate this value. Library
re-identification remains an identity operation, but it must use a separately
named type rather than bringing `IdentityChoice` back into import. Its existing
“remove identity and reread local tags” action maps explicitly to
`MetadataSeed::FileTags`.

Only `ExternalRelease` asserts a release identity. Its provider payload produces
`release_identities` rows. File Tags and Manual produce no identity rows and
therefore import onto a fresh album.

The committed release records the seed provenance:

```text
MetadataSeed::ExternalRelease(MusicBrainz) → releases.metadata_source = musicbrainz
MetadataSeed::ExternalRelease(Discogs)     → releases.metadata_source = discogs
MetadataSeed::FileTags                     → releases.metadata_source = file_tags
MetadataSeed::Manual                       → releases.metadata_source = manual
```

`metadata_source_release_id` is present only for `ExternalRelease`. A Manual
release has no source to reset from, so the library editor must not offer Reset
to Source for it. File Tags reset continues to reread reachable local files;
provider reset continues to replay `source_release_payloads`.

## Candidate and library data flow

```text
FILESYSTEM
    │
    ▼
watched_import_folders
    │ scan
    ▼
folder_scan_roots
    └─ scan_candidate
         ├─ scan_candidate_file
         ├─ scan_cue_sheet / scan_cue_track / scan_cue_index
         └─ scan_candidate_resolved_boundary
                    │
                    ▼
              SELECT CANDIDATE
                    │
                    ▼
        Neutral pane, metadata_seed = NULL
                    │
       ┌────────────┼────────────┐
       ▼            ▼            ▼
    LOOKUP       FILE TAGS      MANUAL
       │            │            │
 provider search    │ lazy read  │ blank seed
       │            ▼            │
       │   scan_candidate_tag_snapshot
       │      └─ scan_candidate_file_tag
       ▼            │            │
 signals/matches    │            │
 source payloads    │            │
       │            │            │
 choose pressing  Use Tags   Enter Manually
       └────────────┼────────────┘
                    ▼
       import_candidate_state.metadata_seed
                    │
       candidate edit and artist-assignment rows
                    │
                 IMPORT
                    │ one candidate expectation,
                    │ one metadata seed,
                    │ one final library transaction
                    ▼
       albums → releases → tracks → release_files
          │          ├─ release_identities (external only)
          │          ├─ audio_formats / segments
          │          └─ covers
          └─ artists and ordered artist-credit rows
```

## Candidate-state schema and migration

Keep `bae-core/migrations/001_initial.sql` immutable and add
`002_import_metadata_seeds.sql` to the contiguous ladder in `migrations.rs`.
This is a real database migration, not a compatibility decoder: an existing
library is transformed once, and every runtime reader supports only the new
shape. A fresh library applies migrations 1 and 2 in order.

Rename the candidate-choice columns so the database states what they contain:

```text
pick_kind            → seed_kind
pick_source          → seed_source
pick_release_id      → seed_release_id
identity_pick_author → metadata_seed_author
```

`seed_kind` accepts `external_release | file_tags | manual`. The source and
release ID are both present exactly when the kind is `external_release`.
`metadata_seed_author` remains `user | identification`; identification may
choose only `external_release` after a unique settled result. File Tags and
Manual are explicit user decisions.

Migration 2 is registered with `Migration::run`: its checked-in SQL owns the
table rebuilds and constraints, while its Rust backfill calls the same artist-
text parser the version-1 editor uses before those columns are removed. Existing
candidate decisions map without inference:

```text
pick_kind = release  → seed_kind = external_release
pick_kind = unknown  → seed_kind = file_tags
pick_kind = NULL     → seed_kind = NULL
```

It renames `identity_pick_author` to `metadata_seed_author`, preserves provider
source and release IDs only for external releases, replaces the string artist
edit columns with the normalized assignment tables below, and creates the tag
snapshot tables. Migration 2 converts each version-1 comma-separated artist
override into ordered `NewArtistSeed` rows using the version-1 trim-and-drop-
blank rules; it never links by name. A value that the version-1 form rejects
also makes the migration fail rather than silently changing the credit.

The migration runs atomically and ends with `PRAGMA foreign_key_check`. Tests
build a version-1 database with representative external, File Tags, unpicked,
edited, and child-row states, apply migration 2, and verify both preserved data
and new invariants. A failed migration leaves the version-1 database intact.

The stored identify verdict, signals, durations, file decisions, and candidate
seed remain independent portions of `import_candidate_state`. Disabling
automatic lookup leaves the identify portion absent; it does not create a
sentinel verdict.

Changing the chosen seed clears album/track metadata edits derived from the old
seed. File-role decisions and measured durations remain because they describe
the candidate's files. A remote cover selection is cleared when it does not
belong to the new external seed; a local folder-cover selection remains valid.

## File-tag snapshot

Provider documents remain raw JSON in `source_release_payloads` because they
are provider-owned documents replayed as a whole. File tags are application-
owned fields and use relational rows rather than an opaque JSON document.

```text
scan_candidate
├─ watched_folder_path ─┐
├─ path                 ├─ candidate key
├─ generation           │
├─ file_edit_revision   │
└─ content_hash         ┘
          │ 1
          ▼
scan_candidate_tag_snapshot
├─ watched_folder_path ─┐
├─ candidate_path       ├─ primary key
├─ scan_generation      │
└─ file_edit_revision  ─┘
          │ 1
          ▼ many
scan_candidate_file_tag
├─ watched_folder_path ─┐
├─ candidate_path       ├─ primary key
├─ relative_path        ┘
├─ file_size
├─ modified_at_ns
├─ content_type
├─ title
├─ track_artist
├─ album_title
├─ album_artist
├─ year
├─ track_number
└─ disc_number
```

Both tables are device-local and descend from the scanned candidate. They have
no `_updated_at` and do not enter `synced_tables()`.

Opening File Tags performs this operation:

1. Read the candidate's scan generation, file-edit revision, and relevant audio
   rows.
2. Stat each source file and compare its size and modification time with the
   stored snapshot.
3. If every observation matches, project from the stored rows without opening
   audio tags.
4. Otherwise read all relevant files into memory. Insert the snapshot and every
   file-tag row in one transaction only after every read succeeds.
5. If any read fails, return that file-specific failure and write no new
   snapshot. An older snapshot whose observations no longer match is never read.

There is no loading, completion, or error column. Loading belongs to the active
command; a successful snapshot exists as a whole, and failure is returned to
the initiator.

The snapshot stores extracted facts, not `ParsedAlbum`, generated row IDs, or a
prebuilt editor form. File Tags projection combines these facts with the parsed
CUE rows and the candidate's file decisions to construct the seed. This keeps
tag extraction separate from how a release is assembled.

Import validates the snapshot's candidate generation, file-edit revision, and
file observations against the expectation used by the pane. A mismatch fails
before library writes. The worker derives `ParsedAlbum` from the stored snapshot
instead of reopening tags, so the metadata shown in the pane is the metadata
committed.

## Seed projection

The candidate-detail projection must represent navigation separately from the
stored seed. Core supplies the data each surface renders; platform UIs select a
surface and render it.

```rust
pub struct CandidateMetadataPane {
    pub selected_seed: Option<MetadataSeed>,
    pub selected: Option<MetadataSeedProjection>,
    pub file_tags: Option<MetadataSeedProjection>,
    pub manual: MetadataSeedProjection,
}
```

`file_tags` is absent until a successful lazy read. `manual` is derived from the
scanned audio structure and requires no I/O. `selected` is the chosen seed with
the persisted edit overlay applied. Lookup results continue to come from the
identify state and explicit search results; an external release is fully
projected only after its row is chosen and its payloads are stored.

`MetadataSeedProjection` contains the release form, mapping table, cover
options, and artist assignments. The unpicked mapping remains available outside
it so file roles can be edited before any seed is chosen.

Manual projection uses no folder name, filename, CUE title, embedded tag, or
provider value as release metadata. It derives only the physical track slots
needed to bind audio. Album title and album artists are blank. Track titles are
blank; source filenames remain visible in the Source column. CUE slicing and
disc assignment remain because they describe where audio samples live, not who
or what the release is.

## Artist assignment

Every album-artist and track-artist position stores an explicit assignment:

```rust
pub struct NewArtistSeed {
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

pub struct ExistingArtist {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub musicbrainz_artist_id: Option<String>,
    pub discogs_artist_id: Option<String>,
}

pub enum ArtistAssignment {
    Existing { artist: ExistingArtist },
    New { seed: NewArtistSeed },
}

pub enum TrackArtistAssignments {
    AlbumArtists,
    Explicit(Vec<ArtistAssignment>),
}
```

`ReleaseUserEdit.album_artist_names` becomes ordered
`album_artist_assignments`. `TrackUserEdit.artist_names` becomes
`TrackArtistAssignments`; an empty vector no longer doubles as “inherit the
album artist.” `RawReleaseEdit` and `RawTrackEdit` stop carrying comma-separated
artist strings.

Candidate artist overrides are normalized into rows:

```text
import_candidate_album_artist_assignment
├─ content_hash
├─ position
├─ assignment_kind = existing | new
├─ artist_id                         existing only
├─ name                              new only
├─ sort_name                         new only, optional
├─ musicbrainz_artist_id             new only, optional
└─ discogs_artist_id                 new only, optional

import_candidate_track_edit
└─ artist_assignment_kind = album_artists | explicit

import_candidate_track_artist_assignment
├─ content_hash
├─ track_id
├─ position
└─ the same existing/new assignment columns
```

The assignment columns use CHECK constraints matching the Rust variants. Album
assignment rows are an ordered replacement of the seed's album credits. A
track edit explicitly says whether it inherits album artists or owns an ordered
list, so zero explicit artists cannot be confused with an untouched seed.

The candidate assignment tables foreign-key `artist_id` to `artists` with
`ON DELETE RESTRICT`. A pending choice is a real reference: deleting an artist
must first resolve every candidate assignment that names it. Candidate reads
join the canonical artist row and return an `ExistingArtist`; only `artist_id`
is persisted in the assignment row.

Artist resolution follows authoritative state:

- A MusicBrainz seed searches `artists.musicbrainz_artist_id` exactly.
- A Discogs seed searches `artists.discogs_artist_id` exactly.
- A cross-linked seed may search both exact IDs and must reject disagreement.
- A File Tags or Manual name is a new-artist seed until the user selects an
  existing library artist.
- Name search returns suggestions; it never silently turns text into an
  identity or takes the first same-name row.
- `Existing` commits that exact library artist ID.
- `New` with a provider ID performs one final exact-ID lookup inside import
  reconciliation to handle a concurrent insert, then inserts if absent.
- `New` without provider IDs inserts a new artist even when another artist has
  the same name.

Remove the name fallback from `resolve_artists_for_import`. Keep its exact
MusicBrainz, Discogs, and Various Artists handling, but feed it explicit
assignments rather than asking it to infer what the user meant.

## Artist picker UI

Album and track artist fields become one shared token field on macOS and
Avalonia:

- Seeded artists appear as tokens in source order.
- A token linked to the library carries a library-link indicator and uses the
  canonical library name.
- Typing searches existing library artists by name and shows all matching rows,
  with stable library IDs behind them.
- Choosing a result stores `ArtistAssignment::Existing`.
- Choosing `Create “…”` stores `ArtistAssignment::New`.
- Editing a linked token does not rename the library artist; removing it and
  creating text produces a new assignment.
- Track rows expose `Album artists` versus explicit track credits directly.

Add a core artist-search query returning the fields needed to distinguish
same-name rows: library artist ID, name, sort name, source IDs, and image
reference where available. Search ordering is exact name, prefix, then
substring, with a stable name/ID tie-breaker. The bridge transports results;
the UIs do not query or resolve artists independently.

## Import settings

Add `automatic_import_metadata_lookup: bool` to `ConfigYaml`, `Config`, defaults,
validation fixtures, `BridgeConfig`, and both desktop settings surfaces. The
default is `true`, preserving the existing behavior for a newly created
library. The setter carries an absolute Boolean and writes config atomically.

Add these device-local configuration fields beside it:

```text
default_import_metadata_mode = lookup | file_tags | manual | last_used
last_import_metadata_mode    = lookup | file_tags | manual
```

Both default to `lookup`. The existing macOS Import settings tab gains a
Metadata section above Transfers with an `Open unseeded candidates in` picker
for Lookup, File Tags, Manual, and Last Used, plus the automatic-lookup switch.
Avalonia has one scrolling settings window rather than tabs, so it gains an
Import section with the same controls. The bridge exposes both enums and all
three values through `BridgeConfig`.

An explicit Lookup/File Tags/Manual tab selection writes
`last_import_metadata_mode` through an absolute setter. Resolving Last Used,
opening a seeded candidate, changing the configured default, and receiving
candidate-detail updates do not write it. The currently open candidate keeps
its presented mode across detail refreshes; selecting another candidate resolves
its mode from the stored seed first, then the configured default.

The queue sweep reads this setting and the candidate's resolved metadata mode
before admitting background identification. Turning it off, or changing the
resolved default away from Lookup, cancels queued and running background
identification and removes their runtime progress, without deleting settled
verdicts or selected seeds.
Interactive Find, provider-row selection, and release re-identification do not
consult the setting. Turning it on schedules unresolved actionable candidates.

The queue progress indicator is absent while automatic lookup is disabled and
no interactive identification is running. A candidate with no stored verdict
or seed remains a normal neutral Pending row rather than receiving a fabricated
“not found” classification.

## Core operations

Replace `pick_candidate_identity` with commands whose names describe their
effects:

```text
select_candidate_metadata_seed(candidate_key, MetadataSeed)
load_candidate_file_tags(candidate_key)
start_import(candidate_key, storage_mode, pin)
search_library_artists(query, limit)
replace_candidate_album_artists(candidate_key, assignments)
replace_candidate_track_artists(candidate_key, track_id, assignments)
set_automatic_import_metadata_lookup(enabled)
set_default_import_metadata_mode(mode)
set_last_import_metadata_mode(mode)
```

Selecting `ExternalRelease` fetches and stores all required provider documents
before storing the seed. Selecting File Tags requires a valid stored snapshot.
Selecting Manual requires only an actionable scanned candidate. Seed
replacement and clearing seed-derived edits are one transaction.

`start_import` reads the stored `MetadataSeed`; it does not accept a provider
payload, tag values, or artist names from the UI. The command carries candidate
and revision identifiers. Preparation dispatches directly on the seed variant:

```text
ExternalRelease → archived provider payload projector
FileTags         → stored tag snapshot + parsed CUE projector
Manual           → blank manual projector over physical track slots
```

No branch probes the other sources as a fallback. A missing payload, snapshot,
or artist row is an error naming the broken invariant.

## Bridge and desktop surfaces

Rename bridge types and methods with the core model:

- `BridgeIdentityPick` → `BridgeMetadataSeed`
- candidate `picked` → `metadata_seed`
- candidate `claim` becomes an external identity projection only where the
  sidebar needs it; File Tags and Manual have no claim.
- `pick_candidate_identity` → `select_candidate_metadata_seed`
- bridge edit records carry `BridgeArtistAssignment` and
  `BridgeTrackArtistAssignments`.

The macOS `ImportIdentity` navigation enum becomes
`ImportMetadataMode.lookup | fileTags | manual`. Avalonia receives the same
three modes. Neither platform derives whether a mode is selected; it compares
the presented mode with `detail.metadata_seed`.

When selection changes, both desktop stores choose the presented mode in this
order:

1. The concrete mode corresponding to `detail.metadata_seed`, when present.
2. A fixed configured default of Lookup, File Tags, or Manual.
3. `last_import_metadata_mode` when the configured default is Last Used.

Only a user tab click publishes `set_last_import_metadata_mode`; programmatic
selection uses the same local navigation state without publishing history.

The mapping pane renders:

- Lookup with no stored results: only `Find this release`.
- Lookup with unresolved matches: inline pressing rows and Find.
- A selected pressing: release card, editor, mapping, and commit controls.
- File Tags before reading: the File Tags action and no release card.
- File Tags while reading: a spinner in that surface without replacing the
  surrounding pane.
- File Tags after reading but before selection: the projected card and
  `Use File Tags`.
- Selected File Tags: editor, mapping, and commit controls.
- Manual before selection: blank projected fields and `Enter Manually`.
- Selected Manual: editable blank fields, physical mapping, and commit controls.

The production previews must cover every state above, including album and track
artist fields with an existing linked artist, a new artist seed, and ambiguous
same-name search results. Extend the existing Lookup/no-search and File
Tags/not-selected previews; do not build preview-only copies of mapping or
artist-resolution logic.

Every new or changed string lands in every macOS and Avalonia locale catalog in
the same commit as its UI.

## Tests that define the behavior

Bug fixes begin with failing tests against production functions.

### Metadata seed

- A scanned candidate with no seed projects a neutral pane and unpicked mapping.
- Opening each metadata mode changes no database row.
- Each affirmative action stores the corresponding `MetadataSeed` and survives
  a candidate-detail reload.
- Only `ExternalRelease` writes `release_identities`.
- File Tags writes `metadata_source = file_tags`; Manual writes
  `metadata_source = manual`; both leave `metadata_source_release_id` NULL.
- Changing seeds clears only seed-derived edits and invalid remote art.
- A Manual release offers no Reset to Source operation.

### File-tag snapshot

- The first File Tags read uses the real reader and stores one complete
  snapshot.
- A second read with unchanged observations opens no tag files.
- A changed generation, file-edit revision, size, or modification time makes
  the snapshot unreadable and causes one complete replacement read.
- One unreadable file leaves no new partial snapshot.
- CUE-backed and standalone candidates project from the stored facts and parsed
  scan rows.
- Pane projection and import preparation consume the same stored facts.
- Import refuses a changed candidate rather than rereading tags behind the
  displayed pane.

### Artist assignment

- Exact MusicBrainz and Discogs IDs resolve an external seed to the existing
  artist.
- Conflicting exact IDs fail rather than choosing either artist.
- A same-name File Tags or Manual seed remains `New` until the user picks a
  library result.
- Choosing an existing artist persists and commits its exact ID.
- Choosing Create inserts a distinct artist even when the name already exists.
- A deleted existing-artist assignment fails with its missing ID.
- Album artist ordering and explicit track-credit ordering round-trip.
- `AlbumArtists` and `Explicit([])` cannot collapse into one state.

### Automatic lookup

- With the setting off, scan and queue sweep dispatch no provider request.
- Interactive Find still performs a provider request while the setting is off.
- Turning the setting off cancels background runs without deleting settled
  results.
- Turning it on schedules unresolved candidates.
- Config and bridge round-trips preserve the value.

### Default metadata mode

- Each fixed default opens an unseeded candidate in its corresponding mode.
- Last Used resolves each concrete remembered mode.
- A stored External Release, File Tags, or Manual seed overrides the configured
  default on candidate selection.
- An explicit mode click updates Last Used even while the configured policy is
  fixed to another mode.
- Opening a candidate, resolving Last Used, and receiving detail updates do not
  update Last Used.
- A detail update for the active candidate preserves its currently presented
  mode unless the user selects another candidate.
- Background identification runs only when the resolved mode is Lookup and
  automatic lookup is enabled.
- Manual and File Tags defaults dispatch no DiscID, OCR, text-signal, provider,
  or matching work.
- Config YAML and bridge round-trips preserve both enums and reject Last Used
  as a concrete remembered mode.

### UI

- Mode navigation does not call seed selection.
- Each affirmative action calls seed selection with the displayed seed.
- File Tags loading remains on its row/surface and exposes failures without
  dismissing or changing modes.
- Artist suggestions store library IDs; Create stores a new-artist seed.
- Import controls appear only for the selected presented seed.
- macOS previews build under the minimal preview host; Avalonia view tests
  render the corresponding states.

## Implementation order

- Add failing core tests for the three seed variants, source provenance, and
  the absence of a candidate seed.
- Add migration 2 tests against a version-1 database, then add the numbered SQL
  migration and register it in the migration ladder.
- Replace the duplicated identity-choice types with `MetadataSeed`, revise the
  relational candidate columns, and add Manual release provenance.
- Add the tag snapshot tables, injected tag reader, atomic load/store path, and
  snapshot-backed File Tags projector.
- Add the Manual projector and make pane/import preparation dispatch on the
  stored seed.
- Replace string artist edits with `ArtistAssignment`, normalized candidate
  assignment rows, deterministic commit resolution, and the artist-search query.
- Update candidate detail and bridge projections so all three navigation
  surfaces have core-owned values.
- Extend production previews, then wire the macOS three-mode section and shared
  artist token field.
- Wire the same bridge values into Avalonia and add its view tests.
- Add the automatic lookup, default-mode, and last-used settings; make the
  sweep respond to absolute lookup-setting changes and make candidate selection
  resolve the configured presentation policy.
- Translate every new string, run focused component tests, then run the normal
  dependency-aware commit verification.

## Verification before declaring the transition complete

- Search the repository for `IdentityPick`, `IdentityChoice`,
  `BridgeIdentityPick`, `pick_kind`, `pick_candidate_identity`, “Unknown
  import,” `album_artist_text`, `artist_text`, and the first-name-match import
  resolver. Every remaining occurrence must describe a genuinely separate
  concept or be removed.
- Verify schema CHECK constraints and all SQLite readers/writers agree on the
  three seed variants and artist-assignment variants.
- Apply the migration ladder to a populated version-1 fixture, verify
  `PRAGMA user_version = 2`, run `PRAGMA foreign_key_check`, and confirm a
  failed migration leaves the original database readable at version 1.
- Verify generated Swift and C# bindings compile through their consumers; do
  not edit generated bindings.
- Run bae-core import, database, queue-sweep, and library identity tests;
  bae-bridge desktop/all-target tests; macOS build and preview environment
  audit; Avalonia tests and view tests; localization gates; Rust formatting and
  clippy for affected crates.
- Confirm File Tags is read once on first use, reused on reopen, and not reread
  by import using an instrumented real candidate.
- Confirm automatic lookup off produces no background provider traffic while
  manual Find still works.
- Confirm all four default-mode settings, all three remembered modes, and all
  automatic-lookup combinations open the intended surface without changing a
  candidate's stored seed.
