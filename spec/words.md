# The words bae uses

What each word on screen means, so every screen, platform and locale says the
same thing the same way. A new word, or a new meaning for one, goes here in
the commit that puts it on screen. [bae.md](bae.md) states what bae does; this
file states what it calls things.

## The library and where it lives

- **Library.** One catalog and its files. A synced library is kept in the
  cloud and is the same on every device that has it; a library not synced yet
  lives only on the Mac that made it. Say "the library", never "the cloud
  library" or "your local library": there is one library, wherever its copies
  are.
- **Synced / not synced.** Whether the library is kept in the cloud. A library
  starts not synced and becomes synced when sync is set up for it.
- **The cloud.** Where the library is stored: the Dropbox or OneDrive folder,
  the iCloud container, or the S3-compatible bucket it syncs through. "The
  library will remain in the cloud and on your other devices."
- **This device's copy.** What the library has on this phone or Mac: its
  catalog and the files downloaded here. Removing the library from this device
  deletes this copy; the library itself is untouched.

## Sync

- **Sync.** The page and the Library row about keeping this device's copy and
  the cloud in step. On screen it is "Sync", never "Cloud home".
- **Signed out.** This device has no working sign-in for the cloud: none is
  saved, or the one it had expired. Fixed with Sign In.
- **Storage access was denied.** This device is signed in, but that account
  isn't allowed into the cloud folder or bucket. Signing in again doesn't fix
  it; the person who manages the storage does.
- **Sync failed.** Sync stopped on an error. The Sync row says only that; the
  Sync page says what went wrong and offers the fix where there is one.
- **Last synced.** When this device's copy last matched the cloud. Shown only
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
