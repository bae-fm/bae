# The words bae uses

How bae writes, and what its own words mean, so every screen, platform and
locale says the same thing the same way. A new word, or a new meaning for one,
goes here in the commit that puts it on screen. [bae.md](bae.md) states what
bae does; this file states what it calls things.

## Capitalization

Sentence case everywhere: capitalize the first word and proper names only
("Rename library", "Sign in to Dropbox…", "Show in Finder"). This covers
buttons, titles of windows, sheets, dialogs and pages, settings rows, field
labels and notes, on every platform. The one exception is the macOS menu
bar's menus and items, which use title case as every Mac app's do ("Rename
Library…", "Open Library"). "bae" itself is always lowercase. Each
translation follows its own language's capitalization: German keeps its
capitalized nouns, and no language copies English title case.
scripts/loc-words.py refuses a capitalized word past a sentence's
start that is no name, acronym or other label named in the sentence, in the
macOS, Android and core catalogs, a translation whose every word is
capitalized, and a menu bar title not in title case.

## Punctuation

An ellipsis is one character, "…", never three full stops, in every language
("Sign in to Dropbox…", "Checking the bucket…"). bae-loc's catalog check
refuses three full stops in a core string or any of its translations, and
scripts/loc-english-skeleton.py refuses them in the macOS and Android
catalogs, source and every locale.

## The library and where it lives

- **Library.** One catalog and its files. A synced library is kept in
  the cloud and is the same on every device that has it; a library not synced yet
  lives only on the Mac that made it. Say "the library", never "the cloud
  library" or "your local library": there is one library, wherever its copies
  are.
- **Synced / not synced.** Whether the library is kept in the cloud. A library
  starts not synced and becomes synced when sync is set up for it.
- **Cloud.** Where a synced library is kept: the Dropbox or OneDrive
  folder, the iCloud container, or the S3-compatible bucket it syncs through.
  "The cloud" in everyday words, "cloud home" where sync is set up (the Sync
  settings section "Cloud home", setup's steps), never "storage" alone for
  either. "The library will remain in the cloud and on your other devices." A
  release whose files are all in the cloud is "In cloud" beside "Local" and
  "Pinned", and moving a local release there is "Move to cloud". "Storage"
  stays for the Storage Manager, a release's Storage sheet and a device's own
  storage.
- **This device's copy.** What the library has on this phone or Mac: its
  catalog and the files downloaded here. Removing the library from this device
  deletes this copy; the library itself is untouched.

## Sync

- **Sync.** The page and the Library row about keeping this device's copy and
  the cloud in step. On screen it is "Sync"; the place it keeps in step is the
  "cloud home" in its setup and its settings section. The states it
  shows are coven's sync state
  ([coven §19.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#191-what-the-app-sees)).

## People and devices

- **Devices.** The phones and Macs that have a copy of the library. "This
  device" is the one in your hand.
- **Restore code.** The person's own code that adds a new device of theirs
  to the library, or gets the library back when every device is lost. Always
  "restore code", in the apps and in bae's spec, as coven names it
  ([coven §13.1](https://github.com/bae-fm/coven/blob/main/spec/coven.md#131-a-persons-new-device-the-restore-code)).
  An invite is another person's code, never a restore code.
- **Everyone.** The listening-history choice that shares your plays with the
  library's people. Not "Household", not "Members".

## Removing and deleting

- **Remove.** Take a synced library off this device: this device leaves the
  library and deletes its copy. Nothing else changes.
- **Delete.** Erase a library that was never synced. It exists only here, so
  it is gone for good; audio files in your own folders stay.
