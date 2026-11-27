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

## Noticing changes on a share

A local folder tells you when something changes in it; on macOS bae watches
each folder with FSEvents and re-reads only what moved. A network mount tells
you less. We checked, with a scratch folder on the share: creating, renaming
and deleting files from this Mac produced the same events on the SMB mount as
on a local disk. A change made on the server itself, or from another computer,
produces nothing here, because the mount only reports what this machine did.

So a watched folder on a share gets both: the watch, which makes this Mac's own
edits show up at once, and a check every 15 minutes that looks for folders
that changed and re-reads them. That check is what notices an
album someone copied onto the server from elsewhere. The import list marks
such a folder and says how often it's checked.

Our own spec had this wrong for a week: a rewrite said a network folder "has
no watch" and relies on the 15-minute check alone. Deleting a few broken
albums from the share and watching them vanish from the list within a minute,
not fifteen, is how we noticed.

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

---

## Notes: pictures from a share (2026-11-24)

*Notes, not yet written up. Facts come from plans/picture-pipeline.md (profiling of 2026-11-23, the survey of every picture path, the person's decisions, and the libvips proof of 2026-11-24) and bae's commits of 2026-11-24. The proof's numbers were taken on one shared development Mac while other builds ran, and on an arm64 Android emulator on the same Mac.*

### The symptom

A person adds a folder on an SMB share to import. Selecting a release in the
import list left the main pane blank for seconds, and the scan's progress bar
stopped moving while it was blank.

### Where the time went

A sample of the running app showed 13 of Swift's shared threads, the pool that
runs every piece of Swift async work, inside ImageIO, each copying bytes out of
a memory-mapped file on the share. The main thread was idle. Rust's scan,
import and identification threads were parked, waiting for work. Nothing was
busy; the pool was full of picture reads, and everything else waited behind
them, including the release pane's data and the scan's progress.

Three things put the reads there:

- **One click started about fifteen full reads.** The release had fifteen scans
  in its folder (booklet pages, back, disc and obi), PNGs of 25 to 63 MB each,
  about 500 MB in all. The import pane read every one of them whole from the
  share to draw a 256 px tile.
- **ImageIO memory-maps the file.** `CGImageSourceCreateWithURL` maps it, so
  every read is a page fault on whichever thread copies the bytes. On a share,
  each fault waits on the network, nothing can cancel it, and a share that
  drops or a file that changes ends in a crash instead of a read error. The
  decode ran inside `Task.detached`, which is the shared pool.
- **PNG has no cheap small decode.** A JPEG decodes straight to a half, a
  quarter or an eighth of its size. A PNG must be read and decoded whole
  before it can be shrunk, so a 256 px tile of a 50 MB PNG costs as much as
  the full picture.

A debugger trace found one more: a cancelled load kept its decode running and
was forgotten, so the next request for the same picture started a second
decode of the same file. The import list cancels and restarts a row's cover
load on every redraw, so during a scan some covers decoded twice.

### Every picture path, before

A survey of every place bae shows a picture found three decoders (Rust's
`image` crate in core, ImageIO on the Mac, BitmapFactory on Android), and the
apps decoding nearly every picture themselves:

- A folder picture was read by the app for its thumbnail, read whole again by
  core for the barcode and text pass, and read a third time to make the stored
  cover.
- Orientation was handled four ways, and pictures were shrunk three ways.
- The Mac had five decoded caches, Android two plus two byte caches, and Now
  Playing on the Mac kept its own one-entry cache beside them.
- Every library cover is stored at 600 px, so several places drew it upscaled:
  the Mac album panel at 680 px, Android's full player at 777 to 1,281 px, and
  every full-size viewer.

### The plan

One path for every picture:

- **Core reads each original once,** in plain chunks, never memory-mapped,
  checking between chunks whether anyone still wants the picture. A few reads
  run at once per volume, fewer on a network share, and what is on screen
  reads before what is prefetched.
- **Core makes the size the app asks for,** rounded up to the next of four
  steps (192, 384, 768 and 1536 px), applies the orientation and converts the
  colour, keeps the copy on disk, and sends the app its encoded bytes. Requests
  for the same picture and size share one job, and the job stops when its last
  requester goes away.
- **The full-size viewer zooms from tiles** that core cuts, 512 px each, on the
  Mac and on Android alike, so memory holds about a screen's worth of the
  picture instead of all of it.
- **Each app keeps one image store,** which decodes only core's small copies,
  on a queue of its own.
- **Covers name their source picture,** with synced 768 px and 1536 px copies
  made from it, and every image file of a release gets a synced 384 px preview
  of about 30 KB. A release with fifteen scans syncs about 450 KB of previews
  beside about 500 MB of originals, so any device can show its pictures without
  downloading the originals.

### The apps' image stores (landed 2026-11-24)

The first step changed only the apps, and it removes the stall on its own:

- **The Mac decodes on a queue the image store owns,** at most four at once,
  never on Swift's shared threads. A slow read now holds one of the store's
  four decode slots instead of the pool every other piece of async work needs.
- **A started decode stays joinable** after its last waiter cancels, so asking
  again decodes once. An import row keeps its cover load running across
  redraws.
- **One cache per app.** The Mac's five decoded caches became one of 256 MB.
  Android's became one decoded cache of 64 MB with shared loads and three
  decode threads, and its byte caches went, since core already keeps the files.
- **Now Playing and the lightbox go through the store.** The lightbox reads the
  original only when a zoom asks for full resolution.

### Picking the decoder: libvips

Moving every decode into core needs a decoder that reads a file once, top to
bottom, shrinking as rows arrive, and that cuts tiles. Core's `image` crate
reads JPEG, PNG, GIF and WebP but not TIFF, which many rips use for their
scans. libvips does all of it: its thumbnail operation streams the file, its
`dzsave` cuts tile pyramids, and it handles orientation and colour profiles.
The cost is a C library with glib underneath, which bae has to build for the
Mac and for every Android architecture and ship the way it ships FFmpeg. So
the first step was a proof that it builds and runs on Android.

It does. libvips 8.18.7 builds from source for the Mac (arm64) and for both
Android ABIs the app ships (arm64-v8a and x86_64), with JPEG, PNG, TIFF, WebP,
GIF, EXIF orientation and colour profiles. It reads through a custom source
that calls Rust's `read` and `seek`, so nothing is memory-mapped, and that read
callback is where a cancellation check goes.

The test picture was a 6,000 by 5,600 PNG of 55.7 MiB with the "rotate
clockwise" orientation flag. Decoding it whole would take 96 MiB. Each job ran
in its own process, and peak memory includes the test binary and libvips:

| Job | Mac | Android arm64 emulator |
| --- | --- | --- |
| 256 px JPEG thumbnail, turned upright | 62 to 68 MiB, 0.48 s | 49 MiB, 0.50 s |
| 512 px tile pyramid, 14 levels, 191 tiles | 78 to 84 MiB, 0.46 s | 53 MiB, 0.57 s |

The emulator runs arm64 code through the Mac's virtualisation, so its time
shows that the code runs, not how fast a phone is.

What it costs to ship:

- **Download size:** the Mac app grows by 4.1 MiB compressed (FFmpeg takes
  3.4 MiB), and the Android app by 5.1 MiB compressed per ABI (FFmpeg takes
  2.2 MiB).
- **Licences:** libvips, glib, libexif and, on Android, libiconv are LGPL, so
  bae ships them as separate shared libraries a person can replace, with their
  licence texts and a source offer, as it does FFmpeg. Everything else is
  permissive. No GPL or patent-encumbered codec is in the build, which leaves
  out HEIC for now.

One constraint for tiles came out of the proof: a picture stored sideways
(orientations 5 to 8) can't be turned a quarter turn while it streams without
holding all of it, so the tile service turns each tile instead of the whole
picture.

### What's left

- **The picture service:** core reads, sizes and keeps the copies, and the
  apps ask it through one call, `picture(source, pixels)`.
- **Cover sources and synced copies:** covers name their source picture and
  get their 768 px and 1536 px copies, and release image files get their
  384 px previews.
- **The tiled viewer** on the Mac and on Android.

### Commits on main

| Commit | What |
|---|---|
| `d1e317f517` | Decode pictures on the image store's own queue |
| `9f23c5914d` | Keep an import row's cover load running across redraws |
| `3f89609c10` | Draw Now Playing's cover through the image store |
| `0696e974b4` | Decode the Mac lightbox's screen-fit picture through the image store |
| `679b2d1629` | Give Android's image store one cache, shared loads and a bounded decoder |
| `6790ef6f16` | Make each image store own its decode queue |
| `1c38406eed` | Build the import cell's cover-load accessor only for tests |

The libvips proof is on branch `libvips-proof` and lands with the picture service, its first caller.
