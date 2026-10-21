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

| | Desktop (macOS; Windows and Linux) | Phone (iOS, Android) |
|---|---|---|
| Watch folders and import | yes | no |
| Identify on MusicBrainz and Discogs | yes | no |
| Edit a release's metadata | yes | no |
| Browse, search, play | yes | yes |
| Pin a release for offline play | yes | yes |
| Cast to AirPlay, Chromecast, UPnP | yes | yes (iOS finds no UPnP devices, §7.7) |
| Subsonic and MCP servers | yes | no |
| Export and save files | yes | no |

The phone builds of bae-core leave the import pipeline, identification,
catalog lookups and metadata editing out entirely (§11.2).

The macOS app is the full desktop app. On Windows and Linux, bae-avalonia
holds the desktop app's start-up, update and packaging machinery and one
window; the desktop views are ported from the macOS app later.

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
numbers, barcode, country, media, status and packaging.

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

An album names one *primary* release: the one whose cover it shows and that
opens by default. With none set, the first release serves.

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

Each track records:

- its playing order in the release, counted from zero across every side;
- the side or disc it sits on, where the pressing has them, which playback
  pauses between (§7.3);
- its audio format: content type, sample rate, bits per sample, channels;
- its loudness, measured at import (§3.7).

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
| An album's artists | `albums.artist_id`, then `album_artists` | who the album is by, in credit order |
| A track's artists | `track_artists` | who the track is by, in credit order |
| A release's roles | `release_artist_roles` | who did what on the release, as the catalog stated it |
| A track's roles | `track_artist_roles` | who did what on the track, as the catalog stated it |
| A work's artists | `work_artists` | who wrote or arranged the work |

An artist row holds a name, a sort name, and the artist's MusicBrainz and
Discogs ids where known. It may carry one picture, bae's own copy (§4.1).

Two artists can be merged into one, and only one way: on the import page, when
an import's credit shows that two library artists are the same person and the
person says so (§3.7). That is the one place bae shows both artists beside the
evidence that they are one; bae has no other merge action.

Merging records that one artist was absorbed into the other; both rows stay, so
a credit another device gave the absorbed artist still resolves. Every read
shows the survivor. A chain of merges shows its last artist, and a loop, left by
two devices merging a pair in opposite directions, shows the artist with the
smallest id.

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

### 2.5 Catalog records

A release keeps one *record* per catalog that describes it. A record names
either the pressing itself or only its album:

- MusicBrainz and Discogs records usually name the pressing, with the album
  they file it under: a MusicBrainz release group, a Discogs master.
- Other catalogs, such as Wikidata or a streaming service, are reached by
  links. Their records name the album's page.

One record is the one the release's metadata was read from (`draft_catalog`),
or the release says its metadata came from the folder's own tags. Re-reading
a release's metadata reads that source again (§3.6).

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
| album credit | the album and the artist |
| track credit | the track and the artist |
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
  MusicBrainz group of an album already in the library joins that album (§3.7).

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

Nothing is refused as already in the library. A release keyed by its contents
would make a second import replace the first, and with it the tracks that
ratings, history and playlists point at.

**The folder's hash.** Each release records a hash of its folder's contents
(`releases.content_hash`). The hash covers every file of the release: each
one's path relative to the release folder, its size and its modification time,
sorted, so moving the folder doesn't change its hash. It is never a key and
never refuses anything. It does one job: it tells the import list that the
household probably already has a folder (§3.2).

"Probably", because the hash reads no bytes: two folders can hash alike and
still differ. A copy to another disk usually changes the modification times,
so the same rip copied elsewhere usually hashes differently.

To have bae read edited tags again, Ana resets the release's metadata from its
source (§3.7); she doesn't import it again.

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

Ana watches `~/Music`. bae walks it a directory at a time and reports
candidates as it finds them. A *candidate* is a release the scan found, not yet
imported:

- **A folder with audio directly in it** is one release. Ana's
  `Artist Name - Album Title (1979)/` with its eleven FLAC files, CUE sheet and
  log is one candidate.
- **A folder whose children hold the audio** is read one of two ways: as one
  release, or as separate releases. Carol's `Box Set Title/` holds `CD1/` and
  `CD2/`. Every part's name carries a number, and the numbers are exactly 1 and
  2, so the scan reads the folder as one release with two discs.
- **Anything else** stays separate: a part with no number, a gap in the
  numbers, numbers that don't start at one, or audio sitting in the folder
  beside its parts. A `Scans/` folder beside `CD1/` and `CD2/` holds no audio,
  so it is not a part; it travels with the release.

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

A watched folder on a network volume has no watch: a network mount reports
only the changes this machine makes, so a watch would claim a reliability the
mount can't give. bae checks such a folder every 15 minutes instead, and the
import list says so.

**What a scan stores.** Everything a scan reads is device-local: the
candidates, their files and the role each plays (audio, track sheet, artwork,
damaged image, document, other), the parsed CUE sheets, the tags each audio file carried, and
when each folder was first seen. A scan writes as it finds; completing removes
what it didn't see, in the same transaction that marks it complete. A failed or
interrupted scan keeps both old and new entries.

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
| Pending | no release carries the folder's hash, and not skipped; including a failed import |
| Done | some library release carries the folder's hash; ordered by upload, what is moving first |
| Skipped | dismissed by the person, or broken |

Each Pending row is in exactly one state. The first that applies wins:

1. **Importing**: an import is queued or running.
2. **Identifying**: a lookup is queued, running, or writing its answer.
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
identified, unmatched, or not looked up.

**Done.** A folder is Done when some release in the library, from any device,
carries its content hash (§2.7). Its row lists every release that does. Ben
copies Ana's rip folder to his laptop, keeping the modification times. His scan
finds it, and Ana's release carries its hash, so the folder lands in Done
beside her release: Ben probably already has it.

The row says so only for a folder none of its releases was imported from. Each
device records, for itself alone, which folder each of its imports read
(`local_release_origin`), so Ana's own folder lists her release without the
hint, uploaded or not.

Done offers no re-import. A second copy comes from importing another folder,
such as a copy whose modification times changed.

**Skipped.** A broken folder's row looks like a Found row: the folder's own
cover when an image in it decodes, the album title and artist its tags state
or else the folder's name, and under them why it can't be imported, such as
"2 audio files are damaged". Selecting it opens it the way a Found release
opens, read-only: its images, then every other file with each damaged one
marked and why. Where a release's Import would be, the pane states the reason.
A playable track in it can still be auditioned.

The person can select any number of rows. The import list in core owns the
selection, not a surface, and it only ever holds rows the list shows: the read
that stops showing a selected row — another tab, filter or search, or a row
whose state moves it out of the filter, such as one that starts importing
under Identified — takes it out of the selection before the list is
delivered, and a row shown again comes back unselected. The pane a selection
opens reads only that selection, so with nothing shown selected it shows
nothing selected. A bulk action runs in core over the selected rows that offer
it, in the order the list shows them. A broken folder offers no action, so
selected among others it counts toward the selection but holds back combining
it.

### 3.3 Clues in the files

Identification reads clues from the files, then looks them up. A clue is one
of:

- **A disc ID**, computed from a CUE sheet or a rip log's table of contents.
  bae hashes the track offsets the way MusicBrainz does; it never reads a
  drive.
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

Ana's folder yields a disc ID from her CUE sheet, the barcode from her scan of
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
left unhashed: a disc ID of a vinyl rip names no disc.

### 3.4 Looking up

bae looks the clues up on MusicBrainz and Discogs. Each catalog answers for
itself and at its own pace:

| Clue | Asked of |
|---|---|
| disc ID | MusicBrainz |
| ISRCs | MusicBrainz |
| barcodes | both |
| catalog numbers | both |
| album and artist title | both, only when every clue above came back empty |

Discogs needs an API key; without one it is not asked. Either catalog can be
switched off. MusicBrainz is asked no more than once every 1.2 seconds, under
the rate it allows.

**Rounds.** One lookup is not enough. Ana's disc ID returns three CD
pressings; one of them carries LBL-1001, which the folder also states, so that
number is now worth asking Discogs about. A run therefore goes in rounds:

1. Pool every answer so far and rank it (below).
2. Read each offered release's full document. A search result states less
   than its document: Discogs names only the first label, and no lookup lists
   the tracklist.
3. Ask what came into effect: a catalog number a found release carries that the
   folder states; an offered release's pressing on the catalog that has no
   record of it, by the release its page names there, its barcode, or its
   catalog number under its label.
4. Stop when a round asks nothing new and every offered release is read.

**Pressings and albums.** The two catalogs describe the same object in their
own words. A record from each becomes one *pressing* when what they state
about the physical object agrees: the barcode with or without spaces, the
catalog number with or without a dash, the country, the media. Two records of
one catalog stay two pressings, because that catalog's editors separated them.

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

**Ranking.** Each pressing is scored by what it states against what the folder
states. For each fact, a pressing agrees, says nothing, or disagrees; one that
says nothing never loses to one that disagrees. Which lookup found a pressing
adds no weight. The pressings tied at the top are offered; the rest are set
aside under "N more releases".

Two kinds of pressing never stand:

- **One whose tracklist doesn't fit.** A pressing whose read tracklists all
  list another number of tracks than the folder holds is not a match. Lengths
  are not compared: catalogs round them and count pregaps differently.
- **Look-alikes.** Pressings that would show exactly the same on screen are
  kept once; nobody could choose between them in bae.

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
bae picks it and writes its metadata into her draft. A disc ID often returns
several pressings of one album: an exact clue is not a unique answer, and
choosing between them is the person's.

When the run doesn't pick, the pane says why: nothing found, several
pressings, a record that couldn't be read, a tracklist that couldn't be
checked, or a medium the folder rules out. The person then:

- **picks** one of the offered pressings;
- **searches** by artist and album, catalog number or barcode, with each
  catalog's results landing as they come;
- **links the album** when several pressings of one album are offered and they
  can't tell which is theirs. The link names the album, not a pressing, and the
  fields every offered pressing agrees on fill the draft;
- **keeps their own draft**, from the folder's tags or typed by hand.

The person also decides which clues a run asks about. Ben unchecks a barcode
that belongs to a box set rather than his disc; Carol picks the one catalog
number of thirty that names her pressing. Those choices are stored with the
candidate, and changing them starts the next run.

### 3.6 The draft

A *draft* is the metadata an import will write: the album fields, the pressing
facts, the album's artists, and one track per audio unit. An *audio unit* is
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

Every draft records who wrote it: nobody yet, the folder's tags, an
identification, or the person. Every pane control is stored the moment it's
used, under the candidate's hash, so the person can leave a candidate half
done and come back to it, even after a restart. Where the pane was, such as a
half-typed search, is stored too.

The mapping table pairs each unit with its draft track, showing both the
folder's and the catalog's durations. For a CUE sheet, the person can bind each
`FILE` line to an audio file and say which disc each sheet is, or ignore a
sheet.

Before importing, the person can play a file through a second player, which
pauses the library's playback and resumes it when it ends.

### 3.7 Importing

Importing measures loudness and adds the release to the library. Audio stays
where it is; bae never copies, moves or rewrites it.

Ana clicks Import. bae:

1. **Checks the files** are the ones the scan saw: same paths, sizes and
   modification times. A folder that changed since is refused, to be read
   again.
2. **Reads each file** where it sits, for a cloud import: coven hashes it, and
   checks every later download against that hash. A local import records each
   file's path, size and modification time; its bytes are read when the release
   is uploaded (§4.2).
3. **Measures loudness** by decoding every track: its EBU R128 integrated
   loudness and true peak, and the album's over all its tracks. A near-silent
   track stores no loudness and plays unchanged.
4. **Checks the audio decodes**, in the same pass. A track that fails, or
   decodes short, fails the import rather than failing later at play time.
   This check is a setting, on by default.
5. **Finds the album.** A release whose record's album matches an album in the
   library joins it, even when the library already holds that pressing: the
   import is another copy (§2.7). Otherwise the album takes its id from its
   group (§2.6).
6. **Writes everything in one transaction**: the album; the release, with a
   fresh random id and the folder's hash; its tracks, files, segments,
   credits, works, records and cover. The artists are decided in that
   transaction (§2.3).

Nothing durable is written before the last step, so a cancel before it leaves
the library as it was. Once the write begins, a cancel is refused and the
import completes.

**The cover.** The cover comes from the folder's own image files, an image
embedded in a tag, or a catalog's image (the Cover Art Archive for
MusicBrainz, or Discogs). A catalog's cover, and the Discogs pictures of the
release's new artists, are fetched before the import runs, so the import
itself needs no network. Every stored image is downscaled to at most 600
pixels on a side and stored as JPEG.

**Where it goes.** Each import lands on this device or goes to the cloud, as
the person last chose (§4.2). A cloud import attaches its files in the same
transaction as its rows, which queues their upload, so the choice is never
lost, and needs no connection to record it.

**Automatic identification and import.** By default, each release a scan finds
for the first time is identified on its own, in the background, behind
anything a person asks for. With "Import automatically when identified" on,
any run that picks unattended imports straight away: one bae started on its
own, or one the person started with Identify, a retry, or a changed clue. A
pressing the person picks themselves, from a run's list or their own search,
waits for their Import. Both are settings (Appendix C), and each works without
the other: with automatic identification off, the runs a person starts still
import. Turning import on imports nothing already identified.

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
  can be identified again from its own files; its disc ID comes from its stored
  tracks. The person then names its pressing, whose records replace the old
  ones and whose facts seed its metadata, or says it is no catalog's pressing:
  then it keeps no records and moves to a fresh album.
- **Resetting.** A release's metadata can be re-read from its source: the
  catalog release its record names, or the files' tags if its files are on this
  device.
- **One import at a time.** Imports run one after another on one worker.

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
- **Covers, artist images and avatars: bae's own.** bae makes them, covers and
  artist images downscaled to at most 600 pixels as JPEG, and hands the bytes
  to coven, which keeps them.

A cover belongs to one release, an artist image to one artist, an avatar to one
person. Changing a cover attaches a new file to the release's cover row; coven
never rewrites a stored file, and deletes the old one once nothing refers to it
([coven §17.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#178-deleting-stored-files)).
An image's version is the content hash coven records for its file, so the apps
and Subsonic clients that cache art by version fetch a cover again exactly when
its bytes change.

### 4.2 Local and uploaded releases

A release is *local*, or *uploaded* so other devices can play it.

Ana imports her CD rip to her laptop only. The release is local: its files are
her own, at their path on her laptop. Its rows sync like any release's, so
Ben's phone shows the album, its tracks and its cover, but can't play it. Ana
moves it to the cloud. bae attaches each file to its `release_files` row, and
coven uploads it from where it sits; from then on every device can play it.
Ana's files stay where they are, untouched.

A local release's audio stays on the importing device. Its paths sit in the
device-local table `local_release_files`, one row per file with its path and
modification time, not in the synced rows. Attaching a file to a synced row
queues its upload in the same write
([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-attaching-a-file)),
and uploading is the person's choice. The rows sync anyway, so every device
knows what the household owns, and a rating or playlist entry pointing at a
local release's track points at a row every device can read
([coven §15.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#153-references-stay-readable)).

A local release's cover still uploads, so other devices can show it. coven
queues it as soon as the cover row is written, with or without storage
connected. That upload is bae's own file, not the release's: until its audio is
attached the release reads as local everywhere, and nothing about it reads as
uploading. Waiting files draw no progress bar; a bar appears only while a
file's bytes are moving.

**Uploading attaches the audio.** Moving a release to the cloud attaches every
one of its files in one write, and the same write removes their
`local_release_files` rows. A release is never half attached. Before that
write, bae checks each file still has the size and modification time it
recorded; a file that changed since import refuses the upload. The rows
describe the file as imported, and a changed file would give every device audio
that doesn't match them.

**Detaching keeps local playback.** Two actions take a release's audio back off
its rows. Each clears the attachment of every file and records this device's
path for it in `local_release_files`, in one write, so the release plays from
this device's disk the moment the write lands:

- **Cancelling an upload** while any of its files is still queued puts the
  release back at its original paths. The originals must still be unchanged.
- **Making a release local again** (§4.4) copies its files into a folder the
  person picks, then points the release at the copies.

Other devices again show the release but can't play it. What already reached
storage stays until nothing refers to it; coven then deletes it
([coven §17.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#178-deleting-stored-files)).

- **A cloud import** (§3.7) attaches its files in the same write as its rows.
- **Uploads queue.** Files go up three at a time by default, a setting from
  one to eight. The queue can be paused, and stays paused across restarts; a
  paused upload keeps its place.

### 4.3 Pinning and the cache

An uploaded release can be pinned to stay on a device; otherwise its files are
cached as they play.

Ben's phone holds little. When he plays Ana's album, the chunks he plays are
fetched and kept in the cache. Before a flight he pins it: the phone downloads
every file whole and keeps it regardless of the cache's size.

Each kind of file has its own cache budget on each device, so audio never
pushes the covers out:

| Kind | Budget | Fetched |
|---|---|---|
| Audio and the folder's other files | 20 GiB | as read |
| Covers | 512 MiB | as soon as the row arrives |
| Artist images | 256 MiB | as soon as the row arrives |
| Avatars | 64 MiB | as soon as the row arrives |

Over budget, the least recently used go first. Covers are never pinned; an
evicted one shows a placeholder until fetched again.

Pins download one release at a time from a queue that can be paused,
cancelled and retried. A release becomes pinned only once every file lands. The
queue lives in memory: after a restart it is empty, and a release that wasn't
fully pinned stays as it was. Unpinning moves the files into the cache, still
readable, now evictable.

### 4.4 Making a release local again

Making an uploaded release local again copies its files into a folder the
person picks.

Carol wants her box set off the cloud. She picks a folder on her drive. Every
file is copied there from the cache or storage, and each copy's SHA-256 is
checked against its row's content hash. Then one write detaches every file and
records its new path (§4.2). A cancel or failure before that write removes the
partial copies and leaves the release uploaded. The cover stays attached: it is
bae's own, and every device shows it.

The file names come from the release's rows, which another device may have
written. Each is checked to be a plain relative path before it is joined to
Carol's folder, so no row can make a device write outside the folder chosen.
One bad name copies out nothing at all.

### 4.5 How files sit in storage

Storage holds encrypted objects under random names. Ana's FLAC files are not a
folder anyone can browse in Dropbox: each is one object, encrypted with a key
of its own that travels only inside the row's encrypted writes, at a path coven
fixes when the file is attached
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-how-a-file-is-stored)).
The provider sees no album, artist or file name.

The path is fixed when the file is attached and kept in its row, so renaming an
album never moves a file. A new cover is a new object with a new id. Two
devices replacing one cover then never write the same object, and a device
applying an older write still finds the bytes that write names.

### 4.6 Export and save

Two ways put a release back on disk, from desktop:

- **Export** copies a release's files byte for byte into a folder named after
  its source folder. The copy is staged and moved into place only once every
  file is written.
- **Save** writes new files with a *preset*: a codec, a filename pattern, where
  pregaps go, and whether to embed the cover. A preset can write a release as
  one image with a CUE sheet. FLAC and MP3 presets come built in (Appendix C).

Both run from a queue that can be paused, cancelled and retried. Either reads
files from wherever they are: the person's own path, the cache, or storage.

**Edge cases.**

- **Deleting a release** deletes its rows and coven's copies of its files. The
  person's own files are never deleted.
- **A device with no cloud home** has only local releases; moving to the cloud
  needs one.
- **What disconnecting would cost.** bae counts the uploaded releases this
  device hasn't pinned: those it could no longer play without the cloud.

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
rename. An invite carries the current name, since it expires within a day.
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
  joining, with the name they typed, so every member has a profile. Someone
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
- **An avatar is a file on the `people` row**, bae's own (app-provided),
  downloaded as soon as the row arrives.
- **The app joins `members()`**, who is in, with `people`, how to show them.

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

A new device's first screen offers the two ways in: **Join library**, with an
invite, and **Add device**, with the person's own restore code. Either one
then scans or takes a typed code, and the code decides: an invite joins and a
restore code restores, whichever was chosen. A restore code's screens call
that adding this device: the button is **Add**, and the running restore says
it is adding this device. A code saved in iCloud Keychain, and a restore over
this device's copy (§6.4), say **Restore**. A Mac also offers **Create new
library**.

Dan joins. Ana invites him with his Dropbox email and a role; her laptop shares
the store's folder with his account and shows the invite. On his phone he
chooses **Join library**, scans the invite, types "Dan", and his phone asks to
join. Ana's laptop shows the request and she approves it. His phone then
writes his `people` row while it joins. Everyone's devices show Dan by name.
On S3, Ana makes an access key for Dan instead of sharing with an account. An
invite carries the library's current name and expires within a day.

Removing a person deletes their circle and its rows: their private history and
private playlists. Their household ratings, history and playlists stay,
credited to them.

People lists each person's active devices. Their removed and replaced
devices, whose records coven keeps, sit under them in a **Removed devices**
list that starts closed, each marked removed or replaced, with no actions.

**Adding a device.** Carol's new tablet joins with her restore code alone; no
other device approves it. The code holds Carol's keys, so it already proves the
tablet is hers
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)),
and an approval would strand a person whose other devices are all lost. The
tablet gets the code one of three ways:

- **scanning** the QR code another of her devices shows, after choosing **Add
  device**;
- **iCloud Keychain**, on Apple devices: the first screen lists the restore
  codes coven saved there for libraries not already on this device, and Carol
  picks one; a library the device already holds is opened, not restored. A
  code there is Carol's, not one device's: it stays while she is a member, so
  removing the library from one of her devices leaves it offered there (§6.4),
  and it goes when she leaves the library or deletes it from the list;
- **typing** it in, after choosing **Add device**.

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
also needs Carol's S3 access key. The QR code one of her own devices shows
carries that key beside the code, so scanning it is enough. A typed code asks
for the key separately.

**Where the code shows.** The apps call the restore code the *recovery code*.
Carol adds a device from **Add Device…** under her own row in People, shown
only for a library with a cloud home. Its sheet tells her to choose **Add
device** on the new device, then scan its QR code or type the code it shows;
on S3 it says the QR code carries her key. Settings keeps the code under **Your
recovery code**, for the day she loses every device: hidden until she asks,
then shown in place as coven formats it, to copy or hide, and hidden again
when Settings closes. On S3 that section also says she will need her access
key and secret.

**Removing a device.** Ben's old phone is lost. He removes it from his laptop.
Had he lost every device, Ana, an admin, could remove it for him: admins can
remove any person's device, and everyone else only their own. Removing a device
shows what is left to do at the provider, such as signing that device out of
the account
([coven §14.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#142-provider-access)).
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

**Dropbox and OneDrive setup.** The person signs in through the browser, then
picks the folder the library lives in. On Dropbox that is a shared folder, so
the household can be invited to it; with none yet, bae says to share one in
Dropbox and look again. On OneDrive it is any folder, browsed from the top,
and the person can make a new one there. coven holds the sign-in; bae never
sees its tokens.

**iCloud setup** uses the Apple Account the device is signed in to.

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
- **the files it carries**: `release_files` the person's own, attached when
  the release is uploaded and read on first use; `covers`, `artist_images` and
  the avatars on `people` bae's own, fetched as soon as their rows arrive.

Appendix B lists every declaration. A test holds the declarations to exactly
that list, each table with its identity, audience, owner flag and files, so a
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
title bar on macOS and in the library's top bar on iOS and Android, never a
bar across the content. While bytes move it is a spinner: files this device is sending,
offline copies downloading, or files still arriving from another device. While something needs the person, a sync failure or a pending record,
it shows a small warning symbol, beside the spinner when both hold. With
neither it is absent. A pass alone doesn't spin it: an idle library runs one
every half minute, and the indicator would blink for nothing.

Clicking it opens what sync is doing, a popover on macOS and a sheet on the
phones, updating as it changes. What needs the person comes first, under *Needs
Attention*: the failure's line and each pending record in plain words, with
*Open Sync Settings*, where each record offers its action. Then one row per
thing moving: a pass syncing the library's changes, the uploads ("2
uploading · 10 queued", with the bytes their providers confirmed), and the
offline downloads, which on the phones open the downloads list. Then the files still arriving, one
section per device, "Arriving from Studio Mac", listing the albums by title
and counting other pictures. With nothing to show it says everything is up
to date. *Retry Now* is always there; it retries everything at once, as
pulling to refresh does.

**The status line.** Settings shows one line for sync: "Synced", or "Waiting: 3
items". Opening it lists each thing coven is waiting on, one of its pending
records
([coven §19.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#191-what-the-app-sees)),
in plain words, with an action wherever the person can act. The error behind
a record sits behind a disclosure, to copy: its text names every cause below
it, with secrets and URLs already cut by coven. Only the process that met the
failure holds that error, so a record read after a restart has no details and
offers none. The details are never the record's internal debug text. This list
replaces any banner of blocked work: there is one place to look.

Each record names what it is about in plain words. A file this device is
uploading is named by its file name and album, read from the upload queue it
is in; any other subject, such as an operation or the connection, by what it
is. In the upload list itself a file's row already names it, so its record
there says only why it waits.

**Files still arriving.** Rows arrive before their files. Ana's Mac imports
an album and uploads its files over the next hour; her phone shows the album
at once. When the phone asks for a file the Mac hasn't uploaded yet, storage
confirms it absent while the Mac is still an active device, and coven records
that the file waits for the Mac's upload
([coven §17.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#174-a-files-status)).
That is the library arriving, not something to fix: such a file is no record
in the pending list and doesn't count toward "Waiting". bae finds the row each
one belongs to (a release's cover or one of its files, an artist image, an
avatar) and names the device it comes from by the name that device
registered under. Once the Mac uploads it, the phone's next fetch of the file
ends its arrival. A file whose uploader was removed, or whose request failed,
stays a pending record.

Arriving is shown quietly where the release is. Its album's card carries a
small cloud mark, which names the device to assistive technologies and on
hover. Its detail says "Arriving from Studio Mac" under the release's facts,
and each track whose audio is still arriving shows dimmed, with "Arriving" in
place of its length, and can't be played or queued until it lands. Nothing
is coloured as a warning. A device knows a file is arriving only once it has
asked storage for it: covers, artist images and avatars come down as their
rows arrive, so those are known at once, while a track's audio is known after
the device first tries to read it, by playing it or keeping it offline. When
the last file lands, the mark and the line go by themselves.

A cover or other picture still arriving is no failure either. Reading it
names its file instead of failing, and the picture shows its placeholder
until the file lands; the request waits for that and then reads the picture,
so it appears by itself. Nothing about it is logged above debug. A reader
that has to answer at once, such as the artwork a car's media browser asks
for, gets no picture rather than waiting.

Ben's laptop shows "Waiting: 2 items": Dropbox wants him to sign in again, and
a file of his vinyl rip moved before it uploaded. The list offers *Sign In* for
the first: core signs in with the access the library's folder needs, then
connects that location again under this device's name. For the second it
offers to reattach the file: put it back where it was imported from, then
retry the upload. Other records offer *Retry*, *Reconnect*, *Resume Uploads*,
*Update App* or *Restore With Code*, or ask for an administrator's access to
storage. A wait the person can't help, such as for another device to come
online, offers nothing.

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
- **A newer schema.** Data written by a newer app needs an update;
  reconnecting can't fix it.
- **Pausing** stops uploads and keeps their place, across restarts too.
- **Covers after opening.** A device fetches the covers it doesn't hold yet.
  Its progress is a row of the sync activity list, with *Cancel*.

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
whole library. Its rows are filled once, when playing starts; after that they
can be removed and reordered but never grow. Each row is its own entry, so the
same track queued twice is two entries, each removable on its own.

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
- **Side or disc.** A record or cassette pauses at a side, named by its letter;
  a CD at a disc, named by its number.
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

A track's file is read from the person's own path for a local release, from
the cache or a pin for an uploaded one, and otherwise fetched from storage a
range at a time ([coven §17.5](https://github.com/bae-fm/coven/blob/main/spec/coven.md#175-reading-a-file)).
Reading ahead keeps playback from waiting on the network, and a seek fetches
only what is under the new position. One buffer per file serves every track
that plays from it, so the eleven tracks of a disc image share one.

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

**Finding receivers.** Casting is off by default: it browses the local network
and serves audio from this machine. While on, devices are searched for only
while the picker is open. Desktops and Android browse the network themselves.
iOS lets its own service browser find them, since sending multicast from the
app needs an entitlement Apple grants by application; it finds Chromecast and
AirPlay receivers but no UPnP ones.

**This device.** On macOS, playback follows a change of the default output
device. Volume and mute live in core, so unmuting restores the level last set.

### 7.8 What a device remembers

The queue and position are per device. A device saves what it was playing:
the source of what's playing from, whether it was shuffled, Up Next, repeat,
the current track and position, volume and mute.

A restart rebuilds what's playing from by its source, in source order with
the cursor on the saved track, or shuffled afresh around it. Removals and
reorders made during the session are not saved. A saved state with any value
out of range is discarded whole. Desktop apps restore on launch as a setting;
mobile apps always do, and save as they go to the background.

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
| `join_with_invite` | joins a library from an invite and writes the person's profile (§5.6) |
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
  read change. The album grid, the artist and composer lists, a search, the
  import list, sync status, uploads, downloads and the queue are all live. A
  subscription that shows a window of a long list is pointed at a new window
  in place as the person scrolls.

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
starts and cancels imports, and serves the import list's live query. Every
write to a candidate loads it whole, applies its rule, and saves it against
the revisions it read, so two writers never disagree about which rows belong
together.

### 9.4 Playing

The playback handle drives one queue on its own thread: play a release,
several releases or the library shuffled; pause, resume, stop; next, previous,
seek; volume and mute; queue edits; repeat and shuffle; play on a receiver or
stop; preview a file before import. Its values — the playing track, its phase,
its position, the queue — are live subscriptions.

### 9.5 Storage and sync

Through `AppServices`: move releases to the cloud and make them local again,
pin and unpin, cancel a transition, export and save; set up a cloud home,
reconnect, pause, read the pending list and act on its records; list people
and their devices, invite a person, approve or decline their request to join,
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
- **Subscriptions** take a callback object the app implements, and return a
  `LiveSubscription`; cancelling or dropping it stops the delivery.
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
| bae-macos | SwiftUI app; shares its service layer and the generated bindings with iOS through the BaeKit package |
| bae-ios | SwiftUI app on BaeKit: onboarding, library, album detail, downloads, playback, settings |
| bae-android | Kotlin app: onboarding, library, album detail, downloads, playback with a system media session, settings, a widget |
| bae-avalonia | Windows and Linux start-up, single instance, crash capture, telemetry, updater and packaging, and one window that calls the bridge |

---

## 11. Crates and conventions

### 11.1 Crates

| Crate | Holds |
|---|---|
| bae-core | the library, schema, import, identification, catalogs, files, playback, outputs, sync integration, settings |
| bae-bridge | the uniffi exports for the apps |
| bae-desktop | the desktop services a bridge handle starts: the MCP and Subsonic servers |
| bae-subsonic | the Subsonic server |
| bae-mcp | the MCP server over bae-automation |
| bae-automation | the automation tools and their JSON shapes |
| bae-cast | receiver discovery, the session media server, and its URLs |
| bae-mirror | the macros that write field-for-field conversions |
| bae-loc | the string catalog: checks it and writes each platform's resources |
| bae-theme | the shared theme, written as Swift and Kotlin |
| bae-test-support | helpers for bae-core's integration tests |
| bae-uniffi-bindgen | the bindings generator |

bae-cast stands between bae-core, which holds the renderer session, and
bae-subsonic, which serves the audio, because neither may depend on the other.
Nothing in bae-core depends on a server crate.

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
  leak. The baeium edition sends none.
- **Errors keep their cause.** A failure carries its typed reason to the
  person who can act on it; nothing defaults over an error.

### 11.3 Checks

A pre-commit hook formats and lints the changed crates. CI runs, on macOS,
iOS, Android, Linux and Windows: formatting, the file-size limit, visibility
and ownership boundaries, clippy for each feature set, dead-code and
unused-dependency checks, cargo deny, docs, every crate's tests, the string
catalogs, the Avalonia smoke test, the macOS and iOS builds and tests for both
editions, and Swift lint and format.

---
## Appendix A. Schema

The schema is `bae-core/migrations/001_initial.sql`, which coven applies as
migration 1. Every table is `STRICT`. "Syncs" means coven syncs its rows
(Appendix B); every other table stays on the device.

### A.1 The library

All sync.

| Table | One row per |
|---|---|
| `artists` | artist: name, folded name, sort name, MusicBrainz and Discogs ids |
| `artist_merges` | artist absorbed into another (§2.3); read through the view `merged_artist_survivors` |
| `artist_images` | artist's picture, keyed by the artist; carries a file |
| `albums` | album: title, first artist, year, primary release, compilation flag |
| `album_artists` | album's artist after the first, in order |
| `works` | MusicBrainz work |
| `work_artists` | work's credited artist, in order |
| `work_parts` | work inside a larger work, in order |
| `releases` | pressing: name, year, labels, barcode, country or region, media, status, packaging, Discogs details, source folder name, content hash, album loudness and peak, where its metadata came from |
| `covers` | release's cover, keyed by the release; carries a file |
| `release_files` | file of a release: name, size, type, and the audio facts the scan read; carries a file once attached, whose hash and location coven writes |
| `release_artist_roles` | credit on a release, as the catalog stated it |
| `release_records` | catalog record of a release: pressing or album, key, album key, URL |
| `tracks` | track: title, number, duration, Discogs position, playing order, side |
| `track_artists` | track's artist, in order |
| `track_artist_roles` | credit on a track, as the catalog stated it |
| `track_works` | work a track performs, in order |
| `audio_formats` | track's format, pregap, loudness and peak |
| `audio_format_segments` | span of a file a track reads: pregap or main, samples and bytes |

Beside them, one device-local table:

| Table | One row per |
|---|---|
| `local_release_files` | file of a local release on this device: its path and modification time (§4.2). Attaching the file deletes the row in the same write; detaching it writes one |

Whether a release is local or uploaded is not a column: it is whether its files
are attached.

### A.2 People

All sync (§5), except `local_history`.

| Table | One row per |
|---|---|
| `people` | person, keyed by their `MemberId`: name, private circle, history setting (private or household), avatar |
| `ratings` | rating by a person of an album or of a track, one to five stars |
| `history` | play: person, track, release, when it started, how much was played, audience |
| `playlists` | playlist: whose it is, its name and its audience |
| `playlist_entries` | place in a playlist, keyed by the playlist and a position id: the track there and its position |
| `local_history` | play on this device before the person had a profile: track, release, when it started, how much was played (§5.3) |

### A.3 Playback

| Table | Holds |
|---|---|
| `playback_state` | the one row of what this device was playing (§7.8) |

### A.4 Watched folders and scans

| Table | Holds |
|---|---|
| `watched_import_folders` | watched folders, in order |
| `release_grouping`, `release_grouping_member` | folders read as one release, or kept apart, and by whom (§3.1) |
| `skipped_import_candidates` | candidates the person dismissed |
| `folder_discovery` | when each folder was first seen, and its date |
| `folder_scan_generation_sequence`, `folder_scan_roots`, `folder_scan_directory` | each watched folder's last scan and the directories it saw |
| `scan_candidate`, `scan_candidate_file`, `scan_candidate_part` | the candidates a scan found, their files and their folders |
| `scan_candidate_tag_snapshot`, `scan_candidate_file_tag` | the tags read from a candidate's audio, and its embedded cover |
| `scan_skipped_file`, `scan_skipped_tags` | a broken folder's files as the scan read them, and what its tags call it |
| `scan_sidecar`, `scan_sidecar_file` | files under a folder no release owns |
| `scan_cue_sheet`, `scan_cue_track`, `scan_cue_index`, `scan_sheet_audio_file` | parsed CUE sheets and the audio each `FILE` line resolved to |

### A.5 Import candidates

Keyed by the candidate's content hash.

| Table | Holds |
|---|---|
| `import_candidate_state`, `import_candidate_folder` | the candidate, its revisions, and where it was found |
| `import_candidate_edit`, `import_candidate_album_artist_assignment`, `import_candidate_track`, `import_candidate_track_artist_assignment` | the draft (§3.6) |
| `import_candidate_sheet_disc`, `import_candidate_sheet_reference` | the person's CUE decisions |
| `import_candidate_cover`, `import_candidate_cover_copy`, `import_candidate_remote_cover_asset` | the chosen cover and its fetched bytes |
| `import_candidate_source_artist`, `import_candidate_artist_asset`, `import_candidate_asset_preparation` | the artists' fetched pictures, and that all are ready |
| `import_candidate_draft_provenance`, `import_candidate_applied_source`, `import_candidate_applied_length` | where the draft was read from, and the lengths it was laid against |
| `import_candidate_release_link`, `import_candidate_release_link_partner`, `import_candidate_release_link_album` | the pressing or album the candidate is linked to |
| `import_candidate_session` | where the pane was, and its last failure |
| `import_candidate_failure`, `import_candidate_artist_identity_conflict` | why the last import failed |

### A.6 Identification

| Table | Holds |
|---|---|
| `import_candidate_signals`, `import_candidate_signal_value`, `import_candidate_text_line` | the clues read off a candidate (§3.3) |
| `import_candidate_lookup_choices`, `import_candidate_chosen_catalog`, `import_candidate_discounted_catalog`, `import_candidate_excluded_barcode` | what the person let the lookups use |
| `import_candidate_verdict` | what a run concluded: found, not found, manual only, failed or error; the ledger it showed; a medium conflict |
| `import_candidate_match` and its `_barcode`, `_cover_copy`, `_link`, `_album_link`, `_medium` tables | every release a run returned, with which lookup found it |

### A.7 Catalog releases

Device-local: any device can fetch a release. A new fetch replaces every row
under it.

| Table | Holds |
|---|---|
| `source_release` | one fetched catalog release and every fact an import reads from it |
| `source_release_album_artist`, `source_release_role`, `source_release_entry_credit`, `source_release_entry_role` | its credits |
| `source_release_medium`, `source_release_entry`, `source_release_format` | its media and tracklist |
| `source_release_work`, `source_release_work_composer` | its MusicBrainz works |
| `source_release_link`, `source_release_album_link`, `source_release_record`, `release_group_album_link` | what it says it is on other catalogs |
| `source_release_cover`, `source_release_cover_copy`, `source_release_archive_group` | the images it offers a picker |
| `source_release_unfetched` | linked documents the fetch didn't get, and why |

---

## Appendix B. Synced tables

From `bae_core::sync::synced_tables`. *Shared key* rows take an id computed
from their fact (§2.6); *independent* rows a fresh id. Every synced table's
rows reach every device that can read their audience: a local release's rows
too (§4.2). *Audience* is the store unless the table names one (§5.3); *owner*
tables are changed only by their author's devices (§5.5). `local_release_files`
and `local_history` are not declared, so they never sync.

| Table | Identity | Audience | Owner | File |
|---|---|---|---|---|
| `artists` | shared key | store | | |
| `artist_merges` | shared key | store | | |
| `albums` | shared key | store | | |
| `album_artists` | shared key | store | | |
| `releases` | independent | store | | |
| `release_records` | shared key | store | | |
| `tracks` | independent | store | | |
| `track_artists` | shared key | store | | |
| `works` | shared key | store | | |
| `work_artists` | shared key | store | | |
| `work_parts` | shared key | store | | |
| `track_works` | independent | store | | |
| `release_artist_roles` | independent | store | | |
| `track_artist_roles` | independent | store | | |
| `release_files` | independent | store | | the person's own; attached on upload; read on first use; detached to return to local paths |
| `audio_formats` | independent | store | | |
| `audio_format_segments` | independent | store | | |
| `covers` | shared key: the release | store | | bae's own; fetched with the row |
| `artist_images` | shared key: the artist | store | | bae's own; fetched with the row |
| `people` | shared key: the person's `MemberId` | store | yes | the avatar: bae's own; fetched with the row |
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
| `save_presets` | FLAC, MP3 320 | the save presets (§4.6) |
| `default_track_save_preset`, `default_release_save_preset` | FLAC | which preset a save starts with |
| `pause_between_sides` | on | pausing between sides and discs (§7.3) |
| `side_pause_countdown` | off | when such a pause ends on its own |
| `max_concurrent_uploads`, `max_concurrent_downloads` | 3 | files moved at once, 1 to 8 |
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

A save preset holds a codec (FLAC, WAV or AIFF at a bit depth; MP3, AAC or
Opus at a bitrate), a filename pattern of tokens (title, artist, album, year,
track number, disc number, track total), where pregaps go (appended to the
track before, with or without the hidden first-track audio; left out; or one
file with a CUE sheet), and whether it embeds the cover.

Platform settings, such as the appearance and restoring playback on launch,
live in each app.

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
| `import_candidate_pressing_fact_set` | choose its area, media, status, packaging or Discogs details |
| `import_candidate_cover_set` | choose its cover |
| `import_start` | import a candidate as stored |
| `release_detail_get` | read a library release |
| `release_export` | queue an export of a release's files |
| `release_storage_action` | move to the cloud, pin, unpin, make local, or cancel |
| `output_status` | read the export queue |
| `release_reidentify` | set a release's pressing |
| `release_metadata_reset` | re-read a release's metadata from its source |
| `release_metadata_update` | apply a metadata edit |
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
| `GroupingBlocked` | combines again, separates, or waits for a download |
| `Export`, `Save` | reads the detail |
| `CloudSetup`, `StorageCheck` | changes what the setup, or the check of the storage it failed, names |
| `S3VersioningUnknown`, `S3VersioningUnsupported` | uses a bucket whose versioning coven can read and work with |
| `StorageLocationChangeUnsupported` | keeps the library's storage location; it can't move |
| `DeviceIdentityMissing` | creates, joins or restores |
| `Credentials` | fixes the cloud settings |
| `Network` | retries |
| `Keyring`, `KeyringLocked` | fixes the keyring, or unlocks and is asked again |
| `KeychainEntitlementMissing` | installs a build of bae signed with its keychain entitlement; retrying or unlocking doesn't help |
| `Membership` | retries the membership step, with the advice it carries |
| `AirPlayUnsupported` | uses another receiver |
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
