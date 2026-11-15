# Screens as queries

What a screen shows is what its SQL query returns, delivered as a coven live
query. Rust turns the rows into bridge types and the app draws them, with no
cache, enrichment or other step between the query and the screen. A long list
shows several windows at a time, and each window is its own live query. The
database on the device is the one source of truth: a change is a write to it,
and the live queries carry that write to every screen that reads what it
changed.

The rule agents follow is
`~/dev/agent-instructions/projects/bae-rules/reactive-screens-read-plain-live-queries.md`.

## Where bae keeps what a screen shows

Everything a screen shows is stored in the database, with two exceptions:

- A value that changes many times a second and is stored nowhere, such as an
  import's progress or a transfer's bytes, goes to the view that draws it on a
  keyed latest-value stream (`KeyedValueStream`, spec §10.2).
- State that lives only on screen (hover, focus, an open popover or menu, a
  drag in progress) stays in the view.

A value computed across tables can be kept in a table of its own. SQLite can't
index across tables, so triggers write such a value into a device-local table
in the same write that changes any table it comes from, and an index on that
table serves the screen. The album grid sorts by `album_listing`, and the
import list reads its tabs, states, groups, dates and shown text from
`import_list_placement` (`bae-import/src/tables/import_list/placement.sql`).

## Why

The database is bae's own: SQLite on the device's disk, with no server or
network in between. SQLite reads and writes fast, and coven reruns a query
whenever a write changes what it read, so a screen that shows a query's result
stays current with no other code.

Data that Rust copies into memory and works on there (a structure built from
rows, a filter, a sort, a count, a cache) stops following the database. Some
other code then has to notice each change and redo the work, and until it does,
the screen shows an old copy. That code does what SQLite and coven already do,
worse. So nothing goes between the query and the screen.