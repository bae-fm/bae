# bae User Stories

Cross-platform reference for building Windows and Linux apps. Derived from the macOS native app (bae-macos).

---

## First Launch

### Create Library
- User sees a welcome screen with "Create new library" button
- Clicking it creates a new library and transitions to the main app
- Shows a progress spinner during creation
- Displays inline error (red text) if creation fails

### Restore from Cloud
- User connects another of their devices to an existing library by pasting a restore code generated on a device that already has the library
- The restore code carries the cloud home coordinates and the owner's signing key; for OAuth providers the user signs into their own cloud account during onboarding
- Shows progress spinner during restoration
- Displays inline error if restoration fails (bad or truncated code, wrong version)

### Unlock Library
- If the encryption key is missing from the keyring (e.g. after a reboot on some systems), the user is prompted to enter it
- Shows a lock icon, library name, and key fingerprint
- Input accepts a 64-character hex string; unlock button is disabled until valid
- Displays error if the key is wrong

---

## Main Interface

### Window Chrome
- Title bar blends with app background (no standard macOS title bar chrome)
- No library name in the title bar — window title is just "bae" or empty
- Minimal toolbar: only the section switcher and search bar

### Section Switching
- Segmented control at the top to switch between "Library" and "Import"

### Settings Access
- Settings opens via Cmd+, (standard macOS convention)
- Also accessible from the app menu (bae → Settings)
- Gear icon in the toolbar, positioned to the right of the search bar (rightmost element)

### Search
- Search bar with placeholder "Artists, albums, tracks"
- Typing a query shows search results replacing the album grid
- Results are grouped into Artists, Albums, Tracks sections
- Clicking an artist shows all their albums
- Clicking an album opens the album detail panel and clears search
- Clicking a track plays it immediately
- Empty query returns to the album grid
- Shows "No results" when nothing matches

---

## Album Grid

### Header Bar
- "Library" headline text on the left
- Sort/filter controls on the same line, right-aligned
- View mode picker: Albums / Artists
- Sort field dropdown: Title, Artist, Year, Date Added
- Sort direction toggle: ascending / descending (arrow icon)
- Multiple sort criteria can be stacked (add/remove)

### Layout
- Responsive grid of album cards, ~160px wide
- Each card shows: album art, album title (1 line), artist names (1 line), year

### Interactions
- Single-click selects the album and opens the detail panel on the right
- Double-click plays the entire album
- Selected album has an accent-colored border

### Album Card Hover Menu
- Ellipsis button (three dots) appears in top-right corner of album art on hover
- Semi-transparent dark background on the button
- Dropdown menu with:
  - "Play" — plays the album
  - "Add to Queue" — appends all tracks to queue
  - "Add Next" — inserts all tracks after the current track
- Right-click on the card shows the same menu as a context menu

### Empty State
- "No albums" message with a prompt to import music

---

## Album Detail Panel (Right Sidebar, ~450px)

### Header
- Album art (100x100px) with context menu: "Change Cover..."
- Album title, artist names, year
- Release metadata: format, label, catalog number, country (compact line)
- Play button: starts playing the album
- Close button (X) in the top right

### Release Picker
- If the album has multiple releases, a segmented control lets the user switch between them
- Switching changes the track list below

### Tracks
- Ordered by disc number + track number
- Each row: track number (monospaced, right-aligned), title, duration (monospaced, right-aligned)
- Multi-disc albums show disc.track format (e.g. "2.3")
- Compilation albums show per-track artist names
- Double-clicking a track plays the album starting from that track

### Files (Collapsible)
- Lists each file in the release: original filename, file size, content type

### Storage
- **Local library**: shows the file path on disk (from `unmanaged_path`)
- **Synced library, pinned**: "Pinned for offline" label with an "Unpin" button
- **Synced library, not pinned**: "Cloud" label with a "Pin for offline" button
- Shows progress bar while pinning/unpinning is in progress

### Change Cover Overlay (Modal, ~500x450px)
- Shows remote cover art candidates in a grid (120x120px thumbnails)
- Shows release image files as an additional source
- Clicking a thumbnail selects it as the new cover
- "Refresh" button to re-fetch remote sources
- "Done" button or Esc to close

---

## Now Playing Bar (Bottom of Window)

### Track Info (Left)
- Album art (48x48px) with placeholder if missing
- Track title (bold, 1 line)
- Artist names (secondary, 1 line)

### Transport Controls (Center)
- Previous, Play/Pause, Next buttons
- Play/Pause toggles icon between play and pause
- Progress bar: current time (MM:SS) — slider — total duration (MM:SS)
- Dragging the slider scrubs; position updates during drag but seek happens on release

### Secondary Controls (Right)
- Repeat button: cycles None → Album → Track, icon and color change per mode
- Queue button: toggles the queue popover, accent color when open
- Volume icon + slider (0–100%)

### Keyboard Shortcuts
- Space: play/pause
- Cmd+Right: next track
- Cmd+Left: previous track
- Cmd+R: cycle repeat mode

---

## Queue (Popover)

### Trigger
- Queue button in the Now Playing bar toggles a popover
- Popover anchored to the queue button (~350px wide, ~500px tall)
- Queue button shows accent color when popover is open

### Header
- "Queue" title
- "Clear" button (disabled when empty)

### Now Playing Section
- Shows current track: album art (48x48px), "Now Playing" label, title, artist

### Queue Items
- Each row: album art (40x40px), track title, album title, duration
- Hover: play button overlay appears on the thumbnail; remove button (X) replaces the duration
- Double-click: skips to that track
- Right-click context menu: "Remove from Queue"

### Drag-and-Drop Reordering
- Drag any item to reorder
- Blue insertion line shows where the item will land
- Dragged item fades to 30% opacity
- Drop cursor shows as "move" (not copy)

### Empty State
- "Queue is empty" with "Play an album to fill the queue" message

---

## Import Workflow

### Entry Points
- Switch to "Import" section via the segmented control
- File → Import Folder... (Cmd+I) opens a folder picker
- Drag-and-drop a folder onto the app window

### Empty State
- Large plus icon with "Scan a folder to import music"

### Scan Results (Left Panel, ~200–350px)
- Plus button to scan additional folders
- Overflow menu: "Clear All", "Clear Completed", "Clear Incomplete"
- Incomplete candidates are grouped at the end of the list
- Each candidate row shows:
  - Folder icon; hover shows "Reveal in Finder" tooltip; click opens the folder in Finder (with active/pressed state)
  - Folder name with middle truncation (not tail truncation)
  - X button on hover (right side) to remove the candidate
  - Status icon: folder (pending), spinner (importing), green checkmark (done), red warning (error)
  - Progress bar with percentage during import
  - Error message in red if tracks are incomplete or images are corrupt
  - Candidates with bad audio or corrupt images are dimmed and unselectable
- Candidate rows have generous vertical padding/spacing between them

### Metadata Search (Right Panel)

#### Candidate Header
- Release folder name as title (selectable text); hovering shows tooltip with full path on disk
- Metadata summary: track count, audio format, total file size (plain text, no icons)
- Import status badge

#### File Pane and Search Form (Vertical Split)
- File pane on top, search form + results on bottom (vertical split, not horizontal)

#### File Pane (Collapsible)
- Audio section: CUE+FLAC pairs or individual track files with sizes
- Images section: grid of image thumbnails (120x120px); click to open gallery
- Documents section: list of text files; click to open viewer

#### Search Form (Tabbed)
- General tab: Artist + Album fields, "MusicBrainz" and "Discogs" buttons
- Catalog # tab: catalog number field, "Search" button
- Barcode tab: barcode field, "Search" button
- Discogs button is disabled if no Discogs API token is configured

#### Search Results
- Each result: title, artist, year, format, label, source ("MusicBrainz" or "Discogs")
- "Import" button per result (disabled while importing)

### Image Gallery Overlay (~700x550px)
- Shows images full-size, centered
- "X of Y" counter in toolbar
- Left/right arrow buttons (and arrow keys) to navigate
- "Done" button or Esc to close

### Document Viewer Overlay (~600x500px)
- Shows text file contents in monospaced font
- Fully selectable, scrollable
- "Done" button or Esc to close

---

## Settings Window (~500x400px)

### Library Tab
- Editable library name field; shows "Saved" briefly after saving
- Library ID: read-only, selectable, with a copy button
- Library path: read-only, selectable

### Discogs Tab
- If no token: secure input field with show/hide toggle, "Save" button
- If token exists: masked display with show/hide toggle, "Remove" button
- Status message after save/remove (auto-clears after 2 seconds)

### Sync Tab

#### Status
- Configured: yes/no
- Last sync timestamp
- Error message if any
- Device count
- "Sync Now" button (shows "Syncing..." while active)

#### Cloud Provider
- If connected: provider name, account, bucket/region/endpoint details, "Disconnect" button
- If not connected: provider picker with options:
  - S3 / S3-compatible: bucket, region, endpoint, key prefix, access key, secret key fields + Save
  - OAuth providers (Google Drive, Dropbox, OneDrive): "Connect" button, opens browser
  - iCloud Drive: "Use iCloud Drive" button

#### Identity
- Public key: truncated display with copy button ("Not generated" if none)

#### Connect Another Device
- "Generate Restore Code" button produces a restore code for connecting another of your devices
- Generated restore code: truncated display with copy button
- On the connecting device, the restore code is entered on the welcome screen (see Restore from Cloud)

### About Tab
- App name, version, build number
- "Check for Updates" button

---

## Menus

### File Menu
- Import Folder... (Cmd+I): opens folder picker, scans, switches to Import section
- Check for Updates...: opens Sparkle update dialog

### Playback Menu
- Play / Pause (Space)
- Next Track (Cmd+Right)
- Previous Track (Cmd+Left)
- Cycle Repeat Mode (Cmd+R)

---

## System Integration

### Media Controls
- Hardware media keys (play/pause, next, previous) control playback
- OS media widget (Control Center on macOS) shows: track title, artist, album, artwork, playback position
- Remote seek via media widget

### Clipboard
- Copy operations for: library ID, public key, restore code

### Auto-Updates
- Checks for updates on launch (Sparkle on macOS)
- Manual check via About tab or File menu

### Appearance
- Dark mode enforced
- Minimum window size: 900x600

### Drag-and-Drop
- Drop a folder onto the main window to scan it for import
