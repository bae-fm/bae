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

- **bae** signs in to Google Drive, Dropbox and OneDrive, uses iCloud on Apple
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

Two artists can be merged into one. Merging records that one artist was
absorbed into the other; both rows stay, so a credit another device gave the
absorbed artist still resolves. Every read shows the survivor. A chain of
merges shows its last artist, and a loop, left by two devices merging a pair in
opposite directions, shows the artist with the smallest id.

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
  refused, and the import shows the conflict for the person to settle by
  merging them (§3.7).
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
| work credit | the work, the artist and the position |
| work part | the parent and child works |
| release record | the release and the catalog |
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

A release's id comes from a hash of its folder's contents. The hash covers
every file of the release: each one's path relative to the release folder, its
size and its modification time, sorted, so the same rip hashes the same
wherever it sits.

Importing the same contents again, on any device, is the same release: a
re-import, not a copy. Ana edits her tags and wants bae to read them again;
she imports the folder again, and her release is replaced in place.

Different contents of the same pressing, such as Ana's FLAC rip and Ben's
MP3s, are separate releases under one album.

The hash covers files, never decisions. Carol tells bae which disc each CUE
sheet is; that is stored under the hash, and a hash that moved with it would
lose the very decision it keys. No decision removes a file from a release.

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
document, other), the parsed CUE sheets, the tags each audio file carried, and
when each folder was first seen. A scan writes as it finds; completing removes
what it didn't see, in the same transaction that marks it complete. A failed or
interrupted scan keeps both old and new entries.

**Edge cases.**

- **A broken folder.** A folder with corrupt or zero-byte audio or images, or
  no valid audio at all, is listed as skipped with its reason. It can't be
  imported.
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
| Pending | not imported and not skipped, including a failed import |
| Done | imported; ordered by upload, what is moving first |
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

The person can select any number of rows. The selection is stored as rows,
not held by a surface, and a bulk action runs in core over the selected rows
that offer it.

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
2. **Reads and hashes each file** where it sits. The hash is the file's SHA-256,
   which coven later checks every download against.
3. **Measures loudness** by decoding every track: its EBU R128 integrated
   loudness and true peak, and the album's over all its tracks. A near-silent
   track stores no loudness and plays unchanged.
4. **Checks the audio decodes**, in the same pass. A track that fails, or
   decodes short, fails the import rather than failing later at play time.
   This check is a setting, on by default.
5. **Finds the album.** A release whose records name a pressing already in the
   library is refused as already there. A release whose record's album matches
   an album in the library joins it. Otherwise the album takes its id from its
   group (§2.6).
6. **Writes everything in one transaction**: the album, release, tracks,
   files, segments, credits, works, records and cover. The artists are decided
   in that transaction (§2.3).

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
the person last chose (§4.2). A cloud import records its upload in the same
transaction as its rows, so the choice is never lost, and needs no connection
to record it.

**Re-import.** Importing a folder whose hash matches a release in the library
replaces that release in the same transaction (§2.7).

**Automatic identification and import.** By default, each release a scan finds
for the first time is identified on its own, in the background, behind
anything a person asks for. With "Import automatically when identified" on, a
run that picks unattended imports straight away. Both are settings
(Appendix C).

**Edge cases.**

- **Two artists for one name.** An import whose credit names two library
  artists, each linked to a different catalog, fails with the conflict. The
  person merges them and the candidate is ready again (§2.3).
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
a release comes back to one device (§4.4), where files sit in storage (§4.5),
and how files go back out to disk (§4.6). coven stores and moves the files;
its [§17](https://github.com/bae-fm/coven/blob/main/spec/coven.md#17-files)
explains how.

### 4.1 Whose files

Two kinds of file, matching coven's
[two kinds](https://github.com/bae-fm/coven/blob/main/spec/coven.md#171-two-kinds-of-file):

- **Audio: the household's own files.** Every file of a release's folder,
  audio or not, is the person's file at its path. coven records its path, size
  and modification time and never changes it.
- **Covers and artist images: bae's own.** bae makes them, downscaled to at
  most 600 pixels as JPEG, and hands the bytes to coven, which keeps them.

A cover belongs to one release, an artist image to one artist. Changing a
cover stores a new image with a new id and deletes the old one; coven never
rewrites a stored file.

### 4.2 Local and uploaded releases

A release is *local*, or *uploaded* so other devices can play it.

Ana's CD rip is local after import: its files are her own, at their path on her
laptop, and other devices don't see the release. Ana moves it to the cloud.
coven uploads each file from where it sits, and with the last one marks the
release uploaded; from then on, the release, its tracks, files and cover reach
every device. Ana's files stay where they are, untouched.

- **Every import lands local first.** A cloud import (§3.7) records its upload
  with its rows and becomes uploaded when the upload lands.
- **An album and its artists follow their releases.** An album reaches other
  devices while it has an uploaded release; an artist while an uploaded
  release's album, track or work credits it.
- **Uploads queue.** Files go up three at a time by default, a setting from
  one to eight. The queue can be paused; a paused upload keeps its place.
  Cancelling an upload in progress keeps the release local, and takes back
  out anything already stored.

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
file is copied there, each checked at its new path, before the release is
marked local in one commit; the stored copies are then deleted. A cancel
before that commit removes the partial copies and leaves the release uploaded.

The file names come from the release's rows, which another device may have
written. Each is checked to be a plain relative path before it is joined to
Carol's folder, so no row can make a device write outside the folder chosen.
One bad name copies out nothing at all.

### 4.5 Where files sit in storage

A cloud home stores its files one of two ways, chosen when it is set up:

- **Opaque**: encrypted, at paths that hash each file's id.
- **Browsable**: in the clear, at paths a person can follow:

```
release_files/{album}/{release}/{source folder}/{file name}
covers/{album}/{release}/cover-{image id}.{ext}
artist_images/{artist}/artist-{image id}.{ext}
```

The path is fixed when the file is first stored and kept in its row, so
renaming an album never moves a file. An image's path carries its image id, so
a new cover is a new object. Two devices replacing one cover then never write
the same object, and a device applying an older write still finds the bytes
that write names.

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

- **Deleting a release** deletes its rows and bae's copies of its files. The
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
people, and each person adds and removes their own devices.

### 5.2 Everyone edits the library

Every member can change the library. There is no play-only role, since coven
checks no permission on library writes, and a rule only the UI enforced would
not hold: any member's device could write around it.

Which edits a platform's UI offers is a separate choice. The mobile apps don't
gain library editing because their devices can write.

### 5.3 Each person's own data

Each person has a circle with only themselves in it. Private rows live there,
household rows in the store. bae makes no other circles.

Each person's own data:

| Data | Where |
|---|---|
| Ratings, of albums and of a release's tracks | always household |
| History | private or household: one setting per person |
| Playlists | each one private or household |

There are no separate favorites: the highest rating is the favorite.

Changing the history setting moves the person's history rows; changing a
playlist's setting moves that playlist. A move between the store and a circle
is an ordinary write in coven
([§15.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#154-moving-rows)).

Ben keeps his history private. Carol's devices never see what he played. He
makes a playlist for a party and shares it: it moves from his circle to the
store, and everyone sees it.

### 5.4 Profiles

coven owns identity: members, roles and devices. bae keeps a synced `people`
table, in the store, holding each person's name and avatar.

- **A person is keyed by their coven `MemberId`**, which is stable: replacing
  keys, or re-inviting someone who lost every device, keeps the same id
  ([coven §13.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#133-losing-everything)).
- **Ratings, history and playlists point at the person**, not the member.
- **The joining person's own device writes their `people` row** as part of
  joining, with the name they typed, so every member has a profile. A
  re-invited person keeps their row, since their `MemberId` is the same.
- **An avatar is a file on the `people` row**, bae's own (app-provided),
  downloaded as soon as the row arrives.
- **The app joins `members()`**, who is in, with `people`, how to show them.

### 5.5 "Ana rated this"

Rows credited to a person are owner tables: ratings, household history,
household playlists, and the person's `people` row. Only their author's devices
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
| Remove a device | the person | nothing beyond coven |
| Remove a person | an admin | their circle goes, with its rows |

Dan joins. Ana invites him; his phone scans the invite, he types "Dan", and his
phone writes his `people` row while it joins. Everyone's devices show Dan by
name.

Removing a person deletes their circle and its rows: their private history and
private playlists. Their household ratings, history and playlists stay,
credited to them.

A new device gets its restore code by scanning a QR code on another of the
person's devices, from iCloud Keychain on Apple devices, or by typing it in.

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

Each reason is the same: the data is one device's, and another device would be
wrong to follow it. Ana's queue is what she is listening to, not Ben. A scan is
of one machine's disk. A fetched catalog release can be fetched by any device
that wants it. A transfer limit fits one machine's link.

A local release stays on its device (§4.2); only an uploaded release, and the
album and artists it needs, reach other devices.

### 6.2 Cloud homes

A library's *cloud home* is where coven stores it: an S3-compatible bucket,
Google Drive, Dropbox, OneDrive, or iCloud on Apple devices. A library can
also have none and stay on one device. The baeium edition offers S3 only.

Setting up a cloud home chooses opaque or browsable storage (§4.5). For S3,
the person enters a bucket, region, endpoint, optional key prefix, and an
access key. For the account providers, they sign in. coven's
[Part VI](https://github.com/bae-fm/coven/blob/main/spec/coven.md#part-vi-storage)
says what each provider needs.

A device can hold several libraries, each its own coven store under
`~/.bae/libraries/<library id>/`, and remembers which it last opened.

### 6.3 How bae declares its tables

coven syncs only the tables it is told about. bae declares each library table
with:

- **its row identity**: a *shared key*, an id computed from the fact the row
  states (§2.6), or an *independent* id minted fresh for each row: releases,
  tracks, files, audio formats and segments, track credits and works, role
  credits, and covers;
- **the files it carries**: `release_files` the person's own, read on first
  use; `covers` and `artist_images` bae's own, fetched as soon as their rows
  arrive.

Appendix B lists every declaration. A test holds the declared set to exactly
the tables that carry an `_updated_at` column, so a device-local table can't
start syncing by accident, and a new library table can't be forgotten.

Two devices' edits merge cell by cell: an edit to a column the other left alone
survives, and of two edits to one column the later wins
([coven §8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#8-merging)).

### 6.4 When sync stops

Sync runs in the background. A library opens and plays from its own copy
whether or not the cloud is reachable.

- **Launching offline.** A failed connect at start-up opens the library
  unconnected and records the failure for the banner. *Reconnect* connects
  again, or runs the next cycle now when already connected.
- **A blocked operation** — a write, a circle operation, or a cleanup coven
  can't finish — is listed with its reason, and the person can retry it.
- **A newer schema.** Data written by a newer app needs an update;
  reconnecting can't fix it.
- **Pausing** stops uploads and keeps their place.
- **Covers after opening.** A device fetches the covers it doesn't hold yet;
  the progress shows as a banner and can be cancelled.

The library's encryption key lives in the system keyring. A keyring that is
locked right now — a locked session or a sleeping display — is told apart from
a broken one: the host asks again after unlock instead of reporting the library
unusable. A device can forget the key, which locks the library until it is
unlocked again, and can remove a library entirely: its directory and every
secret it holds, leaving the cloud copy untouched.

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
| `create_library` | registers a new library and gives this device its identity; with no name given, one is generated |
| `restore_from_code` | opens a library on a new device from a restore code (§5.6) |
| `prepare_device_pairing_join`, `join_prepared_device_pairing_cancellable` | joins this device to a library another device admits |
| `Config::discover_libraries` | lists registered libraries, including any that can't be opened, with why |
| `app::bootstrap` | opens a library and starts its services: the store, sync, playback, and on desktops import and identification |
| `AppServices::close` | stops playback, saving where it was, then identification and import, then closes the store |
| `remove_local_library` | deletes a closed library from this device (§6.4) |

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

Through `AppServices`: move releases to the cloud and back, pin and unpin,
cancel a transition, export and save; set up a cloud home, reconnect, pause,
retry a blocked operation; list members, start a device pairing, remove a
member; make a restore code.

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
  OAuth applications, the iCloud driver, any sign-in in flight, and a runtime
  for the calls that run before a library is open: create, restore, join, sign
  in.
- **`init_app`** opens a library and returns an **`AppHandle`**, which wraps
  `AppServices` and, on desktop, the MCP and Subsonic servers and the cast
  controller.
- **Async calls** run on the library's own runtime. A Swift task cancelled
  mid-call cancels the Rust work it started.
- **Subscriptions** take a callback object the app implements, and return a
  `LiveSubscription`; cancelling or dropping it stops the delivery.
- **Callbacks the app provides**: UI events, the artwork reader, device-join
  progress, and on Apple devices the iCloud driver.

### 10.3 Generated bindings

The Swift, Kotlin and C# bindings are generated from the Rust at build time
and are not in the repository. A type's fields are read in
`bae-bridge/src/types/`.

Build features choose what a build exports:

| Feature | Adds |
|---|---|
| `oauth-providers` | Google Drive, Dropbox and OneDrive sign-in |
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
| `releases` | pressing: name, year, labels, barcode, country or region, media, status, packaging, Discogs details, local or uploaded, source folder name, content hash, album loudness and peak, where its metadata came from |
| `covers` | release's cover, keyed by the release; carries a file |
| `release_files` | file of a release: name, size, type, storage path, SHA-256, and the audio facts the scan read; carries a file |
| `release_artist_roles` | credit on a release, as the catalog stated it |
| `release_records` | catalog record of a release: pressing or album, key, album key, URL |
| `tracks` | track: title, number, duration, Discogs position, playing order, side |
| `track_artists` | track's artist, in order |
| `track_artist_roles` | credit on a track, as the catalog stated it |
| `track_works` | work a track performs, in order |
| `audio_formats` | track's format, pregap, loudness and peak |
| `audio_format_segments` | span of a file a track reads: pregap or main, samples and bytes |

### A.2 Playback

| Table | Holds |
|---|---|
| `playback_state` | the one row of what this device was playing (§7.8) |

### A.3 Watched folders and scans

| Table | Holds |
|---|---|
| `watched_import_folders` | watched folders, in order |
| `release_grouping`, `release_grouping_member` | folders read as one release, or kept apart, and by whom (§3.1) |
| `skipped_import_candidates` | candidates the person dismissed |
| `folder_discovery` | when each folder was first seen, and its date |
| `folder_scan_generation_sequence`, `folder_scan_roots`, `folder_scan_directory` | each watched folder's last scan and the directories it saw |
| `scan_candidate`, `scan_candidate_file`, `scan_candidate_part` | the candidates a scan found, their files and their folders |
| `scan_candidate_tag_snapshot`, `scan_candidate_file_tag` | the tags read from a candidate's audio, and its embedded cover |
| `scan_sidecar`, `scan_sidecar_file` | files under a folder no release owns |
| `scan_cue_sheet`, `scan_cue_track`, `scan_cue_index`, `scan_sheet_audio_file` | parsed CUE sheets and the audio each `FILE` line resolved to |
| `candidate_selection`, `candidate_selection_revision` | the import list's selection, for one session |

### A.4 Import candidates

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

### A.5 Identification

| Table | Holds |
|---|---|
| `import_candidate_signals`, `import_candidate_signal_value`, `import_candidate_text_line` | the clues read off a candidate (§3.3) |
| `import_candidate_lookup_choices`, `import_candidate_chosen_catalog`, `import_candidate_discounted_catalog`, `import_candidate_excluded_barcode` | what the person let the lookups use |
| `import_candidate_verdict` | what a run concluded: found, not found, manual only, failed or error; the ledger it showed; a medium conflict |
| `import_candidate_match` and its `_barcode`, `_cover_copy`, `_link`, `_album_link`, `_medium` tables | every release a run returned, with which lookup found it |

### A.6 Catalog releases

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
from their fact (§2.6); *independent* rows a fresh id. *Follows* names the
foreign key a row reaches other devices through: it syncs while the release,
artist or album at the end of that chain does (§4.2).

| Table | Identity | Follows | File |
|---|---|---|---|
| `artists` | shared key | its credits | |
| `artist_merges` | shared key | `id` | |
| `albums` | shared key | its uploaded releases | |
| `album_artists` | shared key | `album_id` | |
| `releases` | independent | uploaded (`remote`) | |
| `release_records` | shared key | `release_id` | |
| `tracks` | independent | `release_id` | |
| `track_artists` | independent | `track_id` | |
| `works` | shared key | its credits | |
| `work_artists` | shared key | `artist_id` | |
| `work_parts` | shared key | `child_work_id` | |
| `track_works` | independent | `track_id` | |
| `release_artist_roles` | independent | `release_id` | |
| `track_artist_roles` | independent | `track_id` | |
| `release_files` | independent | `release_id` | the person's own; read on first use; never repointed |
| `audio_formats` | independent | `track_id` | |
| `audio_format_segments` | independent | `audio_format_id` | |
| `covers` | independent | `id` | bae's own; fetched with the row |
| `artist_images` | shared key | `id` | bae's own; fetched with the row |

---

## Appendix C. Settings

Each library's settings are `preferences.yaml` in its directory, beside
coven's `config.yaml`. They are device-local. Every key is written; a file
missing one fails to load, and only a missing file means every default.

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
| `identification.import_when_identified` | off | importing what an automatic run picks |
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
| `configuration/library.rs` | search results, artists, composers and works, the storage table, the queue, sync status and blocked operations |
| `configuration/services.rs` | settings, the MCP and Subsonic servers, receivers, cloud-home setup |
| `configuration/settings.rs` | sorting, restore codes, members and pairing, save presets, errors |
| `playback_values.rs` | the playing track, position, side-pause prompt, media controls, preview |
| `events_and_transfers.rs` | UI events, uploads, downloads, exports and saves |
| `editing.rs` | release detail and edits, artist credits, the mapping table, edit validation |
| `pressing.rs` | pressing facts: area, media, labels, status, packaging, Discogs details |
| `device_pairing.rs` | join progress on both devices |
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
| `Import`, `ImportData` | reads the detail; the source couldn't be read |
| `CandidateImportInProgress`, `CandidateBeingIdentified`, `CandidateAlreadyImported` | waits, or nothing is needed |
| `MetadataTrackCount` | picks another release: it lists another number of tracks |
| `AlreadyInLibrary` | sees the album it is already in |
| `GroupingBlocked` | combines again, separates, or waits for a download |
| `Export`, `Save` | reads the detail |
| `CloudSetup` | changes what the setup names |
| `DeviceIdentityMissing` | creates, joins or restores |
| `Credentials` | fixes the cloud settings |
| `Network` | retries |
| `Keyring`, `KeyringLocked` | fixes the keyring, or unlocks and is asked again |
| `Membership`, `DeviceJoin` | retries the membership step, with the advice the end carries |
| `AirPlayUnsupported` | uses another receiver |

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
