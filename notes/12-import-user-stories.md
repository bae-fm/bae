# User Stories

Cross-platform reference for implementing the app UX. These stories describe what the user sees and does, not how to implement it. Each platform (macOS, Linux, Windows) implements using native patterns.

See also: [06-import-ux-goals.md](06-import-ux-goals.md) for the design rationale, [07-import-state-machine.md](07-import-state-machine.md) for the state machine, [11-app-layout.md](11-app-layout.md) for the top-level layout.

---

## Candidate Sidebar

### US-1: Candidates are presented as folders

When a user scans a folder, each detected music directory appears in the sidebar as a folder item showing:
- Folder icon
- Folder name (filesystem basename, e.g., "1990 - People's Instinctive Travels")

No track count, no format, no file size in the sidebar. The sidebar is a list of folders — nothing more.

### US-2: Candidate status indicators

Each candidate shows a status icon replacing the folder icon:
- **Pending** — folder icon (default)
- **Importing** — spinner/loading indicator
- **Imported** — checkmark (green)
- **Incomplete** — folder icon, but the entire row is dimmed

### US-3: Incomplete candidates are blocked

If a folder contains corrupt or incomplete files (bad audio headers, 0-byte files, truncated FLACs, corrupt images), the candidate is marked incomplete. An incomplete candidate:
- Shows dimmed text
- Is not selectable (clicking does nothing)
- Shows a sub-line explaining the problem, e.g.:
  - "2 of 14 tracks incomplete"
  - "1 corrupt image"
  - "3 of 10 tracks incomplete, 2 corrupt images"

The `bad_audio_count` and `bad_image_count` fields on the candidate provide this data. Total audio count comes from `track_count`.

### US-4: Remove candidates

A pending or incomplete candidate shows a remove/X button on hover. Clicking it removes the candidate from the list. Imported or importing candidates cannot be removed.

### US-4a: Add more folders from the sidebar

The candidate sidebar has an "Add Folder" button (e.g., a + icon at the top or bottom of the list). Clicking it opens the file picker to scan another folder. The new candidates append to the existing list. This lets users build up a batch without clearing previous scans.

### US-4b: Clear candidates

The candidate sidebar has a way to clear candidates in bulk. Options:

- **Clear all** — removes all candidates from the list (resets to empty state)
- **Clear completed** — removes only imported candidates, keeping pending/incomplete ones
- **Clear incomplete** — removes only incomplete (corrupt/bad file) candidates

This could be a context menu, a dropdown button, or individual clear options. The key is that users can clean up the list without removing candidates they're still working on.

---

## File Display

### US-5: File pane shows categorized release contents

When a candidate is selected, the main content area shows the folder's files grouped into sections:

**Audio** — collapsed by default (disclosure group). Either:
- CUE+FLAC pairs: shows each pair (cue name + flac name + combined size + track count)
- Track files: shows each audio file (name + size)

Audio is collapsed because it's rarely needed for identification — images and documents are more useful.

**Images** — always visible. Artwork files shown as a thumbnail grid (120px). These help identify the release — users look at scans for catalog numbers, spine text, disc art.

**Documents** — always visible. Text/log/nfo files (name + size). Clickable (see US-5b). Rip logs and NFO files often contain release information.

Each section header shows the section name and count. Empty sections are hidden.

### US-5a: Image gallery lightbox

Clicking an image thumbnail in the file pane opens a full-size gallery view as a centered overlay with a dark backdrop. The gallery:
- Shows the image at full resolution, centered
- Has left/right navigation arrows (or keyboard arrows) to cycle through all images
- Shows the filename below the image
- Has a "Done" button or Escape key to dismiss
- Clicking the dark backdrop also dismisses it

The gallery helps users identify releases by examining scans of spines, disc art, label logos, and barcodes.

### US-5b: Document viewer

Clicking a document file (log, nfo, m3u, cue, txt) in the file pane opens a viewer as a centered overlay with a dark backdrop. The viewer:
- Shows the file content in a monospaced font
- Text is selectable
- Shows the filename in the header
- Has a "Done" button and Escape key to close
- Clicking the dark backdrop also dismisses it
- Tries UTF-8 first, falls back to Shift-JIS (common for Japanese rip logs)

NFO and log files often contain release info (catalog numbers, edition notes, rip details) that help with identification.

### US-6: Empty file display

If no files are found in a category, that section is hidden (not shown as empty).

---

## Search Form

### US-7: Tabbed search with three modes

The search form has three tabs:

**General** (default) — two text fields:
- Artist (pre-filled from folder metadata if available)
- Album (pre-filled from folder name)
- Separate MusicBrainz and Discogs buttons (Discogs requires API key)

**Catalog Number** — single text field:
- Placeholder: "e.g. WPCR-80001"
- Single Search button (searches MusicBrainz; also Discogs if key is configured)

**Barcode** — single text field:
- Placeholder: "e.g. 4943674251780"
- Single Search button (same behavior as catalog)

### US-8: Search pre-fills from folder name

When a candidate is selected, the General tab's Album field pre-fills with the folder name (the `albumTitle` from the candidate). The Artist field is empty (artist detection from tags is not reliable enough to pre-fill).

### US-9: Search results

Search results appear as a list below the search form. Each result shows:
- Title
- Artist (if present)
- Year (if present)
- Format (if present, e.g., "CD", "Vinyl")
- Label (if present)
- Source indicator ("MusicBrainz" or "Discogs")
- Import button

### US-10: Import a search result

Clicking "Import" on a search result starts the import process. The candidate's status changes to "Importing" with a progress indicator. The user can switch to other candidates while the import runs.

---

## Layout

### US-11: Resizable file/search split

The file pane (top) and search area (bottom) in the import main content are separated by a draggable divider. Users can drag the divider to give more space to either the file display or the search results, depending on what they're focused on.

### US-12: Overlays dismiss on click-outside

All overlay panels (image gallery, document viewer, cover picker) dismiss when the user clicks on the dark backdrop area outside the panel. No need to find and click a close button — just click away. Escape key also works.

### US-13: No modal sheets

Content viewers (image gallery, document viewer, cover picker) are presented as centered overlays with a dark backdrop, not as platform modal sheets. This avoids:
- Sheet size jumping/animation on open
- Modal blocking (sheets prevent interaction with the parent window)
- Inconsistent dismiss behavior

### US-14: Inline album detail

Clicking an album in the library grid shows its detail in a right-side panel (fixed width). The album grid stays visible on the left so the user can quickly switch between albums. An X button in the detail header closes the panel and returns to the full-width grid.

The grid keeps its column layout stable — it just narrows when the detail panel opens, reflowing columns as needed.

---

## Bridge API

These are the bridge methods that all platforms call. The bridge wraps bae-core and exposes a platform-agnostic interface via UniFFI.

### Search methods

```
search_musicbrainz(artist, album) -> [MetadataResult]
search_discogs(artist, album) -> [MetadataResult]
search_by_catalog_number(catalog, source) -> [MetadataResult]
search_by_barcode(barcode, source) -> [MetadataResult]
```

`source` is "musicbrainz" or "discogs". For catalog/barcode, call with each source separately or call MusicBrainz first and Discogs if a key is available.

### File methods

```
get_candidate_files(folder_path) -> CandidateFiles
```

Returns categorized files for a scanned folder:
- `audio`: either CUE+FLAC pairs or individual track files
- `artwork`: image files
- `documents`: text/log/nfo files
- `bad_audio_count`, `bad_image_count`: corrupt file counts

### Candidate fields

```
BridgeImportCandidate:
  folder_path: String       — full path
  album_title: String       — folder basename (used as display name)
  artist_name: String       — empty at scan time
  track_count: u32          — from CUE parsing or file count
  format: String            — "CUE+FLAC" or "FLAC"
  total_size_bytes: u64     — sum of audio file sizes
  bad_audio_count: u32      — corrupt/incomplete audio files
  bad_image_count: u32      — corrupt image files
```

---

## What's NOT in scope (yet)

These are features not yet in the bridge or other platforms:

- **Disc ID auto-lookup** — automatic matching via CUE file fingerprint (Stage 2 path 1 in the UX goals doc). Requires the full state machine.
- **Confirm step** — review metadata and select cover art before importing. Currently we go straight from search result to import.
- **Auto-advance** — after importing one candidate, automatically select the next pending one.
- **Drag-to-reorder candidates** — reordering the sidebar list.
