# bae

bae is a music library and player for a household. It plays the household's
own files, describes them with what MusicBrainz and Discogs say about them, and
keeps one library on every device the household owns. This document explains
what bae does and why, for someone deciding whether the design is right and for
someone changing it.

bae keeps its data in SQLite and syncs it through
[coven](https://github.com/bae-fm/coven). This document does not repeat how
coven works. Where bae relies on coven's rules, it links to the section of
[coven's spec](https://github.com/bae-fm/coven/blob/main/spec/coven.md) that
explains them.

The examples follow one household. Ana, Ben and Carol share a library called
Household, stored in a Dropbox folder. Each has a laptop and a phone. Dan joins
later. Their records:

- **Ana's CD rip.** A 1979 CD of *Album Title* by *Artist Name*, ripped to one
  FLAC file per track with a CUE sheet and a rip log. The back of the case
  prints the barcode 0 12345 67890 5 and the catalog number LBL-1001.
- **Ben's download.** The same album, bought as MP3s from a download store.
- **Carol's box.** *Box Set Title*, two CDs, each ripped to one FLAC disc
  image with its CUE sheet, in folders `CD1/` and `CD2/`.
- **Ben's vinyl rip.** A 2×LP vinyl rip of *Double Album Title*, at 96 kHz, one
  FLAC per track, sides A to D.

Names of artists, albums and songs in this document are placeholders.

## Contents

- [1. What bae is](#1-what-bae-is)
- [2. Library](#2-library)
- [3. Import](#3-import)
- [4. Files](#4-files)
- [5. People](#5-people)
- [6. Sync](#6-sync)
- [7. Playback](#7-playback)
- [8. Servers](#8-servers)
- [9. API](#9-api)
- [10. Bridge](#10-bridge)
- [11. Crates and conventions](#11-crates-and-conventions)
- [Appendix A. Schema](#appendix-a-schema)
- [Appendix B. Synced tables](#appendix-b-synced-tables)
- [Appendix C. Settings](#appendix-c-settings)
- [Appendix D. Subsonic endpoints](#appendix-d-subsonic-endpoints)
- [Appendix E. MCP tools](#appendix-e-mcp-tools)
- [Appendix F. Bridge types](#appendix-f-bridge-types)
- [Appendix G. Errors and states](#appendix-g-errors-and-states)
- [Appendix H. Catalogs and vocabularies](#appendix-h-catalogs-and-vocabularies)

---

## 1. What bae is

A household's music is scattered. Ana's rips sit on her laptop, Ben's downloads
on his, and Carol's box set on an external drive. Tags are wrong or missing.
Nobody can play another's records, and nothing says that Ana's FLAC rip and
Ben's MP3s are the same album.

bae solves this in four steps:

- **It plays the household's own music.** bae is a music library and player for
  a household, on macOS, iOS, Android, Windows and Linux. It plays the files
  the household already has; it sells and streams nothing.
- **It looks folders up.** bae can look up a folder of audio files on
  MusicBrainz and Discogs. A match links the album, its edition and its artists
  to their entries there. Those entries link on to other linked data, such as
  Wikidata, so a release in bae can name its page on many catalogs at once.
- **It keeps the files where they are.** Ana's FLAC files stay in her music
  folder. bae reads them, measures them and plays them; it never copies, moves
  or rewrites them (§3, §4).
- **It syncs the library.** The library syncs between the household's devices
  using coven, through cloud storage the household already pays for. There is
  no bae server (§6).

### 1.1 Desktops and phones

Not every device does every job. A desktop finds, identifies and imports
releases. A phone browses, downloads and plays them.

| | Desktop (macOS; Windows and Linux) | Phone (Android; iOS) |
|---|---|---|
| Watch folders and import | yes | no |
| Identify on MusicBrainz and Discogs | yes | no |
| Edit a release's metadata | yes | no |
| Browse, search, play | yes | yes |
| Pin a release for offline play | yes | yes |
| Cast to AirPlay, Chromecast, UPnP | yes | yes |
| Subsonic and MCP servers | yes | no |
| Export and save files | yes | no |

The phone builds of bae-core leave the import pipeline, identification,
catalog lookups and metadata editing out entirely (§11.2).

The macOS app is the full desktop app. On Windows and Linux, bae-avalonia
holds the desktop app's start-up, update and packaging machinery and one
window; the desktop views are ported from the macOS app later. Opening it
while it runs hands the launch to the running copy; a launch that can't hand
over, logged, takes over when that copy is gone.

The Android app is the full phone app. On iOS, bae-ios holds the phone app's
start-up and release machinery and one screen; the phone views are rebuilt
from the Android app later. Its launch starts telemetry, crash reporting,
bae's directory, the host and the keychain, in that order. The screen shows
the sheep, the bae wordmark, and either the step that stopped the launch with
its error, or the libraries on this device by name, or why they couldn't be
read. It opens no library, so nothing in the phone column runs there yet.

### 1.2 Two editions

bae builds in two editions from one source:

- **bae** signs in to Dropbox and OneDrive, uses iCloud on Apple
  devices, and sends typed telemetry (§11.2).
- **baeium** uses S3-compatible storage only, sends no telemetry, and needs
  nothing proprietary to build or run. On iOS it works with a free personal
  signing identity; on Android it reads barcodes with ZXing.

The difference is a set of build features (§10.3). Nothing at run time decides
which edition a device is.

---

## 2. Library

A library has to say what the household owns, in a shape every device agrees
on. Ana's CD rip and Ben's MP3s are one album but two sets of files. A box set
is one release in two folders. A disc image is one file holding eleven tracks.
This section describes the library's model; Appendix A lists its tables.

### 2.1 Albums and releases

An *album* groups releases. A *release* is one pressing whose audio the library
holds: a particular CD, LP or download, with its own year, labels, catalog
numbers, barcodes, country, media, status and packaging. A pressing can be
printed with more than one barcode (an EAN and a UPC): it keeps every one its
source states, in order, each product once, and the editor shows one row per
barcode, each removable, with a row to add another, as it does labels. A saved
CUE sheet's `CATALOG` line states the barcode when every one names the same
product, and none otherwise.

Ana imports her 1979 CD. bae makes an album, *Album Title*, and under it a
release with the CD's facts. Ben imports his MP3s. His files are different
contents of the same album, so they become a second release under the same
album:

```
album    Album Title, Artist Name
  release  1979 CD, LBL-1001, 0123456789012   (Ana's FLAC rip)
  release  download                           (Ben's MP3s)
```

Every import is its own release, so the library can hold one pressing twice:
two folders of the same CD are two releases under one album (§2.7).

An album's *primary* release is the one whose cover it shows and that opens
and plays by default. It is not a stored pointer: each release holds a place in
its album's order (`releases.album_order`), and the primary is the release
first in that order, ties broken by release id. So every album with a release
has exactly one primary on every device, and two devices that each made a
different release primary agree on one after they sync. A release joining an
album goes after the album's others, so an album's first release stays its
primary. "Set as Primary Release" moves a release before the album's others,
and is refused for a release of another album. Moving or deleting a release
writes nothing else: the next release in the order is then the primary.

An album credited to the various-artists artist alone is a *compilation*, and
its tracks show their own artists. The flag is decided from the album's
credits at every write of them, an import's or an edit's, so it never outlives
an edit of the artists.

**The album grid** shows every album holding a release, newest first unless
the person sorts it by title (ignoring ASCII case), artist (§2.3), year or date
added, either way; an album of unknown year sorts after every year ascending
and before them descending, and albums the sort ties go by id. Grouped by
artist, the grid lists an album under each artist its credits show, once per
artist, the artists in order of sort name (else name) and then id, each
artist's albums in the grid's sort. The artist list shows every artist an
album's credits show, sorted by name or album count, ties going by name and
then id.

A release has tracks, a cover, and the files its tracks play from. Its pressing
facts are typed values in bae's own vocabulary (Appendix H), read from what the
catalogs state, so a surface shows one shape whichever catalog stated them.

### 2.2 Tracks, files and segments

A track plays from *segments* of files, so one disc image plays as many tracks.

Carol's first disc is one FLAC image with a CUE sheet listing eleven tracks.
bae stores the image once, as one file of the release, and gives each track a
segment of it: a start and end sample. Ana's per-track FLAC files are the
simple case: each track has one segment covering its whole file.

A track may have two segments: its *pregap*, then its main body. A pregap is the
audio before a track's index 01 on a CD, such as a count-in or applause. It can
be stored audio in the same file or the file before, or silence a CUE `PREGAP`
line asks bae to generate. §7.4 says how playback uses it.

A release holds its *mediums*: the discs, records or tapes its audio was read
from, in order, each with its carrier where the source states one and its
number of sides (a CD is one side, a record as many as its tracks' positions
name). They are what the person has, not every medium the pressing is made
of: Carol's CD rip of a box of records and CDs holds the CDs alone, while the
pressing's media still say what the box is.

Each track records:

- its playing order in the release, counted from zero across every side;
- its place: the medium it sits on, the side of that medium, and its number
  on that side. Every track has one, written when a source is applied and
  never edited (§3.6). Its label is a display of the place: a track on a
  record or cassette reads by its side's letter, the letters counting the
  sides of the release's records and cassettes in order (A … Z, then AA, AB,
  …), so a 2×LP rip reads A1 … D5; a track on any other medium reads by its
  number, under its disc when the release holds more than one medium (disc =
  the medium's position among the release's), so the box's CD rip reads
  "Disc 2 · 3". Playback pauses between sides and discs by it (§7.3);
- its length, measured from its audio at import (§3.7). Every track has one,
  so every surface that lists tracks (the album page, search, the queue, the
  release editor, a Subsonic song) shows each track's length, and none has a
  case for a track without one;
- its audio format: content type, sample rate, bits per sample, channels;
- its loudness, measured at import (§3.7).

A track's sample rate and channel count are above zero, and its bit depth is
absent (lossy audio) or 1 to 32; the database refuses any other value. Import
refuses audio that states a deeper one, naming the file. A stored value that
breaks these rules fails reading the track with an error naming it.

Each file records what the scan read of it: its content type, duration, sample
rate, bit depth or bitrate, channels, and whether it held one track or a
CUE-described disc. Lossless codecs state a bit depth; lossy ones a bitrate.

The files of a release include everything in its folder, not only the audio:
the CUE sheet, the rip log, scans of the booklet. A release's folder comes back
whole on export (§4.6).

### 2.3 Artists and credits

Artists credit albums, releases, tracks and works:

| Credit | Table | What it says |
|---|---|---|
| An album's artists | `album_artists` | who the album is by, in credit order |
| A track's artists | `track_artists` | who the track is by, in credit order; a track with none is by the album's artists |
| A release's roles | `release_artist_roles` | who did what on the release, as the catalog stated it |
| A track's roles | `track_artist_roles` | who did what on the track, as the catalog stated it |
| A work's artists | `work_artists` | who wrote or arranged the work |

An artist row holds a name, a sort name, and the artist's MusicBrainz and
Discogs ids where known. It may carry one picture, bae's own copy (§4.1).

**An album's and a track's artists are each one ordered list.** No credit is
the album's "own" artist with the others added: the first credit is position
0 of the same list. Each place in a list is its own row, so a list that names
one artist twice keeps both credits. Every reader takes the whole list:

- An album's line names every credit in order (see "Credit lines" below).
- Library search finds an album by its title or by any of its credits' names
  (an artist's own name or the other name the credit prints). A query matches
  anywhere inside a title or a name, ignoring case by Unicode's lowercase
  (accents still count: `ecrin` doesn't find `Écrin`), and each kind of result
  comes in title or name order, ties going by id. A track's search
  row names the track's own credits, else its album's.
- Sorting albums by artist compares every credit's sort name (else its name)
  in list order, so an album by A comes before one by A and B, which comes
  before one by A and C. The storage table's artist column sorts the same way.
- The artist list and an artist's page count an album for every artist its
  list names, once per artist.

**Credit lines.** A credit prints a name and, when another credit follows it,
what the source prints between them: MusicBrainz's join phrase (" feat. ",
" & "), Discogs's `join` (a comma as ", ", any other word or sign with a space
either side). A credit that states nothing prints ", "; the last credit's
join is never printed, and is not kept. One builder makes every line from
those values: the album page and its track rows, the grid, the queue and now
playing (and what AirPlay and DLNA devices are sent), search results, the
import list and catalog picker rows, the identification title search,
Subsonic's `artist`, and Save As's ARTIST (the track's line) and ALBUMARTIST
(the album's). A track's line is its own credits, else its album's. Each
credit still names its artist, so the artist's page lists the album and
Subsonic lists every credited artist in `artists` (its one `artistId` is the
first credit's).

**An artist's other names.** `artists.name` is the name an artist goes by.
The other names a credit prints for it, a MusicBrainz credit name or a
Discogs name variation (`anv`) that is not the artist's own, are the
artist's `artist_credits`, keyed by the artist and the folded name, so two
devices meeting one printed name write one row. A credit links one of them,
or none to print the artist's own name; it is never text. Importing decides
it once the artist is: a printed name that folds to the artist's own links
none, any other links that name, added if new. A merged artist's credit
prints the stored name it links, else the survivor's name. Credits with no
catalog id match an artist by its own name or any of its other names (the one
it matches is the name the credit prints). The artist page lists the names,
and renames or removes each in one write with every credit linking it:
renamed, they print the new name; removed, the artist's own. The artist field
of an editor lists each found artist with its other names under it; picking
one prints the artist by that name. That search is live for as long as the
field is open: one read, moved to each text typed and to the part of the list
on screen, so an artist added, renamed or given another name meanwhile shows
as it is. Typing a name, or picking an artist for a tag credit that spelled it
differently, prints the artist's own name.

Two artists can be merged into one, and only one way: on the import page, when
an import's credit shows that two library artists are the same person and the
person says so (§3.7). That is the one place bae shows both artists beside the
evidence that they are one; bae has no other merge action.

Merging records that one artist was absorbed into the other; both rows stay, so
a credit another device gave the absorbed artist still resolves. Every read
shows the survivor. A chain of merges shows its last artist, and a loop, left by
two devices merging a pair in opposite directions, shows the artist with the
smallest id. The artist each artist shows as is kept on the device
(`artist_shown_as`), written in the same write as the artist or the merge that
decides it, whichever device made that write.

**Which artist a credit names.** One rule decides, inside the transaction that
writes the credit, so nothing landing in between can make it wrong:

1. A credit carrying a MusicBrainz or Discogs id is the library artist holding
   that id, followed through any merge.
2. Otherwise it is the one library artist whose folded name matches and whose
   ids don't contradict the credit. The folding ignores case, accents and
   spacing.
3. Several such artists, or none, make a new artist.

A credit that matches an artist fills in that artist's missing ids and sort
name.

**Edge cases.**

- **Two ids, two artists.** A credit names MusicBrainz artist X and Discogs
  artist Y, and the library holds X and Y as separate artists. The write is
  refused, and the import page shows the conflict for the person to settle
  (§3.7). bae never settles it on its own.
- **A contradicting id.** A credit whose name matches an artist that holds a
  different id of the same catalog is refused outright.

### 2.4 Works

Works are compositions. Tracks link to them, in the order a track performs
them, and they nest: a movement is part of a symphony, in the symphony's order.
Works come from MusicBrainz, which names them; a work's artists come with it.

**The composer list** shows every artist credited on a work, and every artist
holding a catalog role credit that none of their works links: a release's
credit is linked when a track of the release performs one of their works, a
track's when the track performs one. A merged artist's works and credits count
under the artist it shows as. Each row counts the artist's works, the releases
a track of which performs one of them, and the unlinked credits; the list sorts
by name, works or releases, ties going by name, then id. An artist's composer
page lists those works and unlinked credits.

### 2.5 Catalog records

A release keeps one *record* per catalog that describes it. A record names
either the pressing itself or only its album:

- MusicBrainz and Discogs records usually name the pressing, with the album
  they file it under: a MusicBrainz release group, a Discogs master.
- Other catalogs, such as Wikidata or a streaming service, are reached by
  links. Their records name the album's page.

A release's list of records names the applied one first: the record its
draft was read from, or the pressing its import linked. Nothing reads a
record's catalog again after the import: the library holds every value, and
editing a release changes those values (§3.8).

### 2.6 The same fact gets the same id

Ana and Ben import while apart, each on a laptop with no network. Both
imports name the artist *Artist Name*, known to MusicBrainz. When the laptops
next sync, the library must hold one artist, not two.

So the same fact gets the same id on every device. Artists, works and albums
take their ids from their catalog ids: each is a name-based UUID over a text
key that spells the fact.

| Row | Id from |
|---|---|
| artist | its MusicBrainz artist id, or failing that its Discogs artist id |
| work | its MusicBrainz work id |
| album | its MusicBrainz release group, or failing that its Discogs master |
| album credit | the album and the place in its list |
| track credit | the track and the place in its list |
| work credit | the work, the artist and the position |
| work part | the parent and child works |
| release record | the release and the catalog |
| cover | the release |
| artist image | the artist |

coven merges two devices' rows that share a table and key, so two devices
stating one fact write one row
([coven §9.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#92-keys-and-unique-values)).

**Edge cases.**

- **An artist known only by name** has nothing that identifies it across
  devices, so it gets a fresh id. If a later credit gives it a catalog id, it
  keeps the id it has; row ids are never rewritten.
- **An album with no group.** A release whose metadata came from its tags names
  no group, so its album gets a fresh id.
- **A second catalog's group.** A Discogs import whose record pairs with the
  MusicBrainz group of an album already in the library joins that album
  (§3.7); where the two records name two library albums, the one the draft
  was read from decides.
- **Two devices editing one credit list.** A credit row is a place in the
  list, so two devices writing the same place write one row, and the artist it
  names merges like any other cell. One device shortening a list while another
  lengthens it can leave a gap in the positions; readers order by position, so
  a gap changes nothing shown.
- **A credit name removed while another device links it.** Removing an
  artist's other name unlinks every credit linking it in the same write. A
  credit another device linked to it meanwhile names a row that is gone, and
  coven takes that credit out (recorded as lost, back once relinked), so no
  device shows a credit printing a name its artist no longer holds.

### 2.7 A release's identity

Every import is its own release. It gets a fresh random id, its own tracks and
its own files: rows of its own and, once uploaded, copies of its own in
storage. Two releases never share a track or a file row.

Ana imports her 1979 CD rip. Her backup drive holds a copy of the same rip,
whose modification times the copy changed, and she imports that too. The album
now shows two releases, 1979 CD and 1979 CD, and each behaves as a separate
copy everywhere, local or uploaded. Ben's MP3s, different contents of the same
album, are a third release under it. Ratings, history and playlist entries
point at one copy's tracks and stay on that copy.

A release is never keyed by its contents: one keyed so would make a second
import replace the first, and with it the tracks that ratings, history and
playlists point at. What another device imported refuses nothing here.

**The folder's hash.** A hash of a folder's contents covers every file of the
release: each one's path relative to the release folder, its size and its
modification time, sorted, so moving or renaming the folder doesn't change its
hash. Each device records, for itself alone, the hash of the files each of its
imports read (`local_release_origin`); the record never syncs, and the release
carries no hash. It does one job: a folder whose hash it holds is Done in this
device's import list (§3.2), wherever the folder sits now.

One hash is one release on a device. An import of files this device already
imported is refused when it is asked for, and nothing runs; the person is told
this device already imported those files. Two imports of the same files asked
for before either lands both run, and the one that writes second finds the hash
taken: it fails with the same words and writes nothing. The release imported
first stands. Deleting the release deletes the record with it, and the folder
is back in Found.

The hash reads no bytes, so it tells files apart by name, size and time alone.
A copy to another disk usually changes the modification times, so the same rip
copied elsewhere usually hashes differently and is a folder of its own to
import, as Ana's backup is. A copy that keeps them hashes alike and is Done
beside the folder it was copied from.

A scan tells a file it has read by the same facts, with its path on disk in
place of the relative one: a file whose path, size and modification time are
the ones the last scan stored is the file that scan read (§3.1).

Tags edited after an import are not read again: the library holds the values
the import wrote, and the release editor changes them (§3.8).

The hash covers files, never decisions. Carol tells bae which disc each CUE
sheet is; that is stored under the candidate's hash, and a hash that moved with
it would lose the very decision it keys. No decision removes a file from a
release.

---
## 3. Import

Importing turns a folder on disk into a release in the library. The hard parts
are knowing where one release ends and the next begins, working out which
pressing the folder is, and never losing the person's decisions along the way.
This section follows Ana's CD rip from her music folder to her library: the
scan (§3.1), the import list (§3.2), the clues in the files (§3.3), the lookups
(§3.4), picking a pressing (§3.5), the draft (§3.6), and the import itself
(§3.7). Import runs on desktops only.

### 3.1 Watched folders and the scan

The person watches folders; bae scans them and finds releases, a folder each,
or folders read as one.

Ana watches `~/Music`. bae reads several of the folders directly in it at
once, each a directory at a time, and reports candidates as it finds them: a
folder's releases arrive as that folder is read, not in the order the folders'
names sort in. It takes those folders newest first, every scan alike, so a
folder Ana just added, or one whose own contents just changed (an album added
inside an artist's folder), shows up before the rest of `~/Music` is read: by
the latest of the dates the system keeps for the folder — when it was added to
`~/Music` (macOS only, and not on a network share), created, or modified.
Folders with no date go last, and folders with the same date go in name order.
Copies keep their old dates on some systems, so the order is a guess; it
changes which candidates show first, never what the scan finds. A *candidate*
is a release the scan found, not yet imported:

- **A folder with audio directly in it** is one release. Ana's
  `Artist Name - Album Title (1979)/` with its eleven FLAC files, CUE sheet and
  log is one candidate.
- **A folder whose children hold the audio** is read one of two ways: as one
  release, or as separate releases. Carol's `Box Set Title/` holds `CD1/` and
  `CD2/`. Every part's name carries a number, the numbers are exactly 1 and 2,
  and each part holds one folder of audio — its own tracks, or one folder of
  tracks below it such as `CD1/FLAC/` — so the scan reads the folder as one
  release with two discs. The discs play in the order their numbers give, not
  the order their names sort in: `CD2/` before `CD10/`, and
  `1936 - CD 1/` before `1935 - CD 2/`.
- **Anything else** stays separate: a part with no number, a gap in the
  numbers, numbers that don't start at one, a part holding several folders of
  audio, or audio sitting in the folder beside its parts. Dave's
  `Artist Collection/` holds `01 Originals/`, with three albums in it, and
  `02 Remasters/`, with one: they number themselves 1 and 2, but the first is
  three releases, not a disc, so the collection is four releases.
- **A part holds audio.** A folder with none is not a part, whatever its name.
  A `Scans/` folder beside `CD1/` and `CD2/` travels with the release. A
  numbered `CD3/` with nothing in it is no third disc; with only `CD1/` holding
  audio the folder yields one release and has nothing to read one way or the
  other.

The scan's reading is a guess, stored as the folder's *grouping*, marked as the
scan's. The person can flip it: read a folder as one release, or keep its parts
apart. Their answer is marked as theirs, and no later scan overrides it. The
person can also combine releases from anywhere into one, in an order they
choose; such a grouping reads exactly those folders.

Files under a folder that no release there owns, such as a cover beside two
discs kept as separate releases, are a *sidecar*. At most one release reads a
folder's sidecar.

**Watching.** Each watched folder has its own filesystem watch. A change reads
again only the folder directly under the watched root that it reached, and
only after that folder has been quiet for a second, so a download writing into
one album for an hour holds back only that album.

A watched folder on a network volume is watched like any other, but a network
mount reports only the changes this machine makes. bae also checks such a
folder every 15 minutes, which is what notices a change made on the server or
from another machine, and the import list says so.

**What a scan stores.** Everything a scan reads is device-local: the
candidates, their files and the role each plays (audio, track sheet, artwork,
damaged image, document, other), the parsed CUE sheets, the tags each audio file carried, and
when each folder was first seen. A scan writes as it finds; completing removes
what it didn't see, in the same transaction that marks it complete. A failed or
interrupted scan keeps both old and new entries. Each write reads only the
entry it writes and the entries reading the same files, which it finds by their
folders, so a write costs the same however much else the watched folder holds.

**What a scan reads.** A scan reads each file once. It opens a track twice:
FFmpeg's probe, which says whether it plays, and one read of its tags, which
places it in play order, names a broken folder, and seeds its release's draft,
picture included. A folder read again within the same scan, such as disc
folders read as one release after each was read on its own, reads none of its
files again.

A rescan, such as the one every desktop launch makes of each watched folder,
takes what the last scan of the folder stored for each file whose path, size
and modification time are unchanged (§2.7): a track's stream and tags, whether
an image decodes, what a CUE sheet parses to, and what a broken folder's tags
call it. It opens only the files that are new or changed, so a rescan of a
folder nothing in has changed opens none. A release's draft takes each
unchanged track's tags, and the picture they embed, from the reading stored
for the release, and reads only its new or changed tracks. A store that says
two different things about one file fails the scan.

**Edge cases.**

- **A broken folder.** A folder with corrupt or zero-byte audio, or no valid
  audio at all, is listed as skipped with its reason. It can't be imported.
  The scan reads every file before judging it, so it stores each file of the
  folder with what it made of it — each damaged track with why: it is empty,
  or holds no audio bae can read — and the album title and artist its first
  playable track's tags state.
- **A damaged image** — zero bytes, or bytes that don't decode as the picture
  its name says — doesn't skip the folder: the release is its audio. The image
  is never artwork, so it is never the cover, and it imports as plain bytes the
  library never shows as a picture. It is still carried, so it comes back on
  export.
- **A CUE sheet that disagrees with the disk** — naming absent audio, failing
  to parse, or naming a codec bae can't play from one file — is left unbound,
  and the folder stays importable.
- **A watched folder inside another.** Watching a folder that contains watched
  folders takes them over. What is known about a folder is keyed by its path on
  disk, so the person's groupings survive. Unwatching the folder that covers a
  grouping deletes it.
- **A dismissed candidate** is skipped by path, so a later scan doesn't offer it
  again.

### 3.2 The import list

The import list shows every candidate in one of three tabs:

| Tab | Holds |
|---|---|
| Pending | this device imported no release from the folder's files, and not skipped; including a failed import |
| Done | this device imported a release from the folder's files (§2.7); what is still uploading first |
| Skipped | dismissed by the person, or broken |

Done runs the releases coven still has files of in its upload queue first,
the first queued first, and then the rest in the list's order. Only a
release's own files count, as the Storage Manager counts them (§4.2); the
list reads the queue as coven's state view `coven_uploads`
holds it, so it is never waiting on it and never out of step with it.

Each tab sorts newest first, oldest first, or by path either way. A date order
sorts by each row's date — its folder's for Pending and Skipped, its release's
import for Done — with undated rows last; a path order sorts by watched folder
and then by path, in natural order ("Disc 2" before "Disc 10", case aside).
On Pending a release nested below a folder is listed under that folder's
group, and so is one at that folder while another is nested below it: the
group sorts by its newest member's date and its folder, its header leads its
members, and folding the group shut keeps the header, still counting its
members, and leaves the members out of the list. Each list keeps its folds,
and folding or opening every group is one change. A header offers to read its
folder as one release (§3.1) when a stored reading of the folder keeps its
releases apart, or, with nothing stored, when the folder is the nearest one
above some release that holds two releases or more, counting the releases of
every tab. While the rows on top are a group's, that group's header stays
pinned over the top of the list; over a release in no group no header is
pinned. Each member is read with where its group's header sits in the list,
so the pinned header is its own group's however far above it the header is
and whichever rows have loaded.

Each Pending row is in exactly one state. The first that applies wins:

1. **Importing**: an import is queued or running.
2. **Identifying**: a lookup is queued, running, or writing an answer that
   picks nothing unattended. A lookup that picked its pressing unattended
   (§3.5) puts the row in **Identified** while bae reads that pressing in
   full; the row offers Import then too (§3.7). A row shows a lookup at
   work, waiting for its run or running, only while it is Identifying, so
   one whose pick is being read shows none, as Identified counts it.
3. **Error**: bae could not store a lookup's answer.
4. **Import error**: the last import failed, or the release can't be worked on
   as it stands.
5. **Identified**: linked to a catalog pressing, or to an album whose pressing
   is unknown (§3.5).
6. **Not looked up**: no lookup is stored for the folder's files.
7. **Unmatched**: the person kept their own draft over what the lookup offered.
8. **Error**: bae broke on its own side and the stored lookup ended there.
9. **Lookup error**: a catalog could not answer, or could not hand over a
   release it found in full.
10. **Unmatched**: the lookup took its release for the folder and the person
    set it aside.
11. **Needs you**: the lookup left the answer to the person.

A filter narrows Pending to one group of these states: needs you, in progress,
identified, unmatched, or not looked up. Each entry counts the Pending rows it
holds, whatever the list shows. Text typed into the search keeps the rows
whose shown title or artists contain it, case folded as library search folds
it (§2.3), and a group's header while any of its members is kept; the list
says how many of the tab's rows it keeps of how many it has.

**Done.** A folder is Done when this device imported a release from its files.
This device's record of the import is keyed by the folder's hash (§2.7), not
its path, so a folder moved or renamed after its import stays Done. A Done row
looks exactly like the row the folder had in Found — the same cover, title and
artists, drawn the same way — and says nothing more but its release's upload:
the tab says it is done. While files of the release wait in this device's
upload queue, the line under its title names the upload's phase, as the
Storage Manager does (§4.2): Uploading while any file is being sent, beside a
bar of the bytes of the release's whole size stored or sent, or Queued with no
bar while its files only wait. The row reads how many of the release's files
wait and how many are being sent in the list's own read, from the counts the
Storage Manager reads, so the row changes as each file starts and lands; the
bar's bytes arrive apart, in the release's value of the upload transfer stream
the Storage Manager's chip reads. It names the one release imported from those
files, which Show in Library and a double-click on the row open.

A folder this device didn't import is in Found like any other, whatever another
device imported and whichever library releases came from the same files. Ben
copies Ana's rip folder to his laptop, keeping the modification times. Ana's
release syncs to him, but his scan finds the folder in Found, his to import or
skip.

Done offers no re-import. A second copy comes from importing another folder,
such as a copy whose modification times changed.

**Skipped.** A broken folder's row looks like a Found row: the folder's own
cover when an image in it decodes, the album title and artist its tags state
or else the folder's name, and under them why it can't be imported, such as
"2 audio files are damaged". Selecting it opens it the way a Found release
opens, read-only: its images, then every other file with each damaged one
marked and why. Where a release's Import would be, the pane states the reason.
A playable track in it can still be auditioned.

The person can select any number of rows. Each list a surface opens holds
its own selection beside its view and its folds, in core, not in the surface,
and it only ever holds rows the list shows: the write that stops showing a
selected row — another tab, filter or search, a fold, or a change that moves
the row out of the filter, such as one that starts importing under Identified
— takes it out of the selection in that same write, and a row shown again
comes back unselected. A write that stores a candidate's state, such as an
edit in its pane or a pick, changes only the rows whose values change, each in
place, so no step of it shows the row as something it isn't, such as unlinked
on the way to linked again: typing into the selected row's pane keeps the row
selected and the pane open, under any view that still shows it. A release the scan reads again with changed files is
the same row: it stays selected, under a group as folded as it was. The pane a selection opens reads only that selection,
so with nothing shown selected it shows nothing selected. A bulk action runs
in core over the selected rows that offer it, in the order the list shows
them, and answers with the rows it failed on; how many it has done moves
with each row, so it reaches the app only in a `KeyedValueStream` (§10.2)
the list hands out, holding it while the action runs. A broken folder offers
no action, so selected among others it counts
toward the selection but holds back combining it.

Another tab, filter, search or order is a view the list has not read yet:
until the read reflecting it lands, the list shows no rows rather than the
previous view's, nor "No matches" for them.

A list narrowed anew — another tab, filter or search — shows on top the row
its view opened at: its first selected row as it sat when the view was set,
or its top while no selected row is shown. The write that sets the view
works that place out and keeps it on the list, so no read of the list looks
for the selected row; selecting rows since moves no place, and the list
otherwise keeps the person's place as rows come and go.

Revealing a candidate — the first one still being identified, or one the
person asks for — shows it on the tab its placement puts it on, with the
filters cleared and its group opened, the list's other folds kept.

Everything the list shows is read from tables, one read for its rows, its
counts and its selection: each release's tab, state, group, dates, shown text
and what its commands are decided from are kept beside it as it changes
(Appendix A.7), so the list reads the rows it shows and not the whole queue.
A row reads none of its folder's files, so a folder on a slow or unreachable
volume does not hold up the list, and a linked row reads only the records its
stored releases name, not the releases whole.
Its counts — each tab's, each filter entry's, how many items the list holds
and how many releases its filters keep, and how many each group's header
counts — are kept beside the placement as rows move, so reading them costs
the same however long the queue is. A search counts the rows its index finds:
each release's shown text is kept, case folded, in a trigram index beside its
placement (Appendix A.7), so a search of three characters or more reads the
rows holding it, and a shorter one reads every release's text from the
index. A search that finds few rows reads its windows from them, sorted;
one that finds many reads its windows in the list's order, testing each row.
A list folding groups shut, when the folded members outnumber half of what it
shows, reads its windows past them, so with every group folded a window costs
about the headers it shows; and whether a
header offers to read its folder as one release is kept on the header as the
rows below it and the readings stored for it change.
The pane reads its one candidate's rows by the candidate's key, and so does
every read of one candidate's state, draft or preparation, so how long one
takes does not grow with how many candidates the queue holds.

### 3.3 Clues in the files

Identification reads clues from the files, then looks them up. A clue is one
of:

- **Disc IDs**, one per disc the folder's rips describe: each rip log whose
  table of contents reads, and each CUE sheet with no such log beside it, in
  the same folder. A sheet beside a log describes the disc the log already
  hashed, so it isn't hashed again; a disc two files hash alike counts once.
  bae hashes the track offsets the way MusicBrainz does; it never reads a
  drive. Carol's box, a CUE sheet per disc, yields two Disc IDs.
- **Barcodes**, decoded from the bars on the artwork, read from the digits
  printed under them, or taken from a CUE sheet's `CATALOG` line.
- **Catalog numbers**, read from the folder's text: the CUE sheet, the
  artwork, the folder and file names, text files, and the tags.
- **ISRCs**, the recording codes in the audio's tags.
- **The tags**: album title, artists and label become lines of the folder's
  text; each file's title, or its file name without the number, is the track's
  title.

Reading the artwork means reading text off images. On macOS this uses the
system's Vision framework; a platform with no image reader reads no artwork.

Ana's folder yields a Disc ID from her CUE sheet, the barcode from her scan of
the case, LBL-1001 from the folder's text, and eleven ISRCs. The fast clues
come first, so the disc-ID lookup need not wait for the artwork to be read.

**Where the audio came from.** The files can prove a medium:

- **A CD rip**: a rip log, an AccurateRip report, or a CUE sheet a known ripper
  signed.
- **A download**: what a store writes into the files it sells, or the set of
  tags a label delivers with every track (an ISRC, a copyright line and the
  label), where nothing says a disc was read.
- **Not a CD**: lossless audio at a sample rate no CD plays.

Ben's vinyl rip is at 96 kHz, so it is not a CD. A CUE sheet alone proves
nothing, since vinyl rips have them too, and a folder at 44.1 kHz could come
from anything.

A proven medium sets aside any pressing whose stated media could not have
produced the audio (§3.5). A folder proven not to be a CD has its CUE sheet
left unhashed: a Disc ID of a vinyl rip names no disc.

### 3.4 Looking up

bae looks the clues up on MusicBrainz and Discogs. Each catalog answers for
itself and at its own pace:

| Clue | Asked of |
|---|---|
| each Disc ID | MusicBrainz |
| ISRCs | MusicBrainz |
| barcodes | both |
| catalog numbers | both: a number the person picked as it stands; one a found release carries as it stands on MusicBrainz, under the offered releases' artists on Discogs |
| album and artist title | both, only when every clue above came back empty |

Discogs needs an API key; without one it is not asked. Either catalog can be
switched off. MusicBrainz is asked no more than once every 1.2 seconds, under
the rate it allows.

**Rounds.** One lookup is not enough. Ana's Disc ID returns three CD
pressings; one of them carries LBL-1001, which the folder also states, so that
number is now worth asking Discogs about. A run therefore goes in rounds:

1. Pool every answer so far and rank it (below).
2. Read each offered release's document. A search result states less than
   its document: Discogs names only the first label, and no search lists the
   tracklist. A run reads only what ranking the release and joining its album
   read: the release's own page and its album's (the release group or
   master), and for a MusicBrainz release the Wikidata items and Discogs
   releases those two pages link, which its album statements (below) read. A
   Discogs release's run read asks MusicBrainz nothing. A release a Disc ID
   lookup returned is read from that answer, which lists its tracks, its
   mediums and their Disc IDs, its labels, barcode, links and release group,
   but not its annotation: until a pick reads it, its notes are its
   disambiguation alone. The release is stored naming every other document
   its pages link as one the read stopped short of, and its own page too
   where the read took it from a Disc ID answer; a pick, an unattended pick
   included, reads the rest, that page included, before it applies anything.
   Runs go four at a time. An unattended pick's read goes beside them, not in
   a run's place, so the next folder's run starts as soon as one picks.
   That complete read of a Discogs release asks MusicBrainz in one request
   which of its releases links the release's page and which of its release
   groups links the master's, since the release's page names its master; a
   page MusicBrainz knows nothing of is linked from none.
3. Ask what came into effect: a catalog number a found release carries that the
   folder states, searched on MusicBrainz as it stands and on Discogs under
   the artist of each release the run offers, whichever release carries the
   number; an offered release's pressing on the catalog that has no record
   of it, by the release its page names there, its barcode, or its catalog
   number under its label, and on Discogs the artist it credits. A short
   number is printed by hundreds of unrelated releases, which Discogs serves
   a page at a time, and the artist leaves the ones that share it;
   MusicBrainz answers a number in one page, so naming the artist there
   saves nothing. The artist is the album's, not each carrier's: a number's
   search turns up other artists' releases that reuse it, and searching
   under each of theirs would ask Discogs again for every one. An offered
   release credited to no one, or to various artists, names no artist, and
   the number is searched alone. While no release is offered, nothing names
   the album's artist: a number Discogs has not been asked about is searched
   alone, and one it has is not asked again. A confirmed number is never
   searched under its label: the other catalog may spell the label
   differently. A pressing whose number a search in effect already asked
   that catalog, as it stands or under the same artist, is not looked up by
   it again.
4. Stop when a round asks nothing new and every offered release is read.

**Pressings and albums.** The two catalogs describe the same object in their
own words. A record from each becomes one *pressing* when what they state
about the physical object agrees: the barcode with or without spaces, the
catalog number with or without a dash, the year, the country, the media. A
record that states nothing about one of them doesn't disagree; one stating a
different year does. Two records of one catalog stay two pressings, because
that catalog's editors separated them.

A pressing's records keep their own statements: neither fills in what the
other leaves out. Its row shows what a pick applies, its lead record's facts,
and carries each record's own. Picking it applies the picked record alone: the
draft's pressing facts and year are that record's, or the year the folder
names where it states none, and the other catalog's record is linked as a
record of the release and fills nothing of the pressing. The album a pressing
belongs to is the two records', so either's album documents speak for its
title, artists and year. Within one record, its album's year and the year the
album first came out are its own album page's (its release group or master);
the other catalog's release it links as itself gives them only where that page
states none. An album card names the label every pressing
naming one agrees on, and none where they name different labels.

Pressings group under albums. A MusicBrainz release group and a Discogs master
are one album only when a statement says so, never because their titles look
alike. Five statements count:

1. The release group's page, or the release's own, links the master.
2. That page links a Wikidata item, and the item states the master.
3. The release links a Discogs release, whose own document files it under the
   master.
4. A release of each, on the list, prints the same barcode, and their titles
   share a word.
5. They print the same catalog number under the same label, and their titles
   share a word.

**A release's mediums.** A MusicBrainz release lists its media, each with its
format, its own tracks, and the Disc IDs registered against it. Discogs lists one flat tracklist and states the
media as format entries with quantities, so bae splits the tracklist by its
positions:

- `2-5` is the fifth track of disc 2.
- `CD2-5`, `CD-5` and `BD-1` sit on a disc of the carrier the prefix names
  (CD, DVD, BD for Blu-ray, SACD, VHS, LD for LaserDisc): disc 2 of the CDs,
  or that carrier's only disc.
- `A3` is on side A of a record or cassette. A record holds two sides, the
  second the letter after the first, so a third side, or a letter that doesn't
  follow, starts the next record: a box whose every record restarts at A
  splits per record.
- A heading, or an index row printing no position, goes with the next row
  that names a medium (an index with its first sub-track's). A tracklist whose
  positions name no medium is one medium.

Each Discogs medium takes its carrier from the format entries, one unit of a
format per medium. A prefix names its own carrier. Lettered records take the
sided formats (vinyl, cassette, shellac) in format order, and the other discs
the remaining units in order. Where a group's mediums and units don't count
alike, the group takes the one carrier its units name, else none.

**Which of its mediums the folder holds.** A release is read against the
folder's audio once, and the run's documents, a pick, the draft it writes, the
shared album and a library re-identify all read that one reading. The mediums
it reads are a set whose tracks, laid out against the folder's audio — a
Discogs index or heading read as one track or as its sub-tracks — number
exactly as many as the folder's; a medium listing no tracks is in none.
Between such sets, in order:

1. the folder's Disc IDs: the set more of whose mediums MusicBrainz registers
   one of them against wins;
2. the carrier: audio proven a CD rip rules out a medium that plays no CD
   audio (a record, a Blu-ray), and audio at a rate no CD plays rules out a
   CD;
3. the lengths: fewer tracks whose stated length is more than three seconds
   off, then fewer tracks with no stated length, then the smaller total
   difference.

A CD rip of a box of records and CDs reads the CDs, and a record stating no
lengths never beats a CD whose lengths agree. Sets still tied that read the
same tracks, such as a hybrid SACD's two layers, are one answer. Sets that
read different tracks are not chosen between, and never by which comes first:
the release can't be applied, the error naming every disc of those sets, and
the person picks another release or uses the file metadata. A release no set
of whose mediums holds the folder's tracks is refused, naming the folder's
count and every track the release lists; one whose chosen tracks can't be read
(a track with no title, an album credited to no one) is no match.

**Ranking.** Each pressing is scored by what it states against what the folder
states. For each fact, a pressing agrees, says nothing, or disagrees; one that
says nothing never loses to one that disagrees. Which lookup found a pressing
adds no weight, with one exception: a Disc ID's lookup returns only releases
holding that disc, so a release returned for more of the folder's Disc IDs
holds more of its discs and ranks above one returned for fewer. The pressings
tied at the top are offered; the rest are set aside under "N more releases".

Two kinds of pressing never stand:

- **One whose tracklist doesn't fit.** A pressing whose read tracklists all
  list another number of tracks than the folder holds, or tracks that can't be
  read, is not a match. Lengths only choose which mediums are read; they never
  rule a pressing out: catalogs round them and count pregaps differently.
- **Look-alikes.** Pressings that would show exactly the same on screen are
  kept once; nobody could choose between them in bae. Whether one is already
  in the library plays no part: that is read live when a row shows, and what a
  run keeps must not depend on what the library held when it ran.

The last point of the ranking reads the catalogs' free-text notes. A word that
sets one pressing's notes apart, which the folder's text also states, ranks
that pressing first among otherwise equal ones.

A run's answers are kept with its verdict, so they re-rank without being
fetched again, and a fetched release is stored on the device (`source_release`
and its tables, Appendix A).

### 3.5 Picking a pressing

One matching pressing is picked automatically; otherwise the person picks,
searches, or edits.

A run *picks unattended* only when all of these hold:

- the folder's own files rule out none of the pressings found;
- it found exactly one pressing, two catalogs' records of it counting once;
- every record of that pressing was read in full;
- its tracklist lists as many tracks as the folder holds.

Ana's run finds one pressing, on both catalogs, read in full, eleven tracks.
bae picks it, and her row reads Identified at once. bae reads the pressing in
full beside the runs still going, then writes its metadata into her draft.
Until that read ends, her pane shows the pressing selected and being read; an
answer that picks nothing reads no pressing, and the pane shows none. A
Disc ID often returns several pressings of one album: an exact clue is not a
unique answer, and choosing between them is the person's.

When the run doesn't pick, the pane says why: nothing found, several
pressings, a record that couldn't be read, a tracklist that couldn't be
checked, or a medium the folder rules out. The person then:

- **picks** one of the offered pressings;
- **searches** by artist and album, catalog number or barcode, with each
  catalog's results landing as they come. Nothing more is read about a result
  until it is picked, so a statement joins two catalogs' albums on the
  search's cards only once a pick has read a MusicBrainz release's documents;
- **links the album** when several pressings of one album are offered and they
  can't tell which is theirs. The link names the album, not a pressing, and the
  fields every offered pressing agrees on fill the draft;
- **keeps their own draft**, from the folder's tags or typed by hand.

A pick applies its release to the draft at once (§3.6), an unattended one as
soon as its read in full ends. Nothing previews the release's tracklist first:
the draft the pick writes is what the pane shows.

The person also decides which clues a run asks about. Ben unchecks a barcode
that belongs to a box set rather than his disc; Carol picks the one catalog
number of thirty that names her pressing. Those choices are stored with the
candidate, and changing them starts the next run.

### 3.6 The draft

A *draft* is the metadata an import will write, as plain values: the album
fields, the pressing facts, the album's artists, the mediums the audio is on,
and one track per audio unit with its title, artists, place (medium, side and
number), and the length its source lists for it. It also holds what the source credits beyond the editable fields: each
track's composers and other roles, the works it performs and the works those
nest among, and the composers the source credits on the release as a whole. An *audio unit* is
what one track plays: a whole file, or one slice a bound CUE sheet carves out of
an image. Units add up: a disc image with two loose bonus tracks offers
thirteen.

**Track order.** The units play in the order the files' own numbers give
them. A file's disc and track numbers come from its tags (`DISCNUMBER` and
`TRACKNUMBER`, ID3 `TPOS` and `TRCK`, MP4 `disk` and `trkn`); a number its tags
don't state comes from its name, read as `07 Title` (track 7) or `2-07 Title`
(disc 2, track 7). Files go by disc, then track; a file stating no number goes
after those that do, and files the numbers don't tell apart go in name order.
A release read from several folders keeps each folder's files together, the
folders in name order. Ben's download names its files by title alone, but tags
`Track Title.mp3` as track 7, so it is the seventh unit: a draft read from the
tags numbers it 7, and a catalog's tracklist lays its seventh track on it.

**Mediums and places.** Applying a source writes the draft's mediums and each
track's place, and nothing edits them afterwards: the pane and the library
editor show a track's position and edit only its title and artists.

- A MusicBrainz release gives each medium it reads its format; a record or
  cassette has as many sides as its positions name letters, counted from its
  lowest, and one naming none is one side. A track's number is its position's
  digits (`A3` and `3` are 3).
- A Discogs release gives the mediums its tracklist splits into, each with its
  carrier (§3.4); a track's number is the digits after its side letter or
  after its last `-`, and a collapsed index takes its first row's.
- The files' tags, and nothing, give one medium per disc the folder states —
  a CUE sheet's disc, a folder of a release read from several, a file's disc
  number — each of unstated carrier and one side, the files stating no disc
  first; a track's number is its tag's or its name's, a CUE track's its
  sheet's.
- A track whose source states no number is one more than the tracks before it
  on its own side.

A file decision that gains a track, or moves a CUE sheet to another disc,
places that audio as the folder lays it out, adding a medium of unstated
carrier where the draft holds fewer; a medium no track sits on any more is
dropped. A file decision is kept under the files' hash, so it settles every
folder holding the same files in one write, each found by that hash.

**What the files' tags say.** The tags are read whole: a tag stating a value
more than once (two `ARTIST` comments, an ID3v2.4 frame of two values) states
each, so a file naming two artists gives its track two credits. An album
field is the value every file stating it gives; files that state it
differently give it the first file's, in play order (disc, then track). A
folder whose units are CUE sheets reads its album from the sheets' `TITLE`,
`PERFORMER` and `REM DATE` the same way, the first sheet's winning, and with
no title stated anywhere the folder's name is the title.

- The album title is the one every file states. Where they differ, it is the
  first file's without the disc marker it ends with and the separator before
  it: `Album Title (cd 1)`, `Album Title [disc 1]`, `Album Title CD1` and
  `Album Title - Disc 1` give `Album Title`. A marker is `disc`, `disk`, `cd`
  or `d` and a number, in brackets or not; `Album Title Volume 1` names no
  disc and stays as it is.
- The album artist is the `ALBUMARTIST` the files agree on, else the first
  file's; where no file names one, the `ARTIST` they agree on, else the first
  file's, so a compilation whose files neither flag it nor name an album
  artist is credited to its first track's artist. With no artist named
  anywhere the draft asks for one before it imports.
- A track whose `ARTIST` names the album's artists is credited to the album;
  one naming others is credited to them.
- A folder every file of which says it is a compilation, by its compilation
  flag (`COMPILATION`, ID3 `TCMP`, MP4 `cpil`) or by naming "Various Artists"
  or "Various" as its album artist, is credited to Various Artists, and each
  track keeps the artists it names.
- Each file's dates (`DATE`, `YEAR`, `RELEASEDATE`, `ORIGINALRELEASEDATE`)
  name its years by the rule identification reads a folder's name by: two or
  more years make the earliest the album's year and the latest the pressing's;
  one year is the album's, the pressing's left unknown. With the year the
  album first came out known (a record states it), that year is the album's
  and the latest later one the pressing's. The album's year and the
  pressing's are each the one the files agree on, else the first file's.

Applying a source — a catalog's release, the folder's tags, or nothing —
writes everything it says into the draft once. The draft points back at
nothing in the source: the import, the pane, and every later edit read the
draft alone, so a release its catalog's editors change after it was applied,
and fetched again, changes nothing the draft imports. A release that lists no
tracks fills the album fields and leaves each track as it was. A file decision
(§3.1) keeps each retained track, with every value it holds; a track the folder
gains takes its title from its tags and has no roles, works or listed length,
since no source said anything about it, and the draft keeps only the works its
tracks still perform.

Every draft records who wrote it: nobody yet, the folder's tags, an
identification, or the person. Every pane control is stored the moment it's
used, under the candidate's hash, so the person can leave a candidate half
done and come back to it, even after a restart. Where the pane was, such as a
half-typed search, is stored too. Since each edit is written as it is made,
the pane has no reset and no undo: to start over, the person applies a source
again (a catalog release, or the files' tags) or clears the metadata, and the
file decisions stay as they are.

The mapping table pairs each unit with its draft track, showing the length the
track's source listed beside the folder's own, and marking the two when they
disagree by more than three seconds. Where the draft has several discs or
record sides, each is headed once, however many runs of rows it holds: the
loose files on it, and under its name each CUE sheet's entries on it (a sheet
the release splits across sides names itself again on the next side). For a
CUE sheet, the person can bind each `FILE` line to an audio file and say which
disc each sheet is, or ignore a sheet. The table also counts the tracks nobody
has named yet, and the pane states that count beside Import without blocking
it: importing titles each of them after its file.

Before importing, the person can play a file through a second player, which
pauses the library's playback and resumes it when it ends.

### 3.7 Importing

Importing measures loudness and adds the release to the library. Audio stays
where it is; bae never copies, moves or rewrites it.

Ana clicks Import. bae:

1. **Checks the files** are the ones the scan saw: same paths, sizes and
   modification times. A folder that changed since is refused, to be read
   again.
2. **Fetches the new artists' pictures**: Discogs' picture of each Discogs
   artist the release credits that the library doesn't hold yet. An artist
   the library holds keeps its own picture, so none is fetched for it. A
   picture that can't be fetched fails the import with why, and importing
   again fetches it again. While it fetches, the import's progress says
   "Fetching artist pictures…"; an import that creates no Discogs artist
   fetches nothing and its progress never says so.
3. **Reads each file** where it sits, for a cloud import: coven hashes it, and
   checks every later download against that hash. A local import records each
   file's path, size and modification time; its bytes are read when the release
   is uploaded (§4.2).
4. **Measures loudness** by decoding every track: its EBU R128 integrated
   loudness and true peak, and the album's over all its tracks. A near-silent
   track stores no loudness and plays unchanged. The same decode records
   *scrub points* for a FLAC whose seektable leaves FFmpeg somewhere to seek
   more than 10 seconds of audio short of its target: none, one of only
   placeholder points, or one whose points lie more than 10 seconds apart,
   or start or stop more than 10 seconds from the file's start or end. For
   each segment of a track, the frame it starts in, then the first frame
   each 10 seconds of audio on, or sooner where the bytes from the last point would outgrow the window a
   scrub fetches (§7.6). A segment's last frame is never a point: a decode
   that jumps to a FLAC's last frame can't tell which sample it holds, so the
   frame before it stands in. Each point is a frame's first sample and the
   byte it starts at, read from a decode that began on a frame. Every MP3 gets
   scrub points too: its frames state no position, and each depends on the
   frames before it. For the first frame each 10 seconds of audio past the
   file's first, its point is the latest earlier frame a decode can start at
   and still play it exactly, with the first sample such a decode plays
   exactly; the decode read the whole file from its first frame. Any other
   file gets none.
5. **Checks the audio decodes**, in the same pass. A track that fails, or
   decodes short, fails the import rather than failing later at play time.
   This check is a setting, on by default.
6. **Finds the album.** The album the person picked on the pane, if any,
   else the album the release's records name: the record the draft was read
   from decides, then the other catalog's record — the first naming a
   library album by a copy of its pressing (the import is another copy, §2.7)
   or by its album (release group, master). Library albums are the
   curator's: two albums the records name stay two, and no question is
   asked. Where one record names several, its group's own album wins if the
   library holds it, else the oldest. With neither, the album is new and
   takes its id from its group (§2.6). The pane shows which: "New album", the
   album the records name, or the one picked, and its album picker lists any
   library album to pick instead; a picked album deleted meanwhile takes its
   pick with it. Re-identifying a library release files it the same way.
   The "already in your library" banner names every album holding the
   release, or another release of its album: those holding the release
   first, then oldest first. One read answers it for every release the
   pane offers, and follows the library while the pane is open; an identify
   verdict or search result, in flight or stored, holds no library status of
   its own.
7. **Writes everything in one transaction**, from the draft as it stands, read
   from no source again: the album; the release, with a fresh random id, and
   this device's record of the folder's hash it read (§2.7); its tracks, files, segments, scrub points, credits, roles, works, records,
   cover, and the new artists' pictures. Each track row is made once its audio is bound, with the
   length that audio measures, so no track is ever stored without one. A track
   left untitled takes its file's name. Each credit, role
   and composer is its own artist credit until that transaction decides which
   library artist it is (§2.3), so two credits naming one artist write one.

Nothing durable is written before the last step, so a cancel before it leaves
the library as it was. Once the write begins, a cancel is refused and the
import completes.

**The cover.** The cover comes from the folder's own image files, an image
embedded in a tag, or a catalog's image (the Cover Art Archive for
MusicBrainz, or Discogs). Applying a catalog's release stores, with the
draft, the covers it offers the picker: every claimed release's own images,
then their albums'. The first is the draft's cover until the person picks
another. A draft read from the tags, typed in, or linked to an album offers no
catalog's covers. A catalog's cover is fetched with the draft, before the
import runs, so the import never asks a catalog for it. The artists' pictures
are not part of the draft: nothing before the import shows them, and many
drafts are never imported, so a pick or an edit fetches none and the import
fetches them (step 2). Every stored image is downscaled to at most 600 pixels
on a side and stored as JPEG.

**Where it goes.** Each import lands on this device or goes to the cloud, as
the person last chose (§4.2), and a cloud import stays whole here too once
stored when the Pinned choice is on. A cloud import stages its files with
coven for the cloud in the same transaction as its rows, which queues their
upload, so the choice is never lost, and needs no connection to record it.

**Automatic identification and import.** By default, each release a scan finds
for the first time is identified on its own, in the background, behind
anything a person asks for. With "Import automatically when identified" on,
any run that picks unattended imports straight away: one bae started on its
own, or one the person started with Identify, a retry, or a changed clue. A
pressing the person picks themselves, from a run's list or their own search,
waits for their Import. Both are settings (Appendix C), and each works without
the other: with automatic identification off, the runs a person starts still
import. Turning import on imports nothing already identified.

Import asked for while bae reads a run's unattended pick, from the row, the
pane or a selection, leaves the read going and doesn't import the draft as it
stands: it waits for the pick to be stored and then imports, as automatic
import does, with the setting on or off. The row then no longer offers Import.
A read that stores no pick, because it failed or was cancelled, imports
nothing.

**Edge cases.**

- **Two ids, two artists.** Carol imports a release crediting *Artist Name*
  with a Discogs id and a MusicBrainz id. The library already holds two
  artists of that name: A, from a Discogs import, holds the Discogs id; B,
  from a MusicBrainz import, holds the MusicBrainz id. The import fails with
  the conflict, and the import page offers five answers:

  | Answer | The credit names |
  |---|---|
  | Same artist | A and B, merged into one (§2.3) |
  | It's A | A; the credit's MusicBrainz id is ignored |
  | It's B | B; the credit's Discogs id is ignored |
  | Neither | a new artist carrying neither id |
  | Change the pressing or match | whatever the new match credits (§3.5) |

  bae never picks for her: either catalog's id could be the wrong one, and a
  wrong guess credits the album to another artist on every device. Her answer
  is stored with the candidate like any pane control (§3.6), and the
  candidate is ready again.
- **Identifying a library release again.** A release already in the library
  can be identified again from its own files; its Disc IDs come from the rip
  logs and CUE sheets this device holds of it, and its track lengths from its
  stored tracks. The person searches from the release editor (§3.8) and picks
  its pressing, which is applied to the release's edit session at once, with
  no confirm step, and the editor shows what it gave. A pressing whose tracks
  don't lay out on the stored ones is refused with both counts, and one
  several of whose disc sets fit them alike is refused naming those discs.
  The editor on every app states that refusal in its own line (G.1), as the
  import does, never as a generic import failure, and MCP's re-identify
  returns the refusal's own text.
- **One import at a time.** Imports run one after another on one worker.

### 3.8 Editing a library release

One editor edits a library release: the album detail's release menu opens it
with one *Edit…* item, and the import pane of an imported folder shows the
same editor for each release the folder became. It has the parts of the
import pane's draft, laid out the same way: the cover beside the album and
pressing fields, the album the release files under, the track table, and the
same *Identify* controls — *Automatic* identifies the release from its files
and *Search* opens the typed search — whose results replace the draft in
place until a pick applies one, or *Back* returns to it. *Save* writes the
session; *Cancel*, shown once something changed, discards it.

A library release is edited in a *session* the editor opens on it: its form
holds the release's values as the library holds them. Applying a catalog
release in the session reads it once, against the release's stored track
lengths and its files, and replaces every value the release holds — the
album fields, the pressing, each track's title, artists, medium, side and
number, the roles and works the catalog credits, the release's records (the
applied one first) and the album those name — none of which is written yet;
the cover stays the release's own. Saving writes the whole session in one
transaction: the edited fields, and where a catalog was applied, its every
value too, so nothing a previous source credited survives. The release lands
in the album its records name (§3.7), or in a library album the person
picked, which keeps its own title and artists. The editor names that album
with the import pane's album picker: the album the release is in, the one an
applied catalog's records name, or a new album where they name none the
library holds, until the person picks one; a pick holds over a catalog
applied after it. The cover is not part of the session: a cover picked in the
editor is written at once, and the session keeps what was applied and picked.
Cancelling discards the session; nothing is re-read, and there is no reset to
a source: the library holds no pointer back into one. A release with nothing
applied saves its edited fields alone, its mediums, roles and works as stored.

Every editor opens a session of its own, so the editor sheet and the import
pane open on the same release, or MCP's re-identify while either is open,
never share an apply or a pick. A save first checks the release against what
its session opened on: if the library no longer holds it so — another
session saved it, MCP wrote its fields, another device's edit arrived, or an
artist it credits was renamed — the save is refused, writes nothing, and the
editor says the release changed after it opened; *Cancel* opens it again as
it is now. Only the cover, which is written at once, never makes a session
out of date. No other save or field edit on the device runs between a save's
check and its write. After a save, the editor works in a fresh
session opened on what it saved.

The editor lays the tracks out as the import's mapping table does: a heading
for each disc or record side where the release has several, and two or more
consecutive tracks a CUE sheet carves from one file under that file's name,
once; every other track names its own file in its row. An applied catalog
lays them out again at the places it gives them.

---
## 4. Files

A library is metadata plus files. The metadata is small and syncs whole. The
files are large, belong partly to the person and partly to bae, and some
devices can't hold them all. This section says which files are whose (§4.1),
how a release reaches other devices (§4.2), how a device keeps them (§4.3), how
a release comes back to one device (§4.4), how files sit in storage (§4.5),
and how files go back out to disk (§4.6). coven stores and moves the files;
its [§17](https://github.com/bae-fm/coven/blob/main/spec/coven.md#17-files)
explains how.

### 4.1 Whose files

Two kinds of file, matching coven's
[two kinds](https://github.com/bae-fm/coven/blob/main/spec/coven.md#171-two-kinds-of-file):

- **Audio: the household's own files.** Every file of a release's folder,
  audio or not, is the person's file at its path. bae and coven record its
  path, size and modification time and never change, move or delete it.
- **Covers, artist images and avatars: bae's own.** bae makes them, each
  downscaled to at most 600 pixels as JPEG, and hands the bytes to coven,
  which keeps them.

A cover belongs to one release, an artist image to one artist, an avatar to one
person. A row references its image by the id of a coven file, the record coven
keeps of the bytes
([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)):
the `file` column of `covers` and `artist_images`, `avatar_id` of `people`. A
coven file never changes its bytes, so changing a cover stages a new file and
points the release's cover row at it in one write; the old file goes with its
last reference, and coven deletes its stored bytes once nothing refers to them
([coven §17.9](https://github.com/bae-fm/coven/blob/main/spec/coven.md#179-deleting-stored-files)).
When two artists merge (§2.3) and only the absorbed one has an image, the
survivor's row references the same file: nothing is uploaded again.

The apps and Subsonic clients cache art by an image's version, which must
move whenever the bytes a device can show for it do. A row references its new
file at once, and every device receives it with the row, but another device
can read the file only once its upload completes: until then coven has it on
the uploading device only. So the version is the file's id, followed by its
content hash once coven records it as stored. The uploading device's version
moves with its write; another device's moves when the write arrives, and again
when the upload completes. Until then that device shows the image it had, or
none.

### 4.2 Local and uploaded releases

A release is *local*, or *uploaded* so other devices can play it.

Ana imports her CD rip to her laptop only. The release is local: its files are
her own, at their path on her laptop. Its rows sync like any release's, so
Ben's phone shows the album, its tracks and its cover, but can't play it. Ana
moves it to the cloud. bae stages each file with coven for the cloud, and coven
uploads it from where it sits; once every file is in storage every device can
play it. Ana's files stay where they are, untouched.

**Where a file plays from.** A `release_files` row references its file by the
id of a coven file
([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)),
or by none while its release is local. Each device records the originals it
holds in the device-local table `local_release_files`, one row per file with
its path and modification time: the person's file it imported, or the copy a
detach made (§4.4). These paths never leave the device. So a device reads each
release file by one rule:

- the row references a coven file: through coven, which reads the original on
  the device moving it to the cloud, and the cache or storage once it is
  stored;
- otherwise, this device holds an original: from that original;
- otherwise the file isn't here. Another device holds it, keeping it local or
  still moving it to the cloud, and this device can't tell which.

A release is *uploaded* once every one of its files' rows references a coven
file stored in the cloud. Until then it is local, on every device. The rows
sync anyway, so every device knows what the household owns, and a rating or
playlist entry pointing at a local release's track points at a row every
device can read
([coven §15.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#153-references-stay-readable)).
Whether a file is stored changes in coven's own record of it, with no change
to a row of bae's, so the release's listing follows coven's `coven_files`
(Appendix B).

A local release's cover still uploads, so other devices can show it. coven
queues it as soon as the cover row is written, with or without storage
connected. That upload is bae's own file, not the release's: until its audio
moves, the release reads as local everywhere, and nothing about it reads as
uploading. Waiting files draw no progress bar; a bar appears only while a
file's bytes are moving.

**Moving to the cloud.** Moving a release to the cloud stages every one of its
files for the cloud in one write, each read whole once, and points each row at
its coven file. The files of that write are one move: coven uploads each, and
once the last is stored one write of coven's switches them all to the cloud,
changing no row of bae's
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)).
So other devices see the release local until its last file lands, and never
half uploaded. The `local_release_files` rows stay; until the move finishes
coven reads each file from its original, so this device keeps playing the
release while it uploads, and the upload list shows it uploading. Before that
write, bae checks each file still has the size and modification time it
recorded; a file that changed since import refuses the move. The rows describe
the file as imported, and a changed file would give every device audio that
doesn't match them. The stored Pinned choice (Appendix C) asks coven to keep
each file whole on this device once the move finishes: coven pins it itself and
downloads its copy from storage, never reading the original for the cache, and
retries a failed download by itself.

If an original changes after the write and before its upload, the move fails:
none of its files switches, and the pending list names the file and offers
Cancel Upload, which ends the move and makes the release local here again.

**Making a release local.** Two actions take a release's audio back off its
coven files. Each takes every file of the release off its coven file in one
write, so the release plays from this device's disk the moment the write
lands, and coven forgets each file with its last reference:

- **Cancelling an upload** while any of its files is still queued, or after
  its move failed. coven retires each upload. The release plays from its
  originals again, which must still be unchanged, unless its move failed
  because one changed.
- **Making a release local again** (§4.4) copies its files into a folder the
  person picks, then, in the same write that takes them off their coven files,
  records the copies in `local_release_files`.

Other devices again show the release but can't play it. What already reached
storage stays until nothing refers to it; coven then deletes it
([coven §17.9](https://github.com/bae-fm/coven/blob/main/spec/coven.md#179-deleting-stored-files)).

- **A cloud import** (§3.7) stages its files for the cloud in the same write as
  its rows, kept here once stored when the Pinned choice says so, and records
  their originals as a local import does.
- **Uploads queue.** Files go up three at a time by default, a setting from
  one to eight. The queue can be paused, and stays paused across restarts; a
  paused upload keeps its place.

**The Storage Manager** lists every release with its album's title and
credits, its media, its files' count and total size, whether it is uploaded,
and whether this device keeps all its files pinned (§4.3). It shows every
release, or only the local ones, or only the uploaded ones, and shows the
shown list's count and total size. It sorts by album title (ignoring ASCII
case), artist (§2.3), media (a release stating none last), file count, size,
or storage — uploading, then pinning: files coming to this device, waiting
or downloading in the download queue (§4.3), then local, pinned, and in the
cloud only — either way, ties going by album title,
then when the release was added, then id. The upload queue and the pins are
coven's, which the list's query reads beside the library's rows. Whether a
release is pinned, here and in the album and release details, is read from
how many of its files coven's `coven_pins` lists, whole on this device, a
count bae keeps per release in the transaction that changes a pin, so each
view's own query follows a pin as it lands. coven lists a pin only once its
whole copy is kept, whether the download queue pinned the file or coven keeps
it here after a move. Whether a release is uploaded is kept the same way, from
coven's `coven_files`, in the transaction that records a file, switches it to
the cloud or deletes it, on this device or as another device's write applies.
How many of a release's files
wait in the upload queue, and the place of the first, are counts bae keeps
the same way from
coven's `coven_upload_queue`, in the transaction that queues or dequeues a
file; the import list's Done tab reads them in queue order, and each Done row
its own release's (§3.2). Whether
the download queue holds a release waiting or downloading is a flag bae keeps
the same way, in the transaction that changes the queue, and where the release
sorts by storage is worked out from these in the same write, so the storage
sort reads an index like every other.

A release with files waiting to upload shows, in its row and in its album's
storage band, how many wait and how many are being sent (its Done row in the
import list shows the phase that makes, §3.2), and while any is
being sent, how many bytes of its whole size are stored or sent. The counts
are read in the same query as the row: which files are being sent comes
from coven's `coven_sending`, a live view of the uploads in flight. How far
the bytes have got is not in any table: coven's upload watch tells, file by
file, how much of each file's encrypted object has gone to the connection,
whether the file goes in parts or in one request. A release's bytes are its
stored files' sizes and, of each file being sent, that share of its size,
beside the stored bytes a live query of the files being sent reads; the
files being sent together count their objects' bytes. Each file's share is
told by file too, for the Storage Manager's rings. How fast they go is
measured beside the database from the same watch's bytes, file by file, as
each step of them goes, so a file sent in one request has a rate too: a
rate for each release, its files being sent together, and for every file
being sent together; no screen shows one file's rate, so none is sent. A
file that only waits has no rate, and a pause ends every measurement. While its files wait, or a transfer runs,
the release offers no storage action but cancelling, which would race it
otherwise; moving a release to the cloud shows that transfer until its files
are queued.

Each row's storage state is the place it sorts by, which core reads with the
row: Uploading, its chip filled as far as its bytes have come while any file
is being sent; Pinning, filled as far as its download has come once it
starts; Local, Pinned or Cloud at rest, or, while another transition this
process runs leaves the files where they are, that transition's words. No
rate shows beside it.

Under the list, beside its count and total size, each transfer queue
holding files waiting or on the move shows while it does: uploads, pins
(§4.3), and exports and saves (§4.6), each with how fast it goes, an export
with how far its release has been written, and a control that pauses or
resumes the whole queue. A queue holding only failed entries shows nothing.
The list's own query reads which queues hold work, whether each is paused,
and the release a pin or export queue runs, from their tables beside the
rows; how fast and how far arrive apart, keyed by that release.

### 4.3 Pinning and the cache

An uploaded release can be pinned to stay on a device; otherwise its files are
cached as they play.

Ben's phone holds little. When he plays Ana's album, the chunks he plays are
fetched and kept in the cache. Before a flight he pins it: the phone downloads
every file whole and keeps it regardless of the cache's size.

Each kind of file has its own cache budget on each device, so audio never
pushes the covers out:

| Kind | Budget |
|---|---|
| Audio and the folder's other files | 20 GiB |
| Covers | 512 MiB |
| Artist images | 256 MiB |
| Avatars | 64 MiB |

A file downloads when it is first read: a cover when it is first shown, a
track as it plays. Over budget, the least recently used go first. Covers are
never pinned; an evicted one is fetched again the next time it is shown.

Pins download one release at a time from a queue that can be paused,
cancelled and retried. A pin that fails stays in the queue with its error;
retrying queues every failed pin again, each in its place, so a release's own
control retries them all, as the queue's does. coven's record of the failed
download is listed among the pending records (§6.4), whose *Retry* queues the
release's pin again; nothing retries it on its own. A read that fails, as a
track plays, leaves no record: its failure goes to whatever read it. A release
becomes pinned only once every file lands. The
queue is kept on the device and emptied as the library opens: after a restart
it is empty and running, and a release that wasn't fully pinned stays as it
was. How far the release downloading has got is not kept, nor how fast it
goes. Its bytes are the sizes of its files kept whole and, of each file
`coven_downloading` lists, the share of its size its object has moved, as
coven's download watch tells it, so they move as a file arrives rather than
as each lands; a share counts against the file's own size, since the
watch's bytes are the encrypted object's. Its rate is what it has moved
since it began over the time since. Unpinning moves the files into the
cache, still readable, now evictable.

Asking to pin several releases queues, in the order asked and in one write,
each that can be: one already waiting or downloading, pinned, local, or
deleted meanwhile is passed over, as the releases' listing says, and one whose
pin failed is queued again in its place, as retrying queues it. If the write fails, nothing is
queued; the person sees the error and the failure is reported as a bug.

### 4.4 Making a release local again

Making an uploaded release local again copies its files into a folder the
person picks.

Carol wants her box set off the cloud. She picks a folder on her drive. Every
file is copied there from the cache or storage, and each copy's SHA-256 is
checked against its coven file's content hash. Then one write takes every file
off its coven file and records its new path (§4.2). A cancel or failure before that write removes the
partial copies and leaves the release uploaded. The cover keeps its coven file:
it is bae's own, and every device shows it.

The file names come from the release's rows, which another device may have
written. Each is checked to be a plain relative path before it is joined to
Carol's folder, so no row can make a device write outside the folder chosen.
One bad name copies out nothing at all.

### 4.5 How files sit in storage

Storage holds encrypted objects under random names. Ana's FLAC files are not a
folder anyone can browse in Dropbox: each is one object, encrypted with a key
of its own that travels only inside the encrypted writes of its coven file's
record, at a path coven fixes when the file is staged
([coven §17.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#174-how-a-file-is-stored)).
The provider sees no album, artist or file name.

The path is fixed when the file is staged and kept in its coven file's record,
so renaming an album never moves a file. A new cover is a new object with a new id. Two
devices replacing one cover then never write the same object, and a device
applying an older write still finds the bytes that write names.

### 4.6 Export and save

Two ways put a release back on disk, from desktop:

- **Export** copies a release's files byte for byte into a folder named after
  its source folder. The copy is staged and moved into place only once every
  file is written.
- **Save** writes new files with a *preset*: a codec, a filename pattern, where
  pregaps go, and whether to embed the cover. Each file's track number is the
  track's number on its side, and its disc number the track's medium's
  position, written only for a release of several mediums. A lossless preset writes the
  16, 24 or 32 bits it names, whatever depth the source has. A preset can
  write a release as one image with a CUE sheet. FLAC and MP3 presets come
  built in (Appendix C).

Both run from a queue that can be paused and cancelled, kept and emptied as
the pin queue is (§4.3); a failed one stays in it, with its error, until it is
cancelled. A save keeps the preset it was queued
with, so editing the preset changes nothing already queued. Either reads files
from wherever they are: the person's own path, the cache, or storage.

**Edge cases.**

- **Deleting a release** deletes its rows and coven's copies of its files. The
  person's own files are never deleted.
- **A device with no cloud home** has only local releases; moving to the cloud
  needs one.
- **What disconnecting would cost.** bae counts the uploaded releases this
  device hasn't pinned: those it could no longer play without the cloud. The
  count reads an index of the releases whose pinned count falls short of
  their file count.

---

## 5. People

A household is several people sharing one library. Each wants to rate albums,
keep a history, make playlists, and see who did what, without a server to ask
who is who. This section describes how bae maps people onto coven's members
and circles. coven's
[Part III](https://github.com/bae-fm/coven/blob/main/spec/coven.md#part-iii-people)
explains members, devices, admins, circles and owner tables themselves.

### 5.1 One library, one household

A household shares one library, its files and its metadata. Ana's CD rip,
once uploaded, is everyone's to play.

Each person is a coven member with their own devices. Admins add and remove
people. Each person adds their own devices, and removes their own; an admin can
remove any person's device too (§5.6).

**The library's name.** A library has one name, its coven store's name, and an
admin can rename it. Ana renames Household to Home; every device shows Home
once it syncs the rename. Renaming is an admin entry in coven, so only admins
can do it: the name is the household's, not one person's.

The name lives in the store. A restore code carries no name: it is kept for
years, written down, and a name inside it would go stale with the first
rename. An invite carries the current name, since it expires within a week.
The iCloud Keychain entry coven keeps a restore code in also saves the name,
as one of the person's devices last saw it, so a new device's list can say
which library each code is for (§5.6). It is only a label there: a rename made
while none of the person's devices had the library open leaves the old name
until one opens it, and the restore finds the library by its id, never its
name. Once a device has loaded the library, it shows the store's name.

### 5.2 Everyone edits the library

Every member can change the library. There is no play-only role, since coven
checks no permission on library writes, and a rule only the UI enforced would
not hold: any member's device could write around it.

Which edits a platform's UI offers is a separate choice. The mobile apps don't
gain library editing because their devices can write.

### 5.3 Each person's own data

Each person has a circle with only themselves in it, made when their profile is
(§5.4). Private rows live there, household rows in the store. bae makes no
other circles.

Each person's own data:

| Table | One row per | Where |
|---|---|---|
| `ratings` | person and album, or person and track | always household |
| `history` | play | private or household: one setting per person |
| `playlists` | playlist | each one private or household |
| `playlist_entries` | place in a playlist | where its playlist is |

**Ratings.** A rating is one to five stars, given by one person to an album or
to a track. Ana gives *Album Title* four stars; Ben gives it two. They are two
rows, one per person, keyed by the person and the album, so Ana rating it again
from her phone changes her one row. Clearing a rating deletes the row. A
track's rating belongs to one release's track, so the tracks of two copies of a
pressing (§2.7) are rated apart.

Ben keeps his history private. Carol's devices never see what he played. He
makes a playlist for a party and shares it: it moves from his circle to the
store, with its entries, and everyone sees it. A move between the store and a
circle is an ordinary write in coven
([§15.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#154-moving-rows)).

**History.** Ben plays track 3 of Ana's album for two minutes: one `history`
row records Ben, the track, the release, when the play started, and how much
of it he heard. He skips track 4 after ten seconds: nothing is recorded. A play
counts once 30 seconds of the track, or half of it, whichever comes first, has
played; a 40-second interlude counts after 20. A skip isn't a listen.

How much he heard counts only audio that played: not pauses, not waiting for
the network, not what a seek jumped over. The row is written when the play
counts and updated as he keeps listening. Its start time is the playing
device's clock when the play began, stored as UTC.

bae records plays and syncs them by their audience; no screen, and nothing
the bridge offers, lists them.

Each play is its own row, with a fresh id. A play count kept in one cell would
lose plays: two devices adding to it at once merge cell by cell, and the later
write wins
([coven §8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#8-merging)).

**Changing the history setting** applies to new plays only. A play takes its
audience from the setting when it begins, and keeps it. Ben switches his history
to household on Friday; what he played before Friday stays in his circle.
Moving old plays would publish what he played in private, and switching back
couldn't take from the household what its devices have already read. A new
profile starts private.

**Plays before a profile.** A device can play before its person has a profile:
Ana's new library has no cloud home yet, so she is no member and has no circle
(§5.4). Her plays are kept in the device-local table `local_history`. When her
profile is created, the same write moves every one of them into her private
circle. They never go to the household on their own: she hadn't chosen to share
them.

**Playlists.** A playlist belongs to the person who made it, and is private or
household, each playlist on its own. Its entries are rows keyed by the playlist
and a *position id*, a fresh id per entry, so the same track can sit in a
playlist twice. Each entry also has a position number that orders it.
Playlists list by name, and profiles list by name too.

Ben's phone and laptop each add a track to his party playlist while offline.
Both take the next position, 7. When they sync, both entries stay; equal
positions order by position id, so every device shows the same order. A
reorder names every entry it read, and is refused if an entry arrived or went
in between, so it never drops an entry it didn't see.

### 5.4 Profiles

coven owns identity: members, roles and devices. bae keeps a synced `people`
table, in the store, holding each person's name, avatar, private circle and
history setting.

- **A person is keyed by their coven `MemberId`**, which is stable: replacing
  keys, or re-inviting someone who lost every device, keeps the same id
  ([coven §13.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#133-losing-everything)).
- **Ratings, history and playlists point at the person**, not the member.
- **The joining person's own device writes their `people` row** as part of
  joining, with the name the admin typed in the invite, so every member has a
  profile. They can rename themselves later. Someone
  re-invited after losing every device keeps their row, since their
  `MemberId` is the same. If the write fails, the person has joined anyway,
  and Settings offers to create the profile again.
- **The person who made the library** becomes a member when it gets a cloud
  home. Until then there is no member to credit. Settings then asks for their
  name and creates their profile.
- **Making a profile makes the person's circle**, the one only they are in
  (§5.3), and moves any plays waiting in `local_history` into it.
- **A removed person invited again is a new member.** coven gives them a fresh
  `MemberId`, and they start with an empty new profile. Their old `people`
  row, ratings and household playlists stay under the old identity, read-only:
  they are owner rows (§5.5), and the old identity has no device left to
  change them.
- **An avatar is a file on the `people` row**, bae's own (§4.1): the app
  hands bae the image the person picked, and bae stores it downscaled to at
  most 600 pixels as JPEG, as it does a cover, or refuses an image it can't
  decode. Other devices download it when it is first shown.
- **The people list joins coven's members with `people`** in one query:
  everyone in the library, in member-id order, each with their profile once
  they made one, and the person using this device read the same way. A
  removed person's row stays, read-only, but they are no one in the list.

### 5.5 "Ana rated this"

Rows credited to a person are in owner tables: `ratings`, `history`,
`playlists`, `playlist_entries`, and the person's `people` row. Only their
author's devices
can change them
([coven §9.6](https://github.com/bae-fm/coven/blob/main/spec/coven.md#96-owner-tables)),
so "Ana rated this" is authentic.

This is authenticity, not a threat-model guarantee. The library itself stays
editable by every member.

### 5.6 Adding and removing

| Operation | Who | What bae does |
|---|---|---|
| Add a person | an admin, by invite ([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)) | the new person's device writes their `people` row as it joins |
| Add a device | the person, from the new device, with their restore code ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)) | nothing beyond coven |
| Remove a device | the person, for their own; an admin, for anyone's | nothing beyond coven |
| Remove a person | an admin | their circle goes, with its rows |
| Rename the library | an admin | every device shows the new name (§5.1) |

A new phone's first screen is bae's sheep and name over **Scan code** and
**Enter code**, with no other words: the code says what it is for. An invite
joins someone's library, and the person's own restore code adds this device to
theirs. A Mac's first screen offers **Join library** and **Add device**, which
both lead to the same scan-or-type screen where the code decides the same way,
and **Create new library**. A restore code's screens call that adding this
device: the button is **Add**, and the running restore says it is adding this
device. A code saved in iCloud Keychain, and a restore over this device's copy
(§6.4), say **Restore**. A scanned code starts at once, as an invite does, with
no screen to confirm it: the person scanned it to add this device, so the
running restore shows next. Only an S3 code without the key stops first, to ask
for it. A typed restore code shows where its library is stored and waits for
**Add**. A code that doesn't read says so on the code screen, and a restore
that fails shows its error beside **Add**, to try again.

Dan joins. In People, Ana taps **Invite** and enters "Dan", his Dropbox email
and a role, then picks **In person** or **Send**. Her laptop shares the store's
folder with his account and shows the invite. On S3, Ana makes an access key
for Dan instead of sharing with an account. Either way the invite carries the
library's current name and Dan's, and Dan's phone starts joining as soon as it
reads the code, with no form to fill in
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)):

- **In person:** Dan taps **Scan code** and scans Ana's screen. Ana's laptop
  lets him in as soon as it reads his request, and neither of them taps anything
  else. The invite expires after an hour.
- **Send:** Ana picks how long the invite lasts, an hour, a day or a week, and
  sends the code. Dan taps **Enter code** and pastes it. His waiting screen
  shows his confirmation number, a countdown to the next check, and **Check
  now**. When his request arrives, Ana's laptop shows a dialog with Dan's name,
  the same number, **Approve** and **Decline**. Ana asks Dan for his number and
  approves if it matches.

His phone then writes his `people` row with that name while it joins.
Everyone's devices show Dan by name.

**Open invites.** Until it ends, each invite Ana's laptop made is a row in
People: the person's name, marked invited, with when it expires. Only the
device that made it shows the row, since only that device can act on it. The
row's refresh button runs Retry Now for the library (§6.4), so the request
shows the moment Dan says he has entered the code, without waiting for the next
check. Opening the row shows the invite again, with **Cancel invite**. Closing
the request dialog keeps the request, and the row opens it again. Once Ana's
laptop has begun letting Dan in, cancelling fails and says Dan is already
joining, rather than leaving him half joined.

On S3 the invite's screen also shows the id of the key it carries, and says to
delete that key in the provider's console if nobody uses the invite: bae keeps
no record of it once the invite ends. Cancelling an invite asks first, and on
S3 the question names the key, as removing a person does.

**Dan's waiting.** Dan can close bae while he waits. When bae next starts, it
opens on his waiting screen and keeps checking, with no sign-in: coven keeps
his Dropbox or OneDrive sign-in on his phone beside the join
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)).
Only when coven has no sign-in it can use, because Dropbox refused the kept one
or it is gone, does bae show the sign-in it showed when he first joined, then
go on checking. Cancelling that sign-in leaves the join under way for the next
start. The waiting screen has no Cancel once his request is sent, since coven
can't take a sent request back: the join ends when Ana approves, declines or
cancels, or the invite expires. Before then, while Dan signs in to his
provider, Cancel works, and the invite stays open for him to scan again. When the invite ends without letting him in,
Dan's phone returns to the first screen and says it was declined, cancelled or
expired: all three delete his request, so his phone can't tell which.

Removing a person deletes their circle and its rows: their private history and
private playlists. Their household ratings, history and playlists stay,
credited to them.

On S3 each person's row in People shows the id of their access key. Removing
a person asks first; on S3 the question also says that bae will forget their
key and that Ana deletes it in her provider's console. bae keeps no record or
reminder of the key afterwards
([coven §14.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#143-s3-access-keys)).

People lists each person's active devices. Their removed and replaced
devices, whose records coven keeps, sit under them in a **Removed devices**
list that starts closed, each marked removed or replaced, with no actions.

**Adding a device.** Carol's new tablet joins with her restore code alone; no
other device approves it. The code holds Carol's keys, so it already proves the
tablet is hers
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)),
and an approval would strand a person whose other devices are all lost. The
tablet gets the code one of three ways:

- **scanning** the QR code another of her devices shows, drawn large enough
  for a phone's camera to read from a Mac's screen at 25 to 45 cm, and for a
  Mac's camera to read from a phone's at 20 to 25 cm, even when it carries
  her S3 key;
- **iCloud Keychain**, on Apple devices: the first screen lists the restore
  codes coven saved there for libraries not already on this device, and Carol
  picks one; a library the device already holds is opened, not restored. A
  code there is Carol's, not one device's: it stays while she is a member, so
  removing the library from one of her devices leaves it offered there (§6.4),
  and it goes when she leaves the library or deletes it from the list;
- **typing** it in.

The iCloud Keychain list shows each code under its library's name as Carol's
devices last saw it (§5.1), over where the library is stored: the provider and
account. Dropbox and OneDrive show the account's email; S3 shows the bucket and
endpoint; iCloud shows iCloud. Two libraries can share a name, and a rename
none of her devices has seen yet leaves the old one, so the location is what
tells them apart. The restore screen for a saved code shows both. A typed or
scanned code carries no name, so its screens show only where the library is
stored until the tablet has loaded it, then the library's current name.

**Deleting a saved code.** Carol once tried a library on S3, then deleted its
bucket, and her new tablet still offers it. Each code in the iCloud Keychain
list has **Delete**. It asks first, saying that it removes the saved code from
iCloud Keychain on all her Apple devices and doesn't delete the library, or
anything on this device or in her storage; then the code leaves the list. The
S3 key saved beside it goes with it. A delete that fails says so, keeps the
code listed, and deleting again finishes it. A later join, restore or reset on
one of her devices saves the code again
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

**A saved code bae can't read.** An entry in iCloud Keychain can be one bae
can't restore from: saved in an older format or damaged, holding a different
library's code than the one it's saved under, or saved more than once. The list
still shows it, as a row saying the code can't be used and why, with the
library's id beneath, since the entry has no name to show. It has **Delete**
only, which asks and deletes as for any saved code; there is nothing to
restore from. One such entry never hides the codes beside it. When one of
Carol's devices opens the library, or applies a rename to it, it replaces the
entry with the library's current name and its own current code, so the row
becomes an ordinary one; a device whose keys another device replaced has no
current code to save, and leaves the entry for her to delete
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

**The S3 key.** On S3 a restore code doesn't reach the bucket alone: the device
also needs Carol's S3 access key. The QR code one of her own devices shows is
her restore code with that key inside it, so scanning it is enough
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
The code to type holds no key, and asks for it separately.

**Where the code shows.** The apps call the restore code the *recovery code*.
Carol adds a device from **Add Device…** under her own row in People, shown
only for a library with a cloud home. Its sheet tells her to choose **Add
device** on the new device, then scan its QR code or type the code it shows;
on S3 it says the QR code carries her key. While it shows the code it watches
her devices, as People's list of them does: once a device of hers joins that
wasn't among them when the sheet made the code, the sheet says that device's
name was added in place of the code, and **Done** closes it. Her devices are
read before the code is made, so a device that restores from it is never
counted among them, whenever this device learns that it joined. While the
sheet watches, Carol's device syncs every 5 seconds instead of every 30, so it
says the new device was added within seconds of it joining. A quiet sync is two
requests to the cloud home, about 24 a minute, only while the sheet is open
([coven §6.7](https://github.com/bae-fm/coven/blob/main/spec/coven.md#67-waiting-and-retrying)).
Settings keeps
the code under **Your recovery code**, for the day she loses every device:
hidden until she asks, then shown in place as coven formats it, to copy or
hide, and hidden again when Settings closes. On S3 that section also says she will need her access
key and secret.

**Removing a device.** Ben's old phone is lost. He removes it from his laptop.
Had he lost every device, Ana, an admin, could remove it for him: admins can
remove any person's device, and everyone else only their own. Removing a device
shows what is left to do at the provider, such as signing that device out of
the account
([coven §14.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#142-provider-access)).
On S3 it shows nothing: a person's devices share one access key, so removing
one device changes nothing at the provider.
Removing this library from a device is the device leaving on its own; §6.4
covers it.

---

## 6. Sync

Each device keeps its own copy of the library and works offline. Sync brings
the copies together. bae decides what syncs and in what shape; coven moves and
merges it. This section says what syncs (§6.1), where it goes (§6.2), how bae
declares it (§6.3), and what a person sees when sync stops (§6.4).

### 6.1 What syncs and what stays

| Stays on the device | Syncs |
|---|---|
| Playback state: the queue and position (§7) | The library: albums, releases, tracks, works, artists, and their files' metadata |
| Import and scan state (§3.1–§3.6) | People (§5.4) |
| Fetched catalog data (§3.4) | Ratings, history and playlists (§5.3) |
| Settings (Appendix C) | |
| A local release's audio and its paths (§4.2) | |
| Plays from before the person had a profile (§5.3) | |

Each reason is the same: the data is one device's, and another device would be
wrong to follow it. Ana's queue is what she is listening to, not Ben. A scan is
of one machine's disk. A fetched catalog release can be fetched by any device
that wants it. A transfer limit fits one machine's link.

A local release's rows sync like any release's; only its audio, and the paths
it sits at, stay on the importing device until the person uploads it (§4.2).

### 6.2 Cloud homes

A library's *cloud home* is where coven stores it: an S3-compatible bucket,
Dropbox, OneDrive, or iCloud on Apple devices. A library can
also have none and stay on one device. The baeium edition offers S3 only.

Every cloud home holds the same thing: encrypted objects under random names
(§4.5). coven's
[Part VI](https://github.com/bae-fm/coven/blob/main/spec/coven.md#part-vi-storage)
says what each provider needs.

**S3 setup.** Dan, on the baeium edition, sets up a library on a Cloudflare R2
bucket. The form asks for the bucket, region, endpoint and access key, and
nothing else. coven knows R2 by its endpoint's host and fills in how to talk to
it
([coven E.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#e3-s3-settings-and-profiles)).
An Advanced section holds the optional key prefix and a choice of profile,
automatic by default. For a service coven doesn't know, *Detect Settings*
probes the bucket with the key and shows what it found, which the person can
then use. When coven refuses a setup, bae says why in plain words.

The key prefix is the folder in the bucket the library lives under. The app
passes it as typed, and coven parses it once
([coven §20.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#203-where-a-store-lives)):
slashes at its ends mean nothing, so Dan typing `music/bae/` or `/music/bae`
gets `music/bae`, and only slashes is the bucket's root, as empty is. Settings
then shows the prefix in that one spelling. A prefix that can't be a folder
path is refused before any request, with a line naming the folder at fault:
two slashes in a row, a character other than a–z, A–Z, a digit, `-` or `_`,
or a folder name over 255 characters.

**Dropbox and OneDrive setup.** The person signs in through the browser, then
picks the folder the library lives in. On Dropbox that is a shared folder, so
the household can be invited to it; with none yet, bae says to share one in
Dropbox and look again. On OneDrive it is any folder, browsed from the top,
and the person can make a new one there. The picker moves into a folder only
once its folders have loaded; a listing that fails leaves it where it was,
with the failure and Retry, and a top that fails to list shows only those.
coven holds the sign-in; bae never
sees its tokens. bae's Dropbox app has access to the whole Dropbox, not an app
folder: an app folder can't be shared, so a household couldn't be invited to
it.

**iCloud setup** uses the Apple Account the device is signed in to, and keeps
the library in bae's iCloud container, `iCloud.fm.bae.desktop`.

A device can hold several libraries, each its own coven store, and remembers
which it last opened. coven keeps each store under `~/.bae/stores/`; bae keeps
a library's settings in `~/.bae/libraries/<library id>/`. One library whose
storage settings can't be read never hides the others: the list carries each
library with its settings or why they can't be read, shows that reason in
place of the account, and offers no Open for it. Launch and switching
libraries pass over it.

### 6.3 How bae declares its tables

coven syncs only the tables it is told about. bae declares each library table
with:

- **its row identity**: a *shared key*, an id computed from the fact the row
  states (§2.6), or an *independent* id minted fresh for each row: releases,
  tracks, files, audio formats and segments, track credits and works, role
  credits, plays and playlists. A cover takes its release's id, and a rating
  its person and what it rates;
- **its audience**: every library table is in the store; `history` and
  `playlists` name theirs per row, and `playlist_entries` take their
  playlist's (§5.3,
  [coven §15.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#151-declaring-audiences));
- **whether it is an owner table**: `people`, `ratings`, `history`,
  `playlists` and `playlist_entries` are (§5.5);
- **the columns that reference a coven file**: `file` on `release_files`,
  `covers` and `artist_images`, and `avatar_id` on `people`, each a trailing
  reference: a write setting it goes out at once, and other devices see the
  file on its uploading device until it is stored
  ([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)).

bae declares its four kinds of file when the store opens, each a coven
*namespace* with its kind and this device's cache budget (§4.3):
`release_files`, the person's own; `covers`, `artist_images` and `avatars`,
bae's own. Each file is downloaded when it is first read.

Appendix B lists every declaration. A test holds the declarations to exactly
that list, each table with its identity, audience, owner flag and file
references, and the namespaces with their kinds and budgets, so a
device-local table such as `local_release_files` or `local_history` can't start
syncing by accident. Another holds every table the schema creates to one
of two lists, declared or device-local, so a new table stays on one device only
when that was decided.

Two devices' edits merge cell by cell: an edit to a column the other left alone
survives, and of two edits to one column the later wins
([coven §8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#8-merging)).

### 6.4 When sync stops

Sync runs in the background. A library opens and plays from its own copy
whether or not the cloud is reachable.

**The activity indicator.** Sync takes one small control in the window's
title bar on macOS and in the library's top bar on Android, never a
bar across the content. While bytes move it is a spinner: files this device
is sending, or offline copies downloading. While something needs the person, a sync failure or a pending record,
it shows a small warning symbol, beside the spinner when both hold. With
neither it is absent. A pass alone doesn't spin it: an idle library runs one
every half minute, and the indicator would blink for nothing.

Clicking it opens what sync is doing, a popover on macOS and a sheet on the
phones, updating as it changes. What needs the person comes first, under *Needs
Attention*: the failure's line and each pending record in plain words, with
*Open Sync Settings*, where each record offers its action. A failure a pending record shows has no line of its own: a pass that
storage refused or couldn't reach leaves the connection's record, a blocked
slot and a failed operation of coven's own have theirs, and the failure again
beside its record would read as a second problem. Then one row per
thing moving: a pass syncing the library's changes, the uploads ("2
uploading · 10 queued", with the bytes their providers confirmed), and the
offline downloads, which on the phones open the downloads list. With nothing
to show it says everything is up to date. *Retry Now* shows only while pressing
it would run a pass and a failure that pass can clear holds work back. coven
publishes what a retry made now would do, decided as it answers the retry
([coven api.md
E5](https://github.com/bae-fm/coven/blob/main/spec/api.md#e5-storage-and-sync)):
run a pass, or be answered at once because sync isn't running, a failure only
the person can clear holds every pass (signing in again, an update, a reset),
or the provider asked every request to wait. Retry Now shows only while it
would run a pass, and comes back on its own when such a wait ends. The failure
is the pass's own, such as storage unreachable or answering with a server
error, or with the pass not failing, a pending record's that coven retries by
itself after a wait: an upload a server error refused, or a local disk failure.
A blocked slot and coven's own failed operation count as their records do. A
failure only an update, a reset or nothing clears, such as a removed device,
never shows it, nor does a record waiting on another device's work; a record
waiting on the person offers its own action. It retries everything at once, as
pulling to refresh does.

**The status line.** Settings shows one line for sync. With records pending it
says "Waiting: 3 items". With none it names what sync is doing: Syncing,
Synced, Offline, Sync failed, or Sync stopped when the library has a cloud home
but isn't syncing. A failure's error sits under the line, unless a pending
record shows it. "Checking sync…"
shows only until the pending list is first read, and if it can't be read the
line says "Couldn't read sync status", opening onto the error. Opening it lists each thing coven is waiting on, one of its pending
records
([coven §19.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#191-what-the-app-sees)),
in plain words, in the order they arose, with an action wherever the person
can act. A record whose condition changes keeps its place. The error behind
a record sits behind a disclosure, to copy: its text names every cause below
it, with secrets and URLs already cut by coven. Only the process that met the
failure holds that error, so a record read after a restart has no details and
offers none. The list follows those errors beside the records: a retry that
fails for the same reason leaves its record as it was, and the list shows the
error the retry met. The details are never the record's internal debug text. This list
replaces any banner of blocked work: there is one place to look.

Each record names what it is about in plain words. A file is named by the row
on this device that references it: a release file by its file name and album, a
cover by its album, an artist image or a profile picture as such, and a
release file or cover no row names any more, kept only by a lost value
naming it, as a file of a release that is gone; a file no row here names, of
no namespace bae knows, is a file. A move's record, about an original its
upload found missing or changed, is named by the release file this device
recorded at that original's path. The list is one query over coven's pending
records, coven's records of the files they name and the library's rows, so a
record is named as its row stands in the same read. Any other subject, such
as an operation or the connection, is named by what it is. In the upload list
itself a file's row already names it, so its record there says only why it
waits.

**Files another device hasn't uploaded.** A row references its coven file from
the write that stages it
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)).
Ana's Mac imports an album to the cloud and uploads its files over the next
hour; her phone shows the album at once, each row referencing its file, which
coven records on the Mac only until the last of the album's files is stored
and one write switches them all to the cloud. Until then the phone can't read
the files, and can't tell an upload still running on the Mac from a release
the Mac keeps local: either way the release reads as not uploaded, and a
picture whose file isn't readable here shows a plain tile in its place. No
album or track is marked as on its way. No device waits for another's upload,
and a read of a file that fails leaves nothing pending, so no pending record
is about another device's upload.

Ben's laptop shows "Waiting: 2 items": Dropbox wants him to sign in again, and
a file of his vinyl rip moved before it uploaded. The list offers *Sign In* for
the first: core signs in with the access the library's folder needs, then
connects that location again under this device's name. For the second it
offers to restore the file: put it back where it was imported from, and coven
retries the upload. A file that changed instead stops its release's move to
the cloud, and its record offers *Cancel Upload*, which ends the move and
makes the release local here again (§4.2). A pin the download queue couldn't
finish offers *Retry*, which queues the release's pin again, the failed entry
in its place or the release anew if it left the queue (§4.3); a file coven
keeps here after a move retries its own download. Other records offer
*Retry*, *Reconnect*, *Resume Uploads*, *Update App* or *Restore With Code*. bae never asks for a storage
administrator's credentials: the person makes and deletes S3 access keys in the
provider's console, and no record asks them to delete one
([coven §14.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#143-s3-access-keys)).
A wait the person can't help, such as for another device to come online,
offers nothing.

- **A removed device.** When this device, or its person, was removed from the
  library, its record says so in its own error line, "This device was removed
  from the library.", and offers only the record's details. Resetting would
  register the device again under keys that no longer count, asking storage to
  take back a device the library removed.
- **Replaced keys.** When another of the person's devices replaced their keys,
  this device can no longer send. Its record offers *Restore With Code*: bae
  closes the library and asks for the person's current restore code, warning
  that restoring loads the library from storage again and discards edits on
  this device that haven't synced. Core checks first that the code restores
  this library; another library's code, or an invite, changes nothing. coven
  then loads the library from the code under a new device id beside this
  device's copy, which stays as it was until the load is complete and only
  then is replaced
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
  A restore that fails, such as with a code the replaced keys made or with
  storage out of reach, leaves the library on this device: the code screen
  shows why and the person can restore again, and going back opens the
  library as it was.

"Synced" means coven finished a pass and nothing is pending. Until the pending
list has been read, or after a pass failed, the line says it is checking
rather than claim the library is synced.

- **Launching offline.** A failed connect at start-up opens the library
  unconnected, and the status line says so. *Reconnect* connects again, or
  runs the next cycle now when already connected.
- **The network coming back.** Each attempt that finds no network backs off
  longer, up to five minutes. While Ana's phone is locked, Android blocks
  bae's network, so every pass and upload fails that way; once she unlocks it
  the network is back, but the next attempt would still be minutes away. So
  the apps tell sync when the network may be back: when the platform reports
  the network became usable or changed, when bae comes to the foreground, and
  when the device wakes. Each is reported once, on the change, and only while
  a library is open. coven then ends every wait whose last attempt got no
  answer from storage and runs a pass at once; a wait after storage answered,
  such as with a server error, keeps its time
  ([coven §6.7](https://github.com/bae-fm/coven/blob/main/spec/coven.md#67-waiting-and-retrying)).
- **Retrying now.** Pulling the library down to refresh on a phone asks to
  retry everything now: every ordinary backoff ends, whether storage
  answered the last attempt or not, and a pass runs
  ([coven §6.7](https://github.com/bae-fm/coven/blob/main/spec/coven.md#67-waiting-and-retrying)).
  A cover another device has just uploaded comes down at once instead of
  after its download's backoff. Each wait keeps its count, so a retry that
  fails again waits longer. Provider cooldowns, failures that never retry,
  and waits for the person's own action hold. bae's own triggers, such as
  opening a library, only ask for a pass.
- **How the retry went.** The pull's spinner stays until the first pass after
  the retry ends, however long that takes; there is no timer. A pass already
  running when the person pulls doesn't count: it read the store before they
  asked, so the next one answers. If the retry didn't sync, the reason shows
  once it's known: on Android a snackbar, whose *Retry* asks again. The reason
  is the pass's failure, such as being offline; the provider asking every
  request to wait, with the date and time it ends, after which sync goes on by
  itself; or that sync isn't running. A pass held by such a wait, or by a
  failure only the person can clear, answers at once. *Retry Now* in the
  activity list waits for the same answer and shows a retry that didn't sync
  the same way on Android, and in the list on macOS.
- **A newer schema.** Data written by a newer app needs an update;
  reconnecting can't fix it.
- **Pausing** stops uploads and keeps their place, across restarts too.

**Resetting a device.** Opening can fail because the database is damaged, and
sync can find that this installation is another copy of the device, such as
Carol's phone restored from a backup
([coven §19.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#193-another-copy-of-the-same-device)),
which coven reports as the sync status's failure. There, and only there, bae
offers to reset the device: load the library from storage again under a fresh
device id. Before it does, a warning says that edits on this device
that haven't synced will be discarded. The person was told before choosing, so
nothing is shown after; bae doesn't present coven's reset notice.

**A reset or restore that stops partway.** A reset, or a restore with the
person's current code, loads the new copy beside this device's copy, then
swaps it into place. If the app stops after the swap, such as when it quits,
the library waits for that same call to finish it
([coven §19.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#193-another-copy-of-the-same-device)).
bae still lists the library, under the name the new copy loaded, with the
call that finishes it, and opening it fails with that call's own line:
"Restoring this library on this device did not finish." or "Resetting this
device for this library did not finish." Only that call finishes it. A reset
would load a restore's library under the keys the restore replaced, and a
restore would delete the copy a reset keeps, so coven refuses the other call
before anything changes.

The apps show such a library with that line, in red, where they list
libraries that can't open: the Mac's library list, and the library switcher
on phones. They offer only its call. *Finish Restoring* asks for the person's
current restore code, and the S3 key for a library on S3, then restores
again. *Finish Resetting* resets again, without the warning the person
already accepted. Opening the library, such as at launch, lands on the same
screen. Going back leaves the library waiting and returns to the open
library, to the Mac's library list, or on a phone with no library open, to
onboarding.

**Lost values.** When two devices edit one cell and one edit is replaced
unseen, coven records a lost value
([coven §8.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#82-lost-values)).
bae shows none and dismisses none. coven keeps each one, and any file it names,
until an app dismisses it, so nothing is gone while bae doesn't show them.

**The keyring.** The library's encryption key lives in the system keyring. A
keyring that is locked right now, by a locked session or a sleeping display,
is told apart from a broken one: the host asks again after unlock instead of
reporting the library unusable. A device can forget the key, which locks the
library until it is unlocked again.

**Removing a library from a device.** Dan stops using his old laptop. Removing
Household from it first removes the laptop from the library, then deletes its
directory and the secrets the laptop keeps for it; the cloud copy stays. Leaving
is an admin entry, so it needs storage. Offline, removal says so and waits
rather than deleting the local copy and leaving a device listed that will
never come back. A library with no cloud home has no members to leave, and is
deleted at once.

- **Unsent edits.** Dan's laptop still holds edits it hasn't sent. Before
  removing, bae asks coven what this device hasn't sent, and the confirmation
  counts it: the edits, and the files that haven't finished uploading, then
  that removing the library discards them. The library keeps running while
  bae asks, so *Cancel* changes nothing; for a library that isn't open, bae
  opens its store just long enough to count. If Dan confirms, the laptop
  leaves without sending them. Refusing instead would tie his removal to
  uploads that may be stuck; the counts let him decide.
- **A library that can't open.** A library whose settings can't be read, or
  whose reset or restore stopped partway, can't be opened to count what it
  hasn't sent, nor to leave. Its confirmation says it can't be opened, so any
  changes on this device that weren't sent are lost. Removing it deletes it
  from this device without opening it, through coven's delete of a closed
  store: the secrets this device keeps for it and its directory, then bae's
  files and the record, as in any removal. The device stays among the
  library's members until it is removed from them (§5.6). For a reset or
  restore that stopped partway, the delete takes both copies, the one the
  call loaded and the one it replaced, so giving up on the call is the other
  way out of it.
- **A removal that stops partway.** bae and coven can't commit together, so
  bae writes a record of the removal in its own directory before coven
  deletes the store, and deletes the record last. A removal that stopped is
  found from its record, never from bae files that coven's store list lacks:
  a library hidden from that list, by unreadable settings or a first restore
  or join cut off partway, still exists. At launch bae shows a recorded
  removal whose store coven no longer lists, with *Retry*, and opens no
  library while it shows.
  *Retry* deletes only what is left on this device; it never tries to leave
  again. A record whose store coven still lists stopped before the delete: it
  is cleared, and bae says the library is still on this device, with nothing
  to retry. Removals, finishing them and looking for them each hold one
  removal lock, an OS file lock that excludes other processes too, so a
  lookup never sees a removal still waiting on coven.
- **The restore code stays.** The restore code and S3 key coven saved in iCloud
  Keychain are Dan's, not the laptop's, and every one of his Apple devices sees
  them. Removing Household from the laptop keeps them: his other devices still
  offer restoring it, and so does the laptop, now that it no longer holds the
  library (§5.6). They are deleted when Dan leaves the library, by one of his
  devices as it applies his removal, whether an admin removed him or he removed
  himself, or when he deletes the code from the iCloud Keychain list (§5.6)
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

---
## 7. Playback

Playing a record well means more than decoding files. An album should play
without gaps where it has none, stop where the record has to be turned over,
keep its hidden pregaps, and sound as loud as the next one. And a device should
play from its own disk, the cache or the cloud without the listener knowing.
This section covers the queue (§7.1–§7.2), how tracks join (§7.3–§7.4),
loudness (§7.5), where the audio comes from and goes to (§7.6–§7.7), and what a
device remembers (§7.8).

### 7.1 The queue

The queue has two parts: *Up Next*, then what's *playing from*.

Ana plays her CD rip from its third track. The release becomes what she is
playing from, with the cursor on track 3. She then adds two tracks of Ben's
download to Up Next:

```
now playing   Album Title, track 3          (from the release)
up next       Ben's track 1, Ben's track 5  (played first, then gone)
playing from  Album Title, tracks 4–11
```

When track 3 ends, Up Next drains first, then the release continues at track
4.

What's playing from is a release, several releases in the order chosen, or the
whole library. Albums picked together in the album grid, played, queued or
dragged to the queue, go in the grid's order under its sort and grouping,
which core reads for the ids it is handed, whatever order they were picked
in. Its rows are filled once, when playing starts; after that they
can be removed and reordered but never grow. It holds each track once: a
release named twice among several plays once, where it is first named.
Removing the playing row leaves the track after it next. Once its last row
leaves, removed or its track deleted from the library, nothing is playing
from. Up Next entries are each their own, so the same track queued twice
there is two entries, each removable on its own.

| Command | Effect |
|---|---|
| Play a release, from a track or shuffled | Up Next is cleared; the release is what's playing from |
| Play next / add to queue | tracks go to the front or end of Up Next |
| Remove, reorder, skip to | acts on one entry, within its own part |
| Clear Up Next | empties Up Next only |
| Clear playing from | drops what's playing from; the current track plays on, then Up Next, then playback stops |

A track deleted from the library leaves every place it sits in the queue.

### 7.2 Repeat, shuffle, previous

**Repeat** is off, the whole of what's playing from, or the current track. The
button steps off, then everything, then the track. Commands set a mode; they
never toggle, so a repeated command lands in the same place.

**Shuffle** rearranges only what's still to come in what's playing from. The
current track and those before it don't move. Turning shuffle off puts the
rows back in the order they had when it turned on, keeping any removals and
reorders made meanwhile. Repeating a shuffled list shuffles it afresh for each
pass.

**Previous** goes back one track within the first three seconds of a track,
and restarts the track after that. Playing an Up Next entry, previous goes back
to the track of what's playing from that it interrupted.

### 7.3 Gapless, sides and discs

Tracks join without a gap: the next track is decoded ahead and the audio
output crosses to it within one buffer. This needs both tracks in one sample
rate and channel count; otherwise the output is rebuilt between them, with a
short silence.

There is an optional pause between sides or discs. Ben plays his 2×LP rip.
When side A ends, playback pauses and shows "Side A ended"; he presses Play,
or Next, to start side B. A CD box set pauses between discs the same way.

- **When it pauses.** Only between two tracks of one release, on different
  sides, while playing that release in order. Up Next entries and a shuffled
  list never pause: they aren't a record being played.
- **Side or disc.** A boundary is a change of medium or of side, and the
  current track's medium says what it is: a record or cassette pauses at a
  side, named by its letter; a CD at a disc, named by its number among the
  release's mediums. A medium with no physical carrier, a download or a disc
  of unstated carrier, never pauses.
- **The countdown.** The pause can end on its own after 5, 15, 30, 45 or 60
  seconds. Every device's UI counts down to one moment core sets, so they show
  the same number. Closing the prompt stops the countdown and waits for Play.
- **Settings.** Pausing is on by default: a side break is part of how the
  record was made. Turning it on also catches a crossing already prepared.

### 7.4 Pregaps

Pregaps play as on the disc. A track's stream starts at its first sample, its
index 00; the track itself starts at index 01, after the pregap. The player
shows time from index 01, negative while the pregap plays.

- **Playing straight through** plays each pregap, as the disc does.
- **Choosing a track** — clicking it, Next, Previous, skipping to an entry —
  starts at index 01 and skips the pregap.
- **A generated pregap**, from a CUE `PREGAP` line, is silence bae makes before
  the track's first stored sample.

A pregap, stored or generated, is kept only as a count of samples. Its length
on the track's timeline comes from that count and the track's sample rate.

### 7.5 Loudness

Loudness is per track or per album, or off.

The gain is worked out at play time from the loudness stored at import
(§3.7), never stored itself:

- it brings the chosen loudness to −18 LUFS;
- it is capped so the track's loudest true peak can't clip;
- *per album* keeps the album's tracks loud and quiet relative to each other;
- with the chosen measurement missing, the other is used; with neither, or
  off, the track plays unchanged.

The default is off.

### 7.6 Where the audio comes from

Playback plays files on this device, or streams them from storage.

A track's file is read from the person's own path for a local release, through
coven from that path while the release moves to the cloud, from the cache or a
pin for an uploaded one, and otherwise fetched from storage a range at a time
([coven §17.6](https://github.com/bae-fm/coven/blob/main/spec/coven.md#176-reading-a-file)).
Reading ahead keeps playback from waiting on the network, and a seek fetches
only what is under the new position. One buffer per file serves every track
that plays from it, so the eleven tracks of a disc image share one.

A track start in a FLAC, WavPack, Opus or Vorbis file jumps to the byte
import recorded for it, since those files' frames state their own position;
in an MP3 it starts at the file's first frame, since an MP3 track is a whole
file (a CUE sheet over lossy audio is not bound); in any other it seeks to its
first sample, the recorded byte fetched ahead.
The scan read every disc image, so import can always read where each of its
tracks starts; an image where it can't is reported as a bug, and its tracks
seek to their first sample with the whole file read ahead.
A seek inside a track
is left to FFmpeg, which seeks through the file's own index where it has one:
a FLAC's seektable, APE's, an MP4's sample table. A FLAC with scrub points
(§3.7) has no index FFmpeg can use, or one too sparse to land a seek near
its target, so the seek jumps to the last point at or before the new
position and decodes forward to it: one window fetched from the point,
instead of a search across the file with one fetch per step, or a decode of
everything from a seektable point minutes earlier.
An MP3 has no index FFmpeg seeks exactly (it would place the seek from the
file's bitrate or its table of contents, an estimate), so the seek decodes
from the last scrub point (§3.7) that plays the new position exactly, or from
the file's start before the first, and plays the samples a play from the
start plays there. In each case the first sample played is the one asked for.
Any other lossy track is seeked a little early and decoded up to the new
position, so its decoder has the frames before it.

Offline, a track whose bytes aren't on the device stops playback with a typed
reason the UI shows. A decoder that stops producing for good is caught and
reported, rather than left behind a frozen position bar.

### 7.7 Outputs

| Output | How it plays |
|---|---|
| This device | bae decodes and plays through the system output |
| AirPlay | bae decodes, and sends the audio to the receiver |
| Chromecast | the receiver fetches the track from bae over HTTP, driven by commands |
| DLNA renderers | the same, over UPnP |

Switching to a receiver keeps the queue and sends the current track at its
current position. Ending the session returns to this device, paused where it
was.

**Fetching receivers.** A Chromecast or UPnP renderer fetches each track from
a small HTTP server bae runs for the session, on the local network, with a
random credential. A track stored as one whole file in a codec the receiver
plays is served as its own bytes. Anything else — a slice of a disc image, a
track whose pregap is in another file or generated, a codec it doesn't play —
is converted on the way: to MP3 at 320 kbps, or to WAV at its stored depth, as
the person chose. A UPnP renderer is never counted on to play WAV or Opus, so
it gets MP3.

**AirPlay** receivers of both generations play. A receiver that demands a PIN,
or offers only an encryption bae doesn't implement, is refused with that
reason.

bae sends audio ahead of what the receiver plays by the delay the receiver
reports when the connection is set up, or two seconds when it reports none.
The position shown, the play recorded in history, and the end of each track
follow what the receiver is playing: each is held back by that same delay.

A receiver takes 44.1 kHz stereo, so bae converts every other track on the way
out. A track bae can't set up a conversion for doesn't play on the speaker,
which says it can't play the track's sample rate; it is never sent
unconverted. A conversion that fails partway stops playback with an error and
is reported as a bug; no audio is skipped silently.

How to connect is read from what the receiver advertises, never guessed. An
AirPlay advertisement must state its features; a RAOP one without them states
none, and a receiver without status flags sets none, as older receivers do. A
receiver whose features or status flags can't be read, or whose AirPlay
advertisement states no features, shows as one bae can't connect to because it
advertises settings bae can't read, even when its other advertisement reads;
the log names the setting and the text it carried.

**Finding receivers.** Casting is off by default: it browses the local network
and serves audio from this machine. While on, devices are searched for only
while the picker is open. Desktops and Android browse the network themselves.

A UPnP renderer is known by its UDN, so it stays the same entry when its
address changes. One that states no UDN is left out of the picker, and the
log names its address.

**This device.** On every platform, playback and a preview follow a change of
the default output device: they move to the new one at the same position. On
macOS and Windows bae starts them again on the new device, and on iOS it does
so when the system changes the route in a way the stream doesn't follow; a
paused one stays paused. On
Android bae reopens the output on the new device, and a change while paused
takes effect on resume. On Linux the sound server, PipeWire or PulseAudio,
moves them; with ALSA alone there is no default device to follow. On the
desktop, when the device goes away and there is none to move to, playback
stops and says the audio device isn't available, and a preview ends. Volume
and mute live in core, so unmuting restores the level last set.

**Sample rates.** On macOS bae plays at the native sample rate: when the
output device supports the track's rate, a track or a preview that starts a
stream sets the device to that rate, and the track plays at it with no
sample-rate conversion; playback that moves to another device sets that one.
It isn't bit-perfect: the output still goes through Core Audio's shared mixer,
which mixes in other apps' sound, mixes in 32-bit float, and scales the samples
when the volume is below full. A change of rate between tracks leaves a gap, as
it always has, while the device switches straight from one track's rate to the
next's. The device goes back to the rate and format it had before bae set it
when playback stops, the app quits, or playback moves to another device; when
a preview played over paused playback ends, the device goes back to the paused
track's rate. On a device that can't run at the track's rate, bae sets it to
one it can and converts the track to that rate: one of the track's family
(whole multiples of 11.025 kHz, like 44.1 kHz, or of 8 kHz, like 48 kHz) when
the device has one, the highest at or below the track's rate, else the lowest
above it; only a device with no rate of that family gets the other family's,
chosen the same way. On Windows the system converts every track to the rate it mixes at. On Linux the stream
opens at the track's rate and the sound server, or ALSA's own conversion,
converts it to the device's; on Android AAudio's mixer does; on iOS the audio
session does.

**When the output won't start.** Playback that can't start its output — on
this device or a receiver — stops and says why: the audio device was
disconnected or isn't available, it can't play the track's sample rate, or the
AirPlay speaker didn't accept the connection. These are the device's or the
speaker's doing. Any other failure is bae's own: it shows as an error, is
reported as a bug, and the log keeps the whole error with its causes.

**Sharing the speaker.** Android lets one app play at a time, through audio
focus. Every start of playback asks the system first — Play, playing a release
or the library, skipping to an entry, Next from a side's pause, the next track,
a side's countdown running out — from the app, the lock screen, a car, the
assistant or the widget alike. While something that outranks music holds the
speaker, such as a phone call, the system refuses: nothing plays, what would
have played waits paused and shows paused, and the system's reason is logged.
Playback gives the speaker back when it stops. Losing it to another app pauses;
a short loss, such as a navigation prompt, that paused playing music resumes it
when it ends. Desktops play without asking.

**Cars and other controllers.** On Android, a car, a watch or another app that
controls playback browses the library and plays from it through the system's
media session. Covers reach it as links it opens itself. Only an app connected
to the session can open them, and it loses access when its last connection
ends or the session closes.

### 7.8 What a device remembers

The queue and position are per device. The queue is stored as it changes: Up
Next, what's playing from in its play order — shuffled or not, with the
session's removals and reorders — which track of it is playing, and repeat.
Beside it a device saves the entry playing and its position, volume and mute,
while a track is loaded.

A restart takes the queue up as it stood: the same order, the same history
before the playing track, and the saved entry loaded paused at its position.
Nothing is shuffled again. A deleted track leaves the stored queue with the
library; when it was the one playing, nothing is resumed. Desktop apps restore
on launch as a setting; off, the library opens with an empty queue and forgets
what it played. Mobile apps always restore, and save as they go to the
background.

---

## 8. Servers

Some listening happens outside bae: a Subsonic app on a phone bae has no app
for, or an agent tidying the import list. bae can serve both from a desktop.
Each server is off by default, and its secret lives in the keyring.

### 8.1 Subsonic

The Subsonic server lets other apps browse and play the library.

Ben turns it on and sets a username and password. His phone's Subsonic client
connects to his laptop, lists the artists, opens an album and plays it.

- **What it serves.** Browsing by artist and album, album lists, one song,
  search, streaming, cover art (Appendix D). A Subsonic album is a bae release.
  bae has no folder tree, so it shows one folder holding everything.
- **Streaming.** A track that is one whole file is served as that file, with
  byte ranges. Any other track is decoded from its segments and served as WAV.
  A client that asks for a transcode gets one, MP3 at 128 kbps unless it says
  otherwise.
- **Login.** Only token login is accepted: the client sends its username, a
  salt of at least six characters, and the MD5 of password plus salt. With no
  username or password set, nobody gets in. Failures come back as Subsonic
  error documents, which clients read, not HTTP errors.
- **Where it listens.** On this machine by default; binding to every address
  opens it to the network. The default port is 4533.
- **Ids** carry their kind: `ar-` an artist, `al-` a release, `tr-` a track. A
  cover's id adds a version, so a client caching art fetches a new cover.
- **Out of scope.** Playlists, stars and ratings, play counts, podcasts,
  jukebox. Scrobbles are accepted and ignored; album lists that need play
  counts or genres come back empty.

### 8.2 MCP

The MCP server offers the automation tools, for agents.

An agent connects to `http://127.0.0.1:47777/mcp` with the bearer token bae
shows. It can list watched folders and import candidates, search the
catalogs, pick a candidate's release, set its fields, labels, pressing facts
and cover, start its import, and read, edit, export or move a library release
(Appendix E). Each tool's input is a JSON schema; a tool that fails returns its
typed error.

The server listens only on this machine, and every request must carry the
token.

---
## 9. API

Every app does the same things with a library: open it, read it, watch it
change, import, play, move files, sync, and change settings. bae-core's Rust
API does each once, so no platform keeps its own copy of a rule. This section
names the API's parts; the source is the reference for each signature.

### 9.1 Opening a library

`AppDir` is bae's directory, `~/.bae` under the home the host passes in. Each
library is registered there by id.

| Call | What it does |
|---|---|
| `create_library` | creates a new library's store and gives this device its identity; `create_library_default` generates the name |
| `restore_from_code`, `restore_from_keychain` | opens a library on a new device from a restore code, typed or scanned, or saved in iCloud Keychain (§5.6) |
| `saved_restore_codes` | lists the restore codes coven saved in iCloud Keychain for libraries not already on this device, each with its library's saved name; an entry coven can't restore from is listed with its library id and why, for deleting only |
| `forget_saved_restore_code` | deletes a saved restore code, readable or not, and the S3 key beside it from iCloud Keychain on all the person's Apple devices; no library, storage or device's copy changes, and an absent code succeeds (§5.6) |
| `restore_replacing_device` | after another of the person's devices replaced their keys, checks the code restores this library, then restores it under a new device id beside this device's copy, which a failed restore leaves as it was; called again, finishes a restore that stopped after the restored copy took this one's place (§6.4) |
| `join_with_invite` | joins a library from an invite and writes the person's profile with the name the invite carries (§5.6) |
| `unfinished_joins`, `resume_join` | list the joins this device has under way, and continue one after bae restarts, without the code, signing in only when coven has no sign-in it can use (§5.6) |
| `reset_device` | loads a library again under a fresh device id, after the warning; called again, finishes a reset that stopped after the new copy took the old one's place (§6.4) |
| `Config::discover_libraries` | lists registered libraries, including any that can't be opened, with why: storage settings that can't be read, or a reset or restore of this device's copy that didn't finish, with the call that finishes it |
| `app::bootstrap` | opens a library and starts its services: the store, sync, playback, and on desktops import and identification; a library whose reset or restore didn't finish fails with the call that finishes it |
| `AppServices::close` | stops playback, saving where it was, then identification and import, then closes the store |
| `unsent_before_removal` | counts what removing a library would discard: the edits and files this device hasn't sent, for the open library or, opening its store briefly, one that isn't open (§6.4) |
| `AppServices::remove_library` | stops the library's services, records the removal, leaves the store discarding what this device hasn't sent, then deletes the library from this device and the record last (§6.4) |
| `remove_unopenable_library` | records the removal of a library that can't open, deletes its store from this device without opening it, losing what it hasn't sent, then its files and the record last (§6.4) |
| `UnfinishedRemoval::find`, `finish_removals` | report recorded removals that stopped partway, and finish them (§6.4) |

Creating, restoring and joining each return coven's open handle, and bae
starts the library on that same handle rather than opening it again.

`bootstrap` returns `AppServices`, the running library. Every later call goes
through it. A store is opened once per process; closing waits for every worker
holding it.

### 9.2 Reading and watching

Reads come two ways:

- **One-time reads** return the library as it stands: an album's detail, a
  release's tracks and files, an artist, a work, a search.
- **Live subscriptions** deliver a value, then a new one each time the rows it
  read change. The album grid, the artist and composer lists, a search, an
  editor's artist search, the import list, sync status, uploads, downloads and
  the queue are all live. A read that fails is delivered as a value and the
  subscription goes on, but one that fails because the store closed, or
  because the library can no longer read or write it, is delivered once and
  ends the subscription. A subscription that shows a window of a long list is
  pointed at a new window in place as the person scrolls, and a window that
  leaves the screen before its first value is let go of then, so the
  windows a fast scroll stops on are not read behind every one it passed;
  one that has ended takes the new window and reads nothing more. The downloads list on the phones is such a list: it
  reads how many releases the queue holds and the windows on screen, and its
  pause and retry controls and summary line read how many are in each phase.
  The Storage Manager's list is also told
  which releases are selected in it, and each of its values carries the row
  of each one the list holds, on screen or not, and the storage actions every
  one of those allows: a selection is offered only those. Its inspector
  shows, and its storage actions act on, a selected release only while the
  list holds it: a release
  that finishes uploading out of the Local list, no longer matches the filter,
  or is deleted leaves the inspector and the actions' targets as it leaves the
  list, including while an action's confirmation or folder prompt is open.
  The inspector reads its release live by id: the release's files, each beside
  its upload while one waits (whether it is being sent, and what it waits for
  with the error behind that while this process holds it; how far it has got
  arrives apart, by file; and while the release waits in or runs through the
  download queue, whether the file is kept whole on this device yet, as
  coven's `coven_pins` says, or out of that queue, while coven downloads the
  files it keeps here after a move (§4.2), whether each is kept yet;
  and whether coven is downloading the file now to be kept, as
  `coven_downloading` says, how far it has got arriving apart, by file, from
  coven's download watch), and its entry in the download, export and upload
  queues, a download with how many of its files are downloading, wait and
  are kept, and coven's fetch of the files it keeps here, with how many are
  downloading and kept, all read for that release alone and at once, so its queue entries
  and its files' pins are as they stood together. It shows nothing of a
  whole queue, which the list's footer pauses (§4.2): its upload shows how
  many of its files are uploading, queued and stored, a count of none left
  out, so a release with nothing being sent reads only what waits and what
  is stored; a control that
  cancels it, and while files are sent, the bytes of its size stored or sent
  and how fast they go. Each file is one line, its name and size after a
  mark: none for a file at rest, a dashed circle for one waiting its turn, a
  ring filling as it is sent; a file waiting on a reason says why when the
  pointer rests on it. While the release pins, its download shows the same
  way: how many of its files are pinning, waiting and pinned, again leaving
  out a count of none; a control that
  cancels the pin, and while it downloads the bytes it holds of its size and
  how fast it goes; a file downloading shows a ring filling as it comes, and
  one waiting its turn a dashed circle. Coven's fetch
  shows the same counts and marks, with no control and no bytes: no queue
  entry of bae's holds it. A failed pin shows its error instead, with a
  control that retries it (§4.3) and one that cancels it. Selecting a file shows its details under the list,
  closed by a button or by selecting nothing: its name and the folder it
  sits in within the release's folder, what kind of file it is, its audio
  format, and its size. Core reads a file's kind from its content type, and
  for a cue sheet or a rip log from its extension.
  The upload queue's own subscription says only whether it is paused; how
  fast each release's upload goes, the bytes and speed each download moves,
  and the percent each export reaches arrive apart, per release.

What a read returns is already resolved: labels, paths, flags, totals, sides
and positions are worked out in core, and a surface only renders them. A
detail embeds its summary, so a surface holding a detail can treat it as one.

`subscribe_ui_events` carries what isn't a value: a playback error, tracks
added to the queue, import progress.

### 9.3 Importing

The import service (desktop only) owns the watched folders, the scan, the
candidates, extraction, identification and the import worker. Its handle
watches and unwatches folders, scans, groups and separates candidates, runs and
cancels identification, searches, picks, edits drafts, binds CUE sheets,
starts and cancels imports, and serves the import list's live query. What
each candidate has in flight — what its identification is doing, the step
its import is at, its search — is a subscription by key, sent as it steps;
how far through its step a running import is moves many times a second, so
it reaches an app only in a `KeyedValueStream` (§10.2), by candidate. Every
write to a candidate loads it whole, applies its rule, and saves it against
the revisions it read, so two writers never disagree about which rows belong
together.

### 9.4 Playing

The playback handle drives one queue on its own thread: play a release,
several releases or the library shuffled; pause, resume, stop; next, previous,
seek; volume and mute; queue edits; repeat and shuffle; play on a receiver or
stop; preview a file before import. Its values are live subscriptions: the
playing track and its phase, with volume, mute, repeat and where playback is
going, sent only when one changes; the timeline, the line of the playing
track and of a preview, each a position and whether it moves on from it,
sent only as playback leaves the line it holds (a seek, another track or
clip, a pause or resume, a new length, the pregap ending, or a position more
than a second off the line), for the operating system's media controls and
Media3, which move the position on themselves; and the queue. Those surfaces
show no time before a track's start, so a playing pregap is one line
standing at the start, and the line that moves begins as the track does. The playing track's position and
a preview's, which move as audio plays, reach an app only in a
`KeyedValueStream` (§10.2): the track's under its id, with the seeks applied
so far. A seek the person drops is held in front of the track's position
until core has applied it, so every view drawing that track — the seek bar,
the queue's strip — shows where they dropped it meanwhile.

### 9.5 Storage and sync

Through `AppServices`: move releases to the cloud and make them local again,
pin and unpin, cancel a transition, export and save; set up a cloud home,
reconnect, pause, read the pending list and act on its records; list people
and their devices, watch for a device of this person's that joins after the
watch begins, invite a person in person or by sending the code, list this
device's open invites, approve or decline a sent invite's request,
change a role, remove a person or a device, rename the library; make a restore
code; create and rename a profile, set its avatar and history setting, rate,
and make and edit playlists.

### 9.6 Settings

`ConfigHandle` holds the library's settings. Each change edits a copy,
writes it to disk, then publishes it to subscribers, so a UI follows a change
with no restart. Appendix C lists the settings.

### 9.7 Errors

The locale never crosses into core. A failure reaches a UI as a category and a
detail: the UI shows one translated line per category, and offers the detail,
the Rust error chain, untranslated, to copy. A category says what the person
can do: fix credentials, retry the network, unlock the keyring, wait for an
identification. Appendix G lists them.

A failure while bringing a library up has a step as well as a category: bae
starting, opening secure storage, reading sign-in settings, listing this
device's libraries, or opening, resetting, closing or removing one. Each app
titles the failure by its step and says what to do next for that step, above
the category's line and the detail, so a keychain that won't open never reads
as a library that failed to open.

What launch finds wrong without stopping the app is kept apart from those
failures for the whole run: sign-in settings bae can't read, and on the Mac a
keychain that won't start, which leaves its libraries listed but closed. It
shows beside whatever opens or fails later, so a library that opens doesn't
hide it and a later failure doesn't replace it. On phones a keychain that won't
start stops the launch.

---

## 10. Bridge

The apps are written in Swift and Kotlin, and the API is Rust. The bridge is
how the API reaches Swift and Kotlin, and C# for bae-avalonia.

### 10.1 Only translation

bae-bridge does one thing: it converts bae-core's types to and from types
uniffi can export. It looks nothing up, formats nothing, filters nothing and
holds no state. Anything else is added to bae-core, and the bridge calls it.

The reason is the platforms: anything a UI or the bridge decides, each
platform would decide again, and they would drift. So a UI renders what it is
given. It formats numbers, sizes, durations and dates in its locale, and turns
message keys into text from its string catalog. It never derives a value from
two fields, sorts or groups by a domain rule, or builds a URL.

Most `Bridge*` types restate a core type field for field. Macros in bae-mirror
write those copies, and destructure every field, so a field added in core fails
to compile in the bridge until it is carried across.

### 10.2 The handle and the host

- **`BridgeHost`** is built once per process. It holds bae's directory, the
  OAuth applications, on phones the app's sign-in presenter, and a runtime for
  the calls that run before a library is open: create, restore, join, reset,
  sign in. A call that creates or opens a store hands back coven's handle, and
  the app starts the library on it.
- **`init_app`** opens a library and returns an **`AppHandle`**, which wraps
  `AppServices` and, on desktop, the MCP and Subsonic servers and the cast
  controller.
- **Async calls** run on the library's own runtime. A Swift task cancelled
  mid-call cancels the Rust work it started.
- **Closing a library** ends it before its runtime: playback saves where it
  was and stops, the desktop servers, identification, import and sync stop,
  the library's tasks, its live subscriptions among them, end and its store
  closes, and only then does the runtime shut down. Releasing the handle closes it the same way; the app closes it
  first, off the main thread, to wait for it and hear a failure, which a
  release reports as a bug. A handle released on the library's own runtime
  closes it on a thread of its own, and opening, resetting, restoring or
  removing that library waits for the close first. After a removal deleted
  the store, only the runtime is left to end. An open that fails after the
  library started closes it the same way before its runtime ends.
- **Subscriptions** take a callback object the app implements, and return a
  `LiveSubscription`; cancelling or dropping it stops the delivery. Each runs
  as one of the library's tasks, and so does every task core starts to read
  and merge its values, so closing or removing the library stops them all
  before the store closes, and none hears from a closed store. One opened on
  a closed library delivers nothing and is reported as a bug.
- **Fast values** — transfer bytes and rates, file shares, export
  percents, playback and preview positions, import percents, and how far a
  bulk action has got, which core sends many times a second — reach an app
  only in a `KeyedValueStream`, keyed by release, file, track or candidate,
  never in a callback, a
  record field or a subscription value. A `feed_*` call starts writing one
  on the bridge's side and returns it; cancelling or dropping it, or closing
  the library, stops it, and it then holds nothing, whatever still runs that
  wrote it — a bulk action's stream included. A read that fails empties
  it until the next read fills it whole. Its one callback names the keys that
  started or stopped holding a value, never a value that moved. The native
  view drawing a key reads the key's latest value once per display frame
  while it holds one; nothing the UI framework observes holds a fast value.
- **Pulled subscriptions** — the browse and download lists, the searches,
  the import list, and the reads of which selected albums are gone and which
  offered releases the library holds — are objects the app asks for each
  next value and points at new windows or a new query in place. Closing or
  removing the library cancels every one before the store closes: a read
  waiting on one, or asked of it after, answers that it was cancelled, never
  with the closed store, and it takes no new request. One opened as the
  library closes is cancelled from the start.
- **Callbacks the app provides**: UI events, the artwork reader, and on phones
  the sign-in presenter, which shows the provider's page and returns where it
  redirected. Desktops sign in through coven's own browser flow; iCloud runs in
  coven.

### 10.3 Generated bindings

The Swift, Kotlin and C# bindings are generated from the Rust at build time
and are not in the repository. A type's fields are read in
`bae-bridge/src/types/`.

Build features choose what a build exports:

| Feature | Adds |
|---|---|
| `oauth-providers` | Dropbox and OneDrive sign-in |
| `cloudkit` | iCloud |
| `cast` | the receiver picker and casting |
| `desktop` | casting, plus import, identification, metadata editing, export and save, and the MCP and Subsonic servers |

The baeium edition builds without the first two, and a check confirms its
bindings export none of their symbols.

### 10.4 The apps

| App | Shape |
|---|---|
| bae-macos | AppKit app whose application delegate owns start-up, the main menu and the windows; each window hosts SwiftUI views. The main menu holds the standard macOS menus with their standard actions, sent up the responder chain, plus bae's own commands. Its service layer and the generated bindings live in the BaeKit package, whose start-up pieces the iOS app also uses |
| bae-ios | iOS start-up on BaeKit (telemetry, crash reporting, the host and the keychain), release packaging, and one screen that lists this device's libraries through the bridge |
| bae-android | Kotlin app: onboarding, library, album detail, downloads, playback with a system media session, settings, a widget |
| bae-avalonia | Windows and Linux start-up, single instance, crash capture, telemetry, updater and packaging, and one window that calls the bridge |

---

## 11. Crates and conventions

### 11.1 Crates

| Crate | Holds |
|---|---|
| bae-core | the library, schema, import, identification, files, playback, outputs, sync integration, settings |
| bae-catalogs | the MusicBrainz, Discogs and Wikidata clients and the rate their requests are admitted at, the catalogs bae knows and the pages they publish, and the covers they offer with the cache their images are read through |
| bae-audio | the audio codec over FFmpeg (decode, encode, probe, resample), the sparse buffer a decode reads a file's bytes from, and the stream a decoded track plays from |
| bae-types | the domain values the catalogs, identification and the library share: what a pressing is, how two pieces of written text name one thing, and how a barcode is read |
| bae-util | what the other crates build on, with no library vocabulary and nothing else in bae beneath it: content types, duration labels, outbound HTTP and its retries, error text, stored images, text encodings, the embedded-server lifecycle |
| bae-bridge | the uniffi exports for the apps |
| bae-desktop | the desktop services a bridge handle starts: the MCP and Subsonic servers |
| bae-subsonic | the Subsonic server |
| bae-mcp | the MCP server over bae-automation |
| bae-automation | the automation tools and their JSON shapes |
| bae-cast | the AirPlay, Google Cast and UPnP protocols, receiver discovery, the session media server, and its URLs |
| bae-mirror | the macros that write field-for-field conversions |
| bae-loc | the string catalog: checks it and writes each platform's resources |
| bae-theme | the shared theme, written as Swift and Kotlin |
| bae-test-support | helpers for bae-core's integration tests |
| bae-uniffi-bindgen | the bindings generator |

bae-cast stands between bae-core, which holds the renderer session, and
bae-subsonic, which serves the audio, because neither may depend on the other.
Nothing in bae-core depends on a server crate. bae-core holds no device
protocol: playback drives a fetching receiver through a command channel and an
AirPlay receiver through a sink it hands decoded audio to, and bae-cast connects
each device and hands playback the channel or sink.

### 11.2 Conventions

- **One schema.** The database schema is one migration file. A schema change
  edits it; there is never a second migration, and development data is reset.
  No library has to advance from an older schema before 1.0.
- **One shape at a time.** When a shared shape changes — a bridge type, an
  event, a file layout, a storage path — every caller changes with it. No
  compatibility flags, silent defaults for renamed fields, or decoders for old
  data.
- **Desktop-only code is compiled out of phones.** One condition gates the
  import pipeline, identification and the catalog clients, and the same
  condition gates their dependencies, so a phone build carries none of them.
- **Visibility.** Every crate denies unreachable `pub` items and dead code;
  test-only code is gated to tests, never allowed to rot.
- **Files.** A source file stays under 1,000 lines, split along its domain's
  own boundaries.
- **No real names.** Durable artifacts use placeholder artist, album and song
  names.
- **bae is lowercase** in everything a person reads.
- **Strings.** Every user-facing string lives in a catalog, translated into
  every shipped locale in the change that adds it. Core and the bridge emit
  keys and data, never prose.
- **Telemetry is typed.** What leaves a device is a closed catalog of events
  with typed fields. A string can't be a field, so a name, path or title can't
  leak. Events from one installation share an id bae makes once and keeps in
  `~/.bae/install-id`, never coven's device id, and each launch adds an id of
  its own. The baeium edition sends none.
- **Errors keep their cause.** A failure carries its typed reason to the
  person who can act on it; nothing defaults over an error. A logged error
  keeps its whole chain of causes and its stack.
- **Bugs reach the crash reporter.** A bug core finds — code that broke its
  own guarantee — is logged with its causes and handed to the platform's crash
  reporter as a non-fatal event naming the operation, through the reporter
  each app hands core at launch. Crash reporting itself stays in the apps.
- **Logs stay in the platform's log.** Core's and each app's log lines go to
  the platform's own log on the device: the unified log on macOS and iOS,
  logcat on Android, ETW on Windows, the systemd journal on Linux. A
  development (Debug) build of the macOS or iOS app has the unified log save
  every level of its lines, so a running build is read after the fact with
  `log show`; a release keeps the system's defaults, which save default,
  error and fault lines and drop the rest, so bae doesn't fill a person's log
  with its detail. A desktop app whose output is a terminal — run from one —
  also prints its lines there; launched from Finder, the Dock or a desktop
  launcher, it has no terminal and formats no line for one.
- **Reporting needs its settings.** A bae-edition build missing a telemetry
  or crash-reporting setting turns that reporting off and logs a warning
  naming what's missing.

### 11.3 Checks

A pre-commit hook formats and lints the changed crates. CI runs, on macOS,
iOS, Android, Linux and Windows: formatting, the file-size limit, visibility
and ownership boundaries, clippy for each feature set, dead-code and
unused-dependency checks, cargo deny, docs, every crate's tests, the string
catalogs, the Avalonia smoke test, the macOS builds and tests and the iOS
builds for both editions, and Swift lint and format.

---
## Appendix A. Schema

The schema is `bae-core/migrations/001_initial.sql`, which coven applies as
migration 1. Every table is `STRICT`. "Syncs" means coven syncs its rows
(Appendix B); every other table stays on the device.

bae gives coven the collation `natural_order`, which coven registers on every
connection to a library: it orders text the way a person reads folder names
("Disc 2" before "Disc 10", case aside, then by bytes, so two different
strings never tie). It gives the same answer on every device, and it carries
a version, 1: a change to what it compares bumps the version, and coven
rebuilds the indexes that use it when the library next opens for writing.

It also gives coven the function `bae_catalog_rank`, a catalog's place in the
order surfaces list catalogs (MusicBrainz, Discogs, then the rest by name),
which indexes order records by, and bae's other SQL functions; each is
deterministic, with a version that works the same way.

### A.1 The library

All sync.

| Table | One row per |
|---|---|
| `artists` | artist: name, folded name, sort name, MusicBrainz and Discogs ids |
| `artist_credits` | another name a credit prints for an artist, keyed by the artist and the folded name (§2.3) |
| `artist_merges` | artist absorbed into another (§2.3); the view `merged_artist_survivors` follows each chain to the artist it shows as |
| `artist_images` | artist's picture, keyed by the artist: the coven file it references (`file`), with an index by it, its type, size in pixels and source |
| `albums` | album: title, year, compilation flag |
| `album_artists` | album's credit, one per place in its list, from position 0: its artist, the other name it prints (none: the artist's own) and what follows it |
| `works` | MusicBrainz work |
| `work_artists` | work's credited artist, in order |
| `work_parts` | work inside a larger work, in order |
| `releases` | pressing: its album and place in the album's order (§2.1), name, year, labels, barcodes, country or region, media, status, packaging, Discogs details, source folder name, album loudness and peak |
| `covers` | release's cover, keyed by the release: the coven file it references (`file`), with an index by it, its type, size in pixels and source |
| `release_files` | file of a release: name, size, type, the audio facts the scan read, and the coven file it references (`file`), none while its release is local here or on another device, with an index by it; with an index of each release's files in `natural_order` of their names, id breaking a tie, the order every list of a release's files shows, read from the index; and by size and by content type, name and id breaking a tie, the other orders a release's file table sorts by, each read from its index forward or backward |
| `release_artist_roles` | credit on a release, as the catalog stated it |
| `release_records` | catalog record of a release: pressing or album, key, album key, URL; with an index of each release's records in the order surfaces list catalogs, which the release reads them in |
| `release_mediums` | a release's medium: its position, carrier and number of sides (§2.2) |
| `tracks` | track: title, place (its medium, side and number, all required), length (required: the import measures every track's audio), playing order |
| `track_artists` | track's own credit, one per place in its list, from position 0, as an album's |
| `track_artist_roles` | credit on a track, as the catalog stated it |
| `track_works` | work a track performs, in order |
| `audio_formats` | track's format, pregap, loudness and peak |
| `audio_format_segments` | span of a file a track reads: pregap or main, samples and bytes |
| `audio_segment_scrub_points` | frames a seek inside a segment can start its decode at, one row per segment that has any, keyed by the segment: each frame's first sample, its byte, and the first sample a decode from it plays exactly, packed in sample order in one blob; for a FLAC whose seektable is missing or sparser than the points' 10-second spacing, and for every MP3 (§3.7) |

Beside them, device-local tables:

| Table | One row per |
|---|---|
| `local_release_files` | the original this device holds of a release file: its path and modification time (§4.2), with an index by path. Importing writes it; making the release local again rewrites it to the copy; moving to the cloud leaves it |
| `local_release_origin` | release this device imported, keyed by the release: the hash of the folder's files it read (§2.7), unique, so one hash is one release here. Importing writes it in the release's own transaction; deleting the release deletes it |
| `artist_shown_as` | artist, keyed by the artist: the artist it shows as (§2.3) |
| `composer_work` | work and artist shown credited on it |
| `composer_release` | release and artist shown credited on a work one of its tracks performs |
| `composer_unlinked_credit` | catalog role credit none of its artist's works links (§2.4), keyed by the credit: the artist it shows as and its release |
| `composer_listing` | artist the composer list shows (§2.4): its name and its counts of works, releases and unlinked credits, with an index for each of the list's sorts |
| `album_listing` | album the grid shows (§2.1), keyed by the album: its title, year and date added, and its credits' sort names and ids in credit order, with an index for each of the grid's sorts |
| `album_artist_link` | album the grid shows and artist its credits show |
| `artist_listing` | artist the artist list shows (§2.1): its name, its sort name (else name) and how many shown albums credit it, with an index for each of the list's sorts and the grid's grouping |
| `release_listing` | release, keyed by the release: what the Storage Manager (§4.2) sorts and filters it by — its album's title and credit keys, its media, its files' count and size, and whether it is uploaded — with an index for each sort, alone and under the uploaded filter; how many of its files this device keeps pinned and whole, with an index of the uploaded releases not wholly pinned; and how many of its files wait in this device's upload queue and the place of the first, with an index of the releases uploading in queue order, which the import list's Done tab reads (§3.2); whether the download queue holds it waiting or downloading; and where it sorts by storage, worked out from those |
| `release_listing_totals` | uploaded or not: how many releases are, and their files' total size |
| `album_search`, `track_search`, `work_search`, `artist_search`, `artist_credit_search` | album title, track title, work title, artist name and sort name, or other printed name, lowercased, in an FTS5 trigram index library search (§2.3) matches against, beside the searched row's key and, for a track or a work, its title to order by |
| `album_search_entry`, `track_search_entry`, `work_search_entry`, `artist_search_entry`, `artist_credit_search_entry` | searched row, keyed by its own key: the integer its search row is found by |

Local triggers write every table here after the first two in the same write as the library rows they
come from, on this device or applied from another, so they always match those
rows. A release's pinned count follows the pins coven's `coven_pins` lists as
kept the same way, in the transaction that changes a pin or its kept copy,
whether this device's own write, another device's applied, or coven's as a
file is pinned, downloads, uploads or is unpinned. A write applied from
another device can hold a row before a row it references; the triggers count
a row once what it references is there. An
album's listing row is written only when one of its values changes, and its
releases' rows only when its title or credits do, so a release added to an
album, even a box set of thousands, rewrites no other release's row.

Whether a release is local or uploaded is not a column: it is whether every
one of its files references a coven file that coven records as stored.

### A.2 People

All sync (§5), except `local_history`.

| Table | One row per |
|---|---|
| `people` | person, keyed by their `MemberId`: name, private circle, history setting (private or household), and the coven file of their avatar (`avatar_id`), with an index by it |
| `ratings` | rating by a person of an album or of a track, one to five stars |
| `history` | play: person, track, release, when it started, how much was played, audience |
| `playlists` | playlist: whose it is, its name and its audience |
| `playlist_entries` | place in a playlist, keyed by the playlist and a position id: the track there and its position |
| `local_history` | play on this device before the person had a profile: track, release, when it started, how much was played (§5.3) |

### A.3 Playback

| Table | Holds |
|---|---|
| `playback_queue` | the one row of this device's queue: repeat, what's playing from, whether it's shuffled and which of its rows is playing (§7.1) |
| `playback_manual` | Up Next entry, in order: the track it plays |
| `playback_context` | row of what's playing from, in play order: its track, held once, and its place before shuffle |
| `playback_state` | the one row of what this device was playing (§7.8) |

### A.4 Watched folders and scans

| Table | Holds |
|---|---|
| `watched_import_folders` | watched folders, in order |
| `release_grouping`, `release_grouping_member` | folders read as one release, or kept apart, and by whom (§3.1) |
| `skipped_import_candidates` | candidates the person dismissed |
| `folder_discovery` | when each folder was first seen, and its date |
| `folder_scan_generation_sequence`, `folder_scan_roots`, `folder_scan_directory` | each watched folder's last scan and the directories it saw; the scans' progress reads the roots in their watched folders' order, by the index of their positions |
| `scan_candidate`, `scan_candidate_file`, `scan_candidate_part` | the candidates a scan found, their files and their folders, with an index on the folder each candidate reads its files from |
| `scan_candidate_tag_snapshot`, `scan_candidate_file_tag` | the tags read from a candidate's audio, and its embedded cover |
| `scan_skipped_file`, `scan_skipped_tags` | a broken folder's files as the scan read them, and what its tags call it |
| `scan_sidecar`, `scan_sidecar_file` | files under a folder no release owns |
| `scan_cue_sheet`, `scan_cue_track`, `scan_cue_index`, `scan_sheet_audio_file` | parsed CUE sheets and the audio each `FILE` line resolved to |

### A.5 Import candidates

Keyed by the candidate's content hash.

| Table | Holds |
|---|---|
| `import_candidate_state`, `import_candidate_folder` | the candidate, its revisions, and where it was found, with an index on the candidates a file decision was made for, whose decisions a scan reads before it walks, and one on the folders they were found in, whose candidates a watched folder's removal forgets by the range of folders below it |
| `import_candidate_edit`, `import_candidate_album_artist_assignment`, `import_candidate_track`, `import_candidate_track_artist_assignment` | the draft's editable fields, each track's place and listed length (§3.6) |
| `import_candidate_medium` | the draft's mediums |
| `import_candidate_track_role`, `import_candidate_release_role`, `import_candidate_work`, `import_candidate_work_composer`, `import_candidate_work_part`, `import_candidate_track_work` | the draft's roles and works, as values |
| `import_candidate_sheet_disc`, `import_candidate_sheet_reference` | the person's CUE decisions |
| `import_candidate_cover`, `import_candidate_cover_copy`, `import_candidate_remote_cover_asset` | the chosen cover and its fetched bytes; a catalog's cover is never stored without them |
| `import_candidate_cover_option`, `import_candidate_cover_option_copy` | the covers the applied release offers the picker, in order |
| `import_candidate_album_pick` | the library album the person picked for it to join |
| `import_candidate_draft_provenance` | where the draft was read from |
| `import_candidate_release_link`, `import_candidate_release_link_partner`, `import_candidate_release_link_album` | the pressing or album the candidate is linked to |
| `import_candidate_session` | where the pane was, and its last failure |
| `import_candidate_failure`, `import_candidate_artist_identity_conflict` | why the last import failed |

Beside them, keyed by the candidate's key as the import list names it:

| Table | Holds |
|---|---|
| `import_candidate_standing` | where a candidate something is running for stands, and the only place that is held: its identification queued, running, writing its answer (reading the pressing it picked unattended first, and whether Import was asked of that answer) or failed to write it, for the files it names; its import queued, running or writing its release. Each change is a write its owner waits for — the queue admitting and letting go, the run starting, answering and ending, the import's steps — and a change that cannot be written fails what asked for it; a change no one waits on is reported as a bug. A candidate's identification ends in the write that removes the candidate, reads it with other files or combines it into another release, so a folder removed and found again starts with none, and in the write of a person's decision about it — a pick, its files' tags, a cleared draft, a linked album, a skip — or of an import's claim, so no read sees the decision beside the identification it answers. A second import of a candidate is refused while one stands. The import list, the pane and the per-candidate runtime read it with the rows, so each row's state (§3.2) and the filter's counts come from one read; the count of imports in flight the list draws is its rows an import holds, counted in the runtime's read. A stored verdict ends the run it answers in its own transaction, a release ends its import in its own, and a failed import's failure ends it in the failure's. How far a run or an import has got is not stored. No row outlives the process: the table is emptied as the library opens |
| `import_identification_batch` | the identifications counted as one batch for the count the list draws: each candidate admitted since the batch opened, and whether its identification has ended. Each change to an identification moves it in that change's own write; the batch's rows go once every one has ended, and the table is emptied as the library opens |

### A.6 Identification

| Table | Holds |
|---|---|
| `import_candidate_signals`, `import_candidate_signal_value`, `import_candidate_text_line` | the clues read off a candidate (§3.3) |
| `import_candidate_lookup_choices`, `import_candidate_chosen_catalog`, `import_candidate_discounted_catalog`, `import_candidate_excluded_barcode` | what the person let the lookups use |
| `import_candidate_verdict` | what a run concluded: found, not found, manual only, failed or error; the ledger it showed; a medium conflict |
| `import_candidate_match` and its `_barcode`, `_cover_copy`, `_link`, `_album_link`, `_medium` tables | every release a run returned, with which lookup found it |

### A.7 The import list

| Table | Holds |
|---|---|
| `import_list_placement` | where each row of the import list sits (§3.2): one row per release the queue lists and one per header of a group on Found, with its tab, its state on Found and why it needs the person (how many pressings, what the folder's files say, how bae broke), the group listing it, the date and path it sorts by, the text it shows and that text case folded for the search, and what its commands are decided from: whether it can be acted on, whether its last import failed, whether its draft would import, what its lookup came to, whether a grouping reads it; for a header, whether its folder is offered to be read as one release. Each is worked out from the tables above, which no index reaches, so triggers write it in the same write that changes any of them: no read sees it behind its tables. One index per order serves each tab's windows, and for a list folding groups three partial ones per order: the headers and the rows outside every group, the headers alone, and each group's members |
| `import_list_search` | each release's shown title and artists, case folded as the search folds what is typed, in an FTS5 trigram index under its placement row's number, kept by triggers on the placement in the write that moves a row: what the list's search finds its rows by (§3.2) |
| `import_list_tab_count`, `import_list_entry_count`, `import_list_group_entry_count` | how many placement rows each tab holds of each kind, how many of Found's releases each filter entry holds, and how many of each group's releases each entry holds, kept by triggers on the placement in the write that moves a row, so the list's counts read these and not the placement |
| `import_list` | each import list a surface has open: its tab, filter entry, search text and order, where its view opened (its first selected row when the view was set, or its top), and the revisions of its view and its selection, which every change moves so a surface can wait for the read that reflects one |
| `import_list_collapsed` | the groups a list folds shut; one goes with its group's header |
| `import_list_selection` | the rows a list has selected: only rows it shows, since the write that stops it showing one takes it out |

No list outlives the process that opened it: the import service empties
`import_list` and the tables hanging off it as the library opens, with the
standings.

### A.8 Catalog releases

Device-local: any device can fetch a release. A new fetch replaces every row
under it.

| Table | Holds |
|---|---|
| `source_release` | one fetched catalog release and every fact an import reads from it |
| `source_release_album_artist`, `source_release_role`, `source_release_entry_credit`, `source_release_entry_role` | its credits |
| `source_release_medium`, `source_release_medium_disc`, `source_release_entry`, `source_release_format` | its mediums, each with its carrier where stated and the Disc IDs registered against it, their tracklists, and its format entries |
| `source_release_work`, `source_release_work_composer` | its MusicBrainz works |
| `source_release_link`, `source_release_album_link`, `source_release_record`, `release_group_album_link` | what it says it is on other catalogs |
| `source_release_cover`, `source_release_cover_copy`, `source_release_archive_group` | the images it offers a picker |
| `source_release_unfetched` | linked documents the fetch didn't get, and why: the source failed, Discogs had no key, or a run's read stopped short of it (the release's own page, where the run read it from a Disc ID answer) |

### A.9 Transfer queues

| Table | Holds |
|---|---|
| `release_queues` | whether the pin queue and the export and save queue are paused |
| `download_queue` | release queued to pin, in the order queued: queued, downloading, or failed with its error (§4.3) |
| `output_queue` | release queued to export or save, in the order queued: its folder, export or save with the preset captured whole, and queued, writing, or failed with its error (§4.6) |

No transfer outlives the process that ran it, so the library opens with every
queue empty and running.

---

## Appendix B. Synced tables

From `bae_core::sync::synced_tables`. *Shared key* rows take an id computed
from their fact (§2.6); *independent* rows a fresh id. Every synced table's
rows reach every device that can read their audience: a local release's rows
too (§4.2). *Audience* is the store unless the table names one (§5.3); *owner*
tables are changed only by their author's devices (§5.5). *File* names the
column that references a coven file, a trailing reference, and the namespace
its files are in. `local_release_files`,
`local_release_origin`, `local_history`, the import tables (A.4–A.8),
`import_candidate_standing` and `import_identification_batch` among them, and
the transfer queues (A.9) are not declared, so they never sync.

| Table | Identity | Audience | Owner | File |
|---|---|---|---|---|
| `artists` | shared key | store | | |
| `artist_credits` | shared key (artist, folded name) | store | | |
| `artist_merges` | shared key | store | | |
| `albums` | shared key | store | | |
| `album_artists` | shared key | store | | |
| `releases` | independent | store | | |
| `release_records` | shared key | store | | |
| `release_mediums` | shared key | store | | |
| `tracks` | independent | store | | |
| `track_artists` | shared key | store | | |
| `works` | shared key | store | | |
| `work_artists` | shared key | store | | |
| `work_parts` | shared key | store | | |
| `track_works` | independent | store | | |
| `release_artist_roles` | independent | store | | |
| `track_artist_roles` | independent | store | | |
| `release_files` | independent | store | | `file`, in `release_files`: the person's own; set on moving to the cloud or a cloud import; read on first use; cleared to return to local paths |
| `audio_formats` | independent | store | | |
| `audio_format_segments` | independent | store | | |
| `audio_segment_scrub_points` | independent: its segment | store | | |
| `covers` | shared key: the release | store | | `file`, in `covers`: bae's own; read on first use |
| `artist_images` | shared key: the artist | store | | `file`, in `artist_images`: bae's own; read on first use; shared by two merged artists |
| `people` | shared key: the person's `MemberId` | store | yes | `avatar_id`, in `avatars`: bae's own; read on first use |
| `ratings` | shared key: the person, and the album or track | store | yes | |
| `history` | independent, one per play | per row: the person's circle or the store, by their setting | yes | |
| `playlists` | independent | per row: the person's circle or the store | yes | |
| `playlist_entries` | shared key: the playlist and a position id | its playlist's | yes | |

---

## Appendix C. Settings

Each library's settings are `preferences.toml` in
`~/.bae/libraries/<library id>/`, apart from coven's store (§6.2). They are
device-local. Every key is written; a file
missing one fails to load, and only a missing file means every default.
Unknown keys are ignored. The file can be edited by hand; enum values use
their Rust spelling, such as `replay_gain_mode = "Album"`. TOML has no null
value, so `discogs = "NotConfigured"` means no stored key; the other values
are `"Unvalidated"`, `"Valid"`, and `"Rejected"`.
The [default preferences file](../bae-core/test-fixtures/preferences/default.toml)
shows the complete TOML layout.

| Setting | Default | What it decides |
|---|---|---|
| `discogs` | none | the stored Discogs key's state: unvalidated, valid or rejected; the key is in the keyring |
| `replay_gain_mode` | off | loudness: off, track or album (§7.5) |
| `save_presets` | FLAC 24-bit, MP3 320 | the save presets (§4.6) |
| `default_track_save_preset`, `default_release_save_preset` | FLAC | which preset a save starts with |
| `pause_between_sides` | on | pausing between sides and discs (§7.3) |
| `side_pause_countdown` | off | when such a pause ends on its own |
| `max_concurrent_uploads`, `max_concurrent_downloads` | 3 | files moved at once, 1 to 8; adding this device, joining, and a restore or reset of this device's copy read the library from storage under the download limit, a library new to the device at the default |
| `show_remaining_time` | off | the seek bar counts down |
| `library_full_width` | off | the library page spans the window |
| `verify_decode_on_import` | on | the decode check at import (§3.7) |
| `import_storage.cloud`, `import_storage.pinned` | on, on | where an import goes, and whether it stays here |
| `identification.automatic` | on | identifying found releases on their own |
| `identification.import_when_identified` | off | importing what any run picks unattended |
| `identification.catalogs` | both on | which catalogs are asked |
| `cast_enabled` | off | casting to receivers |
| `cast_transcode_format` | MP3 | what a receiver gets when a track needs converting |
| `mcp.enabled`, `mcp.port` | off, 47777 | the MCP server; its token is in the keyring |
| `subsonic.enabled`, `.port`, `.username`, `.bind_address` | off, 4533, none, 127.0.0.1 | the Subsonic server; its password is in the keyring |

A save preset holds a codec (FLAC, WAV or AIFF at 16, 24 or 32 bits; MP3, AAC
or Opus at a bitrate), a filename pattern of tokens (title, artist, album, year,
track number, disc number, track total), where pregaps go (appended to the
track before, with or without the hidden first-track audio; left out; or one
file with a CUE sheet), and whether it embeds the cover.

Platform settings, such as the appearance and restoring playback on launch,
live in each app. The apps start in the system's light or dark mode with the
Pink accent and the Plum background tone.

---

## Appendix D. Subsonic endpoints

Each is served at `/rest/<name>` and `/rest/<name>.view`.

| Endpoint | Returns |
|---|---|
| `ping` | an empty success |
| `getLicense` | a valid license |
| `getMusicFolders` | one folder holding everything |
| `getArtists`, `getIndexes` | every artist, by first letter |
| `getArtist` | an artist and their releases |
| `getAlbum` | a release and its tracks |
| `getAlbumList2` | releases by `alphabeticalByName`, `newest`, `byYear` or `random`; other types return none |
| `getSong` | one track |
| `search3` | artists, releases and tracks; an empty query returns the whole library, paged |
| `stream` | a track's audio (§8.1) |
| `getCoverArt` | a release's cover or an artist's image |
| `scrobble` | success, recording nothing |

---

## Appendix E. MCP tools

| Tool | Does |
|---|---|
| `config_get` | read the library's automation settings |
| `watched_folders_list`, `watched_folder_add`, `watched_folder_remove`, `watched_folders_scan` | manage and scan watched folders |
| `import_candidates_list`, `import_candidate_get` | read the import list and a candidate |
| `import_candidate_skip_set` | skip or restore a candidate |
| `import_search` | search the catalogs |
| `import_candidate_release_select` | link a candidate to a catalog release and read its draft from it |
| `import_candidate_file_tags_read` | read a candidate's draft from its tags, keeping its link |
| `import_candidate_edit_field_set` | type one album field |
| `import_candidate_labels_set` | set its labels and catalog numbers |
| `import_candidate_barcodes_set` | set its barcodes |
| `import_candidate_album_set` | pick the library album it joins, or leave it to its records |
| `import_candidate_pressing_fact_set` | choose its area, media, status, packaging or Discogs details |
| `import_candidate_cover_set` | choose its cover |
| `import_start` | import a candidate as stored, or once the pick its run is reading is stored |
| `release_detail_get` | read a library release |
| `release_export` | queue an export of a release's files |
| `release_storage_action` | move to the cloud, pin, unpin, make local, or cancel |
| `output_status` | read the export queue |
| `release_reidentify` | re-identify a release as a catalog release: apply its every value in an edit session of its own and save it, records and album with it, into the album its records name or a library album given over them |
| `release_album_set` | move a release into a library album, which keeps its own title and artists, as the editor's album picker does |
| `release_metadata_update` | apply a metadata edit: the album fields, the pressing, and each track's title and artists (a track's place is the source's, not edited) |
| `library_search` | search the library |

---

## Appendix F. Bridge types

The `Bridge*` types live in `bae-bridge/src/types/`, by area:

| File | Types for |
|---|---|
| `library_playback.rs` | albums, releases, tracks, files, audio formats, covers and galleries, records, catalogs, cloud providers, libraries |
| `configuration/library.rs` | search results, artists, composers and works, the storage table, the queue, sync status and pending records |
| `configuration/services.rs` | settings, the MCP and Subsonic servers, receivers, cloud-home setup |
| `configuration/settings.rs` | sorting, restore codes, people and devices, save presets, errors |
| `playback_values.rs` | the playing track, position, side-pause prompt, media controls, preview |
| `events_and_transfers.rs` | UI events, uploads, downloads, exports and saves |
| `editing.rs` | release detail and edits, artist credits, the mapping table, edit validation |
| `pressing.rs` | pressing facts: area, media, labels, status, packaging, Discogs details |
| `storage_inspector.rs` | the storage inspector's files and transfers |
| `live_results.rs` | windows and sections of live lists |
| `import/candidate.rs`, `import/grouping.rs` | watched folders, candidate files, CUE binding, import steps, grouped folders |
| `import/identify.rs` | clues, lookups, the run's ledger, identify state |
| `import/list.rs` | the import list, its filter, the candidate pane and its commands |
| `import/search.rs` | catalog search results, pressings and album cards |
| `import/triage.rs` | candidate rows, states, badges, selection, matched pressings |
| `cover.rs` | catalog cover galleries |

The `import` and `cover` types, and the conversions in `conversion/`, exist
only in desktop builds.

---

## Appendix G. Errors and states

### G.1 Error categories

A `BridgeError` is `Cancelled` (show nothing), `NotFound` (a library), or
`Diagnostic` with one of these categories and a detail:

| Category | The person |
|---|---|
| `Database`, `Config`, `Internal` | reports it |
| `SyncUpdateRequired` | updates the app |
| `DeviceResetRequired` | resets the device, after its warning (§6.4) |
| `Import`, `ImportData` | reads the detail; the source couldn't be read |
| `CandidateImportInProgress`, `CandidateBeingIdentified` | waits |
| `MetadataTrackCount` | picks another release: it lists another number of tracks |
| `TiedMediums` | picks another release or uses the file metadata: several of its discs fit the folder alike, and the line names them |
| `ReleaseChangedSinceOpened` | cancels to see the release as it is now: it changed after the editor opened it (§3.8) |
| `GroupingBlocked` | combines again, separates, or waits for a download |
| `Export`, `Save` | reads the detail |
| `CloudSetup`, `StorageCheck` | changes what the setup, or the check of the storage it failed, names |
| `S3VersioningUnknown`, `S3VersioningUnsupported` | uses a bucket whose versioning coven can read and work with |
| `StorageLocationChangeUnsupported` | keeps the library's storage location; it can't move |
| `S3Prefix` | changes the key prefix: it has two slashes in a row, or the folder the line names holds a character other than a–z, A–Z, a digit, `-` or `_`, or runs over 255 characters |
| `DeviceIdentityMissing` | creates, joins or restores |
| `Credentials` | fixes the cloud settings |
| `Network` | retries |
| `Keyring`, `KeyringLocked` | fixes the keyring, or unlocks and is asked again |
| `KeychainEntitlementMissing` | installs a build of bae signed with its keychain entitlement; retrying or unlocking doesn't help |
| `Membership` | retries the membership step, with the advice it carries |
| `InviteJoining` | leaves the invite: cancelling it came after the inviting device began letting its person in (§5.6) |
| `AirPlayUnsupported`, `AirPlayUnreadableSettings` | uses another receiver |
| `ReplacementUnfinished` | finishes the reset or the restore of this device's copy that stopped partway: the one the category names, never the other (§6.4) |

### G.2 Why a found release isn't picked

| Reason | §3.5 |
|---|---|
| nothing found | no release could be the folder's |
| several | several pressings |
| unread document | a record couldn't be read in full |
| tracks unknown | the release lists no tracks to count |
| medium disagrees | the folder's files rule out every release found |

### G.3 Grouping blocks

A release read from several folders can't be worked on when a folder it reads
changed or is gone, its parent folder's files go with another release or are
claimed by several, or a download into that folder is still running.

---

## Appendix H. Catalogs and vocabularies

**Catalogs.** MusicBrainz and Discogs answer lookups. AllMusic, Apple Music,
Bandcamp, Deezer, Genius, Musik-Sammler, Rate Your Music, Spotify and Wikidata
are reached by the links those two state, and appear as records (§2.5).
MusicBrainz covers come from the Cover Art Archive.

**Pressing facts.** Each is a closed list keyed as stored:

- **Status:** official, promotion, bootleg, pseudo-release, withdrawn,
  expunged, cancelled.
- **Packaging:** jewel case, slim jewel case, digipak, cardboard sleeve, keep
  case, unpackaged, cassette case, book, fatbox, snap case, gatefold cover,
  discbox slider, super jewel box, digibook, plastic sleeve, box, slidepack,
  snap pack, metal tin, longbox, clamshell case, digifile, slipcase, other.
- **Medium:** CD, SACD, DVD, HD DVD, Blu-ray, vinyl, shellac, acetate, cassette,
  DCC, DAT, cartridge, reel-to-reel, digital, MiniDisc, LaserDisc, CDV, Video CD,
  video tape, flash memory, floppy, cylinder, DualDisc, VinylDisc, DVDplus, and
  more in `crate::pressing::Medium`.
- **Area:** an ISO 3166-1 country code, or a named region no code covers.
- **Discogs details:** what Discogs states about a pressing that no other
  field holds.

**Audio codecs.** Lossless, stating a bit depth: FLAC, APE, ALAC, PCM, WavPack,
DSD. Lossy, stating a bitrate: MP3, AAC, Opus, Vorbis.
