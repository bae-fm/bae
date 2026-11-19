# bae

**This document is a draft.** bae's design still changes often, and parts of
this text are wrong. bae's coding rules (the rules its developers and agents
follow) outrank this document: where it states a shape or a behavior that a
rule forbids, the rule holds and the text here is a mistake. When you find this
text contradicting a rule, the code or a change you build, ask bae's author, say
which text looks wrong and why, and offer the edit. Edit this document only
after the author agrees.

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

Names of artists, albums and songs in this document are placeholders. The
words bae puts on screen, and what each means, are in [words.md](words.md).

## Contents

- [1. What bae is](#1-what-bae-is)
- [2. Library](#2-library)
- [3. Import](#3-import)
- [4. Files](#4-files)
- [5. People](#5-people)
- [6. Sync](#6-sync)
- [7. Playback](#7-playback)
- [8. Servers](#8-servers)
- [9. Across the apps](#9-across-the-apps)
- [11. Crates and conventions](#11-crates-and-conventions)
- [Appendix A. Schema](#appendix-a-schema)
- [Appendix B. Synced tables](#appendix-b-synced-tables)
- [Appendix C. Settings](#appendix-c-settings)
- [Appendix D. Subsonic endpoints](#appendix-d-subsonic-endpoints)
- [Appendix E. MCP tools](#appendix-e-mcp-tools)
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
  a household, for macOS, iOS, Android, Windows and Linux; macOS and Android
  have their apps, and the others hold start-up and release machinery only
  (§1.1). It plays the files the household already has; it sells and streams
  nothing.
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

The macOS app is the full desktop app. On Windows and Linux, bae-avalonia
holds the desktop app's start-up, update and packaging machinery and one
window; the desktop views are ported from the macOS app later. Opening it
while it runs hands the launch to the running copy; a launch that can't hand
over, logged, takes over when that copy is gone.

The Android app is the full phone app. On iOS, bae-ios holds the phone app's
start-up and release machinery and one screen; the phone views are rebuilt
from the Android app later. The screen shows
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

The build decides which edition a device runs, and a device never switches
editions while it runs.

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
barcode, each removable, with a row to add another, as it does labels. Its
media are a list of carriers, each with how many of it there are (two CDs, one
vinyl record). Its labels are a list of library labels (§2.3), each with the
catalog number it gives the pressing; a place may state a catalog number
alone. Its labels, its barcodes and its media are each kept one row per place
in the list, and a save replaces the whole list. A saved
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

An album's page heads with its title, artists and year, then the selected
release on two lines: its pressing (its own year when that differs from the
album's, where it was released, its media and details, its audio and its
length), and its labels, each with the catalog numbers it gives the pressing.

The person picks which of an album's releases its page shows. The page shows
the picked release while the album holds it, and the primary release otherwise.

On macOS the album page's track list lays each side or disc out in two
columns. A side's second column starts after half its tracks, rounded up, when
the side has more than eight; a shorter side fills the first column alone.

The album page's pressing line starts with the release's year only when it
differs from the album's year shown above it (a 2001 reissue of a 1992 album
says 2001; the 1992 original says nothing).

On macOS the person picks several tracks of the album page's track list:
cmd-click picks or unpicks a row, shift-click picks the rows from the last pick
to the one clicked, and a plain click clears the picks. A shift-click picks the
tracks from the last pick to the one clicked in the order their release lists
them, or the clicked track alone when the last pick is not a track of the
clicked track's release. A pick or a clear that the person makes before a
shift-click's tracks show as picked cancels that shift-click. The page clears
the picks when it shows another release or closes. The app tints a picked row
and marks it as selected for screen readers. A row's menu acts on every pick
when the row is among several, and on the row alone otherwise. For several it
offers *Play N tracks*, *Play next*, *Add to queue* and *Add to playlist*. Each
action takes the tracks in the order their release lists them (by release, then
position), each once, whatever order the person picked them in.
Play plays the tracks the library holds from the first as a list of tracks
(§7.1), Play next and Add to queue put the tracks the library holds in Up next
in that order, and Add to playlist appends the tracks to the playlist in that
order in one write: when the library lacks any of the tracks, Add to playlist
adds none of them. New playlist starts a playlist holding the tracks the library
holds.

On macOS the person picks several albums in the album grid the same way:
cmd-click picks or unpicks an album, shift-click picks the albums of the cards
from the last pick's card to the one clicked, and a plain click clears the
picks. A shift-click picks each album once, in the grid's order under its sort
and grouping, including albums whose cards are off screen. Grouped by artist,
each end is the card the person clicked, in its artist's group. A pick or a
clear that the person makes before a shift-click's albums show as picked
cancels that shift-click.

Every import is its own release, so the library can hold one pressing twice:
two folders of the same CD are two releases under one album (§2.7).

An album's *primary* release is the one whose cover it shows and that opens
and plays by default. It is not a stored pointer: each release holds a place in
its album's order (`releases.album_order`), and the primary is the release
first in that order, ties broken by release id. So every album with a release
has exactly one primary. A release joining an
album goes after the album's others, so an album's first release stays its
primary. "Set as primary release" moves a release before the album's others,
and bae refuses it for a release of another album. Moving or deleting a release
writes nothing else: the next release in the order is then the primary.

A release with no stored name, year or media shows as "Release N", where N is
its place among its album's releases in that order, counted from 1, ties broken
by release id. The album page, the release page and the composer page's release
rows all show the same N.

Deleting a release deletes, in one write, its tracks and their places in the
queue, its files and its cover (§4.2), and is refused for a release the library
doesn't hold. Deleting an album deletes every release
it holds in one write, and is refused for an album holding none. An album
whose last release goes, by a delete or by moving to another album, is deleted
in the same write, with its credits and ratings (a release another device adds to
that album meanwhile goes with it, as a cascade does, and no device keeps it); so is a work no track performs
any more (§2.4) and an artist nothing credits any more (§2.3), each with what
hangs on it. A save that takes a credit out and writes it again deletes
nothing. Both
deletes ask first, saying that bae permanently deletes the release, or every
release of the album, from the person's devices and the cloud, and keeps the
original files in their folders. On macOS the
album detail's menu offers *Delete release* and *Delete album…*, and a grid
card's menu offers *Delete album…* when it acts on one album; on Android the
album page's menu offers *Delete album…*.

An album credited to the various-artists artist alone is a *compilation*, and
its tracks show their own artists. The flag is decided from the album's
credits at every write of them, an import's or an edit's, so it never outlives
an edit of the artists.

**The album grid** shows every album, newest first unless
the person sorts it by title, artist (§2.3), year or date
added, either way; an album of unknown year sorts after every year ascending
and before them descending, and albums the sort ties go by id. Grouped by
artist, the grid lists an album under each artist its credits show, once per
artist, the artists in order of sort name (else name) and then id, each
artist's albums in the grid's sort. Each artist's group carries the artist's
picture and how many albums it holds, and each album listed in it carries its
other credits, those besides that artist, printed as a line (none when that
artist is its only credit). The artist list shows every artist an
album's credits show, sorted by name or album count, ties going by name and
then id. Under its name an artist's row, its page and its search result say how
many albums credit it ("1 album", "3 albums"), in the viewer's language.

**Names order in the device's language.** Every list that sorts by a name
(album titles and artists in the grid, the artist and composer lists, a
composer's page, the Storage manager's album and artist columns, and the
order of library search's results and of an artist's albums and other printed
names, the playlists, the people and the cast picker's receivers) orders them as a reader of the device's language
expects, by Unicode collation with that language's tailoring: in Swedish å, ä
and ö come after z, in Spanish ñ after n, in Russian "я" and "Я" sit together,
and in the root order accented letters sort with their base letter. Names
that differ only in case have different places, lowercase first. The device
holds its language in one local row, set by the app (macOS: its first
preferred language, Android: its locale) at launch and when its language
changes, and a collation key of every listed name written in that
language beside the name; lists order by the keys. A change of language
reorders every list, the playlists, the people and the cast receivers among
them. A change of language, or of the collation data the keys were written with
(which bae checks when it opens the library, before any list shows), rewrites
every key in one write, so a list is in one order or the other and never
between them. The keys are device-local and
are not synced; every reader of the library file on the device (the app, MCP and
Subsonic) sees the one order.

**Browsing.** The library shows one view at a time: albums, artists or
composers. The view's name heads the page, and its menu switches
views. On macOS the artists and composers views list their rows
beside the open one's page. At the
top of the list the heading is full size; as the list scrolls it shrinks into a
slim bar pinned above it, and a scroll that stops part way leaves it at the
nearer size. On Android, the heading's end holds Search, the Activity
button (§6.4), ⋮, whose menu gives the view's sort under *Sort by* (the
fields, the direction, and for albums *Group by artist*), then *Shuffle
library*, and last the person's menu (below); screen readers name that ⋮ *More
options*. On Android screen readers name the back arrow of an album, artist, composer or playlist page opened from the library *Back to library* (*Back* when
opened from another page), and the album page's ⋮ *Album options*. A card's title or artists, or a row's name, too long
for its one line ends in "…". On Android the grid grouped by artist opens each
artist's albums with a header of the artist's round picture, name and album
count, which opens the artist; the header of the artist being scrolled through
stays pinned under the heading, as a slim strip of picture and name, until the
next artist's header pushes it off. A card in a group shows the album's year
and, when it credits others besides the group's artist, "with" them.

**The person's menu.** The person's picture, or the initials of their name
(§5.4), ends the top bar: on a Mac at the end of the window's title bar, and on
Android after ⋮. Screen readers name it *Your menu*. It opens a menu of *Add
device…* and, for an admin, *Invite someone…*, then, after a divider,
*Playlists* and *History*, and, after another, *Settings…* (on Android *Add
device*, *Invite someone* for an admin, *Playlists*, *History* and *Settings*).
Add device opens the pairing of a new device, as *Add device…* on Your devices
does; Invite someone opens the invite, as People's *Invite…* does; on a Mac
whose library has no cloud home yet, *Set up a cloud home…* comes first and
opens the cloud home setup, as the Sync page's *Set up a cloud home…* does, and
Add device and Invite someone each open a sheet that says the cloud home comes
first: "To add a device, set up a cloud home first. Your library and music stay
on this Mac until you do." (an invite says "To invite someone, …"), with
*Cancel* and *Set up a cloud home…*. On a Mac, Playlists and History open in
the side pane beside the queue, which a control at its top switches between
*Queue*, *Playlists* and *History*; on Android each opens as its own screen
with a back arrow.

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
A track's length runs from its index 01 to where the next track's stream starts
(that track's index 00 when it has a pregap in this file), or to the end of its
file, so it leaves out the pregap of the track after it.

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

Each file records what the scan read of it: the role it plays (audio, track
sheet, artwork, damaged image, document, other), for a document which kind it
is (a CUE sheet read as text, a rip log, an AccurateRip report, a playlist,
or other text), its content type, and for audio, and only audio, the
container its bytes are in (read from the bytes, never the file's name),
duration, sample rate, bit depth or bitrate, channels, and whether it held one
track or a CUE-described disc. Lossless codecs state a bit depth; lossy ones a
bitrate. The role and the kind are kept because the content type can't say
them: a damaged image is stored as plain bytes, and a CUE sheet, a rip log and
a playlist all as text. The scan reads a document's kind from its name: `.log`
a rip log, `.accurip` an AccurateRip report, `.m3u` and `.m3u8` a playlist,
`.txt` other text.
Re-identifying a library release reads its files as the role and kind each
recorded, not from their names: its audio, by the facts its import stored, its
track sheets (parsed again from the file), its rip logs and its AccurateRip
reports. It reads each sheet, log, report and image from wherever this device
can get it: the original on this device, the device's cache or storage. When
none of those can give a file, such as an original that moved or changed, the
reading of the release's Disc ID or artwork that needs it fails with the
reason.

The files of a release include everything in its folder, not only the audio:
the CUE sheet, the rip log, scans of the booklet. A release's folder comes back
whole on export (§4.6).

A release's detail lists the files no track plays from, as the editor's Files
section does (§3.8): every file the scan did not read as audio a track plays
from, in the release's file order.

### 2.3 Artists and credits

Artists credit albums, releases, tracks and works:

| Credit | Table | What it says |
|---|---|---|
| An album's artists | `album_artists` | who the album is by, in credit order |
| A track's artists | `track_artists` | who the track is by, in credit order; a track with none is by the album's artists |
| A release's roles | `release_artist_roles` | who did what on the release: one of bae's roles (Appendix H) or the catalog's own words, with their qualifiers |
| A track's roles | `track_artist_roles` | who did what on the track, in the same terms |
| A work's artists | `work_artists` | who is credited on the work, and in what role: one of bae's roles (Appendix H) or the catalog's own words, with their qualifiers |

An artist row holds a name, a sort name, and the artist's MusicBrainz and
Discogs ids where known. It may carry one picture, bae's own copy (§4.1).

**An album's and a track's artists are each one ordered list.** No credit is
the album's "own" artist with the others added: the first credit is position
0 of the same list. Each place in a list is its own row, so a list that names
one artist twice keeps both credits. Every reader takes the whole list:

- An album's line names every credit in order (see "Credit lines" below).
- Library search finds an album by its title or by any of its credits' names
  (an artist's own name or the other name the credit prints), and an artist
  or a composer by its name, its sort name or any other name its credits
  print. An editor's artist search finds artists the same way, and by their
  library, MusicBrainz or Discogs id. A query matches
  anywhere inside a title or a name, ignoring case by Unicode's lowercase
  (accents still count: `ecrin` doesn't find `Écrin`), and each kind of result
  comes in title or name order, ties going by id. A search that finds nothing
  of any kind shows its no-results state, and a search screen leaves out each
  section that found nothing.
- A track shows its own credits, else its album's, wherever bae names a
  track's artists: its search row, its row in an album's detail, what plays,
  the tags of a saved copy, and Subsonic's songs.
- Sorting albums by artist compares every credit's sort name (else its name)
  in list order, so an album by A comes before one by A and B, which comes
  before one by A and C. The storage table's artist column sorts the same way.
- The artist list and an artist's page count an album for every artist its
  list names, once per artist.

**Credit lines.** A credit names an artist and, when another credit follows
it, what the source prints between them: MusicBrainz's join phrase (" feat. ",
" & "), Discogs's `join` (a comma as ", ", any other word or sign with a space
either side). The last credit's join is never printed, and is not kept. bae
keeps a line as names with typed joins, never as one string. bae recognizes
" feat. ", " & ", " and ", " with " and " vs. " (any spacing and case; a comma
or nothing is the plain join) and keeps any other join as the source wrote it.
The apps show a recognized join in the viewer's language (a language that writes
"feat." in Latin letters, as Chinese does, says "feat."), show an
unrecognized join as written, and join each run of plain names with their
list formatter in the viewer's language ("A feat. B, C, and D"): the album
page and its track rows, the grid, the queue and now playing, search results,
the import list and catalog picker rows. A line that leaves bae with no
language (what AirPlay and DLNA devices are sent, Subsonic's `artist`, the
identification title search, the folder name of Save as) joins plain names with
", " and writes a recognized join in English. Save as's tags hold each credit
as its own value, ARTIST repeated for the track's credits and ALBUMARTIST
repeated for the album's, and join nothing. A track's line is its own credits,
else its album's. Each credit still names its artist, so the artist's page
lists the album and Subsonic lists every credited artist in `artists` (its
one `artistId` is the first credit's).

**An artist's other names.** `artists.name` is the name an artist goes by.
The other names a credit prints for it, a MusicBrainz credit name or a
Discogs name variation (`anv`) that is not the artist's own (the spaces
around either aside), on an album or track credit, a role credit (a release's
or a track's) or a work's artist, are the
artist's `artist_credits`, keyed by the artist and the folded name: one
printed name of an artist is one row. A credit links one of them,
or none to print the artist's own name; it is never text. Importing decides
it once the artist is: a printed name that folds to the artist's own links
none, any other links that name, added if new. A merged artist's credit
prints the stored name it links, else the survivor's name. Credits with no
catalog id match an artist by its own name or any of its other names (the one
it matches is the name the credit prints). The artist page lists the names,
and renames or removes each in one write with every credit linking it:
renamed, they print the new name; removed, the artist's own. Adding or
renaming a name of an artist deleted or merged away meanwhile is refused. The
artist field of an editor lists each found artist with its other names under it; picking
one prints the artist by that name. While the field is open, its list shows
each artist as it is now, so an artist added, renamed or given another name
meanwhile shows as it is. Typing a name, or picking an artist for a tag credit that spelled it
differently, prints the artist's own name.

Two artists are merged into one by any write that finds one catalog id on
two artists, since one id is one artist (below). bae has no merge action yet:
merging artists a person names is for the main app, designed later.

Merging records that one artist was absorbed into the other; both rows stay, so
a credit naming the absorbed artist still resolves. Every read shows the
survivor. A chain of merges shows its last artist. The artist each artist
shows as is kept on the device (`artist_shown_as`), written in the same write
as the artist or the merge that decides it. No write deletes a merge; an
artist a merge names stays while the merge does, and any other artist is
deleted by the write that leaves nothing crediting it (§2.1), with its other
printed names and its picture.

**Which artist a credit names.** One rule decides, inside the transaction that
writes the credit, so nothing landing in between can make it wrong:

1. A credit carrying a MusicBrainz or Discogs id is the library artist holding
   that id, followed through any merge, or the new artist an earlier credit
   of the same write makes holding it. One id is one artist: where several
   library artists hold it, the write merges them into the one with the
   smallest id, which takes the ids and sort name it lacks from the others,
   and the credit names it. Where the credit's two ids disagree, naming two
   artists or an artist that holds a different id of the other catalog, the
   MusicBrainz id decides: the credit's Discogs id is dropped, and the credit
   goes by its MusicBrainz id alone. MusicBrainz' and Discogs' Various
   Artists entries are one artist: a credit naming one by its id alone, which
   no artist holds, goes by both.
2. Otherwise it is the one library artist whose folded name matches and whose
   ids don't contradict the credit. The folding ignores case, accents and
   spacing, and takes a word-final sigma (ς) as σ.
3. Several such artists, or none, make a new artist.

A credit that matches an artist fills in that artist's missing ids and sort
name.

The import pane and the release editor show beside each credit of a draft
the artist these rules name for it now, or that it would be new, or the
several artists it leaves ambiguous. Each artist field (the album's artists,
or a track's own) shows one badge for all its artists: Library when every
artist is in the library, New when every one is new, how many are new, how
many library artists its one credit could be, or how many of its credits
are left to choose. A picked artist counts as in the library. The badges
always name the artists that importing or saving the draft would name.
bae stores the answer of these rules for each credit of a candidate's
draft. The write that adds or changes a credit, or changes an artist the
rules can name for it, stores the new answer in the same transaction,
whether this device made the write or applied another device's, and the
pane reads the stored answers. bae stores no release editor's form,
because what the person types stays in the editor, so the editor's read
of its draft is the one place that applies these rules to credits no
write has stored: it applies them to each credit of the form, and reads
again when a write changes an artist the rules can name.

**Edge cases.**

- **Two ids, two artists.** A credit names MusicBrainz artist X and Discogs
  artist Y, and the library holds X and Y as separate artists. The credit
  names X, and Y keeps its own ids; nobody is asked (rule 1).
- **One id, two artists.** The library holds a credit's id on two artists,
  each under a different name. One id is one artist, so the next write naming
  that id merges them into the smaller id (rule 1) and goes on; nobody is
  asked.
- **A contradicting id.** A credit whose Discogs id names an artist holding
  a different MusicBrainz id goes by its MusicBrainz id alone: the artist
  holding that id, or a new one. A credit whose MusicBrainz id names an
  artist holding a different Discogs id is that artist, which keeps its own
  Discogs id. A credit whose name alone matches an artist holding a
  different id is not that artist (rule 2).
- **Credits that disagree with each other.** A write's credits meet the new
  artists its earlier credits make by rule 1, as they meet library artists,
  and no write is refused for its ids. Two credits sharing a MusicBrainz id
  and carrying different Discogs ids are one artist, holding the first's
  Discogs id; two sharing a Discogs id and carrying different MusicBrainz
  ids are two, the second going by its MusicBrainz id alone. A credit whose
  ids name a new artist of the write's and another artist makes them one
  where neither holds an id contradicting the credit or the other, and
  otherwise goes by its MusicBrainz id. A catalog's credit carries one
  catalog's id and a typed credit none, so only MCP's
  `release_metadata_update`, whose credits carry the ids its caller wrote,
  brings these about.
- **Various Artists holding another id.** A credit naming Discogs' Various
  Artists entry alone, which no artist holds, is the artist holding
  MusicBrainz' Various Artists id, even one an edit gave another Discogs id,
  which it keeps. A credit naming MusicBrainz' entry alone, which no artist
  holds, where the artist holding Discogs' entry holds another MusicBrainz
  id, is not that artist: it goes by its MusicBrainz id (rule 1).

**Record labels.** A label row holds a name, its name key, and the label's
MusicBrainz and Discogs ids where known. A release's label row names one, or
none when it states only a catalog number. A catalog's entry for "no label" is
no label: MusicBrainz's "[no label]" (157afde4-4bf5-4039-8ad2-5a15acc85176)
and Discogs' "Not On Label" (1818) with its "Not On Label (… Self-released)"
forms are read as a release with no label, keeping the catalog number given with
them, and a release whose only label is one states no label. Which label it names is decided by
one rule, inside the transaction that writes the release's labels, an
import's or an edit's save:

1. A label carrying a MusicBrainz or Discogs id is the library label holding
   that id, followed through any merge, or the new label an earlier label of
   the same write makes holding it. One id is one label: where several
   library labels hold it, the write merges them into the one with the
   smallest id, which takes the ids it lacks from the others. Where the
   label's two ids disagree, naming two labels or a label that holds a
   different id of the other catalog, the MusicBrainz id decides: the
   label's Discogs id is dropped, and it goes by its MusicBrainz id alone.
   This is the step a credit's ids take, one rule for both.
2. On an edit, a label carrying no id whose name is the name of a label the
   release names now is that label, so saving a release keeps its labels.
3. Otherwise it is the one library label whose name key matches and whose
   ids don't contradict the label's. The key is the name's words run
   together without the trade words it ends on ("Records", "Recordings",
   "Music", "Ltd" and the like), so *Label Name Records* and *Label Name* are one
   label; a name of trade words alone matches none. Initials are their own
   key: *DFC* is not *Dance Floor Corporation*.
4. Several such labels, or none, make a new label.

A label that matches fills in its missing ids. Two labels merge in any
write that finds one id on both (rule 1); as for artists, bae has no merge
action yet. Both rows stay, and every read shows the survivor, following a
chain as artists do. The label each label shows as is kept on the device
(`label_shown_as`), written in the same write as the label or the merge that
decides it. The artist edge cases hold for labels: two ids
naming two labels name the MusicBrainz one, and a label whose name alone
matches a label holding a different id of the same catalog is not that
label. Two labels of one write sharing a MusicBrainz id and carrying
different Discogs ids are one label, holding the first's Discogs id; two
sharing a Discogs id and carrying different MusicBrainz ids are two, the
second going by its MusicBrainz id alone. A label whose ids name a new label
of the write's and another label makes them one where neither holds an id
contradicting the label or the other, and otherwise goes by its MusicBrainz
id. A catalog states one catalog's id for a label, but a label both records
of an identified pressing state carries both their ids, so an import, not
only an edit, can bring these about. A label of a write named as a library
label the write merges away is not that label.

### 2.4 Works

Works are compositions. Tracks link to them, in the order a track performs
them, and they nest: a movement is part of a symphony, in the symphony's order.
Works come from MusicBrainz, which names them; a work's artists come with it,
every one the work's relations state, each with its role (§3.4).
A work no track performs any more is deleted by the write that leaves it so,
with its credits and parts (§2.1); a work whose part a track performs is
performed through it. A release's detail names the works each of its
tracks performs, in the track's order, each with its composers.

**A release's tracks.** A release's detail lists its tracks in playing order,
in a group for each run of tracks on one side or disc. A track on a record or a
cassette is on a lettered side, the letters counting the sides of the release's
sided mediums in order, across mediums: the detail labels the groups of a CD,
then a record of two sides and a record of one, Disc 1, Side A, Side B, Side C.
A track on any other medium is on a numbered disc when the release holds more
than one medium, and the release with one such medium shows its tracks in one
group with no heading. Each group carries its tracks' playing time. When the
release has more than one group, each group shows its time beside the release's
total. Each track carries its own credits (or the album's), the works it
performs and the stars the person gave it. The detail lists the release's
images, the images a cover can be made from and the files no track plays from,
with the files' count and total size.

**A release's credits.** A release's detail lists who did what on it, every
role. Its credits are the release's own, each track's own, and for each track
the credits in a role other than a writing one of the works it performs, as
that track's (a work's writing credits are the work's composers, named with the
work). They group by role, with its qualifiers, and artist, each with the
tracks it covers; artists who share a role and the same tracks are one row,
and one artist in two roles is two rows. A row covering every track names no
tracks. Rows go in the order of their first credit: the release's credits in
the catalog's order, then each track's by track, then by the credit's order. A
release with no credit has no row. On macOS the Credits are listed above the
Works, which show only when a track of the release performs a work, and list
each work a track performs in the track order, as the
track's number, the work and its composers, and a click on a work opens its
page; on Android a line under each track names the works it performs, a tap on it
opens the work, and the Credits follow the tracks, each row a role at the start and
its artists at the end, without the tracks each covers.
On Android a row of chips under the release picker has one chip for each
catalog that describes the release, and each chip opens its catalog's page. The
Files after the Credits name each file no track plays from with what kind of
file it is.
On Android the album's menu offers *Set as primary release* above *Delete
album…* when the album holds more than one release and the release in view is
not the primary one.

**The composer list** shows every artist credited on a work in a writing role
(Appendix H), and every artist holding a catalog role credit in a writing role
that none of the works they wrote links: a release's credit is linked when a
track of the release performs a work they wrote, a track's when the track
performs one. An artist who produced, engineered or performed and wrote
nothing is not on the list. A merged artist's works and credits count under
the artist it shows as. Each row counts the artist's works, the releases a
track of which performs one of them, and the unlinked writing credits; the
list sorts by name, works or releases, ties going by name, then id. Under its name a
composer's row, its page and its search result say how many works it is
credited on ("1 work", "3 works"), in the viewer's language.

**An artist's composer page** lists its works, then its release credits and
its track credits in every role that none of the works it wrote links, each
part opened by a heading: *Works*, *Credits*, *Recordings*. A credit's row
shows its role in the viewer's language, with its qualifiers in parentheses. A credited work is listed under every work
the library holds that it is part of, each of those heading its group whether
the artist is credited on it or not, with its parts indented below it; a
credited work part of none heads a group of its own, and a work is listed once
in a group however many of its parts are credited. Groups sort by their
heading work's title, its parts after it by title. Each credit is listed under
its release: releases by album title, two albums sharing a title by id, then
the release's place among its album's; a release's credits by track, then
their own order. Titles compare
without case, as the album grid's do. A work shows the cover of its first
album by title, compared the same way, then by id, among the albums with a
release that performs it: that album's primary release's cover when the release performs
the work, otherwise that of the album's first release, in its order, that
does. Opening the page opens the first work it lists.

**A work's page** lists its parts, in the work's order, then the releases
that perform it, then their tracks that do. Releases go in the order the
work's cover is picked by: by album title, compared without case, two albums
sharing a title by id, each album's releases together in its order. Tracks go
by their release in that order, then by place. The page leaves out a section
with no rows.

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

Ana imports two releases by *Artist Name*, known to MusicBrainz, one after the
other. The library must hold one artist, not two: one MusicBrainz artist id is
one row.

So the same fact gets the same id. Artists, works and albums
take their ids from their catalog ids: each is a name-based UUID over a text
key that spells the fact.

| Row | Id from |
|---|---|
| artist | its MusicBrainz artist id, or failing that its Discogs artist id |
| label | its MusicBrainz label id, or failing that its Discogs label id |
| work | its MusicBrainz work id |
| album | its MusicBrainz release group, or failing that its Discogs master |
| work credit | the work, the artist and the position |
| work part | the parent and child works |
| release record | the release and the catalog |
| cover | the release |
| artist image | the artist |

Every other row gets a fresh id, a place in a list (an album's or track's
credits, a release's labels, barcodes, media and mediums) included: a UUIDv7,
whose first 48 bits are when it was made by the device's clock, so one write's
new rows sit together in the store's indexes. Anyone who can read the row can
read when it was made, as its `created_at` already says.

A row's id is its table's key
([coven §9.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#92-keys-and-unique-values)),
so one fact is one row.

**Edge cases.**

- **An artist or a label known only by name** has no catalog id to take an id
  from, so it gets a fresh id. If a later write gives it a catalog
  id, it keeps the id it has; row ids are never rewritten.
- **A new artist or label given another id within its write.** A new artist
  or label takes its id from the catalog ids it holds once the write that
  makes it has decided every credit or label. A label
  first named by its Discogs id alone, which a later label of the same
  release gives a MusicBrainz id, takes the MusicBrainz id's; an artist
  first credited by name alone, which a later credit of the write names with
  a catalog id, takes that id's.
- **An album with no group.** A release whose metadata came from its tags names
  no group, so its album gets a fresh id.
- **A second catalog's group.** A Discogs import whose record pairs with the
  MusicBrainz group of an album already in the library joins that album
  (§3.7); where the two records name two library albums, the one the draft
  was read from decides.
- **A list saved whole.** A save of an album's or a track's credits, or of a
  release's labels, barcodes, media or mediums, replaces the list: the rows it
  held go and the rows it names are written, numbered from the start of the
  list, so positions have no gaps.
- **A credit name removed.** Removing an artist's other name unlinks every
  credit linking it in the same write, so no credit prints a name its artist no
  longer holds.

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
release: each one's path relative to the release folder, as the release names
it (§3.1), its size and its modification time, sorted, so moving or renaming
the folder doesn't change its hash. Each device records, for itself alone, the
hash of the files each of its imports read (`local_release_origin`); the record
never syncs, and the release carries no hash. It does one job: a folder whose
hash it holds is on Imported in this device's import list (§3.2), wherever the folder
sits now.

One hash is one release on a device. An import of files this device already
imported is refused when it is asked for, and nothing runs; the person is told
this device already imported those files. An automatic import refused this way
records no failed import: the folder is on Imported. Two imports of the same files asked
for before either lands both run, and the one that comes second finds the hash
taken, as it reads what to import or as it writes: it fails with the same
words and writes nothing. The release imported
first stands. Deleting the release deletes the
record with it, and the folder is back in Found as a folder whose tags bae
never read: an imported folder holds no draft (§3.7), so the write that
deletes the release stores the folder's blank draft and its own cover, and its
row shows its folder's name (for a release read from several folders, the grouping's name) until the person or an identification writes its draft. bae identifies the folder like any folder in Found, and Reset to
file metadata reads its tags again.

The hash reads no bytes, so it tells files apart by name, size and time alone.
A copy to another disk usually changes the modification times, so the same rip
copied elsewhere usually hashes differently and is a folder of its own to
import, as Ana's backup is. A copy that keeps them hashes alike and is on Imported
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
choose; such a grouping reads exactly those folders. Combining selects the
release it makes. When a release's files come from several folders, its
pane lists those folders at the top. A combined release's pane offers Keep as Separate
Releases, which separates it at once, with no confirmation, and selects the
releases it returns.

Files under a folder that no release there owns, such as a cover beside two
discs kept as separate releases, are a *sidecar*. At most one release reads a
folder's sidecar.

**Watching.** Each watched folder has its own filesystem watch. A change reads
again only the folder directly under the watched root that it reached, and
only after that folder has been quiet for a second, so a download writing into
one album for an hour holds back only that album. If watching panics, bae
reports a bug and stops every watch; scans, refreshes and the network check go
on. A change that arrives while bae can't read its list of watched folders
matches no folder, so bae reports a bug and reads nothing for it; the next
change under the folder, a refresh, or the next launch reads it. A directory
that cannot be watched
(inotify's watch limit on a large tree, or the watches not starting) is not
hidden: the tree is read and stored as usual, and the folder's status is *not
watched*, with the first directory that failed and the system's reason. The
import list marks the folder as it marks a failed scan, and an alert,
headlined "Not watching {folder} for changes", shows the reason. A folder read
again after a change reports it the same way.

A watched folder on a network volume is watched like any other, but on macOS
and Linux a network mount reports only the changes this machine makes (Windows
does report a server's changes on an SMB share). bae also checks such a folder
every 15 minutes, on every platform, which is what notices a change made on the
server or from another machine, and the import list says so. bae checks each
folder whose last scan found it on a network volume, the volume the list shows.
A check that panics is reported as a bug and reads the whole folder.

**What a scan stores.** Everything a scan reads is device-local: the
candidates, their files and the role each plays (audio, track sheet, artwork,
damaged image, document, other) with a document's kind, the parsed CUE
sheets, what each audio file's tags say of its title, artists, album, album
artists, compilation flag,
years and disc and track numbers, or, for tags that don't parse, why, and when
each folder was first seen. A file
is stored by its name in the release and by its path, as the system's own
bytes for it, which is what opens it again. A scan stores what it finds as it
goes, a few entries in each write, so an interrupted scan keeps what it stored
and the next scan finds the rest. Completing removes what it didn't see, in the
same transaction that marks it complete. A failed or interrupted scan keeps
both old and new entries. A scan that panics fails as an error does, with the
panic's message, and bae reports a bug. A panic while removing a watched folder
fails the removal the same way, and the folder is read again. bae refuses,
whole, an add or a removal of a watched folder that would leave a watched
folder it can't list. Storing a candidate takes the same time however much else
the watched folder holds.

**How a scan's state shows.** A folder shows how its current scan ended:
complete, failed with the error, or not watched with the reason. While a scan
of all of the folder runs or waits to run, or a removal holds the folder, the
folder shows as scanning, and it never shows as scanning with nothing scanning
it, even after bae quits during a scan. A watched folder that no scan has
started shows Not scanned. bae keeps which folders a scan is reading, and each
failure it could not store with the scan, in the device-local table
`folder_scan_held`, which the library empties each time it opens.

When bae can't store that a scan or a removal is starting, it starts nothing
and tells the person why. A scan whose failure bae can't store with the scan, a
whole scan's or a changed folder's reading's, still shows as failed with its
error until the folder's next scan starts, with the volume the scan found the
folder on, so a folder whose first scan could not even store its start shows
too. A refusal that is a bug is reported as one; a full, failing or damaged
disk, a store that failed as a whole (on an earlier bug too, which was reported
as it happened), or the library closing is not. A scan that ended with nothing
stored, as one that panicked while storing its failure does, shows as failed,
saying it stopped before bae stored its result.

The folder scans list every watched folder that has a status, with that status
and, once a scan has found its volume, whether that volume is on the network,
and say whether any folder's last scan failed or left it not watched and
whether bae is scanning any folder. A scan stores its start, with the folder's
volume, as it begins. Until the list has loaded, the menu's Folders section
says *Reading…* and lists no folder. The scan indicator lists each folder bae
is scanning, with how many folders the scan has stored so far.

On a Mac, a folder whose scan failed, or that is not watched, raises an alert
headlined "Could not scan {folder}" or "Not watching {folder} for changes",
with the error or the reason. The app keeps, while it runs, the problems it
raised an alert for, each by its folder, kind and detail. The folder scans
list's query takes those problems in its request and lists each problem not
among them, so one problem raises one alert while the app runs, and a problem
still stored when the app starts raises its alert again.

**What a scan reads.** A scan reads each file once. For each track it checks
whether bae can play it and reads its tags, which place it in play order, name
a broken folder, and start a new release's draft, picture included. A folder
this device imported gets no draft: its release is the library's. A folder read again within the same scan, such as disc
folders read as one release after each was read on its own, reads none of its
files again. A track that can't be read at all — the file won't open, or a
read of it fails partway, as when a network share goes away, whether the probe
or the tag read meets it, and whether the tags are read to place the track or to seed its folder's draft — fails the pass with the file's name and the
system's error; that says nothing about the file, so the pass keeps nothing
about it (it is not damaged audio, and its folder gets no blank draft) and the next one reads it again. Only bytes
that read fine and are no audio bae plays make a track damaged.

A rescan, such as the one every desktop launch makes of each watched folder,
takes what the last scan of the folder stored for each file whose path, size
and modification time are unchanged (§2.7): a track's stream and tags, or why
its tags don't parse, whether an image is damaged, what a CUE sheet parses to,
and what a broken folder's tags call it. Tags that don't parse, such as a
comment block cut short, are a fact of the file as it is, so a track whose tags
don't parse is not read again until it changes. It opens only the files that are new or changed, so a rescan of a
folder nothing in has changed opens none. A release's draft takes each
unchanged track's tags, and the picture they embed, from the reading stored
for the release, and reads only its new or changed tracks; the stored picture
is read back only when the track it came from is unchanged. A rescan leaves a
release whose files are all unchanged exactly as it was. A store that says two
different things about one file fails the scan.

**Edge cases.**

- **A broken folder.** A folder with corrupt or zero-byte audio, or no valid
  audio at all, is listed as skipped with its reason. It can't be imported.
  The scan reads every file before judging it, so it stores each file of the
  folder with what it made of it — each damaged track with why: it is empty,
  or holds no audio bae can read — and the album title and artist its
  playable tracks' tags agree on, read as a draft's are. The scan also
  stores with them why bae can't import the folder and the folder's cover
  image, which its row and pane show.
- **A file whose name isn't valid UTF-8**, which Linux allows (a Latin-1
  `Café.flac` from an old rip), is read and imported like any other. Its name
  in the release, which the library syncs and every surface shows, is its
  text with each byte that isn't UTF-8 written as `\x` and two hex digits:
  `Caf\xE9.flac`. Its path is kept as the system's bytes wherever bae opens
  the file again: the scan's rows, and the original a local release plays
  from (§4.2). Windows allows a name with an unpaired UTF-16
  surrogate, which escapes the same way and imports there too: a name in the
  release is a label, never read back as a path, so the backslash that
  separates folders on Windows is one of its characters (§4.4).
- **A folder whose name isn't valid UTF-8** is read like any other. It is
  known by its path as the system's bytes everywhere bae keys a folder: the
  release it holds, the watched folder, its place below the watched folder,
  the groups and nesting of the import list, and every decision about it
  (skipped, read as one release, kept apart). Its name, and its place below
  the watched folder, show escaped the same way: `Caf\xE9`, `Artist/Caf\xE9`.
  Identification reads its name as it shows, so a catalog number in its
  brackets still counts.
- **Two files that show one name**, such as a Latin-1 `Café.flac` and one
  spelled `Caf\xE9.flac`, can't both be in a release, which names each file
  once. A release that would hold both, including the files of a folder it
  takes in, is listed as skipped, its reason naming the name. So is a release
  read from several folders two of whose names show the same, such as a
  Latin-1 `Café` and a folder spelled `Caf\xE9`, each holding `01.flac`.
- **A damaged image** — zero bytes, or a file whose first bytes are not those
  of the format its extension names (JPEG, PNG, WebP, GIF or BMP; an image
  with any other extension passes) — doesn't skip the folder: the release is its audio. The image
  is never artwork, so it is never the cover, and it imports as plain bytes the
  library never shows as a picture. It is still carried, so it comes back on
  export.
- **A CUE sheet that disagrees with the disk** — naming absent audio, failing
  to parse, or naming a codec bae can't play from one file — is left unbound,
  and the folder stays importable.
- **A watched folder inside another.** When the person watches a folder that
  contains watched folders, bae stops watching those folders and watches the
  new folder in their place, in one write. bae stores what it knows about a
  folder under the folder's path on disk, so the person's groupings stay.
  Unwatching the folder that covers a grouping deletes it. bae refuses to
  watch a folder inside a watched one, stores nothing, and names both folders
  in the error. No two watched folders ever overlap.
- **A dismissed candidate** is skipped by path, so a later scan doesn't offer it
  again.

### 3.2 The import list

With no watched folder, the Import screen has no list: bae's sheep plays
there, every few seconds taking in made-up releases or playing on its own,
above *Add a folder to import music from* with *Import Folder…* and *Add
Discogs Key…*; with reduced motion asked for, it holds still. Until the
watched folders have been read, the screen shows a plain surface: a library
not read yet is not a library without folders.

The import list shows every candidate in one of three tabs:

| Tab | Holds |
|---|---|
| Found | this device imported no release from the folder's files, and not skipped; including a failed import |
| Imported | this device imported a release from the folder's files (§2.7); what is still uploading first |
| Skipped | dismissed by the person, or broken |

A tab reads *Nothing here yet* only once its list has been read and holds no
rows (*No matches* when the filter narrows it to none); until the list has
first been read, it is a plain surface.

Imported lists first the releases that still have files in coven's upload queue,
the first queued first, and then the rest in the list's order. Only a
release's own files count, as the Storage manager counts them (§4.2).

Each tab sorts newest first, oldest first, or by path either way. A date order
sorts by each row's date — its folder's for Found and Skipped, its release's
import for Imported — with undated rows last; a path order sorts by watched folder
and then by path, in natural order ("Disc 2" before "Disc 10", case aside). A
release combined from folders in several watched folders sorts by its first
part: that part's watched folder and path.
On Found, the list shows a release nested below a folder in that folder's group,
and it also shows there a release at that folder while another release sits
below it: the group sorts by its newest member's date and its folder, its header
leads its members, and folding the group shut keeps the header, still counting
its members, and leaves the members out of the list. The screen keeps which
groups it folds, and each read of the list takes them: a group that leaves the
list and comes back under the same folder comes back folded. Folding or opening
every group is one change, and it holds for the groups Found gains later: once
the person folds every group, Found shows each new group folded too, until the
person opens that group. A header offers to read its
folder as one release (§3.1) when a stored reading of the folder keeps its
releases apart, or, with nothing stored, when the folder is the nearest one
above some release that holds two releases or more, counting the releases of
every tab. While the rows on top are a group's, that group's header stays pinned
over the top of the list; over a release in no group no header is pinned. The
pinned header is the top row's own group's, however far above it the header's
row is, and it leaves with the group's last row as that row scrolls away.

Each Found row is in exactly one state. The first that applies wins:

1. **Importing**: an import is queued or running.
2. **Identifying**: a lookup is queued, running, or writing an answer that
   picks nothing unattended. A lookup that picked its pressing unattended
   (§3.5) puts the row in **Identified** while bae drafts from it and
   fetches its cover; the row offers Import then too (§3.7). A row shows a
   lookup at work, waiting for its run or running, only while it is
   Identifying, so one whose pick is being drafted shows none, as Identified
   counts it.
3. **Error**: bae could not store a lookup's answer, or could not read what a
   lookup starts from or whether an answer is stored already.
4. **Import error**: the last import failed, or the release can't be worked on
   as it stands.
5. **Identified**: linked to a catalog pressing, or to an album whose pressing
   is unknown (§3.5).
6. **Not looked up**: no lookup is stored for the folder's files.
7. **Unmatched**: the person kept their own draft over what the lookup offered.
8. **Error**: bae broke on its own side and the stored lookup ended there,
   such as when it could not keep the albums the run read.
9. **Lookup error**: a catalog could not answer, or could not hand over the
   cover of the release the lookup picked.
10. **Unmatched**: the lookup took its release for the folder and the person
    set it aside.
11. **Needs you**: the lookup left the answer to the person.

A Found row's title line reads from its draft. A row with no draft, from the
tags or anywhere else, names its folder. A row with one shows the draft's
album title and credits, and, while the draft is linked to a catalog's
pressing or album, an arrow after the title: its facts were read from a
catalog record. On every other row the arrow is hidden, not removed, so the
title line keeps its width whichever way the row reads.

A filter narrows Found to one group of these states: needs you, in progress,
identified, unmatched, or not looked up. Each entry counts the Found rows it
holds, whatever the list shows. Text typed into the search keeps the rows
whose shown title or artists contain it, case folded as library search folds
it (§2.3), and a group's header while any of its members is kept; the list
says how many of the tab's rows it keeps of how many it has.

**Imported.** A folder is on Imported when this device imported a release from its files.
This device's record of the import is keyed by the folder's hash (§2.7), not
its path, so a folder moved or renamed after its import stays on Imported. An Imported row
shows the library release imported from the folder's files as the library
holds it now — its album's title and credits, each printed as the library
prints it, its cover, and the arrow of a release a catalog's record describes
— so retitling the album, editing its credits or merging an artist they name
changes the row at once. It says nothing more but its release's
upload: the tab says it is done. Its pane is that release, to edit as the
library editor edits it (§3.8), with the folder's images and other files as
its scan read them: the folder holds no draft once imported. While files of the release wait in this device's
upload queue, the line under its title names the upload's phase, as the
Storage manager does (§4.2): Uploading while any file is being sent, beside a
bar of the bytes of the release's whole size stored or sent, or Queued to upload with no
bar while its files only wait. The row changes as each file starts and
finishes uploading, and its bar moves as bae sends bytes. The row names the one release imported from those
files, which Show in library and a double-click on the row open.

A folder this device didn't import is in Found like any other, whatever another
device imported and whichever library releases came from the same files. Ben
copies Ana's rip folder to his laptop, keeping the modification times. Ana's
release syncs to him, but his scan finds the folder in Found, his to import or
skip.

Imported offers no re-import. A second copy comes from importing another folder,
such as a copy whose modification times changed.

**Skipped.** A broken folder's row looks like a Found row: the folder's own
cover when an image in it is undamaged, the album title and artist its tags state
or else the folder's name, and under them why it can't be imported, such as
"2 audio files are damaged" or "More than one file is named Caf\xE9.flac".
Selecting it opens it the way a Found release opens, read-only: its images,
then every other file with each damaged one marked and why. Where a release's
Import would be, the pane states the reason. A playable track in it can still
be auditioned.

The person can select any number of rows. A change the person makes to the view takes out of the selection the rows it stops showing:
another tab, filter or search takes out each selected row the new view doesn't
show, a fold takes out the selected rows it hides, and a row they hid comes
back unselected when it is shown again. A row that leaves the list through a
change to the row itself stays selected: a row that starts importing under
Identified, a Found row whose import finishes, or a folder the scan no longer
finds. The list draws no highlight for it while it is hidden, draws it
selected again if it comes back, and no action applies to it while it is
hidden. So a selected
Found row whose import finishes moves to Imported, but the list stays on Found,
the row stays selected and its pane stays open, showing the release as
imported (Imported's pane, above) with its way to open the release in the
library, until the person selects another row or changes the tab, filter or
search. An edit in a row's pane or a pick changes the row in place, and no
step of it shows the row as something it isn't, such as unlinked on the way to
linked again: typing into the selected row's pane keeps the row selected and
the pane open, under any view that still shows it. A release the scan reads
again with changed files is the same row: it stays selected, under a group as
folded as it was. With nothing selected, the pane shows nothing selected. A
bulk action, combining, keeping separate and showing the folders run over the
selected rows the list shows that offer them, in the order the list shows
them, and a bulk action names the rows it failed on. While a bulk action runs,
its bar says how many rows it has done and fills to the share done; until the
action has counted the rows, the bar moves without a count. A broken folder
offers no action, so selected among others it counts toward the selection but
holds back combining it. Combining selects the release it makes, and keeping a
release as separate releases selects the releases it returns. Shift-click
selects every release the list shows from one release to another in its order,
and Select All every release it shows, including releases not yet on screen.

Each offer shows the number of rows it runs on and whether it can run. A row's
own offers act on that row alone and leave out Combine. A selection offers
Combine for every selected row while every one can join one release, and not
at all otherwise. The tab bar shows every tab's count, whatever the person
narrows the list by. A folder on a slow or unreachable volume does not hold up
the list. A search shows its rows as they arrive, and its "N of M" and the
list's length once it has counted them, with a placeholder in their place
until then.

The import list handles up to 100,000 folders. bae stores the tab bar's counts,
the filter's counts and how many of each group's members each filter entry
holds, and changes them in the same write that places, moves or removes a
row, so no write makes bae count the whole list again.

Another tab, filter, search or order is a view the list has not read yet:
until its first rows arrive, the list keeps drawing the rows it has, and
shows neither *Nothing here yet* nor *No matches* for the new view (§9.2).

A list narrowed anew (another tab, filter or search) shows on top the row its
view opened at: its first selected row as it sat when the person asked for the
view, or its top while no selected row is shown. Changing the order keeps the list's scroll
and every selected row, and the list draws the rows in the old order until the
rows in the new order arrive. Selecting rows moves the list nowhere. As rows
come and go, the list keeps the release on top where it is, its filters and
folds included, unless the person scrolls first. A header or a broken folder on
top keeps its position.

Revealing a candidate (the first one still being identified, or one the person
asks for) shows it on the tab its placement puts it on, with the filters
cleared and its group opened, the list's other folds kept. The list then
scrolls to it; a candidate the view no longer shows is scrolled to nowhere.

The pane reads its one candidate's rows by the candidate's
key, and so does every read of one candidate's state, draft or preparation, so
how long one takes does not grow with how many candidates the queue holds.
The pane reads each of its parts with a statement of its own. Each statement
binds only the key and finds the candidate's scan row, and the content hash
the scan stored, by that key, never by a value another statement returned.
A scan that stores other folders writes none of the rows the pane reads, so
coven reruns none of the pane's statements for it.
The parts are the folder's files, with the track count and the audio summary
that the statement computes over the scan's rows; whether the person skipped
the candidate; the release this device imported from it; its current state
and its verdict's cards, with the failure lines its ledger does not show; and,
for a candidate this device has not imported, its draft, its cover choice,
its mapping table with each row's draft track, what the library holds for
the draft's credits, the linked release's records, the covers the picker
offers, the linked release's library status and the album an import files
under. The statement for the records and the covers returns none unless the
candidate's state is current for the scan.

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
  title. A file whose tags don't parse states none of these, and the other
  files' tags still count.

A file that can't be read at all, such as a rip log, a text file or an audio
file's tags on a drive that fails, is not a file that states nothing: the
run fails with the read's error, and identifying again reads it anew. A rip
log whose table of contents has a row that doesn't read, or whose track
numbers don't run 1, 2, 3 in order, fails the run with that, since the Disc ID
of a table with a row missing would look as valid as the real one; so does a
CUE sheet that can't be laid out as a disc, such as one naming an audio file
with no measured length. A rip log with no table of contents at all states no
disc, and is passed over.

Identification reads the audio's tags from the files on every run, so the
scan's stored reading keeps none of the ISRCs, labels, copyright lines or
store markers: nothing else uses them.

Text in the files is often in a code page older than Unicode. A CUE sheet, a
rip log or a text file is decoded from its bytes: a byte order mark names its
encoding, else text that is valid UTF-8 is read as UTF-8, else bae detects the
code page, such as Windows-1252 or Shift_JIS. Tag text that declares Latin-1
is decoded from its bytes the same way, since taggers wrote other code pages
under that mark: an ID3v1 tag's title, artist and album, and the ID3v2 title,
artist, album artist, album, publisher and copyright frames marked Latin-1.
Tag text declared UTF-8 or UTF-16 is taken as it is.

Reading the artwork means reading text off images, which bae does on macOS
only. An image the system can't decode or read is not an image with nothing
printed on it: the candidate's artwork scan fails with the image's path and the
reason, its barcode and text signals show that failure, and the next run reads
the image again.

Ana's folder yields a Disc ID from her CUE sheet, the barcode from her scan of
the case, LBL-1001 from the folder's text, and eleven ISRCs. The fast clues
come first, so the disc-ID lookup need not wait for the artwork to be read.

**Where the audio came from.** The files can prove a medium:

- **A CD rip**: a rip log with a table of contents bae can parse (the layout
  Exact Audio Copy and XLD write; a log without that table proves nothing), an
  AccurateRip report with an `[AccurateRip ID: ...] found.` line, or a CUE
  sheet with Exact Audio Copy's `REM COMMENT "ExactAudioCopy ..."` line. No
  other ripper's CUE sheet proves a CD rip.
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

bae looks the clues up on MusicBrainz and Discogs. Each clue is asked once of
every catalog that answers its kind, and each catalog answers for itself: one
that fails leaves the matches of the other, and the run pools the answers once
everything it asked has answered.

| Clue | Asked of |
|---|---|
| each Disc ID | MusicBrainz |
| ISRCs | MusicBrainz |
| barcodes | both |
| catalog numbers | both: a number the person picked as it stands; one a found release carries as it stands on MusicBrainz, under the offered releases' artists on Discogs |
| album and artist title | both, only when no Disc ID, barcode or catalog number returned anything and the draft has a title; ISRC matches don't count, since a recording is on every compilation that reissued it |

Discogs needs an API key; without one it is not asked. Its switch moves
without a key: switched on with none, Discogs waits for one, and switched off,
its key is cleared (Settings asks first when one is saved). Saving a key, or
checking a stored one again, asks Discogs whether it works, and the key's
setting says so while it asks. A check that never reaches Discogs leaves the
key stored and unvalidated, which says the person is offline, and is checked
again on request; one Discogs answers with no verdict on the key (an error
status, an unreadable reply) stores nothing and tells whoever asked. A call
with the stored key that Discogs answers 401 marks it rejected, and one it
answers confirms a key not yet checked. What a check or a call says of
a key is recorded only while that key is still the one stored: an answer
about a key replaced since leaves the new key as its own save recorded it. A
Discogs label or artist name
loses the " (2)" Discogs adds to tell two of one name apart: the
id tells them apart, and the name is what a release prints. Either catalog can be
switched off, but not the last one asked. A library left asking none, its
last asked catalog having lost its key, has nothing to search, and Find online
says so. A catalog switched off is asked nothing: a read
of a release asks its own catalog, and of the documents its pages link, only
those on a catalog switched on; the rest it names as not asked, and a later
read asks them once their catalog is switched on. MusicBrainz is asked no more than once every 1.2 seconds, under
the rate it allows.

**Which lookups go first.** Each catalog's calls wait in three lines, and the
next call allowed goes to the first line holding one, oldest first within it:

1. **Awaited**: a person is looking at the screen the call fills. A typed
   search, the covers of the picker, a pick and the reads that apply it,
   linking an album, saving a Discogs key, a library release identified
   again from its sheet, and every lookup of the run of the candidate a pane
   has open.
2. **Requested**: a person asked for it and isn't watching it, such as the
   run of a release they asked to identify, the artist pictures an import
   fetches, and checking a stored Discogs key again.
3. **Background**: what nobody asked for, the runs of releases a scan found
   on its own.

Opening a candidate in the pane moves its identification up: if its run is
still waiting its turn (runs go four at a time, below), it starts next, ahead
of every other waiting run, and every lookup of its runs, those already waiting
included, goes in the Awaited line. When the pane shows another candidate or
closes, its lookups go back to their own line, behind the calls that were
there before them. The spacing and any wait the catalog asks for hold every
line together.

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
   disambiguation alone. The release is read naming every other document
   its pages link as one the read stopped short of, and its own page too
   where the read took it from a Disc ID answer. A person's pick reads the
   rest, that page included, before it applies anything. An unattended pick
   reads nothing more: its draft is read from what its run read, and the
   import reads every release the candidate is linked to in full before it
   writes anything, so an import whose read fails fails. The import then
   stores the releases it read and commits the records stored for what its
   link claims (§3.5). The Disc ID lookup
   asks for the release's artist relations, so the release read from its
   answer states the artists it credits as roles. Runs go four at a time.
   That complete read of a Discogs release asks MusicBrainz in one request
   which of its releases links the release's page and which of its release
   groups links the master's, since the release's page names its master; a
   page MusicBrainz knows nothing of is linked from none.
3. Ask what came into effect: a catalog number a found release carries that the
   folder states, searched on MusicBrainz as it stands and on Discogs under
   the artist of each release the run offers, whichever release carries the
   number; an offered release's pressing on the catalog that has no record
   of it, by the release its page names there, or where it names none, by
   every barcode it prints and every catalog number it prints under a label,
   each asked on its own, and on Discogs under the artist it credits. A short
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
states none. A year identification guesses from text (a folder's name, a title) is four digits from 1900 to 2099; a year a source states outright (a tag's date, a catalog's release year) takes any value. The folder's years are read against every record of a row: a
year any of them says the album first came out confirms the album, and the
folder names the pressing only by a year after all of them. An album card names the label every pressing
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

Wherever two labels are compared (a pressing's records listing their labels
each once, statement 5, the releases a catalog-number search keeps), two
names are one label when they read alike once the trade words they end on are
dropped, or one is written as the other's initials; two names of trade words
alone are one only when written alike.

A stored catalog release, and each match a run keeps, holds the albums the
statements about it named, in the order they were read, strongest first;
which statement named each is not kept, since nothing past the reading
decides by it.

**A release's mediums.** A MusicBrainz release lists its media, each with its
format, its own tracks, and the Disc IDs registered against it. Discogs lists one flat tracklist and states the
media as format entries with quantities, so bae splits the tracklist by its
positions:

- `2-5` is the fifth track of disc 2.
- `CD2-5`, `CD-5` and `BD-1` sit on a disc of the carrier the prefix names
  (CD, DVD, BD for Blu-ray, SACD, VHS, LD for LaserDisc): disc 2 of the CDs,
  or that carrier's only disc.
- `A`, `A3` and a sub-track's `A3a` are on side A of a record or cassette.
  A record holds two sides, the
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

**A release's credits.** A Discogs release credit that states `tracks` is a
credit of each track it names, not of the release. The text is a comma list
of items, each a position (`3`, `A1`, `50-1`) or `X to Y`, the rows from X
through Y in tracklist order, across sides and discs; positions compare as
written with case aside, spaces and empty items are ignored, and a row's
sub-tracks are part of it. An item that names no row is skipped, and a credit
none of whose items names a row is dropped, each with a warning naming the
release. A collapsed index takes the credit once however many of its
sub-tracks the credit names, and a credit on tracks of a medium the folder
doesn't hold reaches no track. Every artist relation MusicBrainz states is
read, in the relation's order: a work's is the work's credit, a recording's
the track's, a release's the release's. Each is a credit in the role its type
and attributes state (Appendix H), printing the name the relation credits as
one of the artist's other names (§2.3); a relation naming several instruments
is one credit per instrument. One artist in two roles is two credits, and an
artist stated twice in the same role on a work, a track or a release is one.
A Discogs credit's role string is read role by role: it splits at the commas
outside brackets, each part is a credit, and a bracket qualifies its role.
Every role is kept.

**Which of its mediums the folder holds.** The run, a pick, the draft it
writes, the shared album and a library re-identify all agree on which of a
release's mediums the folder holds. The mediums
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
  in the library plays no part, so what a run keeps doesn't depend on what the
  library held when it ran; a row shows whether its pressing is in the library
  now.

The last point of the ranking reads the catalogs' free-text notes. Discogs
writes links and formatting into a note as markup; bae reads the note as the
text it shows (`[l=Label Name]` is Label Name), and a link that names only an
id stays as written. A word that sets one pressing's notes apart, which the
folder's text also states, ranks that pressing first among otherwise equal
ones.

A run's answers are kept with its verdict, so they re-rank without being
fetched again, and a release whose document it read is stored on the device
(`source_release` and its tables, Appendix A) for as long as something names
it. A catalog release is stored only while a row names it: a match of a
verdict, of a run in flight or of a typed search, a draft read from it, a link
to it, or a linked pressing it partners. A fetch whose release no row names,
such as a library release's re-identify applying a release, stores nothing.
The write that drops the last row naming a release —
a verdict replaced or cleared, a run ending, a search replaced, a pick of
another release, an unlink, an import, a folder leaving the queue — deletes
it. A write that names a release already stored replaces it only where its
copy read a document the stored one could not, so a copy read earlier never
overwrites a fuller one. So is the run as the pane lays it out: how each
clue stood, its rows, and each catalog's answer to each row — the releases it
found, nothing, a failure, or why nobody was asked. Every release a lookup
found is kept, also one no list shows because its tracklist doesn't fit or it
looks like an offered one; only its lookup shows and counts it. A release is
kept once, as the run last read it, and every lookup that found it shows it
that way, while the run is going and after it ends, so a candidate opened
later shows what its run showed. Its track titles and notes are not kept. A
run with no clue to lay out keeps none.

A stored release keeps no album year or media count of its own: its album
year is the year the album first came out, else the pressing's year, and its
media are counted from its mediums or format entries.

### 3.5 Picking a pressing

One matching pressing is picked automatically; otherwise the person picks,
searches, or edits.

A run *picks unattended* only when all of these hold:

- the folder's own files rule out none of the pressings found;
- it found exactly one pressing, two catalogs' records of it counting once;
- the run read every record of that pressing;
- its tracklist lists as many tracks as the folder holds.

The release a run picks is the lead: the offered release with the lowest
position. The import list's reason, the check the pane shows beside Import and
the release the run applies always agree on that pick, or on why the run picks
none.

Ana's run finds one pressing, on both catalogs, read in full, eleven tracks.
bae picks it, and her row reads Identified at once. bae writes the metadata
of the releases her run read into her draft, with the cover it fetches. Until
that write ends, her pane shows the pressing selected and being drafted; an
answer that picks nothing drafts no pressing, and the pane shows none. A
Disc ID often returns several pressings of one album: an exact clue is not a
unique answer, and choosing between them is the person's.

While the run is out, the pane names in words each thing it still waits on:
a Disc ID or ISRC still being read from the folder, a catalog still being
asked about a clue, the title search waiting on the clues, and the artwork
still being read, which can add barcodes and catalog numbers. The search
form says the artwork is still being read under the fields its text
suggests to.

The pane's identifier chips list every value they hold: the ISRC chip writes
every code the files' tags carry, separated by " · " and wrapping inside the
chip, and its accessibility label reads them all. Catalog numbers the folder's
text offers that no release confirms are one chip listing every one, each a
button that puts that number into the run; none is folded behind a count.

A pressing's row lists every record's value where its records state a fact
differently: the year, a catalog number, a barcode, the country, the format.
Each value is followed by the catalogs that state it, `1992 MusicBrainz / 1993
Discogs`; a fact the records agree on, or only one record states, is listed as
stated with no catalog. Two spellings of one catalog number or barcode are one
value. The row lists the notes of its records the folder's text names it by,
each followed by its catalog where they differ, and the barcodes it states.
What the row lists is what look-alike rows are compared by, so two pressings
that differ only in what a second record states are two rows. Picking the
row still applies its lead record alone. An album's card lists every title,
every artist and every label its records state, each once, `Low Tide Hours /
Low Tide Hours (Remastered)`: titles, artists and labels that differ only in
case, accents or punctuation are one. The summary line of an import pressing row
lists the format, then the country, `CD · US`; the library's release summary
keeps the country first. A row whose lead record names a pressing the library
already holds shows dimmed and marked In library, on every list of cards (a
verdict's, a ledger cell's, a typed search's), and the mark comes and goes as
the library gains and loses the release.

When the run doesn't pick, the pane says why: nothing found, several
pressings, a record that couldn't be read, or a tracklist that couldn't be
checked. A folder whose files rule out every release found (§3.3) counts as
not found: its row waits on the person as Not found, and the pane still lists
the ruled-out releases among the excluded ones, where they can be expanded and
picked. The person then:

- **picks** one of the offered pressings;
- **searches** by artist and album, catalog number or barcode, with each
  catalog's results landing as they come, and a line naming each catalog
  still being searched; *Retry* asks again only the
  catalogs that failed, keeping what the others returned. A library that
  asks no catalog is in an invalid state: a search started there fails, and
  bae stores no search. An answer bae cannot
  save counts as that catalog's failure, so Retry covers it. bae reads
  nothing more about a result until the person picks it, so a search's cards
  show two catalogs' albums as one only once a pick has read a MusicBrainz
  release's documents;
- **links the album** when several pressings of one album are offered and they
  can't tell which is theirs. The link names the album, not a pressing, and the
  fields every offered pressing agrees on fill the draft;
- **keeps their own draft**, from the folder's tags or typed by hand.

A pick applies its release to the draft at once (§3.6), an unattended one as
soon as its draft is read from what its run read. Nothing previews the
release's tracklist first: the draft the pick writes is what the pane shows. While a person's pick waits
on the network, the row it was made on names what it waits on: reading the
release, and then each other catalog's release of the pressing, from its
catalog, or fetching the cover from the catalog that offers it. A release the
library already holds whole is read without a line, and the rest of the
pick, which is local, shows none; the row takes no second pick meanwhile.
The pane shows selected the row of its cards that holds the release bae
linked the candidate to. While bae reads the release a run picked unattended,
the row that holds it says bae is reading the release, and shows selected too
until bae links the candidate to a pressing.
A pick claims its release and, where the pick pairs another catalog's, that
catalog's release. The link holds the claimed releases' records: each claimed
release's own record outranks what another says about its catalog, the
applied catalog comes first, and the albums kept for the releases' albums join
them. The link's records follow each later store of a claimed release, or
change to the albums kept for its album, and an import commits them.

The person also decides which clues a run asks about. Ben unchecks a barcode
that belongs to a box set rather than his disc; Carol picks the one catalog
number of thirty that names her pressing. Those choices are stored with the
candidate, and changing them starts the next run.

### 3.6 The draft

A *draft* is the metadata an import will write, as plain values: the album
fields, the pressing facts, the album's artists, the mediums the audio is on,
and one track per audio unit with its title, artists, place (medium, side and
number), and the length its source lists for it. It also holds what the source credits beyond the editable fields: each
track's credits in every role, the works it performs and the works those
nest among, and the credits the source states on the release as a whole. An *audio unit* is
what one track plays: a whole file, or one slice a bound CUE sheet carves out of
an image. Units add up: a disc image with two loose bonus tracks offers
thirteen. The draft's labels and barcodes are kept as the form holds them,
one row each as typed (a label's name and catalog number each as typed), a
row added and not filled yet included. A label row applied from a catalog
keeps that catalog's label ids while its name stays the one the catalog
gave; edited to another name, it carries none, and the import decides the
label by name (§2.3).

**Track order.** The units play in the order the files' own numbers give
them. A file's disc and track numbers come from its tags (`DISCNUMBER` and
`TRACKNUMBER`, ID3 `TPOS` and `TRCK`, MP4 `disk` and `trkn`); a number its tags
don't state comes from its name, read as `07 Title` (track 7) or `2-07 Title`
(disc 2, track 7). Files go by disc, then track; a file stating no number goes
after those that do, and files the numbers don't tell apart go in name order.
A release read from several folders keeps each folder's files together, the
folders in name order. Name order is natural order: "Disc 2" before "Disc 10",
case aside, then by bytes, so the import lists a folder's files in the order
the library lists the release's files in. Ben's download names its files by title alone, but tags
`Track Title.mp3` as track 7, so it is the seventh unit: a draft read from the
tags numbers it 7, and a catalog's tracklist lays its seventh track on it.

**Mediums and places.** Applying a source writes the draft's mediums and each
track's place, and nothing edits them afterwards: the pane and the library
editor show a track's position and edit only its title and artists.

- A MusicBrainz release gives each medium it reads its format; a record or
  cassette has as many sides as its positions name letters, read as a Discogs
  position's side letter (§3.4), counted from its lowest, and one naming none
  is one side. A track's number is its position's
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
differently leave the field empty, never the majority's or one file's value.
A folder whose units are CUE sheets reads its album from the sheets' `TITLE`,
`PERFORMER` and `REM DATE` the same way, and with no title stated anywhere the
folder's name is the title.

- The album title is the one every file states. Titles are compared without
  the disc marker each ends with and the separator before it, so a set whose
  discs tag `Album Title (cd 1)`, `Album Title [disc 2]`, `Album Title CD3` or
  `Album Title - Disc 4` agrees, and gives `Album Title`. A marker is `disc`,
  `disk`, `cd` or `d` and a number, in brackets or not; `Album Title Volume 1`
  names no disc and is a title of its own. Files whose titles still differ
  leave the title empty, and the draft shows its placeholder.
- The album artist is the `ALBUMARTIST` the files agree on; where no file
  names one, the `ARTIST` they agree on. Files that state either differently
  leave the album artist empty, so a compilation whose files neither flag it
  nor name an album artist has none and each track keeps its own artist. With
  no album artist the draft asks for one before it imports.
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
  pressing's are each the one the files agree on, else empty.
- A file whose tags don't parse states nothing: the album fields are what the
  folder's other files state, and its name places and titles its track, as
  for a file with no tags. The scan's seeding, Reset to file metadata and a
  file decision read it alike, and the stored reading keeps why its tags
  don't parse.

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
identification, or the person. Every state of a folder this device did not
import holds a draft row: the scan writes the blank draft and records nobody as
its writer, with the cover the folder came with, and the write that deletes this
device's release of the folder (§2.7) writes it again. bae stores each pane
control's value the moment the person uses it, under the candidate's hash, so
the person can leave a candidate half done and come back to it, even after a
restart. The pane keeps which part it shows and the text typed in its search
fields while it is open; a search is stored when the person runs it (§3.5).
The pane opens on Find online while identification runs for the candidate or
its answer waits on the person: nothing drafted the candidate from anywhere,
or the release its run picked failed a check against the folder. Otherwise
the pane opens on the draft. A folder this device imported holds no draft and
no pane state: a control or pane command that reaches it after the import
finished, such as one clicked on a pane still drawing the draft, writes nothing and is answered that this device already
imported those files, naming the release they became, as a second Import is
(§2.7). Since each edit is written as it is made, the pane has no reset and no
undo: to start over, the person applies a source again (a catalog release, or
the files' tags) or clears the metadata, and the file decisions stay as they
are.

The pane's draft card shows the folder's audio on one line under the album's
year: the codec, the sample rate, the bit depth (a lossy codec's bitrate in
its place) and the channels, in the order a track's audio format lists them
(§2.2), as in `FLAC · 44.1 kHz · 16-bit · Stereo`. The line gives no count of
files or tracks: the table under it lists the tracks.

The mapping table pairs each unit with its draft track. Each row shows the
length its audio measures; where the track's source lists a length more than
three seconds apart, that listed length follows it in the warning colour, so
the row says what disagrees. A CUE image's track names its image once, in its
sheet's heading, and its source cell gives where the track starts in the
image (10:22), in the found table and after import alike; the entry's title is
in the title column only. Where the draft has several discs or
record sides, each is headed once, however many runs of rows it holds: the
loose files on it, and under its name each CUE sheet's entries on it (a sheet
the release splits across sides names itself again on the next side). For a
CUE sheet, the person can bind each `FILE` line to an audio file and say which
disc each sheet is, or ignore a sheet. The table also counts the tracks nobody
has named yet, and the pane states that count beside Import without blocking
it: importing titles each of them after its file.

The files the folder carries that are no track, such as a rip log, its images
(which its gallery above the tracks also shows) or a track
sheet that carves no tracks, are listed under the tracks beneath a ruled *Files*
heading, one line per file: its name in monospace, its size at the right, a
hairline between lines, a damaged file marked with why. Every size bae shows, here
and elsewhere, is in decimal units with at most one decimal and none when whole
("3.9 GB", "420 MB", "1 GB"), the number and the unit in the person's language.
A document opens in the
viewer when clicked, over whatever else is open. A library release's editor
(§3.8), on an imported folder's pane and in the album's *Edit release* sheet,
lists the release's files, images included, the same way, under its tracks, and
an imported folder's pane puts the folder's images above its tracks; a
release's files show the same way in all three. On the Mac, a click on one of
the folder's images opens the lightbox over the folder's images at that image.
The lightbox shows the image the person last moved to while the folder holds
it, otherwise the folder's first image, and its arrows move to the images
before and after it, wrapping around the ends; a lone image has none.

Before importing, the person can play a file through a second player, which
pauses the library's playback and resumes it when it ends.

### 3.7 Importing

Importing measures loudness and adds the release to the library. Audio stays
where it is; bae never copies, moves or rewrites it.

bae limits how many of the person's files it holds open at once, across the
scan, imports, saves, exports, playback and previews, so it never runs out of
open files.

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
3. **Reads each file** whole where it sits, for a local import as for an
   import to storage, one file at a time: bae records its path, size and
   modification time and hashes its content and each chunk of it, and the
   import's progress counts
   the bytes read. The upload of an import to storage checks what it sends against
   those hashes, and so does a local release's later move to storage (§4.2),
   which reads nothing again. A file whose size or modification time is no
   longer what the scan recorded fails the import before any row is
   written.
4. **Measures loudness** by decoding every track: its EBU R128 integrated
   loudness and true peak, and the album's over all its tracks. bae measures
   several tracks at once. A track whose audio is in more files than bae may
   hold open at once fails the import, saying so. A near-silent
   track stores no loudness and plays unchanged. The same decode records
   *scrub points* for a FLAC whose seektable would leave a seek more than 10
   seconds of audio short of its target: none, one of only
   placeholder points, or one whose points lie more than 10 seconds apart,
   or start or stop more than 10 seconds from the file's start or end. For
   each segment of a track, the frame it starts in, then the first frame
   each 10 seconds of audio on, or sooner where the bytes from the last point would outgrow the window a
   scrub fetches (§7.6). A segment's last frame is never a point, since a
   decode that jumps there can't tell which sample it holds; the frame before
   it stands in. Each point is a
   frame's first sample and the byte it starts at, read from a decode that
   began on a frame. Every MP3 gets scrub points too: its frames state no
   position, and each depends on the frames before it. For the first frame
   each 10 seconds of audio past the file's first, its point is the latest
   earlier frame a decode can start at and still play it exactly, with the
   first sample such a decode plays exactly; the decode read the whole file
   from its first frame. Any other file gets none. A measurement that panics,
   or a read of a track's file that panics, fails the import, as a track that
   can't be read does, and is reported as a bug: nothing it measured is
   stored.
5. **Checks the audio decodes**, in the same pass. A track that fails, or
   decodes short, fails the import rather than failing later at play time.
6. **Finds the album.** The album the person picked on the pane, if any,
   else the album the release's records name: the record the draft was read
   from decides, then the other catalog's record — the first naming a
   library album by a copy of its pressing (the import is another copy, §2.7)
   or by its album (release group, master). Library albums are the
   curator's: two albums the records name stay two, and no question is
   asked. Where one record names several, its group's own album wins if the
   library holds it, else the oldest. With neither, the album is new and
   takes its id from its group (§2.6). The pane shows which: "New album", the
   album the records name, or the one picked, as a pop-up under the identity
   (an album's line is its title and its artists, or its title alone for an
   album that credits no one), whose items are that album,
   New album (a new album even where the records name one the library holds),
   Choose another album…, which searches the library's albums, and after a
   pick the release's own album again; a picked album deleted meanwhile (§2.1)
   is no longer picked, and the import files under the album the records name
   or a new one. Re-identifying a library
   release files it the same way. The "already in your library" banner
   names every album holding the release, or another release of its album:
   those holding the release first, then oldest first. The banner and the
   dimmed rows change as the library gains and loses releases. bae stores no
   library status with an identify verdict or a search result, in flight or
   stored.
7. **Writes everything in one transaction**, from the draft as it stands, read
   from no source again: the album; the release, with a fresh random id, and
   this device's record of the folder's hash it read (§2.7); its tracks, files, segments, scrub points, credits, roles, works, records,
   cover, and the new artists' pictures. Each track row is made once its audio is bound, with the
   length that audio measures, so no track is ever stored without one. A track
   left untitled takes its file's name. An existing album is checked in that
   same write: one deleted since the import was prepared, because a later
   write took its last release (§2.1), fails the import, which writes nothing
   and tells the person to pick another album or New album.
   The same write decides a new album's id: the album takes its group's id
   (§2.6), except that a New album the person picked keeps its own id when
   the library holds the group's album, even one that another import of the
   group wrote after this import was prepared. The same write decides which
   library artist each credit, role and composer names (§2.3), so two credits
   naming one artist write one, and which library label each label names
   (§2.3).
   The same transaction deletes what the folder's state holds that nothing
   reads once the release is in: the identify verdict with the releases it
   found, what the person let the lookups use, the pane's last failure, the
   artist and label decisions it just resolved its credits and labels by,
   the album picked for it, and the draft with its source, its link and its
   cover, since the Imported row and the imported folder's pane read the
   library release (§3.2). The clues stay, since the automation interface
   lists them for the folder with the file each was read off (no screen shows
   which file a clue came from), and so do the file decisions, which lay its
   files out. Every later command that would change the draft of a folder this
   device imported is refused with the words an import of its files is. An
   import that fails deletes none of it, so a retry reads what the person
   and the run settled.

Nothing durable is written before the last step, so a cancel before it leaves
the library as it was. Once the write begins, a cancel is refused and the
import completes. An import whose work panics fails as one that errs: its
failure is recorded on its candidate, it is reported as a bug, and the next
import runs.

**The cover.** The cover comes from the folder's own image files, an image
embedded in a tag, or a catalog's image (the Cover Art Archive for
MusicBrainz, or Discogs).

A draft read from a folder's tracks starts with the first of these that
exists: an image in the folder whose name begins with the word cover, folder
or front (a leading number or punctuation is ignored, and a name that also
says back, rear, disc, cd, booklet, inlay or inside does not count); the
picture embedded in the tags of its tracks, from the first track that has
one; any other image in the folder. A track's embedded picture is the one
marked front cover, else its first picture of any kind, among those in a
format bae can store (JPEG, PNG, GIF or WebP). Embedded art is the fallback,
so having a picture comes before which picture it is.

While the draft is the one read from the pressing
the candidate is linked to, the picker offers that pressing's covers: every
claimed release's own images, then their albums'. When bae later stores more
of a claimed release, or of its album, the picker offers it. A MusicBrainz
release group a kept statement joins to a claimed
release offers its front image after that release's own album images. The first is the
draft's cover when the release is applied, until the person picks another. The
apps name each image in the viewer's language, from its parts: the service
that serves it (the Cover Art Archive, Discogs), the album's front image as the
album's, a gallery entry's number and what it shows (front, back, booklet and
the archive's other types) and the archive's comment, joined by the language's
separator. A release's gallery names its cover Cover and an image file by its
filename. On Android the album page's cover opens the gallery only when the
release has gallery items, and the gallery shows its page counter, with the
number of items as its total, only when it holds more than one. Saving the cover picked, in the import's picker or a library
release's, can download it from its catalog or read it from the release's
storage, so the picker says it is saving the cover while it does. On the Mac, an album's cover, and the release editor's, opens
that picker as the Change cover sheet, on its grid, and the release editor's sheet opens over the editor; a tile there shows its image and its name, since its section's heading says where it comes from, and opens the lightbox. Until the
picker fetches the catalogs' covers (below), a draft read from the tags, typed
in or cleared, or a candidate linked to an album or to nothing, has no
catalog's covers to offer. The fetch finds the covers of the candidate's link:
its claimed releases' covers for a pressing, the covers each catalog shows for
the album for an album, and none for a candidate linked to nothing. A catalog's cover is fetched with the draft, before the
import runs, so the import never asks a catalog for it. A cover is a catalog's image, or
one of the folder's images in a format bae can store, or the artwork its
tags hold, as the scan read them. When the person saves a draft naming a
file the scan did not store as an image, an image in another format, or
artwork the tags do not hold, bae refuses the draft, and the cover stays as
it was. Dropping one of the folder's images on the pane's cover makes it the
cover, or bae refuses it by the same rule. The pane shows a plain tile in
place of a missing cover, with no words and no invitation to add an image.
The artists' pictures
are not part of the draft: nothing before the import shows them, and many
drafts are never imported, so a pick or an edit fetches none and the import
fetches them (step 2). Every stored image is downscaled to at most 600 pixels
on a side and stored as JPEG.

A library release's picker and a candidate's each show, under the source the
person chose (all, the release's files, or one catalog): a library release's
saved cover, shown under all; the covers its catalogs offered when this device
last fetched them; and the images a cover can be made from, the release's
files or the folder's images in a format bae can store. Each section shows its
count, and the picker its total. The picker's preview and its lightbox focus
the cover the person last moved to, while the source shows it; otherwise the
release's saved cover or the candidate's chosen cover, while the source shows
that; otherwise its first cover. The preview shows where the focused cover
sits among them all as "N of M", and the arrows of the preview and the
lightbox move to the covers before and after it, wrapping around the ends; a
picker that shows no cover focuses none. Opening the picker, and its Refresh,
fetch the catalogs' covers and store them, replacing what the last fetch
stored on this device (§A.8). The open picker shows that fetch, a change to
the release's files or saved cover, or the candidate's folder, draft or link,
from this or another device, and the person's choice of another source or
another cover as each happens. A scan of other folders, and the drafts and
links bae stores for them, write none of the rows a candidate's picker reads,
so coven reruns the picker's read for none of them. bae stores a candidate's
fetch with the link it fetched for, and its picker shows the fetch only while the candidate
keeps that link; otherwise it offers what the pressing that bae read the
draft from offers (above). A fetch that fails stores nothing, so the covers stored
stay on screen, and for a release or candidate that names no catalog release,
the fetch stores that it has no catalog covers.

**Where it goes.** Each import lands on this device or goes to storage, as
the person last chose (§4.2): This Mac or Cloud, one choice. Either import
records where its files go in the same transaction as its rows, and an import
to storage queues their upload in that transaction, so the choice is never
lost and needs no connection to record it.

**Automatic identification and import.** By default, each release a scan finds
for the first time is identified on its own, in the background, behind
anything a person asks for. With "Import automatically when identified" on,
any run that picks unattended imports straight away: one bae started on its
own, or one the person started with Identify, a retry, or a changed clue. A
pressing the person picks themselves, from a run's list or their own search,
waits for their Import. Both are settings (Appendix C), and each works without
the other: with automatic identification off, the runs a person starts still
import. Turning import on imports nothing already identified. An automatic
import that cannot start has nobody to tell, so the refusal is recorded as
the release's failed import, where its row and pane show it (the setting that
decides it failing to read included), and one that is
a bug (a store that fails, a row that is damaged) is also reported as a bug.
A failed import, automatic or asked for, whose candidate changed meanwhile has
no failure to record: that is logged, not reported as a bug.
It records nothing when another import already holds the release, when this
device already imported its files, or when the library is closing.

Import asked for while bae drafts a run's unattended pick, from the row, the
pane or a selection, leaves the draft going and doesn't import the draft as it
stands: it waits for the pick to be stored and then imports, as automatic
import does, with the setting on or off. The row then no longer offers Import.
A draft that stores no pick, because it failed or was cancelled, imports
nothing.

A run whose step or lookup panics ends as bae's own error (§3.2),
and is reported as a bug; the queue's place it held goes to the next run. A
reading of the folder's signals that panics fails every signal, so its run
ends the same way. Writing a run's answer that panics fails as an answer that
could not be stored (§3.2), reported as a bug. A queue that panics is
reported and stops for the session, as it does when the library closes.

**Edge cases.**

- **Two ids, two artists.** Carol imports a release crediting *Artist Name*
  with a Discogs id and a MusicBrainz id. The library already holds two
  artists of that name: A, from a Discogs import, holds the Discogs id; B,
  from a MusicBrainz import, holds the MusicBrainz id. The import credits B
  and drops the credit's Discogs id (§2.3); A keeps its own. Nobody is asked,
  and nothing on the import page offers to merge them. A label carrying two
  ids that two library labels hold imports the same way, as the MusicBrainz
  id's label.
- **Identifying a library release again.** A release already in the library
  can be identified again from its own files; its Disc IDs come from the rip
  logs and CUE sheets this device holds of it, each sheet bound to its audio
  and given its disc by the rules a folder's scan applies, so it hashes the
  discs its folder did. Importing stores with the release what the person
  decided in the import pane about a sheet (`release_sheet_bindings` and
  `release_sheet_discs`, A.5): the audio they bound a `FILE` line to, or that
  they cleared it, and the disc they said the sheet is, or that bae should
  ignore it. Identifying the release again binds a sheet they bound to the file
  they chose, leaves one they cleared unbound, and gives a sheet they numbered
  the disc they gave it or skips one they ignored; it reads a sheet they decided
  nothing about by the scan's rules. Its track lengths come from its stored
  tracks. The person searches from the release editor (§3.8) and picks its
  pressing, which the editor applies to its form at once, with no confirm
  step, and the editor shows what it gave. A pressing whose tracks don't
  lay out on the stored ones is refused with both counts, and one several of
  whose disc sets fit them alike is refused naming those discs. The editor on
  every app states that refusal in its own line (G.1), as the import does, never
  as a generic import failure, and MCP's re-identify returns the refusal's own
  text.
- **One import at a time.** bae runs imports one after another.

### 3.8 Editing a library release

One editor edits a library release: the album detail's release menu opens it
with one *Edit…* item, and the import pane of an imported folder shows the
same editor for each release the folder became. It has the parts of the
import pane's draft, laid out the same way: the cover beside the album and
pressing fields, the album the release files under, the track table (its
rows play, and show lengths and a CUE image's starts as the import pane's do,
§3.6), and the same *Identify* controls — *Automatic* identifies the release from its files
and *Search* opens the typed search — whose results replace the draft in
place until a pick applies one, or *Back* returns to it. *Save* writes the
form; *Cancel*, shown once something changed, discards its changes. While an apply
reads a release from its catalog, the editor names the catalog it reads from,
as an import's pick does (§3.5); saving, cancelling and picking an album are
local and show no spinner, their controls disabled until they end.

The editor opens with a *form* that holds the release's values as the
library holds them, and the editor holds its form: bae stores none of the
form's changes until a save. Each change the person makes (a field, a
label or barcode row, a choice of what the pressing is, the album's artists,
a track's title or artists) changes the form as it is made, in the order
made, and the editor shows, for the form as it stands, how each credit in it
stands to the library, whether anything changed, and why the form can't be
saved, when it can't. The draft follows
the library while the editor is open: how each credit stands to the
library, the album's title and credits, and the cover show what the library
holds after any write that changes them, including a write that merges an
artist or renames the album, with no edit. Applying a catalog
release to the form reads it once, against the release's stored track
lengths and its files, and replaces every value the release holds — the
album fields, the pressing, each track's title, artists, medium, side and
number, the roles and works the catalog credits, the release's records (the
applied one first) and the album those name — none of which is written yet;
the cover stays the release's own. Saving writes the whole form in one
transaction: the edited fields, and where a catalog was applied, its every
value too, so nothing a previous source credited survives. The release lands
in the album its records name (§3.7), or in a library album the person
picked, which keeps its own title and artists. The editor names that album
with the import pane's album picker: the album the release is in, the one an
applied catalog's records name, or a new album where they name none the
library holds, until the person picks one (a library album, or a new album,
which a save writes with the release's edited title and year and the credits
of the album it left); a pick holds over a catalog applied after it. A
library album the person picked stops being the pick once someone deletes
it. The cover is not part of the form: the editor writes a cover the person
picks at once, and the form keeps what the person applied and picked.
Cancelling discards the form's changes; bae reads no source again, and there is no reset to
a source: the library holds no pointer back into one. A release with nothing
applied saves its edited fields alone, its mediums, roles and works as stored.
A save decides each label as an import does (§2.3), and a label the person
left as it was keeps the library label it named.

Every editor holds a form of its own, so the editor sheet and the import
pane open on the same release, or MCP's re-identify while either is open,
never share an edit, an apply or a pick. A save's one write first checks the
release against what its form opened on: if the library no longer holds it
so (another editor saved it, MCP wrote its fields, or someone renamed an
artist it credits), bae refuses the save and writes nothing, and the editor
says the release changed after it opened; *Cancel* opens it again as it is
now. Only the cover, which the editor writes at once, never makes a form out
of date. The
check and the rows are one write, so nothing lands between them. After a
save, the editor works in a fresh form opened on what it saved. When the person, another device or MCP
deletes a release while its editor is open, the editor closes: in the album
detail the release's sheet closes, and in the import pane the folder
returns to Found with its draft (A.5, `import_candidate_edit`).

The editor lays the tracks out as the import's mapping table does: a heading
for each disc or record side where the release has several, and two or more
consecutive tracks a CUE sheet carves from one file under that file's name,
once; every other track names in its row the file its body, from its
INDEX 01 on, plays from. A pregap stored at the tail of the file before
(§2.2) is not named: a row names one file. An applied catalog lays them out
again at the places it gives them.

Under the tracks, the editor lists the release's files that no track plays
from (§3.6): its track sheets, documents, images and anything else its folder
held, in the release's file order, each as the role its import's scan gave it
(§2.2) says. A track sheet reads as the text it is, an image the scan found
damaged says so, and a document or track sheet opens in the viewer, read from
wherever this device can get the file and decoded as a folder's document is
(§3.3).

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
  which keeps them. It makes one from an image up to 8,192 pixels on its
  longer side, a 48-megapixel camera photo included; a 16-bit PNG, which
  takes twice the memory to decode, up to about 5,800 pixels square.

A cover belongs to one release, an artist image to one artist, an avatar to one
person. A row references its image by the id of a coven file, the record coven
keeps of the bytes
([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)):
the `file` column of `covers` and `artist_images`, `avatar_id` of `people`. A
coven file never changes its bytes, so changing a cover stages a new file and
points the release's cover row at it in one write; the old file goes with its
last reference, and coven deletes its stored bytes once nothing refers to them
([coven §17.10](https://github.com/bae-fm/coven/blob/main/spec/coven.md#1710-deleting-stored-files)).
When two artists merge (§2.3) and only the absorbed one has an image, the
survivor's row references the same file: nothing is uploaded again.

The three image references are bound
([coven §17.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#174-bound-and-trailing-references)):
the write that points a row at a new image goes out only once coven has
stored the image's file, and another device receives the row and the stored
file in one transaction. So no device but the one that made an image sees a
row naming an image it can't read, unless a schema change that needs every
device updated is made between the write and the end of its upload. The write
then goes out together with the change, and the stored file after them: a
device still on the old app shows neither until it updates, then has the row
while the file is still on the device that made it, and until the file
arrives it can't read the image and shows a plain tile where it would be.
The device that made the image shows it at once, read from
coven's own copy. Until the file is stored, that device's later writes wait
behind it too, a rating made right after included, which is why a bound file
must be small: coven refuses a write naming a file over 1.5 MiB. Every image
bae stores is a JPEG at most 600 pixels across, which stays under that limit.

When an image changes, every device and every Subsonic client shows the new
one, and an image a device couldn't read shows once its file is stored.

A cover that is not on this device shows once the library's storage connects,
from the first screen on, instead of failing as it does in a library with no
storage. A library opened offline still opens.

### 4.2 Local and uploaded releases

A release is *local*, or *uploaded* so other devices can play it.

Ana imports her CD rip to her laptop only. The release is local: its files are
her own, at their path on her laptop. Its rows sync like any release's, so
Ben's phone shows the album, its tracks and its cover, but can't play it. Ana
moves it to storage. coven uploads each file from where it sits; once every
file is in storage every device can play it. Ana's files stay where they are,
untouched.

**Every file is coven's.** A `release_files` row references its file by the
id of a coven file
([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)),
from the write that imports it on. A local import stages each file on the
importing device only: coven records the original's path, size and
modification time, and the hashes of the bytes the import read
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)).
Before that write, bae compares the size and modification time coven found
with the ones its scan recorded, and refuses the import if a file changed in
between: the release's rows describe the file the scan read. The paths of the
originals never leave the device. Every device reads every release file through
coven:

- the file is stored: from the cache or storage;
- the file is on this device only: from its original, which coven checks
  still has its recorded size and modification time;
- the file is on another device only: it isn't here. Another device has it,
  keeping it local or still moving it to storage.

A release with any audio file on another device only cannot play here, and
names that device in the album and release views: "On Ana's Mac". bae names the
device that alone holds the audio file with the lowest file id among the
release's audio files that another device alone holds. The album and release
views stop showing the line once coven records the file in storage, and show
the device's new state when a member removes the device. A device removed
from the library, or whose person was, reads "On Ana's Mac, a removed device",
and one replaced when it was reset reads "On Ana's Mac, a replaced device":
its files stay on it, and no device can move them to storage any more. Such
a release still offers moving it to storage here, and coven refuses the move,
the app showing coven's reason that another device holds the file; it offers
no pinning, since not every file of it is in storage (§4.3). Playing it fails
with the same line, naming the device that holds the file with the name and
state the device has at that moment.

Whether this device can play a release comes from these records alone; bae
doesn't ask the disk whether
an original is still there. An original gone or changed since import fails
what reads it, when it reads it: playing a track says its original files are
missing or changed, moving the release to storage fails as it reads the
file (one already moving waits or fails as below), and an export fails.

A release is *uploaded* once every one of its files is in storage.
In the apps' words the library's place in storage is *the cloud*: a release
there reads In cloud (Cloud in the Storage manager's Storage column), the
move is Move to cloud, and no screen says "storage"
alone for it. Where sync is set up, the place itself, the folder or S3 storage on
the household's provider that coven's store lives in, is the person's *cloud
home*: the Sync settings section and the setup steps. The Storage manager and a
release's Storage sheet keep "Storage", since they cover the files on this
device too.
Until then it is local, on every device. The rows sync anyway, so every
device knows what the household owns, and a rating or playlist entry pointing
at a local release's track points at a row every device can read
([coven §15.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#153-references-stay-readable)).
Where a file is changes in coven's own record of it, with no change to a row
of bae's (Appendix B).

A release's cover is bound (§4.1), so it reaches other devices with the
release, local or not: the write that imports the release, or sets its cover,
waits until coven has stored the cover, and other devices receive the
release's rows and its stored cover together. coven uploads covers apart from
the release files, never behind them, and pausing uploads doesn't hold them.
That upload is bae's own file, not the release's: until its audio moves, the
release reads as local everywhere, and nothing about it reads as uploading.
Waiting files draw no progress bar; a bar appears only while a file's bytes
are moving.

**Moving to storage.** Moving a selection of releases to storage records
one move of all the files of each release, each release in its own write,
reading none of the files
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)).
The app names each release that can't move, and the rest move; when coven
refuses a release's move, the app shows coven's reason. No row of bae's changes, so
no release shows a transition on the way. coven uploads each file, checking every chunk it
sends against the hashes the import's read recorded, and once the last is
stored one write of coven's switches them all to storage. So other devices
see the release local until its last file lands, and never half uploaded.
Until then coven reads each file from its original, so this device keeps
playing the release while it uploads, and the upload list shows it
uploading. An original that changed since the import fails the move, even
one that kept its size and modification time: the rows describe the file as
imported, and a changed file would give every device audio that doesn't
match them. coven refuses to move a release when one of its files is on
another device only, is in storage already, or is in another move. A move
holds its files until it finishes, and a failed move holds them until the
person cancels it. A release moved to this device again
(§4.4) moves like one imported here. A move never keeps a copy on this
device: once it finishes the files are in storage and unpinned, and the
release is pinned afterwards like any other (§4.3).

If an original goes missing before its upload, the move waits for it, the
release's row in Storage Manager naming the file. If one changed, its upload stops before a
changed byte is sent and the move fails: none of its files switches, and the
release's row names the file and offers Cancel upload.

**Cancelling an upload** while any of its files is still queued, or after its
move failed, cancels the release's move in one write. Nothing was switched, so
nothing is taken back and no row changes: every file stays on this device
only, and the release plays from its originals again. What already reached storage stays until nothing refers to
it; coven then deletes it
([coven §17.10](https://github.com/bae-fm/coven/blob/main/spec/coven.md#1710-deleting-stored-files)).
Other devices go on showing the release without playing it.

- **An import to storage** (§3.7) stages its files for storage in the same write as
  its rows, each read whole once and compared with the scan as a local
  import's are. Its
  files are one move, as a release moved to storage is: other devices see
  them on the importing device until all are stored, and cancelling its upload
  leaves it a local release on that device.
- **Deleting a release** deletes its files' coven records with its rows, and
  with them coven's records of their originals; the originals stay where they
  are. Deleting an album does this for each of its releases.
- **Uploads queue.** Files go up three at a time by default, a setting from
  one to eight. The queue can be paused, and stays paused across restarts; a
  paused upload keeps its place. The pause holds release files only: a cover,
  artist image or avatar is part of sending a write, and goes on uploading,
  up to the same number at a time, counted apart from the release files.
- **Moving files only on unmetered networks.** While the person has it on,
  coven holds uploads and downloads of release files on a metered network
  ([coven §17.9](https://github.com/bae-fm/coven/blob/main/spec/coven.md#179-uploads)),
  and they go on at the next unmetered one. It is off by default, lasts across
  restarts, and doesn't hold playing a track that isn't pinned, which still
  streams.
- **Transfers on the Mac.** Settings lists Transfers after Playback in its
  sidebar, and its pane holds Pause uploads and
  Pause downloads, then Uploads at once and Downloads at once, pop-ups of how
  many files each moves at a time. Downloads are pins and moves-to-this-device, which share one
  queue so they contend for the same downloads; the pause and the limit cover
  both, and nothing else counts or names one as the other. Either
  pause lasts across restarts, and while either holds, the sidebar says Paused
  beside Transfers, in a chip, with ↑ for uploads, ↓ for downloads or ⇅ for both. A paused
  file keeps its place; changes to the library still sync. Neither limits
  speed.
- **Transfers on Android.** Settings lists Appearance, Library, Playback,
  Transfers, Storage (§4.7), Casting and About as rows on one card, each opening a page, in the
  order of the Mac's sidebar. Transfers opens a page with Pin over
  Wi-Fi only, the setting above, and Pins at once, how many files are pinned at a time. A phone
  never uploads release files, so Android has no pause.

**The Storage manager** lists every release with its album's title and
credits, its media, its files' count and total size, whether it is uploaded,
whether this device keeps all its files pinned (§4.3), and when it was
added. It shows every
release, or only the local ones, or only the uploaded ones; text typed in its
filter field narrows that to the releases whose album library search finds
(§2.3), as each character is typed, and blank text narrows nothing. It shows
the shown list's count and total size, and says there are no matches when the
text narrows it to none. It sorts by album title (ignoring ASCII
case), artist (§2.3), media (a release stating none last), file count, size,
storage — a failed download first, then uploading, then pinning: files
coming to this device, waiting or downloading in the download queue (§4.3),
then local, pinned, and in cloud only — or when the release was added, either way, ties going by album
title, then when the release was added, then id. On macOS the list is a table
with a column for each of these; a column can be dragged wider or narrower or
to another place, sized to its header and the rows in view by double-clicking
the divider after its header, and hidden or shown again from the menu the
header row opens, except the release column, which always shows. Which
columns show, their order and their widths are kept across launches. A
release reads as pinned, here and in the album and release details, once this
device keeps every one of its files whole, and the views change as each pin
finishes.

At rest, a release's album storage band says where its files are: on one
device, or in storage, pinned here or not. Only a
file in storage is pinned, so no release reads as both on one device and
pinned.

A release with files waiting to upload shows Uploading, filled as far as its
bytes have come, in its row and in its album's storage band, or before then
Queued in its row and Queued to upload in its band (its Imported row in the import
list shows the phase that makes, §3.2); the
Storage manager's inspector shows how many wait, are being sent and are
stored, and how many bytes of its whole size are stored or sent. While coven
sends none of the release's files, the row shows Queued. When a file's
original is missing, the row shows Waiting
instead, and the inspector says *Waiting for a file*, names the file and offers
*Restore source file…*, since only restoring it lets the move go on. Its sheet
says to put the file back where it was imported from, and its *Retry* asks coven
to retry the upload now; the
release's storage band shows Queued to upload and, under it, a warning that
names the file. A pause, the network, a metered network or a retry's time holds
the whole queue, so it shows once, where the queue shows, never on a release: in
place of the queue's rate under the list, on the Activity panel's Uploading
line, and on the phones' Pinning screen. A release's bar fills to its stored
files' sizes plus, of each file being sent, the share of its encrypted object
sent so far, whether the file goes in parts or in one request; the Storage
manager's rings show each file's share. bae shows how fast each release's
files go, and every file being sent together, a file sent in one request
included; no screen shows one file's rate. A file that only waits has no
rate, and a pause ends every measurement. While its files wait, or a download
holds it, the release offers no storage action but cancelling, which would
race it otherwise. A release offers each action its place allows (upload, pin,
unpin, move to this device), and when coven refuses one, the app shows coven's
reason. Upload alone also waits for storage to be connected, since coven
queues a move with no storage rather than refusing it. The Storage Manager's
selection offers an action when every selected release does, and only
cancelling when a selected release is moving or its move failed. The Mac's
menu offers nothing until it has the actions of the selection it shows. When
the selection holds exactly one release the list shows, the inspector shows it
and the Unpin alert names its album's title. An action on the selection acts on
each release the person picked, refusing one it can't act on (a release that
someone deleted meanwhile, or one in the wrong state for the action), and the
person sees each refusal. A release in a download is the download queue's
entry for it (§4.3, §4.4), a pin or a move-to-this-device, ended in the write
that changes its files, so the entry and coven's change commit together and the
release never shows a download its files have finished.

Each row's storage state is the place it sorts by, and the Storage column shows
it as one word with no icon: Error, first,
for a pin, a move to the cloud or a move to This Mac that failed; Waiting, for
an upload held by a missing original; Uploading, filled as far as its bytes
have come while any file is being sent; Pinning, filled as far as its download
has come; Moving, filled as far as a move to This Mac's copies have downloaded
(§4.4); Queued, for an upload, a pin or a move to This Mac that waits its turn;
Local, Pinned or Cloud at rest. The column draws a chip only for Uploading,
Pinning and Moving, the states that fill, and draws every other state as a
plain word, Error in the danger colour and Waiting in the warning colour.
The inspector says the rest for the selected
release: how many files are queued, moving or done, with Cancel; for a failed
pin, *Could not pin*, and for a failed move to This Mac, *Could not move to
This Mac*, each with *Details* (§9.6) and *Retry*; for a failed move to the
cloud, *Could not move to the cloud*, naming the file whose original changed
since import, with *Details* and *Cancel upload*; and it marks, in the release's
file list, the file whose original is missing or changed, or whose download
failed. No rate shows beside a row, nor how many entries wait ahead of it or
what holds the queue back: a row's storage is its state and at most the one
change on its way. A move fails when a file's original changed before its upload
(coven keeps the move failed, naming that original, until the person cancels
the move); the release stays on this
device only, its row sorts first beside a failed download, and its Cancel ends
the move.

Under the list, beside its count and total size, each transfer queue
holding files waiting or on the move shows while it does: uploads, pins
(§4.3), and exports and saves (§4.6), each with how fast it goes, or in its
place what holds the whole queue back (Paused, Waiting for network, Waiting for
Wi-Fi, Retrying at the earliest retry's time, the first that holds), an export
with how far its release has been written, and a control that pauses or
resumes the whole queue. The upload queue shows Paused while the person paused
it and also while sync isn't running, and its control offers Resume only while
the person paused it. The upload queue shows Retrying at the earliest time
coven gives a file's retry or a provider's cooldown to end. A queue holding
only failed entries shows nothing.

### 4.3 Pinning and the cache

An uploaded release can be pinned to stay on a device; otherwise its files are
cached as they play.

Pin shows only for a release whose files are all in storage. coven refuses
to pin a file on one device only, on every device, the one it is on included
([coven §17.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#178-cache-and-pinning)),
so a local release offers no pin anywhere, and one moving to storage offers
it once its last file is stored.

Ben's phone holds little. When he plays Ana's album, the chunks he plays are
fetched and kept in the cache. Before a flight he pins it: the phone downloads
every file whole and keeps it regardless of the cache's size.

Each kind of file has its own cache budget on each device, so audio never
pushes the covers out:

| Kind | Budget |
|---|---|
| Audio and the folder's other files | 20 GB, as Keep up to sets it (§4.7) |
| Covers | 512 MiB |
| Artist images | 256 MiB |
| Avatars | 64 MiB |

Pinned files don't count toward a budget: a device that pins 20 GiB of audio
still caches the next album it plays from the cloud, up to the budget, and no
pinned file goes.

A file downloads when it is first read: a cover when it is first shown, a
track as it plays. Over budget, the least recently used go first. Covers are
never pinned; an evicted one is fetched again the next time it is shown.
Shown large, in the lightbox on the Mac or the gallery on Android, an image
the app is still loading is named under its spinner.

The download queue holds pins and moves-to-this-device (§4.4) and downloads one release
at a time. Pins download from a queue that can be paused,
cancelled and retried. Pausing stops the download that is running at once
rather than after its file, and its release stays queued in its place; resuming
continues from the bytes the stopped download kept, which coven holds apart
from the cache, and nothing of it was pinned until every file is whole.
Cancelling a pin removes what it downloaded, in the write that takes its entry
out of the queue: the release's files leave this device, partial downloads
included. The queue takes no entry while the device has no
usable network, and goes on when it returns; a
platform that cannot tell holds nothing back. What holds the queue back,
Paused or Waiting for network, shows once where the queue shows: in place of
its rate under the Storage manager's list, on the Activity panel's Pinning
and Moving to This Mac lines and on the phones' Pinning screen, never on a release. A pin that fails stays in the queue with its error,
nothing of it pinned, and its row sorts first, reading Error, with Could not pin in the inspector; one that breaks on a bug fails the
same way, reported as one, and the queue goes on to the next. The queue's
Retry queues every failed pin again, each in its place; a release's own Retry
queues only that release's failed pin. The phones' Pinning screen offers Pause
while the queue holds an entry, and Retry while an entry has failed. Each entry keeps its release's file count and total size from the write that queued it, so its row (on a Mac's transfer queues and Android's Pinning screen) still says "4 files · 120 MB" under *Release no longer in library* once the library has dropped the release and its files. While a release's pin waits, its
storage band offers Cancel pinning in Pin's place, which takes the entry out of
the queue and removes what it downloaded. A failed download shows on its
release in Storage Manager, whose *Retry* queues the release's pin again; nothing retries it on its own. A read that fails, as a
track plays, leaves no record: its failure goes to whatever read it. A release
becomes pinned only once every file lands. The
queue is kept on the device across restarts, entries and pause: a release that
wasn't fully pinned stays as it was, and the entry that was downloading
downloads again from its start. coven downloads each file whole first and pins
nothing until then; one write pins every file of the release once the last is whole and takes it out of the queue
([coven §17.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#178-cache-and-pinning)),
so a failed or cancelled pin leaves no file of it pinned. The release
downloading shows as its bytes the sizes of its files kept whole, of its files
whose download for this pin finished, and, of each file in flight, the share
of the file's size its encrypted object has moved, so the bytes move as a file
arrives and never go back as a file lands; a download that failed, was held or
was dropped counts none of its file. Its rate is what it has moved since it
began over the time since. Unpinning removes the release's files from this device in the same write, with room in the cache or not: the pinned whole copy, any cached chunks and the partial download a pin left go once the write has committed, nothing else is evicted, and playing the release again reads from storage. It also ends any request coven still holds to
keep them here ([coven §17.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#178-cache-and-pinning)); unpinning a selection is one write for
all its releases, and one that is gone or holds no files is named while the
rest unpin. coven has nothing to remove for a file only this device holds, so
unpinning a local release leaves its files here and refuses nothing.

Unpin asks first while the device's `ask_before_unpinning` setting is on, in
both apps and everywhere Unpin is offered: the Mac's Storage sheet for a
release, the Storage Manager's row menu (one alert for the whole selection,
naming the release when it is one and counting them when several), and the
Android album page's download control. The alert says the files leave this
device now and the release stays in the cloud and plays from there while online.
Unpin in the alert unpins; with "Do not show this again" ticked it first turns
the setting off, and if that write fails nothing unpins and the error reaches
the person. Cancel changes nothing, ticked or not. With the setting off, Unpin
unpins at once. The MCP tool's unpin never asks.

Asking to pin several releases queues, in the order asked and in one write,
each that can be: one already waiting or downloading, or pinned, needs
nothing, and the write queues again, in its place, one whose pin failed, as
retrying queues it. bae refuses a release that
someone deleted meanwhile and queues the rest, and the person sees the refusal.
A release whose files are not in the cloud queues too: coven refuses its pin
when the pin runs, and the entry fails with coven's reason. If the write
fails, it queues nothing; the person sees the error and bae reports the failure
as a bug.

### 4.4 Making a release local again

Making an uploaded release local again brings its files into a folder the
person picks, and leaves them on this device only. Only the desktop apps offer it.
In the apps' words the move names the device as the import's destination
does (This Mac or Cloud): on the Mac it is Move to This Mac.

Carol wants her box set out of storage. She picks a folder on her drive. Each
release she selects goes in a folder of its own below it, named for the
release's source folder, so two releases that both hold `01.flac` do not
collide; a release with no source folder name fails its entry. One request
queues the whole selection in the download queue (§4.3), in one write: a
release already waiting or downloading needs nothing, a failed pin's entry
becomes this move-to-this-device in its place, and the write refuses a release
someone deleted meanwhile, while the rest queue and the person sees the
refusal. A local release queues too: coven refuses its move when the move runs,
and the entry fails with coven's reason. The
queue makes them local one release at a time, in the order asked, and its
pause, cancel, retry and restarts are the pins': pausing stops the download
that is running at once and its entry stays queued; resuming starts it again
from its start; the queue is kept across restarts, its entries and pause, and
the entry that was running starts again. A queued move-to-this-device reads Queued to
move to This Mac, and Queued in the Storage manager's row; one that fails
stays with its error, reading Could not move to This Mac, and Error in the
row, until retried or cancelled. A cancel takes the entry out
and stops the download. coven downloads every file to its path below the
folder
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)),
checking its content hash as every download does,
writes it under a temporary name in its final folder, and renames it once it
is whole and synced. While it downloads, the release's Storage manager row and
its storage band fill Moving to This Mac… as far as its copies have downloaded, and
the inspector shows the bytes of the whole release and of the file being
downloaded, over a bar that fills to the share of the release's bytes
downloaded, a few times a second as the bytes arrive. Then one
write switches every file to Carol's device only, coven recording each copy as
the file's original there, and takes the release out of the download queue
(§4.3); no synced row of the release changes. Other devices see the files on Carol's device,
drop their pins and cached bytes of them, and can't play the release. The
stored bytes go once nothing names them. A cancel or failure before that write
removes every copy and every folder the call made, a cancel's at the next
write, and leaves the release uploaded. The cover keeps its coven file: it is bae's own, and every device
shows it. If two devices make one release local at once, the later write wins
on both; coven deletes the losing device's copies it left as it wrote them,
with the folders it made for them once empty, and a copy the person changed
meanwhile stays as their own file.

The file names come from the release's rows, which another device may have
written. A name in the release is a label: it may hold any character, since
each device took it from whatever its own drive holds. A name that held bytes
that aren't UTF-8 is stored as its escaped text (§3.1), whose `\x` holds a
backslash, which separates folders on Windows, and Linux takes `:` and `?`,
which Windows and FAT drives refuse. So each copy's path is its names, split at
`/`, and coven writes each name through the rule it uses for its own local
copies, the same on every device
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)):
`\ : ? * " < > |` and `/` within a name become their full-width lookalikes,
control characters their pictures, a last `.` or space its lookalike, a
Windows device name its letters in full width, and a name too long for a drive
is cut before its extension. Every name is then one name inside Carol's folder
on every device, so no row can make a device write outside the folder chosen:
`Caf\xE9.flac` is written as `Caf＼xE9.flac`, and `..` as `.．`. The tables
refuse a name that names no file, an empty one or one holding an empty name
(`a//b`, a leading or trailing `/`), by a CHECK on each column.
Before anything downloads, coven refuses the whole move when a copy's path
already holds something, passes through a symbolic link below the folder, or
is one path with another of the move's once written, or differs from one only
in case or Unicode form (`Why?.flac` and `Why？.flac`), so no copy ever
replaces another or a file already there. The folder's own path must be valid
Unicode.

### 4.5 How files sit in storage

Storage holds encrypted objects under random names. Ana's FLAC files are not a
folder anyone can browse in Dropbox: each is one object, encrypted with a key
of its own that travels only inside the encrypted writes of its coven file's
record, at a path coven fixes when the file is staged
([coven §17.5](https://github.com/bae-fm/coven/blob/main/spec/coven.md#175-how-a-file-is-stored)).
The provider sees no album, artist or file name.

The path is fixed when the file is staged and kept in its coven file's record,
so renaming an album never moves a file. A new cover is a new object with a new id. Two
devices replacing one cover then never write the same object, and a device
applying an older write still finds the bytes that write names.

### 4.6 Export and save

Two ways put a release back on disk, from desktop:

- **Export** copies a release's files byte for byte into a folder named after
  its source folder. The copy is staged and moved into place only once every
  file is written. The folder and each file take their names in the release,
  written as a copy-out writes them (§4.4): `Caf\xE9.flac` exports as
  `Caf＼xE9.flac` on every platform. Each file is created new, so two names
  written as one (`Track?.flac` and `Track？.flac`, or two that differ only in
  case on a drive that holds them as one) fail the export, which leaves
  nothing behind, rather than one file replacing the other.
- **Save** converts: one track, or a whole release, for a car or a phone
  (FLAC to MP3, say); an exact copy is an export. It writes new files in a
  *format* the person keeps in Settings → Save as: a name, a codec, and the
  codec's bit depth or bitrate. A lossless format writes the source's depth
  (16 bits for a lossy source, which has none of its own), or 16 bits for
  players that can't take more; a lossy one writes one of the bitrates its
  encoder writes: MP3's fourteen constant rates from 32 to 320 kbps, AAC's 96,
  128, 160, 192, 256 and 320, Opus's 64, 96, 128, 160, 192 and 256. The
  formats are offered FLAC, WAV and AIFF, then MP3, AAC and Opus. A new
  format is named after its codec, with the first free number
  when that name is taken ("FLAC", "FLAC (2)"), the number in the digits of the
  viewer's language ("FLAC (٢)" in Arabic); names are unique, and two
  formats may write the same thing. The settings add, change and delete one
  format at a time, and bae refuses an added or changed format whose name
  another has; the edit sheet shows that refusal under the name field. Save as… offers every format, for a track
  and for a release, starting with the one last used; its Format menu ends in
  Edit Formats…, which opens Settings at Save as in front of the panel, which
  stays open behind it; the menu follows any change made there, and a format
  added there becomes the choice of every Save as… panel open at that time. A
  panel opened later starts with the format last used. Deleting the format last used makes the
  first format left the last used, in the same write; the only format left
  can't be deleted, and its Delete… is off. The file name field's preview
  ends in the extension of the format Save as… starts with.
  One file name pattern and one folder name pattern serve every format. A
  pattern is text with values in it, each value one chip: the person types
  between the chips (`Artist - Album`), and Delete after a chip or Forward
  Delete before it takes the whole chip out. A value the track or release
  does not state comes out empty and the text around it stays. Typed text
  that can't make a name turns the field red with why: text that starts the
  name with `.`, one of `\ : ? * " < > | /`, or no pattern at all. Until it
  is fixed, a save is refused, saying why. A file's name is its pattern's
  text as typed, spaces and all, with each value filled in and no leading
  `.`, written as a copy-out writes a name (§4.4), so a title
  `Some/Weird:Title` saves as `Some／Weird：Title.flac`; a single track's Save
  as suggests the same name, whatever format it starts in, and choosing
  another format changes only its extension. A release save writes its files into a folder
  named by the folder pattern, written the same way. The album year is the
  year the album first came out; the release year is the year this release
  came out, or the album's year when the release states none.
  A release with several labels or catalog numbers names its folder by every
  one, in the order the release lists them, a repeat (a label on two of its
  catalog-number rows) named once at its first place, joined with ", ": the label value
  of a pattern renders `Harbor Records, Tide Pool`, the catalog number value
  `HR-1042, TP-7`.
  A release save creates each track's file new, like export, so two names a
  drive holds as one (`Track Title` and `track title` on a drive that folds
  case) fail the save rather than one track replacing the other; a single
  track's Save as replaces a file only when the person confirms it.
  A release save converts several tracks at once, and an export copies one
  file at a time.
  Every save writes one file per track. A pregap, stored or generated, goes
  at the end of the track before, and the hidden audio before track 1 stays
  at the start of track 1; a release stored as one image with a CUE sheet is
  split at its track marks. A track that is all of one file, already in a
  lossless format's codec at the depth the format writes (WAV and AIFF count
  at 16 or 24 bits), is copied without re-encoding, its tags, cover and name
  written fresh; a lossy format always encodes.
  Each file's track number is the track's number on its side, and its disc
  number the track's medium's position, written only for a release of several
  mediums, and its YEAR the pressing's year, else the album's. Three
  settings serve every format, all on to start: embedding the cover in each
  file; a release save adding the cover as `cover.jpg` in its folder; and
  loudness tags, each file's ReplayGain track and album gain and peak from
  the loudness measured at import (§3.7). FLAC and MP3 formats come built in
  (Appendix C).

Both run from a queue that can be paused and cancelled, kept and emptied as
the pin queue is (§4.3), but running again after a restart; pausing stops the
running export or save at once, leaving no output at the final path, and
resuming runs it again from the beginning; a failed one
stays in it, with its error, until it is cancelled. One that breaks on a bug fails the same way, reported as one, and
the queue goes on to the next. A save keeps the format and save settings it was
queued with, so editing them changes nothing already queued. Either reads files
from wherever they are: the person's own path, the cache, or storage.


### 4.7 What a device keeps

Settings → Storage shows what this device keeps for the open library, in bytes
on disk, as a total and a bar of parts; every other library on the device has
its own. The parts, in this order, each shown while it holds bytes, Library
always:

- **Pinned**: releases kept whole to play offline, and the pins on the way to
  being kept (the partial downloads a stopped pin keeps). Pinned releases don't
  count toward the cache (§4.3).
- **Cache**: tracks played from the cloud, covers and pictures, and on a
  desktop the catalog images found while importing. bae downloads them again
  when the person plays or opens them.
- **Library**: the library's database, its settings and keys, and the copies
  of files coven keeps until they are stored.
- **From before the reset**: the installation a reset of this device kept
  (§6.4), while one is here. Nothing uses it.

Releases imported as local stay in their folders, and bae does not count them.

On a desktop, bae keeps each catalog image it downloads, or the answer that
an address serves no image, as a file in the library's cache folder, with a
row in the device-local table `cached_image` that holds the file's size and
when bae last read or wrote it (Appendix A.12). Every row names a file that is
there. A file that no row names, which a crash can leave, bae neither reads nor
counts, and Clear cache removes it. When the rows'
sizes add up to more than 128 MiB, bae deletes the rows of the files it read
or wrote longest ago, and those files, until the rest fit.

**Clear cache** removes every unpinned file and chunk in every kind of file the
library keeps, and on a desktop deletes every `cached_image` row and then the
catalog images' folder. Pinned releases and pins on
the way stay. It asks nothing, and is off while the cache is empty. The two
stores are cleared one after the other; a failure of either is reported, what
was cleared stays cleared since it was all cache, and clearing again finishes.

**Keep up to** is how much audio and other release files the cache holds before
the least recently played go: 1, 5, 10, 20 or 50 GB (decimal, as the byte
formatters print them), 20 GB until chosen. It is coven's budget for release
files on this device, in its own table; no choice is stored by bae. Choosing a
lower limit removes the least recently played files down to it in the same
write. A budget no choice names is an error, never shown as the default. A
reset or a restore in place starts it at 20 GB again.

**Delete this data** (on a Mac, the Old library row's *Delete…*) deletes the
installations resets kept on this device, after asking; Storage shows it while
one is here. The library stays as it is.

On a Mac the Storage pane, after Transfers in Settings, is a box with the total
and the bar over the parts' legend, then Cache (with Clear cache and Keep up
to), Pinned (with Ask before unpinning, Appendix C) and, while one is here, From
before the reset (an Old library row, whose Delete… asks first). Android's Storage page, after
Transfers in Settings, holds the same in a card and sections: Clear cache and
Keep up to (a dialog of radio rows) under Cache, the switch under Pinned, and a
Delete this data row under From before the reset while one is here. Its rows
follow the measurement as it changes, and a failed measurement
shows its error in place of the page.

**Edge cases.**

- **Deleting a release or an album** deletes their rows and coven's copies of
  their files. The person's own files are never deleted. A deletion is
  planned against the rows and files it deletes; when they change before its
  write (a release moves, comes or goes, or its files or cover change), the
  deletion is refused and writes nothing, and asking again deletes what now
  stands.
- **A device with no storage** has only local releases; moving to storage
  needs it set up.

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
rename. An invite carries the current name, since it expires within 30 days.
The entry coven keeps a restore code in, in iCloud Keychain on Apple devices
and in the phone's Block Store on Android, also saves the name, as one of the
person's devices last saw it, so a new device's list can say which library
each code is for (§5.6). It is only a label there: a rename made
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
| `history` | play | private or household: one setting per person, which can also be off |
| `playlists` | playlist | each one private or household |
| `playlist_entries` | place in a playlist | where its playlist is |

**Ratings.** A rating is one to five stars, given by one person to an album or
to a track. Ana gives *Album Title* four stars; Ben gives it two. They are two
rows, one per person, keyed by the person and the album, so Ana rating it again
from her phone changes her one row. Clearing a rating deletes the row. A
track's rating belongs to one release's track, so the tracks of two copies of a
pressing (§2.7) are rated apart. An album's page shows the person's own stars
for it and a line naming who rated it: *You* when they did, then each other
person who did, with their stars, by name. A track's stars on the page are the
person's own. Only a person with a profile rates (§5.4). The page shows the
rating when this person can rate or anyone rated the album.

On macOS the album's stars sit above its tracks: clicking a star rates the album
with that many stars, and clicking the star the person already gave clears the
rating. A person without a profile sees no stars, only the line of who rated
the album.

On macOS a track's row shows the person's stars for it, and its menu offers
*Rate* to a person who can rate: a submenu of five stars down to one, the
person's own checked, then *Clear rating*.

On Android the album's stars sit under its header with the line of who rated
the album under them, and a tap on a star rates and clears as on macOS.

Ben keeps his history private. Carol's devices never see what he played. He
makes a playlist for a party and shares it: it moves from his circle to the
store, with its entries, and everyone sees it. A move between the store and a
circle is an ordinary write in coven
([§15.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#154-moving-rows)).

**History.** Ben plays track 3 of Ana's album for two minutes: one `history`
row records Ben, the track, when the play started, and how much of it he
heard; the track names its release. He skips track 4 after ten seconds:
nothing is recorded. A play counts once 30 seconds of the track, or half of
it, whichever comes first, has played; a 40-second interlude counts after 20. A skip isn't a listen.

How much he heard counts only audio that played: not pauses, not waiting for
the network, not what a seek jumped over. The row is written when the play
counts and updated as he keeps listening. Its start time is the playing
device's clock when the play began, stored as UTC.

bae records plays and syncs them by their audience. Plays show in *History* and
nowhere else: no page in Settings lists them. *History*, from the person's
menu, lists the plays this device holds together, newest first, each with when
it was played and whose it is, under the day they were played, and shows
everyone's plays or one person's. On a Mac History is a tab of the side pane,
and the end of its first day's heading names whose plays it shows, *Everyone*
or a person's name, in the accent with a ⌄, above the column of whose each play
is, and opens a menu of those choices; on Android the same choice ends the
screen's top bar.

Each play is its own row, with a fresh id, so a play count is the number of
rows, never a cell holding a number.

**Changing the history setting** applies to new plays only. The setting is *Off*,
*Only me* (private) or *Everyone* (household). A play takes its audience from
the setting when it begins, and keeps it. With *Off*, a play that begins records
nothing, here or on any device; plays recorded before keep who could see them. Ben switches his history
to household on Friday; what he played before Friday stays in his circle.
Moving old plays would publish what he played in private, and switching back
couldn't take from the household what its devices have already read. A new
profile starts private. A play is recorded only as the person it began as:
once this device is a different person, saving it is refused.

**Plays on a removed device.** A device's person has their profile from the
moment the device registers (§5.4), and before that the device holds none of
the library's rows, so it has nothing to play. A device removed from the
library keeps its copy and keeps writing to it, plays included: each play is a
`history` row under its person's profile, as on any device. The row stays on
that device, which no longer syncs.

**Playlists.** A playlist belongs to the person who made it, and is private or
household, each playlist on its own. Its entries are rows keyed by the playlist
and a *position id*, a fresh id per entry, so the same track can sit in a
playlist twice. Each entry also has a position number that orders it: an
append takes the next number, and a move renumbers every entry from 0.
Playlists list by name, and profiles list by name too. A name a person gives
a library, a profile, a playlist or another printed name of an artist is kept
without its leading and trailing spaces, and a blank one is refused; a Create
button stays off while the typed name is blank. A playlist made from a track holds that track from its first write, and
one made with a track the library lacks is not made. A playlist plays as a list
of its tracks in its order (§7.1), from the entry picked, from the first, or
shuffled; one with no entries plays nothing, and a picked entry the playlist
no longer holds plays nothing, shuffled or not. On macOS a playlist's page has
Play and Shuffle.

The Playlists list shows every playlist the person can see, by name: their
own and the household ones others made, never another person's private ones
(those are not on this device). Each shows its track count, its length, its
owner, who sees it, and a picture. The picture is the covers of the first four
different releases its entries are on, as a 2×2 square; a playlist on fewer
than four releases shows its first entry's cover, and one with no entries
shows the missing-art tile. A playlist's page shows its owner, count, length
and who sees it, then its tracks in order, each with its credit line (§2.3),
album and release cover. On macOS each track also shows its number in the
playlist, from 1; while a move's write runs, the rows show no number. The Add to playlist menu offers only the person's own
playlists, and a playlist can be made only once the person has a profile.

A playlist is made with a name and who sees it, *Only me* or *Everyone*,
starting on *Only me*; a blank name cannot be made, by the rule that refuses a
blank library or profile name. On macOS the side pane's Playlists tab lists
the playlists, each with its picture, name, track count and who sees it, and
a + button at the start of the pane's top, *New playlist…* to screen readers,
opens the sheet for a new one; clicking a playlist
opens its page in the window, as the playlist made opens. Dragging tracks onto
a playlist in the pane adds them to its end: while they are held over it the
playlist is drawn in the accent.
On Android the Playlists screen, opened from the person's menu, lists the playlists with a *New playlist* button
under them, or under *No playlists yet* when there are none, which opens a
dialog for the name and who sees it, and the playlist made opens; a person
without a profile sees *Playlists appear once you have a profile* and no
button. A playlist opens as its own page on Android, with its picture, owner,
count and who sees it, then Play and Shuffle and its tracks in order, each with
its credit line and album, a tap on one playing from it; a playlist deleted while
its page is open says it is not in the library anymore.

On macOS a track's menu offers *Add to playlist* when the person can make
playlists: a submenu of their own playlists by name, which adds the track to the
end of the one chosen, then *New playlist…*, which opens the sheet holding the
track.

A playlist's owner changes it from its page; no one else sees these controls.
The page's options menu (the ⋯ button after Shuffle on macOS, the bar's menu on
Android) offers *Edit playlist…*, which changes its name and who sees it, and
*Delete playlist…*, which asks first ("bae will delete this playlist from every
device. Its tracks stay in the library.") and then deletes it on every device. Saving
an edit writes the name and who sees it in one write, so a failure changes
neither and leaves the dialog open with its error. A track's menu on a playlist's page offers Play
next and Add to queue to everyone, and *Remove from playlist* to the owner. The
owner reorders the entries by dragging a row: on macOS a line marks the edge
of the row the dragged copy is over, where the entry goes when the owner lets
go, and on Android a held row lifts and the rows between make room. Letting go
moves the entry to just before or just after the entry it now sits by. On
Android a screen reader also offers *Move up* and *Move down* on each of the
owner's rows, each a move of one place, except *Move up* on the first entry and
*Move down* on the last. The row stays where the person let it go until the write returns; then the
page draws the entries as stored, so a write that fails
puts the row back.

On Android a track's menu, on an album's page and on a playlist's, offers *Add
to playlist…* when the person can make playlists: a sheet of their own playlists
under *New playlist…*, which adds the track to the end of the one chosen or
makes a new one holding it.

Ben adds a track to his party playlist, which holds seven entries: the new
entry takes position 7. He drags it to the top: the move names the entry and
the entry it now sits just before, and bae puts it there in one write,
renumbering the other entries from 1 in the order they were in. A move up from
the first entry or down from the last leaves the order as it was, and a move
that names an entry the playlist no longer holds fails.

### 5.4 Profiles

coven owns identity: members, roles and devices. bae keeps a synced `people`
table, in the store, holding each person's name, avatar, private circle and
history setting.

- **A person is keyed by their coven `MemberId`**, which is stable: replacing
  keys, or re-inviting someone who lost every device, keeps the same id
  ([coven §13.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#133-losing-everything)).
- **Ratings, history and playlists point at the person**, not the member.
- **The joining person has a `people` row from the moment they join**, under
  the name they typed, which their join request carries with the private
  circle it asks for (named `bae-person:` and the member's id, which the
  invite fixes). Coven admits the person, their device and their circle in
  one step, and bae registers a joining write with coven: as a local write, in
  the transaction that first applies a device's registration, it inserts the
  person's row (that name, private history, that circle as `private_circle`)
  when the member has none
  ([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)).
  The registration of any of the person's devices runs it (a join, a
  restore, a pairing, a reset, or a snapshot that covers it) on every device
  of theirs, once each, and a failing write fails the registration. So a join
  that finishes in one go, one whose first load failed and one bae quit
  during all give the person their profile as their device registers, with
  no later step to wait for. They can rename themselves later. Someone
  re-invited after losing every device, or restoring, keeps their row, since
  their `MemberId` is the same: that admission carries no name, and the write
  only points the row's `private_circle` at the circle the admission made
  when it names another, the old circle's key being lost with the old keys.
- **The person who made the library** is its first member and admin from the
  moment it exists, with this device registered, though it has no cloud home
  yet. **Create new library…** on a Mac asks their name first. coven records
  that name and their private circle, which bae names `bae-person:` and the
  member id coven makes for them, as their join facts in the library's first
  slot, as an admission records a joiner's, and makes the circle with them as
  its only member
  ([coven §10.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#103-creating-a-store)).
  The joining write above runs as that slot applies on this device and
  inserts their `people` row (that name, private history, that circle), so a
  library never exists without its creator's profile, offline and before any
  storage. Each later device of theirs runs the same write at its own
  registration and finds the row, so a device restored before this one ever
  uploaded still gets the profile.
- **A removed person invited again is a new member.** coven gives them a fresh
  `MemberId`, and they start with an empty new profile. Their old `people`
  row, ratings and household playlists stay under the old identity, read-only:
  they are owner rows (§5.5), and the old identity has no device left to
  change them.
- **An avatar is a file on the `people` row**, bae's own: the app hands bae
  the image the person picked, and bae stores it the way it stores a cover
  or an artist image (§4.1). Other devices download it when it is first
  shown.
- **The people list shows everyone in the library**, each with their profile
  once it reaches this device, sorted by the profile's name in the device's
  language. The people with no profile yet come after them, in the order their
  member ids were made: when the founder created the library, or when an
  inviter created the invite. A removed person's row stays, read-only, and the
  list does not show them.

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
| Add a device | the person, by pairing: one of their devices shows a pairing code, the new device sends a request with it, and the person approves it after checking a number both show ([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)) | nothing beyond coven |
| Remove a device | the person, for their own; an admin, for anyone's | nothing beyond coven; the person tells this device when they have cut a lost device off at the provider (below) |
| Remove a person | an admin | their circle goes, with its rows |
| Rename the library | an admin | every device shows the new name (§5.1) |
| Rename yourself | the person | every device shows the new name on their profile (§5.4) |

**People and devices.** Settings keeps the library's people on a page of their
own, with two sections. *You* holds the person's picture first: their image,
or the initials of their name on a tint while they have none. On a Mac,
*Choose…* opens the system's panel for picking an image, and *Remove*, shown
while they have a picture, clears it; on Android, tapping the Picture row opens
the system's photo picker, and a *Remove picture* row in red, shown while they
have a picture, clears it. A picked image that the phone cannot read shows the
snackbar "Could not read that picture." Then the person's name, with *Change…*,
which renames their profile in a sheet that says everyone in the library sees the name
and keeps the name as typed when the rename fails, with a danger notice,
"Could not save the name.", and *Details* for the cause, but no retry button,
since *Save* is the retry; their role, which only an
admin changes, and from another person's page; their listening history, *Off*,
*Only me* or *Everyone* (§5.3), with a note, under the box on a Mac and in
the dialog on Android: "Who sees what you play, or Off to keep no record.
Applies to new plays; earlier plays keep who could see them." (Android: "A
change applies to plays from now on; …"); and *Devices*, which opens *Your
devices*: each of the person's devices, this one reading *This device* with no
menu since a device leaves by removing the library from it, and each other
device with a ⋯ menu (⋮ on Android) holding *Remove…*. On a Mac, *Add device…*
is a plain button under the box, on the right; on Android *Add…* is the last
row. *People* has one row for each other person, led by their picture or initials,
with their role and how many devices they have, and the open invites below them; on a Mac an admin's
*Invite…* is a plain button under the box, on the right, and on Android *Invite*,
with a +, is the last row of the People card, after the invites. An admin's row opens that person's
page: their role, with *Make admin* or *Make member*, *Remove from library…* in
red, and their devices, each with a ⋯ menu (⋮ on Android) holding *Remove…*. A
member's person rows open nothing: a member has nothing to do there. On Android
the Name row opens the rename dialog, whose **Save** is off while the name is
blank, and listening history is chosen from radio
rows in a dialog. Removing a person or a device still asks first. A person's
page closes, and the people page shows, once that person has left the
library. Rename Library starts from the name as it stands when it opens, and
the macOS menu bar always shows Rename Library, enabled for an admin.

Removing a person or a device, changing a role, renaming the library and
approving a request are admin changes. Each is one write on the device that asks: coven checks it
against the library as that device has it, except an approval, which only sync
checks, and records it, with or without a connection, and the call returns
once it is recorded. A person, device or circle that device hasn't read yet
isn't refused there, since it may have joined elsewhere: the change is
recorded, and sync refuses it as gone if it finds none once it has read on. Sync then catches up,
checks it again and publishes it; until then the library is unchanged, on that
device too
([coven §11.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#111-the-steps)).

The same write records what the change changes in the device-local
`admin_operations`, keyed by its coven operation: the new name, the person and
the role, the person, the device, or for an approval the person it lets in
and the name the invite gave them. coven records what kind of change it is and
how it ended, so the row holds neither. Each change shows, in a box titled
**Changes from this Mac** on a Mac and **Changes from this device** on Android,
where the change is: a rename under the library's name on the Library page, a
role change on that person's page above their role, and removing a person or a
device and approving a request on People and devices. Each place shows only its
own changes, and its box shows only while it has one. Each shows as what it
asks ("Remove Ben",
"Rename the library to “Home”", "Let Dan in"), named from the person's profile or the
device's name as they stand ("Remove a device" for one this device hasn't
read yet): waiting to sync, then done, refused or
stopped. Refused says why the library refused it (Ben is no longer in the
library, it would leave the library without an admin, this device isn't
allowed to make it, such as an approval after another admin demoted Ana, or,
for an approval, the library has changed its keys too many times to send them
all to a new device at once).
Stopped says why this device stopped syncing for good before the change was
made: another device removed it, a restore replaced its copy of the library,
another copy of its library syncs as this device, or another of the person's
devices replaced their keys. Nothing refused a stopped change, and another
device may still make it. Done covers a change sync found already made. Ana's laptop is offline when she removes Ben's old phone; the
row says it waits to sync, Ben's phone stays in the list, and once the
laptop syncs the phone moves to his removed devices and the row says done.
A finished change stays until the person dismisses it: one write
acknowledges its outcome to coven and deletes its row. A change still
waiting can't be dismissed. Removing a device says at once what is left to
do at the provider, titled as a removal still to happen
([coven §14.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#142-provider-access)
names the step for Dropbox, OneDrive and iCloud, and on S3 there is none).

**Cutting off a lost device.** When the step is for one of the person's own
devices and their account holds the library's storage, the same message offers
**I cut off this device** beside **Not yet**: the person does the step at the
provider and presses it, which tells coven that nothing more of that device
can land, so this device takes back every account the lost device's shares
named that no member or open invite uses
([coven §14.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#142-provider-access),
[§20.9](https://github.com/bae-fm/coven/blob/main/spec/coven.md#209-sharing-with-accounts)).
Only the member whose account holds the store has devices that change
sharing, so for anyone else, and on S3, the button would only be refused and
isn't offered. When bae can't find out whether the person's account holds the
store, the removal fails to the person, changing nothing, and they retry it. Coven refuses a device the
history doesn't have removed yet, which the removal on this device may not have
reached; the app then says the removal hasn't reached this device yet and asks
to **Try again** after the next sync, or **Not yet**. While a request to one of
the accounts runs, coven changes nothing and the app says this device is still
syncing with the cloud and to wait for it to end, with **Try again** and
**Cancel**. Another person's device
removed by an admin gets only **OK**: its person does the step.

A new phone's first screen shows bae's sheep and name over "Your music,
everywhere", with **Scan code** and **Enter code** at its foot: the code says
what it is for. An invite
joins someone's library, and the person's own restore code adds this device to
theirs. An Android phone that holds libraries lists them under **Your libraries**,
ending in **Add library**, which opens a sheet with **Scan code** and **Enter
code** in place of the buttons, each library opening on a tap, with a ⋮ menu holding
**Details…** for a library that opens, and **Remove…** (§6.4). **Details…** shows, in a sheet, the library's sync
status as this phone last knew it (as on a Mac, below), when it last synced, and
its provider, account and folder (no *Open in browser*: the Sync page has it). One that has restore codes saved on it lists
them below those (below). A Mac lists its libraries the same
way under **Your libraries**: a click on one opens it, and its ⋯ button, or a
right click, holds **Remove…**, or **Delete…** for one never synced. A
library that can't be read, or whose reset, restore or removal stopped
partway, says so under its name, and its menu offers to finish it first. A
store whose name can't be read is listed beside the others, titled *Library
that cannot be read* with its store id under the title, and offers Remove
only. A join or restore that stopped before it finished and whose saved
settings can't be read can't resume, and coven fails listing the joins and
restores under way, naming its library, instead of listing none. The first
screen shows that failure as a row after the libraries, in the same list, even
when the device holds none: *Unfinished join that cannot be read* with the
library's id under it and **Discard…**, which asks "Discard the unfinished
join?" ("It did not finish, and this Mac cannot read what it saved. Discarding
removes it from this Mac. The library in the cloud is not touched." on a Mac, "phone" on
Android) with **Discard** and **Cancel**. **Discard** deletes the unfinished
installation from this device and lists what is under way again, so another unreadable one shows its own row and a join
that can resume goes on; a discard that fails says why where the listing's
failures show, and the row stays. A join whose request was sent stays pending in the
library's storage until its invite ends, and the person's saved restore code
stays. A
library that fails to open on a Mac shows the failure joined to its row, under
its name: **Could not open**, with **Details**; when Reset from cloud
replaces what failed (bae can't use the library's data on this Mac, such as a
damaged database file or tables that don't match the app's, and the library
has storage to load it from), it says "Something is wrong with this library’s
data." and offers **Reset from cloud…**, the reset below, in place of *Retry*.
A library kept on this Mac only has
none, so its failure shows **Details** alone, and its row's menu still offers
**Delete…**. A phone
that fails to open a library the same way shows a notice in the middle of the
screen, "The library did not open" over the same sentence, with **Details** and **Reset from
cloud…** under them, which asks as a device reset does (§6.4). A
library row shows its name alone otherwise, on a phone and on a Mac: no
provider line, and nothing for one that never synced. While a removal runs, its
row shows the step under its name with a spinner in place of the ⋯ (the ⋮ on a
phone), and the other libraries and adding one are dimmed and can't be used
until it ends. **Details…** hangs a popover off the ⋯, titled with the library's name, with its sync status as
this Mac last knew it (**Synced**, or **Unsent changes** while it holds some it
never uploaded to its storage; a library kept on this Mac only uploads nothing,
so it never shows **Unsent changes**), when it last synced, and its provider, account and folder, without *Open in
browser*, which the Sync page keeps. A
library that is open shows its live sync status; any other is read as it last
stood, without opening it, and one that is closing once its close has ended.
A library stops counting as open as its close begins, even while the app
still holds it. A Mac's first screen
shows the sheep and the name over "Your music, everywhere", and offers the same two under **Create new library…**, which asks for the person's name on a step of its own, **Start a new library** (**Your name**, **Cancel** and **Create**, off while the name is blank), before it creates anything, and its File menu offers
them as **Scan Code…** and **Enter Code…**; the code decides the same way.
Once it lists libraries, the three are the items of **Add library…** under the
list. On a Mac, **Enter Code…** opens a sheet over the first screen, titled
**Enter code**, with one field for a restore code or an invite, **Cancel**, and
**Continue**, off until the field holds something; **Scan Code…** opens the
scanner. On both platforms the field is several lines tall and monospaced, and
shows a typed or pasted code in groups of four with a space between, as Settings
shows a restore code, and bae reads the code without the spaces. A scanned code starts at once, as an invite does, with no sheet to
confirm it, and a typed code starts when the person chooses **Continue**: the
person scanned or entered it to add this device, or to join. Only an S3 code
without the key stops first, to ask for it on a second sheet, **S3 key**, in a
few words ("Enter your credentials for your S3 storage.") over the access key's
two fields, with **Back** to the code and **Add**. The running restore takes the
window in place of the sheet, with its step, a bar and **Cancel**, and a running
join takes it with its wait for approval. A code that doesn't read, and a
restore or join that fails, says so on the sheet, to try again; a join
under way, whose code bae kept, offers **Join** there to resume it. On Android the
code screen calls a restore code's action adding this device: the button is
**Add**, and a typed restore code waits for it. A saved code, in iCloud Keychain
or on an Android phone, and a restore over this device's copy (§6.4), say
**Restore**. On a Mac the restore over this device's copy reads its code
when the person chooses **Restore**, not as it is typed, so a half-typed code
shows no error; **Restore** is off until the field holds something. A scanned code on Android starts at once, as an invite does, and
only an S3 code without the key stops first, to ask for it over the access
key's two fields. A code that doesn't read says so on the code screen, and a
restore that fails shows its error beside **Add**, to try again. The code screens on both platforms do not show where the library is stored.
On Android every screen that holds a text field (the join name step, and the screen that asks for the S3 key of a saved code) keeps its fields above the keyboard.

**A restore that stops partway.** Just before coven adds the new device to the
library, the restore records itself on the device: the code, the device's name,
and on Dropbox and OneDrive the person's sign-in. From then on it only goes
forward
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
When bae quits or the restore fails after that, the first screen opens on the
running restore the next time it shows, as at a start with no library to open,
and finishes it from what coven kept, with no code. Only when coven has no
sign-in it can use, because the provider refused the kept one or it is gone,
does bae show the sign-in again. A join under way goes on first, and the restore
the next time the first screen shows after it. A finishing that fails shows its
error on the Enter code sheet, where the person's restore code finishes the restore
too. A restore whose person's keys another of their devices replaced since ends,
and says that the current restore code restores the library. Cancel stops it as
it stops any restore: until the device was added, coven takes the restore back
and nothing of it stays; after, it can't be taken back, and bae says that only
finishing the restore goes on from there.

A device holds one join or restore of a library at a time: a restore of a
library whose join waits on the device, or a join of one whose restore
recorded itself there, is refused before anything changes ([coven
§13.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#134-edge-cases-installing-on-the-device)),
and the code screen goes on with the call under way instead, a join first, as
at launch.

Dan joins. In People, Ana taps **Invite** and enters his Dropbox email, with a
line saying the library is shared with that Dropbox account so he can join, and
taps **Create invite**. The form asks for no name (Dan types his own when he
joins), but for his role, Member or Admin, and how long the invite lasts, 7 days
or 30 days; it starts on Member and 7 days. Her laptop records the
invite in one write, offline too, and shows it at once, marked as sharing
access: sync then checks it again and shares the store's folder with his
account, and the invite's code shows once it has. On S3 the form asks for an
access key Ana made for Dan, or **Use my key instead**: Dan then uses the key
this device connects with, and the form says that to remove him later Ana makes a new
key for herself and enters it on each of her devices; **Use their own key
instead** goes back. A library on iCloud or OneDrive asks for his Apple Account
or Microsoft account email in the same way. When sync can't share it, or
the library refuses the invite (Ana is no longer an admin, say), the invite
says why in its row and where its code would be, and **Cancel invite** ends
it. A grant the provider refuses for good, for a reason only a person can
fix (Dropbox has no account for the address, the inviter's email is
unverified, their team doesn't share outside the team, their plan has no
sharing, or the folder holds as many people as the plan allows), ends the
invite at once instead of being retried, and the invite is a notice until Ana
dismisses it (**Open invites**, below). Either way the invite carries the
library's current name and Ana's, the name on her profile (§5.4); a person with
no profile can't be an inviter, which is a bug of bae's, not a state to show
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)):

Dan taps **Scan code** and scans Ana's screen, or Ana sends him the code and
he taps **Enter code** and pastes it. After the code reads, his phone asks his
name on a step of its own, "You are invited to Household", with **Your name**
empty (its placeholder says it appears on his profile, ratings and playlists),
and **Join**, off while the field is blank; **Cancel** (a Mac) or Back (Android)
returns to the first screen. A code whose library already has a join under way
on his phone goes on with that join instead, with no name step, since coven
refuses a second, different name. He then joins under that name, which coven
refuses only by its rule for names, and the step says so. His waiting screen
says "Tell Ana this code" over his confirmation number, with a spinner beside
"Waiting for Ana to let you in…" (Ana is the name the invite carries as its
inviter), and under a "Join library" bar on Android. It has no countdown, no
**Check now** and no date: while it shows, the join checks every 5 seconds
from the end of its last check, and when it doesn't, on coven's doubling waits
(a provider's cooldown holds either). Every other state of a running join (opening,
connecting, loading, registering, committing, an error) is the same step with the library's
name as its headline, the progress line under it, and **Cancel** while it can cancel. When
his request arrives, Ana's laptop shows a dialog titled "Dan wants to join", with the name Dan typed, says "Approve them if their screen shows this code:", and shows
the same number under it, with **Approve** and **Decline**. Ana compares it with
Dan's screen, or asks him for his number, and approves if it matches. Every
invite waits for her approval: someone who saw the code could use it before Dan,
and only the number shows whose request it is.

Once his phone has opened the library and loaded his admission, his
`people` row exists, written as his phone registered, with the name he typed
(§5.4). Everyone's devices show Dan by name.

**Once Ana approves.** As soon as Dan's phone finds her approval, the library
is on his phone and the join is over: coven keeps his keys and publishes the
library, then loads it, and a cancel changes nothing from then on
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)).
When the load fails, because his phone lost its access to the storage first
(Ana removed him, or the provider took the share back), the library opens all
the same, under the name he typed, and the app says what failed.
It is no failed join, and the first screen lists no join under way for it: it
is a library that can't reach its storage, which its sync state shows, which
sync retries, and which Dan removes as he removes any library. A load that
bae quitting stops leaves the same library, whose sync loads the rest once it
opens. A load that stopped before it applied Dan's admission leaves him with no
profile on his phone yet; the load that applies it writes the profile, at that
open or a later one (§5.4).

**Open invites.** Until it ends, each invite Ana's laptop made is a row in
People: the address it shares with (the key's id on S3), marked invited, with a
chevron that opens the invite's own screen, titled **Invite**. While this device
is offline and sync has not yet shared the library with the address, the row
says "Shares access once online". Once Dan's request is approved, his row is
his name marked joining, with the address under it, and opens nothing. The rows follow coven's list of this
device's open invites
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)),
which coven ends when the invite is approved, declined, cancelled or expires, so
an expired invite leaves People when its lifetime ends. Only the device that
made it shows the row, since only that device can act on it. Opening
the row shows the invite again. While the library is being shared with the
address, its screen shows a bar and "Sharing the library with dan@example.com…";
while the sync state says this device is offline, it says that the code shows
there once this device is online and the provider has shared the library with
the address; once the library is shared it shows the code to scan or enter on
Dan's device, with **Copy code** and **Share**, **Cancel invite** beside
**Done**. While the screen shows, this device syncs every 5 seconds instead of
every 30, as Add device's sheet does. A row shows coven's
pending record for its invite when it is a hold: a refused check, a failed
grant, or a failure to read Dan's request. The invite's own waiting is not
shown: the request it hasn't found yet, and a request that awaits Ana's
decision. Closing the request dialog keeps the request, and the row opens it
again. Approving is one write on Ana's laptop that records the approval, offline
too; sync then lets Dan in. From that write until the invite ends, Dan's row is
marked joining and offers no Approve or Decline. Once Ana's laptop has begun
letting Dan in, cancelling fails and says Dan is already joining, rather than
leaving him half joined.

An invite a sharing refusal ended leaves coven's open invites, and coven keeps
the operation's outcome until Ana's laptop acknowledges it. People shows a
notice in the invite's place: the address it was to share with, "Could not
invite:" and what went wrong and what to fix ("Dropbox has no account for it";
"your Dropbox email is not verified. Verify it, then invite them again"), and
**Dismiss**. The notice stays until she dismisses it, in its place among the
rows by when the invite was made. **Dismiss** is one local write that
acknowledges the outcome to coven, so the notice leaves the list in that
write.

On S3 the invite's screen also shows the id of the key it carries, and says to
delete that key in the provider's console if nobody uses the invite: bae keeps
no record of it once the invite ends. Cancelling an invite asks first ("Cancel this invite?", "dan@example.com
will not be able to use this invite.", **Keep** and **Cancel invite**), and on S3
the question also names the key, as removing a person does.

**Dan's waiting.** Dan can close bae while he waits. When bae next starts, it
opens on his waiting screen and keeps checking, with no sign-in: coven keeps
his Dropbox or OneDrive sign-in on his phone beside the join
([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)).
Only when coven has no sign-in it can use, because Dropbox refused the kept one
or it is gone, does bae show the sign-in it showed when he first joined, then
go on checking. Cancelling that sign-in leaves the join under way for the next
start. The waiting screen has no Cancel once his request is sent, since coven
can't take a sent request back: the join ends when Ana approves, declines or
cancels, the invite expires, or Ana's laptop is gone for good (removed,
replaced by a restore, or its person's keys replaced elsewhere), whose
requests the library's other admins end. Before then, while Dan signs in to
his provider, Cancel works, and the invite stays open for him to scan again;
on Dropbox, cancelling also takes the library's folder back out of his
Dropbox if his phone had added it.
Dan's phone sends no request once the invite has expired by its clock, and a
request that arrives after the invite ended is turned away rather than left
waiting. A code he enters after it expired, with no join of that library under
way on his phone, says the invite ended before asking him to sign in to his
provider. When the invite ends without letting him in, Dan's phone shows one
screen, "This invite has ended" with "Ask Ana for a new one.", and **OK**,
which returns to the first screen: it was declined or cancelled, it expired,
or the device that made it is no longer in the library. Each ends his request
the same way, so his phone can't tell which, and a join that was under way at
launch ends on the same screen.

Removing a person deletes their circle and its rows: their private history and
private playlists. Their household ratings, history and playlists stay,
credited to them.

On S3 each person's page in People and devices shows the id of their access key. Removing
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
tablet gets the code one of these ways:

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
- **saved on an Android phone**: coven keeps each phone's codes in Google's
  Block Store, which doesn't sync. The phone's first screen lists the codes
  saved on it for libraries it doesn't hold: those it saved itself, and those
  a new phone received when Carol set it up from her old one, directly or
  from the old phone's cloud backup. Tapping one restores it
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code));
- **typing** it in, or pasting it, as it shows: in groups of four with spaces
  between them, or the same characters run together. The spaces drop before
  the code decodes.

The saved-code list sits on the first screen under Your libraries, titled
*Saved in iCloud Keychain* on a Mac and *Saved on this device* on an Android
phone, with a line under it saying what the codes are. Each row ends in a ⋯
menu (⋮ on Android). The list shows each code under its library's name as Carol's
devices last saw it (§5.1), over where the library is stored: the provider and
account. Dropbox and OneDrive show the account's email; S3 shows the S3 storage's name and
endpoint; iCloud shows iCloud. Two libraries can share a name, and a rename
none of her devices has seen yet leaves the old one, so the location is what
tells them apart. On a Mac a row has a **Restore** button, and the restore screen for a saved code shows both. On
an Android phone a tap restores at once, as a scanned code does; only an S3
code with no key saved beside it stops first, on a screen that shows both and
asks for the key, and a failed restore returns to that screen, to try again. A
code deleted since the list was read returns to the first screen, saying it is
no longer saved on this device. A typed or
scanned code carries no name, so its screens show only where the library is
stored until the tablet has loaded it, then the library's current name.

**Deleting a saved code.** Carol once tried a library on S3, then deleted its
S3 storage, and her new tablet still offers it. Each code in the saved-code list
has **Delete…** in its menu. It asks first, saying that it removes the saved code from
iCloud Keychain on all her Apple devices, or on Android from this device only,
and doesn't delete the library, or anything on this device or in her storage;
then the list is read again, without the code. Her other Android phones keep
their own. The S3 key saved beside it goes with it. A delete that fails says so, keeps the
code listed, and deleting again finishes it. A later join, restore or reset on
one of her devices saves the code again
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

**A saved code bae can't read.** A saved entry can be one bae can't restore
from: saved in an older format or damaged, holding a different library's code
than the one it's saved under, or, in iCloud Keychain, saved more than once. The list
still shows it, as a row saying the code can't be used and why, with the
library's id beside the reason, since the entry has no name to show. Its menu
holds **Delete…** only, which asks and deletes as for any saved code; there is nothing to
restore from. One such entry never hides the codes beside it. When one of
Carol's devices opens the library, or applies a rename to it, it replaces the
entry with the library's current name and its own current code, so the row
becomes an ordinary one; a device whose keys another device replaced has no
current code to save, and leaves the entry for her to delete
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

The list is read from coven each time the first screen shows, and bae keeps
no copy of it. A list that can't be read says so in place of the list, rather
than looking like a device with no saved codes. An Android phone without
Google Play services has no Block Store and keeps no saved codes, so its first
screen lists none.

**The S3 key.** On S3 a restore code doesn't reach the S3 storage alone: the device
also needs Carol's S3 access key. No restore code holds it, whether typed or
scanned: the new device asks for the key
([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).

**Adding a device by pairing.** Carol adds a device from **Add device…** (on Android, **Add…**) on Your devices, under People and devices,
shown only for a library with storage. Pressing it starts a pairing, coven's *pairing code*, which lasts 30
minutes on her device's clock. Its sheet tells her to scan or enter the
code on the new device, over the QR code, with **Copy code** and **Share**
(**Share…** on a Mac), which hand out the code to type; the code's text is not
shown beside the QR code. On S3 the QR code carries the key her device
connects with and the code to type does not, so the new device asks for the key
only when she types the code. Every QR code bae shows (this one, an invite's,
the printed restore code) is drawn at the size its module count needs at the
measured minimum module size (3.43 points on a Mac, the same number in dp on
Android), so a phone camera can read it, and no screen picks a size of its own.

The new device's welcome takes the pairing code like any other code, scanned or
typed, and sends a request that carries the name the device gave itself. Its
screen says to check that the other device shows the number it shows, then to
add it there, over that six-digit number with "Waiting for your other device…"
under it; it checks every 5 seconds while that screen shows, has no countdown
and no way to check now, and, since its request may be sent, no Cancel. Leaving
the screen ends the pairing and removes the new device's copy. When the
pairing ends without adding the device, whether Carol declined, the 30 minutes
passed or her sheet closed, a step says "This code has ended" and to open Add
device on her other device for a new one, with **OK** back to the first screen;
a code already ended when it is entered ends the same way, before any sign-in.

While her sheet shows the code it follows the pairing. A request replaces the
code with the sheet "<name> wants to join" under "Add it if its screen shows
this code:", over the number the new device shows. **Decline** and **Add** (on
Android, **Decline** in the text colour and **Add** the one coloured button)
answer it. Once she adds it, the sheet says "Adding <name>…" until the device
is in, then says that device's name was added in place of the code, and **Done**
closes it. A pairing she declines closes the sheet. One that expires says "This code has
ended" and to choose Add device again for a new one, and one coven refuses
because the request failed a check says the request was refused and the device
was not added, each in the sheet with **Done**. Closing the sheet, however it
closes, ends the pairing, which coven then forgets. The sheet keeps showing
its last step until it closes; a
pairing coven has forgotten while the sheet follows it says "This code has
ended" as an expired one does. While the sheet follows it, Carol's
device syncs every 5 seconds instead of every 30, so the request reaches her
within seconds of being sent. A quiet sync is two requests to the storage,
about 24 a minute, only while the sheet is open
([coven §6.7](https://github.com/bae-fm/coven/blob/main/spec/coven.md#67-waiting-and-retrying)).

The restore code is shown on the Sync page alone: Settings keeps
the code there, in a **Restore code**
section, for the day she loses every device; the Library page has no row for
it. bae makes it from her keys, on this device, when Settings opens, and
names what it is doing meanwhile, never with a bare spinner: the sheet says it
is making a code for her new device, and the section that it is making her
restore code. It shows masked: its first two groups and its last, the groups
between as dots, the same groups in every app.
**Show** reveals it in
groups of four and **Hide** masks it again; **Copy** copies the whole code,
shown or not; **Print…** on a Mac and **Print** on Android print one page: the
QR code, the code in groups of four, where the library is stored, the date, and
how to use and keep the code. The page names no library and holds no S3 key,
and its QR code holds the code to type. Closing Settings drops the code. A
caption under it says to print it or keep it in a password manager, and
nothing about iCloud Keychain or a phone's backup: whether those hold it
depends on settings bae can't read. On S3 the section also says she will need
her access key and secret.

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
| The paths of a local release's originals, in coven's records (§4.2) | |
| A removed device's own writes, plays among them (§5.3) | |

Each reason is the same: the data is one device's, and another device would be
wrong to follow it. Ana's queue is what she is listening to, not Ben. A scan is
of one machine's disk. A fetched catalog release can be fetched by any device
that wants it. A transfer limit fits one machine's link.

A local release's rows sync like any release's, and so do coven's records of
its files, each naming the device it is on; only its audio, and the paths coven
records for its originals, stay on that device until the person moves it to
storage (§4.2).

### 6.2 Storage

A library's *storage* is where coven stores it: S3-compatible storage,
Dropbox, OneDrive, or iCloud on Apple devices. A library can
also have none and stay on one device, on a Mac. A phone joins a library and
never makes one, so a phone's library always has a cloud home: a phone never
shows a library as Not synced and never offers to set sync up. The baeium
edition offers S3 only.

Every kind of storage holds the same thing: encrypted objects under random names
(§4.5). coven's
[Part VI](https://github.com/bae-fm/coven/blob/main/spec/coven.md#part-vi-storage)
says what each provider needs.

**S3 setup.** Dan, on the baeium edition, sets up a library in S3 storage on
Cloudflare R2. The form asks for the endpoint, region and bucket, then the access key
ID and secret access key, and nothing else: no profile, no key prefix, as the
library takes all of the S3 storage. coven knows R2 by its endpoint's host and fills
in how to talk to it
([coven E.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#e3-s3-settings-and-profiles)).
When coven refuses a setup, bae says why in plain words, and the form stays as
typed. A region or endpoint that doesn't match the S3 storage shows its reason under
Region; S3 storage that isn't there, or that holds other files, under Bucket;
any other reason at the top of the form, above the fields. The reason
sits in its field's own row. A
refused key reads "This key was not accepted. Check the access key ID and secret
access key.", and a key the S3 storage doesn't allow reads "This key cannot use this
S3 storage. Give it read and write access, or use another key."

**Replacing the S3 key.** The Sync page of an S3 library, on the Mac and on
Android, shows the id of the access key the library records for the person,
which another of their devices may have replaced, in its Cloud home rows (the
secret never shows), with **Replace key…** (on Android, **Replace key**).
Replace key opens the S3 key sheet: *Enter a key that can use this library's S3
storage.* over Access key ID and Secret access key, with **Cancel** and
**Save**, which stays off until both fields hold text. The welcome's S3 key
sheet draws the same two fields. **Save** calls coven's one call for a new S3
key, which checks the key with S3 storage and replaces the one the library uses;
while it checks, the sheet says *Checking the key…* in place of the fields, with
only **Cancel**. A key S3 storage refuses keeps the typed text and says *Could
not use this key* / *S3 storage does not accept this access key.* above the
fields. Any other failure keeps the typed text too, and says *Could not use
this key* over why. **Cancel** while bae checks closes the sheet and stops the
check, and nothing changes; when the check had already passed, bae finishes
using the key and the page shows the line below. After a save the sheet
closes, the page shows the new key's id, and a line the person can dismiss
says *Saved the new S3 key.* / *Delete the old key, AKIA4EXAMPLEOLD, in your S3
provider's console.*, naming the old key's id.
When the library already records the typed key, because another of the
person's devices replaced it, the line says only *Saved the new S3 key.* bae
never deletes the old key and keeps no record of its id once the person
dismisses the line or the page closes ([coven
§14.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#143-s3-access-keys)).
When using the key fails after the sheet closed, because the person cancelled
once the check had passed or saved another key meanwhile, the library may
still record the key, so the page's line says *Could not use this key* over
why, until the person dismisses it.
The words for the place are always "S3 storage", never "bucket".

**Choosing where, on macOS.** *Set up sync…* opens a sheet that lists the
providers this build offers, in the order iCloud, Dropbox, OneDrive and
S3-compatible, one row each with a line saying what it is (iCloud
uses the Apple Account the Mac is signed in to; Dropbox, a shared folder in the
person's Dropbox; OneDrive, a folder in their OneDrive; S3, S3 storage they
manage), and choosing a row only selects it. *Continue* opens the selected
provider's screen: the S3 form, iCloud's one line, or, for Dropbox and OneDrive,
the browser's sign-in, which opens as that screen does and has no button of its
own to press. Every screen has *Cancel* and no Back: on the list it closes the
sheet, and on a provider's screen it goes back one screen, to the list. A
screen that waits says what it waits on, in place of the form: signing in to
the provider in the browser, with a line saying to finish there and come back,
reading the folders, or the setup's own step over its bar; only *Cancel*
stays under it. Setting up is the form's one action, *Set up*. A build that
offers one provider, as baeium offers S3, has no list: the sheet opens on that
provider's screen, and *Cancel* closes it.

Setup ends on the library's restore code, for every provider, because bae
can't tell whether the Mac's keychain carries the code anywhere else, and
losing the Mac could otherwise lose the library. Once the storage is connected
the sheet shows the code masked, as Settings does, with Show, Copy and Print…
and *Done*, which lands on the Sync page, whose Restore code section holds the
same code, which Settings keeps there alone.

**What setup shows while it runs.** Setting storage up, or reconnecting
it after signing in again, waits on the provider, at
most a minute of rate limits, and on a reconnect loading what other devices
published. So setup shows each step coven reports
([coven §20.8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#208-checking-a-provider-before-use)):
checking the location holds nothing but this library on S3, Dropbox and
OneDrive, detecting the settings on S3, preparing iCloud, reading the account
on Dropbox and OneDrive, connecting and checking the provider,
creating the library in the cloud home, or loading with loading's own steps, and
recording changed access. The apps show the step's name over a bar in place of
a spinner, counted for loading's steps. Signing in to Dropbox or OneDrive in
the browser, listing the provider's folders, and making a new folder (which
Dropbox also shares) each say so while they run, on both apps, the last naming
the folder. On macOS the check of a folder the person chose on Dropbox or
OneDrive names it too, "Checking the folder Music…". The dev reset reports
connecting storage as one step of its own.

**Cancelling, and a setup bae was stopped partway through.** coven journals
each setup before its first write to storage, and from its point of no return,
creating the library in storage or recording changed access, a setup finishes
rather than undoing ([coven
§11.6](https://github.com/bae-fm/coven/blob/main/spec/coven.md#116-every-call-that-writes-to-storage-in-steps)).
A setup sheet offers Cancel while a setup runs until it reaches that point;
once the setup is past it, the sheet hides Cancel and stays to show the setup
finishing. Cancel undoes the setup between its steps, not in the middle of a
request, so the sheet says "Cancelling…" in place of the step until the setup
ends as cancelled, which shows nothing; then the sheet shows the provider's
screen again as it was entered. Closing the setup's screen cancels its setup
the same way. A setup bae was stopped partway through is undone as the
library opens if it hadn't reached that point, and otherwise finishes once
sync starts, which the library does at launch for it as for a library with
storage set up. While a setup runs, the Sync page says "Setting up your cloud home…"
in place of any state that names no problem; a library with no location
recorded yet shows that where Settings would offer setting up sync, and
Settings shows the location once the setup ends. The Sync page shows every
storage setup that ended undone, why with *Dismiss*, until the person dismisses
it or starts another setup. Starting a setup while another runs, a call still under way or
one from before that sync finishes, is refused, saying to try again once it
has. A cancel counts from the moment the setup's call starts.

Every library has its storage location to itself: S3 storage, a Dropbox
shared folder or a OneDrive folder
([coven §20.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#203-where-a-store-lives)).
coven can't tell how far an S3 access key reaches, so it takes every key to
reach all of the S3 storage, and every member of a Dropbox or OneDrive library
reaches all of its folder: a member of one library could otherwise delete
another's objects. Setting a new library up refuses a location holding
anything but that library's own folder and coven's marker naming it, with a
line saying to use empty S3 storage or a folder only this library uses.
Reconnecting, restoring and joining read only the marker, and refuse a
location whose marker is missing or names another library.

**Dropbox and OneDrive setup.** The person signs in through the browser, then
picks the empty folder the library lives in, in one folder picker for both
providers: a line over the list, the folders to choose from (a chosen one is
filled in the accent), *New folder…*, *Look again*, and the folder setup found
holding other files. Only the provider's facts differ. On Dropbox the folder is a shared
folder, so the household can be invited to it: the list is flat, and a new
folder is made at the top of the person's Dropbox and shared. On OneDrive any
folder will do: the list shows where the person is, each folder opens into the
folders inside it, and a new folder is made in the one shown. *New folder…*
shows a name field in the list, its text aligned with the folder names above it,
and *Create* adds the new folder to the list and chooses it. A name already
taken there is refused with a line naming it, leaving the list as it was. Set up
needs a chosen folder, and checks it first: a folder holding anything returns to
the list, saying the folder isn't empty and to choose an empty one or make a new
one, and that folder isn't set up until another
is chosen. The picker moves into a folder only once its folders have loaded; a
listing that fails leaves it where it was, with the failure, and a top that
fails to list shows only that. On the Mac *New folder…* and *Look again*, which lists the
top again after a folder was made or shared elsewhere, sit at the left of the
sheet's footer, and *Up* at the right of the line that shows where the person
is. coven holds the sign-in; bae
never sees its tokens. bae's Dropbox app has access to the whole Dropbox, not an
app folder: an app folder can't be shared, so a household couldn't be invited to
it.

**The folder in Settings.** Every connection to a Dropbox shared folder or a
OneDrive folder reads the folder's name and web address as the signed-in
account names them, and coven records them with the location
([coven §20.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#202-the-providers-and-signing-in)).
Settings' Sync page shows that name as *Folder*, under the account, with
*Open in browser*, which opens the folder's page at the provider; when no app
on the device opens it, the section says so. S3 and iCloud show neither, and
nor does Dropbox's App folder, which has no label. A folder renamed at the
provider shows its new name in Settings as soon as a connection has recorded
it, on a Mac and on Android.

**iCloud setup** uses the Apple Account the device is signed in to, and keeps
the library in bae's iCloud container, `iCloud.fm.bae.desktop`.

A device can hold several libraries, each its own coven store, and remembers
which it last opened. coven keeps each store under `~/.bae/stores/`; bae keeps
a library's cache in `~/.bae/libraries/<library id>/`, and this device's
settings for it in its database (Appendix C). One library whose
storage settings can't be read never hides the others: the list carries each
library with its settings or why they can't be read, shows that reason in
place of the account, and offers no Open for it. A store whose name can't be read is listed by its id
instead of failing the listing, with no Open either. Launch and switching
libraries pass over it. A switch to the next or previous library opens the one
after or before the active one among those that open, wrapping at the ends,
and does nothing when no other library opens. The switcher shows when the
device holds more than one library, in any state. Switching keeps the open library until the next one
has opened; a switch whose open fails leaves the open library on screen with
the failure shown over it.

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
- **the columns that reference a coven file**
  ([coven §17.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#172-files-and-references)),
  each with its binding
  ([coven §17.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#174-bound-and-trailing-references)):
  `file` on `release_files` is trailing, so a write setting it goes out at
  once and other devices see the file on its uploading device until it is
  stored; `file` on `covers` and `artist_images` and `avatar_id` on `people`
  are bound, with coven's default limit of 1.5 MiB, so a write setting one
  goes out only once its file is stored, together with it (§4.1).

bae declares its four kinds of file when the store opens, each a coven
*namespace* with its kind and this device's cache budget (§4.3):
`release_files`, the person's own; `covers`, `artist_images` and `avatars`,
bae's own. Each file is downloaded when it is first read.

Appendix B lists every declaration. Every table bae creates is either
declared or listed as device-local, so a new table stays on one device only
when that was decided.

Two devices' edits merge cell by cell: an edit to a column the other left alone
survives, and of two edits to one column the later wins
([coven §8](https://github.com/bae-fm/coven/blob/main/spec/coven.md#8-merging)).

### 6.4 When sync stops

Sync runs in the background. A library opens and plays from its own copy
whether or not storage is reachable.

**The Activity button.** Sync takes one small control in the window's
title bar on macOS and in the library's top bar on Android, never a
bar across the content. While bytes move it is a spinner: files this device
is sending, or offline copies downloading. While a problem the person hasn't dismissed is
listed in the Activity panel, it shows a small warning symbol, beside the
spinner when both hold; with every problem dismissed it goes back to its usual look. While a
queue is held and no bytes move, it is a pause symbol in the spinner's place,
so the hold stays in reach: a queue is held while its line in the Activity
panel shows what holds it in place of its count, whatever holds it (Paused,
Waiting for network, Waiting for Wi-Fi, Retrying at its time). A held queue
with no line, no release waiting in it, doesn't show it. With none of these it
is absent. A pass alone doesn't spin it: an idle library runs one
every half minute, and the button would blink for nothing.

Clicking it opens the Activity panel, a popover on macOS and a sheet on the
phones, updating as it changes. It shows the same sync status as the Sync page
(below), and what the upload and download queues hold. What needs the person
comes first, under *Needs
attention*, one row each, a line between rows: the app's own problems (the
sign-in settings it can't read; on macOS a library that failed to open while
another stays open), each its title alone, saying what failed; a failure to read sync's state;
sync's failure, with *Open sync settings*, which closes the panel and opens the
Sync page. A file that can't move shows where the person can retry or cancel it, on
its release in Storage Manager, and a change that can't go out shows in its
change box, which offers *Dismiss* once the change has ended and no retry,
never in the panel. Nothing in the panel retries. A row shows no
error: clicking it closes the panel and opens the alert with what to do and the
causes (§9.6); on Android a tap closes the sheet and opens a dialog with the
same words and causes. Each problem has an × that takes it out of the panel,
without fixing it: it still shows where it is fixed, and a problem that leaves
the list and comes back shows again. While a library is open these problems show
only there. Then, under a *Transfers* label, one row per thing moving, each its
words, how far it has got and a bar, with no icon: Syncing changes, while
coven's sync state says Syncing (a pass receiving or sending, a file a write
waits for uploading, or the next pass's turn with nothing holding it), with no
count, so its bar moves; Uploading ("1 of 2 releases"); Pinning ("1 of 3
releases"), which on the phones opens the Pinning screen; and on the desktop
Moving to This Mac on a line of its own ("0 of 1 release"): a
move-to-this-device changes where files live for every device, while a pin only
keeps a copy on this one. Every count is in releases, a release done once all
its files are, and every bar is filled by the bytes of the batch done (for
uploads, the bytes the providers confirmed), each release counting its size: a
done release all of it, an upload its stored files and its files' shares on the
way, the pin or move-to-this-device downloading its share. Each bar moves as
its transfers move, and a finished file or release counts as done once bae
records it stored. A release
removed from the library counts none of its bytes. A bar moves while a release's
files transfer and its count waits. A batch is everything the queue has held
since it was last empty, so a count only grows, and it starts over once the
queue empties. Pinning and Moving to This Mac share the download queue but each
counts its own batch: the releases queued to pin, or to make local, since none
of that kind waited or downloaded. Uploading counts the releases with files
queued to upload; a cover or picture on its way counts in no line. A release
that leaves its batch without finishing, cancelled, failed or removed from the
library, stays in the count and never counts as done. A queue's hold (Paused,
Waiting for network, and for uploads Waiting for Wi-Fi or Retrying at its time)
stands in place of its count, as on the Storage manager's list (§4.2): the
upload line shows Paused while sync isn't running too. On the desktop each
queue's line ends in Pause, or Resume while the person paused that queue;
pinning and moving to This Mac are one queue, as in Transfers settings, so
their lines pause and resume together. Under the
lines, *Open Storage Manager* opens the Storage manager. On the phones the
Pinning line ends in Pause or Resume too, and opens the Pinning screen. With
nothing to show it says everything is up to date. The panel doesn't offer *Retry
now*: the Sync page's fix does.

**The Sync page.** Sync has a page of its own, off the Library page. Its top
is one block: *Status* with the state in words, then a line that says what
happened, then a second line that says what to do, then the button that does
it, each only when the state has one; on the Mac the button is a primary
button. On the Mac a state in a color (green, amber or red) has a dot before
its words, here and on the Library page's Sync row, and gray has none. *Last
synced* follows the button, in the same box on the Mac. *Last synced* shows
under a state that is a problem, amber or red, and never under Synced.
Under the block are the library's storage rows (§6.2).
coven gives the state
([coven §19.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#191-what-the-app-sees)),
and when several hold the first of these shows: Not synced, Removed from the
library, Device needs reset, New restore code, Update needed, Signed out, No
access, Folder missing, Clock is wrong, Cloud full,
Offline · unsent changes, Offline, Sync failed, Syncing…, Waiting to sync,
Synced. Mac and Android show the same words and
the same button for each state, except that a line or button that names the
device says *this Mac* on the Mac and *this device* on Android, and a state
only the Mac has. The lines below are the Mac's.

Every line that names the provider names the library's: the lines below use
Dropbox, and OneDrive and iCloud read the same with their own names. An S3
library reads *your S3 service* in place of the provider's name and *S3
storage* where the line names the place the library is kept.

- **Gray.**
  - Not synced, on the Mac only, since a phone joins a library and never
    makes one: *This library is only on this Mac.* / *Set up a cloud home to
    invite people and use it from your other devices.*, with *Set up a cloud
    home…*.
  - While a cloud home setup runs, *Setting up your cloud home…* takes the
    place of any state that is no problem (§6.2), on the Mac only: *Creating
    the library in Dropbox.* / *Your music stays on this Mac until you move it
    to the cloud.*
  - Syncing…, only while coven sends or receives changes or files: *Receiving
    changes from your other devices.* while a pass applies incoming changes, and
    *Sending your changes.* while a change goes out or waits next in line, with
    any cover, artist image or profile picture it names.
- **Green.** Synced, with a dot: *Your library is up to date.* Synced means
  data and files both: a file still uploading is Syncing….
- **Amber.**
  - Waiting to sync while the provider asked bae to wait: *Dropbox asked bae
    to slow down.* / *Resuming in 42 seconds.*, the seconds counting down.
  - Waiting to sync after a failure that clears by itself, a network error or
    a server error: *Syncing with Dropbox did not go through.* / *Retrying in
    18 seconds.*, the seconds counting down, with *Retry now*.
  - Waiting to sync while this device waits for another to save a fresh copy
    of the library: *This Mac needs a fresh copy of the library from your other
    devices, so nothing comes in or goes out.* / *Open bae on one of them so it
    can save one.*
  - A paused upload, or one held for the network, puts no state on the Sync
    page: Transfers shows it (§4.2).
  - Offline: *Syncing resumes when this Mac reconnects.*
  - Offline · unsent changes, while changes or files wait to go out, with no
    count: *Syncing resumes when this Mac reconnects.* / *Your changes stay here
    until then.*
  - New restore code, after another of the person's devices made a new
    restore code, naming that device: *You made the new code on your iPhone.* /
    *Enter the new restore code to restore the library on this Mac. The restore
    discards any changes this Mac has not sent.*, with *Enter restore code…*.
    Sync stays stopped until the person scans the code, and the scan restores
    this device as a new device that takes its place (below).
  - Update needed: *Another device uses a newer bae.* / *Update this one to
    keep syncing with it.*, with *Update…*, which opens bae's updater on the
    Mac and bae's page in the Play Store on Android.
  - Signed out of Dropbox, and of OneDrive with its name: *Your changes stay on
    this Mac until you sign in to Dropbox again.*, with *Sign in to Dropbox…*.
  - Signed out of iCloud, on the Mac only: *Your changes stay on this Mac until
    you sign in to iCloud in System Settings.*, with *Open System Settings…*.
  - Signed out of S3, while storage refuses the library's access key: *Your
    changes stay on this Mac.*, with *Sign in to S3…* (on Android, *Sign in to
    S3*), which opens the same S3 key sheet as
    *Replace key…* (§6.2). The page keeps showing the recorded key's id, and a
    saved key shows the same line about deleting the old one.
  - A sign-in that expired reads the same as none saved.
- **Red.**
  - Removed from the library, when an admin removed this device: *An admin
    removed this Mac from the library, so it no longer syncs.* / *To reconnect,
    add this Mac from one of your other devices.*; when an admin removed the
    person: *An admin removed you from the library, so this Mac no longer
    syncs.* / *To reconnect, ask an admin to re-add you.* Both have *Remove
    from this Mac…*.
  - Device needs reset, when a change from another device is damaged: *A
    change from another device arrived damaged, so this Mac cannot take in new
    changes.* / *Reset to reload the library from the cloud.*, with *Reset from
    cloud…*.
  - No access to the cloud: *This Mac can no longer reach the library in
    Dropbox.* / *Ask the person who shared it to give you access again.*
  - Cloud folder missing: *The library’s folder is gone from Dropbox.* /
    *Restore it from Dropbox’s deleted files to keep syncing.*, and for
    OneDrive *from OneDrive’s recycle bin*. An iCloud or S3 library says only
    *The library is gone from iCloud.* or *The library is gone from S3
    storage.*
  - Clock is wrong, for S3 only: *The storage service refuses this Mac’s
    requests because its clock is off.* / *Set the date and time automatically
    in System Settings.*; on Android, *in Settings*.
  - Cloud full: *Changes, new music and covers cannot upload until there is
    space in Dropbox.*, with *Open in browser* for a Dropbox or OneDrive library whose folder has a
    label to open, and *Open System Settings…* for iCloud. An S3 library reads *Your S3 service
    has no space left for this library.* / *Ask whoever manages it for more.*,
    with no button.
  - Sync failed, for any failure with no state of its own, always with a
    button, *Retry now* unless named otherwise, and bae retries by itself too:
    - the device has no space: *This Mac is out of space, so changes from your
      other devices cannot be saved.* / *Free some space.*
    - bae can't read its saved keys: on the Mac *bae cannot read its keys
      while the keychain is locked.* / *Unlock it to keep syncing.*; on
      Android *bae could not read its saved keys on this device.* / *Restart
      bae.*
    - the provider refused a request and retrying won't clear it, told as the
      provider told it: *Dropbox refused bae’s request (Invalid request).* /
      *If retrying does not help, restart bae.*
    - the library's database, the device's disk, decryption, an operation of
      coven's own, coven's copy of an image bae made (a cover, an artist
      image or an avatar) failing its check, or a bug in bae: *bae ran into a
      problem on this Mac and keeps trying.* / *If it does not clear, restart
      bae.*; on Android, *this device*
    - a start that failed with sync not running: *bae could not connect to
      Dropbox when it started.* / *Check the internet connection, then
      reconnect.*, with *Reconnect*.

A file that can't be uploaded, missing or changed since import, shows on its
release in the Storage manager, not on the Sync page.

The page never says "Sync stopped", "Checking sync…" or "Disconnect". Until
the status is first read it shows nothing in the block, and if it can't be
read the block says "Could not read sync status", with *bae could not read
this library’s sync status.* / *Restart bae.* The Library page's Sync row shows the state only, and the
Library row in Android's settings and the Library item in the Mac's settings
sidebar show it only while it is amber or red.

The Sync page lists no pending records: the page's status says what stops sync,
a file's upload, pin, download or move shows on its release in Storage Manager,
a change the person made (an invite, a role, a rename) shows in its change box
where it happens, and what only waits for coven to retry shows nowhere but the
status's Waiting to sync.

A failed start never shows beside the state that a later start or storage
setup brought.

**Signing in again to the recorded storage.** A device has no way to drop its
storage sign-in and stay in the library: removing the library from the device
is how it leaves. Its credentials can still go missing on their own, such as
a keychain entry deleted by hand, while coven keeps the location it recorded.
coven then reports the state Signed out, with no cause
([coven F.4](https://github.com/bae-fm/coven/blob/main/spec/coven.md#f4-opening-closing-and-sync)),
and the Sync page shows it as it shows any sign-out: the recorded storage's
rows and the fix that connects that location again, never other storage: a
new access key for its S3 storage, the provider's sign-in for Dropbox and
OneDrive, iCloud's own setup for iCloud. On a Mac, a library with no recorded location
is Not synced, and offers setting up sync instead.

The Sync page's cause names no file. A release's files upload as trailing
files, which never reach coven's sync state, so no state a pending record
names is about a release file: the line for a file that is missing, changed or
refused to bae says so without a name. While a library is
restored or joined, its waiting list names each record by what it is about,
such as an operation or the connection, and a file as a file, since no library
on this device names it yet. In the upload list a file's row already names it,
so its record there says only why it waits.

**Files another device hasn't uploaded.** A row references its coven file from
the write that stages it
([coven §17.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#173-staging-a-file)).
Ana's Mac imports an album to storage and uploads its files over the next
hour; her phone shows the album at once, each row referencing its file, which
coven records on the Mac only until the last of the album's files is stored
and one write switches them all to storage. Until then the phone can't read
the files, and can't tell an upload still running on the Mac from a release
the Mac keeps local: either way the release reads as on Ana's Mac (§4.2), and
a picture whose file isn't readable here shows a plain tile in its place, as
a picture still loading does, with no spinner. No
album or track is marked as on its way. No device waits for another's upload,
and a read of a file that fails leaves nothing pending, so no pending record
is about another device's upload.

Ben's laptop has two problems: Dropbox wants him to sign in again, which the
Sync page's status names, with a *Sign in* button, and a file of his vinyl rip
moved before it uploaded, which its release's row in Storage Manager names.
*Sign in*: bae signs in with the access the library's folder needs, then
connects that location again. On a library in
S3 storage, *Sign in* asks for a new access key for that S3 storage instead, the same
form Settings shows a device that lost its key, and connects the S3 storage again
with it. The release's row
offers to restore the file: put it back where it was imported from, and coven
retries the upload. A file that changed instead stops its release's move to
storage, and its row offers *Cancel upload*, which ends the move and
leaves every file of the release on this device only (§4.2). A pin the download queue couldn't
finish offers *Retry*, which queues the release's pin again, the failed entry
in its place or the release anew if it left the queue (§4.3). bae never asks for a storage
administrator's credentials: the person makes and deletes S3 access keys in the
provider's console, and no record asks them to delete one
([coven §14.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#143-s3-access-keys)).
A wait the person can't help, such as for another device to come online,
offers nothing.

- **A removed device.** When this device, or its person, was removed from the
  library, its sync state is Removed from the library, and the Activity panel
  lists sync's failure as "This device was removed from the library.", with
  its details behind the row. Resetting would
  register the device again under keys that no longer count, asking storage to
  take back a device the library removed. A phone whose join the library's
  history refused to admit, which only a faulty or hostile writer can cause,
  was never in the library: coven stops it for good, and bae shows it as a
  removed device, in the same words, with Removed from the library as its
  sync state
  ([coven §13.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#132-adding-a-person-invites)).
- **New restore code.** When another of the person's devices made a new
  restore code, this device stops syncing, and its sync state is New restore
  code, naming that device. No device can send the changes this device has
  not sent: coven refuses every write signed with the keys the new code
  replaced
  ([coven §12.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#123-replacing-a-members-keys)).
  So the state says, before the scan, that the restore discards them.
  *Enter restore code…* opens the restore screen, where the person pastes or scans
  the new code, on the Mac and on Android. Before any
  sign-in, coven checks that the code restores this library; another library's
  code, or an invite, changes nothing. coven then restores this device with the code: a new device
  loads the library beside this device's copy, takes its place once it has
  loaded, and starts without the changes the old copy had not sent
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
  A scan that fails before the new device registers, such as one of an older
  code or with storage out of reach, leaves the library as it was: the scan
  screen says why, an older code saying that the code from the other device
  restores the library, and the person can scan again. A restore that stops
  after the new device registers stays unfinished until the person finishes
  it (below).

"Synced" means coven finished a pass and nothing is pending. Until the status
has been read the line says it is checking rather than claim the library is
synced. A screen's first read of the library's own database lands at once,
so nothing stands in for it, neither a spinner nor a line, until its rows
arrive, and a failed read shows why in its place: the library's lists and
detail pages, search, downloads, the Storage manager and its inspector, an
import folder's draft and imported release, what is waiting, what sync is
doing, and who is in the library. A local action that ends at once shows no
spinner either, its control only disabled while it runs: deleting a saved
restore code, saving or refreshing the MCP and Subsonic servers, and the
invite form reading where the library is stored, whose access fields show once
it has. While Android removes a release's download, its control says so.

- **Launching offline.** A failed connect at start-up opens the library with
  sync not running, and the Sync page says why, with *Reconnect* as the fix,
  which starts sync again. Core offers it while a failed start leaves
  sync stopped with storage set up on this device, and the apps show it
  where core offers it: a running loop retries a failed pass by itself, and
  with no storage set up on this device there is nothing to start, so signing
  in again to the recorded storage is the way back instead. A start that breaks on a
  bug is reported as one and shows on the line the same way. Why the start
  failed is a row of this device's (`sync_start_failure`), kept until a
  start gets under way, whether a reconnect's or that of a storage setup such
  as signing in again, and emptied as the library opens, since no attempt
  outlives the process that made it. coven counts each start of sync in its
  sync status row, a setup's among them, and that count deletes the row in
  the transaction that starts sync, so the line never shows a failed start
  beside sync that runs, even when the new start's passes end the way the
  last one's did. The row holds the failure's class, and
  for a storage failure coven's reason with what that reason names (the
  access a sign-in lacks, a size and its limit), each in a column of its own,
  so the line words it as it words the same failure from a pass. A read
  whose bytes changed on their way to this device, which coven tells by two
  reads of one stored object disagreeing, is worded as data damaged on its
  way here, not in storage. A panic in coven's own storage work is a bug,
  kept as `internal`, never as a storage reason; so is it anywhere else bae
  words a storage failure, a pending write's or a pass's among them. A start
  fails only in the classes coven's sync errors map to, or as a bug; a class
  outside them is refused as a bug rather than kept.
- **The network coming back.** Each attempt that finds no network backs off
  longer, up to five minutes. While Ana's phone is locked and idle, Android's
  Doze mode blocks bae's network (App Standby does the same to an app left
  unused), so every pass and upload fails that way; once she unlocks it
  the network is back, but the next attempt would still be minutes away.
  coven watches the device's network itself while a library is open, and the
  apps report nothing: when the device gains a usable network or moves to
  another, Doze or App Standby ending among them, coven ends every wait whose
  last attempt got no answer from storage and runs a pass at once; a wait
  after storage answered, such as with a server error, keeps its time
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
  the retry ends, however long that takes; there is no timer. Under it, the
  Sync page says what sync is doing meanwhile. A pass already
  running when the person pulls doesn't count: it read the store before they
  asked, so the next one answers. If the retry didn't sync, the reason shows
  once it's known: on Android a snackbar, whose *Retry* asks again. The reason
  is the pass's failure, such as being offline; the provider asking every
  request to wait, with the date and time it ends, after which sync goes on by
  itself; or that sync isn't running. A pass held by such a wait, or by a
  failure only the person can clear, answers at once. The Sync
  page's *Retry now* waits for the same answer and shows a retry that didn't
  sync where it is on macOS, as a snackbar on Android.
- **A newer schema.** Data written by a newer app needs an update;
  reconnecting can't fix it.
- **Pausing** stops uploads of release files and keeps their place, across
  restarts too. A cover, artist image or avatar goes on uploading: the write
  naming it waits for it (§4.1).
- **A cover, artist image or avatar that can't upload** fails as a release
  file's upload does. bae never removes the image to let sync go on: a failure
  coven retries by itself waits and retries, and one only the person can fix,
  such as a full account or refused credentials, waits for them, its pending
  record showing it. Meanwhile coven holds the write naming the image, and
  every later write of this device, and sends them once it has stored the
  image. That image's pending record names the image as the file holding sync.
  coven sends a release file's row without waiting for its upload, so the
  upload holds no write and its record names no holding file.

**Resetting a device.** Opening can fail because bae can't use this device's
copy of the library's data, such as a damaged database file or tables that
don't match the app's, and sync can find that another installation replaced
this one
([coven §19.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#193-another-copy-of-the-same-device)),
which coven reports as the last pass's failure. There, and only there (a copy
behind another installation resets at once, below), bae
offers Reset from cloud: load the library from storage again under a fresh
device id. A failed open offers Reset from cloud only when the library has
storage to load the library from, beside the failure, where the app shows it
(§5.6). Before bae resets, a warning says
that bae will discard edits on this device that haven't synced. bae warned
the person before they chose, so bae shows nothing after; bae doesn't
present coven's reset notice. When sync
finds another installation, the screen says why in the middle of the window
and offers *Reset from cloud…*. Either way, *Reset from cloud…* opens an
alert (a dialog on Android), titled "Reset from cloud?", which gives the
warning, with *Reset* and *Cancel*. Leaving the
screen without resetting, by *Cancel* on a Mac or Back on Android, returns to
the open library, to the Mac's library list, or on a phone with no library
open, to onboarding. A reset that fails returns to that screen saying
"Could not reset from cloud", with *Retry*, which resets again without the
warning the person already accepted.

**A copy behind another installation resets at once.** When sync stops with
coven's reason *Device ID in use* (this copy found another installation of
its device, such as a restored backup or a cloned disk, using its device id,
and this copy is the one behind), bae asks nothing. bae then closes the open
library, which stops
playback for the few seconds the reset takes, and calls coven's reset, showing
the reset's steps on the screen a reset shows. The reset registers this copy as
a new device of the same member and replaces nothing, so the other
installation keeps its id and keeps syncing. When the library opens again the
app shows one notice: "Another installation of this device had newer changes,
so bae reset this device’s copy from the cloud.", then "bae did not keep N edits that had
not synced." when N is above zero, then "bae did not keep M files that had not
uploaded." when M is above zero, each with its own plural forms in every
language. The counts are the edits and files coven's sync state says this
copy had not sent, and with none unsent the app shows no notice. A reset
that fails returns to the screen that offers Reset from cloud, saying why. When
another installation replaced this device, or the member's keys changed, bae
still asks before it resets. The reset starts at most once for each open
library, and a library that closes first resets nothing and shows nothing.

**A reset or restore that stops partway.** A reset, or a restore with the
person's current code, loads the new copy beside this device's copy, records
itself, registers the new copy's device, then swaps it into place. If it
stops once it recorded itself, by a failure or when the app quits, the
library waits for it to finish
([coven §19.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#193-another-copy-of-the-same-device)).
bae still lists the library, under the name the new copy loaded, with the
call that began it, and opening it fails with that call's own line:
"Restoring this library on this device did not finish." or "Reset from cloud
did not finish." Coven keeps what finishing needs on
the device: a reset's device name, and a restore's code, its S3 key inside
the code, and its provider sign-in. So bae finishes either without the code
or a sign-in, and so does running the call that began it again. The other call is refused before anything changes: a reset would load
a restore's library under the keys the restore replaced, and a restore would
delete the copy a reset keeps.

The apps show such a library with that line, in red, where they list
libraries that can't open: the Mac's library list, and the library switcher
on phones. Its row offers *Finish restoring* or *Finish reset from cloud*, and that,
or opening the library, such as at launch, finishes it at once: finishing
needs nothing from the person, and the library can't open until it is done,
as a join under way goes on at launch (§5.6). A reset finishes on the screen
a reset shows, with no warning, which the person accepted when it began; if
it fails, the screen says why and *Finish reset from cloud* tries again. A restore
finishes on the screen that asks for the person's current restore code,
which shows how far it has come and offers *Cancel*. If it fails, the screen
says why and the code finishes it too, signing in to the provider again, as a
refused Dropbox or OneDrive sign-in needs. A reset under way can't be
cancelled, so finishing one offers no *Cancel*. Cancelling a restore before
its new device registered gives it up, and this device's copy is the library
again; after, coven keeps it, and the cancel shows "The restore cannot be
cancelled: this device was already added to the library. Finish restoring to
use the library." Going back leaves the library waiting and returns to the
open library, to the Mac's library list, or on a phone with no library open,
to onboarding.

**Lost values.** When two devices edit one cell and one edit is replaced
unseen, coven records a lost value
([coven §8.2](https://github.com/bae-fm/coven/blob/main/spec/coven.md#82-lost-values)).
bae shows none and dismisses none. coven keeps each one, and any file it names,
until an app dismisses it, so nothing is gone while bae doesn't show them.

**The keyring.** The library's encryption key lives in the system keyring. A
keyring that is locked right now, by a locked session or a sleeping display,
is told apart from a broken one: bae asks again after unlock instead of
reporting the library unusable.

**Backups of the phone.** Android's backup and its transfer to a new phone
leave out every library and its stores and the keyring: they are this
device's, and a copy elsewhere would be a second copy of it (coven's
[§19.3](https://github.com/bae-fm/coven/blob/main/spec/coven.md#193-another-copy-of-the-same-device)).
The restore code saved in Block Store goes with the backup, so a phone set up
from one lists it on its first screen (§5.6).

**Removing a library from a device.** Dan stops using his old laptop. Removing
Household from it first removes the laptop from the library, then deletes its
directory and the secrets the laptop keeps for it; the library stays in storage. Leaving
is an admin entry, so it needs storage. Offline, removal says so and waits
rather than deleting the local copy and leaving a device listed that will
never come back. A library that doesn't sync was never synced: it has no
members to leave and exists only on this device, so it is deleted at once, and
the Mac calls that **Delete**, never Remove. If the laptop has left but closing
its copy then fails, coven discards the closed copy without opening it, and the
removal goes on to delete bae's files for it. Only if that discard fails too does the removal
fail, saying the laptop left the library and its files are still on it.

- **The confirmation.** On a Mac, Remove asks "Remove “Household” from this
  Mac?", naming the library, and Delete asks "Delete “Household”?"; on Android
  Remove asks "Remove “Household” from this device?". Remove says
  "The library will remain in the cloud and on your other devices." Delete says
  the library isn't synced, so it will be deleted for good, and that the audio
  files stay in their folders. The first screens on a Mac and on Android ask this way,
  and neither platform's Settings removes the library. On a Mac, a library that
  doesn't sync offers *Delete* in place of *Remove*. Neither platform
  asks the person to unlock the device first: the confirmation is the only step.
  *Remove* loses nothing (the library stays in the cloud), so it is the
  confirmation's accent button, the Mac alert's default; only *Delete* is red.
  On Android's first screen no library is open, so a library that opens is
  opened to count what it hasn't sent and again to leave its store, its row
  showing each step under its name, with a spinner in place of the ⋮, and a
  library that can't open is removed without opening, as on a Mac. A removal that fails, or the count
  before it, shows an alert on a Mac and a dialog on Android, "Could not remove
  “Household”", with the error's line, *Retry* and *Cancel*: *Retry* counts and
  asks again, or runs again the removal the person confirmed, and the row goes
  back to its name; one that left the store but not this
  device's files goes on to the screen that finishes deleting them.
- **Unsent edits.** Dan's laptop still holds edits it hasn't sent. Before
  removing, bae asks coven what this device hasn't sent, and the confirmation
  says so: it leads with "Unsent changes will be lost". The library keeps running while
  bae asks, so *Cancel* changes nothing; for a library that isn't open, bae
  opens its store just long enough to count. If Dan confirms, the laptop
  leaves without sending them. Refusing instead would tie his removal to
  uploads that may be stuck; the confirmation lets him decide.
- **What the removal shows while it runs.** Leaving waits on storage, and
  deleting a large cache takes a while, so a removal reports each step as it
  starts: stopping
  the desktop's servers, playback, and identification and import, ending the
  library's background work, leaving the library, then deleting this device's
  files. For the open library the apps show the step's name on the screen they wait on. For a
  library that isn't open, its row shows the step's name, as words with no bar,
  under the library's name, with a spinner in place of the ⋯ or ⋮: the open's
  steps while it is opened to count, then the removal's, or the removal's from
  the start for one that can't open, whose removal reports leaving it and
  deleting its files.
- **A library that can't open.** A library whose settings or name can't be read, or
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
- **A leaving that stops partway.** Once the device's leaving is under way
  in storage, coven's delete only goes forward: if it fails there, as when
  storage refuses the answer, the library stays on the list as being
  removed, with "Removing this library from this device did not finish."
  under its name, and coven refuses to open it. Its row offers *Finish
  removing* in place of *Open*: coven opens the library and goes on as the
  removal would, publishing the leaving if it hadn't, and bae's files and
  the record follow as in any removal, its steps shown on the row as a
  removal's are. On Android, opening such a library, or a removal of the
  open library that stops this way, lands on a screen with that line and
  *Finish removing*. Opening it from anywhere else fails with that line. A
  restore or reset of it is refused the same way. *Remove* deletes it
  without leaving, as for any library that can't open.
- **Signed out of the library's storage.** A device that holds no
  credentials for the library's storage can't tell the library's other
  devices it left, so removing the library is refused before anything
  changes, saying "Sign in to this library's storage again, then remove
  it."
- **A removal that stops partway.** bae and coven can't commit together, so
  bae writes a record of the removal in its own directory before coven
  deletes the store, and deletes the record last. A removal that stopped is
  found from its record, never from bae files that coven's store list lacks:
  a library hidden from that list, by unreadable settings or a first restore
  or join cut off partway, still exists. At launch bae shows a recorded
  removal whose store coven no longer lists, with *Retry* when asking again
  can get past the failure (G.1), and opens no library while it shows.
  That library already left its store, so it offers no *Reset from cloud…*.
  *Retry* deletes only what is left on this device; it never tries to leave
  again. While it runs, it shows finishing each earlier removal as a step over
  a bar, counted by removal; until the first step, it names its work,
  removing the library, under a spinner. A record whose store coven still
  lists stopped before the delete: it is cleared, and bae says the library is
  still on this device, with nothing to retry.
- **The restore code stays.** The restore code and S3 key coven saved in iCloud
  Keychain are Dan's, not the laptop's, and every one of his Apple devices sees
  them. Removing Household from the laptop keeps them: his other devices still
  offer restoring it, and so does the laptop, now that it no longer holds the
  library (§5.6). They are deleted when Dan leaves the library, by one of his
  devices as it applies his removal, whether an admin removed him or he removed
  himself, or when he deletes the code from the iCloud Keychain list (§5.6)
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
  On an Android phone the code coven saved in Block Store is that phone's
  own, and removing a library from the phone keeps it too: the phone's first
  screen offers restoring it. It goes when Dan deletes it from that list, or
  when the phone applies his removal from the library.

---
## 7. Playback

Playing a record well means more than decoding files. An album should play
without gaps where it has none, stop where the record has to be turned over,
keep its hidden pregaps, and sound as loud as the next one. And a device should
play from its own disk, the cache or storage without the listener knowing.
This section covers the queue (§7.1–§7.2), how tracks join (§7.3–§7.4),
loudness (§7.5), where the audio comes from and goes to (§7.6–§7.7), and what a
device remembers (§7.8).

### 7.1 The queue

The queue has two parts: *Up next*, then what's *playing from*.

Ana plays her CD rip from its third track. The release becomes what she is
playing from, with the cursor on track 3. She then adds two tracks of Ben's
download to Up next:

```
now playing   Album Title, track 3          (from the release)
up next       Ben's track 1, Ben's track 5  (played first, then gone)
playing from  Album Title, tracks 4–11
```

When track 3 ends, Up next drains first, then the release continues at track
4. While the next track loads, the now-playing bar, the queue's now-playing
card, Android's full player and the system's media controls show it, with its
title, artists and cover, so the app shows a track at every moment of a track
change.
The speaker mark on an album's track list is on the track loading, and Go to
Now Playing opens its album.

What's playing from is a release, several releases in the order chosen, a
list of tracks in the order it was sent, or the whole library. The queue's
heading for it says which: "Playing from" and the album's title for one
release, "Playing from" alone for several, "These tracks" for a list, and
"Your library" for the library. The queue keeps
which of the four it is, and the release when it is one; which releases
several were, or which tracks a list named, is the tracks they filled. A play
of several of which only one has tracks plays that release. A list of tracks
plays from the track at the index it names, with the tracks before it behind
the cursor; it leaves out any track the library doesn't hold and plays a track
named twice twice. A start the list doesn't have, or a start track the library
doesn't hold, plays nothing. Albums picked together in the album grid, played, queued or
dragged to the queue, go in the grid's order under its sort and grouping,
whatever order they were picked in. Its rows are filled once, when playing starts; after that they
can be removed and reordered but never grow. A release named twice among
several plays twice. Removing the playing row leaves the track after it
next. Once its last row leaves, removed or its track deleted from the
library, nothing is playing from. Every entry, in Up next or what's playing
from, is its own, so the same track queued twice is two entries, each
removable, reorderable and skipped to on its own.

| Command | Effect |
|---|---|
| Play a release, from a track or shuffled | Up next is cleared; the release is what's playing from |
| Play a list of tracks, from one of them or shuffled | Up next is cleared; the list is what's playing from |
| Play next / add to queue | tracks go to the front or end of Up next |
| Remove, reorder, skip to | acts on one entry, within its own part |
| Clear up next | empties Up next only |
| Clear playing from | drops what's playing from; the current track plays on, then Up next, then playback stops |

bae offers *Shuffle library* while the library holds a track.

A spoken request to play something ("play" and a name, from a car's controls)
plays the first track the library search finds, else the first track of the
first album it finds that has a track on its main release. The search says
which, and nothing plays when it finds neither. Each page that Android Auto
shows of the albums, the composers, a composer's page, an album's tracks, a
work's children or the albums a search lists shows, after every write, exactly
the rows at the page's places, including after a write that moves a row across
the page's first place, and none for a page past the list's end.

Every play (a release, several, a list of tracks, the library shuffled)
answers whoever asked with what it did, and the app shows it in words unless
it played everything named:

| Answer | Meaning |
|---|---|
| Playing | everything named plays |
| Playing without releases | those of several releases that hold no tracks are left out; the outcome carries how many, and the line says it |
| Playing without tracks | those tracks of a given list the library doesn't hold are left out; the outcome carries how many, and the line says it. A play of tracks the person picked (§2.1) leaves out the ones the library lacks and says nothing of them |
| Nothing to play | what was named holds no tracks; the queue is as it was |
| Start track gone | the track to start from is no longer on the release or in the library, or the list has none at its index; the queue is as it was |

A reorder answers too, whether it moved its entry: one whose entry, or the
entry it goes before, is no longer in that part moves nothing and answers that
it moved nothing.
A remove, a skip to an entry and either clear answer too: a remove or skip
whose entry is no longer in its part changes nothing and answers that it is
done, and a failed write answers its error. So does every other queue command:
play next, add to queue, adding a release, insert, repeat and shuffle answer
once they are done or why they are not. The app shows a failed write's
error. When the queue can't say what plays next or can't record the move to
it (Next, Previous, a track ending, a side's pause ending, the playing track
deleted), playback stops with an error shown. It shows nothing for a command that changed nothing because the queue
changed: that is rare, and the screen already shows the queue as it now is,
so the gesture just ends.

When shuffle turns on or off, the queue keeps showing the rows of the old order
until the new order's count and first rows arrive, so the list never empties
between the two.

When the read of what's playing from's tracks fails, the rows it hasn't read
stay placeholders, and under them the queue says it couldn't read the rest,
with *Details* and *Retry*, which reads them again. Turning shuffle on or off
opens the list in the other order, and the new list shows no failure of the old
one.

On Android, the system's playlist (the notification, the lock screen and
Android Auto) is the current track, then Up Next from its first track, then
the tracks after the current one in the order of what's playing from, at most
100 in all, and it follows the queue as it changes. When bae can't read those
tracks, the queue screen shows its error in place of the rows until it can
read them again.

Dragging a row in the queue sends nothing until it is dropped: the drop is the
one reorder, however many rows the held row passed. While it is held, the Mac
draws a line over the gap where it will land and no row moves, as Mac lists
show a drop (no line over a gap that leaves it where it is); Android's held row
follows the finger and the other rows slide apart around an open slot where it
will land.

The queue screens keep a row dragged to a new place there until its move
returns, so it never jumps back while the move is on its way. When the move
returns, the part draws its rows, which show what is stored: the row in its new
place, or where it was when the move failed or moved nothing. A drop puts the
entry at the place among its part's rows where the person dropped it (for
what's playing from, among the tracks after the playing one), whether or not
the screen has shown the row it then sits before.

A track deleted from the library leaves every place it sits in the queue. A
deleted track that is playing stops, and playback goes on with what's next;
one loaded to play next is let go. This holds however the track was deleted.
From the delete until playback moves off the track, the now-playing bar, the
queue's now-playing card, Android's full player and home-screen widget, and
the system's media controls show the track as removed: they keep their place and the transport
keeps the state playback is in, while the title reads "Removed from
library", with no artists, no cover, and no album for Go to Now Playing to
open.

### 7.2 Repeat, shuffle, previous

**Repeat** is off, the whole of what's playing from, or the current track. The
button steps off, then everything, then the track. Commands set a mode; they
never toggle, so a repeated command lands in the same place.

**Shuffle** rearranges only what's still to come in what's playing from. The
current track and those before it don't move. Every row of what's playing from
has two places, one in its own order and one in the shuffled order, and the
mode says which the list reads in. Turning shuffle on gives the rows after the
current track new places in the shuffled order; turning it off changes only the
mode, so the whole list reads in its own order again with the current track
still current. Tracks played while shuffled that come after the current track
in that order are to come again. A row taken out while shuffled is gone from
both orders, and a row moved while shuffled is moved in both: it sits just
before the row it was put before, in the shuffled order and in its own.
Repeating a shuffled list shuffles it afresh for each pass.

Like repeat, shuffle is a mode of its own: with nothing playing, turning it on
or off records it, and the next play starts that way. While it is on, a play
shuffles what it plays, starting from the track it names when it names one. A
play asked shuffled turns it on. Clearing what's playing from leaves it as it
was.

**Previous** goes back one track within the first three seconds of a track,
and restarts the track after that. Playing an Up next entry, previous goes back
to the track of what's playing from that it interrupted.

### 7.3 Gapless, sides and discs

Tracks join without a gap: the next track is decoded ahead and the audio
output crosses to it within one buffer. This needs both tracks in one sample
rate and channel count; otherwise the output is rebuilt between them, with a
short silence.

There is an optional pause between sides or discs. Ben plays his 2×LP rip.
When side A ends, playback pauses and shows "Side A ended"; he presses Play,
or Next, to start side B. A CD box set pauses between discs the same way. On
the Mac the prompt is a card hanging from the top of the window with nothing
dimmed, and the now-playing bar keeps the artist under the title while it is
open; once it is closed, the bar's second line says "Side A ended".

- **When it pauses.** Only between two tracks of one release, on different
  sides, while playing that release in order. Up next entries and a shuffled
  list never pause: they aren't a record being played.
- **Side or disc.** A boundary is a change of medium or of side, and the
  current track's medium says what it is: a record or cassette pauses at a
  side, named by its letter; a CD at a disc, named by its number among the
  release's mediums. A medium with no physical carrier, a download or a disc
  of unstated carrier, never pauses.
- **The countdown.** The pause can end on its own after 5, 15, 30, 45 or 60
  seconds. Every device counts down to the same moment, so they show the same
  number. Closing the prompt stops the countdown and waits for Play.
- **Settings.** Pausing is on by default: a side break is part of how the
  record was made. Turning it on also catches a crossing already prepared.
  Playback settings show it as *Pause between sides and discs*, with
  *Resume automatically* (Never, or after 5, 15, 30, 45 or 60 seconds)
  joined under it with no divider: the two read as one setting, so neither
  needs a note. While pausing is off the row shows dimmed and does nothing.

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

A track's file is read through coven: from the original on this device for a
local release, imported here or moved to this device again (§4.4), moving to storage
or not, from the cache or a pin for an uploaded one, and otherwise fetched from
storage a range at a time
([coven §17.7](https://github.com/bae-fm/coven/blob/main/spec/coven.md#177-reading-a-file)).
Reading ahead keeps playback from waiting on the network, and a seek fetches
only what is under the new position. One buffer per file serves every track
that plays from it, so the eleven tracks of a disc image share one. The buffer
fetches a 4 MiB window at a time and drops what lies more than a window behind
the track furthest back in the file, keeping the file's start while the file
is open.

A track starts at its first sample, and a seek inside a track plays from the
sample asked for, in every format bae plays: bae uses where import recorded
each track's start, the file's own index where it has one, and the scrub
points import recorded (§3.7), so a seek fetches about one window instead of
searching across the file. The scan read every disc image, so import can
always read where each of its tracks starts; an image where it can't fails the
import, so no track is stored without its start. A seek bae can't place fails
the playback, as does a recorded position that can't be a start, and the
failure names the record: bae never plays from wherever a failed seek left
the reader. A track's load, or a load at a saved position, fetches first the
window its decode reads first.

The seek bar never leaves the player. Under the played part it draws, in a
lighter fill, the parts of the playing track whose bytes its file's buffer
holds, range by range: each byte range the buffer holds, cut to the track's
own bytes, is placed in the track by the track's first and last byte and its
scrub points (§3.7), with the bytes between two of them spread evenly over
the time between. The fill grows as windows arrive, before and after the
first sound; a track whose bytes are all held fills from its start to its
end. What the buffer drops behind the playhead leaves the fill too, under
the played part, so after a seek back past it the dropped stretch shows as a
gap until the fill fetches it again. A track a receiver plays (§7.7),
or one whose length is unknown, has no fill. Android's full player draws a taller
line with an upright thumb and no fill at all.

While the person drags a seek bar, on every platform and in every bar (the
now-playing bar, the full player, the import preview's), the position time it
shows follows the dragged position: the elapsed time, and the remaining time
where the bar counts down. The playing position, which keeps moving while
they drag, doesn't move the time, and neither the time nor the fill steps
back to it between the drop and the seek landing. On release the player
seeks there, the bar shows the seek's target from the next frame, and the
playing position follows from there. A seek that fails
ends playback with its error, as any failure of the player does (§7.6), and
the bar then shows no time, as it does when nothing plays: the dropped
position is never left standing.

While the person waits on a track to start, or on a seek to land, and one
of the playing track's reads fetches from storage, an arc turns around the
play/pause control. Reading ahead while the track plays, a preload of the
next track, and a load this device serves from its own copy or the cache show
no arc. The playing track's reads, in every file it plays from (a pregap at
the end of the file before included), go ahead of a preload's, which waits
while one of them is fetching. The control itself shows pause while a track loads to play, and a
press lands the load paused.

Offline, a track whose bytes aren't on the device stops playback with a typed
reason the UI shows. A decoder that stops producing for good is caught and
reported, rather than left behind a frozen position bar.

A panic in playback is a bug, reported to the crash reporter. One that
leaves the track unable to go on stops playback with an error the person
sees, rather than a frozen position bar, and the player serves the next
command. A preview that can't start (its file can't be read, or its audio stream won't
open) or that fails as it plays or as it is moved (a seek, or starting again on
a new device) ends with an error of its own, which the
screens show as they show a playback error. It never stops the main player: that
plays on, or gets back the resume the preview owed it.

### 7.7 Outputs

| Output | How it plays |
|---|---|
| This device | bae decodes and plays through the system output |
| AirPlay | bae decodes, and sends the audio to the receiver |
| Chromecast | the receiver fetches the track from bae over HTTP, driven by commands |
| DLNA renderers | the same, over UPnP |

Switching to a receiver keeps the queue and sends the current track at its
current position. Ending the session returns to this device, paused where it
was, at the volume and mute last set: there is one volume, whatever plays.
However a session on a Chromecast or UPnP receiver ends, the receiver is told
to stop once, unless the link to it is already lost, and bae then closes the
cast's connections to a Chromecast.
It runs from silent to full, and a level asked for past either is held to it
where it is set, so this device, the receiver, the stored row and every screen
hold the same level. A volume slider moves this device and the receiver as it
is dragged; the stored row and the other screens take the level the drag ends
on, in one write. The slider shows the level the drag ended on until that write
returns, and then shows the stored level. A UPnP renderer that describes no volume control can't take
a level: the set fails and is told to the person as a command the receiver
refused, not taken as done. Each receiver says whether it takes a volume: a
Chromecast and an AirPlay receiver do, a UPnP renderer does when its description
has a RenderingControl service. For one that does not, the now-playing bar on
macOS and the full player on Android show no mute and no slider, and say
"Volume is set on" the receiver's name in their place, so the refusal is never
reached from the screen.

**Fetching receivers.** A Chromecast or UPnP renderer fetches each track from
a small HTTP server bae runs for the session, on the local network, with a
random credential. A track stored as one whole file in a codec and container
the receiver plays is served as its own bytes, declared as the type of their
container: `audio/ogg` for an Opus file, `audio/wav` for PCM in a WAV file.
Both kinds play FLAC, MP3 and AAC files; a Chromecast also plays Opus and WAV
files, and neither plays AIFF. Anything else — a slice of a disc image, a
track whose pregap is in another file or generated, a codec or container it
doesn't play — is converted on the way: to MP3 at 320 kbps, or to WAV at its
stored depth, as the person chose. A UPnP renderer is never counted on to play
WAV or Opus, stored or converted, so it gets MP3. (DLNA's required audio format is uncompressed PCM sent as
`audio/L16`, not a WAV file; bae doesn't serve L16.)

bae talks to a Chromecast with a Cast client of its own. It answers the
receiver's keep-alive as it comes, and it pings a receiver that has sent
nothing for five seconds. A receiver that stays silent ten seconds after that
ping is lost, and the session ends as when the link drops. bae waits at most
ten seconds to connect to a Chromecast and start its media app.

A Chromecast's media app ending ends the session the same way. It can be
stopped from another device, replaced by another app, or have another sender
load its own media into it. Another sender's pause or play is followed, and
the session goes on.

A load or command the receiver refuses, or doesn't answer within ten seconds,
is reported and the session goes on.

A Chromecast tells bae of each change as it happens: a pause from its buttons
or another device, a track's end. bae doesn't ask it for its status after
connecting. A UPnP renderer is asked twice a second.

On a Chromecast or UPnP receiver, the position shown moves on from the last
status the receiver gave, at the speed it stated, and a play counts toward
history the moment the receiver's position reaches the threshold, not at the
next status. A receiver that states no speed bae can read keeps its position
where it was until its next status.

**AirPlay** receivers of both generations play. An AirPlay 2 receiver that asks
for no PIN or password pairs with the password-less transient pairing. A
receiver that asks for a PIN (on every cast, or once), a password, or AirPlay 1
device verification pairs when the person enters the code it asks for, as
described under Pairing below. bae refuses a receiver that offers only an
encryption it does not implement, with that reason, before it connects, and
refuses the same way a receiver whose AirPlay advertisement says it plays only
from people in its home, only from its home's admins, or only from its owner's
account; an AirPlay 1 receiver takes that from its AirPlay advertisement too.

A receiver that advertises on both AirPlay service types is one entry, whichever
advertisement arrives first. An AirPlay 1 receiver is connected at the port and
with the encryption its RAOP advertisement states; its AirPlay advertisement
states neither, so a receiver seen only there is not offered.

**Pairing.** bae pairs with an AirPlay receiver when the cast starts, before
playback moves. Every request bae sends an AirPlay receiver, to pair or to
play, gives the receiver this device's name as Your devices lists it. A
receiver that shows a PIN shows that name with it, and a receiver that asks
its owner to accept the cast, as a Mac's AirPlay Receiver does, names this
device in that prompt.
A receiver that asks for a PIN or a password does not fail the
cast: bae asks the person for the code in a sheet (macOS) or a dialog
(Android), and the cast goes on when the receiver accepts it. On macOS the
sheet sits on the main window, since the cast connecting is the app's. On
Android the dialog sits over the cast picker or the Casting page, whichever is
open, and otherwise over the app. Closing the picker or the page leaves it
open. Cancel closes it, casts nothing, and leaves the receiver showing its PIN
until the receiver ends it.

The sheet is titled Pair with and the receiver's name, with a line under the
title that says why it asks and what to enter, a field, Cancel and Pair.
A PIN field has one box for each digit, as many as the receiver's PIN has,
takes digits only, and keeps leading zeros.
A password field shows dots. Pair stays off until the PIN has all its digits, or
the password has a character. The lines under the title are:

- first pairing: "Enter the PIN it shows."
- a receiver that asks for a PIN every time: "Kitchen asks for a PIN every
  time. Enter the PIN that it shows."
- a receiver that no longer knows this device: "Kitchen no longer knows this
  device. Enter the PIN that it shows." A receiver that asks for a password
  says "Kitchen no longer knows this device. Enter the password set on it."
- a password receiver: "Enter the password set on it."

A wrong code keeps the sheet open, with the digits as typed, every box and the
line under them in red, and the line under the title changed to "Enter the PIN
it shows now." under "That PIN did not match." (a password says "That
password did not work." under its field), since a wrong PIN makes the receiver
show a new one. bae never tries again by itself. While bae pairs, the field is
off, "Pairing…" shows under it with a spinner, Pair is off and Cancel still
works. When the receiver answers a wrong code with a wait before the next
try, the sheet takes the next code at once, and bae sends it when the wait
ends, with "Pairing…" showing until then. bae refuses a code of the wrong
shape before it connects, with "Enter the 4-digit PIN." or "Enter the
password." under the field.

A receiver that refuses fails the cast with a line that says what is wrong and
what to do. macOS shows it in an alert titled "Could not cast to" and the
receiver's name, with OK, wherever the person is. Android shows it in the cast
picker while the picker is open, as a notice with the same title, and in a
snackbar otherwise. The lines are:

- locked after too many wrong PINs: "Kitchen refuses more PIN attempts. Reset
  it, then cast again."
- busy pairing with another device: "Kitchen is pairing with another device.
  Cast again when it is done."
- asking to wait a stated time before another cast: "Kitchen asks to wait 30
  seconds before another try. Cast again after that."
- not accepting new pairings: "Kitchen is not accepting new pairings. Allow
  pairing on it, then cast again."
- paired with as many devices as it allows: "Kitchen is paired with as many
  devices as it allows. Remove one in its settings, then cast again."
- plays only from people in its home: "Kitchen plays only from people in its
  home. Join that home to cast to it."
- plays only from the admins of its home: "Kitchen plays only from the admins
  of its home. Change who can play on it in its settings."
- plays only from the account that set it up: "Kitchen plays only from the
  account that set it up. Change who can play on it in its settings."
- a reply that doesn't check out: "Kitchen did not complete the pairing. Cast
  to it again."
- a receiver that can't be reached: "Kitchen did not answer. Check that it is
  on and on this network, then cast again."

A receiver that forgets this device while casting ends the session on this
device with the playback error "The AirPlay speaker no longer accepts this
device.": on macOS in an alert titled "Could not cast", with Pair again… and
Cancel, and on Android in a snackbar with Pair again…. Pair again… casts to the
speaker again, which opens the pairing sheet. A receiver that answers the first pairing step
with HTTP 470 asks for a PIN, and so does an AirPlay 1 receiver whose
advertisement asks for nothing but that answers bae's first request
(OPTIONS) with HTTP 403: bae asks for its PIN once and keeps the pairing.
A Mac whose AirPlay Receiver plays only for its
current user refuses to show a PIN, and bae says it plays only from the
account that set it up.

bae keeps its pairing with a receiver that asks for a PIN once (AirPlay 1
device verification included), or for a password, in this device's
keychain, one entry per receiver and kind of code, for every
library on the device: later casts to the receiver pair with it and ask
nothing. For an AirPlay 2 receiver with a password, bae keeps the password
beside the pairing, since the receiver asks for it again on each connection.
An AirPlay 1 receiver with a password has no pairing of its own, so bae keeps
its password in an entry of its own. One that verifies the device and has a
password asks for the PIN, then the password, and bae keeps each in its own
entry. When the receiver no longer accepts a kept pairing or password, bae asks
for the code with "no longer knows this device", and the new pairing replaces the kept one once the
receiver accepts it. A PIN that a receiver asks for on every cast covers every
connection bae makes to it during that cast, and bae keeps nothing of it
after. Once bae sends the code, the pairing and the keychain write run to
their end even when the cast is called off meanwhile, and a write the
keychain refuses fails the cast.

bae sends audio ahead of what the receiver plays by the delay the receiver
reports when the connection is set up, or two seconds when it reports none.
When the stream starts or resumes, bae sends at most 64 packets of audio
(about half a second, and never more than the delay) at once, because a Mac's
AirPlay Receiver drops the packets of a burst past about the 90th. From then
on bae sends audio at the pace the receiver plays it, so the receiver starts
playing that half second less than the delay after the start or resume. The
position shown and the play recorded in history follow what the receiver is
playing: until the receiver starts playing, the position stays where the
stream started or resumed, and after that it is the delay behind the audio bae
has sent. Each track ends the delay after bae sends the track's last audio.

The stream bae opens to a receiver, AirPlay's realtime one, carries 44.1 kHz
stereo, so bae converts every other track on the way out. (AirPlay's buffered
streams can carry 48 kHz 24-bit ALAC; bae doesn't open one.) A track bae
can't set up a conversion for doesn't play on the speaker, which says it can't
play the track's sample rate; it is never sent
unconverted. A conversion that fails partway stops playback with an error and
is reported as a bug; no audio is skipped silently.

bae keeps every AirPlay receiver's clock in step with its own over NTP: it
answers the receiver's timing requests from the moment it starts setting up
the session, since a receiver such as a Mac's AirPlay Receiver syncs its clock
before it accepts the stream, and it marks where the stream's audio falls in
time with a sync packet as the stream starts and one each second after. bae runs no
PTP clock, so it asks no receiver for PTP timing, even one that advertises PTP.

How to connect is read from what the receiver advertises, never guessed. An
AirPlay advertisement must state its features; a RAOP one without them states
none, and a receiver without status flags sets none, as older receivers do; a
RAOP receiver that states no encryption types takes an unencrypted stream. A
receiver whose features, status flags, `acl` or encryption types can't be read, or
whose AirPlay advertisement states no features, shows as one bae can't connect
to because it advertises settings bae can't read, even when its other
advertisement reads; the log names the setting and the text it carried.

A receiver that advertises as both AirPlay and RAOP is one receiver. Its id is its MAC
without colons, in capitals, whether it came from the AirPlay `deviceid` or the
MAC that begins the RAOP name, and it shows once, at the AirPlay 2 dialect when
it has one.

**Finding receivers.** Casting is off by default: it browses the local network
and serves audio from this machine. While on, devices are searched for only
while the picker or the Casting settings page is open, and on Android the page
counts as open only while the app is on screen. The two share one search: the
second to open joins it and keeps what it found, and it ends when the last of
them closes or casting is turned off. Desktops and Android browse the network
themselves.
The picker and the page list the receivers found, by name, after the receiver
playback is cast to, which stays listed while a new search hasn't found it
again. bae keeps what discovery found in a device-local table, which it empties
as the library opens, since no discovery outlives the process that ran it. A
receiver carries, besides its
id, name and kind, the model it states of itself and the address discovery
found it at. The model is the Cast `md` record, the AirPlay `model` record
(`am` on a RAOP service), or the UPnP description's `modelName`; a receiver that
states none has none. The address is the IP address a Cast or AirPlay service
resolved to, or the host a UPnP description was fetched from. Once the person
picks a receiver, the picker or the page names it while bae connects, until
the cast starts or fails. On macOS the cast connecting is the app's, not the
picker's or the page's: both name it, so does the Cast button's tooltip, and
closing either leaves it connecting; a cast that fails says why in an alert,
wherever the person is. A cast that connects closes the picker that started
it, never one opened after. On Android it is the open library's too: closing the
picker or the page leaves it connecting, both name it, and a cast that fails
says why in the picker while it is open and in a snackbar otherwise. A cast that
connects closes the picker that started it, never one opened after.
Stopping casting, turning it off, or picking another
receiver while one connects calls that connect off: it moves nothing, and
nothing says it failed. A connect to a Chromecast already under way runs to
its end and is closed, which leaves the receiver as an ended session does.

**The Casting page.** While casting is on, the page lists each receiver found,
with its name and kind: Chromecast, AirPlay or UPnP. They are the picker's
rows and work the same: picking one casts to it, and the one being cast to says
Casting in place of its kind. Each row ends in a ⋮ menu (⋯ on macOS) holding
Details…, and Disconnect on the receiver being cast to. Details is a sheet on
Android and a popover off the ⋯ on macOS, titled with the receiver's name and
showing its Type, its Model, left out when it states none, and its Address; it
shows no status, since the row already says Casting. The picker shows each
receiver once the same way, with no separate row for the cast under way, and
keeps Disconnect on the row of the receiver cast to, with no menu on any row.
Until the first answers it shows a spinner and says it is looking; there is no
end state, since the search runs for as long as the page is open. On Android, Settings opens Casting as a
page of its own.

A UPnP renderer is known by its UDN, so it stays the same entry when its
address changes. One that states no UDN is left out of the picker, and the
log names its address. A receiver with several zones lists each zone as a
renderer of its own, with its own name and controls.

**When casting breaks.** A panic in casting's own work is a bug, reported to
the crash reporter, and ends only what it was doing. A thread sending audio,
sync or timing to an AirPlay receiver that panics ends the session the way a
receiver that stops answering does: playback comes back to this device,
paused, and the picker says the receiver was lost. The server a Chromecast or
UPnP renderer fetches from that panics ends the cast, and the next cast starts
a new one. Connecting to a Chromecast that panics fails the cast as bae's own
error, not as an unreachable device. An advertisement or search answer whose
reading panics is left out of the picker, and browsing goes on. bae waits at most
ten seconds on an AirPlay receiver's control connection — to connect, send a
request, or hear back — so a receiver that stops answering without closing it
can't hold up ending the session.
A receiver that refuses a pause ends the session the same way, rather than
the app showing Paused while it plays on. A resume asks nothing of the
receiver: bae sends audio again, and its next sync packet tells the receiver
where that audio plays.

**This device.** On every platform, playback and a preview follow a change of
the default output device: they move to the new one at the same position. On
macOS and Windows bae starts them again on the new device, and on iOS it does
so when the system changes the route in a way the stream doesn't follow; a
paused one stays paused. On
Android bae reopens the output on the new device, and a change while paused
takes effect on resume. On Linux the sound server, PipeWire or PulseAudio,
moves them; with ALSA alone there is no default device to follow. On the
desktop, when the device goes away and there is none to move to, playback
stops and says the audio device isn't available, and a preview ends.
Unmuting restores the level last set; muting leaves the level shown where it
was, beside the mute.

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
disconnected or isn't available, it can't play the track's sample rate, the
AirPlay speaker didn't accept the connection, or the AirPlay speaker no longer
accepts this device. These are the device's or the speaker's doing. Any other
failure is bae's own: it shows as an error, is reported as a bug, and the log
keeps the whole error with its causes. When
this device's output won't open as playback starts, playback says why the
same way and ends: a later play or queue change fails with that reason.

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
ends or the session closes. A composer there lists its page's rows (§2.4), a
work opening its parts and releases and a credit opening its album. An album
lists its main release's tracks, in playing order. A work lists its parts, in
their order, then the releases that perform it, ordered as its page orders
them, each opening its album. After the controller asks for a page of an
album's or a work's list, bae tells it each time a row on the page changes and
each time the list's count changes, and reads the page again when the
controller asks for it again. A search there lists the first 50 albums library
search finds, in title order, and bae tells the controller of changes to a
page of them the same way. bae answers a request for a page only for the phrase that controller
searched for last, and refuses one for any other phrase.
A controller that sends one track plays its release from that track; one that
sends two or more plays those tracks, in the order sent, as a list of tracks
(§7.1).

**The home-screen widget.** Android's now-playing widget shows the track the
open library plays, with Play or Pause and Next, and a tap elsewhere opens the
app. The widget changes with each change of track or of playing, and shows
what the in-app player bar shows. With no
library open nothing plays, so the widget shows no track and offers nothing to
resume.

### 7.8 What a device remembers

The queue and position are per device. The queue is stored as it changes: Up
Next, what's playing from (a release, several, a list of tracks, or the
library) and its tracks
in play order — shuffled or not, with the session's removals and reorders —
which track of it is playing, and repeat and shuffle.
Beside it a device saves the entry playing and its position while a track is
loaded, the position never more than a second of playing behind, and keeps
what playback is doing in one row (§9.4), whose volume and mute a resumed
session comes back at.

A restart takes the queue up as it stood: the same order, the same history
before the playing track, and the saved entry loaded paused at its position.
Nothing is shuffled again. A deleted track leaves the stored queue with the
library; when it was the one playing, nothing is resumed. Desktop apps restore
on launch as a setting; off, the library opens with an empty queue and forgets
what it played. Mobile apps always restore, and save as they go to the
background.

A restart that can't read the saved entry, the queue or the volume resumes
nothing and says why as a playback error. It never makes up a volume: the
next play reads the saved one first, and a play whose read fails again stops
with that error instead of starting at full volume. Until playback saves a track of its own, it leaves the saved entry as it is, so the
failed restart doesn't write over the place the person was in.

---

## 8. Servers

Some listening happens outside bae: a Subsonic app on a phone bae has no app
for, or an agent tidying the import list. bae can serve both from a desktop.
Each server is off by default, and its secret lives in the keyring. A server
that stops on its own — a failure, or a bug, which is reported — shows as
failed with why, not as running.

### 8.1 Subsonic

The Subsonic server lets other apps browse and play the library.

Ben turns it on and sets a username and password. His phone's Subsonic client
connects to his laptop, lists the artists, opens an album and plays it.

- **What it serves.** Browsing by artist and album, album lists, one song,
  search, streaming, cover art (Appendix D). A Subsonic album is a bae release.
  bae has no folder tree, so it shows one folder holding everything. An album,
  and each of its songs, shows the release's year: its pressing's, else its
  album's.
- **Streaming.** A track that is one whole file is served as that file, with
  byte ranges, under the type of its container (a WAV file is `audio/wav`,
  whatever codec it holds); its song's `contentType` says the same. Any other
  track is decoded from its segments and served as WAV.
  A client that asks for a transcode gets one, MP3 at 128 kbps unless it says
  otherwise. A stream that fails partway — a file that can't be read, a decode
  or encode that fails, a bug — ends with an error, never as if the track
  ended there. A bug among them, such as a read of the track that panics, is
  reported as one.
- **Login.** Only token login is accepted: the client sends its username, a
  salt of at least six characters, and the MD5 of password plus salt. It doesn't run
  without both a username and a password. Failures come back as Subsonic
  error documents, which clients read, not HTTP errors; a request that hits a
  bug is reported as one and answered with a generic error document. The messages in those
  documents are always English: the request carries no language, so bae has
  none to translate them into, and clients act on the code.
- **Where it listens.** On this machine only by default. Available on
  offers this machine only, each network the machine is on (Wi-Fi, a VPN),
  by name and address, or every network. A network is kept as that network,
  not its address, so the server follows it to a new address, stops when it
  has none and starts when it has one again. A network with no address is
  offered only while it is the one chosen, marked not connected. The default
  port is 4533.
- **Settings.** The switch is what the person wants; the line under it is
  what is happening, as it changes: running, or not running and why, shown
  where it is fixed. While it runs, Available on shows the address an app
  connects to (`localhost` on this machine only; each network's own address,
  never `0.0.0.0`), as one line that can be copied. Its pop-up names the
  network alone when closed ("Wi-Fi") and with its address in its menu. Why it isn't running: a
  missing username or password at its field, a port another app holds at the
  port, a network with no address on the line itself, any other failure
  under the switch. The
  server starts as soon as nothing stands in its way; the switch stays on.
  Each field saves on its own when the person presses Return or leaves it; a
  port or network change restarts the server at its new address, and a
  username or password change applies to the next sign-in.
  On but not running, it also shows outside Settings: a mark on the
  Subsonic pane in Settings' sidebar, and a row under *Needs attention* in
  the Activity panel that opens this pane. Both read as a warning, or as
  danger when the server could not start for a reason of its own. The row's
  × takes it out of the panel until the server has run and stopped again.
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
token. A request that hits a bug is reported as one and answered with an
internal error: HTTP 500, or, for a tool call, the call's own internal error
in the response stream already under way.

On macOS, Settings' Automation page turns the server on or off and sets its port.
Its switch shows the new setting while bae saves it, and the stored setting once
the save returns, so a save that fails puts the switch back.

---
## 9. Across the apps

Every app does the same things with a library: open it, read it, watch it
change, import, play, move files, sync, and change settings. Every app
follows the same rules for each, which this section states.

### 9.1 Opening a library

bae keeps its own directory, `~/.bae` under the person's home, and registers
each library there by id.

Creating a library registers this device under the device name the person
gives it and gives the creator their profile, under the name they typed, in
the same step (§5.4). Given no library name, bae names the library itself. A
blank creator's name fails before anything is made.

Deleting a saved restore code (§5.6) deletes it, readable or not, with the S3
key beside it; no library, storage or device's copy changes, and deleting a
code that is already gone succeeds. The question the apps ask first names the
library of a readable code and names none for an unreadable one.

When the system keychain refuses a library's key for now, the Mac shows the
keychain screen and opens the library again once the person unlocks the
keychain, and Android shows the failed open with *Retry*.

**How far an open has come.** Opening a library can take a while on a large
one: after an app update it runs the migrations the library hasn't run, and
after a new version of SQLite or of one of bae's orders or functions it
rebuilds the indexes they changed. An open reports each of these steps as it
runs, with how much of it is done: first rebuilding, counted in rebuilds, then
migrating, counted in migrations, at most four times a second within a step
([coven E1](https://github.com/bae-fm/coven/blob/main/spec/api.md#e1-opening)).
An open with neither step reports nothing. Wherever such an open runs, the
apps show the step's name over a bar of how much of it is done in place of the
spinner: the screen a library opens or switches on, the restore over this
device's copy, and, on the Mac, the row of a library that isn't open while
removing it counts what it would lose. Between steps, and through an open that
runs none, the screen names what it waits on under its spinner, never a bare
spinner: finding this device's libraries at launch, opening the library, and
on the Mac setting one up, closing it, removing it, or resetting from cloud; a
reset before it first reports where it stands says "Resetting from cloud…".

**How far a setup has come.** Creating a library, a restore and a join each
report their steps the same way: a restore or a join reports where it stands
(connecting, waiting for approval with the confirmation number and when it
checks next, loading, or the records it waits on), and creating a library
reports its one step. The debug Mac app's dev reset reports finishing earlier
removals, removing each library on this device (counted by library), deleting
their saved restore codes (counted by code), deleting this device's pairings
with speakers, deleting bae's files on this device, creating the profile's
library, setting it up, and connecting it to its storage, and the first step
that fails stops it. The apps show the step's
name over a bar, never a bare spinner: the bar fills as the step's units are
done, and moves without a count for a step of no units and for each step of a
storage setup other than loading.

A restore, a join and a device reset load the library from storage, and say
where they stand state by state: a new installation's database opening, then
connecting, waiting for approval (a join's), loading, registering this device
(a restore's and a reset's: the admin's approval registered a joining device),
and finishing: keeping this device's keys and publishing the library, the last
work before it opens. A join reports loading for the check that finds its
approval, then finishes, then reports loading again for the library it
published
([coven E10](https://github.com/bae-fm/coven/blob/main/spec/api.md#e10-joining-and-restore)).
Loading reports its steps with how much of each is done: reading the library's
history for the person's keys, counted in slots, when the saved copy holds
none for them; downloading each saved copy, the one every member reads and
then each circle's, counted in bytes; writing the copies into the library
here, counted in tables; applying the changes saved with them, then
downloading and applying the changes made since, counted in changes. A step
runs only when it has work. The apps show each step's name over its bar on
the restore and join screens, and on the screen a reset loads on. Registering
and finishing have no count, since how much the registration reads isn't
known until it has read it: the apps name each as they name connecting, and a
join that has found its approval can no longer be cancelled. A restore can be
cancelled until it ends, a join while it opens or connects, and a reset never;
the apps show Cancel only while it can.

**Closing a library** stops playback, saving where it was, then the desktop's
servers, identification, import and sync, and then the library's store. A
close the app shows reports each step as it starts: stopping the desktop's
servers, stopping playback, stopping identification and import, stopping sync
once its running pass ends, ending the library's background work, and closing
the store. The apps show the step's name while it runs: on the Mac over a
moving bar on the loading screen, on Android over one on its own screen.
Before the first step, and between steps, both screens name what they wait
on, closing or removing the library, under a spinner. That covers closing a
library, the close before restoring or resetting this device's copy, and, on
Android, the close of the library a newly opened one replaces. Quitting the
Mac app, and the close after a removal or a failed open, show nothing. A step
of the close that breaks on a bug is reported as one and fails the close, and
the steps after it still run. A close the Mac app shows that fails ends the
library's session anyway, since the library is closed either way: the welcome
screen shows *Could not close the library* with the failure and says the
session ended anyway.

### 9.2 Reading and watching

Every screen shows the library as it stands, and changes as soon as a write
changes what it shows. A read that fails shows its failure, and the screen
goes on following the library, unless the library closed or can no longer be
read or written.

**Long lists.** The album grid, the artist and composer lists, a composer's
page, the download queue, the Storage manager, an artist picker's search, the
queue's upcoming tracks and the import list each sort by terms that end with
a unique one, so every row has one place. A list shows a placeholder at each
place whose row it hasn't read yet. When rows come or go above the rows on
screen, the list keeps the top row on screen where it was. When another order,
a reopen or a jump changes the list, the list keeps drawing the rows it has
until the new rows arrive, so the screen never empties.

The album grid draws an open album's detail under the row that holds its card:
the card the person opened, in its artist's group when the grid groups by
artist, else the album's first card, which the grid also uses once the
artist's group no longer holds the album. A write that moves the card, such as
a new title under a title sort, moves the detail with it. Revealing an album
opens it and scrolls to that card, and a write that moves the card while the
reveal waits sends the reveal to the card's new place.

The artist picker's search puts the artists whose id or name the text matches
exactly first, then those whose name starts with it, then the rest, by name
and id. Until the rows for new text arrive, the picker shows the rows the
previous text found; when the new text's first read fails, the picker shows
that failure instead of a list.

**The Storage manager's selection.** A click, a Command-click or an arrow key
changes the selection. Shift-click selects every release from one release to
another in the list's order, and Select All every release the list shows,
loaded or not. The app offers only the storage actions every selected release
allows. A row's menu acts on the row the person clicked, or on the selection
when that row is in it. The inspector shows, and the storage actions act on, a
selected release only while the list holds it: a release that finishes
uploading out of the Local list, no longer matches the filter, or is deleted
leaves the inspector and the actions' targets as it leaves the list, including
while an action's confirmation or folder prompt is open.

**The Storage manager's inspector** shows the release's files and its entries
in the download, export and upload queues, as they stand together. It shows
nothing of a whole queue, which the list's footer pauses (§4.2). Its upload
shows how many of its files are uploading, queued and stored, leaving out a
count of none, so a release with nothing being sent reads only what waits and
what is stored; a control that cancels it; and while files are sent, the bytes
of its size stored or sent and how fast they go. Each file is one line, its
name and size after a mark: none for a file at rest, a dashed circle for one
waiting its turn, a ring filling as it is sent; a file waiting on a reason
says why when the pointer rests on it. While the release pins, its download
shows the same way: how many of its files are pinning, waiting and pinned,
again leaving out a count of none; a control that cancels the pin; and while it
downloads, the bytes it holds of its size and how fast it goes. A file
downloading shows a ring filling as it comes, and one waiting its turn a
dashed circle. A failed pin says it failed instead, its error behind *Details*
(§9.6), with a control that retries it (§4.3) and one that cancels it.
Selecting a file shows its details under the list, closed by a button or by
selecting nothing: its name and the folder it sits in within the release's
folder, what kind of file it is, its audio format, and its size. A file's kind
comes from the role and document kind its import stored (§2.2): a CUE sheet,
parsed or not, a rip log, an AccurateRip report, a playlist, text, or a
damaged image; and from its content type, an audio file's or an image's
format, and a PDF.

### 9.3 Importing

The import list and the pane show the step each candidate's import is at, and
how far through it the import has got. The search form suggests the clues the
candidate's run is reading, else the clues stored for it. Two edits of one
candidate never leave its rows disagreeing about which rows belong together.

### 9.4 Playing

A preview's Pause and Play are each their own command, and a preview already
paused or playing stays so, so a pause from the lock screen or a media key
never starts a paused preview.

What playback is doing is one device-local row, `playback_now`: the playing
track, its phase and whether its load waits on storage, the volume, the mute,
where playback is going (a receiver by its id, name, kind, model, address and
whether it takes a volume), the preview, and why playback last failed, until
it next plays or the person dismisses the failure notice. A failure because an
AirPlay speaker no longer accepts this device names that speaker as a
receiver is named above, and Pair again… casts to it. Starting playback with the library resets
the row to stopped, keeping the volume and mute only when it resumes what it
played. A track the library no longer holds shows as a
removed track (§7.1).

The failure notice shows wherever the person is while the row holds a failure:
an alert on macOS, a snackbar on Android. A second failure while the notice
shows replaces the first, and the notice shows the latest. When the notice
ends (the person presses the alert's button or the snackbar's dismiss button,
or the snackbar times out), bae clears the row's failure, so a later failure
shows again, even one with the same reason as the one before it. When bae
can't record what playback is doing, playback stops with an internal error,
reported as a bug, so the row, the audio and the screens agree.

Each command that puts tracks into Up next (add to queue, play next, add a
release, insert) adds only the tracks the library holds, and the screen that
sent the command shows how many it added: the queue button's badge on macOS, a
snackbar on Android. When an add puts no tracks in, no screen shows a count,
and only the screen that sent an add shows its count.

The operating system's media controls and Media3 move the position on
themselves, from a line bae keeps in another device-local row,
`playback_timeline`: for the playing track and for a preview, a position and
whether it moves on from it at the play rate, and whether playback started the
track's line because it applied a seek, in which case the line's position is
the seek's target. bae writes a new line only as playback leaves the one it
holds (a seek, even one to where the line puts the track, another track or
clip, a pause or resume, a new length, the pregap ending, or a position more
than a second off the line), and starting playback with the library resets it
to no line. Those surfaces show no time before a track's start, so a playing
pregap is one line standing at the start, and the line that moves begins as
the track does. A seek the person makes through Media3 shows its target there
until playback writes a line for that track that it started at a seek. A
paused load, such as a restore, starts no line at a seek.

While a track plays here, the position shown moves each time the output has
played 8 ms of audio, one display frame at 120 Hz; while it plays on a
receiver, bae works out where the receiver has reached from its last status
every 8 ms. Past the track's stored length the position holds at the length.
When playback accepts a seek the person made, every view that draws the track
(the seek bar, the queue's strip) or the preview shows the target at once, and
no earlier position follows.

When playback ends on a bug of its own, or because the audio output won't open
as it starts, bae says so once: playback stops with why, on this device, with
no preview, and a bug goes to the crash reporter. The next command starts
playback afresh and is served, so the next press works, and nothing else
starts it: a device that can't open is tried once for each command, never in
a loop, and each failed try is told once. A command that can't be served fails
with "Something went wrong". Commands sent meanwhile are served in order.

Playback that starts afresh holds the queue with its repeat mode, the volume
and mute, and the saved entry, loaded paused at its position (§7.8), whether
or not the app restores on launch. Nothing plays until a command plays it, and
a next or previous from the loaded track stays paused. The rest ends with the
playback that ended: a session on a receiver, which is told to stop so it
doesn't play on while the app says stopped, a preview, a preloaded next track,
a side-pause countdown, and the side-end reason of a pause, which comes back
as a plain pause. A play under way keeps what was last saved of it.

### 9.5 Settings

This device's settings for the library are the `preferences` row. Each change
edits the stored row in one write, and every screen follows it with no
restart. Appendix C lists the settings.

On a Mac, Library settings starts with two rows. **Name** shows the library's name,
with *Rename…* for an admin, or for anyone when the library isn't synced, and
**Library folder** has *Show in Finder*. For a library with storage, two
rows follow, **Sync** and **People and devices**, each
opening its page and showing where it stands: Sync the state of the sync,
with its dot in the state's color, People and devices "2 people · 3
devices". A library with no storage has one row in place of those two,
**Sync** saying "Not synced", with a chevron, *Set up sync…* and a note that the library
is only on this Mac. Removing the library is not in Settings; it is on the
first screen's list (§6.4).

On Android, Settings has one **Library** row that opens the Library page. Its
card holds **Name**, with an edit mark and *Rename* for an admin, **Sync**
with the same state, and **People and devices**: a phone's library always has
storage. Each row opens its page. Settings keeps the switch to
another library on this device under the Library row.

### 9.6 Errors

Every failure has a category and a detail: the app shows one translated line
per category, and offers the detail, the error's chain of causes, untranslated,
to copy. A category says what the person can do: fix credentials, retry the
network, unlock the keyring, wait for an identification. Appendix G lists them.

An app offers the detail only when it says something the category's line
doesn't: a path, an underlying cause, the provider's or the network's own
message. A notice never shows the detail: a notice shows the category's line,
or a title the app words for what failed with what to do under it, and its
*Details* opens the causes one a line, led by the category's line when the
notice didn't show it, with a button that copies them whole: in an alert on
macOS and a dialog on Android. The Mac's alerts, these and every confirmation,
are bae's own and not the system's: no app icon, an elevated face with 20
point corners hung 36 points below the window's top edge, the title in bold,
the message small and secondary under it, and bae's buttons below. The alert
sits over the frontmost window, or over the sheet open on it, and nothing
below it can be reached while it is up, by pointer, keyboard or VoiceOver.
bae's alert draws in that window's tone, accent and light or dark appearance,
and follows them when they change while the alert is up. The first button
listed is the default, drawn in the accent unless it removes something, which
draws red; Return presses it and Escape presses Cancel. A category whose line
names the whole fault, with the counts, discs or folder it carries, shows the
line alone, since its detail is the same sentence again in English.

An app offers *Retry* on an error only when making the same call again can get
past it: a busy database, the network or a keychain locked this second can
pass; a damaged database, a bug, an edit to fix or a call only another call
answers can't. A database failure says which it met, from SQLite's or coven's
own answer: busy (another connection held it past SQLite's wait, the store
open in another process, coven's own work the call waits on, or what it read
changed under it), damaged (a page SQLite can't read, a file that isn't a
database, a stored value nothing writes), or refused (anything else).
Appendix G lists which categories offer it.

A failure while bringing a library up has a step as well as a category: bae
starting, opening secure storage, reading sign-in settings, listing this
device's libraries, or opening, resetting, closing or removing one. Each app
titles the failure by its step and says what to do next for that step, so a
keychain that won't open never reads as a library that failed to open; the
category's line and the detail are what its *Details* opens. A listing fails as a configuration to fix
when a folder or a file under it can't be read, as deleting fails when it
finishes a library's recorded deletion, and as a bug, "Something went wrong",
when coven's work for it panicked.

What launch finds wrong without stopping the app is kept apart from those
failures for the whole run: sign-in settings bae can't read. It shows beside
whatever opens or fails later, so a library that opens doesn't hide it and a
later failure doesn't replace it. On macOS, while a library is open, it shows
in the Activity panel (§6.4) and nowhere else in the window; on the welcome it
shows in the welcome's content. On Android, while a library is open it shows in the Activity sheet
and nowhere else on its screens; on its screens without an open library it
shows at their top. A keychain that won't open stops the launch, since
listing, opening and removing libraries all need it. On Linux the keychain is
the desktop's secret service.

---

## 11. Crates and conventions

### 11.1 Crates

| Crate | Holds |
|---|---|
| bae-app | the composition root: the bootstrap that opens a library and builds every long-lived part of it once, `AppServices`, the one owner of the opened library every surface calls, which passes each call on to the part that answers it, and the live reads the screens pull over the library and its views |
| bae-library | the library, files, outputs, sync integration and settings: the library manager and the long-lived parts it drives, and the library's schema, assembled from each crate's parts of it (a phone's without import's) |
| bae-views | the library's views: what the album, artist and composer lists, the detail views, library search, the artist picker, the Storage manager and its inspector, the sync activity and the open invites read, as live queries over the store, and the shapes a screen shows them in, with the tables those lists read, kept by triggers in the write that changes what they come from |
| bae-import | import, desktop only: watched folders and their scans, the candidates they hold, the drafts a person shapes and the pane's session over them, identification runs against the catalogs and the queue that admits them, cue sheets, loudness measurement, and the import that writes a candidate's release into the store, with import's own tables and the watched folders' paths as the store keys them |
| bae-playback | playback: the service that plays the queue on this device's output or a remote renderer, the queue, its resume point and play history as the store keeps them, the outputs it plays on, the preview player, and the renderer session a connected cast or UPnP device is driven through |
| bae-identify | identification, desktop only: the reducer that asks every key a candidate's signals put in effect and ranks what the catalogs return, the release groups and pressing evidence it ranks by, the view a surface shows of a run, and the verdict a finished run stores |
| bae-track-audio | reading a track's stored audio for a decode, for playback, a save or export, import's loudness pass and a transcode: the readers that fill a sparse buffer from a release file or a local path, what of the buffers a decode reads, the stretch of a file a preview plays, and why a read failed |
| bae-preferences | this device's preferences for a library: the `preferences` row and its save presets, the values they hold, which catalogs a lookup asks, and the transfer concurrency the setters accept |
| bae-transfers | the transfer queues: the download queue that keeps releases on this device or makes them local and the export and save queue, each worked through one release at a time, paused, cancelled and retried, their entries as this device's tables hold them, each ended in the write that does its work, and the writes that move a selection's files to the cloud or out of the pinned cache |
| bae-store | the library's store over coven, with no table: the `Store` every read, write and live query runs through, the coven calls bae makes on a library (storage setup, sync, keys, files, pins, uploads, invites, restore codes), the schema's shape (each crate's parts, run in crate order as one migration) and the SQL functions and collation the parts declare, list windows (each the rows from one key of a list's order on, at most a limit of them, read as a run of index seeks, with the count of the rows before a key and the key at a position), the cancellable live queries a surface pulls values from and the live reads over them, whose request the caller changes on the same query, each request saying whether a read that finished after it was replaced still answers (a window's rows do; another id's detail does not), and the names a person enters |
| bae-records | the library's records and where each release's files are: artists, albums, releases, tracks, works and labels with their files, the rules that resolve credits and labels and merge identities, the row writers an import and an edit call inside their own writes, release storage, and the language this device orders names in, with the `Records` owner over them |
| bae-people | the library's members and each person's own data: who is in the library and on which devices, the admin changes this device asked for, profiles, plays, ratings and playlists, with the `Membership` and `People` owners |
| bae-sync-status | the sync status as the Sync page and the Activity panel show it: coven's sync state, the release file its failure is about, what a retry would do, why this device's last start failed and where its storage setup stands, with the `SyncStatus` owner |
| bae-app-dir | bae's own directory and what it holds before any library opens: the libraries registered on this device and the one last opened, the installation id, the keyring namespace, and the transfer limits a library runs under before its preferences can be read |
| bae-diagnostics | how a failure is told: the closed catalog of telemetry events and their upload, the bugs core hands the platform's crash reporter, the panic hook that reports each panic once, the tasks and threads that end what a panicking one served, and the error categories a person is shown |
| bae-catalogs | the MusicBrainz, Discogs and Wikidata clients and the rate their requests are admitted at, the catalogs bae knows and the pages they publish, and the covers they offer with the cache their images are read through, which reports a panic in its own disk work; what the catalogs say about a release in bae's terms: searches and lookups, the documents a release is read from and what they state, each catalog's release mapped onto bae's values, and why a lookup failed |
| bae-audio | the audio codec over FFmpeg (decode, encode, probe, resample), the sparse buffer a decode reads a file's bytes from, and the stream a decoded track plays from |
| bae-types | the domain values the catalogs, identification and the library share: what a pressing is, how two pieces of written text name one thing, and how a barcode is read |
| bae-util | what the other crates build on, with no library vocabulary and nothing else in bae beneath it: content types, duration labels, outbound HTTP and its retries, error text, the natural order of names and the folding library search compares text by, stored images, text encodings, the embedded-server lifecycle, the log subscriber every app installs |
| bae-bridge | the uniffi exports for the apps |
| bae-desktop | the desktop services a bridge handle starts: the MCP and Subsonic servers |
| bae-subsonic | the Subsonic server |
| bae-mcp | the MCP server over bae-automation |
| bae-automation | the automation tools and their JSON shapes |
| bae-cast | the AirPlay, Google Cast and UPnP protocols, receiver discovery, the session media server, and its URLs |
| bae-mirror | the macros that write field-for-field conversions |
| bae-loc | the string catalog: checks it and writes each platform's resources |
| bae-theme | the shared theme, written as Swift and Kotlin |
| bae-uniffi-bindgen | the bindings generator |

bae-cast stands between bae-playback, which holds the renderer session, and
bae-subsonic, which serves the audio, because neither may depend on the other.
Nothing in bae-library or bae-playback depends on a server crate. bae-playback
holds no device protocol: playback drives a fetching receiver through a command
channel and hears its status as the receiver sends it (Cast) or asks for it
(UPnP), and drives an AirPlay receiver through a sink it hands decoded audio to,
and bae-cast connects each device and hands playback the channel or sink.

### 11.2 Languages, telemetry and failures

- **bae is lowercase** in everything a person reads.
- **Every string is translated.** Every string a person reads is translated
  into every shipped locale in the change that adds it.
- **Locales.** bae is written in English and ships 27 other locales: German,
  French, Spanish, Italian, Dutch, Polish, Czech, Slovak, Hungarian, Romanian,
  Greek, Bulgarian, Croatian, Ukrainian, Turkish, Hebrew, Arabic, Brazilian
  and European Portuguese, Swedish, Danish, Norwegian Bokmål, Finnish,
  Japanese, Korean, and Simplified and Traditional Chinese. They are the
  languages of the markets where people use desktop apps and expect them in
  their own language.
- **Telemetry is typed.** What leaves a device is a closed catalog of events
  with typed fields. A string can't be a field, so a name, path or title can't
  leak. Events from one installation share an id bae makes once and keeps in
  `~/.bae/install-id`, never coven's device id, and each launch adds an id of
  its own. The baeium edition sends none.
- **Errors keep their cause.** A failure reaches the person who can act on it
  with its reason, and bae never puts a default in place of an error. A logged
  error keeps its whole chain of causes and its stack.
- **Bugs reach the crash reporter.** A bug bae finds, code that broke its own
  guarantee, is logged with its causes and handed to the platform's crash
  reporter as a non-fatal event naming the operation. A panic is such a bug,
  on whatever thread it happens, and bae reports each panic once, with its
  backtrace, the operation it interrupted, or "work outside every catch", and
  the file, line and column where it began.
- **A panic ends what its work served.** The operation that asked for the work
  fails with "Something went wrong", never left waiting, and the failure is not
  reported again. A later call that waits on a service a panic ended fails the
  same way. Every call on a library an earlier bug failed shows "Something went
  wrong" until the library reopens.
- **Logs stay in the platform's log.** bae's log lines go to the platform's own
  log on the device: the unified log on macOS and iOS, logcat on Android, ETW
  on Windows, the systemd journal on Linux. A development (Debug) build of the
  macOS or iOS app has the unified log save every level of its lines; a
  release keeps the system's defaults, which save default, error and fault
  lines and drop the rest, so bae doesn't fill a person's log with its detail.
  A desktop app run from a terminal also prints its lines there.

---
## Appendix A. Schema

The schema is one migration, which coven applies as migration 1: each
domain's part of it, in order, each part running after the parts creating the
tables its triggers fire on. Every table is `STRICT`. "Syncs" means coven
syncs its rows (Appendix B); every other table stays on the device.
A phone's library holds no import tables: they never sync (§6.1), and a phone
imports nothing (§1.1). A phone's library holds the catalog image cache's
table, which stays empty there (§4.7).

A list is stored as rows, one per item, when its items have an identity of
their own, when a query filters, sorts, joins, counts or indexes them, or when
they are edited one at a time: a release's labels, barcodes and media are
rows, and so are a failed verdict's lookup failures. A list that is only ever set whole
and read whole is one JSON value in one column: a release's Discogs details,
a draft's media, a queued save's preset. coven merges a synced row cell by
cell, so a synced JSON list is one cell, set whole by its editor.

bae gives coven the collation `natural_order`, which coven registers on every
connection to a library: it orders text the way a person reads folder names
("Disc 2" before "Disc 10", case aside, then by bytes, so two different
strings never tie). It gives the same answer on every device, and it carries
a version, 1: a change to what it compares bumps the version, and coven
rebuilds the indexes that use it when the library next opens for writing.

It also gives coven the function `bae_catalog_rank`, a catalog's place in the
order surfaces list catalogs (MusicBrainz, Discogs, then the rest by name),
which indexes order records by, and bae's other SQL functions; each is
deterministic, with a version that works the same way. Each part of the
schema declares the functions and the collation its SQL names.

### A.1 The library (bae-records)

All sync.

| Table | One row per |
|---|---|
| `artists` | artist: name, folded name, sort name, MusicBrainz and Discogs ids |
| `artist_credits` | another name a credit prints for an artist, keyed by the artist and the folded name (§2.3) |
| `artist_merges` | artist absorbed into another (§2.3); the view `merged_artist_survivors` follows each chain to the artist it shows as |
| `labels` | record label: name, name key, MusicBrainz and Discogs ids (§2.3) |
| `label_merges` | label absorbed into another (§2.3); the view `merged_label_survivors` follows each chain to the label it shows as, for the triggers that fill `label_shown_as` |
| `artist_images` | artist's picture, keyed by the artist: the coven file it references (`file`), with an index by it, and its type |
| `albums` | album: title, year, compilation flag |
| `album_artists` | album's credit, one per place in its list, from position 0: its artist, the other name it prints (none: the artist's own) and what follows it |
| `works` | MusicBrainz work |
| `work_artists` | work's credited artist, in order, with the role it holds (a key of bae's list, or `stated` with the catalog's words) and its qualifiers, and the other name it prints (none: the artist's own) |
| `work_parts` | work inside a larger work, in order |
| `releases` | pressing: its album and place in the album's order (§2.1), name, year, country or region, status, packaging, Discogs details, source folder name (its text, escaped as a file's name is, §3.1; never empty), album loudness and peak |
| `release_labels` | a release's labels: each at its position in the list, the library label it names and the catalog number it gives the pressing, either or both (§2.1) |
| `release_barcodes` | a release's barcodes: each at its position in the list (§2.1) |
| `release_media` | a release's media: each carrier with its count, at its position in the list (§2.1) |
| `covers` | release's cover, keyed by the release: the coven file it references (`file`), with an index by it, and its type |
| `release_files` | file of a release: name (its path in the release, each byte of it that isn't UTF-8 escaped, §3.1; none of its `/`-separated names empty), type, the role the scan gave it (audio, and only audio, stating audio facts) and, for a document and only a document, its kind, the audio facts the scan read, and the coven file it references (`file`, required), whose record holds its size and where it is, with an index by it; with an index of each release's files in `natural_order` of their names, id breaking a tie, the order every list of a release's files shows, read from the index; and by content type, name and id breaking a tie, another order a release's file table sorts by, read from its index forward or backward. Its order by size sorts the release's files by their coven files' sizes |
| `release_artist_roles` | credit on a release, with the role it holds (a key of bae's list, `instrument` with an instrument's id, `voice` with a voice's id, `performer` with the catalog's words for its part, or `stated` with the catalog's words) and its qualifiers, and the other name it prints (none: the artist's own) |
| `release_records` | catalog record of a release: pressing or album, key, album key; its URL is made from these as it is read; with an index of each release's records in the order surfaces list catalogs, which the release reads them in |
| `release_mediums` | a release's medium: its position, carrier and number of sides (§2.2) |
| `tracks` | track: title, place (its medium, side and number, all required), length (required: the import measures every track's audio), playing order |
| `track_artists` | track's own credit, one per place in its list, from position 0, as an album's |
| `track_artist_roles` | credit on a track, with its role, qualifiers and printed name as a release's |
| `track_works` | work a track performs, in order |
| `audio_formats` | track's format, pregap, loudness and peak |
| `audio_format_segments` | span of a file a track reads: pregap or main, samples and bytes; one main and at most one pregap per format, each kept to one by a unique index |
| `audio_segment_scrub_points` | frames a seek inside a segment can start its decode at, one row per segment that has any, keyed by the segment: each frame's first sample, its byte, and the first sample a decode from it plays exactly, packed in sample order in one blob; for a FLAC whose seektable is missing or sparser than the points' 10-second spacing, and for every MP3 (§3.7) |

Beside them, device-local tables:

| Table | One row per |
|---|---|
| `artist_shown_as` | artist, keyed by the artist: the artist it shows as (§2.3) |
| `label_shown_as` | label, keyed by the label: the label it shows as (§2.3) |
| `release_storage` | release, keyed by the release: its files' count and size, whether they are uploaded, how many of them this device keeps pinned and whole, how many wait in this device's upload queue and the place of the first, the name of the file whose original failed a move of it and the name of the file whose missing original a move of it waits for, each from the move's pending record, and the other device that the album and release views name as holding its audio (§4.2), with an index of the releases uploading in queue order (§3.2) |
| `name_sort` | one row: the language this device orders names in (`und`, the root order, until the app sets one) and the version of the collation data every stored name key was written with (§2.1) |

What the library lists, bae-views' part, kept by its triggers:

| Table | One row per |
|---|---|
| `composer_work` | work and artist shown credited on it |
| `composer_release` | release and artist shown credited on a work one of its tracks performs |
| `composer_unlinked_credit` | catalog role credit none of its artist's works links (§2.4), keyed by the credit: the artist it shows as and its release |
| `composer_listing` | artist the composer list shows (§2.4): its name, the name's sort key and its counts of works, releases and unlinked credits, with an index for each of the list's sorts |
| `composer_detail_row` | row of a composer's page (§2.4): a credited work under a work it is part of, a work heading a group, or an unlinked credit under its release, with what the page sorts it by, the sort key of the heading work's title or the release's album title, album and place, and a work's title sort key, and an index of the page's order |
| `album_listing` | album the grid shows (§2.1), keyed by the album: its title's sort key, year, whether it has no year and date added, and its credits' sort names' sort key and ids in credit order, with an index for each of the grid's sorts; library search (§2.3) reads the albums it finds in title order from the title index and stops at its limit |
| `album_artist_link` | album the grid shows and artist its credits show: the artist's sort key from its `artist_listing` row, and the album's title key, year, whether it has no year and date added from its `album_listing` row, with an index for each way of the artists with each sort of their albums, so the grid grouped by artist orders by this table's columns alone, and an index of an artist's page's order: year, an unknown year after every year, then title, then id |
| `artist_listing` | artist the artist list shows (§2.1): its name, the sort keys of its name and of its sort name (else name) and how many shown albums credit it, with an index for each of the list's sorts and the sections of the grid grouped by artist |
| `release_listing` | release, keyed by the release: what the Storage manager (§4.2) sorts and filters it by — its album's title key and credit key, its media, when it was added, and from its `release_storage` row its files' count and size, whether they are uploaded and how many are pinned and queued — with an index for each sort, alone and under the uploaded filter; whether the download queue holds it waiting or downloading, and whether it holds it failed; whether a move of it failed; and where it sorts by storage, worked out from those: a failed download or move, uploading, pinning (a download in the queue), local, pinned, in storage only |
| `storage_totals` | value of a release's `release_storage` uploaded flag (local, uploaded), keyed by it: how many releases hold it and their files' total size, which the Storage manager (§4.2) reads as its count and total size under the Local and Uploaded filters, and adds up under All, when no search narrows the list |
| `upload_batch` | release with files queued to upload since none had any, which the Activity panel's Uploading line counts (§6.4) |
| `album_search`, `track_search`, `work_search`, `artist_search`, `artist_credit_search` | album title, track title, work title, artist name and sort name, or other printed name, lowercased, in an FTS5 trigram index library search (§2.3) matches against, beside the searched row's key |
| `album_search_entry`, `track_search_entry`, `work_search_entry`, `artist_search_entry`, `artist_credit_search_entry` | searched row, keyed by its own key: the integer its search row is found by; a track's and a work's entry also holds its title's sort key, which bae writes with the title and again when the device's language changes, and an index in title order that library search reads its first tracks and works from, stopping at its limit |

Local triggers write every table of bae-views' part in the same write as the
library rows they come from, so they always match those rows. A release's
storage row follows coven's records of its files, the pins `coven_pins` lists
as kept and coven's upload queue the same way, in the transaction that changes
one of them, whether bae's own write or coven's as a file is pinned, downloads,
uploads or is unpinned, and its listing row copies what it sorts by in that
write. Coven applies a write's rows parents first, so a row is never there before a
row it references and no trigger waits for one. An
album's listing row is written only when one of its values changes, and its
releases' rows only when its title or credits do, so a release added to an
album, even a box set of thousands, rewrites no other release's row.

Whether a release is local or uploaded is not a column: it is whether every
one of its files references a coven file that coven records as stored.

### A.2 People (bae-people)

All sync (§5), except `admin_operations`.

| Table | One row per |
|---|---|
| `people` | person, keyed by their `MemberId`: name, private circle, history setting (private or household), and the coven file of their avatar (`avatar_id`), with an index by it |
| `ratings` | rating by a person of an album or of a track, one to five stars |
| `history` | play: person, track, when it started, how much was played, audience |
| `playlists` | playlist: whose it is, its name and its audience |
| `playlist_entries` | place in a playlist, keyed by the playlist and a position id: the track there and its position |
| `admin_operations` | admin change this device asked for, while coven lists its operation: keyed by that operation, the library's new name, the person and their new role, the person removed, the device removed, or the person an approval lets in with the name their invite gave them (§5.6) |

### A.3 Playback

| Table | Holds |
|---|---|
| `playback_queue` | the one row of this device's queue: repeat, what's playing from (its kind, and the release when it plays one), whether shuffle is on, which of its rows is playing (its place in each of the two orders), the number the next entry of either part gets, never given twice |
| `playback_manual` | Up next entry, in order: the track it plays |
| `playback_context` | entry of what's playing from: its track, its place in the list's own order and its place in the shuffled order |
| `playback_state` | the one row of what this device was playing (§7.8) |
| `playback_now` | the one row of what this device's playback is doing: track and phase, whether a load waits on storage, pause prompt, volume, mute, receiver and its kind, preview, and the last failure, with the AirPlay receiver that no longer accepts this device (§9.4) |
| `playback_timeline` | the one row of where this device's playback is, for the surfaces that move the position on themselves: the playing track's line and the preview's, each a position and whether it moves on from there, and whether playback started the track's line at a seek (§9.4) |
| `cast_receiver` | receiver the cast picker lists, as discovery last found it: its id, name, kind (Cast, UPnP or AirPlay) and whether it takes a volume |

### A.4 Watched folders and scans

| Table | Holds |
|---|---|
| `watched_import_folders` | watched folders, in order, no two overlapping: the write that watches a folder refuses one that holds or lies in a watched folder (§3.1) |
| `release_grouping`, `release_grouping_member` | folders read as one release, or kept apart, and by whom (§3.1) |
| `skipped_import_candidates` | candidates the person dismissed |
| `folder_discovery` | when each folder was first seen, and its date |
| `folder_scan_generation_sequence`, `folder_scan_roots`, `folder_scan_directory` | each watched folder's last scan, with how it ended once it stored that, and the directories it saw; whether a pass is reading the folder is in `folder_scan_held` (§3.1) |
| `folder_scan_held` | what bae holds of each folder's scan that no scan stores (a pass reading it, or a failure its scan could not store); it lasts only as long as the process, so the library empties it as it opens (§3.1) |
| `scan_candidate`, `scan_candidate_file`, `scan_candidate_part` | the candidates a scan found, their files and their folders, and for each the image the folder's cover rules pick when bae reads no tags (its blank cover, which the scan writes with its files) and, for a broken one, why bae can't import it (which the scan also writes with its files), with an index on each watched folder's candidates by the folder each reads its files from; each file by its name in the release and its path, a BLOB of coven's bytes for it |
| `scan_candidate_tag_snapshot`, `scan_candidate_file_tag` | the tags read from a candidate's audio that place, name and seed it (no ISRCs, labels, copyright lines or store markers, §3.3), or for a file whose tags don't parse, why, and its embedded cover |
| `scan_skipped_file`, `scan_skipped_tags` | a broken folder's files as the scan read them, and what its tags call it; two of its files can show one name, so each is unique by its path |
| `scan_sidecar`, `scan_sidecar_file` | files under a folder no release owns, each unique by its path |
| `scan_cue_sheet`, `scan_cue_track`, `scan_cue_index`, `scan_sheet_audio_file` | parsed CUE sheets and the audio each `FILE` line resolved to |
| `scan_candidate_unit`, `scan_candidate_sheet_offer` | each candidate's audio units in play order with each unit's length and preview range, and each CUE `FILE` line's offered audio, written with its files |

Every folder path and candidate key in these tables, and in A.5 and A.7, is a
BLOB: a path as coven's bytes for it, a folder below a watched folder as its
names' bytes joined by `/` (empty for the watched folder), a candidate key as
its folder's path bytes or `grouping:` and an id.

### A.5 Import candidates

Keyed by the candidate's content hash.

| Table | Holds |
|---|---|
| `import_candidate_state`, `import_candidate_folder` | the candidate, its revisions, and where it was found, with an index on the candidates a file decision was made for, and one on the folders they were found in |
| `import_candidate_edit`, `import_candidate_edit_label`, `import_candidate_edit_barcode`, `import_candidate_album_artist_assignment`, `import_candidate_track`, `import_candidate_track_artist_assignment` | the draft's editable fields, its label and barcode rows as typed (a label row with the catalog ids its applied release gave it), each track's place and listed length (§3.6). Each draft row also holds whether the draft is blank and whether it would import. Every state of a folder this device did not import holds a draft row: the scan writes the blank one, and the write that deletes this device's release of the folder writes it again, from the folder's stored audio units; an imported one holds no draft, which the import deleted with its source, cover and link (§3.7) |
| `import_candidate_medium` | the draft's mediums |
| `import_candidate_track_role`, `import_candidate_release_role`, `import_candidate_work`, `import_candidate_work_credit`, `import_candidate_work_part`, `import_candidate_track_work` | the draft's roles and works, as values, each credit with its role, qualifiers and the name its catalog printed |
| `import_candidate_sheet_disc`, `import_candidate_sheet_reference` | the person's CUE decisions |
| `import_candidate_cover`, `import_candidate_cover_copy` | the chosen cover, a catalog's with the bytes it served on the same row, so a catalog's cover is never stored without them and no other cover holds any; a cover that names a file or embedded artwork its scan did not store is a damaged row; for embedded artwork a row carries the source file's version and names the artwork by candidate and file, never its bytes |
| `import_candidate_album_pick` | the library album the person picked for it to join |
| `import_candidate_draft_provenance` | where the draft was read from |
| `import_candidate_release_link`, `import_candidate_release_link_partner`, `import_candidate_release_link_album` | the pressing or album the candidate is linked to |
| `import_candidate_claimed_record`, `import_candidate_offered_cover` | what the link claims (§3.5, §3.7): the records its claimed releases describe it by, merged, or a linked album's albums; and the covers a linked pressing offers the picker, in order. They follow each later store of a claimed release or change to the albums kept for its album, and go with the link |
| `import_candidate_pane_failure` | the pane's last command, when it failed, and why |
| `import_candidate_failure` | why the last import failed |

Beside them, keyed by the candidate's key as the import list names it:

| Table | Holds |
|---|---|
| `import_candidate_standing` | where a candidate something is running for stands, and the only place that is held: its identification queued, running, writing its answer (drafting the pressing it picked unattended first, and whether Import was asked of that answer) or failed to write it or to start (what a run starts from, or whether an answer is stored already, could not be read), for the files it names; its import at the step it has reached: queued for the worker, validating its source files, fetching artist pictures, reading files, measuring loudness, or finalizing (writing its release). A change that cannot be written fails what asked for it. A candidate's identification ends in the write that removes the candidate, reads it with other files or combines it into another release, so a folder removed and found again starts with none, and in the write of a person's decision about it — a pick, its files' tags, a cleared draft, a linked album, a skip — or of an import's claim, so no read sees the decision beside the identification it answers. A second import of a candidate is refused while one stands. A panic while bae works out a candidate's state ends the app, after reporting it as a bug, since what the list shows could no longer be trusted. A stored verdict ends the run it answers in its own transaction, a release ends its import in its own, and a failed import's failure ends it in the failure's. What each run published is `import_identify_run`'s. No row outlives the process: the table is emptied as the library opens |
| `import_catalog_apply_step` | the network read each running apply of a catalog release (an import pick's or the release editor's) waits on: reading a release from its catalog or fetching the cover from the catalog that offers it, with that catalog. A row exists only while its read runs, and a write of the row that fails fails the apply with its error. No apply outlives the process, so the table is emptied as the library opens |
| `import_identification_batch` | the identifications counted as one batch for the count the list's chrome draws: each candidate admitted since the batch opened, and whether its identification has ended. Each change to an identification moves it in that change's own write; the batch's rows go once every one has ended, and the table is emptied as the library opens |
| `import_identification_count` | one row while `import_identification_batch` holds any: how many candidates that table holds and how many of them have ended. The count the list's chrome draws reads this row, so it reads one row however many folders the batch holds. Triggers on `import_identification_batch` keep the row in the write that changes a batch row |
| `import_identify_run` | each identify run in flight and the key it identifies. How the run stands is stored in the shape of the verdict it may end on (A.6), owned by the run instead of by a candidate's files: a verdict row, `running` while its lookups are out, with the ledger as it stands (clues still being read or scanned, cells queued or looking up, a title search waiting) and the releases its answers so far combine to as match rows, then the answer it reached while that is written; beside them the signals it last read, scans still going included, and how its rows stand against the text of those signals. A folder candidate's run has its rows only while its standing names the run, so no candidate shows a run its standing does not name, or a standing whose run published nothing. A library release's re-identify run, which has no standing, keeps its rows until it ends or the release's sheet closes. The pane and the release editor's search show the queue's wait or the run's state while identification does something for the key (for a re-identify session, its latest run in flight, else its latest answer), else the state of the candidate's stored verdict, else Idle, and show a verdict the same way in both. Every row a run owns goes with its row here, and the table is emptied as the library opens |
| `import_candidate_search`, `import_candidate_search_source`, `import_candidate_search_pick`, `import_candidate_search_pick_album` | a person's typed search for a candidate (§3.5): the query as columns by mode (artist and album, catalog number, or barcode), and each catalog's part of it in the catalogs' order: not asked (switched off, or needing a credential the library lacks), searching, answered with the releases it returned (match rows the search owns, A.6), or failed (the catalog's failure with its HTTP status, or the error chain of bae breaking). Each answer, pick and switch lays the whole list of the search's releases out again in its own write, and stores each release's pressing row, its card and its place among its row's records, the text of each card and what each row lists (A.6), and that write also stores what the grouping rules decide for each card: its place among the cards (the card with the best row first), its sections (one for each album where it holds two albums of one catalog), the place of each row in its section (by the year its lead record states), its cover and the catalogs it names. The search stands searching while a part is, else failed when a part failed, else no matches when a part answered and no part returned a release, else found. bae writes each catalog's answer in a write of its own, and only to a part still searching, of the search bae asked it for: a new search for the key replaces the row on a run number never used again, so bae writes nothing for an answer to the replaced search, a second answer, or one for a part switched off. Retry puts only the failed parts back to searching, on the same run. Switching a catalog off closes its part of every search and deletes what it returned. A pick of a MusicBrainz release on the list stores what its documents state its album is on the other catalog, and the write lays the list out with every pick together: each MusicBrainz row holds the album bae works out for its group from every pick and the list; a release the list does not hold, a partner the pick claims included, is none of the search's. A folder candidate's search is of the files it names and goes in the scan write that leaves the candidate with other files or none; a library release's names none. Every row a search owns goes with its row, and the table is emptied as the library opens |

Beside the drafts, keyed by a credit's folded name and catalog ids:

| Table | Holds |
|---|---|
| `import_credit_resolution`, `import_credit_resolution_artist` | for each distinct artist credit a draft holds, the library artists the rules of §2.3 name for it, in their order: none when importing would make a new artist, one, or the several the credit leaves ambiguous, each with the ids and sort name the rules give it. The write that adds or changes a credit, or changes an artist the rules can name for it, stores the answer in the same transaction, whether this device made the write or applied another device's, and the write that removes the last credit holding a key deletes its answer |
| `import_credit_resolution_due` | the credits whose answer a transaction's writes may have changed; bae answers each one and empties the table before the transaction commits |

Beside them, keyed by the release an import made:

| Table | One row per |
|---|---|
| `local_release_origin` | release this device imported, keyed by the release: the hash of the folder's files it read (§2.7), unique, so one hash is one release here. Importing writes it in the release's own transaction; deleting the release deletes it |
| `release_sheet_discs` | the disc the person said a release's CUE sheet is, or that the release ignores it, keyed by the sheet's `release_files` row; only a sheet the person decided has a row. Importing writes its rows and identifying the release again reads them, as for `release_sheet_bindings`; deleting the sheet deletes it. The table syncs (Appendix B) |
| `release_sheet_bindings` | the audio the person bound a `FILE` line of a release's CUE sheet to in the import pane, or cleared (no audio), keyed by the sheet's `release_files` row and the `FILE` line's text; only a line the person decided has a row. Importing writes the rows in the release's own transaction, from the candidate's stored decisions (naming each file by its path in the release); identifying the release again reads them. Deleting either file deletes the row. The table syncs (Appendix B) |

### A.6 Identification

| Table | Holds |
|---|---|
| `import_candidate_signals`, `import_candidate_signal_value`, `import_candidate_text_line` | the clues read off a candidate (§3.3), or those a run in flight last read (A.5) |
| `import_candidate_lookup_choices`, `import_candidate_chosen_catalog`, `import_candidate_discounted_catalog`, `import_candidate_excluded_barcode` | what the person let the lookups use |
| `import_candidate_verdict` | what a run concluded: found, not found, manual only, failed or error; how each clue stood in the run it laid out; a medium conflict. Owned by a candidate's files, or by a run in flight (A.5), whose row may be `running`; this table, its matches and its ledger rows name their owner the same way. For a found verdict, the write that stores it also stores what it decides (§3.5): the release it picks, or why it picks none (the folder's files rule out the medium of every release found, it found nothing that fits, it found several pressings, it could not read a record in full, or bae cannot check the tracklist). |
| `import_candidate_verdict_failure` | the lookups a failed verdict's run could not get answered, in the order it sent them: the clue or fetch (a Disc ID, a barcode, a catalog number, the title search, the ISRCs, an offered row's pressing, the picked release's cover), the catalog that failed it, and how it failed. The pane says in lines of their own the ones no failed cell of the verdict's ledger shows: every one where the pane offers no releases; under offered releases, the failed fetches of the picked release, and each other lookup once as a line saying the list is missing its results, without repeats |
| `import_candidate_match` and its `_label`, `_barcode`, `_cover_copy`, `_link`, `_album_link`, `_medium` tables | every release a run returned, once: the ones it offers, the ones set aside, and the ones a lookup found that neither lists, with the pressing row of a listed one; each label with the catalog ids the lookup gave it. A typed search's releases are rows too, owned by the search and each under the catalog part that returned it, held only while that part stands answered (A.5). A search's release states its pressing row, its card (the list place of the card's first release), its place among its row's records, the section of the card its row shows in and the row's place in that section; a ledger cell's release states the same for the cell's own list. A release a verdict or run lists also states which of its fields the folder's text agrees with (Disc ID, barcode, catalog number, label, year, country, album title, album artist), and states its card and place the same way. A match's album's first year is not kept: it is read from the stored catalog release the match names (A.8), as that release is stored now |
| `import_candidate_lookup_value`, `import_candidate_lookup`, `import_candidate_lookup_release` | the run's rows (each Disc ID, barcode and catalog number, the ISRCs, the title search), each row's lookups by catalog with what each answered, and the releases each found, as the match rows holding them; a release a Disc ID's lookup found is what its Disc ID agreement is read from |
| `import_candidate_card_list`, `import_candidate_card`, `import_candidate_card_text`, `import_candidate_row_statement` | what the card rules decided for a list of releases: the list itself, as its owner's own list or the list of one cell of its owner's ledger; each card's place among the list's cards, the id that names it, the release whose cover it shows, the years its releases state, the catalogs it names and, for a card that holds two albums of one catalog, each section's album; the titles, artists and labels each card names; and what each row lists where its records state a fact (years, label lines with their catalog numbers, barcodes, areas, media), each value once with the catalogs that state it where the records differ. A typed search's list and each ledger cell's list have them |
| `import_candidate_lookup_provider`, `import_candidate_lookup_isrc`, `import_candidate_lookup_offered_catalog` | the catalogs the run asked, the ISRCs it looked up, and the catalog numbers it offered to pick |

### A.7 The import list

| Table | Holds |
|---|---|
| `import_list_placement` | where each row of the import list sits (§3.2): one row per release the queue lists and one per header of a group on Found, with its tab, its state on Found and why it needs the person (how many pressings, what the folder's files say, how bae broke), the group listing it, the date and path it sorts by, the text it shows (an imported release's library album title and credits) and that text case folded for the search, and what its commands are decided from: whether it can be acted on, whether its last import failed, whether its draft would import, what its lookup came to, whether a grouping reads it; for a header, whether its folder is offered to be read as one release. Triggers write it in the same write that changes any table it comes from, so it never lags behind them. One index per order, holding Imported's and Skipped's rows and none of Found's, serves their windows. Found's are read in three partial ones per order — the headers and the rows outside every group, the headers alone, and each group's members — so a member sorts in its header's place and only the header holds its group's date, its newest member's |
| `import_list_count` | how many rows of the placement each tab holds of each item (a release, a folder that failed validation, a group's header) and, on Found, how many releases each entry of the filter holds: what the tab bar, the filter and the list's count read (§3.2). Triggers on the placement change them in the write that places, moves or removes a row |
| `import_list_group_count` | how many of each Found group's members each entry of the filter holds: what a group's header reads to count its members while no search narrows the list, and what the list's count reads for the members of the groups it folds shut and, under an entry, for the groups holding the entry's releases (§3.2). Triggers on the placement change them in the write that places, moves or removes a member |
| `import_list_search` | each release's shown title and artists, case folded as the search folds what is typed, in an FTS5 trigram index under its placement row's number, kept by triggers on the placement in the write that moves a row (§3.2) |

### A.8 Catalog releases

Device-local: any device can fetch a release. A release is stored in the
write that writes the first row naming it (a match, a draft's provenance, a
link or a linked pressing's partner), and triggers on those four tables delete
it in the write that drops the last; a later write's copy replaces every row
under it only where it read a document the stored copy lists as unfetched
(§3.4). The provenance, link and partner rows name it by a key checked as the
write commits, so the release and the row naming it land in either order.

| Table | Holds |
|---|---|
| `source_release` | one catalog release a row names, as fetched, and every fact an import reads from it but two it reads derived (§3.4): its album's year and its media counts |
| `source_release_label`, `source_release_barcode` | its labels, each with its catalog number and the label's catalog ids, and its barcodes, in order |
| `source_release_album_artist`, `source_release_role`, `source_release_entry_credit`, `source_release_entry_role` | its credits, each with its role, qualifiers and the name its catalog printed |
| `source_release_medium`, `source_release_medium_disc`, `source_release_entry`, `source_release_format` | its mediums, each with its carrier where stated and the Disc IDs registered against it, their tracklists, and its format entries |
| `source_release_work`, `source_release_work_credit` | its MusicBrainz works, each credit with its role and the name MusicBrainz printed |
| `source_release_link`, `source_release_album_link`, `source_release_record`, `release_group_album_link` | what it says it is on other catalogs |
| `source_release_cover`, `source_release_cover_copy`, `source_release_archive_group` | the images it offers a picker |
| `source_release_unfetched` | linked documents the fetch didn't get, and why: the source failed, its catalog was not asked (switched off, or Discogs had no key), or a run's read stopped short of it (the release's own page, where the run read it from a Disc ID answer) |
| `release_cover_fetch` | the library release a picker fetched covers for, and whether it names a catalog release; deleting a release deletes its rows |
| `release_cover_entry` | the covers a fetch found for a release, in the order offered, each as a stored cover with its catalog beside it for the picker's source filter |
| `candidate_cover_fetch` | the candidate a picker fetched covers for, the link it fetched them for, and whether it was linked then; deleting the candidate's state deletes its rows |
| `candidate_cover_entry` | the covers a fetch found for a candidate, in the order offered, each as a stored cover with its catalog beside it for the picker's source filter |

### A.9 Transfer queues

| Table | Holds |
|---|---|
| `release_queues` | whether the pin queue and the export and save queue are paused |
| `download_queue` | release queued to pin or to move to this device, in the order queued: pin or move to this device, with a move-to-this-device's folder, a BLOB of coven's bytes for its path, and the release's file count and total size when it was queued; and queued, downloading, or failed with its error (§4.3, §4.4) |
| `download_batch` | release and kind the download queue has held since that kind last had none waiting or downloading, which the Activity panel's Pinning and Moving to This Mac lines count (§6.4) |
| `output_queue` | release queued to export or save, in the order queued: its folder, a BLOB of coven's bytes for its path, export or save with its format and save settings captured whole, the release's file count and total size when it was queued, and queued, writing, or failed with its error (§4.6) |

The download queue keeps its entries and its pause across restarts; the export
queue opens empty and running.

### A.10 Settings

| Table | Holds |
|---|---|
| `preferences` | the one row of this device's settings for the library (Appendix C) |
| `save_preset` | save format, in the order listed: its name, unique, and its codec with the source's depth, 16 bits, or a bitrate (§4.6) |
| `save_filename_token` | piece of the one file name pattern every save names its files by, a value or typed text, in order |
| `save_folder_token` | piece of the folder name pattern, a value or typed text, in order |

### A.11 Sync status (bae-sync-status)

Never synced (§6.4).

| Table | Holds |
|---|---|
| `sync_start_failure` | why this device's last attempt to start sync failed, while it stands, at most one row: the failure's class and, for a storage failure, coven's reason with what it names; emptied as the library opens, and cleared by the trigger that fires as any start gets under way (§6.4) |

### A.12 Catalog image cache (bae-catalogs)

Never synced (§4.7).

| Table | One row per |
|---|---|
| `cached_image` | file in the catalog image cache's folder, keyed by its name (the SHA-256 of the image's address): its size in bytes, and its place in the order of the cache's reads and writes, one past the highest each time bae writes or reads the file, which eviction removes the lowest of first |

---

## Appendix B. Synced tables

The table below lists the synced tables: the library's, the people's and the import's two, `release_sheet_bindings` and `release_sheet_discs`. *Shared key* rows take an id computed
from their fact (§2.6); *independent* rows a fresh id. Every synced table's
rows reach every device that can read their audience: a local release's rows
too (§4.2). *Audience* is the store unless the table names one (§5.3); *owner*
tables are changed only by their author's devices (§5.5). *File* names the
column that references a coven file, whether the reference is trailing or
bound (§6.3), and the namespace its files are in. bae declares no other table: `local_release_origin`, the import tables (A.4–A.8) other than `release_sheet_bindings` and `release_sheet_discs`,
`import_candidate_standing`, `import_identification_batch`,
`import_identification_count`, `import_identify_run` and `import_catalog_apply_step` among them, or the
transfer queues (A.9), so they never sync.

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
| `labels` | shared key | store | | |
| `label_merges` | shared key | store | | |
| `release_labels` | shared key | store | | |
| `release_barcodes` | shared key | store | | |
| `release_media` | shared key | store | | |
| `tracks` | independent | store | | |
| `track_artists` | shared key | store | | |
| `works` | shared key | store | | |
| `work_artists` | shared key | store | | |
| `work_parts` | shared key | store | | |
| `track_works` | independent | store | | |
| `release_artist_roles` | independent | store | | |
| `track_artist_roles` | independent | store | | |
| `release_files` | independent | store | | `file`, trailing, in `release_files`: the person's own; set by the import; on the importing device only until moved to storage; read on first use |
| `audio_formats` | independent | store | | |
| `audio_format_segments` | independent | store | | |
| `audio_segment_scrub_points` | independent: its segment | store | | |
| `release_sheet_bindings` | shared key: the sheet's file and the `FILE` line | store | | |
| `release_sheet_discs` | shared key: the sheet's file | store | | |
| `covers` | shared key: the release | store | | `file`, bound, in `covers`: bae's own; read on first use |
| `artist_images` | shared key: the artist | store | | `file`, bound, in `artist_images`: bae's own; read on first use; shared by two merged artists |
| `people` | shared key: the person's `MemberId` | store | yes | `avatar_id`, bound, in `avatars`: bae's own; read on first use |
| `ratings` | shared key: the person, and the album or track | store | yes | |
| `history` | independent, one per play | per row: the person's circle or the store, by their setting | yes | |
| `playlists` | independent | per row: the person's circle or the store | yes | |
| `playlist_entries` | shared key: the playlist and a position id | its playlist's | yes | |

---

## Appendix C. Settings

This device's settings for a library are one row of the library's
device-local `preferences` table, with its save formats as rows of
`save_preset` and its file and folder name patterns as rows of
`save_filename_token` and `save_folder_token`. They are never synced. A copy of the library new
to this device — created, added from another device, joined, or restored or
reset over this device's copy — starts at the defaults below; nothing carries
the settings of a copy it replaces. Each change writes the row in one write,
and a setting takes effect when what it decides next acts: playback as it
loads a track or reaches a side's end, an import as it starts, an identification run
as it starts and as it settles, casting as the picker or its settings page
opens, as the setting changes, or as a cast starts,
the MCP and Subsonic servers as the row changes, and the store's transfer
limits as the library opens and as either limit changes. The schema
refuses what no setter writes: a last-used format that
names no format, two
formats of one name, a format its codec can't write, a name pattern piece
that is not exactly one of a value and typed text, or is typed text of
nothing. A Subsonic server turned on with no username or password, or at a port
outside 1 to 65535 (kept as typed), is kept on and doesn't run until nothing
stands in its way. When the row can't be read, both servers stop and Settings
shows why, instead of showing them off.
Replay gain has no control in the apps.

| Setting | Default | What it decides |
|---|---|---|
| `discogs_key` | none | the stored Discogs key's state: unvalidated (checked while offline), valid or rejected; the key is in the keyring |
| `replay_gain_mode` | off | loudness: off, track or album (§7.5) |
| `save_preset` rows | FLAC (the source's depth), MP3 (320 kbps) | the save formats (§4.6), in order |
| `last_save_preset` | FLAC | which format a save starts with: the one last used; deleting that format makes the first format left the one last used |
| `save_filename_token` rows | track number, “ ”, title | the file name pattern (§4.6) |
| `save_folder_token` rows | artist, “ - ”, album | the folder name pattern of a release save (§4.6) |
| `save_embed_cover`, `save_cover_file`, `save_loudness_tags` | on, on, on | the cover in each file, `cover.jpg` in a release's folder, ReplayGain tags (§4.6) |
| `pause_between_sides` | on | pausing between sides and discs (§7.3) |
| `side_pause_countdown_seconds` | none | when such a pause ends on its own: 5, 15, 30, 45 or 60 seconds, or none to wait for Play |
| `max_concurrent_uploads`, `max_concurrent_downloads` | 3 | files moved at once, 1 to 8; adding this device, joining, and a restore or reset of this device's copy read the library from storage at the default, since the row is in the store being read; the library opens at the row's limits |
| `show_remaining_time` | off | the seek bar counts down |
| `library_full_width` | off | the library page spans the window |
| `ask_before_unpinning` | on | Unpin asks first; the dialog's Don't show this again turns it off, Settings → Storage turns it back on |
| `import_destination` | cloud | where an import goes: this device or the cloud; one choice, offered as a pop-up on the import card, on the bulk pane's Import row and as Import to in Settings, which dims while automatic import is off; with no cloud storage in the library, an import stays on this device whatever it says, and the pop-up is not shown |
| `identify_automatically` | on | identifying found releases on their own |
| `import_when_identified` | off | importing what any run picks unattended |
| `ask_musicbrainz`, `ask_discogs` | both on | which catalogs are asked |
| `cast_enabled` | off | casting to receivers |
| `cast_transcode_format` | MP3 | what a receiver gets when a track needs converting |
| `mcp_enabled`, `mcp_port` | off, 47777 | the MCP server; its token is in the keyring |
| `subsonic_enabled`, `_port`, `_username`, `_network` | off, 4533, none, this machine only | the Subsonic server; `_network` is this machine only, one network by its interface, or every network; its password is in the keyring |

A save format holds a name and a codec: FLAC, WAV or AIFF at the source's
depth or at 16 bits; MP3 at one of the 14 bitrates its encoder writes at a
constant rate (32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256 and
320 kbps); AAC at 96, 128, 160, 192, 256 or 320 kbps; Opus at 64, 96, 128,
160, 192 or 256 kbps. A format switched to another codec keeps its depth
between lossless codecs and its bitrate between lossy ones when the new one
offers it; otherwise it starts at the source's depth, or MP3 at 320, AAC at
256 and Opus at 192 kbps, and a name still as it was made follows the new
codec. A format the person adds starts as FLAC at the source's depth, named
"FLAC" or the first free "FLAC (n)". A name pattern is an ordered list of pieces,
each a value or typed text. The file pattern's values are title, artist,
album, album year, release year, track number, disc number and track total;
the folder pattern's are artist, album, album year, release year, label and
catalog number. Settings → Save as edits both under Names, and a save keeps
the patterns it was queued with.

Platform settings, such as the appearance and restoring playback on launch,
live in each app. Appearance reads Theme (System, Light, Dark), then Tone, then
Accent, so named in every locale; Accent names the chosen colour under its
title, as Theme and Tone show theirs. The apps start with the System theme, the
Plum tone and the Pink accent. The accents are Blue, Orchid, Pink, Coral, Gold,
Lime, Mint and Sky, in that order, each named in every locale; a stored accent
the app doesn't have reads as Pink. On the Mac the chosen Settings pane
is drawn in the chosen accent's selection colour, and so is the highlighted item
of every menu and pop-up bae draws: a raised face with a hairline edge, the
chosen item of a pop-up checked, its round chevron button in the control fill.
Those menus open over or under what opened them, or at the click for a context
menu; the arrows, Return, Escape and typed letters move through them, a click
outside closes them, and VoiceOver reads them as menus. VoiceOver reads each
control that opens one of them as a pop-up that opens a menu, and pressing it
opens the menu. The Mac's popovers (the
cast picker, Activity, the ⓘ and ? tips) are drawn the same way, in the raised
colour with a hairline edge and macOS's arrow. On the Mac a symbol
that a control or the words beside it already name (the ⓘ, a close or remove
cross, the folder, speaker or arrow beside a name) is hidden from VoiceOver; a
symbol that says something nothing else says is read as that: the speaker on the
playing track as Now playing, a failed lookup's mark as Failed. On the Mac
VoiceOver reads each progress bar and spinner by the words beside it ("Pinning",
"Finding your libraries…"), and reads a spinner with no words beside it as
Loading. The parts macOS draws itself, such as the menu bar's menus and the save
panel's Format pop-up, take the Pink accent the app declares, as long as the
Mac's own accent is Multicolor, and the person's Mac accent otherwise; they do
not follow the accent chosen here. Checkboxes, switches, sliders, radio buttons,
the keyboard focus ring, selected text in fields and a list's or table's
selected row show the chosen accent and never the Mac's own accent colour. On
the Mac Theme and Tone are pop-up buttons; on Android the Appearance page has no
heading over its card, the top bar names it. Android's Settings rows each say
where that setting stands under their title: Appearance's theme, tone and
accent, the library's name and its sync state, which of pausing between sides
and restoring on launch are on, how many pins run at once, the total this phone
keeps for the library (§4.7, "3.9 GB on this phone", nothing until the first
measurement), whether casting is on, and the version. The Mac's Settings is one
window with no title bar: its panes' names, as text alone, down a sidebar that
holds the window's buttons at its top, inset 12 points from the panel's top left
corner, and each pane's title in bold at the top left of the pane. A page inside
a pane puts a round button, 24 points across and named "Back to" and the page it
goes back to, before its title. The Settings window opens on Appearance, the
first pane. The sidebar names the sync state beside Library only while it is
amber or red, and Offline beside Subsonic while it is on and not running, each
as a chip tinted in its tone. About shows the bae logo, the sheep over the word,
96 points wide and still, over the version: Android's Version is a row on a
card, the Mac's a line with Check for updates….

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
| `watched_folders_list`, `watched_folder_add`, `watched_folder_remove`, `watched_folders_scan` | manage and scan watched folders; the list gives each folder's path as text and a key, the hex of its path's bytes, which `watched_folder_remove` takes, so a folder whose name isn't UTF-8 is removed as surely as any other; `watched_folder_add` takes a path as text |
| `import_candidates_list`, `import_candidate_get` | read the import list and a candidate |
| `import_candidate_skip_set` | skip or restore a candidate |
| `import_search` | search MusicBrainz or Discogs; its input names no other catalog |
| `import_candidate_release_select` | link a candidate to a MusicBrainz or Discogs release and read its draft from it; its input names no other catalog |
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
| `release_storage_action` | move to storage, pin, unpin, queue a move-to-this-device, or cancel |
| `output_status` | read the export queue |
| `release_reidentify` | re-identify a release as a MusicBrainz or Discogs release: apply its every value to a form of its own and save it, records and album with it, into the album its records name or a library album given over them |
| `release_album_set` | move a release into a library album, which keeps its own title and artists, as the editor's album picker does |
| `release_metadata_update` | apply a metadata edit: the album fields, the pressing, and each track's title and artists (a track's place is the source's, not edited) |
| `library_search` | search the library |

---

## Appendix G. Errors and states

### G.1 Error categories

A failure is a cancellation (bae shows nothing), the app ending (bae shows
nothing), a library that isn't there, or one of these categories with a
detail:

| Category | The person |
|---|---|
| `Database`, `Config`, `Internal` | reports it |
| `DatabaseBusy` | asks again: the library's database was busy with other work, or what the call read changed under it |
| `DatabaseDamaged` | can't go on with this device's copy: the library's database file is damaged, or holds a value nothing writes; a failed open with it on a library that has storage offers Reset from cloud (§6.4) |
| `SyncUpdateRequired` | updates the app |
| `DeviceResetRequired` | resets the device (§6.4): when another installation replaced this one or the member's keys changed, bae asks first and shows the reset's warning; when another installation uses this device's id and this copy is the one behind, bae resets at once without asking |
| `DeviceRemoved` | stops syncing this library on this device for good: the device, or its person, was removed from the library, or the library refused a joined device's admission |
| `Import`, `ImportData` | reads the detail: an import couldn't finish, because a catalog or a source couldn't answer or be read, or its candidate or the library's artists changed before its write, and importing again works on what now stands |
| `ChangedMeanwhile` | asks again, on what now stands: what a change acted on changed before it was written, and nothing was written (a release, album or file deleted before a change of it, an album or release a deletion was planned against, a release whose tracks came or went under an edit, a cover or artist image replaced again, a release picked as its album's primary that moved to another album, an artist given a printed name that was deleted or merged, an artist a credit picked that left the library, a candidate or the watched folders that changed under a change to them, a cancel that came after the import began writing) |
| `InvalidEdit` | fixes what they typed, as the line says: an album title, an artist, an artist's name, a year |
| `FilenamePatternInvalid`, `FolderPatternInvalid` | fixes the file or folder name pattern in Settings: a save named by a pattern that can't make a name wrote nothing, and the detail says why |
| `WatchedFolderOverlap` | watches another folder: the one they asked for lies in a watched folder, or holds one that bae started watching while it added theirs, and the detail names both |
| `CandidateImportInProgress`, `CandidateBeingIdentified` | waits |
| `FilesAlreadyImported` | finds the release this device already imported from these files; they aren't imported again |
| `AlbumGone` | picks another album or New album: the album the import files under was deleted after the import was prepared, and nothing was written |
| `MetadataTrackCount` | picks another release: it lists another number of tracks |
| `TiedMediums` | picks another release or uses the file metadata: several of its discs fit the folder alike, and the line names them |
| `ReleaseChangedSinceOpened` | cancels to see the release as it is now: it changed after the editor opened it (§3.8) |
| `GroupingBlocked` | combines again, separates, or waits for a download |
| `Export`, `Save` | reads the detail |
| `Storage` | does what coven's provider reason, which the line names, asks: frees space, fixes this device's clock, signs in again, retries the network |
| `StorageCheck` | changes what the check of the storage it failed names |
| `S3VersioningUnknown`, `S3VersioningUnsupported` | uses S3 storage whose versioning coven can read and work with |
| `StorageLocationChangeUnsupported` | keeps the library's storage location; it can't move |
| `StoreOwnerAccount` | signs in with the account that holds the library's storage: setting up its storage again or restoring it as the person whose account holds it, under another account, is refused, since their access can't move to another account ([coven §20.9](https://github.com/bae-fm/coven/blob/main/spec/coven.md#209-sharing-with-accounts)) |
| `Credentials` | fixes the storage settings |
| `Network` | retries |
| `Keyring`, `KeyringLocked` | fixes the keyring, or unlocks and is asked again |
| `KeychainEntitlementMissing` | installs a build of bae signed with its keychain entitlement; retrying or unlocking doesn't help |
| `SavedRestoreCodesFull`, `SavedRestoreCodeTooLarge` | on Android, learns the library's restore code couldn't be saved on the phone: Block Store already keeps as many entries as Android gives an app, or the code doesn't fit in one ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)) |
| `Membership` | retries the membership step, with the advice it carries |
| `InviteJoining` | leaves the invite: cancelling it came after the inviting device began letting its person in, or after the invite ended (§5.6) |
| `AirPlayUnsupported`, `AirPlayHomeOnly`, `AirPlayAdminsOnly`, `AirPlayOwnerOnly`, `AirPlayUnreadableSettings`, `AirPlayNotPairing`, `AirPlayFull` | uses another receiver |
| `AirPlayPairingFailed` | reads the detail |
| `AirPlayLockedOut` | resets the receiver |
| `AirPlayBusy`, `AirPlayBackOff`, `AirPlayUnreachable` | retries, once the receiver is done pairing, the stated wait has passed, or the receiver can be reached |
| `Renderer` | reads what the device playback was sent to said when it refused a command |
| `ReplacementUnfinished` | finishes the reset or the restore of this device's copy that stopped partway: the one the category names, never the other (§6.4) |
| `RestoreRegistered` | finishes the restore: it was cancelled after this device was added to the library, so it can't be taken back (§6.4) |
| `MemberKeysReplaced` | scans the new restore code: another of their devices made a new restore code, and the scan restores this device as a new device, which starts without the changes this device had not sent (§6.4) |
| `StorageSetupUndone` | sets the library's storage up again: a storage setup ended undone, and the Sync page shows it until the person dismisses it or starts another setup (§6.2) |
| `StorageSetupFinishing` | waits for the storage setup already running, then tries again (§6.2) |
| `DeletionUnfinished` | finishes removing the library from this device, which stopped partway (§6.4) |
| `DeletionSignInRequired` | signs in to the library's storage again, then removes the library (§6.4) |
| `UnfinishedInstallationUnreadable` | discards the unfinished join or restore of the library the category names, which this device can't read; listing what is under way fails until it is gone (§5.6) |

These show their line alone, without the detail (§9.6): `SyncUpdateRequired`,
`DeviceRemoved`, `CandidateImportInProgress`, `CandidateBeingIdentified`,
`FilesAlreadyImported`, `AlbumGone`, `MetadataTrackCount`, `TiedMediums`,
`ReleaseChangedSinceOpened`, `ChangedMeanwhile`, `InvalidEdit`,
`StorageLocationChangeUnsupported`, `KeychainEntitlementMissing`,
`SavedRestoreCodesFull`, `SavedRestoreCodeTooLarge`, `StoreOwnerAccount`,
`ReplacementUnfinished`, `RestoreRegistered`, `MemberKeysReplaced`,
`StorageSetupFinishing`, `DeletionUnfinished` and `DeletionSignInRequired`.
Every other category shows its detail under its line.

An error offers *Retry* only when making the same call again can get past
it (§9.6): `DatabaseBusy`, `CandidateBeingIdentified`,
`StorageSetupFinishing`, `ChangedMeanwhile`, `Import`, `Network`,
`KeyringLocked`, `Credentials`, `Membership`, a `GroupingBlocked` waiting on a
download, and a `Storage`, `StorageCheck` or `S3VersioningUnknown` whose
provider reason coven retries by itself: the network, throttling, a conflict,
an expired session, an object not there yet, a read a transfer changed, the
provider's quota, a local file, or catching up first. A cancellation, a
library that isn't there, and every other category offer none.

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
  the other carrier families MusicBrainz's and Discogs's format lists name,
  each format a person would call a different object.
- **Area:** an ISO 3166-1 country code, or a named region no code covers.
- **Discogs details:** what Discogs states about a pressing that no other
  field holds.

**Credit roles.** What an artist did on a work, release or track is one of
bae's roles or the catalog's own words, with the qualifiers the catalog states
beside it, in the order it states them. The roles bae names are a closed list,
keyed as stored: the *writing* roles writer, composer, lyricist, librettist,
translator, reviser, scriptwriter and reconstructor (MusicBrainz's `writer`
family, which make an artist a composer of a work), and arranger, producer,
engineer, mixer and mastering; and *performer*, which holds the instrument or
voice the catalog names (none for a bare performer). The instruments are
MusicBrainz's list of 1,062, each held by the id its relations carry, and the
voices are the 18 of MusicBrainz's `Vocal` attribute (the attribute itself, for
vocals of no stated kind, and lead, alto, soprano, choir, background, spoken
and the other kinds), held the same way; bae words each of them in every
language it ships. An instrument or voice neither list
holds is kept as the catalog wrote it. A qualifier bae names is
additional, assistant, associate, co-credit, executive, guest, solo,
pre-mastering or remastering. Every other role, and every qualifier bae has no
name for, is kept as the catalog wrote it. bae words the roles and qualifiers
it names in the viewer's language, a role followed by its qualifiers in
parentheses; the catalog's own words are shown as written. MusicBrainz's
`vocal arranger` is an arranger qualified by the vocals it arranged for
(`vocals` where it names none), an `instrument arranger` by its instruments,
and a `task` by its free text. A MusicBrainz relation that names an artist
without crediting work on the thing (a dedication, a commission, a premiere,
being named after the artist, a previous attribution) is not a credit.
A MusicBrainz `instrument` or `vocal` relation is a performer of each
instrument or voice it names by its id (the `Vocal` attribute itself for a
`vocal` relation naming none); one the lists do not hold is
kept as the credit prints it. Each relationship type page MusicBrainz publishes is
reproduced name for name with its reverse link phrase, which words a role bae
has no name for. Discogs's credits list is captured whole, and a table reads each of its roles: Written-By, Composed By,
Lyrics By and the like are the writing roles, Producer, Engineer, Mixed By and
Mastered By their roles (Co-producer, Executive-Producer and Remastered By with
the qualifier they state), and an instrument or vocal role a performer of the
instrument or voice on MusicBrainz's lists it is (Bass is bass guitar, Drums is
drums (drum set), Violoncello is cello, Backing Vocals is background vocals,
Choir is choir vocals, Solo Vocal is vocals qualified solo). Guest and Soloist
are a performer naming no part, qualified guest or solo, as Performer and
Musician are one with no qualifier. An instrument, voice or ensemble bae's lists
hold none for (Orchestra, Band, Ensemble, Drum Programming, Harmony Vocals) is
a performer of Discogs's own word, and every other role (Hosted By, Conductor,
Remix, Design, Narrator) is kept as Discogs wrote it, as is a role the list
does not hold.

**Audio codecs.** Lossless, stating a bit depth: FLAC, APE, ALAC, PCM, WavPack,
DSD. Lossy, stating a bitrate: MP3, AAC, Opus, Vorbis.
