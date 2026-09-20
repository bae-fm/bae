# Release and album cross-reference enrichment

## Queue and execution
Execute after `plans/remove-field-origins.md` has been implemented, reviewed, verified, and landed. Use a separate focused branch in the same background worktree. Research the current source, write the detailed implementation steps, implement with regression tests, review against this contract, run normal hooks, and coordinate a fast-forward merge and push with the parent agent.

## User contract
Selecting metadata is a one-time replacement of the draft. It overwrites existing values including manually edited ones. No per-field protection and no continuing provider binding.

The selected release wins wherever it supplies data. A corresponding linked release may fill missing release details. Master and release-group records may fill missing album details and provide associated catalog links. An album association must never be represented as a known pressing association. Do not choose an arbitrary pressing or substitute a master main release.

## Lookup behavior
- Fetch the selected release and its parent: Discogs master or MusicBrainz release group.
- Cross-reference release to release through explicit provider URL relationships: MusicBrainz release follows its Discogs release URL; Discogs release asks MusicBrainz's URL endpoint for release relationships.
- Cross-reference album to album independently: MusicBrainz release group follows its Discogs master URL; Discogs master asks MusicBrainz's URL endpoint for release-group relationships.
- Follow newly discovered relevant metadata relationships, including linked release parents. Fetch each document once and stop when no new relevant documents remain.
- Do not enumerate arbitrary pressings or crawl external websites. Reuse existing catalog-link and Wikidata mechanisms for associated links.
- Define deterministic precedence among supplemental sources grounded in the selected provider's existing behavior. Preserve the distinction between pressing year and original album year.
- Preserve the selected release's track ordering and explicit sides; album metadata does not supply a different pressing's tracklist.

## Representation research
The current `ReleaseRecord` requires a pressing key and group key. Establish a representation for independently known album identity instead of fabricating a pressing ID. Update shared canonical models, persistence, bridge types, and all callers together. Product UI work targets bae-macos; avoid unrelated Avalonia features.

## Required synthetic regression cases
- Selected Discogs release has explicit A/B sides and no MusicBrainz release backlink, but its master has a successful MusicBrainz release-group backlink. The result gains the group and its associated AllMusic/Wikidata links without claiming a MusicBrainz pressing; sides remain unchanged.
- Discogs master and MusicBrainz group disagree on the original year. Verify deterministic selected-provider precedence and that a selected release's supplied values win.
- Corresponding linked release fills absent release details, while album records fill only album fields.
- A manual edit exists before applying another source. The resulting draft is the newly assembled replacement, not a merge protecting that edit.
- Relationships form cycles or repeat documents. Each document is fetched once and traversal terminates.
- Verify both lookup directions and archive replay of the assembled metadata.

## Separate issue
Artwork decoding failures for GIF cover responses were diagnosed separately. They are not part of this queued enrichment task; report any resulting verification blocker independently.

## Implementation design

### Identity and persistence

Retain the canonical `ReleaseRecord` name but replace its pressing-only fields
with explicit pressing and album variants composed from `MetadataRef`.
A pressing carries its release identity, an optional known parent album key,
and whether it supplied the applied metadata. An album-only record carries an
album identity and cannot supply pressing duplicate matches or read the draft.
Core produces each page URL. Keep effective catalog-link precedence deterministic:
selected release, explicitly picked partners, directly linked release documents,
selected-provider parent, other linked album documents, then Wikidata links.
A known pressing outranks an album-only record for the same catalog; this never
licenses inventing a pressing from an album relationship.

Add a new ordered migration for persisted record kind and optional parent key.
Recover old parent identities from archived source documents where available;
`group_key == key` is not proof of parenthood or absence because Discogs release
and master IDs can coincide. Preserve release-to-library-album membership.
Update duplicate-pressing and album-merge queries, metadata provenance, library
artwork dispatch, bridge/automation projections, all canonical callers and
fixtures together. Album catalog links must not create false pressing matches.

### Document traversal and replay

Each request is identified by `(PayloadSource, entity key)`. The release anchor
remains required. Queue related documents and reverse URL requests discovered
from each parsed document; mark a request visited before fetching it, including
missing or failed optional requests. Sort same-kind targets by key for stable
precedence and fetch each distinct document once. A MusicBrainz release follows
release relationships, while a release group follows album relationships; a
release URL filed on a group must not fabricate a pressing correspondence.
Discogs releases enqueue their master and reverse release URL lookup; masters
enqueue reverse release-group URL lookup. Release parents are always followed.
Reuse recognized external catalog links and Wikidata items, without crawling
ordinary linked websites or enumerating group/master pressings.

Preserve existing reverse-release alias documents: a unique URL relationship
fetches the canonical MB document once and archives its bytes under the originating
Discogs release key as well. Add the analogous reverse-master alias containing the
MB group document under the Discogs master key. Reuse already fetched canonical
bytes for aliases. This preserves the existing applied-source snapshot shape.
No-match or multiple distinct targets do not claim a counterpart; log ambiguous
results and never choose the first. Archive replacement atomically clears the directly related document keys before
storing the successful set. This removes obsolete reverse aliases and unavailable
canonical parents, so replay cannot resurrect a previous answer omitted by the
fresh lookup. Descendants behind an absent parent become unreachable; frozen
applied-source snapshots retain their own documents.

The same pure relationship extraction drives online traversal and offline archive
assembly; source-only lookup is replaced with entity-keyed access. Supporting
fetch failures preserve the existing optional-enrichment behavior and are logged
with source/key; the anchor failure still fails selection. Explicit source
application may expand previously archived documents with newly supported related
documents, while ordinary pane reads and frozen applied snapshots stay offline.
No later background repair or hidden provider binding is introduced.

### Metadata projection

Build provider-neutral album and pressing metadata before allocating database
rows, using the existing mapper/assembler boundary. Selected release title and
artist credits win when present; linked releases then selected-provider and
other linked parent documents supply genuinely absent album values. Pressing
fields use selected release then explicitly corresponding linked release only;
masters/groups never supply pressing year, label, country, catalog, or barcode.
Album original year uses the selected provider's parent first, then linked
parents; selected release year is the existing default only when no original
album year exists. This distinguishes original album year from pressing year.
Where the selected release supplies an album-level date directly, it wins.
All unknown years stay absent. Selected tracks, order, positions, sides and
credits remain the selected release's; parent records do not replace tracklists.
Retain cross-provider artist IDs only on the same credited name (ignoring case),
without changing credited names or guessing that differently named artists match.
An optional document with an unusable artist credit logs and skips that credit;
its valid sibling credits and other fields remain available. Selected-release
validation remains unchanged.

Apply the assembled result once through existing replacement operations. No
field-origin data or exception for manual edits returns. Source documents and
source identity remain separate from editable values. Metadata detail, candidate
application, library reset/reapply and archive replay must share projection.
Artwork dispatch uses the actual identity kind (release vs group/master).

### Implementation ownership and sequence

1. Add a failing synthetic regression to `ReleasePayloads::records` demonstrating
   selected Discogs release + master + independently linked MB group losing the
   group's catalog links. Preserve selected A/B positions in projection tests.
2. Provider worker implements independently fetchable typed/raw documents and
   reverse URL responses for both entity kinds, with parser/transport tests.
3. Identity worker implements canonical record distinction, ordered migration,
   DB matching, bridge/automation and callers outside payload assembly; add
   populated upgrade and duplicate-vs-album-match tests.
4. Main background worker implements shared relationship traversal, archive
   replay, record precedence, enrichment projection and artwork dispatch; update
   offline fixtures for explicit negative reverse answers.
5. Review against every requirement above and the required synthetic scenarios.
   Run affected core import/identity/archive tests, bridge/automation tests,
   regenerate native bindings, build macOS, and validate changed platform
   callers. Run normal hooks; commit, push, and coordinate parent review and
   fast-forward integration. Do not combine successor work in this commit.

## Queued successor

Follow the authoritative order in [import-improvements-queue.md](import-improvements-queue.md).
Unexpected release-selection diagnostics follow enrichment on a separate branch
in the same worktree.


## Review and verification evidence

- Synthetic album-backlink regression failed before enrichment and now retains
  selected A/B tracks, selected-provider original year, independent MB group
  identity, and associated catalog links without a fabricated MB pressing.
- Real HTTP traversal tests cover release/group cycles, duplicate relationships,
  ambiguous reverse answers, and a failed canonical fetch reached through two
  aliases. Disabling fetch-attempt tracking reproduced the duplicate request.
- Review found an archived parent could return after a fresh lookup omitted it.
  The store/load regression failed with differing payload sets; replacement now
  invalidates related keys and writes successful documents in one transaction.
- Review found unusable supplemental artist credits rejected a valid selection.
  Both group and linked-release regressions failed first; the three focused
  tests now pass, including unchanged selected-release validation.
- Migration review tests actual schema-41-to-42 reopening with production sync
  declarations. It reproduced a pinned clock-column ordinal mismatch; migration
  42 now appends its new kind column. Both populated-data and real synced-store
  upgrade regressions pass. The sync schema test now inspects migrated SQLite
  instead of approximating migrations with a CREATE/DROP text parser.
- Core import/reset integration: 50 passed. Automation: 31 passed. Bridge: 61
  passed. Subsonic: 16 unit and 17 integration passed with serial execution;
  the initial parallel test process exhausted its file descriptor allowance.
- Native bridge generation and macOS app build passed. macOS catalog/source and
  import-store suites: 20 passed. Regenerated C# callers: all 262 Avalonia view
  tests passed. Mobile/other-platform CI has not been run for this commit yet.
- Final full core suite: 2,217 passed. Normal commit hooks remain required.

## Open migration review finding

Do not commit migration 42 in its present form. Reopening an existing database
passes, but historical sync packages still contain the original nine-column
release-record shape. Coven accepts older package schema versions and applies
their SQLite changesets directly against the current schema: an old insert has
no required kind value, and an old update can restore the self-parent sentinel
into the new optional parent column. Keeping clock ordinals stable is necessary
but does not solve historical replay.

The checked-out Coven API has no versioned changeset transformation hook or
implemented minimum-writer-schema protocol. Snapshot publication alone does not
prevent subsequent older writes or account for unpublished local journals.
Two regressions now use the real schema-41 Coven store, capture its SQLite
Session changesets, migrate through the actual ladder, and apply the original
bytes. INSERT aborts on the new constraint. UPDATE succeeds but stores the old
self-parent sentinel as a known album parent. Both regressions fail as expected.
A further migration issue is that archived provider documents are an unsynced
cache: using them to recover parent identity can give different devices different
migrated values from the same synced input.

Resolve this boundary before landing the persistence change; do not add nullable legacy kinds, arbitrary
pressing defaults, or a fallback decoder to conceal the issue.
