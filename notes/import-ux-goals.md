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
3. **Manual search** - User searches by artist, album, catalog number

The media column shows artwork and scans—these help users identify the release (catalog numbers on spines, label logos, disc art).

### Stage 3: Curation

**User has**: A matched release
**User gets**: A library entry with chosen artwork and storage location

Users review the metadata, select cover art (remote or from local scans), choose where to store it. This is the curation step—making the release *theirs*.

### Stage 4: Completion

**User has**: Configured import settings
**User gets**: Release in their library

The folder is now a proper library entry with accurate metadata, chosen artwork, and organized storage.

## UI Serves the Journey

### Three-Column Layout

Maps to the user's mental context at each moment:

| Column | Question | Content |
|--------|----------|---------|
| Sidebar | "What folders do I have?" | Detected candidates |
| Middle | "What's in this one?" | Media assets (for identification) |
| Main | "What do I do with it?" | Current workflow step |

### Detail Header

Shows the selected folder name and current stage (Identifying/Confirming). Users always know:
- Which folder they're working on
- Where they are in the journey

### Media Assets Column

Not a file browser—it's identification context. Grouped by type:

- **Artwork** - Scans often show catalog numbers, barcodes, edition info
- **Audio** - Confirms track count and format
- **Documents** - Rip logs, NFO files with release info

### Visual Grouping

The detail pane (header + files + workflow) shares a background because it's all "about the selected folder." The sidebar is navigation; the detail pane is the workspace.

## Core Principle

**Informed, confident imports.** At each stage, users should feel certain about what's happening. They're not copying files—they're building a curated music collection.

The transformation from "folder" to "library entry" should feel deliberate and satisfying.
