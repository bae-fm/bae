# One owner for every table

*Draft. Facts come from plans/bae-core-crates.md (2026-10-31), the six
bae-import audits in plans/core-crates-audit/ and their merged cut list
(2026-11-03), and bae's commit history from 2026-11-02 to 2026-11-03. Line
counts are of source files without their tests.*

Other titles to consider: "Who writes this table?", "Domains, not topics",
"Taking a music library's core apart by owner".

> TODO: the end of the story. This is written while bae-import's cuts are
> under way and before the domain map exists. Rewrite the last two sections
> once the checker enforces the map.

## Before

bae is a personal music library. Its core is Rust, under native apps for
macOS and Android, and all of its state lives in one SQLite database per
device that coven syncs between the person's devices.

Until the end of October 2026 that core was one crate, bae-core: 133,000
lines of source and 115,000 lines of tests. Nearly every top-level module sat
in one dependency cycle of 19 modules: the database, import, identification,
the library, playback, the queue, configuration, diagnostics, the UI types
and more. Nothing in the code needed a two-way dependency. The cycle came
from five things in the wrong place:

1. Import's storage lived in the database module: 17,400 of its 33,000
   lines were methods over import's own tables.
2. A model of a catalog's release sat inside import, and identification, the
   database, configuration and the library all built on it.
3. Identification and signal extraction ran on import's candidate runtime,
   while import ranked its rows with identification's types.
4. `LibraryManager` was used as a bag of handles. Import and playback called
   it for database reads, configuration and telemetry, nearly always as a
   one-line pass-through.
5. Shared vocabulary (error categories, track positions, list windows) lived
   in high modules that low ones then had to name.

Each cycle broke by moving an item to its real owner, or by handing a
service the lower handles it needed instead of the manager. Then the crates
came out bottom up: bae-diagnostics, bae-app-dir, bae-db, bae-preferences,
bae-identify, bae-playback, and on 2026-11-02 import itself, cut out into
bae-import. What was left of bae-core became bae-library.

That fixed the dependency graph, but not what was inside each crate.
bae-import alone is 113,000 lines, and inside it nobody could say who keeps a
given table right.

## What "who keeps a table right" means

Take `local_release_origin`, the table that remembers which folder a release
was imported from. Import's commit wrote it. A separate file read it. The
same SELECT was then written out again in five more places: the preparation
read, the pane read, the commit itself, the folder scan's write and the
import list. Five copies of a question is five places that can answer it
differently.

Or removing a watched folder. The removal deleted rows directly from the
tables of four other parts of import: skipped candidates, folder discovery,
release groupings and candidate state. Each of those parts had its own rules
about its rows, and the removal skipped all of them.

Or this pair, found while moving code: when the person changed where an
import candidate's metadata came from, bae discarded their answers about
which artist the credits meant, because an answer belongs to the source it
was given for. Label answers are stored the same way and follow the same
logic, but nothing discarded them. Nobody had decided that; the label code
was written later, in a different file, and the rule never reached it.

None of these is a crash. Each is a place where the code that holds a piece
of data and the code that changes it had drifted apart.

## After

The shape we're building toward, for bae and for coven:

- **Every piece of data has one owner, a domain.** A domain owns a set of
  tables and the rules that keep them right: the import list, the playback
  queue, preferences, the library's records. Its tables, triggers, SQL,
  reads, writes and tests live together in one home. Nothing else writes its
  tables. Other domains read them.
- **A domain is not a topic.** "Covers" is a topic: a release's stored cover,
  the covers an import candidate offers and the images a catalog lists
  belong to three domains with three sets of rules. One `covers.rs` holding
  all three would hide who keeps which right. Each goes with its own owner.
- **Crates are layers of domains.** A crate groups the domains at one level.
  Lower layers never reach up, and no crate mixes owners.
- **Each rule is written once,** in the domain that owns it. The apps and
  the bridge show what core gives them and work nothing out themselves.
- **A checker enforces it.** coven's owner-construction checker already
  knows types, constructors and lifetimes. It is learning domains: a map of
  each domain's home, tables and owner, kept in the repository. With the map
  in place, the pre-commit hook refuses a write to a table from outside its
  domain, so the shape can't drift back one convenient shortcut at a time.

## Choosing a domain's layer

"Lower layers never reach up" sounds like a rule you have to fight, so it's
worth saying why it isn't one.

A domain goes in the lowest layer that has everything it needs. If a domain
seems to need something from a layer above it, that isn't a dependency to
allow. It's a sign that something is in the wrong place. bae-core's cycle of
19 modules looked like a tangle of real two-way dependencies, and it had
none. Every one of the five tangles above was an item sitting in the wrong
module.

So the fix is always one of two moves:

- **Put the item where it belongs.** The catalog's release model sat inside
  import, so everything that read a catalog had to depend on import. Moved
  into bae-catalogs, at the bottom, it depends on nothing of bae's, and
  import, identification and the library all read it from below.
- **Hand the lower domain what it needs instead of letting it reach up.**
  Import called `LibraryManager` for database reads, configuration and
  telemetry, which made import depend on the library above it. Each of those
  calls was a pass-through to something lower: the database, preferences,
  diagnostics. Import now takes those handles directly when it's built, and
  the library, which builds it, passes them in.

When one operation spans domains, it belongs to the higher one, and it calls
the lower owner's writes rather than touching its tables. Importing a release
writes the library's records: artists, releases, tracks, images. Import is
above the library's records, so the import commit runs the whole operation,
but every library row it creates goes through bae-db's own writer. Artist
merges were the exception, writing `artist_images` and `artist_merges`
straight from bae-import. Moving those writes into bae-db is one of the cuts
already made.

Today's layers, bottom up:

| Layer | Crates |
|---|---|
| 0 | bae-catalogs, bae-audio, bae-types, bae-util, coven |
| 1 | bae-db, bae-app-dir, bae-diagnostics |
| 2 | bae-identify, bae-track-audio, bae-preferences |
| 3 | bae-import, bae-playback |
| top | bae-library, bae-bridge, the apps |

One place doesn't resolve neatly yet: the schema. Each domain's tables and
triggers should live in its home, but bae-db sits below the domain crates and
assembles the one database migration. Which crate puts the domains' SQL
together, without bae-db reaching up to find it, is the first question the
domain map has to answer.

## The method

For each crate, the same four steps.

**Audit.** Agents read the crate's files completely, one area each, and list
who writes which tables, what sits in the wrong place and which logic is
written more than once. bae-import got six audits: candidate state, the
tables layer, the handle and runtime, scanning and watching, file reading,
and identification and preparation. Their findings were merged into one cut
list, each item checked against the code again before anyone cut.

**Cut, one owner at a time.** The cut list puts wrong owners first: move each
write into the domain that owns the table, then the files by domain, then
the tests beside what they test, then narrow what other code can see. Each
move is its own commit with the crate's tests run. A move that changes
behavior is a bug fix and starts with a failing test.

**Fix what the cuts turn up.** Moving code into its owner puts the owner's
rules next to every write, and that is where bugs show. Removing a watched
folder announced the folders left watched by reading them again after its
write; if that read failed, the app was told no folders were watched at all.
The fix reads the list inside the removal's own write. Writing tests for the
artist merge in bae-db, its new home, found a bug in coven: a write that
moves a file reference from one row to another failed, depending on how the
two rows' keys sorted, because coven checked for the file's last reference
after each row instead of after the whole write.

**Record the domains and switch the checker on.** Once a crate's writes sit
in their homes, its domains go into the checker's map, and the rule turns on
only when it finds nothing. From then on the hook holds the line.

One more habit runs through all of it. Any agent that comes across the same
check written twice, while doing anything else, either merges the copies on
the spot or queues the merge, naming every copy. Nothing is passed by.

## How we get there

The order, as of 2026-11-03:

1. **bae-import.** The wrong-owner cuts are done: the commit's deletes, the
   import origin, scans and candidate state writing each other's tables, the
   watched-folder removal, matches and failures, and the library tables
   import was writing directly, which moved to bae-db. The bugs found on the
   way are being fixed now, each with a failing test. Then one definition for
   each duplicated rule, files moved by domain, about 60 tests moved beside
   their subjects, and narrower visibility.
2. **bae-bridge,** about 42 findings, split the same way: its handle code
   mixes library, playback, queue and downloads in one file today.
3. **The domain map.** bae's domains from the code (library records, import,
   the import list, identification, the playback queue, preferences, views,
   transfers, people and history, sync, and whatever else the data says),
   each with its home, tables, triggers and owner, written into the
   checker's map. The same map decides where each domain's schema lives: its
   tables and triggers move into SQL files in its home.
4. **The other crates.** bae-library, bae-playback, bae-catalogs, bae-db,
   bae-identify and bae-views haven't had a domain audit yet. The map shows
   where each mixes owners; each then gets the same four steps.
5. **coven,** whose 75 tables inside coven-database still sit in one coarse
   domain, gets one writer per table group.

> TODO: numbers worth adding once there are results: findings per audit,
> commits per cut, bugs found by cutting, lines moved, and the checker's
> finding count at the moment it switched on.
