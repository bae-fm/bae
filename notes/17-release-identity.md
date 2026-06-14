# Release identity

How releases are identified across metadata sources, and why the unit of
identity is the release rather than the album.

## Model

A release's identity is a *set* of rows in a side table
(`release_identities`), one per source the user has matched:

```sql
CREATE TABLE release_identities (
    release_id        TEXT NOT NULL,
    source            TEXT NOT NULL,    -- 'musicbrainz' | 'discogs'
    source_group_id   TEXT NOT NULL,
    source_release_id TEXT,             -- NULL → Approximate, present → Exact
    _updated_at       TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    PRIMARY KEY (release_id, source),
    FOREIGN KEY (release_id) REFERENCES releases (id) ON DELETE CASCADE
);
```

A release can hold:

- **0 rows** — Unknown. No identity claim. Came from a rip with no
  identifying signals, or the user opted out of identification.
- **1+ rows** — identified, one row per source. Each row is independently
  *Exact* (`source_release_id` set) or *Approximate* (`source_release_id`
  NULL — group-only claim). A release can mix states across sources, e.g.
  Exact in MusicBrainz, Approximate in Discogs.

Cross-source equivalences (this MB release also corresponds to that
Discogs release) are encoded structurally: a release with rows in both
sources *is* the cross-link. No separate cross-references table.

## Albums aggregate over releases

`albums` carries no per-source identity column. An album is just a row
that groups its releases by shared identity:

> A release attaches to an existing album if it shares at least one
> `(source, source_group_id)` row with any of that album's existing
> releases. Otherwise it gets a new album.

This is the **loose attach rule**. "At least one" matters — a release
joins an album by sharing *any one* identity, not by matching the album's
full identity set.

## Why identity isn't on the album

The natural follow-up question — "if all releases in an album share a
group/master, shouldn't the album just own the group identity?" — runs
into the loose attach rule the moment you hit a real-world example.

**Concrete edge case.** Two releases in the same album:

- Release A: `{ (MB, X), (Discogs, Z) }`
- Release B: `{ (MB, Y), (Discogs, Z) }`

Both attach to the same album because they share `(Discogs, Z)` — but
their MusicBrainz groups differ (X vs Y). Album-level "MB group
identity" is ambiguous here: is it X, Y, or both?

To put identity back on the album we'd have to choose one of:

1. **Album owns a *set* of identity rows** (a mirror `album_identities`
   table). That's the union of `release_identities` joined through
   `releases` — same data, denormalized. Now every `set_identity` /
   re-identify / release-delete has to maintain album-level state in
   lockstep. A SQL view (or materialized view, if it ever gets hot)
   gives the same lookups without the invariant-maintenance hazard.

2. **Tighten the attach rule** to "all releases share the same identity
   set". That kills cases like "MB-only reissue grouped with Discogs-only
   original of the same album" — different sources identifying the same
   conceptual album would land as separate library albums. Restrictive
   enough that users would routinely have albums incorrectly split.

3. **Flat `album.musicbrainz_group_id` / `album.discogs_master_id`
   columns** (the pre-release-identity shape). Only encodes one identity
   per source — can't express the A/B example above. Also can't express
   Approximate cleanly without a separate flag, and a release identified
   in multiple sources has to flatten its identities into the album row.

The current shape sidesteps all three. Album identity is implicitly
derivable from `release_identities` joined through `releases`, and that
derivation handles the edge cases for free.

## Side benefit: re-identify becomes local

Because the album holds no identity column, re-identifying a release
just rewrites that release's rows in `release_identities`. The album's
identity automatically follows — it's a function of its releases.
`set_identity` doesn't need to reconcile album-level state; it only
touches the release.

If identity lived on the album, `set_identity` would need to recompute
the album's identity-set after the change (or split the release into a
new album, or merge two albums together — depending on whether the new
identity still shares anything with the rest of the album's releases).
All of that logic is implicit in the current shape.

## See also

- `bae-core/src/db/models.rs` — `DbAlbum`, `DbReleaseIdentity` field
  definitions.
- `bae-core/src/library/manager.rs` — `find_existing_album_for_import`
  (the attach-rule entry point).
- `bae-core/migrations/001_initial.sql` — the `release_identities`
  schema definition.
