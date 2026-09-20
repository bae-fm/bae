# Release result matching

## Contract and order

Execute in the order in [the serial plan](import-improvements-queue.md), in a
separate focused branch/commit. Match releases before grouping results into album
cards. Album title and artist spelling must not prevent stronger release identity
evidence from being considered. Keep the implementation centered on evidence
already available from the provider records and cross-reference work; do not add
an arbitrary fuzzy score, user-configurable matching rules, or first-result wins.

## Matching policy

- A known direct cross-provider release relationship identifies corresponding
  releases. A master/release-group relationship identifies albums only.
- Without a direct relationship, consider normalized shared identifiers across
  all relevant results, not only those pre-bucketed by exact album text. A shared
  barcode can identify an unambiguous match when known pressing facts do not
  meaningfully conflict. Catalog-number evidence can support matching, but is
  not globally unique; establish corroboration using available release facts.
- Label text is not a mandatory equality gate. Matching names or known label
  identity support a match. Differently written names or unverified abbreviations
  are inconclusive, not proof of different labels and not automatic aliases.
  Missing title/artist/label values do not veto stronger evidence.
- Compare normalized meanings rather than raw strings: catalog case, spaces and
  dash variants; barcode formatting and valid equivalent barcode representations;
  country code versus country name; actual physical medium despite descriptive
  qualifiers. Preserve original values for display and import. Reuse existing
  normalization primitives after reading their contracts. Do not blindly strip
  arbitrary leading zeros or identifier content to manufacture equality.
- Only meaningful known contradictions oppose inferred matches, such as CD
  versus vinyl. Missing data, punctuation, wording, and absent optional qualifiers
  are not contradictions. Define comparison for each available pressing fact in
  the implementation design; do not use equality of a tuple of optional strings.
- Evaluate all plausible counterparts before assigning pairs. If evidence leaves
  multiple plausible releases, keep them separate. Never take the first matching
  code or consume a candidate before a better-supported counterpart is examined.
  A weaker catalog match must not bypass incompatible barcode evidence.
- Build album cards after release matches are established. Use known album links
  for album grouping without merging distinct pressings. Text-based grouping is
  a presentation aid and cannot veto release matches or establish pressing identity.

## Research and implementation

Read complete release_group.rs, its callers and tests, provider result conversion,
and cross-reference/archive types after the preceding enrichment work lands.
Trace all consumers of grouping, including pressing_count, automatic readiness,
manual search, and selection partner claims. Use one canonical matching result so
the count, UI, and claimed partners cannot disagree.

The current pipeline buckets by provider group, merges at most one cross-provider
bucket using its first title and an artist string, then pairs releases only within
that card. It strips barcode non-digits but does not resolve equivalent zero-padded
representations; Discogs search retains only the first listed barcode. Catalog
comparison only trims and lowercases. Pairing takes the first code match, preferring
the same year, and can subsequently pair conflicting barcodes through catalog
equality. Direct provider relationship evidence is absent from this grouping path.

Preserve all available identifiers needed for comparisons rather than discarding
all but the first. Treat a provider's absent group sentinel as absence, never a
shared real group identity. The observed Discogs search record carried master ID
zero; verify the provider decoding/conversion contract and cover it with a synthetic
fixture. Do not expand this task into unrelated provider or import features.

Before source edits, document the concrete evidence rules and ambiguity handling
using the actual types, including multiple barcodes and missing/conflicting values.
Keep album and pressing identities distinct, and do not infer unknown identities
from names. Update affected canonical models and every caller together without
compatibility defaults hiding a changed stored shape.

## Regression requirements

Test the actual result matching function and its selection/count consumers:

- Same barcode, catalog, artist, and year with colon versus dash in the album
  title combines into one pressing with both providers; retained display values
  come from the existing source precedence rules.
- Artist spelling differences or one absent artist do not block established
  identity evidence. Unknown label aliases are neither mandatory failures nor
  fabricated identities.
- Known direct release links combine despite textual variations; group-only
  links combine album presentation without inventing pressing correspondence.
- Catalog formatting, barcode spacing and valid equivalent representations,
  country names/codes, and medium qualifiers compare according to the documented
  semantic rules. Include distinct identifiers that must not normalize together.
- A matching secondary barcode remains usable. A missing barcode differs from
  genuinely incompatible barcode evidence.
- Multiple groups, repeated codes, reissues, and competing matches stay separate
  when ambiguous. Permuting provider/result arrival order does not change pairs.
- CD versus vinyl and other defined meaningful conflicts prevent inferred merges;
  catalog equality cannot silently bypass a stronger contradiction.
- Missing/zero group identity cannot group unrelated releases under one master.
- Grouping, automatic pressing counts, selection partners, persistence/replay,
  and macOS presentation agree on the resulting identities.

Use synthetic fixtures, reproduce confirmed failures before fixing, run affected
tests and normal hooks, and review each contract item before committing/pushing
and coordinating main integration. Do not mutate live media or the live database.

## Existing normalization boundaries

`util/text.rs::squash` already defines canonical catalog-number normalization
for lookup and stored marks: Unicode decomposition, removal of combining marks,
case folding, and retention of letters and digits. Reuse that definition for
catalog evidence rather than keeping `release_group.rs::catalog_key`'s different
trim-and-lowercase rule. Empty normalized identifiers supply no evidence.

`identify/country.rs::named` resolves ISO country codes and their listed English
names to one country. Share this lookup when comparing pressing countries;
do not copy its country table into import matching. Its contract deliberately
does not resolve MusicBrainz's XE/XW region values. Define region comparison
separately where provider records supply that evidence, and keep unrecognized
values inconclusive rather than treating two unknowns as equal countries.

`util/format.rs::physical_medium` is a playback/display classifier: its current
substring matching is case-sensitive, it selects the first of vinyl, cassette,
or CD, and it returns None for both unknown and digital formats. That result
alone cannot establish a pressing's medium or a contradiction. The matching
implementation must preserve actual recognized medium evidence, including
mixed-media descriptions, while ignoring descriptive qualifiers. Reuse existing
physical-medium values where applicable; do not infer digital identity from an
unknown format or equate a mixed-medium release with whichever token matched
first. Cover case variation and mixed/partially described formats in the
production matching tests.

The MusicBrainz conversion also loses medium evidence before matching sees it.
`search.rs::mb_discid_release_to_metadata` puts only the disc-ID-matching
medium's format in `MetadataResult.format`, while its track count and duration
intentionally describe that matching medium. A CD inside a CD-plus-vinyl release
therefore does not establish that the whole pressing is CD-only.
`musicbrainz_mapper.rs::pressing` takes only the first medium's format, so
substituting that helper's format is not a correction: it still discards other
media and may name a medium unrelated to the matched disc ID.

Carry pressing-medium evidence from all media supplied by the provider into
the matching decision. Keep the matching medium's count and duration for disc-ID
readiness; do not change them to the whole release's totals. Preserve incomplete
evidence when any medium's format is absent rather than declaring the remaining
known formats a complete description. Add a regression through the actual
disc-ID response conversion with multiple media, then through the result matcher
and pressing count. Include the matched medium after a different first medium,
and verify that changing media order does not change pressing identity evidence.
Ordinary MusicBrainz search conversion currently supplies no format; absent
format remains unknown, not evidence of a digital release.

`signals/barcode.rs::is_placeholder_code` only recognizes repeated-digit
placeholders; it does not validate check digits or canonicalize UPC/EAN forms.
The present grouping helper strips every non-digit, which can manufacture a
code from unrelated text. Keep validation and equivalent-representation rules
explicit, preserve every provider-supplied barcode for display, and distinguish
missing/unusable evidence from two known incompatible barcode sets. Do not
claim the placeholder helper establishes barcode identity.

## Implementation design

Written after reading `release_group.rs`, its tests, every `group_results` and
`pressing_count` caller, `search.rs` conversions, the Discogs and MusicBrainz
search models, `payloads/relationships.rs`, `util/text.rs::squash`,
`identify/country.rs::named`, `identify/label.rs`, `util/format.rs`, and
`signals/barcode.rs`. It is the contract the implementation and its review are
checked against.

### Evidence a result carries

`MetadataResult` (persisted as `import_candidate_match` rows, read back
unchanged) changes shape; every producer, reader, writer, bridge mirror,
platform caller, automation type and fixture changes with it, in one commit,
through a new ordered migration that rebuilds the table the way `025` did.

- `barcode: Option<String>` becomes `barcodes: Vec<String>`: every barcode the
  source states, in the source's order. MusicBrainz states at most one; a
  Discogs search result lists all of them (`DiscogsSearchResult.barcode`), and
  today only the first survives. Persist them in a child table keyed by the
  match's `(content_hash, position)` with an ordinal, `ON DELETE CASCADE`; the
  migration copies the old column's value as ordinal 0 where present.
- `media: StatedMedia` — what the record says the pressing is made of:
  - `Undescribed`: the response describes no media (MusicBrainz
    `ws/2/release?query=` results; a Discogs search result whose `format` is
    absent).
  - `PerMedium(Vec<Option<String>>)`: one entry per medium the record lists, in
    its order, `None` where that medium's format is not stated. Read from
    `MbReleaseResponse.media` for disc-ID results and for `of_pick` through the
    detail; the matching medium's `format`, count and duration stay as they are.
  - `Descriptors(Vec<String>)`: format names and qualifiers as one flat list
    that does not say which medium each belongs to (`DiscogsSearchResult.format`
    when present, before it is joined into the display `format` string).
  Persist as a `media_kind` column (`'undescribed' | 'per_medium' |
  'descriptors'`) plus a child table of ordinal-ordered entries whose value is
  NULL only for `per_medium`, with CHECKs that tie the two together. The
  migration maps existing rows losslessly: `format IS NULL` → `undescribed`;
  Discogs rows → `descriptors` split on `", "` (the exact inverse of how the
  column was written); MusicBrainz rows → `descriptors` of the one stored
  format string, because a stored disc-ID row kept only its matching medium's
  format and cannot claim a complete media list.
- `links: Vec<MetadataRef>` — releases on other catalogs this record's own
  document names as the same release: a MusicBrainz disc-ID or full release
  response's `url-rels`, parsed through `parse_catalog_url` and kept where they
  name `CatalogPage::Release` on a catalog other than the record's own. A
  response without relations (both search endpoints, every Discogs document)
  names none. `ImportSearchReleaseDetail` gains the same field so `of_pick`
  carries what the full release response stated. Persist in a child table of
  `(content_hash, position, ordinal, catalog, key)`; existing rows have none.

`ImportSearchReleaseDetail.barcode` stays a single display/draft value;
`of_pick` collects it into `barcodes`.

`DiscogsSearchResult.master_id` already decodes zero as `None`
(`optional_master_id`); a synthetic fixture with `"master_id": 0` must show the
converted result carrying `source_group_id: None` and grouping alone.

### Comparing two records

Each pressing fact compares to one of three outcomes: `Same`, `Different`, or
`Unknown` (inconclusive). Never compare tuples of optional strings.

- **Link**: `a.links` names `b` or `b.links` names `a` (catalog and key equal).
- **Barcode**: a stated value is usable only when it is digits with spaces and
  dashes between them (nothing else — a value with letters or other
  punctuation is not a code and is skipped with a `debug!`), at least 8 digits,
  and not `is_placeholder_code`. Its key is its digits, with a 12-digit UPC-A
  written as the 13-digit EAN that prefixes a zero; no other length is
  rewritten, so an 8-digit code and a 13-digit code never meet, and a 13-digit
  code that happens to start with a zero equals the 12-digit code it is the
  EAN form of. `Same` when any usable key on one side equals any on the other;
  `Different` when both sides have at least one usable key and none are equal;
  `Unknown` otherwise.
- **Catalog number**: `squash` both; `Same` when equal and non-empty. A value
  that squashes to `none` states no number (MusicBrainz writes `[none]`,
  Discogs `none`) and is `Unknown`. Never `Different`: the sources punctuate
  and abbreviate these freely.
- **Year**: `Same`, `Different`, or `Unknown` when either is absent.
- **Country**: resolve each through `identify::country::named` (widen it to
  `pub(crate)`); both resolved → `Same`/`Different` by country. Define regions
  beside the matcher: Europe is `XE` or `Europe`, Worldwide is `XW` or
  `Worldwide` (squashed comparison); both regions → `Same`/`Different`. A
  country against a region, or any unresolved value, is `Unknown`.
- **Label**: `identify::label::stated` on both (the trade word dropped);
  `Same` when equal. Otherwise `Unknown` — never `Different`.
- **Medium**: from `StatedMedia`, derive the media the record is known to
  contain and whether that is the complete list. Recognizing a medium in one
  format string is one shared, case-insensitive function in `util/format.rs`
  that returns every recognized `PhysicalMedium` in the string (vinyl,
  cassette, CD by the existing substrings), which `detect_format` then reads
  the first of — one recognizer, not two. `PerMedium`: known = recognized media
  of stated entries; complete = every entry stated and recognized.
  `Descriptors`: known = recognized media among the tokens; never complete.
  `Undescribed`: nothing known. `Different` when either side is complete and
  the other side knows a medium absent from it. `Same` when both are complete
  and equal. `Unknown` otherwise.

Identity evidence makes two records candidates for one pressing: a link; a
`Same` barcode; or a `Same` catalog number corroborated by at least one `Same`
among year, country, label, medium. A `Different` barcode, year, country, or
medium is a contradiction and removes an inferred candidate — a barcode or
catalog candidate. A linked pair is stated, not inferred, and stands.

Support orders candidates as a tuple compared lexicographically:
`(link, barcode Same, catalog Same, number of Same among year, country, label,
medium)`.

### Pairing

Pair the MusicBrainz records against the Discogs records over the whole
result list (the two members of `Catalog::LOOKUP`; any other source is a
programming error), not within an album card. Take support levels from the
highest down: at each level, the candidate pairs whose two members are both
still free are examined together; a member that appears in more than one of
them is ambiguous and is settled unpaired; every remaining pair at the level is
taken. A member whose only pair at this level named an ambiguous member stays
free for lower levels. Nothing depends on arrival order: permuting either side
gives the same pairs.

### Cards

Bucket by `(source, source_group_id)` as now. Then union the buckets joined by
a pair; that is the album grouping known links establish. Then the text merge
as now, over the resulting cards: a card carrying only one source merges with
the first later card carrying only the other source whose album key is equal.
A card may hold more than one bucket of one source when pairs join them; its
`sources` lists each bucket by `source_rank` then first-seen order, its `id`
is the first of those buckets' group ids or the lead's release id, and its
title, artist, label and cover are read from releases in that bucket order.
Rows: each pair is one `Pressing::of([a, b])`, every other release its own
row; ordering stays as now.

### Tests to revise

`a_formatting_difference_in_the_catalog_number_does_not_pair` inverts:
`CAT 2 2` and `CAT 2-2` in the same year are one pressing.
`two_undated_records_pair_by_position_alone` inverts: two Discogs records with
one barcode and nothing to tell them apart are ambiguous, and all three rows
stay separate. `an_absent_artist_matches_only_an_absent_artist` and
`different_titles_across_sources_stay_apart` keep their text-merge meaning only
where no pair joins the cards; add the paired variants the regression list
names. Every remaining regression requirement above gets a production-path
test through `group_results`, `pressing_count`, the disc-ID conversion, and the
stored-verdict read path.
