# Scanning a music library over SMB: count round trips, not bytes

*Draft. Numbers measured 2026-10-25 on one library: 729 folders, 6,103 files,
90 GB, on an SMB share served by another Mac.*

Other titles to consider: "Our scan was waiting, not reading", "A file
operation costs 25 ms, so don't do it twice".

## The symptom

bae finds releases by scanning the folders a person points it at. On a local
disk a scan of this library was quick enough not to notice. On the network
share it took over five minutes, and every launch did it again: a rescan of a
library nothing had touched took nearly as long as the first scan.

The first guess was bandwidth. It was wrong. While the scan ran, bae used 3%
of a CPU and read about 3 MB/s. It wasn't busy reading; it was waiting.

## Where the time went

On this share, each file operation (an open, a stat, a directory listing)
costs 20 to 30 ms whatever its size, because each is a round trip to the
other machine. A scan that does those one at a time is bounded by the number
of operations, not by the bytes it reads.

The scan did a lot of them:

- **Every audio file was opened three or four times:** once by FFmpeg to
  check the audio plays, once to read the track number, once more to read the
  tags for the draft, and for ID3v2 and MP4 files, a second tag parse.
- **Every folder was listed one after another**, 729 of them.
- **Every launch threw it all away.** A rescan of an unchanged folder ended
  with the same rows it started with, so every probe, tag read, image check
  and CUE-sheet parse was work done to change nothing.

## What we changed

**A rescan takes what it stored.** For every file whose path, size and
modification time match the last scan, the rescan uses what that scan read:
its audio facts, tags, image check, parsed CUE sheet. It opens only files that
are new or changed. That alone took the launch rescan from 215 s (warm) and
361 s (cold) to 85 s.

**A first scan reads each track's tags once,** in one parse, and hands them to
the draft. FFmpeg still opens the file: that open is what says the audio
plays. Two opens per file instead of three or four.

**Eight folders at once.** With file reads out of the rescan, what was left
was 729 directory listings, each about 80 ms, in a row. Reading eight
top-level folders at once overlaps those waits. The rescan dropped to 18 s
cold and 11.6 s warm, the first scan to 151 s cold and 46 s warm. The stored
rows are identical to a one-at-a-time scan.

**Newest folders first.** Within a scan, the top-level folders go in order of
the latest date the file system keeps for each: when it was added, created
or modified. So an album the person just copied in shows up in the first
second or two instead of whenever its name comes up. Getting those dates
costs one round trip for the whole folder on macOS (`getattrlistbulk`), about
1 s on a cold share and nothing once cached.

What we learned about those dates, for anyone doing the same:

- `cp -p`, `ditto` and `rsync -a` keep a folder's old modified and created
  dates; a plain `cp -R` doesn't.
- macOS keeps a "date added" (when an item arrived in its folder), moves
  included, on local disks. Over SMB it reports none at all.
- Windows gives a copy a new created date. Linux has a created date only on
  file systems that record one.

So the order is a best guess, and a wrong guess only changes which rows show
first, never what the scan finds.

## Reading from the share while it's busy

Scanning wasn't the only place the share's latency showed. Opening an album
in the library stat'ed every original file on disk inside the database read,
to say whether the album could be played and uploaded. While an import was
reading from the same share, those stats came back late, and the album view
spun until the import finished.

Now the album view returns what the library recorded at import straight away
and asks the disk separately, off the database readers. Until the answer
arrives, the album reads as "checking its originals" rather than blocking.

## Measuring it

Three things made the numbers trustworthy:

- **A harness that logs every file operation and step,** tagged with the
  folder it served. It found the double reads directly, and also found a bug
  in itself: polling the scan's progress through the app's own API started a
  fresh rescan each time, doubling every early number.
- **Cold and warm runs reported separately.** The same build scanned this
  library in 46 s with a warm share cache and 151 s with a cold one. Comparing
  a cold "before" to a warm "after" would have flattered every change.
- **Remeasuring after each change.** One remeasure found a regression: a fix
  elsewhere removed a check that had been hiding the fact that multi-disc
  albums' disc folders never stored their tag reading, so every rescan read
  their tags again, 56 folders at about 280 ms each. The rescan had crept back
  to 33 s. A test now covers it, and the fix brought it back down.

## The result

The original code and today's, back to back on the same share:

| | Before | After |
|---|---|---|
| First scan | 311.6 s | 62 s |
| Launch rescan | 328.5 s | 20 s |
| Opens per audio file, first scan | 3–4 | 2 |
| Opens per unchanged file, rescan | 3–4 | 0 |

What's left of the rescan is listing the folders. Skipping folders whose
modification time hasn't changed would remove most of that, but a file
edited in place doesn't change its folder's time, so that one is a decision,
not a fix.

---

## Appendix: the full findings

Everything from the working findings page, so this post doesn't depend on it.
The library: 729 folders, 6,103 files, 90 GB on an SMB share; 276 albums
identified with MusicBrainz and Discogs both on.

### Where it landed

| | Before | After |
|---|---|---|
| Launch rescan of the library | 215–361 s | 12–20 s |
| First scan | 294–509 s | 46–151 s |
| Scan + identify, live catalogs | 68.6 min | 53.6 min |
| MusicBrainz requests | 2,665 | 1,875 |

Ranges are a warm and a cold share cache. Every identification change was
checked by replaying a recorded run: no album lost its match or its automatic
pick. The final live run matched the original on 250 of 262 albums, gained one
pick, and the rest were tie order or small reshuffles against live catalog
data.

### How it was measured

- **A profiling harness** (a throwaway branch, not on main) logged every
  catalog request, rate-limit wait, scan step and file read, each tagged with
  the album and the lookup that caused it.
- **Record and replay:** one live run saved every catalog answer. A replay
  serves them back instantly with rate limits off, so a code change shows its
  verdicts in minutes. A replay of the unchanged code reproduced all 262
  verdicts.
- **A timing model** replays the requests with their recorded response times,
  each catalog's pace (1.2 s; Wikidata 1 s), four albums at once and the
  rounds' waits. It runs about 20% fast against live runs because it doesn't
  model MusicBrainz's refusals; comparisons between plans are like for like.

### Scanning

| Media folder on the share | Before | After reuse | After folders at once |
|---|---|---|---|
| Launch rescan, cold | 361 s | 85 s | 18 s |
| Launch rescan, warm | 215 s | 65 s | 11.6 s |
| First scan, cold | 509 s | 354 s | 151 s |
| First scan, warm | 294 s | 243 s | 46 s |
| Opens per audio file, first scan | 3–4 | 2 | 2 |
| Opens per unchanged file, rescan | 3–4 | 0 | 0 |

Back to back on the same share, same conditions (original code, then main
`809b4e11c` twice): first scan 311.6 s → 62.0 / 62.2 s; launch rescan
328.5 s → 19.8 / 20.2 s.

- Newest folders first cost about 1 s on a cold share: a cold first scan took
  156 s against 151 s.
- The multi-disc regression: 56 disc folders re-read their tags on every
  rescan (16 s of a 33 s rescan); after the fix that step takes 0.3 s.

### Identification

**What was slow:** MusicBrainz, paced at one request per 1.2 s, was busy about
49 of 60 minutes. 933 of its 2,464 requests were lookups by Discogs link, all
made while reading Discogs pages, and none of them changed a verdict. Discogs
spent 151 of its 375 catalog-number searches on pages 2–10 of results for
short numbers other artists reuse.

| Timing model, 277 albums | Before | After |
|---|---|---|
| Whole library | 49.5 min | 33.9 min |
| Median per album | 32 s | 25 s |
| 90th percentile per album | 59 s | 36.5 s |
| One album on its own, median | 14.2 s | 12.9 s |
| MusicBrainz requests before verdicts | 2,338 | 1,194 |
| Discogs requests before verdicts | 1,582 | 1,347 |

- **A run reads only what decides the match:** a release's own page and its
  album's page, plus, for a MusicBrainz release, the Wikidata and Discogs
  pages its album statements come from. The rest is recorded as stopped short
  of, and every pick reads it before it applies, so drafts are unchanged.
- **An album's first year comes from its own album page.** Before, a Discogs
  release with a MusicBrainz twin took the twin's year while its siblings took
  the master's.
- **A complete read asks MusicBrainz about a Discogs release and its master in
  one request** (418 lookups became 239 requests).
- **Discogs catalog-number searches name the album's artist,** taken from the
  rows the run offers. MusicBrainz is searched once, unnarrowed, since it
  answers in one page. Pressing lookups name the label.
- **A release a Disc ID lookup returned is read from that answer.** The answer
  is the full release minus its annotation; a pick reads the full document.
- **An album's artwork is read four images at a time,** shared across albums.
  Vision read 40 images in 6.5 s one at a time and 3.1 s four at a time.
  Artwork reading was 19 of the 28 minutes of the library's local work.
- **An automatic pick's full read runs beside the queue.** It held one of the
  four run slots and showed the row as identifying. Now the next album starts
  at once, the row reads Identified while the pick is read, and Import asked
  for meanwhile waits for the read. Timing model: library 34.4 → 33.9 min; an
  album's row turns Identified 1–2 s sooner.

### Enrichment

**What was slow:** every automatic pick fetched the pictures of the Discogs
artists its draft credits: 185 artist lookups and 230 downloads per run,
waiting in Discogs's queue behind the lookups that identify albums, for
candidates most of which are never imported.

- **The import fetches the pictures** of the artists it creates, before it
  writes anything, and fails cleanly if one fetch fails ("Fetching artist
  pictures…", in all 28 locales).
- **Artist pictures are stored as 600 px JPEGs,** as the spec says. They were
  stored at full size.
- **A remote cover can't be stored without its bytes,** so the stored "assets
  prepared" flag is gone.

### Bugs found along the way

- **Two folders with identical files shared one draft** where file times are
  coarse (Linux, Windows). The second never stored its tags, so every rescan
  re-read them, importing it with its own tags failed, and its embedded cover
  was missing from the list. CI caught it.
- **The import list crashed** while rows landed during a scan: a group's
  spacer was a row of its own that came and went, and SwiftUI's list asserts
  when it scrolls while a position's row count changes. The spacer is now the
  row's own padding; a table-based list is being built to replace it.
- **A dev reset took over a minute** emptying its S3 prefix one object per
  request (883 objects, 71.5 s). `reset-bae.py` now empties it before
  launching the app, 1,000 keys per request.
- **The ranking ignores track titles.** It counts tracks and compares lengths,
  so a pressing listing a different song in one slot still ranks as a fit.
- **Spec and code disagree** in places the spec should be corrected: §3.4's
  "each catalog answers for itself and at its own pace" is true only of single
  lookups; the title search skips ISRCs on purpose, and a failed lookup counts
  as empty; switching a catalog off doesn't stop the walk asking it; a damaged
  image is checked by its first bytes, not decoded.
- **Seen, not changed:** a track whose tags can't be read stores nothing, so
  every rescan reads it again. `cover_art.rs` mixes the cover model, a disk
  cache and fetching, and fakes a 404 status for "no image".

### What's left is a decision

MusicBrainz's pace now sets the library time: about 1,875 requests at one per
1.2 s, plus 286 "server busy" refusals in the live run, each of which holds
every MusicBrainz request.

- **Automatic picks drafting from what the run read.** About 290 MusicBrainz
  requests per run would move off the pick. It's blocked by artist identity: a
  Discogs pick's artists get their MusicBrainz ids from the complete read, and
  without them the same artist imported later from MusicBrainz becomes a
  second row.
- **The notes tie-break.** It decides 29 of 260 albums. Some picks look right
  (a "made in West Germany" note for a folder that says so), some look wrong
  (promo copies, a later repress matched through the word "Side"). Without
  it, 22 albums lose their automatic pick.
- **Pacing.** Requests are spaced a sixth under each catalog's published rate,
  on purpose (`707ef15fd`), after timing jitter tipped Discogs over its limit.
- **Rescans by folder date.** Skipping the listing of folders whose
  modification time hasn't changed would bring a rescan near zero, but would
  miss files edited in place.

### Commits on main

| Commit | What |
|---|---|
| `2cfcc24a5` | Take an album's first year from its own album page first |
| `2ed36e6f1` | Read only what identification decides by |
| `55d6b7272` | Fetch Discogs artist pictures at import, not at the pick |
| `ad486ad7a` | Never store a catalog's cover without its bytes |
| `19be5201e` | Say the import is fetching artist pictures while it does |
| `0cebb8a7c` | Store Discogs artist pictures as the JPEG every image is |
| `d9a2bcdcf` | Take a rescan's file facts from what the last scan stored |
| `21755e7f8` | Read each track's tags once in a first scan |
| `1f03c3dcf` | Read several of a watched folder's folders at once |
| `1ff446947` | Ask MusicBrainz about a Discogs release and its master in one request |
| `bd7b221f4` | Search catalog numbers on Discogs under the album's artist |
| `717699ea9` | Keep each release's tag reading when its hash already has a draft |
| `8f16fe9b4` | Read a release a Disc ID lookup returned from that answer |
| `5c69bdeac` | Read images four at a time across all extractions |
| `544d670e0` | Hold every read in the folder-removal extraction test until it is cancelled |
| `cb66dfa73` | Give each test folder its own file times |
| `feb7da557` | Read an unattended pick beside the queue, and show it identified meanwhile |
| `baf54132e` | Walk a watched folder's newest folders first |
| `0d6a2e550` | Read a folder's dates in one place for the list and the walk |
| `0c2d54cc6` | Empty the dev profile's storage prefix in reset-bae.py, not the app |
| `9506d9fc1` | Put the import list's air inside its row |
| `809b4e11c` | Keep a rescan from reading multi-disc albums' tracks again |
| `707bd0f02` | Show an album without waiting on the disk its originals sit on |
