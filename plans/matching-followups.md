# Matching follow-ups: medium vocabulary, pick-wide covers, stored draft cover, stored rows

Four items, done serially on the main checkout at `/Users/dima/dev/bae`,
**uncommitted**. The person tests each result from Xcode and iterates; nothing
is committed or pushed by the implementer. The main checkout already carries
uncommitted edits in `bae-core/src/import/release_group.rs`,
`release_group_tests.rs` and `bae-core/src/identify/combine.rs` — the pressing
rollup (records that all support each other become one row; one claimed record
per catalog). Those edits are part of this work and stay; build on them.

The design this extends is `plans/release-result-matching.md`, section
"Implementation design". Read it first. Where this file and that one disagree,
this file wins, and the implementer updates that section to match at the end
of each item.

Repo rules that apply throughout: source files ≤1,000 lines
(`scripts/check-source-file-size.py`), Rust files over 1,000 lines keep tests in
a sibling `_tests.rs`; no `git add -A`; no `CARGO_TARGET_DIR` or `RUSTC_WRAPPER`
per command; no new worktrees; schema changes are a new ordered migration, never
a rewrite of one; any new user-visible string is translated into every locale
catalog in the same change; no real artist or album names in fixtures. Disk is
tight (~12 GB free): do not create build directories beyond what
`bae-bridge/build-macos.sh` already uses in this checkout, and do not run the
Avalonia or iOS/Android builds.

Verification per item: the bae-core unit tests for the modules touched, plus
any macOS test suite whose subject changed (run by suite, not the whole target).
Run bae-core tests with `DYLD_LIBRARY_PATH=$PWD/bae-ffmpeg/dist/lib`. After the
last item, rebuild the macOS bridge with `./bae-bridge/build-macos.sh` so the
person can build the app from Xcode and test. Then report: what changed per
item, what was verified, what was not.

---

## 1. Medium vocabulary

### Why

`pressing_evidence::KnownMedia` reads media through
`util::format::recognized_media`, a substring matcher for "vinyl", "cassette"
and "cd" over free text. Anything else — a download, a DVD, a Blu-ray — is
unknown, so a Discogs "File, FLAC" release can never contradict a MusicBrainz
"CD" and, sharing catalog number, year and country, rolls into the CD's row.
Observed on a real candidate: a 2xHD FLAC reissue joined the CD pressing.

Both catalogs state media from closed vocabularies. The complete lists are in
`plans/medium-vocabulary/`:

- `musicbrainz-format-names.txt` — every `medium_format` row (115 names), from
  https://musicbrainz.org/statistics/formats, which is generated from the table.
- `discogs-format-names.txt` — the 66 format names from the official formats
  list page (Wayback capture of https://www.discogs.com/help/formatslist,
  2026-01-03).
- `discogs-format-descriptions.txt` — the 162 distinct descriptions from the
  same page.

Confirmed against the live Discogs API: a search result's `format` array is the
format name followed by its descriptions (`["Vinyl","LP","Album"]`); the
free-text field (`"text":"Red label"`) is separate and never in that array. The
release document as our client reads it keeps only format names
(`discogs/client.rs`, the `formats[].name` projection).

### What

A new module, `bae-core/src/import/medium.rs` (with `medium_tests.rs` if it
passes 1,000 lines), holding:

- `pub(crate) enum Medium` — carrier classes. One variant per carrier family
  a person would call a different object. Required set, with the names that map
  to each (MusicBrainz names left, Discogs names right; every listed name from
  both files must appear exactly once in the module):
  - `Cd`: CD, CD-R, 8cm CD, 8cm CD-R, Enhanced CD, HDCD, SHM-CD, Blu-spec CD,
    HQCD, Copy Control CD, Data CD, DTS CD, Mixed Mode CD, Minimax CD, CD+G,
    8cm CD+G, CD-i | CD, CDr
  - `Sacd`: SACD and every "Hybrid SACD…", "SHM-SACD…", "SACD (…)" variant |
    SACD
  - `Dvd`: DVD, DVD-Audio, DVD-Video, DVD-R Video, Data DVD, Data DVD-R,
    MiniDVD, MiniDVD-Audio, MiniDVD-Video, Minimax DVD, Minimax DVD-Audio,
    Minimax DVD-Video | DVD, DVDr
  - `HdDvd`: HD-DVD | HD DVD, HD DVD-R
  - `BluRay`: Blu-ray, Blu-ray-R, Ultra HD Blu-ray | Blu-ray, Blu-ray-R,
    Ultra HD Blu-ray
  - `Vinyl`: Vinyl, 7" Vinyl, 10" Vinyl, 12" Vinyl, 3" Vinyl, Flexi-disc,
    7" Flexi-disc | Vinyl, Flexi-disc, Lathe Cut
  - `Shellac`: Shellac, 7"/10"/12" Shellac | Shellac
  - `Acetate`: Acetate, 7"/10"/12" Acetate | Acetate
  - `Cassette`: Cassette, Microcassette | Cassette, Microcassette, Elcaset,
    NT Cassette, DC-International
  - `Dcc`: DCC | DCC
  - `Dat`: DAT | DAT
  - `Cartridge` (tape cartridges): Cartridge, 8-Track Cartridge, PlayTape,
    HiPac | 4-Track Cartridge, 8-Track Cartridge, PlayTape, Pocket Rocker,
    RCA Tape Cartridge, Revere Magnetic Stereo Tape Ca, Sabamobil
  - `ReelToReel`: Reel-to-reel | Reel-To-Reel
  - `Digital`: Digital Media, Download Card | File
  - `MiniDisc`: MiniDisc | Minidisc
  - `LaserDisc`: LaserDisc, 8" LaserDisc, 12" LaserDisc | Laserdisc
  - `Cdv`: CDV | CDV
  - `VideoCd`: VCD, SVCD | (none)
  - `VideoTape`: VHS, Betamax, Betacam SP | VHS, Betamax, Betacam, Betacam SP,
    Beta ED, Super Beta, Super VHS, U-matic, Video 2000, Video8, MiniDV,
    Cartrivision
  - `FlashMemory`: USB Flash Drive, SD Card, microSD, slotMusic, Playbutton |
    Memory Stick, HitClips
  - `Floppy`: Floppy Disk, 3.5" Floppy Disk, 5.25" Floppy Disk, Zip Disk |
    Floppy Disk, Zip Disk
  - `Cylinder`: Wax Cylinder | Cylinder
  - `DualDisc`: DualDisc and its "(… side)" variants | (none)
  - `VinylDisc`: VinylDisc and its "(… side)" variants | (none)
  - `DvdPlus`: DVDplus and its "(… side)" variants | (none)
  - one variant each for: Piano Roll, Edison Diamond Disc | Edison Disc,
    Pathé disc | Pathé Disc, Tefifon | Tefifon, UMD | UMD, VHD | VHD,
    CED | SelectaVision, Wire Recording, Film Reel, MVD, TeD, Mighty Tiny,
    Sopic, KiT Album, ROM cartridge, DataPlay.
  - **Names no carrier** (in the table, mapped to no class): MusicBrainz
    "Other", "Phonograph record"; Discogs "Hybrid", "All Media", "Box Set".
  If a name in the files is not covered above, give it the class the
  implementer judges by the same rule (carrier family a person would call a
  different object) and say so in the report.
- `Medium::musicbrainz(name: &str) -> Lookup` and `Medium::discogs(token:
  &str) -> Lookup`, exact whole-string match, case-insensitive, over the
  catalog's own table. `Lookup` is `Carrier(Medium) | NamesNoCarrier |
  Description | Unrecognized` (`Description` only from Discogs).
- The tables are the vocabulary: literal `&[(&str, …)]` slices in the module,
  each preceded by a comment naming the source URL and capture date as in the
  `.txt` files. A test asserts each table's names equal the corresponding
  `.txt` file's contents exactly (read the files at test time relative to the
  crate, `plans/medium-vocabulary/...` is tracked), so the code and the record
  cannot drift.

`pressing_evidence::KnownMedia` reads media through `Medium`, with the record's
source deciding the table (`PressingFacts::of` has `release.source`; pass it
to `KnownMedia::of`):

- `PerMedium` (MusicBrainz): known = carriers of stated entries; complete iff
  every entry is stated and is a carrier.
- `Descriptors` (Discogs): known = carriers among the tokens; complete iff at
  least one token is a carrier. Descriptions and no-carrier names are ignored.
  An `Unrecognized` token is ignored too — format names are physical carriers
  and that list barely moves, while descriptions are added regularly, so an
  unknown token is almost always a description — but it is logged at `warn`
  with the source, release id and the token, so the vocabulary gets fixed. The
  same `warn` for a MusicBrainz format name the table lacks (which does make
  that record incomplete, since a MusicBrainz entry names a medium).
- `Undescribed`: nothing known, incomplete.
- Comparison unchanged: `Different` when either side is complete and lacks a
  carrier the other knows; `Same` when both complete and equal; else `Unknown`.

`recognized_media` and `PhysicalMedium` in `util/format.rs` stay for playback's
free-text classification; the matcher no longer uses them. Fix the doc comments
that claim one shared recognizer (`util/format.rs`, `import/search.rs` on
`StatedMedia`, the design doc's Medium bullet).

Tests in `pressing_evidence_tests.rs`: the seven worked examples below, plus
one for case-insensitivity and one for each `Lookup` outcome.

- MB `[CD]` vs Discogs `[CD, Album]` → Same.
- MB `[CD]` vs Discogs `[File, FLAC, Album, Reissue]` → Different.
- MB `[CD, DVD-Video]` vs Discogs `[CD, Album, DVD, DVD-Video, NTSC]` → Same.
- MB `[CD, DVD-Video]` vs Discogs `[CD, Album]` → Different.
- MB `[Hybrid SACD]` vs Discogs `[SACD, Hybrid, Multichannel]` → Same.
- MB `[CD]` vs Discogs `[CD, Album, Zorblax]` → Same, and "Zorblax" is
  warn-logged.
- MB `[CD, Zorblax Disc]` vs Discogs `[CD, Album]` → Unknown (MB incomplete),
  and "Zorblax Disc" is warn-logged.
- MB `[CD, (unstated)]` vs Discogs `[Cassette]` → Different: Discogs lists
  every format the release has, so a cassette-only record is not a record
  with a CD in it, whatever MB's unstated medium is.
- MB `[CD, (unstated)]` vs Discogs `[CD, Album]` → Unknown: MB is incomplete
  and Discogs lacks nothing MB knows.

Then a `release_group_tests.rs` case: a CD record and a same-catalog-number
File record, same year and country, are not one pressing.

---

## 2. Cover options belong to the pick

### Why

`ReleasePayloads::covers()` lists one release's own images and the releases
its document cross-links editorially. A pick claims more than that: a primary
plus partners (`AppliedSource.partners`, `MetadataProvenance::ExternalRelease
{ partners }`). On a real candidate the MusicBrainz primary had no archive art
and linked only an Amazon page, the Discogs partner had images, and the
draft's default was the release-group archive URL, which 404s. The picker
(`handle/search.rs::candidate_covers`) already walks primary plus partners; the
default reads a different list.

### What

One function produces a pick's cover options from the claimed documents, and
every reader of "which covers does this pick offer" calls it:

- `pick_covers(primary: &ReleasePayloads, partners: &[ReleasePayloads]) ->
  Result<Vec<RemoteCover>, ImportError>` (name and home at the implementer's
  judgement; `import/cover_art.rs` or `import/payloads.rs`). Order: each
  claimed release document's own images, primary first, then partners in claim
  order; then each claimed album document's images (Discogs master images, the
  MusicBrainz release-group archive URL) in the same order. Each document is
  read once even when reachable twice (the editorial cross-link and a partner
  naming the same Discogs release; two partners under one master). Keep
  `push_unique_cover` for URL uniqueness.
- `default_cover` is the first of that list. Callers: `handle/scan.rs`
  (interactive pick), `sweep/settle.rs`, `db/client/import_list/window.rs`,
  `triage/model.rs`, `search.rs:90`, `payloads.rs::detail_for_audio`. Each must
  have the partners in hand; where one does not today, thread them through
  rather than calling with an empty slice — a pick with partners and a call
  that ignores them is the bug this fixes.
- `candidate_covers` in `handle/search.rs` calls the same function.
- `ReleasePayloads::covers()` and `default_cover()` are removed or reduced to
  the single-document helpers the pick-wide function is built from; no caller
  builds its own list.

Tests: a primary with no art and a partner with images yields the partner's
image first; the release-group URL follows a partner's real image; a Discogs
release reachable as both cross-link and partner contributes its images once.

---

## 3. The draft's cover is a stored value

### Why

The draft's cover selection (`import_candidate_cover`) is empty after a scan;
the pane and the commit each re-derive "embedded art, then the folder's
image" at read time (`service/importing.rs`, the `cover_candidate` match near
line 715; `CoverChoice`'s "or the fallback that import will use" doc). On pick,
`external_candidate_metadata` takes a `fallback_cover` parameter: the
interactive path passes the current selection, the sweep (`sweep/settle.rs`)
passes `None`. So an automatic identification whose remote default failed left
the candidate with no cover although the folder holds `cover.jpg`.

The person's rule: the draft is filled imperatively and read simply. The same
routines produce the value wherever it is produced; what differs between
callers is only whether the answer replaces what is stored.

### What

- One function chooses a folder's own cover: embedded art from the File Tags
  snapshot first, then the folder's deterministic image default (the rule
  `pick_folder_cover` and the commit's chain apply today). It returns a
  `CoverSelection` (`Embedded` or `Local`) or none.
- **Scan** calls it and stores the result as the candidate's cover selection
  when the candidate is created or re-scanned and no selection is stored.
- **Identification** (interactive pick and sweep) stores
  `CoverSelection::Remote` only when `pick_covers` gave a default and the image
  was fetched. Otherwise it does not write the cover at all. The
  `fallback_cover` parameter and `local_or_embedded_cover` go away.
- **The person's choice** through the picker stores directly (as now).
- **Pane and commit** read the stored selection. `importing.rs` commits the
  stored selection's bytes and nothing else: `Remote` → the prepared bytes,
  `Local` → that file, `Embedded` → the snapshot's art, none → no cover. The
  read-time chain is deleted. `CoverChoice`'s doc says the selection is the
  stored value.
- Existing candidates whose stored selection is empty but whose folder has
  art: a Rust migration (next number after 044) that runs the folder-cover
  function over each candidate with no stored cover and stores what it finds,
  so no candidate silently loses the art it showed. If the snapshot needed for
  embedded art is not available at migration time, store the folder image
  where one exists and say so in the report.

Tests: scan stores the folder cover; an identification that finds no image
leaves a stored `Local` selection in place (both paths); one that fetches an
image replaces it; commit with an empty selection writes no cover and does not
re-derive one; the migration fills empty selections.

---

## 4. The identification run's rows are stored

### Why

`combine_results` pairs every answer, then splits the rows into offered and
set-aside; the verdict is stored flat (`import_candidate_match`, one row per
release, `narrowed_out` flag), and every reader regroups each list on its own
(`identify/view.rs`, `identify/ready.rs`, `db/client/import_list.rs`,
`sweep/settle.rs`, `pressing_count`). With rollup, regrouping a sublist can
build different rows than the run did: a record the run settled as ambiguous
because of a record in the other list rolls up when grouped without it. The
run decided the rows; readers should read them, not recompute them.

### What

- Persist row membership: a `pressing` column on `import_candidate_match`
  (the row's index within its list, `narrowed_out` lists numbered separately
  or jointly — implementer's call, documented), and the record order within a
  row is the stored `position` order, lead first. New ordered migration
  (the number after item 3's), rebuilding the STRICT table and its three child
  tables as 044 did. Existing verdicts get their `pressing` by running the
  current grouping over each stored list — the one place regrouping is still
  right, since nothing else records what the old run built. Check whether the
  ledger (`ledger_json`, `TerminalVerdict`) already serializes `Pressing`s and
  keep the two consistent.
- `release_group` gains a function that builds cards from already-formed rows
  (bucket by source group, join buckets a row spans, text-merge single-source
  cards, order) — `group_results` becomes "form rows, then that". Readers of a
  stored verdict call the from-rows function with the stored rows;
  `pressing_count` on a stored verdict is the number of stored rows.
- `combine.rs`'s doc claim "re-grouping either list rebuilds the same rows"
  is replaced by the stored-rows statement, and the test
  `the_discogs_record_of_the_pressing_the_disc_id_named_is_offered_with_it`
  reads rows from the outcome rather than regrouping the set-aside list.

Tests: a stored verdict whose set-aside list would roll up on regrouping is
read back with the rows the run built; migration assigns rows to existing
verdicts; `pressing_count` equals the stored row count.

---

## Record

Append an execution record at the end of this file per item: what was done,
what was verified and how, what was not verified, and any judgement call the
spec left open.

---

## Execution record

### 1. Medium vocabulary

**Done.** `bae-core/src/import/medium.rs` holds `Medium` (41 carrier
families), `Lookup` (`Carrier` / `NamesNoCarrier` / `Description` /
`Unrecognized`), `Medium::musicbrainz`, `Medium::discogs`, and the three
tables — MusicBrainz's 115 format names, Discogs's 66 format names, Discogs's
162 descriptions — each in its page's order with the source URL and capture
date above it. A name matches whole and case-insensitively, compared
character by character in lower case so the accented names compare like the
rest. Every name of both lists is classed as the spec assigned it; none
needed the implementer's judgement.

`pressing_evidence::KnownMedia::of` now takes the record's source and release
id and reads each word in that catalog's own table. A word outside both
Discogs lists is passed over and `warn`-logged; a MusicBrainz name the table
lacks is `warn`-logged and leaves that medium unknown. `util/format.rs` keeps
its free-text classifier for playback; the doc comments there, on
`StatedMedia` and in the design's Medium bullet say what each is for.

**Judgement calls.**

- *`KnownMedia` carries one `complete` flag*, and the comparison is the
  original: `Different` when either side is complete and lacks a carrier the
  other knows, `Same` when both are complete and equal, `Unknown` otherwise.
  A MusicBrainz record is complete when it has entries and every one is
  stated and names a carrier; a Discogs record is complete when its array
  holds at least one carrier, because the array lists the release's formats.
  (This first went in as a three-state `Coverage`, to hold both the rule and
  the spec's then-stated "MB `[CD, (unstated)]` vs Discogs `[Cassette]` →
  Unknown". That example was corrected to `Different`, and the three states
  went with it.)
- *`recognized_media` is gone rather than kept.* Its `Vec` existed for the
  matcher; with the matcher reading the vocabulary, its only reader took the
  first entry, so it folded back into `util/format::physical_medium`, which
  is what playback and the library already call.
- *The vocabulary files are checked in under `bae-core/test-fixtures/`.*
  `plans/` is gitignored, so a test reading `plans/medium-vocabulary/` would
  pass here and fail in a fresh clone. The three captures are copied to
  `bae-core/test-fixtures/medium-vocabulary/` and the drift test
  `include_str!`s them.
- *An empty `PerMedium` list is `Partial`.* "Every entry stated and a
  carrier" is vacuously true of no entries, which would have made a record
  listing no media contradict everything.

**Verified.** `cargo test -p bae-core --lib`:
`import::medium` (3: the table-against-record drift test, the four `Lookup`
outcomes, whole-and-any-case matching) and `import::pressing_evidence` (10,
including the nine worked examples across three tests, the case test, and
the two `warn` assertions through `test_logs::capture_warn_logs`).
`import::release_group` (87) and `import::search::search_tests` (22) pass with
the new rules; the new
`a_file_release_is_not_the_cd_whose_catalog_number_it_carries` covers the
report's own case.

**Changed by the new rules.**
`search_tests::discid_metadata_carries_every_medium_into_pairing` asserted
that a Discogs record naming only the vinyl of a CD-plus-vinyl MusicBrainz
release is that object; under the vocabulary it is not (this is spec example
4's shape), so its second half now uses a Discogs record naming both media.

### 2. Cover options belong to the pick

**Done.** `payloads::pick_covers(primary, partners)` is the one list: every
claimed release document's own images, the primary's first, then the albums'
in the same order, each image once. `ReleasePayloads::covers` is private and
built from the two halves `pick_covers` composes (`release_covers`,
`album_covers`); `default_cover` is gone. `cover_art::musicbrainz_covers`
split into `musicbrainz_release_cover` and `musicbrainz_album_cover`, because
the release-group address is an album option and was sitting among the
pressing's.

`detail_for_audio` takes the pick's partners and fills `cover_art` from
`pick_covers`, so every reader of `ImportSearchReleaseDetail::default_cover`
(`search.rs::of_pick`, `triage::MatchedRelease::of_pick`,
`window.rs::chosen_cover`) is pick-wide without knowing it. Threaded through:
`handle/scan.rs` (both the pick's detail and `external_candidate_metadata`),
`db/client/import_list/window.rs` (both call sites, from the claimed
payloads it already reads). `handle/search.rs::candidate_covers` now calls
`payloads::pick_gallery_covers`, which is the same walk with the archive's
galleries asked for.

**Judgement calls.**

- *Two pick-wide functions, not one.* The picker's gallery fetches the
  archive's complete galleries; the draft's default must not. They share the
  claimed-documents walk and differ in what each set contributes.
- *"Each document is read once" is enforced as "each image is offered once"*
  — `push_unique_cover` by URL, as the spec says to keep. A document reachable
  twice is parsed twice; only the list is deduplicated.

**Verified.** `import::payloads` (90 tests including the two new ones: a
primary with no art offers its partner's image first and the release-group
address after it; a Discogs release reachable as both cross-link and partner
contributes its images once). `import::handle`, `import::service`,
`db::client` and `import::sweep` pass.

**Not verified.** No test asserts the pick-wide gallery ordering of
`pick_gallery_covers` beyond the existing `candidate_covers` tests.

### 3. The draft's cover is a stored value

**Done.** `local_artwork::folder_cover(embedded, artwork)` is the one rule:
the tags' embedded art, else the folder's deterministic image default.

- **Scan** — `folder_scans::write::ensure_candidate_state` stores it whenever
  the candidate has no cover row, for the blank seed and the File Tags seed
  alike. `insert_file_tags_draft` no longer writes the cover.
- **Reset** — `handle/reset.rs` fills the cover the same way, so a reset
  starts the candidate where a scan would.
- **Identification** — `external_candidate_metadata` supplies
  `CoverSelection::Remote` only when `pick_covers` gave a default and the
  image was fetched, and no cover otherwise; the `fallback_cover` parameter
  is gone.
- **The write** — `preparations::settled_cover` decides what a metadata
  application leaves stored: the image the source supplied, or, where it
  supplied none, the cover the candidate's folder gave it. Both writers —
  `apply_metadata` (pick, file tags, source application) and `store_verdict`
  (the sweep) — go through it.
- **Commit** — `service/importing.rs` commits the stored selection and
  nothing else: `Remote` the prepared bytes, `Local` that file, `Embedded`
  the snapshot's art, none no cover. The read-time chain is gone, and
  `pick_folder_cover` takes the selected path rather than an `Option`.
- **Pane and rows** — `window.rs::chosen_cover` reads the stored selection
  (a `Local` selection naming a file the folder no longer holds is
  `warn`-logged and shows nothing); `RowCover` collapsed to the stored
  selection, so a row shows what the candidate would commit with.
- **Migration 045** (`candidate_folder_covers`) runs `folder_cover` over
  every candidate with no stored cover, reading the embedded cover from the
  stored File Tags snapshot and the artwork from the stored candidate files.
  Both were available, so nothing had to fall back to the folder image alone.

**Judgement calls.**

- *`local_or_embedded_cover` is gone, and the stored cover is left alone
  whatever it is.* An application that supplies no image says nothing about
  the cover, so `preparations::settled_cover` carries the stored selection
  forward — a remote one from an earlier pick included — together with the
  bytes prepared for it, which is what keeps
  `prepared_asset_rows::validate_remote_cover` satisfied. The pair is one
  value (`PreparedCover`) for exactly that reason. `assets_prepared` follows
  the settled pair rather than being set to `true`: a remote cover chosen
  from the picker's gallery is stored without bytes and leaves the candidate
  waiting for a source to be applied again, and an application that carries
  that selection forward carries the waiting with it.
  (This first went in as a downgrade — the stored `Remote` dropped — which
  was corrected.)
- *Applying File Tags now keeps the stored cover* when the tags embed no
  art, where it used to clear it. That falls out of the rule above and is
  what makes the pane show `cover.jpg` for a File Tags candidate now that
  nothing re-derives one.
- *A row no longer borrows the matched release's thumbnail.* A verdict that
  found a cover but never fetched it is not what the candidate would commit
  with, so the row shows the stored selection or nothing.
- *`cover_art::serve_empty_archive_for_test`* — test builds point the archive
  at a dead port, where a fetch is a connection failure rather than an
  answer. Two pick tests now offer a partner's album address, which the
  archive answers 404 for in life; the stand-in answers 404 for everything.

**Verified.** `import::handle` and `import::service` (190),
`db::client` (181), `import::sweep` (272), `migrations::tests` (60).
New: `import::preparations::tests::a_source_with_no_image_keeps_the_stored_cover_and_its_bytes`,
`db::client::tests::import_list_tests::the_scan_stores_the_folders_own_cover`,
`import::sweep::tests::a_settled_run_with_no_artwork_keeps_the_folders_own_cover`
(the sweep path), the cover assertion in
`import::handle::tests::pick_partners::a_pick_with_a_partner_stores_it_and_archives_its_documents`
(the interactive path), and
`migrations::tests::candidate_folder_covers::empty_cover_selections_are_filled_from_the_folder`.
Rewritten for the stored value: the reset tests, `the_list_projects_the_applied_draft_and_cover`,
and `metadata_apply_and_clear_preserve_every_physical_decision`, whose last
clause now reads that a draft bringing no image leaves the remote cover the
person chose standing.

**Not verified.** "Commit with an empty selection writes no cover and does
not re-derive one" has no test of its own: after this item the state is
unreachable by any path — a scan fills the selection whenever the folder has
art, and the migration fills the ones already stored — so the nearest real
tests are `retained_unsupported_embedded_cover_is_only_used_when_explicitly_selected`
(the snapshot's art is committed only when selected) and the migration test.
The macOS app's own cover surfaces were not run.

### 4. The identification run's rows are stored

**Done.** A run's rows are recorded and read, never re-formed.

- `import_candidate_match` gains a `pressing` column — the row's index
  **within its own list**, numbered from zero in row order, the matches
  numbering their rows and the narrowed-out releases numbering theirs. The
  records of one row are read in stored `position` order, the lead first.
- **Migration 046** (`match_pressings`) rebuilds the STRICT table and its
  three child tables as 044 did, filling `pressing` from a temp table the
  Rust side writes by running the current grouping over each stored list —
  the one place re-forming is still right. The migration fails loudly if the
  row count changes across the rebuild.
- `CombineOutcome::Found`, `NarrowedOut`, `IdentifyState::Found`/`Failed` and
  `TerminalVerdict::Found` carry `pressings` (and `narrowed_out_pressings`)
  index-aligned with their release lists; `combine_results` numbers them as
  it walks the rows it built.
- `release_group::group_formed_rows(results, rows)` builds the cards over
  rows already formed; `group_results` is now "form the rows, then that"
  (`cards`). `row_count(rows)` is what the Ready rule
  (`VerdictSummary::of`), the queue (`import_list::state_rows`) and the pane
  read; `form_rows(results)` is the only thing that forms rows over a list of
  its own, used by the migration and by test fixtures standing in for a run.
  `pressing_count` is now `#[cfg(test)]`: nothing in production forms rows
  over a stored list any more.
- Readers moved to the stored rows: `identify::view::fold_matches` (and so
  the pane and the narrowed-out disclosure), `sweep::settle::sole_pressing`,
  `db::client::import_list`.
- `combine.rs`'s "re-grouping either list rebuilds the same rows" claims are
  replaced by the stored-rows statement, in the module doc, on `NarrowedOut`
  and on `combine_results`.

**The ledger** already stores each lookup cell's cards whole
(`view::found_or_no_match` groups a lookup's own complete answer at run time
and the ledger is serialized as it stands), so nothing re-groups there
either; the two are consistent, and no ledger change was needed.

**Judgement calls.**

- *Rows are numbered per list*, which is what "the row's index within its
  list" says; the stored `position` sequence still runs across both lists as
  before.
- *`TerminalVerdict::Found` carries a fourth index-aligned list* rather than
  holding `Pressing`s. The stored shape is flat rows plus an ordinal, the
  lead is still `matches[0]`, and every reader that wants rows asks
  `group_formed_rows` for cards.
- *`release_group_tests.rs` and `combine.rs` were split* to satisfy
  `scripts/check-source-file-size.py`: `combine.rs`'s inline test module
  moved to `combine_tests.rs` (the file was 1,016 lines), and
  `release_group_tests.rs`'s ranking half moved to
  `release_group/ranking_tests.rs` (it was already over the limit before this
  work).

**Verified.** `identify::combine` (33), `identify::ready` (23),
`identify::view`, `import::release_group` (87), `import::sweep` (272),
`db::client` (181), `migrations::tests` (60). The spec's three tests:
`the_discogs_record_of_the_pressing_the_disc_id_named_is_offered_with_it` now
reads the rows off the outcome and asserts both that the set-aside list reads
back as the three rows the run built and that forming rows over that list
alone rolls all three into one;
`migrations::tests::match_pressings::stored_matches_are_given_the_row_they_belong_to`;
`identify::ready::tests::the_pressing_count_is_the_rows_the_run_recorded`.

**Not verified.** No test drives a whole run through the sweep and then
reads the rows back out of the database — the migration test and the combine
test cover the two halves of that path separately.

### Across all four

`cargo fmt -p bae-core` after each item; `cargo clippy -p bae-core --lib
--all-targets` clean; `cargo check --workspace` clean (bae-bridge,
bae-automation and the rest compile against the changed types unchanged — no
bridge type changed shape, so no Swift or C# caller needed editing).
`scripts/check-source-file-size.py` clean. `./bae-bridge/build-macos.sh` run
at the end so the app builds from Xcode.

**Not verified across all four:** no macOS test suite was run (no bridge type
changed, and the macOS side reads the same shapes it did), and the iOS,
Android and Avalonia builds were not run.

**The bridge is one build behind.** `./bae-bridge/build-macos.sh` ran after
the four items; the two corrections that followed — the media comparison and
the cover the write settles on — changed `bae-core` after it, and the
rebuild was deliberately not run. Run it before building from Xcode, or the
app carries the core as it stood before them.

**One test was order-dependent before this work and is not now.**
`reset_setup_without_tags_keeps_a_combination_snapshot_ineligible_until_reread`
named `01 - Volume A/cover.jpg` as the folder cover of a two-folder
combination; which folder the combination lists first is not fixed, so the
assertion now names whichever it is and asserts the image rather than the
volume.

**Surprise worth naming:** the combine tests use real artist and album names
(`Van Halen II`, `AC-DC - Dirty Deeds Done Dirt Cheap`), which
`no-real-artist-album-song-names-in-artifacts` forbids. They predate this
work and were left alone.
