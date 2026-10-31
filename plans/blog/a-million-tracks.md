# A million tracks: pay for the rows on screen, not the library

*Draft. Numbers come from bae and coven commit messages of 2026-10-26 to
2026-10-31, the measurement notes in `plans/`, and the output of the
committed benches, all run on one shared development Mac while other builds
ran. Absolute times swing with that load; the before-and-after pairs were
taken on the same machine, usually minutes apart.*

Other titles to consider: "Fifty rows, not the library", "Every write pays
for itself, no read pays for the library", "Where a music library's time
went at 100,000 albums".

> TODO: the million in the title. Only the playback queue (1,000,000
> tracks) and coven's device-local write (1,000,000 rows) were measured at
> that size. The lists were measured at 10,000 to 100,000 rows, the scan at
> 20,000 folders, the restore at 3,000 releases. Either measure the lists at
> a million-track library or retitle.

## The setup

bae is a personal music library. Its state lives in one SQLite database per
device, and coven syncs that database between the person's devices through
storage they already have. Every screen is a live query: core runs the
screen's SQL, delivers the rows, and runs it again when a commit changes
what it read.

That shape decides where time can go. A write commits; the live queries
that read what it changed run again; the screens draw the new rows. So
there are two places a library's size can leak into what the person waits
on: what a write costs, and what each live query's run costs. The rule we
hold both to is the same: the cost follows what changed or what's on
screen, never how much the library holds.

Most of what follows is the places where that wasn't true, how we found
them, and what each one measured before and after.

## The database is the only copy

The first class of slow code held a second copy of database state in
memory and worked on it there.

- **The upload queue.** An "outbox projection" read coven's whole upload
  queue into memory, labelled it with a second query, joined it to the
  pending records and diffed it on every change. With 10,000 and 20,000
  queued files that read took 25 and 50 ms, every time anything changed,
  to find the few files actually in flight. It's now a live query over
  coven's `coven_sending` view joined to the uploads: 38 µs at either size.
  The upload counts in the activity indicator went from 47 / 95 ms to
  23 / 44 µs the same way.
- **The download and export queues** lived in memory behind a lock, and
  each progress step cloned the whole queue for each reader to rebuild and
  compare: 23 ms a step at 50,000 queued releases. They are device-local
  tables now, read through live queries.
- **Counting cloud-only releases** asked coven, one release at a time,
  whether each file was pinned: 7.2 s at 10,000 releases. One SQL statement
  over coven's `coven_pins` took it to 32 ms. Keeping a pinned count per
  release by triggers (below) took it to 6.5 µs.
- **Cancel All** matched every queued file against each release's files in
  Rust, quadratic in the queue: 834 releases of 10,000 files took 2.6 s,
  1,667 of 20,000 took 11.9 s. One indexed statement per release: 21 ms and
  49 ms.

None of these copies was wrong on a small library. They were each a
second answer to a question SQLite already answers, kept in step by hand
and paid for in proportion to the whole queue.

## Sort values kept by triggers, in the same write

A list sorted by something that lives in another table (the album grid by
its artist, the composer list by how many works each composer has, the
Storage Manager by where each release's files are) can't use an index if
the sort value is computed at read time. SQLite reads every row, computes
the value, and sorts. The composer list joined every artist to every work,
role and track credit through an expression: its count took 72 s and a
window 38 s at 10,000 albums, 350 s and 187 s at 20,000, and both reran on
every import.

The fix in each case was a listing table that holds the sort values, kept
by triggers in the same transaction as the write that changes them, with an
index for each sort:

| List | Before, at 20,000 albums | After | What each write pays |
|---|---|---|---|
| composer list window and count | 187 s / 350 s | about 0.05 ms | an album import's write from about 2 ms to 3 ms |
| album grid, flat window and count | 4–80 ms | about 0.05 ms | about 0.3 ms an album |
| artist list window | 47 ms | 0.03 ms | (same write) |
| Storage Manager window | 10–64 ms | about 0.04 ms | about 2.5 ms a release import |
| Storage Manager counts and totals | 35–240 ms | 0.004 ms | (same write) |
| Uploading filter, 20,000 queued files | 31 ms | 0.25 ms | about 6 µs a queued file |

Because the triggers run inside the write, a reader never sees the listing
disagree with the rows it summarizes; there is no background pass that
catches it up. For the counts that follow coven's own tables (queued
uploads, pins), a test applies random sequences of writes (uploads
attached, completed and cancelled, files moved between releases, releases
deleted) and checks after every step that each release's kept count equals
the count probed from coven's views. Breaking either trigger fails it.

coven had to change for this to work: an app's triggers can't fire on a
view, so `coven_pins` and `coven_upload_queue` became tables coven keeps
with its own triggers, which the app's `AFTER` triggers may follow.

### The trap: a trigger that grows with the library

A kept value moves the cost to the write, so the write's cost has to stay
flat too. Twice it didn't:

- Every release added to an album rewrote the album's listing row with the
  values it already had, and that rewrote the listing row of every release
  the album held. Filling one album cost the square of its releases: 4,000
  releases under one album took 295 s. The row is now written only where a
  value differs.
- In the import list, every member of a folder group carried the group's
  date so the plain indexes could place it. A member newer than the group
  rewrote every other member: 5.2 million row updates for 5,000 folders
  under one top folder. The date now lives on the group's header alone.

The group's date also had to stop being a `MAX` over its members, read
twice per placement. A test now counts SQLite's own VM steps for the write
of a scan row, triggers included, and fails if placing the 2,000th member
of a group costs more than 1.5 times the 20th. With the date on the header
but the newest member still found by `MAX`, the 2,000th took 26,829 steps
against the 20th's 3,069; reading the newest member off the group's index
made both 2,829.

## Windows: read fifty rows from the row beside them

Every paged list read its windows with `LIMIT 50 OFFSET n`. SQLite
implements `OFFSET` by reading and discarding the first `n` rows, so the
last window of a long list cost the whole list. At 100,000 rows, the last
window took (the `window_cost` bench, on a loaded machine):

| List | Last window by offset |
|---|---|
| album grid | 31–64 ms |
| artist and composer lists | 46–53 ms |
| Storage Manager | 177–602 ms |
| import list | 224–226 ms |
| album grid grouped by artist | 2.2–5.7 s |

Now a window next to a row whose place the read already knows starts from
that row's sort values. Every sort ends in a unique term, so each row has
exactly one place. For a sort by artist, then title, then id, the rows
after a known row are: the same artist and title with a greater id, then
the same artist with a greater title, then a greater artist. bae runs those
as up to three index seeks in turn (`artist IS ? AND title IS ? AND id > ?`,
and so on outward), each limited to the rows the window still needs.
Scrolling always reaches a window next to one the
list already read, so in the same bench every list's last window by
scrolling took under 5 ms at 100,000 rows, and the grouped grid's under
2 ms.

> TODO: the after numbers come from the bench run behind `9dc24406f`
> (scratch `cursor-paging/after.log`), not from a commit message. Rerun
> `window_cost` on main and cite that.

The hard part is knowing a row's place is still right. A live query's next
run reuses the rows its last run read, but a write between the two could
have moved them. Each table a list sorts carries a version count
(`list_order_versions`), advanced by triggers in every write that inserts,
deletes or reorders its rows; a read starts from its last read's rows only
while those counts stand. A jump with no known neighbour, such as dragging
the scroll bar to the middle, still reads by offset. In the same run, the
grouped grid's last window reached by a jump still took 1.8–5.8 s.

> TODO: decide whether jumps in the grouped grid are worth fixing, and say
> so here.

Every sort of every list has a test that runs `EXPLAIN QUERY PLAN` on each
seek and fails on a full scan of the list's table or a temporary B-tree
for the sort.

## Search: an index of three-letter pieces

Library search ran five `LIKE '%q%'` statements per keystroke, each reading
a whole table. Triggers now keep every searched title and name, lowercased
by the same Rust function the query uses, in an FTS5 trigram index beside
the row's key. A query of three or more characters reads only its matches:
about 0.1–2 ms per kind at 20,000 albums, against 4–40 ms before.

That helps a rare term. A broad one, two letters that are in most titles,
still finds most of the library, and reading every match to sort it costs
more than the unsearched list. The Storage Manager's filter, which narrows
the list as the person types, picks per search:

- **The count.** While the text is in fewer titles than a pass over every
  title costs, it counts what the trigram index finds. Past that, it counts
  the releases the pass doesn't find and takes them off the counts the
  listing already keeps.
- **The window.** When the search finds few releases, it reads them and
  sorts them. When it finds many, it walks the sort's index as the
  unsearched list does and tests each release it reaches against the text
  the search tables keep for it, a few lookups per row, so a broad search's
  window costs about what an unsearched window does.

> TODO: numbers. The commits (`7173795d0`, `c17b465c6`) give none. The
> scratch run behind `c17b465c6` shows, at 100,000 releases, a broad
> search's count in 14.5 ms and its scroll windows in about 1.7 ms, and a
> rare search's count in 1.1 ms; confirm with a committed run.

The import list's search uses the same trigram index over each row's shown
text.

## A million rows in the playback queue

The queue is two device-local tables, Up Next and the lane of what's
playing from, one row per entry. Playing the whole library shuffled writes
one row per track: a million rows for a million-track library.

**Clearing it.** Every write through coven runs under a pre-update hook,
which records each row change so live queries know what changed and synced
tables know what to send. While a pre-update hook is set, SQLite can't drop
a table's pages in one step; it deletes row by row so the hook sees each
one. Clearing a lane of 1,000,000 tracks took about 3 s, on every Clear
playing from, every launch that didn't resume, and inside every play that
replaced a full lane. The bench put the hook's share plainly:

| Plain SQLite, emptying 1,000,000 rows | Median of 5 |
|---|---|
| no hook | 0.031 s |
| with a pre-update hook that does nothing | 2.89 s |

coven now has `write_bulk`: one transaction of plain SQLite, with no hook,
for device-local tables only. Its authorizer sees every table a statement
would write, including through triggers and foreign-key actions, and
refuses the whole bulk write if any is synced, since a change no hook
recorded could never sync. After it commits, every live query that read a
table it wrote runs again once. The queue's fill, its clears and the
emptying at launch go through it. bae's clear went from a median of 2.98 s
to a few hundredths of a second (fastest of five runs 0.05 s; median
0.12–0.30 s on a machine at load 3–5).

A trigger on the lane would bring the row-by-row delete back, so the lane
has none: the trigger that cleared "what's playing from" when the lane
emptied, which ran once per deleted row, became part of the writes that
empty it. For the same reason the lane's upcoming tracks are still read by
`OFFSET` (0.75 ms at the end of 100,000 tracks): a version-counting trigger
would run for each of the million rows one play writes.

**Filling it.** What a million-row fill still costs is mostly index pages
written out of order past SQLite's page cache, which defaults to 2 MiB.
Numbering and inserting the rows in track order lets the table and its
track index grow at their end, so only the integer position index takes
rows out of order: about 10–15% off the fill at the default cache. A larger
cache does more. The same fill in plain SQLite on the same file, committed,
median of 5:

| Page cache | Inserted in track order | Inserted in play order |
|---|---|---|
| 2 MiB (default) | 3.70 s | 4.37 s |
| 16 MiB | 2.93 s | 2.53 s |
| 64 MiB | 2.80 s | 2.32 s |
| 256 MiB | 2.45 s | 2.10 s |

Raising the cache costs nothing for a small library: `cache_size` is a
ceiling, and SQLite holds only pages it has read. The decision is 64 MB per
connection on desktop and 16 MB on phones, with the fill going back to
play order, which is the faster of the two once the cache holds its
indexes.

> TODO: this table, the clear's medians above, and the hook table come
> from the queue bench's runs around `7361c8fd4` (scratch
> `result-before.txt`, `result-final.txt`); the commit message says only
> "about 3 s" and "about 0.03 s". The
> page-cache ceiling isn't in bae yet (coven can set it; bae doesn't).
> Update once it lands, with the bench's numbers from that commit.

## A scan whose cost per folder stays flat

A first scan of a watched folder writes one row per release folder it
finds, so a scan should cost the same per folder however many it has
already written.

The first fix was in how each scan write read what it compared against.
The loaders took `watched_folder_path = :root AND (:only IS NULL OR key =
:only)`: one statement for "the whole folder" and "one entry", which
SQLite can only serve by the root, so every write read every candidate,
file and part under the root. On a real scan of 10,000 folders the write
grew from 7.6 to 18.9 ms and listing every folder took 357 s. With a
statement of its own for one entry, a primary-key lookup, the write stays
at 7.5 ms and the scan lists in 186 s. The optional parameter was what
kept SQLite off the key's index.

Even after that, a tree of 20,000 folders under one top folder took
3,321 s, 55 minutes, and each folder cost more than the one before: from
18.8 ms to 246.8 ms a folder across the scan's tenths. Timing the scan in
tenths (`test_scan_cost`) and reading what grew found three more causes:

1. **The group date on every member** (above).
2. **The newest member read by `MAX` over the group,** twice per
   placement. Now read off the group's index.
3. **Two commits per folder.** Commits and WAL checkpoints were about half
   of what was left. The scan now writes every folder the walk has handed
   over, up to eight, in one transaction. A crash still loses only the
   unfinished write; the next scan finds those folders again.

| 20,000 folders | Before | After causes 1–2 | After batching |
|---|---|---|---|
| flat | 266.2 s | 160.2 s | 77.0 s |
| one tree | 3,321.1 s | 165.4 s | 96.1 s |
| flat, import list open | 340.1 s | 203.9 s | 96.0 s |

After the first two the cost per folder is flat: 6.8 to 8.5 ms a folder
across the tenths of the one-tree scan. With batching, the
open import list also answers about once per four folders (4,999 answers
over 20,000 folders, against 19,670).

> TODO: these come from `plans/handoff-scan-growth.md`, measured on the
> branch before it was rebased onto main, release build, shared machine.
> The landed commit `08871bac2` cites only the batching step (flat 160 s to
> 77 s, one tree 165 s to 96 s). Rerun `test_scan_cost` on main.

## What a synced write costs

Every row bae imports goes through coven's write path: a merge record per
cell, references, unique values, the change for live queries. coven's
`insert_cost` bench measures it as an app pays it: the median per-row time
of 40 writes of 500 inserts through a trigger that reads a six-table view,
in the release profile. Five commits on one day, each a separate change:

| Change | Per row |
|---|---|
| starting point | 70.7 µs |
| record changes only in tables a live query can read | 65.0 µs |
| stage a merged row once instead of three times | 57.2 µs |
| stop recording merge state that only one query needed | 54.3 µs |
| read the schema version and column ids once per write, not per row | 52.3 µs |
| a row's cells in one multi-row upsert | 48.1 µs |

Before those, app statements were prepared afresh on every run, and SQLite
compiles every trigger a statement can fire, and the views they read, into
the statement. Keeping them in the connection's cache took a statement
whose triggers read a six-table view from 20 µs to about 3 µs.

**Device-local tables** don't sync, so coven now keeps only the tables,
columns and (where a waiting live query reads by key) row keys a write
changed, not every column of every row. Filling 1,000,000 local rows went
from 1.37 s to 0.39 s, with plain SQLite at 0.35 s.

**The statement journal.** SQLite keeps a journal per statement, so a
failing statement can be undone alone. An FTS5 index kept by a trigger
merges its segments inside the app's statement and passes SQLite's 64 KiB
in-memory journal limit within a few hundred rows; the journal then moves
to a temporary file for the rest of the transaction, and every later
statement that journals a page makes two system calls for it. That was 19%
of an import's time. coven now keeps the journal in memory up to 124 KiB:
under the 128 KiB the unix file layer writes at once, which the journal
needs when it does move to a file. 256 KiB failed a 32 MiB update with
`SQLITE_FULL`.

**Checkpoints.** SQLite's automatic checkpoint runs inside the commit that
takes the WAL past 1,000 pages, `F_FULLFSYNC` included on Apple platforms.
A live query that reruns after every commit keeps a reader's snapshot open
nearly all the time, so that checkpoint can't finish and every later commit
runs another: 2,000 small commits took 2.3–4.5 s with such a query open,
against 0.36 s with none, and the WAL grew to 23 MB. coven now checkpoints
from a task of its own, never inside a commit: the same 2,000 commits take
0.38–0.45 s with the live query, and the WAL stays under 5 MB.

**The query planner.** Nothing ran `ANALYZE`, so SQLite planned every query
by guesses. coven now runs `PRAGMA optimize` as the database opens and
closes (about 2 ms the first time on four 20,000-row tables). That brought
its own surprise: on a new device each FTS5 index's data table holds two
rows, and with those statistics SQLite planned FTS5's segment cleanup as a
full scan, 38 µs a call becoming 233 µs. coven now drops statistics for
tables under 100 rows. bae, for its part, writes the statements that
matter so they plan the same with or without statistics: the import pane's
"already in your library" check went from 71 ms at 10,000 releases and
186 ms at 20,000 to 0.18 ms, by matching one JSON array of checks by plain
equality on the indexes instead of one `OR` per release.

## Restoring a library on a new device

A new device restores by loading the newest snapshot of the database and
then applying the writes after it. For a 3,000-release library (81,000
rows, a 107 MB snapshot, with references, unique values and CHECKs, close to
bae's schema), measured with in-memory storage so every millisecond is
coven's, on the branch that landed as `f730f4e3f` to `738f8440a`, three
interleaved rounds, medians:

| Library | Restore before | After |
|---|---|---|
| tables referencing each other | 39.45 s | 8.54 s |
| plain tables with a trigger-kept listing and FTS5 index | 20.50 s | 7.50 s |

Installing had decoded every staged record again and written each row
through the write path's per-row helpers, about 32 statements a row. Now
staging writes each record's fields as columns as the bytes stream in, and
the install copies them with SQL a table at a time.

Before installing, coven checks each write the snapshot names against the
writes it had read and its device's earlier writes. That check was one
query per write whose `OR` no index could serve, so each write scanned all
the others: 26,000 writes took 52 s, 84 s of a 97 s restore in one library
built when each finished upload wrote its own slot. One pass in slot order
with a running maximum answers the same question: 5 ms at 26,000. A test
counts SQLite's VM steps for 2,000 writes and fails above 50 a write: the
old scan took 26 million, the new one 16,000.

Two fixes on the network side, against the person's own library: reading
up to 16 slots ahead instead of one at a time (166 serial GETs at about
125 ms each), and not fetching a slot again when its bytes were already
held from an earlier pass (167 serial GETs, about 24 s of a 47 s restore).

## Live queries: rerun less, without paying on every read

A live query that reruns on every write to a table it read is cheap to
build and expensive to have open during a scan. With the import screen open
on one candidate, 1,000 scan writes of other folders reran the candidate's
pane 957 times, every one with the same result.

So coven narrows what a live query depends on. As SQLite prepares a
statement, coven learns the tables and columns it reads, and now also the
rows: a condition on any column narrows a table to the rows it allows,
checked against each changed row as it was before and after the write. A
join narrows each side by the values the other side's rows hold; so do
subqueries, CTEs and the app's own views. A test generates queries and
writes and checks a live query is marked to rerun whenever its result
changed. The same 1,000 scan writes now rerun the pane 0 times, and a
committed test (`test_import_live_query_reruns`, run by hand) asserts
exactly that.

Then a bench found the cost had moved. To narrow a join, each run read the
join partners' values, up to 1,000 per partner, so a later write could be
judged in memory. Those reads often scanned whole tables. The import
list's window at 50,000 folders (release build, `window_cost`):

| | Windows, top to end |
|---|---|
| before the narrowing reached bae (`d606c32c9`) | 2.1–16.5 ms |
| after (`c17b465c6`) | 215–1,609 ms |

No test caught it: rerun counts went down and every assertion held. coven
now counts SQLite steps for an app's tests, its own value reads apart from
the app's statements (`test_track_work`), so a test can run a screen over
10 rows and over 1,000 and fail if the steps grow. One-shot reads, which
nothing reruns, no longer read the join values at all.

The fix is to judge joins at write time instead: a run reads nothing for
its joins, and a write that changes a joined table asks, in its own
transaction before it commits, whether a partner row in the query's scope
holds the changed row's old or new value. One lookup per affected query; a
lookup that runs past 1,000 steps counts as a match, so a write's
cost stays bounded whatever the app's indexes. A read costs what it shows;
a write pays for the live queries that join what it changes.

> TODO: this is committed on a coven branch (`9c1052977`), not landed.
> Fill in the import list and Storage Manager windows and the write cost
> before and after once it's measured, and check the pane's rerun count
> stays 0.

## Scrub points, one row per track

bae stores scrub points so a seek inside a track decodes from a known
frame (the earlier post on seeking covers why). It took points for every
MP3 and for FLACs without a usable seektable, and stored one row per point:
1.2 million rows and about 128 MB for a 50,000-track all-MP3 library, each
row also a sync record. A segment's points are now one row, packed into a
blob of delta varints: 50,000 rows, 14.3 MB, 286 bytes a track.

The rule for when a FLAC needs points also changed. A FLAC with one seek
point every 10 minutes got none, since its seektable wasn't empty, so a
seek decoded up to 10 minutes of audio. Import now takes points unless the
seektable lands every seek within the points' 10-second spacing. On a
12-minute FLAC with points at 0 and 10:00, a seek to just before 10:00 read
43.7 MB in 480 ms; from the scrub point it reads 0.8 MB in 7 ms.

## A log level that made library creation quadratic

Creating a library took 3.07 s, most of it tokenizing bae's 261 KB schema.
`tracing-subscriber`'s setup sets the `log` crate's maximum level from its
own filter, DEBUG because bae's crates log at debug. With debug on,
`sqlite3-parser` built a line and column position for every token it
scanned by walking the SQL from its start, which is quadratic in the
schema's length. Capping `log` at INFO, the level a sink keeps for
dependencies anyway, took creation to 0.92 s. The bench
(`test_library_create_cost`) creates the library under the same logging the
apps install, since without that it never saw the cost.

> TODO: a patch to the scanner itself is decided (in a fork, for review);
> say whether it landed.

## Keeping the numbers

A speed fix keeps its measurement in one of three forms:

- **Plan checks.** Thirteen test files in bae-core run `EXPLAIN QUERY PLAN`
  and fail on a full scan or a sort where an index should serve, including
  one per sort for every list's seeks.
- **Counts.** SQLite's VM steps (the group-member and snapshot-check tests
  above) and statements per write. These come out the same on every
  machine, so they run in the ordinary suites and CI enforces them. Each
  was run against the old code: the snapshot test fails on the old scan,
  and the group test's old code read 26,829 steps against 3,069. The
  pane's rerun count uses coven's live-query run counter; it asserts 0 but
  is an ignored test, run by hand, since it writes 1,000 folders.
- **Ignored timing tests** run by hand in the release profile:
  `window_cost` at 20,000, 50,000 and 100,000 rows, `test_scan_cost`, the
  queue bench at 1,000,000 tracks, coven's `insert_cost` and `restore_cost`.
  Wall time on a shared machine is too noisy to assert on, so these print,
  and the numbers go into the commit message.

> TODO: a performance suite that records those timings per machine and
> compares medians against a committed baseline, with a 25% tolerance and
> no comparison when load exceeds half the cores, is decided but parked
> (bae #376); no `scripts/perf.sh` exists yet. Describe it only once it
> does.

## What we learned

A screen should cost what it shows, and a write what it changes. Most of
the slow paths were one of a few shapes: a copy of database state in
memory, a sort value computed at read time, `OFFSET`, a statement with an
optional parameter that defeats its index, a trigger that rewrites more
than the row that changed, or a cost moved from one side to the other
without anyone counting it. Each showed up the same way: time the
operation at two library sizes, and if the larger one costs more per row,
read what grew.

## Open questions for the post

- The title's million (see the TODO at the top).
- Whether to include the write-side narrowing numbers or end on the
  read-side regression.
- Whether to link the committed benches and the step-count tests.
- Whether the queue section should wait for the page-cache change.
