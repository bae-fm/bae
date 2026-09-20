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
