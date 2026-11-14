# Screens as queries

Every screen bae shows is one SQL query over the library database, delivered
as a coven live query. The database on the device is the one source of truth:
a change is a write to it, and the live queries carry that write to every
screen that reads what it changed.

## The rule

- **SQLite does the work.** The query joins, filters, sorts, counts and
  windows. Rust only turns the rows it returns into bridge types, and the apps
  draw what it returns, with no caches, joins, overlays or sorting of their
  own.
- **A value with no table travels on its own stream.** A value that changes
  many times a second and is stored nowhere, such as an import's progress or a
  transfer's bytes, goes to the view that draws it on a keyed latest-value
  stream (`KeyedValueStream`, spec §10.2), never into a list query's rows,
  filters, sorts or counts.
- **A value computed across tables that a screen sorts or filters by is kept
  in a table.** SQLite can't index across tables, so triggers write such a
  value into a device-local table in the same write that changes any table it
  comes from, and an index on that table serves the screen. The album grid
  sorts by `album_listing`, and the import list reads its tabs, states,
  groups, dates and shown text from `import_list_placement`
  (`bae-import/src/tables/import_list/placement.sql`). No read sees such a
  table behind the tables it comes from.
- **What decides a tab, a group, an order, a filter match or a count comes
  from the database.** The import list's coarse states are rows of their own
  (`import_candidate_standing`, `import_candidate_verdict`,
  `import_candidate_failure`), so the list's query reads them with everything
  else.

- **The read is the query and nothing else.** No places, counts, windows or
  last results kept in Rust between reads; no side pass beside a live query; no
  field written into a request or a table only to make a query rerun; no size
  cutoff choosing between two read paths. Everything a screen shows beside its
  rows (counts, totals, groups, headers, sums, facets) comes from SQL, in the
  same query or its own live query. A read that need not follow changes is a plain SQL
  read, not a live query. The rule agents load is
  `~/dev/agent-instructions/projects/bae-rules/reactive-screens-read-plain-live-queries.md`.

## Why

- **A read costs what it returns.** An indexed window touches about the rows
  it shows. Rust that loads a whole table to filter, sort or count it pays for
  every row on every rerun, and a live query reruns on every write to a table
  it reads.
- **Reruns that cost the whole table starve the WAL checkpoint.** A
  checkpoint copies the WAL back into the database only up to the oldest
  snapshot a read still holds, and starts the WAL over only when no read is
  running. Back-to-back reruns that each read everything leave it no gap, and
  the WAL grows.
- **One answer serves every surface.** The apps, automation and MCP read the
  same queries, so they see the same rows from one snapshot, decided by one rule
  in one place.
