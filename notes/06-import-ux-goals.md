# Import Flow UX Goals

## The Transformation

Import transforms **folders** into **identified, curated releases**.

```
Folders on disk  →  Potential releases  →  Matched releases  →  Library entries
   (unknown)         (detected)             (identified)         (curated)
```

Users start with "I have a bunch of folders." They end with "These are now part of my music collection with accurate metadata and artwork."

## Journey Stages

### Stage 1: Detection

**User has**: Folders somewhere on disk
**User gets**: A list of potential releases

The sidebar populates with detected folders that look like music releases. Each folder becomes a candidate. The user now sees what they're working with.

### Stage 2: Identification

**User has**: A folder that might be a release
**User gets**: A confirmed match to a known release in MusicBrainz/Discogs

This is where folders become releases. Three paths:

1. **Disc ID lookup** - Automatic match via CUE file fingerprint
2. **Multiple matches** - Disc ID matched several editions; user picks the right one
3. **Manual search** - User searches by artist/album, catalog number, or barcode

The file display shows artwork and scans—these help users identify the release (catalog numbers on spines, label logos, disc art).

### Stage 3: Curation

**User has**: A matched release
**User gets**: A library entry with chosen artwork

Users review the metadata and select cover art (remote or from local scans). This is the curation step -- making the release *theirs*. Storage is not a choice at import time; it is determined by the library type (local vs synced).

### Stage 4: Completion

**User has**: Configured import settings
**User gets**: Release in their library

The folder is now a proper library entry with accurate metadata, chosen artwork, and organized storage.

## Folder Import

The import section has one flow: **Folder**. It produces candidates in a sidebar list. The identification, curation, and completion stages run once a candidate is selected.

User selects a directory on disk. The folder scanner walks the tree and emits candidates — one per detected release (leaf directory with audio files).

## UI Serves the Journey

### Layout

Two essential areas:

| Area | Question | Content |
|------|----------|---------|
| Sidebar | "What folders do I have?" | Detected candidates (folder icon + name) |
| Main pane | "What's in this one, and what do I do with it?" | File display + search + results |

The macOS app uses two columns with files displayed inline in the main pane above the search form. A three-column variant where files get their own middle column also works — the key is that the file display is visible alongside the search workflow.

### Candidate Sidebar

Shows detected folders as simple folder items. No track counts or format strings — just the folder name and a status icon (pending/importing/done/incomplete). Incomplete candidates are dimmed and not selectable.

### File Display

Not a file browser — it's identification context. Grouped by type:

- **Audio** — CUE+FLAC pairs or track files. Confirms track count and format.
- **Images** — Scans often show catalog numbers, barcodes, edition info on spines and disc art.
- **Documents** — Rip logs and text files with release info.

### Search Form

Three search modes (tabs):
- **General** — artist + album fields
- **Catalog Number** — single field (e.g., "WPCR-80001")
- **Barcode** — single field (e.g., "4943674251780")

Catalog number and barcode searches are how users identify Japanese pressings, limited editions, and other releases where artist/album text search returns too many or too few results.

### Visual Grouping

The main pane (header + files + search + results) shares a background because it's all "about the selected folder." The sidebar is navigation; the main pane is the workspace.

## Core Principle

**Informed, confident imports.** At each stage, users should feel certain about what's happening. They're not copying files—they're building a curated music collection.

The transformation from "folder" to "library entry" should feel deliberate and satisfying.
