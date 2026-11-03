# The words bae uses

What each word on screen means, so every screen, platform and locale says the
same thing the same way. A new word, or a new meaning for one, goes here in
the commit that puts it on screen. [bae.md](bae.md) states what bae does; this
file states what it calls things.

## Capitalization

Sentence case everywhere: capitalize the first word and proper names only
("Rename library", "Sign in to Dropbox…", "Show in Finder"). This covers
buttons, titles of windows, sheets, dialogs and pages, settings rows, field
labels and notes, on every platform. The one exception is the macOS menu
bar's menus and items, which use title case as every Mac app's do ("Rename
Library…", "Open Library"). "bae" itself is always lowercase.

## The library and where it lives

- **Library.** One catalog and its files. A synced library is kept in
  storage and is the same on every device that has it; a library not synced yet
  lives only on the Mac that made it. Say "the library", never "the cloud
  library" or "your local library": there is one library, wherever its copies
  are.
- **Synced / not synced.** Whether the library is kept in storage. A library
  starts not synced and becomes synced when sync is set up for it.
- **Storage.** Where a synced library is kept: the Dropbox or OneDrive
  folder, the iCloud container, or the S3-compatible bucket it syncs through.
  Always "storage" on screen, never "the cloud" or "cloud home". "The library
  will remain in storage and on your other devices."
- **This device's copy.** What the library has on this phone or Mac: its
  catalog and the files downloaded here. Removing the library from this device
  deletes this copy; the library itself is untouched.

## Sync

- **Sync.** The page and the Library row about keeping this device's copy and
  storage in step. On screen it is "Sync", never "Cloud home".
- **Signed out.** This device has no working sign-in for storage: none is
  saved, or the one it had expired. The status names the storage ("Signed out
  of Dropbox"), on the Library page's Sync row too, and so does the button:
  "Sign in to Dropbox…" (Android "Sign in to Dropbox"), and for OneDrive and
  iCloud the same. S3 has no sign-in; its button is "Enter access key…".
- **Storage access was denied.** This device is signed in, but that account
  isn't allowed into the storage folder or bucket. Signing in again doesn't fix
  it; the person who manages the storage does.
- **Sync failed.** Sync stopped on an error with no other name, or on a
  file that can't be uploaded because it is missing or was changed; then
  the Sync page names the file. The Sync page says what went wrong and
  offers the fix where there is one.
- **Syncing…** Something is being sent or received: a change or a file.
  A pass that finds nothing new leaves the state as it was.
- **Waiting to sync.** This device is online and has changes to send, but
  they are held: uploads are paused, or an upload waits to retry. The Sync
  page says why. Offline with changes to send is "Offline · unsent
  changes".
- **Synced.** Everything this device has, its changes and its files, is in
  storage. A file still uploading is "Syncing…", not "Synced".
- There is no "Checking sync…" and no "Sync stopped": the state is known as
  soon as the library opens, and a sync that couldn't start is "Sync
  failed", with why and Reconnect.
- **Last synced.** When this device's copy last matched storage. Shown only
  on the Sync page, on its own line under the status.

## People and devices

- **Devices.** The phones and Macs that have a copy of the library. "This
  device" is the one in your hand.
- **Removed from the library.** An admin took this device, or its person, out
  of the library: it no longer syncs, and what is here is an old copy.
- **Everyone.** The listening-history choice that shares your plays with the
  library's people. Not "Household", not "Members".

## Removing and deleting

- **Remove.** Take a synced library off this device: this device leaves the
  library and deletes its copy. Nothing else changes.
- **Delete.** Erase a library that was never synced. It exists only here, so
  it is gone for good; audio files in your own folders stay.
