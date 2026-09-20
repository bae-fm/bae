# Application schema changes and retained history

## Contract and ownership

This is required correctness work for the import queue's origin removal and
release/album enrichment. The background worker owns implementation in isolated
bae and Coven checkouts; the parent reviews and coordinates fast-forward landing.
Keep the user's Coven main checkout and its specification edits untouched.

Application database migrations must preserve historical writes, snapshot tails,
and pending local journals. Authenticated original packages and their schema
versions remain unchanged. Current-schema application bytes are a derived value,
not a rewritten signed document. No obsolete record-kind flags, fake identities,
nullable kind defaults, or sentinel-based current model are permitted.

## Confirmed defects

Real schema-41 SQLite changesets pass Coven identity admission after schema42.
An INSERT then fails the required kind constraint. An UPDATE can turn the former
self-parent sentinel into a falsely known parent. Migration41 removes synced
release columns and requires the same history conversion. Local write journals
also omit their authoring schema, while publication stamps retained bytes with
the currently open database version.

## Chosen design

Extend the application migration ladder with explicit changeset transformations.
An adapter names the tables whose row meaning changes and transforms typed old
and new cells from the preceding version into the next version. SQL NULL and an
undefined UPDATE cell remain different values. Preserve operation, primary key,
indirect flag, changed-column masks, and row clocks. Unaffected tables pass
through unchanged; no automatic inference of changed row meaning from a renamed
column is allowed.

Validate original packages against their recorded authoring schema during preliminary
preparation. Compose adapters to the current schema inside ordered transactional
materialization, before current-schema validation, audience checks, conflict
resolution, row application, retained replay, and local replay. A preceding
package in that transaction may supply immutable row context for a sparse UPDATE;
preparation must not treat that not-yet-applied INSERT as a deleted row. Keep the original package for signatures and
history identity; all downstream row interpretation must use the converted
changeset. This includes callers that currently re-read package.changeset after
preparing a package. Verify and migrate snapshot images before applying their
converted tails, including Circle bootstrap paths.

Sparse UPDATEs may omit information needed by a migration. Supply context only
for explicitly declared immutable row identity fields, using the record primary
key. For release records these are catalog and external key: production identity
replacement deletes the old row and inserts a new UUID. Reject an update that
changes these declared identity fields. If the target row is absent, retain the
existing delete-wins behavior rather than inventing values. The migration API
must not offer arbitrary mutable current-row values as historical facts.

Record each local journal write's schema version at capture and retain it through
preparation, retries, publication, and replay. Publish that captured version.
For existing unversioned journals, first use an already prepared/authenticated
package's exact version when available. Otherwise compare recorded table layouts
with registered historical schemas. Accept only a provable interpretation: if
several versions match, their affected-table layouts and composed transformations
must be equivalent. Ambiguity is a typed upgrade failure that rolls back, never
a current-version default.

Migration41 drops the removed origin cells while preserving retained metadata,
identity, row clocks and other fields. Migration42 uses one deterministic mapping
of synced record state for image migration and changeset migration: other album
catalogs become Album; MusicBrainz/Discogs remain Pressing; a distinct group key
is a known parent; an equal key alone is unknown. Local archived provider bytes
cannot affect this canonical transformation. An equal key alone did not establish
parent identity in the previous synced model. Keep that parent unknown and
preserve every archived provider document so explicit source reapplication can
read its evidence. Do not automatically apply cache metadata during migration
or add a host post-migration write facility for it.

## Verification

### Local journal implementation

Capture the host schema version inside the same transaction that attaches the
SQLite session, before executing host SQL. Persist that required version in
`store_write_schemas`, one row for each retained write effect; all partitions
of a write describe that capture. Carry it through
`PreparedStoreWrite` and `MergeReplayWriteEffect`; publication uses this value
for both package contents and signed package references. Rebase retains the
original version and bytes; only the application effect is converted. Retained
journal manifests include the version so cache identity covers its interpretation.

Add Coven bookkeeping migration 3, creating `store_write_schemas` with a required
nonnegative `schema_version` while preserving write ordinal, status, prepared
state, payload ownership and child records. Recovery runs inside the existing outer
database-open transaction before host migrations. Prepared or accepted package
evidence supplies a version only when its captured bytes agree with the journal;
otherwise the host migration history resolver must prove the recorded layout.
Do not label old writes with the pre-upgrade live schema: earlier upgrades may
already have left older pending bytes in that database. Folded receipts lacking
payloads have no schema row. Folding deletes the schema row atomically with
the effect, and loading rejects any retained effect without its schema row or
a receipt that still has one.

A rebased write also retains the schema version of its actual application effect
inside `RebasedStoreWrite`, separately from the original publication bytes.
Capture this version in the transaction that records the actual effect. Migration
3 converts historical rebased JSON through explicit layout recovery of that
actual payload. Discard chooses the version attached to its chosen payload,
inverts its original operations, then converts and applies each inverse in reverse
journal order in one transaction. Immutable row context is read after each prior
inverse has applied. A failed conversion rolls back rows, cleanup, and receipts.

Verification exercises capture, restart after a host schema upgrade, package
preparation, and exact package version/bytes. Include existing unversioned local,
pending, prepared and folded journal states; preserve foreign-key children and
roll back the complete open when interpretation is ambiguous.

- Historical INSERT/UPDATE/DELETE across41 and42 through actual Coven admission
  and merge application, with valid UUIDs and HLC stamps.
- Migration image versus migrated historical-write equivalence for the same
  synced starting state; differing local provider caches do not change it.
- Known/unknown parent, equal release/master IDs despite different local archive contents,
  album-only catalogs, unchanged identity context, refused identity mutation,
  missing target update, SQL NULL and undefined cells.
- Old pending local writes published after upgrade retain their original schema;
  prepared writes survive restart. Ambiguous unversioned recovery rolls back.
- An INSERT followed by a sparse UPDATE in the same received batch; a failed
  later conversion rolls back the preceding INSERT and frontier.
- Mixed old/new remote writes, retained replay, snapshot tails, Circle image/tail
  import, local-journal rebase, and retry after failed conversion.
- Schema conversion failure cannot advance a frontier or commit partial rows.
- Preserve original authenticated bytes and existing row conflict outcomes.

Implement the dependency capability in a focused Coven commit, verify it against
its own production paths, then update bae's pinned dependency and register the
application adapters in the enrichment branch. Remove the temporary direct
coven-database test dependency: bae continues through the Coven facade, including
its test capabilities. Run normal hooks, affected platform checks, parent review,
and CI before main integration. Continue the remaining import queue afterward.

## Rebased local effects and source snapshots

A write's original captured bytes and the actual effect after replay can have
different authoring schemas. Store the actual effect's schema alongside its
rebased hash; do not borrow the original write's version. Discard converts each
inverse under its own version in reverse journal order inside one transaction.
Historical recovery must prove the matching prepared package bytes, not merely
find a signed package that happens to name a schema number.

The import snapshot extension uses ordered migration43. Migration40 continues
writing its historical primary-document/duration shape; migration43 adds required
partner document sets from the exact stored provenance. Preserve the already
frozen primary document strings. Missing required partner archive data fails and
rolls back the upgrade; no default deserializer silently changes the snapshot
contract. Regression coverage includes an earlier updated row followed by a
missing partner, verifying the earlier update also rolls back.
