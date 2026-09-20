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

## Queued successor
After enrichment lands, execute [Release selection error details](release-selection-error-details.md) as the third task, on its own branch in this same worktree. The order is field-origin removal, release/album enrichment, then visible and copyable selection-error details.
